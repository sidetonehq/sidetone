//! SimBrief OFP import (`xml.fetcher.php?username=…&json=1`). Parsing is lenient: SimBrief
//! mixes strings and numbers and adds fields freely, so every value is read as optional text.

use serde_json::Value;

pub const FETCH_URL: &str = "https://www.simbrief.com/api/xml.fetcher.php";

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Plan {
    pub callsign: String,
    pub airline: String,
    pub flight_number: String,
    pub aircraft_icao: String,
    pub registration: String,
    pub origin: String,
    /// English airport name from SimBrief, title-cased ("Amsterdam Schiphol").
    pub origin_name: String,
    pub origin_runway: String,
    pub destination: String,
    pub destination_name: String,
    pub destination_runway: String,
    pub alternate: String,
    /// Initial cruise altitude in feet.
    pub cruise_altitude_ft: Option<u32>,
    pub route: String,
    /// Estimated time en route in minutes.
    pub ete_minutes: Option<u32>,
    pub cost_index: String,
    /// The full ICAO flight plan line, if present.
    pub icao_flight_plan: String,
    /// Navlog waypoints: (ident, lat, lon).
    pub fixes: Vec<(String, f64, f64)>,
}

impl Plan {
    /// Cruise as "FL350" (or "12000 ft" below FL180).
    pub fn cruise_label(&self) -> String {
        match self.cruise_altitude_ft {
            Some(ft) if ft >= 18_000 => format!("FL{}", ft / 100),
            Some(ft) => format!("{ft} ft"),
            None => "—".into(),
        }
    }

    pub fn ete_label(&self) -> String {
        self.ete_minutes.map(|m| format!("{}h{:02}", m / 60, m % 60)).unwrap_or_else(|| "—".into())
    }
}

fn text(v: &Value, path: &[&str]) -> String {
    let mut cur = v;
    for key in path {
        match cur.get(key) {
            Some(next) => cur = next,
            None => return String::new(),
        }
    }
    match cur {
        Value::String(s) => s.trim().to_string(),
        Value::Number(n) => n.to_string(),
        _ => String::new(),
    }
}

/// `navlog.fix` is an array, or a single object for one-fix plans.
fn fixes(v: &Value) -> Vec<(String, f64, f64)> {
    let list = match v.get("navlog").and_then(|n| n.get("fix")) {
        Some(Value::Array(a)) => a.iter().collect::<Vec<_>>(),
        Some(single @ Value::Object(_)) => vec![single],
        _ => return Vec::new(),
    };
    list.into_iter()
        .filter_map(|f| {
            let lat = text(f, &["pos_lat"]).parse().ok()?;
            let lon = text(f, &["pos_long"]).parse().ok()?;
            Some((text(f, &["ident"]), lat, lon))
        })
        .collect()
}

/// SimBrief accepts a username or the numeric Pilot ID (Account Settings); pick the right query.
pub fn lookup_param(input: &str) -> (&'static str, &str) {
    let input = input.trim();
    if !input.is_empty() && input.chars().all(|c| c.is_ascii_digit()) { ("userid", input) } else { ("username", input) }
}

/// Turns SimBrief's terse errors into something a pilot can act on.
fn explain(status: &str) -> String {
    let lower = status.to_ascii_lowercase();
    if lower.contains("unknown userid") || lower.contains("unknown user") {
        "SimBrief doesn't recognise that Pilot ID or username. Check it in Settings (simbrief.com → Account Settings).".into()
    } else if lower.contains("no flight") || lower.contains("no ofp") || lower.contains("not found") {
        "No flight plan found on SimBrief. Generate one on simbrief.com first.".into()
    } else if status.is_empty() {
        "SimBrief returned no flight plan.".into()
    } else {
        format!("SimBrief: {}", status.trim_start_matches("Error:").trim())
    }
}

/// "COPENHAGEN KASTRUP" → "Copenhagen Kastrup" (mixed-case input is left alone).
fn title_case(s: &str) -> String {
    if s.chars().any(|c| c.is_lowercase()) {
        return s.to_string();
    }
    s.split_whitespace()
        .map(|w| {
            let mut c = w.chars();
            c.next().map(|f| f.to_uppercase().chain(c.flat_map(|x| x.to_lowercase())).collect::<String>()).unwrap_or_default()
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Parses a SimBrief JSON response into a plan, or the error SimBrief reported.
pub fn parse(json: &str) -> Result<Plan, String> {
    let v: Value = serde_json::from_str(json).map_err(|_| "SimBrief sent an unexpected response. Try again in a moment.".to_string())?;
    let status = text(&v, &["fetch", "status"]);
    if !status.eq_ignore_ascii_case("success") {
        return Err(explain(&status));
    }
    let airline = text(&v, &["general", "icao_airline"]);
    let flight_number = text(&v, &["general", "flight_number"]);
    let mut callsign = text(&v, &["atc", "callsign"]);
    if callsign.is_empty() {
        callsign = format!("{airline}{flight_number}");
    }
    Ok(Plan {
        callsign,
        airline,
        flight_number,
        aircraft_icao: text(&v, &["aircraft", "icaocode"]),
        registration: text(&v, &["aircraft", "reg"]),
        origin: text(&v, &["origin", "icao_code"]),
        origin_name: title_case(&text(&v, &["origin", "name"])),
        origin_runway: text(&v, &["origin", "plan_rwy"]),
        destination: text(&v, &["destination", "icao_code"]),
        destination_name: title_case(&text(&v, &["destination", "name"])),
        destination_runway: text(&v, &["destination", "plan_rwy"]),
        alternate: text(&v, &["alternate", "icao_code"]),
        cruise_altitude_ft: text(&v, &["general", "initial_altitude"]).parse().ok(),
        route: text(&v, &["general", "route"]),
        ete_minutes: text(&v, &["times", "est_time_enroute"]).parse::<u32>().ok().map(|s| s / 60),
        cost_index: text(&v, &["general", "costindex"]),
        icao_flight_plan: text(&v, &["atc", "flightplan_text"]),
        fixes: fixes(&v),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_ofp() {
        let plan = parse(include_str!("../tests/fixtures/simbrief.json")).unwrap();
        assert_eq!(plan.callsign, "BAW123");
        assert_eq!((plan.origin.as_str(), plan.destination.as_str()), ("EGLL", "ENBR"));
        assert_eq!(plan.cruise_label(), "FL350");
        assert_eq!(plan.ete_label(), "1h52");
        assert_eq!(plan.aircraft_icao, "A20N");
        assert_eq!(plan.fixes.len(), 2);
        assert_eq!(plan.fixes[1].0, "KENET");
        assert_eq!(plan.destination_name, "Bergen Flesland");
        assert_eq!(title_case("COPENHAGEN KASTRUP"), "Copenhagen Kastrup");
    }

    #[test]
    fn reports_errors() {
        let err = parse(r#"{"fetch":{"userid":"","status":"Error: Unknown UserID"}}"#).unwrap_err();
        assert!(err.starts_with("SimBrief doesn't recognise that Pilot ID"), "{err}");
        assert!(parse("<html>").is_err());
        assert_eq!(parse(r#"{"fetch":{"status":"Error: Something else"}}"#).unwrap_err(), "SimBrief: Something else");
    }

    #[test]
    fn username_or_pilot_id() {
        assert_eq!(lookup_param("nickyt"), ("username", "nickyt"));
        assert_eq!(lookup_param(" 123456 "), ("userid", "123456"));
    }
}
