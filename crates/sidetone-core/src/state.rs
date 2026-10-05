//! The single source of truth the UI renders from. Owned by the main thread.

use crate::radio::TransponderMode;
use sidetone_vatsim::boundaries::Boundaries;
use sidetone_vatsim::coverage::RouteLeg;
use sidetone_vatsim::events::Event;
use sidetone_vatsim::geo::LatLon;
use sidetone_vatsim::http::MemberStats;
use sidetone_vatsim::vatspy::VatSpy;
use sidetone_vatsim::worker::Snapshot;
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Connection {
    #[default]
    Disconnected,
    Connecting,
    Connected {
        callsign: String,
    },
}

impl Connection {
    pub fn label(&self) -> &str {
        match self {
            Connection::Disconnected => "Offline",
            Connection::Connecting => "Connecting…",
            Connection::Connected { callsign } => callsign,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct Radio {
    pub active_khz: i32,
    pub standby_khz: i32,
    /// What's on the active frequency, per the VATSIM data feed.
    pub station: Option<TunedStation>,
    pub receiving: bool,
    pub transmitting: bool,
    pub powered: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TunedStation {
    pub callsign: String,
    /// Spoken name ("Heathrow Tower"), or "UNICOM" on 122.800 with no ATC.
    pub name: String,
    pub distance_nm: Option<f64>,
    /// Further away than line-of-sight VHF range.
    pub out_of_range: bool,
}

/// Public VATSIM data (no connection needed).
#[derive(Debug, Default)]
pub struct Network {
    pub snapshot: Option<Arc<Snapshot>>,
    pub vatspy: Option<Arc<VatSpy>>,
    pub feed_error: Option<String>,
    pub metars: HashMap<String, String>,
    pub stats: Option<(u32, MemberStats)>,
    pub boundaries: Option<Arc<Boundaries>>,
    pub events: Arc<Vec<Event>>,
    pub route_atc: Arc<Vec<RouteLeg>>,
}

/// A one-line suggestion about which frequency to be on.
#[derive(Clone, Debug, PartialEq)]
pub enum CoverageHint {
    /// An online station covers you but neither radio is on it.
    Tune { name: String, khz: i32 },
    /// Airborne with no ATC overhead and not on UNICOM.
    Unicom,
}

/// Sidetone's own CPU cost, to prove it doesn't eat frames.
#[derive(Clone, Copy, Debug, Default)]
pub struct Perf {
    pub panel_ms: f32,
    pub window_ms: f32,
    pub loop_ms: f32,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Route {
    pub departure: Option<String>,
    pub arrival: Option<String>,
    /// Where the airports came from, for the UI ("VATSIM flight plan", "manual").
    pub source: &'static str,
}

#[derive(Clone, Debug, Default)]
pub struct Transponder {
    pub code: i32,
    pub mode: TransponderMode,
    pub ident: bool,
}

impl AppState {
    /// Why the transponder needs attention, if it does: wrong code, or not in altitude mode airborne.
    pub fn transponder_warning(&self, assigned: &str) -> Option<String> {
        let assigned = assigned.trim();
        if crate::radio::is_valid_squawk(assigned) && crate::radio::format_squawk(self.transponder.code) != assigned {
            return Some(format!("Assigned {assigned}"));
        }
        if !self.on_ground && !self.transponder.mode.is_mode_c() {
            return Some("Set ALT".into());
        }
        None
    }
}

#[derive(Clone, Debug)]
pub struct Message {
    pub from: String,
    pub text: String,
    /// `elapsed_time()` when received, for fading the panel ticker.
    pub received_at: f32,
}

#[derive(Debug, Default)]
pub struct AppState {
    pub connection: Connection,
    pub com1: Radio,
    pub com2: Radio,
    pub transponder: Transponder,
    pub messages: Vec<Message>,
    pub unread: usize,
    pub ptt_pressed: bool,
    pub network: Network,
    pub position: Option<LatLon>,
    pub altitude_ft: f64,
    pub route: Route,
    /// Nearest airport to the aircraft (ICAO, distance nm).
    pub nearest_airport: Option<(String, f64)>,
    pub on_ground: bool,
    pub coverage_hint: Option<CoverageHint>,
    /// A Sidetone text field currently holds the keyboard.
    pub keyboard_captured: bool,
    /// Set when the connection shown comes from another client ("xPilot").
    pub connection_via: Option<&'static str>,
    /// Aircraft the connected client is rendering nearby.
    pub nearby_aircraft: Option<u32>,
    pub perf: Perf,
}

impl AppState {
    /// The most recent message, if it arrived within `window` seconds of `now`.
    pub fn latest_message(&self, now: f32, window: f32) -> Option<&Message> {
        self.messages.last().filter(|m| now - m.received_at <= window)
    }

    pub fn push_message(&mut self, from: impl Into<String>, text: impl Into<String>, now: f32) {
        self.messages.push(Message { from: from.into(), text: text.into(), received_at: now });
        self.unread += 1;
    }

    /// Airports whose ATIS/METAR we watch: departure, arrival, else the nearest airport.
    pub fn watched_airports(&self) -> Vec<String> {
        let mut out: Vec<String> = [&self.route.departure, &self.route.arrival].into_iter().flatten().cloned().collect();
        if out.is_empty()
            && let Some((icao, _)) = &self.nearest_airport
        {
            out.push(icao.clone());
        }
        out.dedup();
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transponder_warnings() {
        let mut s = AppState { on_ground: true, ..Default::default() };
        s.transponder.code = 1200;
        assert_eq!(s.transponder_warning("4721").as_deref(), Some("Assigned 4721"));
        s.transponder.code = 4721;
        assert_eq!(s.transponder_warning("4721"), None);
        s.on_ground = false;
        assert_eq!(s.transponder_warning("4721").as_deref(), Some("Set ALT"));
        s.transponder.mode = TransponderMode::Alt;
        assert_eq!(s.transponder_warning(""), None);
    }

    #[test]
    fn latest_message_expires() {
        let mut s = AppState::default();
        s.push_message("EGLL_TWR", "hello", 10.0);
        assert!(s.latest_message(15.0, 8.0).is_some());
        assert!(s.latest_message(19.0, 8.0).is_none());
        assert_eq!(s.unread, 1);
    }
}
