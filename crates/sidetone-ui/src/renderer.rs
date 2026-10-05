//! Renders Dear ImGui draw data with fixed-function OpenGL inside an XPLM window callback.

use crate::gl;
use dear_imgui_rs::render::{DrawCmd, DrawVert, PendingFrame, SnapshotTextureId, SynchronousRendererConsumer, TextureOp};
use dear_imgui_rs::texture::TextureFormat;
use dear_imgui_rs::{BackendFlags, Context, TextureId};
use sidetone_core::geometry::project;
use sidetone_xplm::graphics;
use std::collections::HashMap;
use std::os::raw::c_void;

/// Where the ImGui display sits on X-Plane's GL surface for one draw call.
#[derive(Clone, Copy, Debug)]
pub struct Placement {
    /// Window top-left in the coordinate space X-Plane set up for this draw callback (boxels).
    pub left: f32,
    pub top: f32,
    /// ImGui units per boxel (2.0 on Retina) so text is rasterised at native resolution.
    pub scale: f32,
}

struct GlTexture {
    gl_id: i32,
    format: TextureFormat,
}

enum CachedCmd {
    Elements { count: usize, clip: [f32; 4], texture: i32, vtx_offset: usize, idx_offset: usize },
    Sampler(i32),
}

struct CachedList {
    vertices: Vec<DrawVert>,
    indices: Vec<u16>,
    commands: Vec<CachedCmd>,
}

/// Geometry of the last built frame, redrawn on frames where the UI isn't rebuilt.
#[derive(Default)]
struct Cache {
    lists: Vec<CachedList>,
}

pub struct Renderer {
    consumer: SynchronousRendererConsumer,
    textures: HashMap<SnapshotTextureId, GlTexture>,
    cache: Cache,
}

impl Renderer {
    pub fn new(context: &mut Context) -> Renderer {
        let _ = context.set_renderer_name(Some("sidetone-xplm-gl2"));
        let mut flags = context.io().backend_flags();
        flags.insert(BackendFlags::RENDERER_HAS_TEXTURES);
        flags.insert(BackendFlags::RENDERER_HAS_VTX_OFFSET);
        context.io_mut().set_backend_flags(flags);
        let consumer = context.create_synchronous_renderer_consumer().expect("a fresh ImGui context accepts one renderer");
        Renderer { consumer, textures: HashMap::new(), cache: Cache::default() }
    }

    pub fn consumer(&self) -> &SynchronousRendererConsumer {
        &self.consumer
    }

