//! Safe wrappers over the X-Plane plugin SDK.
//!
//! Every function here must be called from X-Plane's main thread. Callbacks that
//! X-Plane invokes are wrapped in [`guard`] so a Rust panic never unwinds into the sim.

pub mod command;
pub mod dataref;
pub mod flight_loop;
pub mod graphics;
pub mod menu;
pub mod paths;
pub mod window;

pub use sidetone_xplm_sys as sys;

use std::cell::RefCell;
use std::ffi::CString;
use std::panic::{AssertUnwindSafe, catch_unwind};

/// Writes a line to X-Plane's `Log.txt`. Main thread only.
pub fn debug_string(message: &str) {
    let mut line = message.replace('\0', "");
    if !line.ends_with('\n') {
        line.push('\n');
    }
    if let Ok(c) = CString::new(line) {
        unsafe { sys::XPLMDebugString(c.as_ptr()) };
    }
}

/// Runs `f`, converting any panic into `fallback` and a log line instead of unwinding into X-Plane.
pub fn guard<T>(context: &str, fallback: T, f: impl FnOnce() -> T) -> T {
    match catch_unwind(AssertUnwindSafe(f)) {
        Ok(value) => value,
        Err(payload) => {
            let reason = payload
                .downcast_ref::<&str>()
                .map(|s| s.to_string())
                .or_else(|| payload.downcast_ref::<String>().cloned())
                .unwrap_or_else(|| "unknown panic".into());
            report_panic(context, &reason);
            fallback
        }
    }
}

/// Enables an SDK feature such as `XPLM_USE_NATIVE_PATHS`. Returns false if unsupported.
pub fn enable_feature(name: &str, enabled: bool) -> bool {
    let Ok(c) = CString::new(name) else { return false };
    unsafe {
        if sys::XPLMHasFeature(c.as_ptr()) == 0 {
            return false;
        }
        sys::XPLMEnableFeature(c.as_ptr(), enabled as i32);
    }
    true
}

/// Seconds since the sim started (wall clock, keeps running while paused).
pub fn elapsed_time() -> f32 {
    unsafe { sys::XPLMGetElapsedTime() }
}

thread_local! {
    static LAST_PANIC: RefCell<(String, u64)> = const { RefCell::new((String::new(), 0)) };
}

/// Logs a panic, collapsing repeats (a per-frame panic would otherwise flood Log.txt).
fn report_panic(context: &str, reason: &str) {
    let line = format!("[Sidetone] panic in {context}: {reason}");
    LAST_PANIC.with(|last| {
        let mut last = last.borrow_mut();
        if last.0 == line {
            last.1 += 1;
            if last.1.is_power_of_two() {
                debug_string(&format!("{line} (repeated {} times)", last.1));
            }
        } else {
            *last = (line.clone(), 1);
            debug_string(&line);
        }
    });
}
