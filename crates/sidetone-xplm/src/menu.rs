use crate::{guard, sys};
use std::ffi::CString;
use std::os::raw::{c_int, c_void};

type Handler = Box<dyn FnMut(usize)>;

/// A submenu under X-Plane's Plugins menu. Item clicks call the handler with the item index.
pub struct PluginMenu {
    id: sys::XPLMMenuID,
    parent_item: c_int,
    _handler: Box<Handler>,
    items: usize,
}

impl PluginMenu {
    pub fn new(title: &str, handler: impl FnMut(usize) + 'static) -> Option<PluginMenu> {
        let title = CString::new(title).ok()?;
        let mut handler: Box<Handler> = Box::new(Box::new(handler));
        unsafe {
            let plugins = sys::XPLMFindPluginsMenu();
            let parent_item = sys::XPLMAppendMenuItem(plugins, title.as_ptr(), std::ptr::null_mut(), 0);
            let id = sys::XPLMCreateMenu(title.as_ptr(), plugins, parent_item, Some(trampoline), &mut *handler as *mut Handler as *mut c_void);
            (!id.is_null()).then_some(PluginMenu { id, parent_item, _handler: handler, items: 0 })
        }
    }

    /// Appends an item and returns its index.
    pub fn add_item(&mut self, name: &str) -> usize {
        let index = self.items;
        if let Ok(c) = CString::new(name) {
            unsafe { sys::XPLMAppendMenuItem(self.id, c.as_ptr(), index as *mut c_void, 0) };
            self.items += 1;
        }
        index
    }

    pub fn add_separator(&mut self) {
        unsafe { sys::XPLMAppendMenuSeparator(self.id) };
    }

    pub fn set_checked(&self, index: usize, checked: bool) {
        let state = if checked { sys::xplm_Menu_Checked } else { sys::xplm_Menu_Unchecked };
        unsafe { sys::XPLMCheckMenuItem(self.id, index as c_int, state as _) };
    }
}

impl Drop for PluginMenu {
    fn drop(&mut self) {
        unsafe {
            sys::XPLMDestroyMenu(self.id);
            sys::XPLMRemoveMenuItem(sys::XPLMFindPluginsMenu(), self.parent_item);
        }
    }
}

unsafe extern "C" fn trampoline(menu_ref: *mut c_void, item_ref: *mut c_void) {
    guard("menu handler", (), || {
        let handler = unsafe { &mut *(menu_ref as *mut Handler) };
        handler(item_ref as usize)
    })
}
