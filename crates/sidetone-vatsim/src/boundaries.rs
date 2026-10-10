//! FIR/sector polygons from VATSpy's `Boundaries.geojson` (CC BY-SA 4.0).

use crate::geo::LatLon;
use serde::Deserialize;

#[derive(Clone, Debug)]
pub struct Boundary {
    pub id: String,
    pub oceanic: bool,
    /// (min_lat, min_lon, max_lat, max_lon) for cheap rejection.
    bbox: (f64, f64, f64, f64),
    /// Polygons → rings (first is the outer ring) → points as (lon, lat).
    polygons: Vec<Vec<Vec<(f64, f64)>>>,
}

#[derive(Clone, Debug, Default)]
pub struct Boundaries {
    pub items: Vec<Boundary>,
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
    oceanic: serde_json::Value,
}

#[derive(Deserialize)]
#[serde(tag = "type")]
pub(crate) enum Geometry {
    Polygon {
        coordinates: Vec<Vec<Vec<f64>>>,
    },
    MultiPolygon {
        coordinates: Vec<Vec<Vec<Vec<f64>>>>,
    },
    #[serde(other)]
    Other,
}

fn ring(points: Vec<Vec<f64>>) -> Vec<(f64, f64)> {
    points.into_iter().filter(|p| p.len() >= 2).map(|p| (p[0], p[1])).collect()
}

impl Boundary {
    /// A boundary from a GeoJSON geometry; `None` for anything that isn't a (multi)polygon.
    pub(crate) fn from_geometry(id: String, oceanic: bool, geometry: Geometry) -> Option<Boundary> {
        let polygons: Vec<Vec<Vec<(f64, f64)>>> = match geometry {
            Geometry::Polygon { coordinates } => vec![coordinates.into_iter().map(ring).collect()],
            Geometry::MultiPolygon { coordinates } => coordinates.into_iter().map(|p| p.into_iter().map(ring).collect()).collect(),
            Geometry::Other => return None,
        };
        let mut bbox = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
        for &(lon, lat) in polygons.iter().filter_map(|p| p.first()).flatten() {
            bbox = (bbox.0.min(lat), bbox.1.min(lon), bbox.2.max(lat), bbox.3.max(lon));
        }
        Some(Boundary { id, oceanic, bbox, polygons })
    }
}

impl Boundaries {
    pub fn parse(json: &str) -> Result<Boundaries, String> {
        let c: Collection = serde_json::from_str(json).map_err(|e| format!("boundaries: {e}"))?;
        let mut items = Vec::with_capacity(c.features.len());
        for f in c.features {
            let oceanic = matches!(&f.properties.oceanic, serde_json::Value::String(s) if s == "1") || f.properties.oceanic == serde_json::json!(1);
            items.extend(Boundary::from_geometry(f.properties.id, oceanic, f.geometry));
        }
        Ok(Boundaries { items })
    }

    /// Ids of every boundary containing `p` (parent FIRs and their sectors can overlap).
    pub fn containing(&self, p: LatLon) -> impl Iterator<Item = &Boundary> {
        self.items.iter().filter(move |b| b.contains(p))
    }
}

impl Boundary {
    pub fn contains(&self, p: LatLon) -> bool {
        let (min_lat, min_lon, max_lat, max_lon) = self.bbox;
        if p.lat < min_lat || p.lat > max_lat || p.lon < min_lon || p.lon > max_lon {
            return false;
        }
        self.polygons.iter().any(|rings| {
            let mut inside = rings.first().is_some_and(|outer| in_ring(outer, p));
            for hole in rings.iter().skip(1) {
                if inside && in_ring(hole, p) {
                    inside = false;
                }
            }
            inside
        })
    }
}

/// Ray casting in lon/lat space (fine for FIR-sized shapes away from the antimeridian).
pub(crate) fn in_ring(ring: &[(f64, f64)], p: LatLon) -> bool {
    let (x, y) = (p.lon, p.lat);
    let mut inside = false;
    let mut j = ring.len().wrapping_sub(1);
    for i in 0..ring.len() {
        let (xi, yi) = ring[i];
        let (xj, yj) = ring[j];
        if (yi > y) != (yj > y) && x < (xj - xi) * (y - yi) / (yj - yi) + xi {
            inside = !inside;
        }
        j = i;
    }
    inside
}

#[cfg(test)]
pub(crate) const SAMPLE: &str = r#"{"type":"FeatureCollection","features":[
 {"type":"Feature","properties":{"id":"EGTT","oceanic":"0"},"geometry":{"type":"MultiPolygon","coordinates":[[[[-6,49],[2,49],[2,55],[-6,55],[-6,49]]]]}},
 {"type":"Feature","properties":{"id":"EGTT-S","oceanic":"0"},"geometry":{"type":"Polygon","coordinates":[[[-6,49],[2,49],[2,52],[-6,52],[-6,49]]]}},
 {"type":"Feature","properties":{"id":"ENSV","oceanic":"0"},"geometry":{"type":"Polygon","coordinates":[[[2,55],[12,55],[12,64],[2,64],[2,55]],[[5,58],[6,58],[6,59],[5,59],[5,58]]]}}
]}"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn point_in_polygon_with_holes() {
        let b = Boundaries::parse(SAMPLE).unwrap();
        let heathrow = LatLon { lat: 51.47, lon: -0.45 };
        let ids: Vec<_> = b.containing(heathrow).map(|b| b.id.as_str()).collect();
        assert_eq!(ids, vec!["EGTT", "EGTT-S"]);
        assert_eq!(b.containing(LatLon { lat: 60.3, lon: 5.2 }).next().unwrap().id, "ENSV");
        assert!(b.containing(LatLon { lat: 58.5, lon: 5.5 }).next().is_none(), "inside the hole");
        assert!(b.containing(LatLon { lat: 0.0, lon: 0.0 }).next().is_none());
    }
}
