use crate::{guard, sys};
use std::os::raw::{c_int, c_void};

type Callback = Box<dyn FnMut(f32) -> f32>;

/// A flight loop callback. The closure receives seconds since its last call and returns
/// the delay until the next call: positive = seconds, negative = frames, 0 = stop.
pub struct FlightLoop {
    id: sys::XPLMFlightLoopID,
    _callback: Box<Callback>,
}

impl FlightLoop {
    pub fn new(callback: impl FnMut(f32) -> f32 + 'static) -> FlightLoop {
        let mut callback: Box<Callback> = Box::new(Box::new(callback));
        let mut params = sys::XPLMCreateFlightLoop_t {
            structSize: std::mem::size_of::<sys::XPLMCreateFlightLoop_t>() as c_int,
            phase: sys::xplm_FlightLoop_Phase_AfterFlightModel as _,
            callbackFunc: Some(trampoline),
            refcon: &mut *callback as *mut Callback as *mut c_void,
        };
        let id = unsafe { sys::XPLMCreateFlightLoop(&mut params) };
        FlightLoop { id, _callback: callback }
    }

    /// Schedules the next call. `interval` uses the same convention as the callback return value.
    pub fn schedule(&self, interval: f32) {
        unsafe { sys::XPLMScheduleFlightLoop(self.id, interval, 1) }
    }
}

impl Drop for FlightLoop {
    fn drop(&mut self) {
        unsafe { sys::XPLMDestroyFlightLoop(self.id) }
    }
}

unsafe extern "C" fn trampoline(elapsed_since_last_call: f32, _elapsed_since_last_loop: f32, _counter: c_int, refcon: *mut c_void) -> f32 {
    guard("flight loop", 1.0, || {
        let callback = unsafe { &mut *(refcon as *mut Callback) };
        callback(elapsed_since_last_call)
    })
}
