//! Joins the data feed, transceivers and VATSpy into named, tunable stations.

use crate::feed::{Controller, DataFeed, TransceiverEntry};
use crate::freq::{channel_to_hz, is_placeholder, parse_mhz, same_frequency};
use crate::geo::{LatLon, distance_nm};
use crate::naming::{Facility, NameSource, station_name_with_source};
use crate::vatspy::VatSpy;
use std::collections::HashMap;

#[derive(Clone, Debug)]
pub struct Station {
    pub callsign: String,
    /// Spoken name, e.g. "Heathrow Tower".
    pub name: String,
    /// Where `name` came from (shown on hover).
    pub name_source: NameSource,
    pub facility: Facility,
    /// Primary frequency as the channel name in kHz (what you dial).
    pub frequency_khz: i32,
    /// Every frequency this station transmits on, in Hz (primary + transceivers).
    pub frequencies_hz: Vec<i64>,
    /// Transmitter sites, or the airport position when no transceivers are known.
    pub positions: Vec<LatLon>,
    pub controller: String,
    pub cid: u32,
    pub rating: i32,
    pub text: Vec<String>,
    pub atis_code: Option<String>,
    /// Where this station is ("Dusseldorf APP", "LON_S"), set only when another online station
    /// shares its spoken name on a different frequency, as every DFS radar unit is "Langen Radar".
    pub qualifier: Option<String>,
}

impl Station {
    /// The spoken name, plus where it is when the name alone is ambiguous:
    /// "Langen Radar · Dusseldorf APP".
    pub fn display_name(&self) -> String {
        match &self.qualifier {
            Some(q) => format!("{} · {q}", self.name),
            None => self.name.clone(),
        }
    }

    pub fn distance_nm(&self, from: LatLon) -> Option<f64> {
        self.positions.iter().map(|p| distance_nm(from, *p)).min_by(f64::total_cmp)
    }

    pub fn transmits_on(&self, hz: i64) -> bool {
        self.frequencies_hz.iter().any(|f| same_frequency(*f, hz))
    }
}

fn station(c: &Controller, transceivers: &HashMap<&str, &TransceiverEntry>, vatspy: Option<&VatSpy>) -> Option<Station> {
    let khz = parse_mhz(&c.frequency)?;
    if is_placeholder(khz) || c.facility == 0 {
        return None;
    }
    let facility = Facility::from_callsign(&c.callsign);
    let mut frequencies_hz = vec![channel_to_hz(khz)];
    let mut positions = Vec::new();
    if let Some(entry) = transceivers.get(c.callsign.as_str()) {
        for t in &entry.transceivers {
            if !frequencies_hz.iter().any(|f| same_frequency(*f, t.frequency)) {
                frequencies_hz.push(t.frequency);
            }
            positions.push(LatLon { lat: t.lat_deg, lon: t.lon_deg });
        }
    }
    if positions.is_empty()
        && let Some(v) = vatspy
    {
        let prefix = c.callsign.split('_').next().unwrap_or_default();
        if let Some(airport) = v.airport(prefix) {
            positions.push(airport.position);
        }
    }
    let (name, name_source) = station_name_with_source(&c.callsign, c.text_atis.as_deref(), vatspy);
    Some(Station {
        name,
        name_source,
        callsign: c.callsign.clone(),
        facility,
        frequency_khz: khz,
        frequencies_hz,
        positions,
        controller: c.name.clone(),
        cid: c.cid,
        rating: c.rating,
        text: c.text_atis.clone().unwrap_or_default(),
        atis_code: c.atis_code.clone(),
        qualifier: None,
    })
}

/// The callsign without its facility suffix: "EDDF_N_APP" → "EDDF_N".
fn callsign_stem(callsign: &str) -> &str {
    callsign.rsplit_once('_').map(|(stem, _)| stem.trim_end_matches('_')).unwrap_or(callsign)
}

/// "Dusseldorf APP" for airport units (from VATSpy), else the callsign stem ("LON_S").
fn place(s: &Station, vatspy: Option<&VatSpy>) -> String {
    let airport_unit =
        matches!(s.facility, Facility::Approach | Facility::Departure | Facility::Tower | Facility::Ground | Facility::Apron | Facility::Delivery);
    let icao = s.callsign.split('_').next().unwrap_or_default();
    match vatspy.and_then(|v| v.airport_name(icao)).filter(|_| airport_unit) {
        Some(name) => {
            let short = [" International", " Intl", " Airport", " Airfield"].iter().fold(name, |n, suffix| n.replace(suffix, ""));
            format!("{short} {}", s.facility.short())
        }
        None => callsign_stem(&s.callsign).to_string(),
    }
}

