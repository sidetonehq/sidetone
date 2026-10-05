//! X-Plane entry points. X-Plane calls these on its main thread; everything is guarded so a
//! panic is logged instead of crashing the sim.

mod app;
mod bridge;
mod sim;
mod ui;

use sidetone_core::bus::Bus;
use sidetone_core::logging;
use sidetone_xplm::{debug_string, enable_feature, guard, paths};
use std::cell::RefCell;
use std::os::raw::{c_char, c_int, c_void};

pub const NAME: &str = "Sidetone";
pub const SIGNATURE: &str = "com.sidetonehq.sidetone";
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

thread_local! {
    static BUS: RefCell<Option<Bus>> = const { RefCell::new(None) };
    static APP: RefCell<Option<app::App>> = const { RefCell::new(None) };
}

fn write_c(out: *mut c_char, text: &str) {
    // X-Plane gives each buffer 256 bytes.
    let bytes = text.as_bytes();
    let n = bytes.len().min(255);
    unsafe {
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), out as *mut u8, n);
        *out.add(n) = 0;
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn XPluginStart(name: *mut c_char, signature: *mut c_char, description: *mut c_char) -> c_int {
    write_c(name, NAME);
    write_c(signature, SIGNATURE);
    write_c(description, "Native VATSIM pilot client for X-Plane 12");
    guard("XPluginStart", 0, || {
        enable_feature("XPLM_USE_NATIVE_PATHS", true);
        enable_feature("XPLM_USE_NATIVE_WIDGET_WINDOWS", true);
        let bus = Bus::new();
        let log_file = paths::system_path().join("Output").join("Sidetone").join("Sidetone.log");
        logging::init(&log_file, bus.sender());
        BUS.with(|b| *b.borrow_mut() = Some(bus));
        log::info!("Sidetone {VERSION} starting; log at {}", log_file.display());
        debug_string(&format!("[Sidetone] {VERSION} loaded, log: {}", log_file.display()));
        1
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn XPluginEnable() -> c_int {
    guard("XPluginEnable", 0, || {
        let sender = BUS.with(|b| b.borrow().as_ref().map(|bus| bus.sender()));
        let Some(sender) = sender else { return 0 };
        let app = app::App::new(sender);
        APP.with(|a| *a.borrow_mut() = Some(app));
        log::info!("Sidetone enabled");
        1
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn XPluginDisable() {
    guard("XPluginDisable", (), || {
        // Take it out first so drop-time callbacks can't re-borrow APP.
        let app = APP.with(|a| a.borrow_mut().take());
        drop(app);
        log::info!("Sidetone disabled");
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn XPluginStop() {
    guard("XPluginStop", (), || {
        log::info!("Sidetone stopping");
        drain_bus(|_| {});
        logging::shutdown();
        BUS.with(|b| *b.borrow_mut() = None);
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn XPluginReceiveMessage(_from: c_int, _message: c_int, _param: *mut c_void) {}

/// Delivers worker-thread events on the main thread. Log lines go straight to Log.txt;
/// everything else is passed to `handle`. Called from the flight loop.
pub(crate) fn drain_bus(mut handle: impl FnMut(sidetone_core::bus::Event)) {
    let events: Vec<_> = BUS.with(|b| b.borrow().as_ref().map(|bus| bus.drain().collect()).unwrap_or_default());
    for event in events {
        match event {
            sidetone_core::bus::Event::SimLog(line) => debug_string(&line),
            other => handle(other),
        }
    }
}
