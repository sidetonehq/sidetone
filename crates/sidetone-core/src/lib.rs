//! Sim-independent core of Sidetone: state, settings, formatting and the event bus.
//! Nothing in here touches the X-Plane SDK, so it can be unit tested on any machine.

pub mod bus;
pub mod clearance;
pub mod dot_command;
pub mod geometry;
pub mod layout;
pub mod logging;
pub mod radio;
pub mod settings;
pub mod setup;
pub mod state;
pub mod watch;
