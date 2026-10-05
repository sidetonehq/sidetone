//! Optional bridges to other pilot clients. These are opt-in "modes" kept apart from the main
//! product: when Sidetone's own network client arrives, it replaces them without touching
//! the rest of the app.

pub mod xpilot;
