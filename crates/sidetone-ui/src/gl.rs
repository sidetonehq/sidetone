//! The handful of legacy OpenGL entry points the renderer needs, linked from OpenGL.framework.
#![allow(non_snake_case, dead_code)]

use std::os::raw::{c_float, c_int, c_uint, c_void};

pub type GLenum = c_uint;

pub const TEXTURE_2D: GLenum = 0x0DE1;
pub const SCISSOR_TEST: GLenum = 0x0C11;
pub const CULL_FACE: GLenum = 0x0B44;
pub const DEPTH_TEST: GLenum = 0x0B71;
pub const BLEND: GLenum = 0x0BE2;
pub const SRC_ALPHA: GLenum = 0x0302;
pub const ONE_MINUS_SRC_ALPHA: GLenum = 0x0303;
pub const VERTEX_ARRAY: GLenum = 0x8074;
pub const COLOR_ARRAY: GLenum = 0x8076;
pub const TEXTURE_COORD_ARRAY: GLenum = 0x8078;
pub const FLOAT: GLenum = 0x1406;
pub const UNSIGNED_BYTE: GLenum = 0x1401;
pub const UNSIGNED_SHORT: GLenum = 0x1403;
pub const TRIANGLES: GLenum = 0x0004;
pub const MODELVIEW: GLenum = 0x1700;
pub const MODELVIEW_MATRIX: GLenum = 0x0BA6;
pub const PROJECTION_MATRIX: GLenum = 0x0BA7;
pub const VIEWPORT: GLenum = 0x0BA2;
pub const RGBA: GLenum = 0x1908;
pub const ALPHA: GLenum = 0x1906;
pub const TEXTURE_MIN_FILTER: GLenum = 0x2801;
pub const TEXTURE_MAG_FILTER: GLenum = 0x2800;
pub const TEXTURE_WRAP_S: GLenum = 0x2802;
pub const TEXTURE_WRAP_T: GLenum = 0x2803;
pub const CLAMP_TO_EDGE: c_int = 0x812F;
pub const LINEAR: c_int = 0x2601;
pub const NEAREST: c_int = 0x2600;
pub const UNPACK_ALIGNMENT: GLenum = 0x0CF5;
pub const UNPACK_ROW_LENGTH: GLenum = 0x0CF2;
pub const ENABLE_BIT: c_uint = 0x0000_2000;
pub const COLOR_BUFFER_BIT: c_uint = 0x0000_4000;
pub const TRANSFORM_BIT: c_uint = 0x0000_1000;
pub const SCISSOR_BIT: c_uint = 0x0008_0000;
pub const TEXTURE_BIT: c_uint = 0x0004_0000;
pub const CLIENT_VERTEX_ARRAY_BIT: c_uint = 0x0000_0002;
pub const CLIENT_PIXEL_STORE_BIT: c_uint = 0x0000_0001;

#[link(name = "OpenGL", kind = "framework")]
unsafe extern "C" {
    pub fn glPushAttrib(mask: c_uint);
    pub fn glPopAttrib();
    pub fn glPushClientAttrib(mask: c_uint);
    pub fn glPopClientAttrib();
    pub fn glEnable(cap: GLenum);
    pub fn glDisable(cap: GLenum);
    pub fn glBlendFunc(s: GLenum, d: GLenum);
    pub fn glEnableClientState(array: GLenum);
    pub fn glDisableClientState(array: GLenum);
    pub fn glVertexPointer(size: c_int, ty: GLenum, stride: c_int, ptr: *const c_void);
    pub fn glTexCoordPointer(size: c_int, ty: GLenum, stride: c_int, ptr: *const c_void);
    pub fn glColorPointer(size: c_int, ty: GLenum, stride: c_int, ptr: *const c_void);
    pub fn glDrawElements(mode: GLenum, count: c_int, ty: GLenum, indices: *const c_void);
    pub fn glScissor(x: c_int, y: c_int, w: c_int, h: c_int);
    pub fn glMatrixMode(mode: GLenum);
    pub fn glPushMatrix();
    pub fn glPopMatrix();
    pub fn glTranslatef(x: c_float, y: c_float, z: c_float);
    pub fn glScalef(x: c_float, y: c_float, z: c_float);
    pub fn glGetFloatv(name: GLenum, out: *mut c_float);
    pub fn glGetIntegerv(name: GLenum, out: *mut c_int);
    pub fn glTexImage2D(target: GLenum, level: c_int, internal: c_int, w: c_int, h: c_int, border: c_int, format: GLenum, ty: GLenum, data: *const c_void);
    pub fn glTexSubImage2D(target: GLenum, level: c_int, x: c_int, y: c_int, w: c_int, h: c_int, format: GLenum, ty: GLenum, data: *const c_void);
    pub fn glTexParameteri(target: GLenum, name: GLenum, value: c_int);
    pub fn glPixelStorei(name: GLenum, value: c_int);
    pub fn glDeleteTextures(n: c_int, textures: *const c_uint);
}
