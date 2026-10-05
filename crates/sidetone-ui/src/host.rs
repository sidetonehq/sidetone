//! Hosts one Dear ImGui context inside one XPLM window.

use crate::renderer::{Placement, Renderer};
use crate::theme;
use dear_imgui_rs::{Condition, Context, Key, MouseButton, Style, SuspendedContext, Ui, WindowFlags};
use sidetone_xplm::graphics;
use sidetone_xplm::window::{Cursor, Decoration, KeyEvent, Layer, MouseStatus, Positioning, Rect, Window, WindowHandler, WindowOptions, WindowRef};
use sidetone_xplm::{elapsed_time, sys};
use std::cell::RefCell;
use std::rc::Rc;

#[derive(Clone, Debug)]
pub enum WindowKind {
    /// Undecorated overlay; dragged by its background.
    Panel,
    /// X-Plane-decorated floating window (title bar, resize) that can pop out.
    Floating { title: String },
}

/// Requests the UI code makes of its host window during a frame.
#[derive(Default)]
struct Requests {
    close: bool,
    pop_out: Option<bool>,
}

/// What a UI build callback receives each frame.
pub struct HostFrame<'a> {
    pub ui: &'a Ui,
    /// The mouse is over this window.
    pub hovered: bool,
    pub popped_out: bool,
    /// Seconds since the sim started.
    pub now: f32,
    pub fonts: theme::Fonts,
    requests: &'a mut Requests,
}

impl HostFrame<'_> {
    pub fn close(&mut self) {
        self.requests.close = true;
    }

    pub fn set_popped_out(&mut self, popped_out: bool) {
        self.requests.pop_out = Some(popped_out);
    }
}

type BuildFn = Box<dyn FnMut(&mut HostFrame)>;

enum Input {
    MousePos(f32, f32),
    Button(bool),
    Wheel(f32, f32),
    Key(Key, bool),
    Char(char),
}

struct Shared {
    /// Set when the user finished dragging/resizing, so the owner can persist the position.
    moved: bool,
    /// User UI scale (Settings), multiplied into the Retina scale.
    user_scale: f32,
    /// Smoothed CPU time this window costs per sim frame, in milliseconds.
    cpu_ms: f32,
}

/// Rebuild the UI at most this often when nobody is interacting with it; in between, the
/// cached geometry is redrawn. Interaction (hover, typing, clicks) rebuilds every frame.
const IDLE_REBUILD_SECONDS: f32 = 1.0 / 15.0;

/// If we hold the keyboard while the mouse is elsewhere and nothing is typed for this long,
/// give the keyboard back to X-Plane (safety net if X-Plane never reports the focus change).
const FOCUS_IDLE_SECONDS: f32 = 5.0;

/// System clipboard for ImGui text fields (Cmd+C/V/X).
struct SystemClipboard;

impl dear_imgui_rs::ClipboardBackend for SystemClipboard {
    fn get(&mut self) -> Option<String> {
        arboard::Clipboard::new().and_then(|mut c| c.get_text()).ok()
    }

    fn set(&mut self, value: &str) {
        if let Err(e) = arboard::Clipboard::new().and_then(|mut c| c.set_text(value.to_string())) {
            log::warn!("Clipboard write failed: {e}");
        }
    }
}

struct Host {
    context: Option<SuspendedContext>,
    renderer: Option<Renderer>,
    pristine_style: Style,
    fonts: theme::Fonts,
    build: BuildFn,
    kind: WindowKind,
    inputs: Vec<Input>,
    scale: f32,
    last_time: f32,
    hover_frame: u64,
    frame: u64,
    any_item_hovered: bool,
    drag_from: Option<(i32, i32, Rect)>,
    shared: Rc<RefCell<Shared>>,
    user_scale: f32,
    last_build: f32,
    last_size: (i32, i32),
    wants_text: bool,
    last_key_time: f32,
}

/// An X-Plane window rendering Dear ImGui.
pub struct ImguiWindow {
    window: Window,
    shared: Rc<RefCell<Shared>>,
}

