//! Secrets live in the macOS Keychain (service "Sidetone"), never in settings files or logs.

const SERVICE: &str = "Sidetone";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Secret {
    SimbriefUsername,
}

impl Secret {
    fn account(self) -> &'static str {
        match self {
            Secret::SimbriefUsername => "simbrief-username",
        }
    }
}

/// Accounts from removed features (the Hoppie CPDLC logon code), deleted on startup so nothing
/// Sidetone no longer uses stays in the Keychain.
const RETIRED_ACCOUNTS: [&str; 1] = ["hoppie-logon-code"];

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

#[cfg(target_os = "macos")]
pub fn delete_retired() {
    for account in RETIRED_ACCOUNTS {
        let _ = security_framework::passwords::delete_generic_password(SERVICE, account);
    }
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

#[cfg(not(target_os = "macos"))]
pub fn delete_retired() {
    let _ = RETIRED_ACCOUNTS;
}
