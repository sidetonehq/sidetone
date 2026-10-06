//! Turns successive VATSIM snapshots into pilot-facing notices: ATIS letter changes, METAR
//! changes and events at your airports. The first observation of anything never alerts.

use crate::settings::Settings;
use crate::state::Route;
use sidetone_services::simbrief::Plan;
use sidetone_vatsim::feed::DataFeed;
use sidetone_vatsim::stations::Station;
use std::collections::{HashMap, HashSet};

#[derive(Default)]
pub struct Watcher {
    atis_codes: HashMap<String, String>,
    metars: HashMap<String, String>,
    events_announced: HashSet<u64>,
}

impl Watcher {
    /// ATIS changes at `airports` (ICAO list). Returns notices.
    pub fn atis(&mut self, stations: &[Station], airports: &[String]) -> Vec<String> {
        let mut notices = Vec::new();
        for s in stations.iter().filter(|s| s.atis_code.is_some()) {
            let icao = s.callsign.split('_').next().unwrap_or_default();
            if !airports.iter().any(|a| a == icao) {
                continue;
            }
            let code = s.atis_code.clone().unwrap_or_default();
            if let Some(old) = self.atis_codes.insert(s.callsign.clone(), code.clone())
                && old != code
                && !code.is_empty()
            {
                notices.push(format!("{} is now information {}", s.callsign, code));
            }
        }
        notices
    }

    /// METAR changes. Returns notices.
    pub fn metars(&mut self, metars: &HashMap<String, String>) -> Vec<String> {
        let mut notices = Vec::new();
        for (icao, text) in metars {
            if let Some(old) = self.metars.insert(icao.clone(), text.clone())
                && old != *text
            {
                notices.push(format!("New METAR {text}"));
            }
        }
        notices
    }

    /// Notices for events at your airports that are live or start within two hours (once each).
    pub fn events(&mut self, events: &[sidetone_vatsim::events::Event], airports: &[String], now: i64) -> Vec<String> {
        let mut notices = Vec::new();
        for e in events {
            let relevant = airports.iter().any(|a| e.involves(a));
            if !relevant || e.start > now + 2 * 3600 || !self.events_announced.insert(e.id) {
                continue;
            }
            let when = if e.is_live(now) { "now".to_string() } else { format!("from {}", sidetone_vatsim::time::format_utc(e.start, now)) };
            notices.push(format!("Event at your airport {when}: {}", e.name));
        }
        notices
    }
}

/// Finds the pilot's CID from the public feed: a live connection with `callsign`, else a
/// prefile with that callsign whose airports match. Only an unambiguous match counts.
pub fn detect_cid(feed: &DataFeed, callsign: &str, departure: Option<&str>, arrival: Option<&str>) -> Option<u32> {
    let callsign = callsign.trim();
    if callsign.is_empty() {
        return None;
    }
    let live: Vec<u32> = feed.pilots.iter().filter(|p| p.callsign.eq_ignore_ascii_case(callsign)).map(|p| p.cid).collect();
    if let [cid] = live[..] {
        return Some(cid);
    }
    let airports_match = |fp: &sidetone_vatsim::feed::FlightPlan| {
        departure.is_some_and(|d| fp.departure.eq_ignore_ascii_case(d)) && arrival.is_some_and(|a| fp.arrival.eq_ignore_ascii_case(a))
    };
    let prefiled: Vec<u32> = feed
        .prefiles
        .iter()
        .filter(|p| p.callsign.eq_ignore_ascii_case(callsign) && p.flight_plan.as_ref().is_some_and(airports_match))
        .map(|p| p.cid)
        .collect();
    match prefiled[..] {
        [cid] => Some(cid),
        _ => None,
    }
}

