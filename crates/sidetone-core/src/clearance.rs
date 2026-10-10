//! The pilot's clearance & notes card, and auto-fill from SimBrief, the VATSIM feed, the ATIS
//! and METARs. Auto-filled values only fill empty fields (or keep their own earlier value
//! current), so typing always wins.

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct FlightNotes {
    pub squawk: String,
    pub initial_altitude: String,
    pub sid: String,
    pub runway: String,
    /// e.g. "FL070", from the departure ATIS.
    pub transition_level: String,
    pub qnh: String,
    pub atis: String,
    pub stand: String,
    pub notes: String,
    /// The ATIS letter was filled by Sidetone (kept current) rather than typed (left alone).
    pub atis_auto: bool,
    /// Same for QNH (from the departure METAR).
    pub qnh_auto: bool,
    /// Same for the transition level (from the departure ATIS).
    pub transition_level_auto: bool,
    pub arrival: ArrivalNotes,
}

/// What you'll need for the arrival: prefilled from SimBrief, the arrival ATIS and METAR, the
/// rest noted as you're told. Auto-filled values follow the same rules as the clearance's.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ArrivalNotes {
    pub runway: String,
    pub star: String,
    /// e.g. "ILS 22".
    pub approach: String,
    pub stand: String,
    pub atis: String,
    pub qnh: String,
    pub transition_level: String,
    /// The frequency you were told to contact next, e.g. "120.130".
    pub frequency: String,
    pub atis_auto: bool,
    pub qnh_auto: bool,
    pub transition_level_auto: bool,
}

fn tokens(text: &str) -> Vec<String> {
    text.to_ascii_uppercase()
        .split(|c: char| c.is_whitespace() || c == '@' || c == ',' || c == ';')
        .map(|t| t.trim_end_matches(['.', ':']).trim_start_matches(':').to_string())
        .filter(|t| !t.is_empty())
        .collect()
}

/// SID names look like CPT3J, BPK5K, OBOKA1G: letters, one digit, optional letter.
fn is_sid(t: &str) -> bool {
    let letters = t.chars().take_while(|c| c.is_ascii_alphabetic()).count();
    let rest = &t[letters..];
    let mut r = rest.chars();
    (2..=6).contains(&letters) && r.next().is_some_and(|c| c.is_ascii_digit()) && r.as_str().len() <= 1 && r.all(|c| c.is_ascii_alphabetic())
}

impl FlightNotes {
    /// Keeps an auto-filled value current with `latest`; a value the pilot typed is left alone.
    /// Returns true if the value changed.
    pub fn track(value: &mut String, auto: &mut bool, latest: &str) -> bool {
        let latest = latest.trim();
        if latest.is_empty() || !(value.trim().is_empty() || *auto) || value.trim() == latest {
            return false;
        }
        *value = latest.to_string();
        *auto = true;
        true
    }

    /// Fills `field` only if the pilot hasn't entered anything.
    pub fn fill(field: &mut String, value: &str) -> bool {
        if field.trim().is_empty() && !value.trim().is_empty() {
            *field = value.trim().to_string();
            return true;
        }
        false
    }
}

/// QNH from a METAR: "Q1022" or "A2992".
pub fn qnh_from_metar(metar: &str) -> Option<String> {
    metar
        .split_whitespace()
        .find(|t| (t.starts_with('Q') || t.starts_with('A')) && t.len() == 5 && t[1..].chars().all(|c| c.is_ascii_digit()))
        .map(String::from)
}

/// Transition level from ATIS text: "TRANSITION LEVEL 70", "TRANS LEVEL FL070", "TRL 70" → "FL070".
pub fn transition_level_from_atis(text: &str) -> Option<String> {
    let t = tokens(text);
    let level = |w: &str| {
        let digits = w.strip_prefix("FL").unwrap_or(w);
        let n: u32 = digits.parse().ok().filter(|_| (2..=3).contains(&digits.len()))?;
        (20..=200).contains(&n).then(|| format!("FL{n:03}"))
    };
    t.iter().enumerate().find_map(|(i, w)| {
        let skip = match (w.as_str(), t.get(i + 1).map(String::as_str)) {
            ("TRANSITION" | "TRANS", Some("LEVEL" | "LVL")) => 2,
            ("TRL" | "TRLVL", _) => 1,
            _ => return None,
        };
        t.iter().skip(i + skip).take(2).find_map(|w| level(w))
    })
}

