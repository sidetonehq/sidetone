//! The pilot's clearance & notes card, and auto-fill from clearances, SimBrief and METARs.
//!
//! Policy: values from an actual ATC clearance overwrite (they're authoritative); values
//! from SimBrief, the VATSIM feed or METARs only fill empty fields, so typing always wins.

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
}

/// What a clearance text told us.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Parsed {
    pub squawk: Option<String>,
    pub initial_altitude: Option<String>,
    pub sid: Option<String>,
    pub runway: Option<String>,
}

impl Parsed {
    pub fn is_empty(&self) -> bool {
        *self == Parsed::default()
    }
}

fn tokens(text: &str) -> Vec<String> {
    text.to_ascii_uppercase()
        .split(|c: char| c.is_whitespace() || c == '@' || c == ',' || c == ';')
        .map(|t| t.trim_end_matches(['.', ':']).trim_start_matches(':').to_string())
        .filter(|t| !t.is_empty())
        .collect()
}

fn is_squawk(t: &str) -> bool {
    t.len() == 4 && t.chars().all(|c| ('0'..='7').contains(&c))
}

fn is_runway(t: &str) -> bool {
    let digits: String = t.chars().take_while(|c| c.is_ascii_digit()).collect();
    let rest = &t[digits.len()..];
    (1..=2).contains(&digits.len()) && digits.parse::<u32>().is_ok_and(|n| (1..=36).contains(&n)) && matches!(rest, "" | "L" | "R" | "C")
}

/// SID names look like CPT3J, BPK5K, OBOKA1G: letters, one digit, optional letter.
fn is_sid(t: &str) -> bool {
    let letters = t.chars().take_while(|c| c.is_ascii_alphabetic()).count();
    let rest = &t[letters..];
    let mut r = rest.chars();
    (2..=6).contains(&letters) && r.next().is_some_and(|c| c.is_ascii_digit()) && r.as_str().len() <= 1 && r.all(|c| c.is_ascii_alphabetic())
}

fn altitude(t: &str, next: Option<&str>) -> Option<String> {
    if let Some(level) = t.strip_prefix("FL").filter(|l| (2..=3).contains(&l.len()) && l.chars().all(|c| c.is_ascii_digit())) {
        return Some(format!("FL{level:0>3}"));
    }
    let digits = t.trim_end_matches("FT");
    if (3..=5).contains(&digits.len()) && digits.chars().all(|c| c.is_ascii_digit()) {
        let ft: u32 = digits.parse().ok()?;
        if (500..=60_000).contains(&ft) && (t.ends_with("FT") || next == Some("FT") || ft.is_multiple_of(100)) {
            return Some(format!("{ft} ft"));
        }
    }
    None
}

/// Extracts clearance items from PDC/CPDLC text (free-form; formats vary by vACC).
pub fn parse(text: &str) -> Parsed {
    let t = tokens(text);
    let mut p = Parsed::default();
    for (i, word) in t.iter().enumerate() {
        let next = t.get(i + 1).map(String::as_str);
        let after = |n: usize| t.iter().skip(i + 1).take(n).map(String::as_str);
        match word.as_str() {
            "SQUAWK" | "SQK" | "SQ" | "SSR" | "CODE" | "XPDR" | "TRANSPONDER" => {
                if let Some(code) = after(2).find(|c| is_squawk(c)) {
                    p.squawk.get_or_insert(code.to_string());
                }
            }
            "RWY" | "RUNWAY" => {
                if let Some(rwy) = next.filter(|r| is_runway(r)) {
                    p.runway.get_or_insert(rwy.to_string());
                }
            }
            "SID" | "DEP" | "DEPARTURE" | "VIA" => {
                if let Some(sid) = next.filter(|s| is_sid(s)) {
                    p.sid.get_or_insert(sid.to_string());
                }
            }
            "CLIMB" | "CLB" | "INIT" | "INITIAL" | "MAINTAIN" | "MAINT" | "ALT" | "ALTITUDE" => {
                let window: Vec<&str> = after(5).collect();
                if let Some(alt) = window.iter().enumerate().find_map(|(j, w)| altitude(w, window.get(j + 1).copied())) {
                    p.initial_altitude.get_or_insert(alt);
                }
            }
            _ => {}
        }
    }
    p
}

impl FlightNotes {
    /// Applies a clearance (overwrites). Returns true if anything changed.
    pub fn apply_clearance(&mut self, p: &Parsed) -> bool {
        let before = self.clone();
        let set = |field: &mut String, v: &Option<String>| {
            if let Some(v) = v {
                *field = v.clone();
            }
        };
        set(&mut self.squawk, &p.squawk);
        set(&mut self.initial_altitude, &p.initial_altitude);
        set(&mut self.sid, &p.sid);
        set(&mut self.runway, &p.runway);
        *self != before
    }

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

/// The SID at the start of a SimBrief route string, if it starts with one.
pub fn sid_from_route(route: &str) -> Option<String> {
    route.split_whitespace().next().filter(|t| is_sid(t)).map(String::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_vsmr_style_pdc() {
        let p = parse("CLR TO @ENBR@ RWY @27R@ DEP @CPT3J@ INIT CLB @6000@ SQUAWK @4721@ WHEN RDY CALL FREQ @121.975@ IF UNABLE CALL VOICE");
        assert_eq!(p.squawk.as_deref(), Some("4721"));
        assert_eq!(p.runway.as_deref(), Some("27R"));
        assert_eq!(p.sid.as_deref(), Some("CPT3J"));
        assert_eq!(p.initial_altitude.as_deref(), Some("6000 ft"));
    }

    #[test]
    fn parses_other_formats() {
        let p = parse("PDC 001 BAW123 CLRD TO ENBR VIA SID BPK5K RWY 09L CLIMB VIA SID TO FL70 DEP FREQ 120.525 SQK 2341");
        assert_eq!(p.sid.as_deref(), Some("BPK5K"));
        assert_eq!(p.runway.as_deref(), Some("09L"));
        assert_eq!(p.initial_altitude.as_deref(), Some("FL070"));
        assert_eq!(p.squawk.as_deref(), Some("2341"));
        assert!(parse("CONTACT LONDON 129.425").is_empty());
        assert_eq!(parse("CLIMB TO @FL350@").initial_altitude.as_deref(), Some("FL350"));
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
        assert!(n.apply_clearance(&Parsed { runway: Some("09R".into()), ..Default::default() }));
        assert_eq!(n.runway, "09R", "a clearance overwrites");
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
    fn helpers() {
        assert_eq!(qnh_from_metar("EGLL 051720Z 25011KT 9999 NCD 21/14 Q1022").as_deref(), Some("Q1022"));
        assert_eq!(qnh_from_metar("KJFK 051751Z 18010KT 10SM FEW250 24/12 A2992 RMK").as_deref(), Some("A2992"));
        assert_eq!(sid_from_route("CPT3J CPT UL9 KENET").as_deref(), Some("CPT3J"));
        assert_eq!(sid_from_route("DCT CPT"), None);
    }
}
