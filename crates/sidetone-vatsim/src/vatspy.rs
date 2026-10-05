//! Parser for `VATSpy.dat` from the VATSpy data project
//! (<https://github.com/vatsimnetwork/vatspy-data-project>, CC BY-SA 4.0).

use crate::geo::LatLon;
use std::collections::HashMap;

#[derive(Clone, Debug)]
pub struct Airport {
    pub icao: String,
    pub name: String,
    pub position: LatLon,
    pub iata: String,
    pub fir: String,
    pub pseudo: bool,
}

#[derive(Clone, Debug)]
pub struct Fir {
    pub icao: String,
    pub name: String,
    pub callsign_prefix: String,
    /// Id of the polygon in Boundaries.geojson.
    pub boundary: String,
}

#[derive(Clone, Debug, Default)]
pub struct VatSpy {
    pub airports: HashMap<String, Airport>,
    /// IATA/FAA LID → ICAO (US callsigns use "JFK_TWR").
    pub by_lid: HashMap<String, String>,
    pub firs: Vec<Fir>,
    /// Two-letter ICAO prefix → the country's word for area control ("Control", "Radar", "Center").
    pub country_suffix: HashMap<String, String>,
    /// UIR id → name (e.g. "AFRE" → "East Africa Control").
    pub uirs: HashMap<String, String>,
}

impl VatSpy {
    pub fn parse(text: &str) -> VatSpy {
        let mut out = VatSpy::default();
        let mut section = "";
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with(';') {
                continue;
            }
            if line.starts_with('[') && line.ends_with(']') {
                section = match line {
                    "[Countries]" => "countries",
                    "[Airports]" => "airports",
                    "[FIRs]" => "firs",
                    "[UIRs]" => "uirs",
                    _ => "",
                };
                continue;
            }
            let f: Vec<&str> = line.split('|').collect();
            match section {
                "countries" if f.len() >= 3 => {
                    if !f[2].is_empty() {
                        out.country_suffix.insert(f[1].to_string(), f[2].to_string());
                    }
                }
                "airports" if f.len() >= 7 => {
                    let (Ok(lat), Ok(lon)) = (f[2].parse(), f[3].parse()) else { continue };
                    let airport = Airport {
                        icao: f[0].to_string(),
                        name: f[1].to_string(),
                        position: LatLon { lat, lon },
                        iata: f[4].to_string(),
                        fir: f[5].to_string(),
                        pseudo: f[6] == "1",
                    };
                    if !airport.iata.is_empty() {
                        out.by_lid.entry(airport.iata.clone()).or_insert_with(|| airport.icao.clone());
                    }
                    // Keep the first (real) entry when pseudo duplicates exist.
                    out.airports.entry(airport.icao.clone()).or_insert(airport);
                }
                "firs" if f.len() >= 3 => out.firs.push(Fir {
                    icao: f[0].to_string(),
                    name: f[1].to_string(),
                    callsign_prefix: f[2].to_string(),
                    boundary: f.get(3).filter(|b| !b.is_empty()).unwrap_or(&f[0]).to_string(),
                }),
                "uirs" if f.len() >= 2 => {
                    out.uirs.insert(f[0].to_string(), f[1].to_string());
                }
                _ => {}
            }
        }
        out
    }

    /// Looks up an airport by ICAO or by IATA/FAA identifier.
    pub fn airport(&self, id: &str) -> Option<&Airport> {
        self.airports.get(id).or_else(|| self.by_lid.get(id).and_then(|icao| self.airports.get(icao)))
    }

    /// The FIR for an area-control callsign prefix, e.g. "LON_S" then "LON", or by ICAO ("EGTT").
    pub fn fir_for_prefix(&self, prefix: &str) -> Option<&Fir> {
        let mut candidate = prefix;
        loop {
            if let Some(fir) = self.firs.iter().find(|f| f.callsign_prefix == candidate) {
                return Some(fir);
            }
            if let Some(fir) = self.firs.iter().find(|f| f.icao == candidate && f.callsign_prefix.is_empty()) {
                return Some(fir);
            }
            candidate = &candidate[..candidate.rfind('_')?];
        }
    }

    /// The nearest non-pseudo airport to `position`, with its distance in nm. A coarse
    /// lat/lon box (widened if empty) keeps this to a handful of distance computations.
    pub fn nearest_airport(&self, position: LatLon) -> Option<(&Airport, f64)> {
        for degrees in [0.5, 2.0, 8.0, 180.0] {
            let found = self
                .airports
                .values()
                .filter(|a| !a.pseudo && (a.position.lat - position.lat).abs() <= degrees && lon_delta(a.position.lon, position.lon) <= degrees * 2.0)
                .map(|a| (a, crate::geo::distance_nm(position, a.position)))
                .min_by(|a, b| a.1.total_cmp(&b.1));
            if found.is_some() {
                return found;
            }
        }
        None
    }
}

fn lon_delta(a: f64, b: f64) -> f64 {
    let d = (a - b).abs() % 360.0;
    d.min(360.0 - d)
}

#[cfg(test)]
pub(crate) const SAMPLE: &str = "\
[Countries]
Germany|ED|Radar
Norway|EN|Control
United Kingdom|EG|Control
United States|K|Center
[Airports]
;ICAO|Airport Name|Latitude Decimal|Longitude Decimal|IATA/LID|FIR|IsPseudo
EDDF|Frankfurt|50.033333|8.570556||EDGG|0
EGLL|London Heathrow|51.4775|-0.461389|LHR|EGTT|0
EGKK|London Gatwick|51.148056|-0.190278|LGW|EGTT|0
ENBR|Bergen/Flesland|60.293522|5.218086||ENSV|0
KJFK|New York-John F. Kennedy Intl NY|40.63975|-73.778925|JFK|KZNY|0
[FIRs]
;ICAO|NAME|CALLSIGN PREFIX|FIR BOUNDARY
EGTT|London|LON|EGTT
EGTT|London Control (South)|LON_S|EGTT-S
EGPX|Scottish|SCO|EGPX
EDGG|Langen||EDGG
KZNY|New York|NY|KZNY
ADR|Adria Radar||ADR
[UIRs]
AFRE|East Africa Control|HAAA,HCSM
";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_sections() {
        let v = VatSpy::parse(SAMPLE);
        assert_eq!(v.airports["EGLL"].name, "London Heathrow");
        assert_eq!(v.airport("JFK").unwrap().icao, "KJFK");
        assert_eq!(v.country_suffix["EG"], "Control");
        assert_eq!(v.fir_for_prefix("LON_S").unwrap().boundary, "EGTT-S");
        assert_eq!(v.fir_for_prefix("LON_X").unwrap().name, "London");
        assert_eq!(v.fir_for_prefix("LON").unwrap().boundary, "EGTT");
        assert_eq!(v.fir_for_prefix("EDGG").unwrap().name, "Langen");
        assert!(v.fir_for_prefix("ZZZZ").is_none());
        assert_eq!(v.uirs["AFRE"], "East Africa Control");
    }

    #[test]
    fn nearest() {
        let v = VatSpy::parse(SAMPLE);
        let (a, d) = v.nearest_airport(LatLon { lat: 51.47, lon: -0.45 }).unwrap();
        assert_eq!(a.icao, "EGLL");
        assert!(d < 2.0);
    }
}