/// A level or altitude as shown everywhere: flight levels as "FL050" ("FL50", "fl 50" and a bare
/// "50" all mean that), altitudes as "5000 ft" ("5000", "A5000", "5000FT"). Anything else as typed.
pub fn display_level(value: &str) -> String {
    let compact: String = value.split_whitespace().collect::<String>().to_ascii_uppercase();
    let digits = |s: &str| (!s.is_empty() && s.chars().all(|c| c.is_ascii_digit())).then(|| s.parse::<u32>().ok()).flatten();
    if let Some(n) = compact.strip_prefix("FL").and_then(digits).or_else(|| digits(&compact).filter(|_| (2..=3).contains(&compact.len()))) {
        return format!("FL{n:03}");
    }
    let feet = compact.strip_suffix("FT").unwrap_or(&compact);
    let feet = feet.strip_prefix('A').unwrap_or(feet);
    match digits(feet) {
        Some(n) if (4..=5).contains(&feet.len()) => format!("{n} ft"),
        _ => value.trim().to_string(),
    }
}

/// A QNH as shown everywhere: "Q1015" for hectopascals, "A2992" for inches ("1015", "q1015",
/// "29.92" too). Anything else as typed.
pub fn display_qnh(value: &str) -> String {
    let compact: String = value.split_whitespace().collect::<String>().to_ascii_uppercase().replace('.', "");
    let number = compact.trim_start_matches(['Q', 'A']);
    match number.parse::<u32>() {
        Ok(n) if (3..=4).contains(&number.len()) && (900..=1100).contains(&n) => format!("Q{n:04}"),
        Ok(n) if number.len() == 4 && (2700..=3200).contains(&n) => format!("A{n}"),
        _ => value.trim().to_string(),
    }
}

/// Whether a filed route and a planned one are the same flight. Speed/level groups and DCTs
/// don't count, and neither do procedures only one of them spells out: filed routes usually
/// leave out the SID and STAR that SimBrief includes, so one being a stretch of the other
/// counts as a match.
pub fn routes_match(a: &str, b: &str) -> bool {
    let (a, b) = (route_points(a), route_points(b));
    let (short, long) = if a.len() <= b.len() { (a, b) } else { (b, a) };
    !short.is_empty() && long.windows(short.len()).any(|w| w == short.as_slice())
}

/// The planned route when it's the filed one with more spelled out: filed routes often leave
/// out the SID and STAR that SimBrief includes, so its route says how you'll actually fly.
/// `None` when they differ, or the filed route is already as full.
pub fn fuller_route(filed: &str, planned: &str) -> Option<String> {
    (routes_match(filed, planned) && route_points(planned).len() > route_points(filed).len()).then(|| planned.trim().to_string())
}

/// Route words without speed/level groups ("ELB/N0459F380" → "ELB") and DCTs.
fn route_points(route: &str) -> Vec<String> {
    route.split_whitespace().map(|t| t.split('/').next().unwrap_or(t).to_ascii_uppercase()).filter(|t| t != "DCT" && !t.is_empty()).collect()
}

/// The STAR at the end of a SimBrief route string, if it ends with one ("… ELKAP ELKA3A").
pub fn star_from_route(route: &str) -> Option<String> {
    route.split_whitespace().last().filter(|t| is_sid(t)).map(String::from)
}

