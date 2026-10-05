//! Hoppie ACARS (<https://www.hoppie.nl/acars/>): the network VATSIM controllers use for CPDLC
//! and pre-departure clearances.

pub mod protocol;
pub mod session;

pub use protocol::{Incoming, Kind, Outgoing, ResponseAttr};
pub use session::{Direction, LogonState, Message, Session};

pub const CONNECT_URL: &str = "https://www.hoppie.nl/acars/system/connect.html";
