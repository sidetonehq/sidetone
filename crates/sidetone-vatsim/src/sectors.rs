//! Who owns the airspace at a position and level, from the VATGlasses data project
//! (github.com/lennycolton/vatglasses-data, CC BY-NC-SA 4.0, downloaded at runtime, never
//! bundled). Its sectors have floors and ceilings and an ordered list of positions that work
//! them: the first one online owns it. That's how stacked sectors ("Bremen Radar" low and high)
//! are told apart, which VATSpy's flat FIR shapes can't do.

use crate::boundaries::in_ring;
use crate::freq::parse_mhz;
use crate::geo::LatLon;
use crate::stations::Station;
use serde::Deserialize;
use std::collections::HashMap;

/// One position that can work airspace ("EDWW" CTR on 126.325).
#[derive(Clone, Debug)]
struct Position {
    /// Callsign prefixes ("EDWW", or "LON" for LON_S_CTR).
    prefixes: Vec<String>,
    /// Callsign suffix ("CTR", "APP", "TWR", "DEP").
    kind: String,
    frequency_khz: Option<i32>,
}

impl Position {
    fn matches(&self, s: &Station) -> bool {
        let mut parts = s.callsign.split('_');
        let prefix = parts.next().unwrap_or_default();
        let suffix = s.callsign.rsplit('_').next().unwrap_or_default();
        self.prefixes.iter().any(|p| p.eq_ignore_ascii_case(prefix)) && self.kind.eq_ignore_ascii_case(suffix) && self.frequency_khz == Some(s.frequency_khz)
    }
}

/// A block of airspace between two levels.
#[derive(Clone, Debug)]
struct Block {
    /// Flight levels (hundreds of feet); none for the surface or no ceiling.
    min: Option<f64>,
    max: Option<f64>,
    ring: Vec<(f64, f64)>,
    bbox: (f64, f64, f64, f64),
}

impl Block {
    fn contains(&self, at: LatLon, fl: f64) -> bool {
        let (min_lat, min_lon, max_lat, max_lon) = self.bbox;
        self.min.is_none_or(|m| fl >= m)
            && self.max.is_none_or(|m| fl < m)
            && (min_lat..=max_lat).contains(&at.lat)
            && (min_lon..=max_lon).contains(&at.lon)
            && in_ring(&self.ring, at)
    }
}

#[derive(Clone, Debug)]
struct Airspace {
    /// Positions in the order they take it: "EUCME" in another file is "fss/EUCME".
    owners: Vec<String>,
    blocks: Vec<Block>,
    /// Only with some runways in use (VATGlasses can't know which): a fallback.
    by_runway: bool,
}

#[derive(Clone, Debug, Default)]
struct File {
    positions: HashMap<String, Position>,
    airspace: Vec<Airspace>,
}

/// Every VATGlasses file loaded so far, by name ("ed", "fss").
#[derive(Clone, Debug, Default)]
pub struct Sectors {
    files: HashMap<String, File>,
}

#[derive(Deserialize)]
struct RawFile {
    #[serde(default)]
    airspace: Vec<RawAirspace>,
    #[serde(default)]
    positions: HashMap<String, RawPosition>,
}

#[derive(Deserialize)]
struct RawAirspace {
    #[serde(default)]
    owner: Vec<String>,
    #[serde(default)]
    sectors: Vec<RawSector>,
}

#[derive(Deserialize)]
struct RawSector {
    min: Option<f64>,
    max: Option<f64>,
    #[serde(default)]
    points: Vec<[String; 2]>,
    #[serde(default)]
    runways: Option<serde_json::Value>,
}

#[derive(Deserialize)]
struct RawPosition {
    #[serde(default)]
    pre: Vec<String>,
    #[serde(rename = "type", default)]
    kind: String,
    #[serde(default)]
    frequency: String,
}

/// "552201" / "-0013137": degrees, minutes and seconds run together.
fn dms(text: &str) -> Option<f64> {
    let (sign, digits) = match text.strip_prefix('-') {
        Some(rest) => (-1.0, rest),
        None => (1.0, text),
    };
    if digits.len() < 6 || !digits.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let (deg, rest) = digits.split_at(digits.len() - 4);
    let (min, sec) = rest.split_at(2);
    Some(sign * (deg.parse::<f64>().ok()? + min.parse::<f64>().ok()? / 60.0 + sec.parse::<f64>().ok()? / 3600.0))
}

