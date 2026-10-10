//! VATSIM public data, independent of X-Plane.
//!
//! - [`feed`]: the v3 data feed and transceivers feed (`data.vatsim.net`).
//! - [`vatspy`]: the VATSpy data project (airport/FIR names), CC BY-SA 4.0, downloaded at runtime.
//! - [`naming`]: turns a callsign into the name you say on the radio ("Heathrow Tower").
//! - [`stations`]: joins the above into tunable stations and matches a COM frequency to one.
//! - [`worker`]: a background thread that keeps everything fresh.

pub mod boundaries;
pub mod coverage;
pub mod events;
pub mod feed;
pub mod freq;
pub mod geo;
pub mod http;
pub mod naming;
pub mod sectors;
pub mod stations;
pub mod time;
pub mod tracon;
pub mod vatspy;
pub mod worker;