impl ImguiWindow {
    pub fn new(kind: WindowKind, rect: Rect, visible: bool, build: impl FnMut(&mut HostFrame) + 'static) -> ImguiWindow {
        let mut context = Context::create();
        let _ = context.set_ini_filename(None::<String>);
        context.set_clipboard_backend(SystemClipboard);
        let _ = context.set_platform_name(Some("sidetone-xplm"));
        let fonts = theme::load_fonts(&context);
        let pristine_style = context.style().clone();
        let renderer = Renderer::new(&mut context);
        let context = context.suspend().unwrap_or_else(|e| panic!("could not suspend ImGui context: {e}"));

        let shared = Rc::new(RefCell::new(Shared { moved: false, user_scale: 1.0, cpu_ms: 0.0 }));
        let host = Host {
            context: Some(context),
            renderer: Some(renderer),
            pristine_style,
            fonts,
            build: Box::new(build),
            kind: kind.clone(),
            inputs: Vec::new(),
            scale: 0.0,
            last_time: elapsed_time(),
            hover_frame: 0,
            frame: 0,
            any_item_hovered: false,
            drag_from: None,
            shared: shared.clone(),
            user_scale: 1.0,
            last_build: -1.0,
            last_size: (0, 0),
            wants_text: false,
            last_key_time: 0.0,
        };
        let (decoration, layer) = match &kind {
            WindowKind::Panel => (Decoration::None, Layer::FloatingWindows),
            WindowKind::Floating { .. } => (Decoration::RoundRectangle, Layer::FloatingWindows),
        };
        let window = Window::new(WindowOptions { rect, visible, decoration, layer }, host);
        if let WindowKind::Floating { title } = &kind {
            window.handle().set_title(title);
            window.handle().set_resizing_limits(600, 360, 4000, 4000);
        }
        ImguiWindow { window, shared }
    }

    pub fn handle(&self) -> WindowRef {
        self.window.handle()
    }

    pub fn is_visible(&self) -> bool {
        self.handle().is_visible()
    }

    pub fn set_visible(&self, visible: bool) {
        let w = self.handle();
        w.set_visible(visible);
        if visible {
            w.bring_to_front();
        } else {
            w.release_keyboard_focus();
        }
    }

    /// Sets the user UI scale (1.0 = default).
    pub fn set_user_scale(&self, scale: f32) {
        self.shared.borrow_mut().user_scale = scale.clamp(0.75, 2.0);
    }

    /// Smoothed CPU cost of this window per sim frame, in milliseconds.
    pub fn cpu_ms(&self) -> f32 {
        self.shared.borrow().cpu_ms
    }

    /// Returns true once after the user moved or resized the window.
    pub fn take_moved(&self) -> bool {
        std::mem::take(&mut self.shared.borrow_mut().moved)
    }
}

impl Drop for Host {
    fn drop(&mut self) {
        if let (Some(mut context), Some(mut renderer)) = (self.context.take(), self.renderer.take()) {
            context.with_active_or_panic(|ctx| renderer.shutdown(ctx));
            drop(renderer);
        }
    }
}

