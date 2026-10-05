//! Joins the data feed, transceivers and VATSpy into named, tunable stations.

use crate::feed::{Controller, DataFeed, TransceiverEntry};
use crate::freq::{channel_to_hz, is_placeholder, parse_mhz, same_frequency};
use crate::geo::{LatLon, distance_nm};
use crate::naming::{Facility, station_name};
use crate::vatspy::VatSpy;
use std::collections::HashMap;

#[derive(Clone, Debug)]
pub struct Station {
    pub callsign: String,
    /// Spoken name, e.g. "Heathrow Tower".
    pub name: String,
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
}

impl Station {
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
    Some(Station {
        name: station_name(&c.callsign, c.text_atis.as_deref(), vatspy),
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
    })
}

/// All tunable controller and ATIS stations, sorted by facility then callsign.
pub fn build(feed: &DataFeed, transceivers: &[TransceiverEntry], vatspy: Option<&VatSpy>) -> Vec<Station> {
    let by_callsign: HashMap<&str, &TransceiverEntry> = transceivers.iter().map(|t| (t.callsign.as_str(), t)).collect();
    let mut out: Vec<Station> = feed.controllers.iter().chain(&feed.atis).filter_map(|c| station(c, &by_callsign, vatspy)).collect();
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
    fn horizon() {
        assert!((radio_horizon_nm(10_000.0) - 135.3).abs() < 0.5);
    }
}