    /// Uploads texture changes and draws the frame. Must run inside an XPLM draw callback.
    pub fn render(&mut self, frame: PendingFrame<'_>, placement: Placement) {
        let mut feedback = Vec::with_capacity(frame.texture_requests().len());
        for request in frame.texture_requests() {
            let key = request.texture();
            match request.operation() {
                TextureOp::Create { format, width, height, row_pitch, pixels } => {
                    if let Some(old) = self.textures.remove(&key) {
                        delete_texture(old.gl_id);
                    }
                    let gl_id = graphics::generate_texture_number();
                    graphics::bind_texture(gl_id);
                    unsafe {
                        gl::glTexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MIN_FILTER, gl::LINEAR);
                        gl::glTexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MAG_FILTER, gl::LINEAR);
                        gl::glTexParameteri(gl::TEXTURE_2D, gl::TEXTURE_WRAP_S, gl::CLAMP_TO_EDGE);
                        gl::glTexParameteri(gl::TEXTURE_2D, gl::TEXTURE_WRAP_T, gl::CLAMP_TO_EDGE);
                        upload(*format, 0, 0, *width as i32, *height as i32, *row_pitch, pixels, true);
                    }
                    self.textures.insert(key, GlTexture { gl_id, format: *format });
                    match request.uploaded(TextureId::new(gl_id as u64)) {
                        Ok(f) => feedback.push(f),
                        Err(e) => log::error!("ImGui texture create feedback rejected: {e}"),
                    }
                }
                TextureOp::Update { rects, .. } => {
                    let Some(texture) = self.textures.get(&key) else {
                        feedback.push(request.retry());
                        continue;
                    };
                    graphics::bind_texture(texture.gl_id);
                    for rect in rects {
                        let r = rect.rect;
                        unsafe { upload(texture.format, r.x as i32, r.y as i32, r.w as i32, r.h as i32, rect.row_pitch, &rect.data, false) };
                    }
                    match request.uploaded(TextureId::new(texture.gl_id as u64)) {
                        Ok(f) => feedback.push(f),
                        Err(e) => log::error!("ImGui texture update feedback rejected: {e}"),
                    }
                }
                TextureOp::Destroy => {
                    if let Some(old) = self.textures.remove(&key) {
                        delete_texture(old.gl_id);
                    }
                    match request.destroyed() {
                        Ok(f) => feedback.push(f),
                        Err(e) => log::error!("ImGui texture destroy feedback rejected: {e}"),
                    }
                }
            }
        }

        let frame = match frame.reconcile_texture_feedback(feedback) {
            Ok(frame) => frame,
            Err(e) => {
                log::error!("ImGui texture reconciliation failed: {e}");
                return;
            }
        };
        self.cache.lists.clear();
        let draw_data = frame.draw_data();
        let [dx, dy] = draw_data.display_pos();
        for list in draw_data.draw_lists() {
            let mut cached = CachedList { vertices: list.vtx_buffer().to_vec(), indices: list.idx_buffer().to_vec(), commands: Vec::new() };
            for command in list.commands() {
                cached.commands.push(match command {
                    DrawCmd::Elements { count, cmd_params } => {
                        let [x1, y1, x2, y2] = cmd_params.clip_rect;
                        CachedCmd::Elements {
                            count,
                            clip: [x1 - dx, y1 - dy, x2 - dx, y2 - dy],
                            texture: cmd_params.texture_id.id() as i32,
                            vtx_offset: cmd_params.vtx_offset,
                            idx_offset: cmd_params.idx_offset,
                        }
                    }
                    DrawCmd::SetSamplerLinear => CachedCmd::Sampler(gl::LINEAR),
                    DrawCmd::SetSamplerNearest => CachedCmd::Sampler(gl::NEAREST),
                    DrawCmd::ResetRenderState | DrawCmd::RawCallback(_) => continue,
                });
            }
            self.cache.lists.push(cached);
        }
        self.draw_cached(placement);
    }

    /// Draws the last built frame again without rebuilding the UI. Cheap: one pass of
    /// client-array draws over a few KB of vertices.
    pub fn draw_cached(&self, placement: Placement) {
        if self.cache.lists.is_empty() {
            return;
        }
        unsafe {
            // Capture X-Plane's transform before ours, to map clip rects to window pixels for glScissor.
            let mut modelview = [0f32; 16];
            let mut projection = [0f32; 16];
            let mut viewport = [0i32; 4];
            gl::glGetFloatv(gl::MODELVIEW_MATRIX, modelview.as_mut_ptr());
            gl::glGetFloatv(gl::PROJECTION_MATRIX, projection.as_mut_ptr());
            gl::glGetIntegerv(gl::VIEWPORT, viewport.as_mut_ptr());
            let to_pixels = |x: f32, y: f32| -> (f32, f32) {
                let bx = placement.left + x / placement.scale;
                let by = placement.top - y / placement.scale;
                project(&modelview, &projection, &viewport, bx, by)
            };

            gl::glPushAttrib(gl::ENABLE_BIT | gl::COLOR_BUFFER_BIT | gl::TRANSFORM_BIT | gl::SCISSOR_BIT | gl::TEXTURE_BIT);
            gl::glPushClientAttrib(gl::CLIENT_VERTEX_ARRAY_BIT);
            graphics::set_state_2d_textured();
            gl::glBlendFunc(gl::SRC_ALPHA, gl::ONE_MINUS_SRC_ALPHA);
            gl::glDisable(gl::CULL_FACE);
            gl::glDisable(gl::DEPTH_TEST);
            gl::glEnable(gl::SCISSOR_TEST);
            gl::glEnableClientState(gl::VERTEX_ARRAY);
            gl::glEnableClientState(gl::TEXTURE_COORD_ARRAY);
            gl::glEnableClientState(gl::COLOR_ARRAY);

            gl::glMatrixMode(gl::MODELVIEW);
            gl::glPushMatrix();
            gl::glTranslatef(placement.left, placement.top, 0.0);
            gl::glScalef(1.0 / placement.scale, -1.0 / placement.scale, 1.0);

            let stride = std::mem::size_of::<DrawVert>() as i32;
            for list in &self.cache.lists {
                for command in &list.commands {
                    match *command {
                        CachedCmd::Elements { count, clip: [cx1, cy1, cx2, cy2], texture, vtx_offset, idx_offset } => {
                            if cx2 <= cx1 || cy2 <= cy1 {
                                continue;
                            }
                            let (px1, py1) = to_pixels(cx1, cy2);
                            let (px2, py2) = to_pixels(cx2, cy1);
                            let (sx, sy) = (px1.min(px2).floor(), py1.min(py2).floor());
                            let (sw, sh) = ((px1 - px2).abs().ceil(), (py1 - py2).abs().ceil());
                            gl::glScissor(sx as i32, sy as i32, sw as i32, sh as i32);

                            graphics::bind_texture(texture);
                            let base = list.vertices.as_ptr().add(vtx_offset) as *const u8;
                            gl::glVertexPointer(2, gl::FLOAT, stride, base as *const c_void);
                            gl::glTexCoordPointer(2, gl::FLOAT, stride, base.add(8) as *const c_void);
                            gl::glColorPointer(4, gl::UNSIGNED_BYTE, stride, base.add(16) as *const c_void);
                            gl::glDrawElements(gl::TRIANGLES, count as i32, gl::UNSIGNED_SHORT, list.indices.as_ptr().add(idx_offset) as *const c_void);
                        }
                        CachedCmd::Sampler(filter) => {
                            gl::glTexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MIN_FILTER, filter);
                            gl::glTexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MAG_FILTER, filter);
                        }
                    }
                }
            }

            gl::glPopMatrix();
            gl::glPopClientAttrib();
            gl::glPopAttrib();
        }
    }

    /// Releases GL textures and detaches from the context. Main thread, context active.
    pub fn shutdown(&mut self, context: &mut Context) {
        if let Ok(reset) = context.prepare_renderer_texture_reset(&self.consumer) {
            for (_, texture) in self.textures.drain() {
                delete_texture(texture.gl_id);
            }
            reset.commit();
        }
    }
}