impl Host {
    fn draw_inner(&mut self, window: WindowRef) {
        self.frame += 1;
        let geometry = window.geometry();
        if geometry.width() <= 0 || geometry.height() <= 0 {
            return;
        }
        let mut viewport = [0i32; 4];
        unsafe { crate::gl::glGetIntegerv(crate::gl::VIEWPORT, viewport.as_mut_ptr()) };
        let scale = self.current_scale(window, viewport[2]);
        let user_scale = self.shared.borrow().user_scale;

        let now = elapsed_time();
        let hovered = self.hover_frame + 1 >= self.frame || self.drag_from.is_some();
        let popped_out = window.is_popped_out();
        let has_focus = window.has_keyboard_focus();

        // Safety net: never sit on the keyboard while the pilot is clearly elsewhere.
        if has_focus && !hovered && now - self.last_key_time > FOCUS_IDLE_SECONDS {
            self.inputs.push(Input::Key(Key::Escape, true));
            self.inputs.push(Input::Key(Key::Escape, false));
            window.release_keyboard_focus();
        }

        let size = (geometry.width(), geometry.height());
        let rescale = (scale - self.scale).abs() > f32::EPSILON || (user_scale - self.user_scale).abs() > f32::EPSILON;
        let must_build = rescale
            || size != self.last_size
            || !self.inputs.is_empty()
            || hovered
            || self.wants_text
            || now - self.last_build >= IDLE_REBUILD_SECONDS
            || now < self.last_build;
        if !must_build {
            if let Some(renderer) = self.renderer.as_ref() {
                renderer.draw_cached(Placement { left: geometry.left as f32, top: geometry.top as f32, scale });
            }
            return;
        }
        let dt = (now - self.last_time).clamp(1.0 / 240.0, 0.25);
        self.last_time = now;
        self.last_build = now;
        self.last_size = size;
        self.build_frame(window, geometry, scale, user_scale, rescale, dt, now, hovered, popped_out, viewport);
    }

    #[allow(clippy::too_many_arguments)]
    fn build_frame(
        &mut self,
        window: WindowRef,
        geometry: Rect,
        scale: f32,
        user_scale: f32,
        rescale: bool,
        dt: f32,
        now: f32,
        hovered: bool,
        popped_out: bool,
        viewport: [i32; 4],
    ) {
        let Some(context) = self.context.as_mut() else { return };
        let Some(renderer) = self.renderer.as_mut() else { return };
        let inputs = std::mem::take(&mut self.inputs);
        let pristine = &self.pristine_style;
        let build = &mut self.build;
        let fonts = self.fonts;
        let is_panel = matches!(self.kind, WindowKind::Panel);
        let mut requests = Requests::default();
        let mut any_item_hovered = false;
        let mut wants_text = false;

        context.with_active_or_panic(|ctx| {
            if rescale {
                theme::apply(ctx, pristine, scale * user_scale);
            }
            let io = ctx.io_mut();
            io.set_display_size([geometry.width() as f32 * scale, geometry.height() as f32 * scale]);
            io.set_display_framebuffer_scale([1.0, 1.0]);
            io.set_delta_time(dt);
            if !hovered {
                io.add_mouse_pos_event([-f32::MAX, -f32::MAX]);
            }
            for input in inputs {
                match input {
                    Input::MousePos(x, y) => io.add_mouse_pos_event([x, y]),
                    Input::Button(down) => io.add_mouse_button_event(MouseButton::Left, down),
                    Input::Wheel(x, y) => io.add_mouse_wheel_event([x, y]),
                    Input::Key(key, down) => io.add_key_event(key, down),
                    Input::Char(c) => io.add_input_character(c),
                }
            }

            let frame = ctx.begin_frame();
            {
                let ui = frame.ui();
                let [w, h] = [geometry.width() as f32 * scale, geometry.height() as f32 * scale];
                let mut flags =
                    WindowFlags::NO_TITLE_BAR | WindowFlags::NO_RESIZE | WindowFlags::NO_MOVE | WindowFlags::NO_COLLAPSE | WindowFlags::NO_SAVED_SETTINGS;
                if is_panel {
                    flags |= WindowFlags::NO_SCROLLBAR | WindowFlags::NO_SCROLL_WITH_MOUSE | WindowFlags::NO_BACKGROUND;
                }
                ui.window("##root").position([0.0, 0.0], Condition::Always).size([w, h], Condition::Always).flags(flags).build(|| {
                    let mut host_frame = HostFrame { ui, hovered, popped_out, now, fonts, requests: &mut requests };
                    // Contain UI panics here so the ImGui frame still ends cleanly.
                    sidetone_xplm::guard("ui build", (), || build(&mut host_frame));
                    any_item_hovered = ui.is_any_item_hovered() || ui.is_any_item_active();
                });
                wants_text = ui.io().want_text_input();
            }
            let pending = frame.render(renderer.consumer());
            renderer.render(pending, Placement { left: geometry.left as f32, top: geometry.top as f32, scale });
        });
        if rescale {
            log::info!(
                "{:?}: drawing at {}x{} boxels, scale {scale}, viewport {}x{}, popped out {popped_out}",
                self.kind,
                geometry.width(),
                geometry.height(),
                viewport[2],
                viewport[3]
            );
        }
        self.scale = scale;
        self.user_scale = user_scale;
        self.any_item_hovered = any_item_hovered;
        self.wants_text = wants_text;

        if wants_text && !window.has_keyboard_focus() {
            window.take_keyboard_focus();
            self.last_key_time = now;
        } else if !wants_text && window.has_keyboard_focus() {
            window.release_keyboard_focus();
        }
        if let Some(pop) = requests.pop_out {
            window.set_positioning(if pop { Positioning::PopOut } else { Positioning::Free });
        }
        if requests.close {
            window.release_keyboard_focus();
            window.set_visible(false);
        }
    }

