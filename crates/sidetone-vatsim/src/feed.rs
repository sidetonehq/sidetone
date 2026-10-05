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
    /// The flight plan for `cid`: from the live connection, else a prefile.
    pub fn flight_plan_for(&self, cid: u32) -> Option<&FlightPlan> {
        self.pilots
            .iter()
            .find(|p| p.cid == cid)
            .and_then(|p| p.flight_plan.as_ref())
            .or_else(|| self.prefiles.iter().find(|p| p.cid == cid).and_then(|p| p.flight_plan.as_ref()))
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
    fn parses_transceivers() {
        let t: Vec<TransceiverEntry> = serde_json::from_str(include_str!("../tests/fixtures/transceivers-data.json")).unwrap();
        assert_eq!(t[0].transceivers[0].frequency, 118_700_004);
    }
}
