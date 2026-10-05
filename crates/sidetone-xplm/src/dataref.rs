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

    pub fn is_writable(&self) -> bool {
        unsafe { sys::XPLMCanWriteDataRef(self.0) != 0 }
    }
}
