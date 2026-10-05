//! VATSIM events (`my.vatsim.net/api/v2/events/latest`).

use serde::Deserialize;

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
struct Response {
    data: Vec<RawEvent>,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
struct RawEvent {
    id: u64,
    name: String,
    link: String,
    airports: Vec<RawAirport>,
    start_time: String,
    end_time: String,
    short_description: String,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
struct RawAirport {
    icao: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Event {
    pub id: u64,
    pub name: String,
    pub link: String,
    pub airports: Vec<String>,
    /// Unix seconds.
    pub start: i64,
    pub end: i64,
    pub summary: String,
}

impl Event {
    pub fn is_live(&self, now: i64) -> bool {
        self.start <= now && now < self.end
    }

    pub fn involves(&self, icao: &str) -> bool {
        self.airports.iter().any(|a| a == icao)
    }
}

/// Parses the events response, keeping events that haven't ended, sorted by start time.
pub fn parse(json: &str, now: i64) -> Result<Vec<Event>, String> {
    let r: Response = serde_json::from_str(json).map_err(|e| format!("events: {e}"))?;
    let mut out: Vec<Event> = r
        .data
        .into_iter()
        .filter_map(|e| {
            Some(Event {
                start: crate::time::parse_iso8601(&e.start_time)?,
                end: crate::time::parse_iso8601(&e.end_time)?,
                id: e.id,
                name: e.name,
                link: e.link,
                airports: e.airports.into_iter().map(|a| a.icao.to_ascii_uppercase()).filter(|a| !a.is_empty()).collect(),
                summary: e.short_description,
            })
        })
        .filter(|e| e.end > now)
        .collect();
    out.sort_by_key(|e| e.start);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_filters() {
        let json = r#"{"data":[
            {"id":1,"name":"Later","airports":[{"icao":"ekch"}],"start_time":"2026-10-06T17:00:00.000000Z","end_time":"2026-10-06T20:00:00.000000Z"},
            {"id":2,"name":"Live","airports":[{"icao":"EGLL"}],"start_time":"2026-10-05T17:00:00.000000Z","end_time":"2026-10-05T20:00:00.000000Z"},
            {"id":3,"name":"Over","airports":[],"start_time":"2026-10-04T17:00:00Z","end_time":"2026-10-04T20:00:00Z"}]}"#;
        let now = 1_791_219_600 + 600;
        let events = parse(json, now).unwrap();
        assert_eq!(events.iter().map(|e| e.id).collect::<Vec<_>>(), vec![2, 1]);
        assert!(events[0].is_live(now));
        assert!(events[1].involves("EKCH"));
    }
}
