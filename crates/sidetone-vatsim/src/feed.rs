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
    /// The flight plan `cid` filed for this trip: the live connection's if it's for these airports,
    /// else the newest prefile that is. After landing you're often still connected with the last
    /// flight's plan while the next one sits in the prefiles.
    pub fn flight_plan_for_trip(&self, cid: u32, departure: &str, arrival: &str) -> Option<&FlightPlan> {
        let this_trip = |fp: &&FlightPlan| fp.departure.eq_ignore_ascii_case(departure) && fp.arrival.eq_ignore_ascii_case(arrival);
        let live = self.pilots.iter().filter(|p| p.cid == cid).filter_map(|p| p.flight_plan.as_ref()).find(this_trip);
        live.or_else(|| {
            self.prefiles
                .iter()
                .filter(|p| p.cid == cid && p.flight_plan.as_ref().is_some_and(|fp| this_trip(&fp)))
                .max_by_key(|p| crate::time::parse_iso8601(&p.last_updated).unwrap_or(i64::MIN))
                .and_then(|p| p.flight_plan.as_ref())
        })
    }

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
    fn the_plan_for_this_trip() {
        // Still connected after Heathrow–Manchester, with the next flight prefiled.
        let feed: DataFeed = serde_json::from_str(
            r#"{"pilots":[{"cid":5,"callsign":"VIR341","flight_plan":{"departure":"EGLL","arrival":"EGCC","route":"UMLAT T418 WELIN T420 ELVOS"}}],
                "prefiles":[{"cid":5,"callsign":"VIR342","last_updated":"2026-10-07T18:00:00Z","flight_plan":{"departure":"EGCC","arrival":"EGLL","route":"LISTO L612 HON N859 KIDLI"}}]}"#,
        )
        .unwrap();
        assert_eq!(
            feed.flight_plan_for_trip(5, "EGCC", "EGLL").map(|f| f.route.as_str()),
            Some("LISTO L612 HON N859 KIDLI"),
            "the prefile for the next flight"
        );
        assert_eq!(
            feed.flight_plan_for_trip(5, "egll", "egcc").map(|f| f.route.as_str()),
            Some("UMLAT T418 WELIN T420 ELVOS"),
            "the live one when it's this trip"
        );
        assert!(feed.flight_plan_for_trip(5, "EGGD", "EDDC").is_none(), "none filed for this trip");
    }

    #[test]
    fn parses_transceivers() {
        let t: Vec<TransceiverEntry> = serde_json::from_str(include_str!("../tests/fixtures/transceivers-data.json")).unwrap();
        assert_eq!(t[0].transceivers[0].frequency, 118_700_004);
    }
}
