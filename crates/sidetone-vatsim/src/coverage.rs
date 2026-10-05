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

/// Online airport stations in the order a pilot contacts them, without duplicates.
fn online_at_airport(icao: &str, stations: &[Station], order: &[Facility]) -> Vec<(String, String, i32)> {
    let mut found: Vec<&Station> = stations.iter().filter(|s| order.contains(&s.facility) && s.callsign.split('_').next() == Some(icao)).collect();
    found.sort_by_key(|s| (order.iter().position(|f| *f == s.facility), s.frequency_khz));
    dedupe(found)
}

/// One entry per (spoken name, frequency): split positions on one frequency show once.
fn dedupe(stations: Vec<&Station>) -> Vec<(String, String, i32)> {
    let mut out: Vec<(String, String, i32)> = Vec::new();
    for s in stations {
        if !out.iter().any(|(_, name, khz)| *name == s.name && *khz == s.frequency_khz) {
            out.push((s.callsign.clone(), s.name.clone(), s.frequency_khz));
        }
    }
    out
}

/// "Kobenhavn (East)" → "Kobenhavn", "Malmo ACC - Sweden" → "Malmo".
fn clean_label(name: &str) -> String {
    let name = name.split('(').next().unwrap_or(name);
    let name = name.split(" - ").next().unwrap_or(name).trim();
    let name = name.trim_end_matches(" ACC").trim_end_matches(" FIR").trim_end_matches(" CTA").trim_end_matches(" UIR");
    name.trim().to_string()
}

const DEPARTURE_ORDER: [Facility; 6] = [Facility::Delivery, Facility::Ground, Facility::Apron, Facility::Tower, Facility::Departure, Facility::Approach];
const ARRIVAL_ORDER: [Facility; 4] = [Facility::Approach, Facility::Tower, Facility::Ground, Facility::Apron];

/// The airspaces a route passes through in order, with who's online in each. Within a FIR,
/// only sectors the route actually crosses are listed (falling back to the whole FIR when the
/// online stations don't map to sectors).
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
        legs.push(RouteLeg { id: dep.to_string(), label: format!("{dep} departure"), online: online_at_airport(dep, stations, &DEPARTURE_ORDER) });
    }

    // Walk the route, grouping consecutive points by FIR and remembering every sector crossed.
    struct Crossing {
        fir: String,
        oceanic: bool,
        sectors: Vec<String>,
    }
    let mut crossings: Vec<Crossing> = Vec::new();
    for p in densify(route, 20.0) {
        let here: Vec<&crate::boundaries::Boundary> = boundaries.containing(p).collect();
        let Some(primary) = here.iter().find(|b| b.id.contains('-')).or(here.first()) else { continue };
        let fir = primary.id.split('-').next().unwrap_or(&primary.id).to_string();
        if crossings.last().is_none_or(|c| c.fir != fir) {
            crossings.push(Crossing { fir: fir.clone(), oceanic: primary.oceanic, sectors: Vec::new() });
        }
        let current = crossings.last_mut().expect("just pushed");
        for b in here {
            if !current.sectors.contains(&b.id) {
                current.sectors.push(b.id.clone());
            }
        }
    }

    for c in crossings {
        let in_fir: Vec<(&Station, &str)> = stations
            .iter()
            .filter(|s| matches!(s.facility, Facility::Center | Facility::Fss))
            .filter_map(|s| station_boundary(s, vatspy).map(|sb| (s, sb)))
            .filter(|(_, sb)| sb.split('-').next() == Some(c.fir.as_str()))
            .collect();
        let crossed: Vec<&Station> = in_fir.iter().filter(|(_, sb)| c.sectors.iter().any(|id| covers_boundary(sb, id))).map(|(s, _)| *s).collect();
        let chosen = if crossed.is_empty() { in_fir.iter().map(|(s, _)| *s).collect() } else { crossed };
        let mut chosen = chosen;
        chosen.sort_by_key(|s| s.frequency_khz);
        let label = vatspy.firs.iter().find(|f| f.icao == c.fir).map(|f| clean_label(&f.name)).unwrap_or_else(|| c.fir.clone());
        legs.push(RouteLeg { id: c.fir, label: format!("{label}{}", if c.oceanic { " (oceanic)" } else { "" }), online: dedupe(chosen) });
    }

    if let Some(arr) = arrival {
        legs.push(RouteLeg { id: arr.to_string(), label: format!("{arr} arrival"), online: online_at_airport(arr, stations, &ARRIVAL_ORDER) });
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

    fn station(callsign: &str, name: &str, khz: i32) -> Station {
        Station {
            callsign: callsign.into(),
            name: name.into(),
            facility: Facility::from_callsign(callsign),
            frequency_khz: khz,
            frequencies_hz: vec![],
            positions: vec![],
            controller: String::new(),
            cid: 0,
            rating: 0,
            text: vec![],
            atis_code: None,
        }
    }

    #[test]
    fn airport_stations_in_contact_order_without_duplicates() {
        let s = vec![
            station("EHAM_APP", "Schiphol Approach", 121_205),
            station("EHAM_TWR", "Schiphol Tower", 119_230),
            station("EHAM_DEL", "Schiphol Delivery", 121_980),
            station("EHAM_S_DEL", "Schiphol Delivery", 121_980),
        ];
        let names: Vec<_> = online_at_airport("EHAM", &s, &DEPARTURE_ORDER).into_iter().map(|o| o.1).collect();
        assert_eq!(names, vec!["Schiphol Delivery", "Schiphol Tower", "Schiphol Approach"]);
        let arr: Vec<_> = online_at_airport("EHAM", &s, &ARRIVAL_ORDER).into_iter().map(|o| o.1).collect();
        assert_eq!(arr, vec!["Schiphol Approach", "Schiphol Tower"]);
    }

    #[test]
    fn labels_are_tidy() {
        assert_eq!(clean_label("Malmo ACC - Sweden"), "Malmo");
        assert_eq!(clean_label("Kobenhavn (East)"), "Kobenhavn");
        assert_eq!(clean_label("London"), "London");
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