/// Gives stations that share a spoken name on different frequencies a qualifier saying where
/// each one is. Split positions on one frequency stay unqualified (they're one voice).
fn qualify(stations: &mut [Station], vatspy: Option<&VatSpy>) {
    let mut by_name: HashMap<String, Vec<usize>> = HashMap::new();
    for (i, s) in stations.iter().enumerate() {
        by_name.entry(s.name.clone()).or_default().push(i);
    }
    for group in by_name.values() {
        let first_khz = stations[group[0]].frequency_khz;
        if group.iter().all(|&i| stations[i].frequency_khz == first_khz) {
            continue;
        }
        let places: Vec<String> = group.iter().map(|&i| place(&stations[i], vatspy)).collect();
        for (k, &i) in group.iter().enumerate() {
            // Two units at one airport ("Frankfurt APP" twice) fall back to their callsigns.
            let clash = group.iter().enumerate().any(|(j, &o)| j != k && places[j] == places[k] && stations[o].frequency_khz != stations[i].frequency_khz);
            stations[i].qualifier = Some(if clash { callsign_stem(&stations[i].callsign).to_string() } else { places[k].clone() });
        }
    }
}

/// All tunable controller and ATIS stations, sorted by facility then callsign.
pub fn build(feed: &DataFeed, transceivers: &[TransceiverEntry], vatspy: Option<&VatSpy>) -> Vec<Station> {
    let by_callsign: HashMap<&str, &TransceiverEntry> = transceivers.iter().map(|t| (t.callsign.as_str(), t)).collect();
    let mut out: Vec<Station> = feed.controllers.iter().chain(&feed.atis).filter_map(|c| station(c, &by_callsign, vatspy)).collect();
    qualify(&mut out, vatspy);
    out.sort_by(|a, b| a.facility.cmp(&b.facility).then_with(|| a.callsign.cmp(&b.callsign)));
    out
}

/// The station you'd hear on `khz`: of all stations transmitting on it, the nearest to `from`.
pub fn on_frequency(stations: &[Station], khz: i32, from: Option<LatLon>) -> Option<&Station> {
    if khz <= 0 {
        return None;
    }
    let hz = channel_to_hz(khz);
    let mut candidates = stations.iter().filter(|s| s.transmits_on(hz));
    match from {
        Some(pos) => candidates.min_by(|a, b| {
            let da = a.distance_nm(pos).unwrap_or(f64::MAX);
            let db = b.distance_nm(pos).unwrap_or(f64::MAX);
            da.total_cmp(&db)
        }),
        None => candidates.next(),
    }
}

/// The ATIS a departing pilot should have: the departure ATIS where an airport splits them
/// (`EHAM_D_ATIS`), else its main ATIS (`EHAM_ATIS`), else any ATIS there.
pub fn departure_atis<'a>(stations: &'a [Station], icao: &str) -> Option<&'a Station> {
    let at_airport = |s: &&Station| s.atis_code.is_some() && s.callsign.split('_').next() == Some(icao);
    let kind = |s: &Station| s.callsign.split('_').collect::<Vec<_>>().len();
    stations.iter().filter(at_airport).min_by_key(|s| match s.callsign.split('_').nth(1) {
        Some("D") if kind(s) == 3 => 0,
        Some("ATIS") => 1,
        Some("A") if kind(s) == 3 => 3,
        _ => 2,
    })
}

/// The ATIS an arriving pilot should have: the arrival ATIS where an airport splits them
/// (`EHAM_A_ATIS`), else its main ATIS, else any ATIS there.
pub fn arrival_atis<'a>(stations: &'a [Station], icao: &str) -> Option<&'a Station> {
    let at_airport = |s: &&Station| s.atis_code.is_some() && s.callsign.split('_').next() == Some(icao);
    let kind = |s: &Station| s.callsign.split('_').count();
    stations.iter().filter(at_airport).min_by_key(|s| match s.callsign.split('_').nth(1) {
        Some("A") if kind(s) == 3 => 0,
        Some("ATIS") => 1,
        Some("D") if kind(s) == 3 => 3,
        _ => 2,
    })
}

