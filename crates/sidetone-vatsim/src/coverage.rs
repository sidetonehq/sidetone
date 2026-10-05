//! Which online ATC covers a position, and which units you'll meet along a route.

use crate::boundaries::Boundaries;
use crate::geo::{LatLon, distance_nm};
use crate::naming::Facility;
use crate::stations::Station;
use crate::vatspy::VatSpy;

/// Rough service radius around the airport for terminal facilities, in nm.
fn terminal_radius(f: Facility) -> Option<f64> {
    match f {
        Facility::Approach | Facility::Departure => Some(40.0),
        Facility::Tower => Some(10.0),
        Facility::Ground | Facility::Apron | Facility::Delivery => Some(3.0),
        _ => None,
    }
}

/// Boundary id an area station works, via its VATSpy FIR entry.
fn station_boundary<'a>(s: &Station, vatspy: &'a VatSpy) -> Option<&'a str> {
    let prefix = s.callsign.rsplit_once('_').map(|(p, _)| p)?;
    vatspy.fir_for_prefix(prefix).map(|f| f.boundary.as_str())
}

/// A whole-FIR station ("EGTT") covers its sectors ("EGTT-S") too.
fn covers_boundary(station_boundary: &str, id: &str) -> bool {
    station_boundary == id || (!station_boundary.contains('-') && id.split('-').next() == Some(station_boundary))
}

/// Online stations (excluding ATIS) whose airspace contains `pos`, most local first.
pub fn covering<'a>(pos: LatLon, on_ground: bool, stations: &'a [Station], vatspy: &VatSpy, boundaries: &Boundaries) -> Vec<&'a Station> {
    let here: Vec<&str> = boundaries.containing(pos).map(|b| b.id.as_str()).collect();
    let mut out: Vec<(&Station, f64)> = Vec::new();
    for s in stations {
        if let Some(radius) = terminal_radius(s.facility) {
            let ground_only = matches!(s.facility, Facility::Ground | Facility::Apron | Facility::Delivery);
            if ground_only && !on_ground {
                continue;
            }
            let airport = s.callsign.split('_').next().and_then(|a| vatspy.airport(a));
            if let Some(d) = airport.map(|a| distance_nm(pos, a.position))
                && d <= radius
            {
                out.push((s, d));
            }
        } else if matches!(s.facility, Facility::Center | Facility::Fss)
            && let Some(b) = station_boundary(s, vatspy)
            && here.iter().any(|id| covers_boundary(b, id))
        {
            // Sector stations beat whole-FIR ones.
            out.push((s, if b.contains('-') { 1_000.0 } else { 2_000.0 }));
        }
    }
    out.sort_by(|a, b| a.0.facility.cmp(&b.0.facility).reverse().then(a.1.total_cmp(&b.1)));
    out.into_iter().map(|(s, _)| s).collect()
}

/// One airspace along the route.
#[derive(Clone, Debug, PartialEq)]
pub struct RouteLeg {
    /// Boundary id ("EGTT") or airport ICAO for the ends.
    pub id: String,
    pub label: String,
    /// (callsign, spoken name, frequency kHz) of online stations.
    pub online: Vec<(String, String, i32)>,
}

/// Splits a polyline into points no more than `step_nm` apart.
fn densify(points: &[LatLon], step_nm: f64) -> Vec<LatLon> {
    let mut out = Vec::new();
    for pair in points.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        let n = (distance_nm(a, b) / step_nm).ceil().max(1.0) as usize;
        for i in 0..n {
            let t = i as f64 / n as f64;
            out.push(LatLon { lat: a.lat + (b.lat - a.lat) * t, lon: a.lon + (b.lon - a.lon) * t });
        }
    }
    out.extend(points.last().copied());
    out
}

fn online_at_airport(icao: &str, stations: &[Station], facilities: &[Facility]) -> Vec<(String, String, i32)> {
    stations
        .iter()
        .filter(|s| facilities.contains(&s.facility) && s.callsign.split('_').next() == Some(icao))
        .map(|s| (s.callsign.clone(), s.name.clone(), s.frequency_khz))
        .collect()
}

