use crate::sys;

/// Sets X-Plane's tracked GL state for 2D, textured, alpha-blended drawing.
pub fn set_state_2d_textured() {
    unsafe { sys::XPLMSetGraphicsState(0, 1, 0, 0, 1, 0, 0) }
}

pub fn bind_texture(texture: i32) {
    unsafe { sys::XPLMBindTexture2d(texture, 0) }
}

pub fn generate_texture_number() -> i32 {
    let mut id = 0;
    unsafe { sys::XPLMGenerateTextureNumbers(&mut id, 1) };
    id
}

/// Bounds of the whole X-Plane desktop in global boxel coordinates: (left, top, right, bottom).
pub fn screen_bounds() -> (i32, i32, i32, i32) {
    let (mut l, mut t, mut r, mut b) = (0, 0, 0, 0);
    unsafe { sys::XPLMGetScreenBoundsGlobal(&mut l, &mut t, &mut r, &mut b) };
    (l, t, r, b)
}

/// Mouse position in global boxel coordinates.
pub fn mouse_location() -> (i32, i32) {
    let (mut x, mut y) = (0, 0);
    unsafe { sys::XPLMGetMouseLocationGlobal(&mut x, &mut y) };
    (x, y)
}

/// Size of X-Plane's main window in boxels.
pub fn screen_size() -> (i32, i32) {
    let (mut w, mut h) = (0, 0);
    unsafe { sys::XPLMGetScreenSize(&mut w, &mut h) };
    (w, h)
}