/// Line-of-sight VHF range in nm between an aircraft at `altitude_ft` and a ground station.
pub fn radio_horizon_nm(altitude_ft: f64) -> f64 {
    1.23 * (altitude_ft.max(0.0).sqrt() + 100f64.sqrt())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> Vec<Station> {
        let feed: DataFeed = serde_json::from_str(include_str!("../tests/fixtures/vatsim-data.json")).unwrap();
        let tx: Vec<TransceiverEntry> = serde_json::from_str(include_str!("../tests/fixtures/transceivers-data.json")).unwrap();
        let spy = VatSpy::parse(crate::vatspy::SAMPLE);
        build(&feed, &tx, Some(&spy))
    }

    #[test]
    fn builds_named_stations_without_observers() {
        let s = fixture();
        assert_eq!(s.len(), 4);
        assert!(s.iter().all(|s| s.callsign != "BS_OBS"));
        let twr = s.iter().find(|s| s.callsign == "EGLL_TWR").unwrap();
        assert_eq!(twr.name, "Heathrow Tower");
        assert_eq!(s[0].facility, Facility::Center);
    }

    #[test]
    fn matches_tuned_frequency_including_833_channels() {
        let s = fixture();
        let heathrow = LatLon { lat: 51.47, lon: -0.45 };
        assert_eq!(on_frequency(&s, 118_700, Some(heathrow)).unwrap().name, "Heathrow Tower");
        // 8.33 channel name dialled as 118.705 is the same frequency.
        assert_eq!(on_frequency(&s, 118_705, Some(heathrow)).unwrap().callsign, "EGLL_TWR");
        assert_eq!(on_frequency(&s, 119_105, None).unwrap().name, "Flesland Tower");
        assert!(on_frequency(&s, 121_500, None).is_none());
    }

    #[test]
    fn picks_the_departure_atis() {
        let mut s = fixture();
        let atis = |callsign: &str, code: &str| {
            let mut a = s.iter().find(|x| x.callsign == "ENBR_ATIS").unwrap().clone();
            a.callsign = callsign.into();
            a.atis_code = Some(code.into());
            a
        };
        assert_eq!(departure_atis(&s, "ENBR").unwrap().atis_code.as_deref(), Some("C"));
        let (arr, dep) = (atis("EHAM_A_ATIS", "E"), atis("EHAM_D_ATIS", "R"));
        s.push(arr);
        s.push(dep);
        assert_eq!(departure_atis(&s, "EHAM").unwrap().atis_code.as_deref(), Some("R"));
        assert_eq!(arrival_atis(&s, "EHAM").unwrap().atis_code.as_deref(), Some("E"));
        assert!(departure_atis(&s, "EGLL").is_none(), "EGLL_TWR is not an ATIS");
    }

    #[test]
    fn shared_names_get_a_place() {
        let spy = VatSpy::parse(crate::vatspy::SAMPLE);
        let template = fixture().into_iter().find(|s| s.callsign == "EGLL_TWR").unwrap();
        let unit = |callsign: &str, name: &str, khz: i32| Station {
            callsign: callsign.into(),
            name: name.into(),
            facility: Facility::from_callsign(callsign),
            frequency_khz: khz,
            ..template.clone()
        };
        let mut s = vec![
            unit("EDDF_APP", "Langen Radar", 120_805),
            unit("EGLL_APP", "Langen Radar", 119_730),
            unit("EDDF_N_DEP", "Frankfurt Departure", 120_150),
            unit("EDDF_S_DEP", "Frankfurt Departure", 136_130),
            unit("EHAM_DEL", "Schiphol Delivery", 121_980),
            unit("EHAM_S_DEL", "Schiphol Delivery", 121_980),
        ];
        qualify(&mut s, Some(&spy));
        assert_eq!(s[0].display_name(), "Langen Radar · Frankfurt APP");
        assert_eq!(s[1].display_name(), "Langen Radar · London Heathrow APP");
        assert_eq!(s[2].qualifier.as_deref(), Some("EDDF_N"), "same airport and unit: callsign");
        assert_eq!(s[3].qualifier.as_deref(), Some("EDDF_S"));
        assert_eq!(s[4].qualifier, None, "split positions on one frequency are one voice");
        assert_eq!(s[5].display_name(), "Schiphol Delivery");
    }

    #[test]
    fn horizon() {
        assert!((radio_horizon_nm(10_000.0) - 135.3).abs() < 0.5);
    }
}
