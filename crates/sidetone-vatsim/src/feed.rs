//! Models for `vatsim-data.json` and `transceivers-data.json` (v3). Unknown fields are ignored
//! and most fields default, so feed additions never break parsing.

use serde::Deserialize;

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct DataFeed {
    pub general: General,
    pub pilots: Vec<Pilot>,
    pub controllers: Vec<Controller>,
    pub atis: Vec<Controller>,
    pub prefiles: Vec<Prefile>,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct General {
    pub update_timestamp: String,
    pub connected_clients: u32,
    pub unique_users: u32,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct Pilot {
    pub cid: u32,
    pub name: String,
    pub callsign: String,
    pub latitude: f64,
    pub longitude: f64,
    pub altitude: i32,
    pub groundspeed: i32,
    pub transponder: String,
    pub flight_plan: Option<FlightPlan>,
    pub logon_time: String,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct FlightPlan {
    pub flight_rules: String,
    pub aircraft_short: String,
    pub departure: String,
    pub arrival: String,
    pub alternate: String,
    pub altitude: String,
    pub route: String,
    pub remarks: String,
    pub assigned_transponder: String,
}

/// A controller or ATIS connection.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct Controller {
    pub cid: u32,
    pub name: String,
    pub callsign: String,
    pub frequency: String,
    pub facility: i32,
    pub rating: i32,
    pub visual_range: i32,
    pub atis_code: Option<String>,
    pub text_atis: Option<Vec<String>>,
    pub logon_time: String,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct Prefile {
    pub cid: u32,
    pub callsign: String,
    pub flight_plan: Option<FlightPlan>,
    /// When it was filed or last amended (ISO 8601).
    pub last_updated: String,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct TransceiverEntry {
    pub callsign: String,
    pub transceivers: Vec<Transceiver>,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Transceiver {
    /// Hz, with a little noise (e.g. 122800015).
    pub frequency: i64,
    pub lat_deg: f64,
    pub lon_deg: f64,
    pub height_msl_m: f64,
}

impl DataFeed {
    /// The flight plan for `cid`: the live connection's, else the most recently filed prefile.
    /// A member can have several prefiles at once (one per callsign, until they expire), and
    /// the newest is the flight they're about to fly.
    pub fn flight_plan_for(&self, cid: u32) -> Option<&FlightPlan> {
        let live = self.pilots.iter().find(|p| p.cid == cid).and_then(|p| p.flight_plan.as_ref());
        live.or_else(|| {
            self.prefiles
                .iter()
                .filter(|p| p.cid == cid && p.flight_plan.is_some())
                .max_by_key(|p| crate::time::parse_iso8601(&p.last_updated).unwrap_or(i64::MIN))
                .and_then(|p| p.flight_plan.as_ref())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_sample_feed() {
        let feed: DataFeed = serde_json::from_str(include_str!("../tests/fixtures/vatsim-data.json")).unwrap();
        assert_eq!(feed.controllers.len(), 4);
        assert_eq!(feed.atis[0].atis_code.as_deref(), Some("C"));
        assert_eq!(feed.flight_plan_for(1234567).unwrap().arrival, "ENBR");
        assert_eq!(feed.flight_plan_for(7654321).unwrap().departure, "EGLL");
    }

    #[test]
    fn latest_prefile_wins() {
        let feed: DataFeed = serde_json::from_str(
            r#"{"prefiles":[
                {"cid":5,"callsign":"VIR324","last_updated":"2026-10-06T20:45:43.5383586Z","flight_plan":{"departure":"LFPO","arrival":"LIRF"}},
                {"cid":5,"callsign":"VIR434","last_updated":"2026-10-06T21:27:03.0980449Z","flight_plan":{"departure":"EGGD","arrival":"EDDC"}},
                {"cid":5,"callsign":"VIR325","last_updated":"2026-10-06T20:48:14.7351191Z","flight_plan":{"departure":"LFPO","arrival":"LIRF"}}
            ]}"#,
        )
        .unwrap();
        assert_eq!(feed.flight_plan_for(5).unwrap().arrival, "EDDC", "the newest prefile, wherever it sits in the feed");
    }

    #[test]
    fn parses_transceivers() {
        let t: Vec<TransceiverEntry> = serde_json::from_str(include_str!("../tests/fixtures/transceivers-data.json")).unwrap();
        assert_eq!(t[0].transceivers[0].frequency, 118_700_004);
    }
}