fn delete_texture(id: i32) {
    let id = id as u32;
    unsafe { gl::glDeleteTextures(1, &id) };
}

#[allow(clippy::too_many_arguments)]
unsafe fn upload(format: TextureFormat, x: i32, y: i32, w: i32, h: i32, row_pitch: usize, pixels: &[u8], create: bool) {
    let (gl_format, bpp) = match format {
        TextureFormat::RGBA32 => (gl::RGBA, 4),
        TextureFormat::Alpha8 => (gl::ALPHA, 1),
    };
    unsafe {
        gl::glPushClientAttrib(gl::CLIENT_PIXEL_STORE_BIT);
        gl::glPixelStorei(gl::UNPACK_ALIGNMENT, 1);
        gl::glPixelStorei(gl::UNPACK_ROW_LENGTH, (row_pitch / bpp) as i32);
        let data = pixels.as_ptr() as *const c_void;
        if create {
            gl::glTexImage2D(gl::TEXTURE_2D, 0, gl_format as i32, w, h, 0, gl_format, gl::UNSIGNED_BYTE, data);
        } else {
            gl::glTexSubImage2D(gl::TEXTURE_2D, 0, x, y, w, h, gl_format, gl::UNSIGNED_BYTE, data);
        }
        gl::glPopClientAttrib();
    }
}