impl Sectors {
    /// Adds one VATGlasses file ("ed" for ed.json).
    pub fn add(&mut self, name: &str, json: &str) -> Result<(), String> {
        let raw: RawFile = serde_json::from_str(json).map_err(|e| format!("VATGlasses {name}: {e}"))?;
        let positions =
            raw.positions.into_iter().map(|(id, p)| (id, Position { prefixes: p.pre, kind: p.kind, frequency_khz: parse_mhz(&p.frequency) })).collect();
        let airspace = raw
            .airspace
            .into_iter()
            .map(|a| {
                let by_runway = a.sectors.iter().any(|s| s.runways.is_some());
                let blocks = a
                    .sectors
                    .into_iter()
                    .filter_map(|s| {
                        let ring: Vec<(f64, f64)> = s.points.iter().filter_map(|[lat, lon]| Some((dms(lon)?, dms(lat)?))).collect();
                        if ring.len() < 3 {
                            return None;
                        }
                        let bbox = ring
                            .iter()
                            .fold((f64::MAX, f64::MAX, f64::MIN, f64::MIN), |(a, b, c, d), &(lon, lat)| (a.min(lat), b.min(lon), c.max(lat), d.max(lon)));
                        Some(Block { min: s.min, max: s.max, ring, bbox })
                    })
                    .collect();
                Airspace { owners: a.owner, blocks, by_runway }
            })
            .collect();
        self.files.insert(name.to_string(), File { positions, airspace });
        Ok(())
    }

    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }

    fn position(&self, file: &str, owner: &str) -> Option<&Position> {
        match owner.split_once('/') {
            Some((other, id)) => self.files.get(other)?.positions.get(id),
            None => self.files.get(file)?.positions.get(owner),
        }
    }

    /// The online station that owns the airspace at `at` and flight level `fl` (feet / 100):
    /// in each block there, the first of its owners that's online. Airspace that only applies
    /// with certain runways in use counts when nothing else does.
    pub fn owner<'a>(&self, at: LatLon, fl: f64, stations: &'a [Station]) -> Option<&'a Station> {
        let mut fallback = None;
        for (name, file) in &self.files {
            for a in &file.airspace {
                if !a.blocks.iter().any(|b| b.contains(at, fl)) {
                    continue;
                }
                let online = a.owners.iter().find_map(|o| {
                    let position = self.position(name, o)?;
                    stations.iter().find(|s| position.matches(s))
                });
                match (online, a.by_runway) {
                    (Some(s), false) => return Some(s),
                    (Some(s), true) => fallback = fallback.or(Some(s)),
                    (None, _) => {}
                }
            }
        }
        fallback
    }

    /// Whether any loaded airspace is at `at` (any level): VATGlasses knows this area.
    pub fn knows(&self, at: LatLon) -> bool {
        self.files.values().flat_map(|f| &f.airspace).flat_map(|a| &a.blocks).any(|b| b.contains(at, b.min.unwrap_or(0.0)))
    }
}

/// The VATGlasses files ("data/ed.json", "data/db-dg-dx.json") that cover ICAO codes
/// (airports or FIRs): a file's name lists the code prefixes it holds. "fss" (Europe's upper
/// airspace) and any other file an owner points into come along.
pub fn files_for(listing: &[String], icaos: &[String]) -> Vec<String> {
    let mut out: Vec<String> = listing
        .iter()
        .filter(|path| {
            let stem = path.trim_start_matches("data/").trim_end_matches(".json");
            let parts = stem.split(['/', '-']).filter(|p| !p.is_empty());
            parts.into_iter().any(|p| icaos.iter().any(|icao| icao.len() >= p.len() && icao[..p.len()].eq_ignore_ascii_case(p)))
        })
        .cloned()
        .collect();
    if let Some(fss) = listing.iter().find(|p| p.as_str() == "data/fss.json")
        && !out.contains(fss)
    {
        out.push(fss.clone());
    }
    out
}

