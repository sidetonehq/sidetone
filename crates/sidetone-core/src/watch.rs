//! Turns successive VATSIM snapshots into pilot-facing notices: ATIS letter changes, METAR
//! changes and friends coming online. The first observation of anything never alerts.

use crate::settings::{Friend, Settings};
use crate::state::Route;
use sidetone_services::simbrief::Plan;
use sidetone_vatsim::feed::DataFeed;
use sidetone_vatsim::stations::Station;
use std::collections::{HashMap, HashSet};

#[derive(Default)]
pub struct Watcher {
    atis_codes: HashMap<String, String>,
    metars: HashMap<String, String>,
    friends_online: Option<HashSet<String>>,
    events_announced: HashSet<u64>,
}

/// Where a friend is right now.
#[derive(Clone, Debug, PartialEq)]
pub struct FriendStatus {
    pub key: String,
    pub label: String,
    pub online_as: Option<String>,
    /// e.g. "A320 EGLL → LEMD" or "Heathrow Tower 118.500".
    pub detail: String,
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

    /// Friends' current status, plus notices for anyone who just connected.
    pub fn friends(&mut self, feed: &DataFeed, stations: &[Station], friends: &[Friend]) -> (Vec<FriendStatus>, Vec<String>) {
        let statuses: Vec<FriendStatus> = friends.iter().map(|f| status(feed, stations, f)).collect();
        let online: HashSet<String> = statuses.iter().filter(|s| s.online_as.is_some()).map(|s| s.key.clone()).collect();
        let notices = match &self.friends_online {
            Some(before) => statuses
                .iter()
                .filter(|s| s.online_as.is_some() && !before.contains(&s.key))
                .map(|s| format!("{} is online as {}", s.label, s.online_as.as_deref().unwrap_or_default()))
                .collect(),
            None => Vec::new(),
        };
        self.friends_online = Some(online);
        (statuses, notices)
    }
}

pub fn friend_key(f: &Friend) -> String {
    match (&f.cid, &f.callsign) {
        (Some(cid), _) => cid.to_string(),
        (None, Some(cs)) => cs.to_ascii_uppercase(),
        (None, None) => String::new(),
    }
}

fn status(feed: &DataFeed, stations: &[Station], f: &Friend) -> FriendStatus {
    let key = friend_key(f);
    let label = if f.label.is_empty() { key.clone() } else { f.label.clone() };
    let matches = |cid: u32, callsign: &str| f.cid == Some(cid) || f.callsign.as_deref().is_some_and(|c| c.eq_ignore_ascii_case(callsign));
    if let Some(p) = feed.pilots.iter().find(|p| matches(p.cid, &p.callsign)) {
        let detail = match &p.flight_plan {
            Some(fp) => format!("{} {} → {}", fp.aircraft_short, fp.departure, fp.arrival),
            None => "No flight plan".into(),
        };
        return FriendStatus { key, label, online_as: Some(p.callsign.clone()), detail };
    }
    if let Some(s) = stations.iter().find(|s| matches(s.cid, &s.callsign)) {
        let detail = format!("{} {}", s.name, crate::radio::format_com_khz(s.frequency_khz));
        return FriendStatus { key, label, online_as: Some(s.callsign.clone()), detail };
    }
    FriendStatus { key, label, online_as: None, detail: String::new() }
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
    use sidetone_vatsim::feed::{Controller, Pilot};
    use sidetone_vatsim::naming::Facility;

    fn atis(code: &str) -> Station {
        Station {
            callsign: "EGLL_ATIS".into(),
            name: "Heathrow Information".into(),
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
    fn friends_coming_online() {
        let mut w = Watcher::default();
        let friends =
            vec![Friend { cid: Some(42), callsign: None, label: "Sam".into() }, Friend { cid: None, callsign: Some("egll_twr".into()), label: String::new() }];
        let empty = DataFeed::default();
        let (statuses, notices) = w.friends(&empty, &[], &friends);
        assert!(notices.is_empty());
        assert!(statuses.iter().all(|s| s.online_as.is_none()));

        let mut feed = DataFeed::default();
        feed.pilots.push(Pilot { cid: 42, callsign: "BAW1".into(), ..Default::default() });
        feed.controllers.push(Controller { cid: 7, callsign: "EGLL_TWR".into(), frequency: "118.500".into(), facility: 4, ..Default::default() });
        let mut twr = atis("A");
        twr.callsign = "EGLL_TWR".into();
        twr.atis_code = None;
        let (statuses, notices) = w.friends(&feed, &[twr], &friends);
        assert_eq!(notices, vec!["Sam is online as BAW1", "EGLL_TWR is online as EGLL_TWR"]);
        assert_eq!(statuses[0].online_as.as_deref(), Some("BAW1"));
        let (_, notices) = w.friends(&feed, &[], &friends[..1]);
        assert!(notices.is_empty());
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
