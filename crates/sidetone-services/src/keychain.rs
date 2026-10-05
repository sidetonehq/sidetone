//! Secrets live in the macOS Keychain (service "Sidetone"), never in settings files or logs.

const SERVICE: &str = "Sidetone";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Secret {
    HoppieLogon,
    SimbriefUsername,
}

impl Secret {
    fn account(self) -> &'static str {
        match self {
            Secret::HoppieLogon => "hoppie-logon-code",
            Secret::SimbriefUsername => "simbrief-username",
        }
    }
}

#[cfg(target_os = "macos")]
pub fn get(secret: Secret) -> Option<String> {
    security_framework::passwords::get_generic_password(SERVICE, secret.account()).ok().and_then(|b| String::from_utf8(b).ok())
}

#[cfg(target_os = "macos")]
pub fn set(secret: Secret, value: &str) -> Result<(), String> {
    security_framework::passwords::set_generic_password(SERVICE, secret.account(), value.as_bytes()).map_err(|e| e.to_string())
}

#[cfg(target_os = "macos")]
pub fn delete(secret: Secret) {
    let _ = security_framework::passwords::delete_generic_password(SERVICE, secret.account());
}

#[cfg(not(target_os = "macos"))]
pub fn get(_secret: Secret) -> Option<String> {
    None
}

#[cfg(not(target_os = "macos"))]
pub fn set(_secret: Secret, _value: &str) -> Result<(), String> {
    Err("Keychain is only available on macOS".into())
}

#[cfg(not(target_os = "macos"))]
pub fn delete(_secret: Secret) {}
