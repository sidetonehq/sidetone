//! Services outside VATSIM's own network, usable before Sidetone is an approved client.
//!
//! - [`simbrief`]: import the latest OFP by SimBrief username.
//! - [`hoppie`]: Hoppie ACARS wire protocol and the pilot-side CPDLC session.
//! - [`keychain`]: secrets (Hoppie logon code, SimBrief username) in the macOS Keychain.
//! - [`worker`]: one background thread doing all of the above's network calls.

pub mod hoppie;
pub mod keychain;
pub mod simbrief;
pub mod worker;