    /// ImGui units per boxel: the GL viewport's pixel density, so text is crisp on Retina.
    fn current_scale(&self, window: WindowRef, viewport_width: i32) -> f32 {
        let boxels = if window.is_popped_out() { window.geometry().width() } else { graphics::screen_size().0 };
        if boxels <= 0 || viewport_width <= 0 {
            return 1.0;
        }
        let raw = viewport_width as f32 / boxels as f32;
        ((raw * 4.0).round() / 4.0).clamp(1.0, 3.0)
    }

    fn to_local(&self, window: WindowRef, x: i32, y: i32) -> (f32, f32) {
        let g = window.geometry();
        ((x - g.left) as f32 * self.scale.max(1.0), (g.top - y) as f32 * self.scale.max(1.0))
    }
}

impl WindowHandler for Host {
    fn draw(&mut self, window: WindowRef) {
        let started = std::time::Instant::now();
        self.draw_inner(window);
        let ms = started.elapsed().as_secs_f32() * 1000.0;
        let mut shared = self.shared.borrow_mut();
        shared.cpu_ms = shared.cpu_ms * 0.95 + ms * 0.05;
    }

    fn mouse_click(&mut self, window: WindowRef, x: i32, y: i32, status: MouseStatus) -> bool {
        let (lx, ly) = self.to_local(window, x, y);
        self.hover_frame = self.frame;
        let is_panel = matches!(self.kind, WindowKind::Panel);
        match status {
            MouseStatus::Down => {
                if is_panel && !self.any_item_hovered {
                    self.drag_from = Some((x, y, window.geometry()));
                } else {
                    self.inputs.push(Input::MousePos(lx, ly));
                    self.inputs.push(Input::Button(true));
                }
            }
            MouseStatus::Drag => {
                if let Some((sx, sy, start)) = self.drag_from {
                    let (dx, dy) = (x - sx, y - sy);
                    window.set_geometry(Rect { left: start.left + dx, top: start.top + dy, right: start.right + dx, bottom: start.bottom + dy });
                } else {
                    self.inputs.push(Input::MousePos(lx, ly));
                }
            }
            MouseStatus::Up => {
                if let Some((sx, sy, _)) = self.drag_from.take() {
                    if (x - sx).abs() + (y - sy).abs() <= 3 {
                        // A click on the background rather than a drag: forward it to ImGui.
                        self.inputs.push(Input::MousePos(lx, ly));
                        self.inputs.push(Input::Button(true));
                        self.inputs.push(Input::Button(false));
                    } else {
                        self.shared.borrow_mut().moved = true;
                    }
                } else {
                    self.inputs.push(Input::MousePos(lx, ly));
                    self.inputs.push(Input::Button(false));
                    if !is_panel {
                        self.shared.borrow_mut().moved = true;
                    }
                }
            }
        }
        true
    }