/// Departure/arrival: manual entries win, then an imported SimBrief plan, then your VATSIM
/// flight plan (live or prefiled).
pub fn route(settings: &Settings, simbrief: Option<&Plan>, feed: Option<&DataFeed>) -> Route {
    let manual = |s: &str| {
        let s = s.trim().to_ascii_uppercase();
        (!s.is_empty()).then_some(s)
    };
    let (dep, arr) = (manual(&settings.vatsim.departure), manual(&settings.vatsim.arrival));
    if dep.is_some() || arr.is_some() {
        return Route { departure: dep, arrival: arr, source: "manual" };
    }
    if let Some(plan) = simbrief.filter(|p| !p.origin.is_empty() || !p.destination.is_empty()) {
        let non_empty = |s: &str| (!s.is_empty()).then(|| s.to_string());
        return Route { departure: non_empty(&plan.origin), arrival: non_empty(&plan.destination), source: "SimBrief" };
    }
    if let (Some(cid), Some(feed)) = (settings.vatsim.cid, feed)
        && let Some(fp) = feed.flight_plan_for(cid)
    {
        let non_empty = |s: &str| (!s.is_empty()).then(|| s.to_string());
        return Route { departure: non_empty(&fp.departure), arrival: non_empty(&fp.arrival), source: "VATSIM flight plan" };
    }
    Route::default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use sidetone_vatsim::naming::Facility;

    fn atis(code: &str) -> Station {
        Station {
            callsign: "EGLL_ATIS".into(),
            name: "Heathrow Information".into(),
            name_source: sidetone_vatsim::naming::NameSource::VatSpy,
            facility: Facility::Atis,
            frequency_khz: 128_075,
            frequencies_hz: vec![],
            positions: vec![],
            controller: String::new(),
            cid: 9,
            rating: 1,
            text: vec![],
            atis_code: Some(code.into()),
        }
    }

    #[test]
    fn atis_letter_change_alerts_once() {
        let mut w = Watcher::default();
        let airports = vec!["EGLL".to_string()];
        assert!(w.atis(&[atis("A")], &airports).is_empty());
        assert!(w.atis(&[atis("A")], &airports).is_empty());
        assert_eq!(w.atis(&[atis("B")], &airports), vec!["EGLL_ATIS is now information B"]);
        assert!(w.atis(&[atis("C")], &["ENBR".to_string()]).is_empty());
    }

    #[test]
    fn event_notices_once() {
        use sidetone_vatsim::events::Event;
        let e = Event { id: 7, name: "Fly-in".into(), link: String::new(), airports: vec!["EGLL".into()], start: 1000, end: 5000, summary: String::new() };
        let mut w = Watcher::default();
        let airports = vec!["EGLL".to_string()];
        assert!(w.events(std::slice::from_ref(&e), &airports, 1000 - 3 * 3600).is_empty(), "too far ahead");
        assert_eq!(w.events(std::slice::from_ref(&e), &airports, 1500).len(), 1);
        assert!(w.events(std::slice::from_ref(&e), &airports, 1600).is_empty(), "only once");
    }

    #[test]
    fn metar_change_alerts() {
        let mut w = Watcher::default();
        let mut m = HashMap::from([("EGLL".to_string(), "EGLL 1 Q1022".to_string())]);
        assert!(w.metars(&m).is_empty());
        m.insert("EGLL".into(), "EGLL 2 Q1021".into());
        assert_eq!(w.metars(&m).len(), 1);
    }

    #[test]
    fn detects_cid_unambiguously() {
        let feed: DataFeed = serde_json::from_str(
            r#"{"pilots":[{"cid":11,"callsign":"BAW123"},{"cid":12,"callsign":"DUP"},{"cid":13,"callsign":"DUP"}],
                "prefiles":[{"cid":21,"callsign":"SAS42","flight_plan":{"departure":"EGLL","arrival":"ESSA"}}]}"#,
        )
        .unwrap();
        assert_eq!(detect_cid(&feed, "baw123", None, None), Some(11));
        assert_eq!(detect_cid(&feed, "DUP", None, None), None, "ambiguous");
        assert_eq!(detect_cid(&feed, "SAS42", Some("EGLL"), Some("ESSA")), Some(21));
        assert_eq!(detect_cid(&feed, "SAS42", Some("EGKK"), Some("ESSA")), None, "airports must match");
        assert_eq!(detect_cid(&feed, "", None, None), None);
    }

    #[test]
    fn route_prefers_manual_then_flight_plan() {
        let mut s = Settings::default();
        let feed: DataFeed = serde_json::from_str(r#"{"pilots":[{"cid":5,"callsign":"X","flight_plan":{"departure":"EGLL","arrival":"ENBR"}}]}"#).unwrap();
        assert_eq!(route(&s, None, Some(&feed)), Route::default());
        s.vatsim.cid = Some(5);
        assert_eq!(route(&s, None, Some(&feed)).arrival.as_deref(), Some("ENBR"));
        let plan = Plan { origin: "EGKK".into(), destination: "LFPG".into(), ..Default::default() };
        assert_eq!(route(&s, Some(&plan), Some(&feed)).source, "SimBrief");
        s.vatsim.departure = "lemd".into();
        let r = route(&s, Some(&plan), Some(&feed));
        assert_eq!((r.departure.as_deref(), r.arrival, r.source), (Some("LEMD"), None, "manual"));
    }
}
