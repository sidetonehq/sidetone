use crate::sys;
use std::ffi::CString;

/// A handle to an X-Plane dataref. Lookups are cached; reading is cheap.
#[derive(Clone, Copy, Debug)]
pub struct DataRef(sys::XPLMDataRef);

impl DataRef {
    pub fn find(name: &str) -> Option<DataRef> {
        let c = CString::new(name).ok()?;
        let r = unsafe { sys::XPLMFindDataRef(c.as_ptr()) };
        (!r.is_null()).then_some(DataRef(r))
    }

    pub fn get_i32(&self) -> i32 {
        unsafe { sys::XPLMGetDatai(self.0) }
    }

    pub fn set_i32(&self, value: i32) {
        unsafe { sys::XPLMSetDatai(self.0, value) }
    }

    pub fn get_f32(&self) -> f32 {
        unsafe { sys::XPLMGetDataf(self.0) }
    }

    pub fn set_f32(&self, value: f32) {
        unsafe { sys::XPLMSetDataf(self.0, value) }
    }

    pub fn get_f64(&self) -> f64 {
        unsafe { sys::XPLMGetDatad(self.0) }
    }

    /// Reads a byte-array dataref as a string (trailing NULs stripped). Returns "" if empty.
    pub fn get_string(&self) -> String {
        let len = unsafe { sys::XPLMGetDatab(self.0, std::ptr::null_mut(), 0, 0) };
        if len <= 0 {
            return String::new();
        }
        let mut buffer = vec![0u8; len as usize];
        let read = unsafe { sys::XPLMGetDatab(self.0, buffer.as_mut_ptr() as *mut std::os::raw::c_void, 0, len) };
        buffer.truncate(read.max(0) as usize);
        while buffer.last() == Some(&0) {
            buffer.pop();
        }
        String::from_utf8_lossy(&buffer).into_owned()
    }

    pub fn is_writable(&self) -> bool {
        unsafe { sys::XPLMCanWriteDataRef(self.0) != 0 }
    }
}
