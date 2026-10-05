use crate::{guard, sys};
use std::ffi::CString;
use std::os::raw::{c_char, c_int, c_void};

/// Global-boxel rectangle, X-Plane style: `top > bottom`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Rect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl Rect {
    pub fn width(&self) -> i32 {
        self.right - self.left
    }

    pub fn height(&self) -> i32 {
        self.top - self.bottom
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MouseStatus {
    Down,
    Drag,
    Up,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cursor {
    Default,
    Hidden,
    Arrow,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Decoration {
    None,
    RoundRectangle,
    SelfDecorated,
    SelfDecoratedResizable,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Layer {
    FlightOverlay,
    FloatingWindows,
    Modal,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Positioning {
    Free,
    CenterOnMonitor,
    PopOut,
    Vr,
}

/// A keyboard event delivered to the focused window.
#[derive(Clone, Copy, Debug)]
pub struct KeyEvent {
    /// The ASCII character, if the key produced one.
    pub character: Option<char>,
    /// XPLM virtual key code (`XPLM_VK_*`).
    pub virtual_key: u8,
    pub shift: bool,
    pub alt: bool,
    pub control: bool,
    pub down: bool,
    pub up: bool,
}

/// Behaviour of a window. All methods run on the main thread.
pub trait WindowHandler {
    fn draw(&mut self, window: WindowRef);
    fn mouse_click(&mut self, _window: WindowRef, _x: i32, _y: i32, _status: MouseStatus) -> bool {
        true
    }
    fn right_click(&mut self, _window: WindowRef, _x: i32, _y: i32, _status: MouseStatus) -> bool {
        true
    }
    fn key(&mut self, _window: WindowRef, _event: KeyEvent) {}
    fn focus_lost(&mut self, _window: WindowRef) {}
    fn wheel(&mut self, _window: WindowRef, _x: i32, _y: i32, _axis: i32, _clicks: i32) -> bool {
        true
    }
    fn cursor(&mut self, _window: WindowRef, _x: i32, _y: i32) -> Cursor {
        Cursor::Default
    }
}

/// A non-owning handle used inside callbacks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WindowRef(sys::XPLMWindowID);

impl WindowRef {
    pub fn geometry(&self) -> Rect {
        let mut r = Rect::default();
        unsafe { sys::XPLMGetWindowGeometry(self.0, &mut r.left, &mut r.top, &mut r.right, &mut r.bottom) };
        r
    }

    pub fn set_geometry(&self, r: Rect) {
        unsafe { sys::XPLMSetWindowGeometry(self.0, r.left, r.top, r.right, r.bottom) }
    }

    /// Geometry of a popped-out window in OS pixels.
    pub fn geometry_os(&self) -> Rect {
        let mut r = Rect::default();
        unsafe { sys::XPLMGetWindowGeometryOS(self.0, &mut r.left, &mut r.top, &mut r.right, &mut r.bottom) };
        r
    }

    pub fn is_visible(&self) -> bool {
        unsafe { sys::XPLMGetWindowIsVisible(self.0) != 0 }
    }

    pub fn set_visible(&self, visible: bool) {
        unsafe { sys::XPLMSetWindowIsVisible(self.0, visible as c_int) }
    }

    pub fn is_popped_out(&self) -> bool {
        unsafe { sys::XPLMWindowIsPoppedOut(self.0) != 0 }
    }

    pub fn is_in_vr(&self) -> bool {
        unsafe { sys::XPLMWindowIsInVR(self.0) != 0 }
    }

    pub fn set_positioning(&self, mode: Positioning) {
        let m = match mode {
            Positioning::Free => sys::xplm_WindowPositionFree,
            Positioning::CenterOnMonitor => sys::xplm_WindowCenterOnMonitor,
            Positioning::PopOut => sys::xplm_WindowPopOut,
            Positioning::Vr => sys::xplm_WindowVR,
        };
        unsafe { sys::XPLMSetWindowPositioningMode(self.0, m as _, -1) }
    }

    pub fn set_title(&self, title: &str) {
        if let Ok(c) = CString::new(title) {
            unsafe { sys::XPLMSetWindowTitle(self.0, c.as_ptr()) }
        }
    }

    pub fn set_resizing_limits(&self, min_w: i32, min_h: i32, max_w: i32, max_h: i32) {
        unsafe { sys::XPLMSetWindowResizingLimits(self.0, min_w, min_h, max_w, max_h) }
    }

    /// Gravity controls how the window moves when the sim window is resized (0 = follow left/bottom, 1 = follow right/top).
    pub fn set_gravity(&self, left: f32, top: f32, right: f32, bottom: f32) {
        unsafe { sys::XPLMSetWindowGravity(self.0, left, top, right, bottom) }
    }

    pub fn take_keyboard_focus(&self) {
        unsafe { sys::XPLMTakeKeyboardFocus(self.0) }
    }

    pub fn release_keyboard_focus(&self) {
        if self.has_keyboard_focus() {
            unsafe { sys::XPLMTakeKeyboardFocus(std::ptr::null_mut()) }
        }
    }

    pub fn has_keyboard_focus(&self) -> bool {
        unsafe { sys::XPLMHasKeyboardFocus(self.0) != 0 }
    }

    pub fn bring_to_front(&self) {
        unsafe { sys::XPLMBringWindowToFront(self.0) }
    }
}

pub struct WindowOptions {
    pub rect: Rect,
    pub visible: bool,
    pub decoration: Decoration,
    pub layer: Layer,
}

struct Inner {
    handler: Box<dyn WindowHandler>,
}

/// An owned XPLM modern window. Destroyed on drop.
pub struct Window {
    id: sys::XPLMWindowID,
    _inner: Box<Inner>,
}

impl Window {
    pub fn new(options: WindowOptions, handler: impl WindowHandler + 'static) -> Window {
        let mut inner = Box::new(Inner { handler: Box::new(handler) });
        let decoration = match options.decoration {
            Decoration::None => sys::xplm_WindowDecorationNone,
            Decoration::RoundRectangle => sys::xplm_WindowDecorationRoundRectangle,
            Decoration::SelfDecorated => sys::xplm_WindowDecorationSelfDecorated,
            Decoration::SelfDecoratedResizable => sys::xplm_WindowDecorationSelfDecoratedResizable,
        };
        let layer = match options.layer {
            Layer::FlightOverlay => sys::xplm_WindowLayerFlightOverlay,
            Layer::FloatingWindows => sys::xplm_WindowLayerFloatingWindows,
            Layer::Modal => sys::xplm_WindowLayerModal,
        };
        let mut params = sys::XPLMCreateWindow_t {
            structSize: std::mem::size_of::<sys::XPLMCreateWindow_t>() as c_int,
            left: options.rect.left,
            top: options.rect.top,
            right: options.rect.right,
            bottom: options.rect.bottom,
            visible: options.visible as c_int,
            drawWindowFunc: Some(draw),
            handleMouseClickFunc: Some(mouse_click),
            handleKeyFunc: Some(key),
            handleCursorFunc: Some(cursor),
            handleMouseWheelFunc: Some(wheel),
            refcon: &mut *inner as *mut Inner as *mut c_void,
            decorateAsFloatingWindow: decoration as _,
            layer: layer as _,
            handleRightClickFunc: Some(right_click),
        };
        let id = unsafe { sys::XPLMCreateWindowEx(&mut params) };
        Window { id, _inner: inner }
    }

    pub fn handle(&self) -> WindowRef {
        WindowRef(self.id)
    }
}

impl Drop for Window {
    fn drop(&mut self) {
        unsafe { sys::XPLMDestroyWindow(self.id) }
    }
}

unsafe fn inner<'a>(refcon: *mut c_void) -> &'a mut Inner {
    unsafe { &mut *(refcon as *mut Inner) }
}

fn mouse_status(status: sys::XPLMMouseStatus) -> MouseStatus {
    match status as u32 {
        sys::xplm_MouseDown => MouseStatus::Down,
        sys::xplm_MouseDrag => MouseStatus::Drag,
        _ => MouseStatus::Up,
    }
}

unsafe extern "C" fn draw(id: sys::XPLMWindowID, refcon: *mut c_void) {
    guard("window draw", (), || unsafe { inner(refcon) }.handler.draw(WindowRef(id)))
}

unsafe extern "C" fn mouse_click(id: sys::XPLMWindowID, x: c_int, y: c_int, status: sys::XPLMMouseStatus, refcon: *mut c_void) -> c_int {
    guard("window click", 1, || unsafe { inner(refcon) }.handler.mouse_click(WindowRef(id), x, y, mouse_status(status)) as c_int)
}

unsafe extern "C" fn right_click(id: sys::XPLMWindowID, x: c_int, y: c_int, status: sys::XPLMMouseStatus, refcon: *mut c_void) -> c_int {
    guard("window right click", 1, || unsafe { inner(refcon) }.handler.right_click(WindowRef(id), x, y, mouse_status(status)) as c_int)
}

unsafe extern "C" fn key(id: sys::XPLMWindowID, key: c_char, flags: sys::XPLMKeyFlags, virtual_key: c_char, refcon: *mut c_void, losing_focus: c_int) {
    guard("window key", (), || {
        let handler = &mut unsafe { inner(refcon) }.handler;
        if losing_focus != 0 {
            handler.focus_lost(WindowRef(id));
            return;
        }
        let flags = flags as u32;
        let byte = key as u8;
        let event = KeyEvent {
            character: (0x20..0x7f).contains(&byte).then_some(byte as char),
            virtual_key: virtual_key as u8,
            shift: flags & sys::xplm_ShiftFlag != 0,
            alt: flags & sys::xplm_OptionAltFlag != 0,
            control: flags & sys::xplm_ControlFlag != 0,
            down: flags & sys::xplm_DownFlag != 0,
            up: flags & sys::xplm_UpFlag != 0,
        };
        handler.key(WindowRef(id), event);
    })
}

unsafe extern "C" fn cursor(id: sys::XPLMWindowID, x: c_int, y: c_int, refcon: *mut c_void) -> sys::XPLMCursorStatus {
    guard("window cursor", sys::xplm_CursorDefault as _, || match unsafe { inner(refcon) }.handler.cursor(WindowRef(id), x, y) {
        Cursor::Default => sys::xplm_CursorDefault as _,
        Cursor::Hidden => sys::xplm_CursorHidden as _,
        Cursor::Arrow => sys::xplm_CursorArrow as _,
    })
}

unsafe extern "C" fn wheel(id: sys::XPLMWindowID, x: c_int, y: c_int, axis: c_int, clicks: c_int, refcon: *mut c_void) -> c_int {
    guard("window wheel", 1, || unsafe { inner(refcon) }.handler.wheel(WindowRef(id), x, y, axis, clicks) as c_int)
}