    fn cursor(&mut self, window: WindowRef, x: i32, y: i32) -> Cursor {
        self.hover_frame = self.frame;
        let (lx, ly) = self.to_local(window, x, y);
        self.inputs.push(Input::MousePos(lx, ly));
        Cursor::Arrow
    }

    fn wheel(&mut self, window: WindowRef, x: i32, y: i32, axis: i32, clicks: i32) -> bool {
        let (lx, ly) = self.to_local(window, x, y);
        self.inputs.push(Input::MousePos(lx, ly));
        let amount = clicks as f32;
        self.inputs.push(if axis == 0 { Input::Wheel(0.0, amount) } else { Input::Wheel(-amount, 0.0) });
        true
    }

    fn key(&mut self, _window: WindowRef, event: KeyEvent) {
        self.last_key_time = elapsed_time();
        if !event.down {
            return; // We synthesise press+release per down event; X-Plane repeats downs while held.
        }
        // X-Plane reports Cmd as its "control" flag on macOS. ImGui runs with Mac behaviours,
        // where Super is the shortcut modifier (Cmd+V paste, Cmd+A select all) and Alt moves by word.
        let modifiers: Vec<Key> = [(event.control, Key::ModSuper), (event.alt, Key::ModAlt), (event.shift, Key::ModShift)]
            .into_iter()
            .filter_map(|(on, key)| on.then_some(key))
            .collect();
        if event.control || event.alt {
            log::debug!("Shortcut key: vk={:#04x} cmd={} alt={} shift={}", event.virtual_key, event.control, event.alt, event.shift);
        }
        if let Some(key) = map_key(event.virtual_key) {
            for m in &modifiers {
                self.inputs.push(Input::Key(*m, true));
            }
            self.inputs.push(Input::Key(key, true));
            self.inputs.push(Input::Key(key, false));
            for m in modifiers.iter().rev() {
                self.inputs.push(Input::Key(*m, false));
            }
        }
        // Text characters only when no shortcut modifier is held (Cmd+V must not type "v").
        if !event.control
            && let Some(c) = event.character
        {
            self.inputs.push(Input::Char(c));
        }
    }

    fn focus_lost(&mut self, _window: WindowRef) {
        self.inputs.push(Input::Key(Key::Escape, true));
        self.inputs.push(Input::Key(Key::Escape, false));
    }
}

fn map_key(vk: u8) -> Option<Key> {
    const LETTERS: [Key; 26] = [
        Key::A,
        Key::B,
        Key::C,
        Key::D,
        Key::E,
        Key::F,
        Key::G,
        Key::H,
        Key::I,
        Key::J,
        Key::K,
        Key::L,
        Key::M,
        Key::N,
        Key::O,
        Key::P,
        Key::Q,
        Key::R,
        Key::S,
        Key::T,
        Key::U,
        Key::V,
        Key::W,
        Key::X,
        Key::Y,
        Key::Z,
    ];
    let vk = vk as u32;
    if (sys::XPLM_VK_A..=sys::XPLM_VK_Z).contains(&vk) {
        return Some(LETTERS[(vk - sys::XPLM_VK_A) as usize]);
    }
    Some(match vk {
        sys::XPLM_VK_BACK => Key::Backspace,
        sys::XPLM_VK_TAB => Key::Tab,
        sys::XPLM_VK_RETURN | sys::XPLM_VK_ENTER | sys::XPLM_VK_NUMPAD_ENT => Key::Enter,
        sys::XPLM_VK_ESCAPE => Key::Escape,
        sys::XPLM_VK_LEFT => Key::LeftArrow,
        sys::XPLM_VK_RIGHT => Key::RightArrow,
        sys::XPLM_VK_UP => Key::UpArrow,
        sys::XPLM_VK_DOWN => Key::DownArrow,
        sys::XPLM_VK_DELETE => Key::Delete,
        sys::XPLM_VK_HOME => Key::Home,
        sys::XPLM_VK_END => Key::End,
        sys::XPLM_VK_PRIOR => Key::PageUp,
        sys::XPLM_VK_NEXT => Key::PageDown,
        _ => return None,
    })
}
