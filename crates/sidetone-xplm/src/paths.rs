use crate::sys;
use std::ffi::CStr;
use std::path::PathBuf;

fn read_path(f: unsafe extern "C" fn(*mut std::os::raw::c_char)) -> PathBuf {
    let mut buffer = vec![0 as std::os::raw::c_char; 1024];
    unsafe {
        f(buffer.as_mut_ptr());
        PathBuf::from(CStr::from_ptr(buffer.as_ptr()).to_string_lossy().into_owned())
    }
}

/// X-Plane's root folder. Call `enable_feature("XPLM_USE_NATIVE_PATHS", true)` first.
pub fn system_path() -> PathBuf {
    read_path(sys::XPLMGetSystemPath)
}

/// X-Plane's `Output/preferences` folder.
pub fn preferences_dir() -> PathBuf {
    let prefs_file = read_path(sys::XPLMGetPrefsPath);
    prefs_file.parent().map(PathBuf::from).unwrap_or(prefs_file)
}

/// The folder containing this plugin's `.xpl` (e.g. `.../plugins/Sidetone/mac_x64`).
pub fn plugin_dir() -> Option<PathBuf> {
    let mut buffer = vec![0 as std::os::raw::c_char; 1024];
    unsafe {
        let me = sys::XPLMGetMyID();
        sys::XPLMGetPluginInfo(me, std::ptr::null_mut(), buffer.as_mut_ptr(), std::ptr::null_mut(), std::ptr::null_mut());
        let file = PathBuf::from(CStr::from_ptr(buffer.as_ptr()).to_string_lossy().into_owned());
        file.parent().map(PathBuf::from)
    }
}