/// The SID at the start of a SimBrief route string, if it starts with one.
pub fn sid_from_route(route: &str) -> Option<String> {
    route.split_whitespace().next().filter(|t| is_sid(t)).map(String::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn levels_display_one_way() {
        for typed in ["FL50", "fl 050", "50", "FL050"] {
            assert_eq!(display_level(typed), "FL050");
        }
        assert_eq!(display_level("FL360"), "FL360");
        for typed in ["5000", "A5000", "5000FT", "5000 ft"] {
            assert_eq!(display_level(typed), "5000 ft");
        }
        assert_eq!(display_level("CLIMB VIA SID"), "CLIMB VIA SID");
        assert_eq!(display_level(""), "");
    }

    #[test]
    fn qnh_displays_one_way() {
        assert_eq!(display_qnh("1015"), "Q1015");
        assert_eq!(display_qnh("q 1015"), "Q1015");
        assert_eq!(display_qnh("995"), "Q0995");
        assert_eq!(display_qnh("850"), "850");
        assert_eq!(display_qnh("0995"), "Q0995");
        assert_eq!(display_qnh("29.92"), "A2992");
        assert_eq!(display_qnh("A2992"), "A2992");
        assert_eq!(display_qnh("STD"), "STD");
    }

    #[test]
    fn transition_level_from_atis_text() {
        let atis = "HEATHROW INFORMATION E TIME 1050 RWY 27R IN USE TRANSITION LEVEL 70 QNH 1020";
        assert_eq!(transition_level_from_atis(atis).as_deref(), Some("FL070"));
        assert_eq!(transition_level_from_atis("TRANS LVL FL080").as_deref(), Some("FL080"));
        assert_eq!(transition_level_from_atis("RWY 22 TRL 55, CAUTION").as_deref(), Some("FL055"));
        assert_eq!(transition_level_from_atis("TRANSITION ALTITUDE 6000 FT"), None);
        assert_eq!(transition_level_from_atis("QNH 1013"), None);
    }

    #[test]
    fn fill_policy() {
        let mut n = FlightNotes { runway: "27L".into(), ..Default::default() };
        assert!(!FlightNotes::fill(&mut n.runway, "27R"), "typed value wins");
        assert!(FlightNotes::fill(&mut n.sid, "CPT3J"));
    }

    #[test]
    fn tracking_follows_auto_values_only() {
        let (mut atis, mut auto) = (String::new(), false);
        assert!(FlightNotes::track(&mut atis, &mut auto, "E"), "fills an empty field");
        assert!(auto);
        assert!(FlightNotes::track(&mut atis, &mut auto, "F"), "keeps an auto value current");
        assert_eq!(atis, "F");
        assert!(!FlightNotes::track(&mut atis, &mut auto, "F"), "no change, no update");
        auto = false; // the pilot typed it
        assert!(!FlightNotes::track(&mut atis, &mut auto, "G"), "typed values are left alone");
        assert_eq!(atis, "F");
    }

    #[test]
    fn filed_routes_without_procedures_match() {
        let simbrief = "DORD2G DORDI DCT OKASI DCT OKEKO DCT MOU DCT ELB DCT ELKAP ELKA3A";
        let filed = "OKASI DCT OKEKO DCT MOU DCT ELB/N0459F380 DCT ELKAP";
        assert!(routes_match(filed, simbrief), "only the SID and STAR differ");
        assert!(!routes_match("OKASI DCT MOU DCT ELKAP", simbrief), "a waypoint is missing");
        assert!(!routes_match("CPT UL9 KENET", simbrief));
        assert!(!routes_match("", simbrief));
        assert_eq!(fuller_route(filed, simbrief).as_deref(), Some(simbrief), "SimBrief adds the SID and STAR");
        assert_eq!(fuller_route(simbrief, simbrief), None, "already as full");
        assert_eq!(fuller_route("OKASI DCT MOU DCT ELKAP", simbrief), None, "a different route");
        // A real one: Heathrow to Manchester, filed without the SID and STAR.
        let planned = "UMLA1G UMLAT T418 WELIN T420 ELVOS ELVO1M";
        assert_eq!(fuller_route("UMLAT T418 WELIN T420 ELVOS", planned).as_deref(), Some(planned));
    }

    #[test]
    fn helpers() {
        assert_eq!(qnh_from_metar("EGLL 051720Z 25011KT 9999 NCD 21/14 Q1022").as_deref(), Some("Q1022"));
        assert_eq!(qnh_from_metar("KJFK 051751Z 18010KT 10SM FEW250 24/12 A2992 RMK").as_deref(), Some("A2992"));
        assert_eq!(sid_from_route("CPT3J CPT UL9 KENET").as_deref(), Some("CPT3J"));
        assert_eq!(sid_from_route("DCT CPT"), None);
        assert_eq!(star_from_route("DORD2G DORDI DCT ELKAP ELKA3A").as_deref(), Some("ELKA3A"));
        assert_eq!(star_from_route("OKASI DCT ELKAP"), None, "a filed route without a STAR");
    }
}