/// The name an owner reference uses for a file: "data/ed.json" → "ed".
pub fn file_name(path: &str) -> String {
    path.trim_start_matches("data/").trim_end_matches(".json").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::naming::{Facility, NameSource};

    fn station(callsign: &str, khz: i32) -> Station {
        Station {
            callsign: callsign.into(),
            name: callsign.into(),
            name_source: NameSource::VatSpy,
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

    /// Bremen: a low sector to FL245 owned by ALR, a high one above it owned by the upper
    /// position, which falls back to the low one when offline. Both over the same square.
    const BREMEN: &str = r#"{
        "positions": {
            "ALR": {"pre": ["EDWW"], "type": "CTR", "frequency": "126.325", "callsign": "Bremen Radar"},
            "UPR": {"pre": ["EDWW"], "type": "CTR", "frequency": "133.725", "callsign": "Bremen Radar"},
            "APP": {"pre": ["EDDW"], "type": "APP", "frequency": "124.800", "callsign": "Bremen Arrival"}
        },
        "airspace": [
            {"id": "LOW", "owner": ["ALR"], "sectors": [{"min": 65, "max": 245, "points": [["530000","0080000"],["530000","0100000"],["540000","0100000"],["540000","0080000"]]}]},
            {"id": "HIGH", "owner": ["UPR", "ALR"], "sectors": [{"min": 245, "points": [["530000","0080000"],["530000","0100000"],["540000","0100000"],["540000","0080000"]]}]},
            {"id": "APP", "owner": ["APP", "ALR"], "sectors": [{"max": 65, "points": [["530000","0080000"],["530000","0100000"],["540000","0100000"],["540000","0080000"]]}]}
        ]
    }"#;

    #[test]
    fn the_owner_depends_on_the_level() {
        let mut sectors = Sectors::default();
        sectors.add("ed", BREMEN).unwrap();
        let online = vec![station("EDWW_ALR_CTR", 126_325), station("EDWW_UPR_CTR", 133_725), station("EDDW_APP", 124_800)];
        let at = LatLon { lat: 53.5, lon: 9.0 };
        let owner = |fl: f64, stations: &[Station]| sectors.owner(at, fl, stations).map(|s| s.callsign.clone());
        assert_eq!(owner(350.0, &online).as_deref(), Some("EDWW_UPR_CTR"));
        assert_eq!(owner(120.0, &online).as_deref(), Some("EDWW_ALR_CTR"));
        assert_eq!(owner(30.0, &online).as_deref(), Some("EDDW_APP"));
        // The upper position offline: the low one takes its airspace too.
        let without_upper = vec![station("EDWW_ALR_CTR", 126_325)];
        assert_eq!(owner(350.0, &without_upper).as_deref(), Some("EDWW_ALR_CTR"));
        // A controller on another frequency isn't that position.
        assert_eq!(owner(350.0, &[station("EDWW_UPR_CTR", 127_000)]), None);
        // Outside the square: nobody.
        assert!(sectors.owner(LatLon { lat: 50.0, lon: 9.0 }, 350.0, &online).is_none());
    }

    #[test]
    fn coordinates_and_file_names() {
        assert!((dms("552201").unwrap() - (55.0 + 22.0 / 60.0 + 1.0 / 3600.0)).abs() < 1e-9);
        assert!((dms("-0013137").unwrap() + (1.0 + 31.0 / 60.0 + 37.0 / 3600.0)).abs() < 1e-9);
        let listing: Vec<String> =
            ["data/ed.json", "data/eg.json", "data/db-dg-dx.json", "data/ek.json", "data/fss.json", "data/lppc.json"].map(String::from).to_vec();
        let wanted = files_for(&listing, &["EDDW".into(), "EKCH".into(), "EKDK".into()]);
        assert_eq!(wanted, ["data/ed.json", "data/ek.json", "data/fss.json"]);
        assert_eq!(files_for(&listing, &["LPPC".into()]), ["data/lppc.json", "data/fss.json"]);
        assert_eq!(file_name("data/db-dg-dx.json"), "db-dg-dx");
    }
}
