//! Which online ATC covers a position, and which units you'll meet along a route.

use crate::boundaries::Boundaries;
use crate::geo::{LatLon, distance_nm};
use crate::naming::Facility;
use crate::sectors::Sectors;
use crate::stations::Station;
use crate::tracon::Tracons;
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

/// Whether a terminal station (approach, departure, tower, ground) works `at`, and how far its
/// airport is: inside its real airspace when the SimAware TRACON Project has it, else within a
/// rough radius of the airport.
fn terminal_covers(s: &Station, at: LatLon, vatspy: &VatSpy, tracons: Option<&Tracons>) -> Option<f64> {
    let airport = s.callsign.split('_').next().and_then(|a| vatspy.airport(a)).map(|a| a.position);
    if let Some(shape) = tracons.and_then(|t| t.for_callsign(&s.callsign)) {
        return shape.contains(at).then(|| airport.map_or(0.0, |a| distance_nm(at, a)));
    }
    let d = distance_nm(at, airport?);
    (d <= terminal_radius(s.facility)?).then_some(d)
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
pub fn covering<'a>(
    pos: LatLon,
    on_ground: bool,
    stations: &'a [Station],
    vatspy: &VatSpy,
    boundaries: &Boundaries,
    tracons: Option<&Tracons>,
) -> Vec<&'a Station> {
    let here: Vec<&str> = boundaries.containing(pos).map(|b| b.id.as_str()).collect();
    let mut out: Vec<(&Station, f64)> = Vec::new();
    for s in stations {
        if terminal_radius(s.facility).is_some() {
            let ground_only = matches!(s.facility, Facility::Ground | Facility::Apron | Facility::Delivery);
            if ground_only && !on_ground {
                continue;
            }
            if let Some(d) = terminal_covers(s, pos, vatspy, tracons) {
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

/// Puts the station that owns your airspace at your level first (VATGlasses), keeping the
/// others that cover you after it. Stacked sectors share a FIR on the map; only the levels
/// tell them apart.
pub fn owner_first<'a>(mut covering: Vec<&'a Station>, owner: Option<&'a Station>) -> Vec<&'a Station> {
    if let Some(owner) = owner {
        covering.retain(|s| s.callsign != owner.callsign);
        covering.insert(0, owner);
    }
    covering
}

/// The airspace detail beyond VATSpy's map: approach shapes (SimAware), sector levels and
/// owners (VATGlasses), each when loaded, and the planned cruise altitude.
#[derive(Clone, Copy, Default)]
pub struct Profile<'a> {
    pub tracons: Option<&'a Tracons>,
    pub sectors: Option<&'a Sectors>,
    pub cruise_ft: Option<u32>,
}

/// Roughly how high you'll be `from_dep` and `to_arr` nm from the ends: climbing and
/// descending at about 300 ft per nm (a 3° path), cruise in between.
fn planned_ft(cruise_ft: u32, from_dep: f64, to_arr: f64) -> f64 {
    (from_dep.min(to_arr) * 300.0 + 1_500.0).min(f64::from(cruise_ft))
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

/// An airport's row on the route: its own units in contact order, and, when it has no approach
/// or departure unit of its own online, the approach units whose area covers it (nearest
/// first). That's who works its traffic top-down, and who the "first call" hint suggests.
fn airport_leg(icao: &str, stations: &[Station], vatspy: &VatSpy, tracons: Option<&Tracons>, order: &[Facility]) -> Vec<(String, String, i32)> {
    let terminal = |s: &Station| matches!(s.facility, Facility::Approach | Facility::Departure);
    let own = |s: &Station| s.callsign.split('_').next() == Some(icao);
    let mut found: Vec<&Station> = stations.iter().filter(|s| order.contains(&s.facility) && own(s)).collect();
    found.sort_by_key(|s| (order.iter().position(|f| *f == s.facility), s.frequency_khz));
    if !found.iter().any(|s| terminal(s))
        && let Some(here) = vatspy.airport(icao).map(|a| a.position)
    {
        let mut nearby: Vec<(&Station, f64)> =
            stations.iter().filter(|s| terminal(s) && !own(s)).filter_map(|s| terminal_covers(s, here, vatspy, tracons).map(|d| (s, d))).collect();
        nearby.sort_by(|a, b| a.1.total_cmp(&b.1));
        found.extend(nearby.into_iter().map(|(s, _)| s));
    }
    dedupe(found)
}

/// One entry per (name, frequency): split positions on one frequency show once. Names carry
/// their place when ambiguous ("Langen Radar · Dusseldorf APP").
fn dedupe(stations: Vec<&Station>) -> Vec<(String, String, i32)> {
    let mut out: Vec<(String, String, i32)> = Vec::new();
    for s in stations {
        let name = s.display_name();
        if !out.iter().any(|(_, n, khz)| *n == name && *khz == s.frequency_khz) {
            out.push((s.callsign.clone(), name, s.frequency_khz));
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
    profile: Profile,
) -> Vec<RouteLeg> {
    let tracons = profile.tracons;
    let mut legs = Vec::new();
    if let Some(dep) = departure {
        legs.push(RouteLeg { id: dep.to_string(), label: format!("{dep} departure"), online: airport_leg(dep, stations, vatspy, tracons, &DEPARTURE_ORDER) });
    }

    // Walk the route, grouping consecutive points by FIR and remembering every sector crossed.
    struct Crossing {
        fir: String,
        oceanic: bool,
        sectors: Vec<String>,
        /// Who owns the airspace at the planned level, in the order you'll meet them.
        owners: Vec<String>,
    }
    let mut crossings: Vec<Crossing> = Vec::new();
    let (start, end) = (route.first().copied(), route.last().copied());
    for p in densify(route, 20.0) {
        let here: Vec<&crate::boundaries::Boundary> = boundaries.containing(p).collect();
        let Some(primary) = here.iter().find(|b| b.id.contains('-')).or(here.first()) else { continue };
        let fir = primary.id.split('-').next().unwrap_or(&primary.id).to_string();
        if crossings.last().is_none_or(|c| c.fir != fir) {
            crossings.push(Crossing { fir: fir.clone(), oceanic: primary.oceanic, sectors: Vec::new(), owners: Vec::new() });
        }
        let current = crossings.last_mut().expect("just pushed");
        if let (Some(sectors), Some(a), Some(b)) = (profile.sectors, start, end) {
            let ft = planned_ft(profile.cruise_ft.unwrap_or(35_000), distance_nm(a, p), distance_nm(p, b));
            if let Some(owner) = sectors.owner(p, ft / 100.0, stations)
                && !current.owners.contains(&owner.callsign)
            {
                current.owners.push(owner.callsign.clone());
            }
        }
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
        let mut chosen: Vec<&Station> = if crossed.is_empty() { in_fir.iter().map(|(s, _)| *s).collect() } else { crossed };
        // Whoever owns the airspace at your level comes first, in the order you'll meet them,
        // even when VATSpy's map doesn't put their sector on the route.
        let owners: Vec<&Station> = c.owners.iter().filter_map(|cs| in_fir.iter().map(|(s, _)| *s).find(|s| s.callsign == *cs)).collect();
        chosen.retain(|s| !owners.iter().any(|o| o.callsign == s.callsign));
        chosen.sort_by_key(|s| s.frequency_khz);
        let chosen: Vec<&Station> = owners.into_iter().chain(chosen).collect();
        let label = vatspy.firs.iter().find(|f| f.icao == c.fir).map(|f| clean_label(&f.name)).unwrap_or_else(|| c.fir.clone());
        legs.push(RouteLeg { id: c.fir, label: format!("{label}{}", if c.oceanic { " (oceanic)" } else { "" }), online: dedupe(chosen) });
    }

    if let Some(arr) = arrival {
        legs.push(RouteLeg { id: arr.to_string(), label: format!("{arr} arrival"), online: airport_leg(arr, stations, vatspy, tracons, &ARRIVAL_ORDER) });
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
        let at_heathrow = covering(LatLon { lat: 51.47, lon: -0.45 }, true, &stations, &spy, &b, None);
        let names: Vec<_> = at_heathrow.iter().map(|s| s.callsign.as_str()).collect();
        assert_eq!(names, vec!["EGLL_TWR", "LON_S_CTR"]);
        let enroute = covering(LatLon { lat: 53.5, lon: -1.0 }, false, &stations, &spy, &b, None);
        assert!(enroute.is_empty(), "LON_S covers only EGTT-S");
    }

    fn station(callsign: &str, name: &str, khz: i32) -> Station {
        Station {
            callsign: callsign.into(),
            name: name.into(),
            name_source: crate::naming::NameSource::VatSpy,
            facility: Facility::from_callsign(callsign),
            frequency_khz: khz,
            frequencies_hz: vec![],
            positions: vec![],
            controller: String::new(),
            cid: 0,
            rating: 0,
            text: vec![],
            atis_code: None,
            qualifier: None,
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
        let spy = VatSpy::parse(crate::vatspy::SAMPLE);
        let names: Vec<_> = airport_leg("EHAM", &s, &spy, None, &DEPARTURE_ORDER).into_iter().map(|o| o.1).collect();
        assert_eq!(names, vec!["Schiphol Delivery", "Schiphol Tower", "Schiphol Approach"]);
        let arr: Vec<_> = airport_leg("EHAM", &s, &spy, None, &ARRIVAL_ORDER).into_iter().map(|o| o.1).collect();
        assert_eq!(arr, vec!["Schiphol Approach", "Schiphol Tower"]);
    }

    #[test]
    fn nearby_approach_covers_an_airport_without_its_own() {
        let spy = VatSpy::parse(crate::vatspy::SAMPLE);
        let s = vec![station("EGLL_APP", "Heathrow Approach", 119_730), station("EGKK_TWR", "Gatwick Tower", 124_225)];
        // Gatwick has a tower but no approach: Heathrow's, 22 nm away, works its departures.
        let names: Vec<_> = airport_leg("EGKK", &s, &spy, None, &DEPARTURE_ORDER).into_iter().map(|o| o.1).collect();
        assert_eq!(names, vec!["Gatwick Tower", "Heathrow Approach"]);
        // An airport with its own approach online doesn't borrow a neighbour's.
        let with_own = vec![station("EGLL_APP", "Heathrow Approach", 119_730), station("EGKK_APP", "Gatwick Director", 126_825)];
        let names: Vec<_> = airport_leg("EGKK", &with_own, &spy, None, &DEPARTURE_ORDER).into_iter().map(|o| o.1).collect();
        assert_eq!(names, vec!["Gatwick Director"]);
        // Too far to cover: Frankfurt's approach doesn't work Heathrow.
        let far = vec![station("EDDF_APP", "Langen Radar", 120_805)];
        assert!(airport_leg("EGLL", &far, &spy, None, &DEPARTURE_ORDER).is_empty());
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
        let legs = along_route(Some("EGLL"), Some("ENBR"), &route, &stations, &spy, &b, Profile::default());
        let ids: Vec<_> = legs.iter().map(|l| l.id.as_str()).collect();
        assert_eq!(ids, vec!["EGLL", "EGTT", "ENSV", "ENBR"]);
        assert_eq!(legs[0].online[0].1, "Heathrow Tower");
        assert_eq!(legs[1].online[0].0, "LON_S_CTR");
        assert_eq!(legs[3].online[0].1, "Flesland Tower");
    }

    /// Heathrow's and Gatwick's approach airspace as boxes, apart from each other.
    fn london_tracons() -> Tracons {
        Tracons::parse(
            r#"{"type":"FeatureCollection","name":"t","crs":{"type":"name","properties":{"name":"x"}},"features":[
             {"type":"Feature","properties":{"id":"EGLL","prefix":["EGLL"],"name":"Heathrow Director"},"geometry":{"type":"Polygon","coordinates":[[[-0.8,51.35],[-0.1,51.35],[-0.1,51.65],[-0.8,51.65],[-0.8,51.35]]]}},
             {"type":"Feature","properties":{"id":"EGKK","prefix":["EGKK"],"name":"Gatwick Director"},"geometry":{"type":"Polygon","coordinates":[[[-0.6,50.9],[0.1,50.9],[0.1,51.3],[-0.6,51.3],[-0.6,50.9]]]}}
            ]}"#,
        )
        .unwrap()
    }

    #[test]
    fn approach_covers_its_real_airspace_not_a_circle() {
        let spy = VatSpy::parse(crate::vatspy::SAMPLE);
        let b = Boundaries::default();
        let s = vec![station("EGKK_APP", "Gatwick Director", 126_825)];
        let heathrow = LatLon { lat: 51.47, lon: -0.45 };
        // 22 nm from Gatwick: inside the old 40 nm circle, outside Gatwick's real airspace.
        assert_eq!(covering(heathrow, false, &s, &spy, &b, None).len(), 1, "the circle says yes");
        assert!(covering(heathrow, false, &s, &spy, &b, Some(&london_tracons())).is_empty(), "the real shape says no");
        let over_gatwick = LatLon { lat: 51.15, lon: -0.19 };
        assert_eq!(covering(over_gatwick, false, &s, &spy, &b, Some(&london_tracons())).len(), 1);
    }

    #[test]
    fn a_neighbouring_approach_only_where_its_airspace_reaches() {
        let spy = VatSpy::parse(crate::vatspy::SAMPLE);
        let s = vec![station("EGLL_APP", "Heathrow Approach", 119_730), station("EGKK_TWR", "Gatwick Tower", 124_225)];
        let with_shapes: Vec<_> = airport_leg("EGKK", &s, &spy, Some(&london_tracons()), &DEPARTURE_ORDER).into_iter().map(|o| o.1).collect();
        assert_eq!(with_shapes, vec!["Gatwick Tower"], "Heathrow's airspace doesn't reach Gatwick");
    }
}
