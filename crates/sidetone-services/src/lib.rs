//! Services outside VATSIM's own network, usable before Sidetone is an approved client.
//!
//! - [`simbrief`]: import the latest OFP by SimBrief username.
//! - [`keychain`]: secrets (the SimBrief Pilot ID) in the macOS Keychain.
//! - [`worker`]: one background thread doing all of the above's network calls.

pub mod keychain;
pub mod simbrief;
pub mod worker;
