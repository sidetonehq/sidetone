//! Approach and departure airspace from the SimAware TRACON Project (CC BY-SA 4.0): the real
//! shapes of terminal units ("Gatwick Director"), so "who covers me" doesn't have to guess
//! with a circle around the airport.

use crate::boundaries::{Boundary, Geometry};
use crate::geo::LatLon;
use serde::Deserialize;

/// One terminal unit's airspace.
#[derive(Clone, Debug)]
pub struct Tracon {
    pub area: Boundary,
    /// Callsign prefixes it belongs to ("EGKK", or "EGKK_F" for a split).
    pub prefixes: Vec<String>,
    /// "DEP" (or "TWR") when it's for that suffix only; otherwise APP and DEP.
    pub suffix: Option<String>,
    /// Radio name ("Gatwick Director").
    pub name: String,
}

#[derive(Clone, Debug, Default)]
pub struct Tracons {
    pub items: Vec<Tracon>,
}

#[derive(Deserialize)]
struct Collection {
    features: Vec<Feature>,
}

#[derive(Deserialize)]
struct Feature {
    properties: Properties,
    geometry: Geometry,
}

#[derive(Deserialize)]
struct Properties {
    id: String,
    #[serde(default)]
    prefix: Vec<String>,
    #[serde(default)]
    suffix: Option<String>,
    #[serde(default)]
    name: String,
}

impl Tracons {
    pub fn parse(json: &str) -> Result<Tracons, String> {
        let c: Collection = serde_json::from_str(json).map_err(|e| format!("TRACON boundaries: {e}"))?;
        let items = c
            .features
            .into_iter()
            .filter_map(|f| {
                let area = Boundary::from_geometry(f.properties.id, false, f.geometry)?;
                let prefixes = f.properties.prefix.into_iter().map(|p| p.to_ascii_uppercase()).collect();
                let suffix = f.properties.suffix.map(|s| s.to_ascii_uppercase()).filter(|s| !s.is_empty());
                Some(Tracon { area, prefixes, suffix, name: f.properties.name })
            })
            .collect();
        Ok(Tracons { items })
    }

    /// The airspace a controller works, by callsign: the longest prefix first ("EGKK_F_APP"
    /// tries "EGKK_F", then "EGKK"); a shape for this exact suffix beats a general one, and a
    /// general one covers APP and DEP.
    pub fn for_callsign(&self, callsign: &str) -> Option<&Tracon> {
        let parts: Vec<&str> = callsign.split('_').collect();
        let (&suffix, prefix_parts) = parts.split_last()?;
        let suffix = suffix.to_ascii_uppercase();
        let candidates = (1..=prefix_parts.len().min(2)).rev().map(|n| prefix_parts[..n].join("_").to_ascii_uppercase());
        for prefix in candidates {
            let matching: Vec<&Tracon> = self.items.iter().filter(|t| t.prefixes.contains(&prefix)).collect();
            let exact = matching.iter().find(|t| t.suffix.as_deref() == Some(suffix.as_str()));
            let general = matching.iter().find(|t| t.suffix.is_none() && matches!(suffix.as_str(), "APP" | "DEP"));
            if let Some(t) = exact.or(general) {
                return Some(t);
            }
        }
        None
    }
}

impl Tracon {
    pub fn contains(&self, p: LatLon) -> bool {
        self.area.contains(p)
    }
}

#[cfg(test)]
pub(crate) const SAMPLE: &str = r#"{"type":"FeatureCollection","name":"TRACONs","crs":{"type":"name","properties":{"name":"x"}},"features":[
 {"type":"Feature","properties":{"id":"EGKK","prefix":["EGKK"],"name":"Gatwick Director"},"geometry":{"type":"MultiPolygon","coordinates":[[[[-0.6,50.9],[0.1,50.9],[0.1,51.3],[-0.6,51.3],[-0.6,50.9]]]]}},
 {"type":"Feature","properties":{"id":"EGKKDEP","prefix":["EGKK"],"suffix":"DEP","name":"Gatwick Departures"},"geometry":{"type":"Polygon","coordinates":[[[-0.3,51.0],[0.0,51.0],[0.0,51.2],[-0.3,51.2],[-0.3,51.0]]]}},
 {"type":"Feature","properties":{"id":"KLAX-F","prefix":["LAX_F"],"name":"SoCal Approach"},"geometry":{"type":"Polygon","coordinates":[[[-119,33],[-117,33],[-117,35],[-119,35],[-119,33]]]}},
 {"type":"Feature","properties":{"id":"LTCN","prefix":["LTCN"],"suffix":"TWR","name":"Maras Tower/Approach"},"geometry":{"type":"Polygon","coordinates":[[[36,37],[38,37],[38,38],[36,38],[36,37]]]}}
]}"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_controllers_to_their_airspace() {
        let t = Tracons::parse(SAMPLE).unwrap();
        assert_eq!(t.items.len(), 4);
        assert_eq!(t.for_callsign("EGKK_APP").map(|t| t.name.as_str()), Some("Gatwick Director"));
        assert_eq!(t.for_callsign("EGKK_DEP").map(|t| t.name.as_str()), Some("Gatwick Departures"), "the DEP shape for DEP");
        assert_eq!(t.for_callsign("EGKK_F_APP").map(|t| t.name.as_str()), Some("Gatwick Director"), "falls back to the shorter prefix");
        assert_eq!(t.for_callsign("LAX_F_APP").map(|t| t.name.as_str()), Some("SoCal Approach"), "a split by double prefix");
        assert_eq!(t.for_callsign("LTCN_TWR").map(|t| t.name.as_str()), Some("Maras Tower/Approach"));
        assert!(t.for_callsign("LTCN_APP").is_none(), "a TWR-only shape isn't for APP");
        assert!(t.for_callsign("EGKK_TWR").is_none(), "towers keep their radius");
        assert!(t.for_callsign("EGLL_APP").is_none());
    }

    #[test]
    fn inside_the_shape() {
        let t = Tracons::parse(SAMPLE).unwrap();
        let gatwick = t.for_callsign("EGKK_APP").unwrap();
        assert!(gatwick.contains(LatLon { lat: 51.15, lon: -0.19 }));
        assert!(!gatwick.contains(LatLon { lat: 51.47, lon: -0.45 }), "Heathrow is outside Gatwick's");
    }
}