/// The airspaces a route passes through in order, with who's online in each.
pub fn along_route(
    departure: Option<&str>,
    arrival: Option<&str>,
    route: &[LatLon],
    stations: &[Station],
    vatspy: &VatSpy,
    boundaries: &Boundaries,
) -> Vec<RouteLeg> {
    let mut legs = Vec::new();
    if let Some(dep) = departure {
        legs.push(RouteLeg {
            id: dep.to_string(),
            label: format!("{dep} departure"),
            online: online_at_airport(
                dep,
                stations,
                &[Facility::Delivery, Facility::Ground, Facility::Apron, Facility::Tower, Facility::Departure, Facility::Approach],
            ),
        });
    }

    let mut seen: Vec<String> = Vec::new();
    for p in densify(route, 20.0) {
        // Prefer the most specific (sector) boundary at each point.
        let mut here: Vec<&crate::boundaries::Boundary> = boundaries.containing(p).collect();
        here.sort_by_key(|b| std::cmp::Reverse(b.id.contains('-')));
        let Some(b) = here.first() else { continue };
        let fir = b.id.split('-').next().unwrap_or(&b.id).to_string();
        if seen.last() == Some(&fir) {
            continue;
        }
        seen.push(fir.clone());
        let label =
            vatspy.firs.iter().find(|f| f.icao == fir).map(|f| f.name.split('(').next().unwrap_or(&f.name).trim().to_string()).unwrap_or_else(|| fir.clone());
        let online = stations
            .iter()
            .filter(|s| matches!(s.facility, Facility::Center | Facility::Fss))
            .filter(|s| station_boundary(s, vatspy).is_some_and(|sb| sb == fir || sb.split('-').next() == Some(fir.as_str())))
            .map(|s| (s.callsign.clone(), s.name.clone(), s.frequency_khz))
            .collect();
        legs.push(RouteLeg { id: fir.clone(), label: format!("{label}{}", if b.oceanic { " (oceanic)" } else { "" }), online });
    }

    if let Some(arr) = arrival {
        legs.push(RouteLeg {
            id: arr.to_string(),
            label: format!("{arr} arrival"),
            online: online_at_airport(arr, stations, &[Facility::Approach, Facility::Tower, Facility::Ground, Facility::Apron]),
        });
    }
    legs
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::feed::{DataFeed, TransceiverEntry};

    fn setup() -> (Vec<Station>, VatSpy, Boundaries) {
        let feed: DataFeed = serde_json::from_str(include_str!("../tests/fixtures/vatsim-data.json")).unwrap();
        let tx: Vec<TransceiverEntry> = serde_json::from_str(include_str!("../tests/fixtures/transceivers-data.json")).unwrap();
        let mut spy_text = crate::vatspy::SAMPLE.to_string();
        spy_text = spy_text.replace("[FIRs]", "[FIRs]\nENSV|Polaris|ENOS|ENSV");
        let spy = VatSpy::parse(&spy_text);
        let stations = crate::stations::build(&feed, &tx, Some(&spy));
        (stations, spy, Boundaries::parse(crate::boundaries::SAMPLE).unwrap())
    }

    #[test]
    fn covering_prefers_local_units() {
        let (stations, spy, b) = setup();
        let at_heathrow = covering(LatLon { lat: 51.47, lon: -0.45 }, true, &stations, &spy, &b);
        let names: Vec<_> = at_heathrow.iter().map(|s| s.callsign.as_str()).collect();
        assert_eq!(names, vec!["EGLL_TWR", "LON_S_CTR"]);
        let enroute = covering(LatLon { lat: 53.5, lon: -1.0 }, false, &stations, &spy, &b);
        assert!(enroute.is_empty(), "LON_S covers only EGTT-S");
    }

    #[test]
    fn route_legs_in_order() {
        let (stations, spy, b) = setup();
        let route = [LatLon { lat: 51.47, lon: -0.45 }, LatLon { lat: 60.29, lon: 5.22 }];
        let legs = along_route(Some("EGLL"), Some("ENBR"), &route, &stations, &spy, &b);
        let ids: Vec<_> = legs.iter().map(|l| l.id.as_str()).collect();
        assert_eq!(ids, vec!["EGLL", "EGTT", "ENSV", "ENBR"]);
        assert_eq!(legs[0].online[0].1, "Heathrow Tower");
        assert_eq!(legs[1].online[0].0, "LON_S_CTR");
        assert_eq!(legs[3].online[0].1, "Flesland Tower");
    }
}
