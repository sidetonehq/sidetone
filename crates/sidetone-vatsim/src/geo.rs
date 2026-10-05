//! Great-circle distances.

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LatLon {
    pub lat: f64,
    pub lon: f64,
}

/// Distance in nautical miles (haversine).
pub fn distance_nm(a: LatLon, b: LatLon) -> f64 {
    const EARTH_RADIUS_NM: f64 = 3440.065;
    let (la1, la2) = (a.lat.to_radians(), b.lat.to_radians());
    let dlat = la2 - la1;
    let dlon = (b.lon - a.lon).to_radians();
    let h = (dlat / 2.0).sin().powi(2) + la1.cos() * la2.cos() * (dlon / 2.0).sin().powi(2);
    2.0 * EARTH_RADIUS_NM * h.sqrt().asin()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn heathrow_to_jfk() {
        let lhr = LatLon { lat: 51.4775, lon: -0.461389 };
        let jfk = LatLon { lat: 40.63975, lon: -73.778925 };
        let d = distance_nm(lhr, jfk);
        assert!((d - 2991.0).abs() < 10.0, "{d}");
    }
}
