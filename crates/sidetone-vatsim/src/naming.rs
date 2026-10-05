//! Turns a VATSIM callsign into the name pilots say on the radio.
//!
//! Priority: a name the controller wrote in their info text (when it fits the facility) →
//! VATSpy FIR names for area stations → airport name + facility word → the callsign.

use crate::vatspy::VatSpy;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Facility {
    Center,
    Fss,
    Approach,
    Departure,
    Tower,
    Ground,
    Apron,
    Delivery,
    Atis,
    Other,
}

impl Facility {
    pub fn from_callsign(callsign: &str) -> Facility {
        match callsign.rsplit('_').next().unwrap_or_default().to_ascii_uppercase().as_str() {
            "CTR" => Facility::Center,
            "FSS" => Facility::Fss,
            "APP" => Facility::Approach,
            "DEP" => Facility::Departure,
            "TWR" => Facility::Tower,
            "GND" => Facility::Ground,
            "RMP" => Facility::Apron,
            "DEL" => Facility::Delivery,
            "ATIS" => Facility::Atis,
            _ => Facility::Other,
        }
    }

    pub fn short(self) -> &'static str {
        match self {
            Facility::Center => "CTR",
            Facility::Fss => "FSS",
            Facility::Approach => "APP",
            Facility::Departure => "DEP",
            Facility::Tower => "TWR",
            Facility::Ground => "GND",
            Facility::Apron => "RMP",
            Facility::Delivery => "DEL",
            Facility::Atis => "ATIS",
            Facility::Other => "",
        }
    }

    /// Default spoken word for airport-based facilities.
    fn word(self) -> Option<&'static str> {
        Some(match self {
            Facility::Approach => "Approach",
            Facility::Departure => "Departure",
            Facility::Tower => "Tower",
            Facility::Ground => "Ground",
            Facility::Apron => "Apron",
            Facility::Delivery => "Delivery",
            Facility::Atis => "Information",
            Facility::Fss => "Radio",
            Facility::Center | Facility::Other => return None,
        })
    }

    /// Radio words a controller of this facility could plausibly be called.
    fn accepts(self, word: &str) -> bool {
        let ok: &[&str] = match self {
            Facility::Tower => &["Tower"],
            Facility::Ground | Facility::Apron => &["Ground", "Apron", "Ramp"],
            Facility::Delivery => &["Delivery", "Clearance"],
            Facility::Approach => &["Approach", "Director", "Radar", "Arrival"],
            Facility::Departure => &["Departure", "Radar", "Approach"],
            Facility::Center => &["Control", "Centre", "Center", "Radar"],
            Facility::Fss => &["Radio", "Control"],
            Facility::Atis => &["Information"],
            Facility::Other => RADIO_WORDS,
        };
        ok.contains(&word)
    }
}

const RADIO_WORDS: &[&str] = &[
    "Delivery",
    "Clearance",
    "Ground",
    "Apron",
    "Ramp",
    "Tower",
    "Approach",
    "Departure",
    "Director",
    "Arrival",
    "Radar",
    "Control",
    "Centre",
    "Center",
    "Radio",
    "Information",
];

/// Words that end a name when scanning backwards from the radio word.
const STOP_WORDS: &[&str] = &[
    "THIS", "IS", "CONTACT", "MONITOR", "OR", "AND", "FOR", "ON", "THE", "CALL", "CALLSIGN", "RTF", "STATION", "WITH", "TO", "VIA", "AT", "OF", "ATIS",
    "AIRPORT", "INTL",
];

/// Title-cases a word written in all caps ("FLESLAND" → "Flesland"), leaving mixed case alone.
fn tidy(word: &str) -> String {
    if word.chars().any(|c| c.is_lowercase()) {
        return word.to_string();
    }
    word.split('-')
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().chain(chars.flat_map(|c| c.to_lowercase())).collect(),
                None => String::new(),
            }
        })
        .collect::<Vec<String>>()
        .join("-")
}

/// Finds "`<Place> <RadioWord>`" in the first line of a controller's info text.
pub fn name_from_text(text: &str, facility: Facility) -> Option<String> {
    let first_line = text.lines().next().unwrap_or(text);
    for segment in first_line.split(['|', '"', '(', ')', ',', ';', '/']).flat_map(|s| s.split(" - ")).flat_map(|s| s.split("..")) {
        let words: Vec<&str> = segment.split_whitespace().map(|w| w.trim_matches(|c: char| !c.is_alphanumeric() && c != '-')).collect();
        for (i, word) in words.iter().enumerate() {
            let radio = tidy(word);
            if !RADIO_WORDS.contains(&radio.as_str()) || !facility.accepts(&radio) {
                continue;
            }
            let mut place: Vec<String> = Vec::new();
            for prev in words[..i].iter().rev().take(3) {
                let upper = prev.to_ascii_uppercase();
                let is_radio_word = RADIO_WORDS.contains(&tidy(prev).as_str());
                let is_code = prev.len() == 4 && prev.chars().all(|c| c.is_ascii_uppercase());
                if prev.is_empty() || is_radio_word || is_code || STOP_WORDS.contains(&upper.as_str()) || !prev.chars().all(|c| c.is_alphabetic() || c == '-') {
                    break;
                }
                place.insert(0, tidy(prev));
            }
            if !place.is_empty() {
                return Some(format!("{} {radio}", place.join(" ")));
            }
        }
    }
    None
}

/// Shortens a VATSpy airport name to what's said on the radio ("London Heathrow" → "Heathrow").
fn short_airport_name(name: &str, fir_city: Option<&str>) -> String {
    // "Moscow (Sheremetyevo)" → "Sheremetyevo"
    if let Some((_, inner)) = name.split_once('(')
        && let Some((inner, _)) = inner.split_once(')')
        && !inner.trim().is_empty()
    {
        return inner.trim().to_string();
    }
    let mut name = name.rsplit('/').next().unwrap_or(name).trim().to_string();
    if let Some((_, after)) = name.split_once('-') {
        name = after.trim().to_string();
    }
    let mut words: Vec<&str> = name.split_whitespace().collect();
    while let Some(last) = words.last() {
        let is_state = last.len() == 2 && last.chars().all(|c| c.is_ascii_uppercase());
        if is_state || ["Intl", "International", "Airport", "Regional", "Municipal", "Field", "Airfield"].contains(last) {
            words.pop();
        } else {
            break;
        }
    }
    if let Some(city) = fir_city
        && words.len() > 1
        && words[0].eq_ignore_ascii_case(city)
    {
        words.remove(0);
    }
    if words.len() >= 3 {
        // "John F. Kennedy" → "Kennedy"; otherwise the city usually leads ("Lexington Blue Grass").
        let has_initial = words.iter().any(|w| w.ends_with('.') || w.len() == 1);
        return if has_initial { words.last() } else { words.first() }.unwrap().to_string();
    }
    if words.is_empty() { name } else { words.join(" ") }
}

fn ends_with_radio_word(name: &str) -> bool {
    name.split_whitespace().last().is_some_and(|w| RADIO_WORDS.contains(&w))
}

fn area_word(vatspy: &VatSpy, fir_icao: &str) -> String {
    let two: String = fir_icao.chars().take(2).collect();
    let one: String = fir_icao.chars().take(1).collect();
    vatspy
        .country_suffix
        .get(&two)
        .or_else(|| vatspy.country_suffix.get(&one))
        .cloned()
        .unwrap_or_else(|| if matches!(one.as_str(), "K" | "P" | "C") { "Center".into() } else { "Control".into() })
}

/// Where a station's spoken name came from, so the UI can show its provenance.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NameSource {
    /// The controller's own info text (the line it came from).
    InfoText(String),
    /// VATSpy airport/FIR data plus the facility type.
    VatSpy,
    /// Nothing better was known.
    Callsign,
}

impl NameSource {
    /// One line for a tooltip.
    pub fn describe(&self) -> String {
        match self {
            NameSource::InfoText(line) => format!("Name from the controller's info: \"{line}\""),
            NameSource::VatSpy => "Name from VATSpy airport/sector data".into(),
            NameSource::Callsign => "No name known; showing the callsign".into(),
        }
    }
}

/// The spoken name for a station.
pub fn station_name(callsign: &str, text: Option<&[String]>, vatspy: Option<&VatSpy>) -> String {
    station_name_with_source(callsign, text, vatspy).0
}

/// The spoken name for a station, and where it came from.
pub fn station_name_with_source(callsign: &str, text: Option<&[String]>, vatspy: Option<&VatSpy>) -> (String, NameSource) {
    let (name, source) = base_station_name(callsign, text, vatspy);
    // Split ATIS: EDDF_A_ATIS / EDDF_D_ATIS are the arrival and departure broadcasts.
    let parts: Vec<&str> = callsign.split('_').collect();
    let name = match (Facility::from_callsign(callsign), parts.as_slice()) {
        (Facility::Atis, [_, "A", _]) if !name.contains("Arrival") => name.replacen(" Information", " Arrival Information", 1),
        (Facility::Atis, [_, "D", _]) if !name.contains("Departure") => name.replacen(" Information", " Departure Information", 1),
        _ => name,
    };
    (name, source)
}

fn base_station_name(callsign: &str, text: Option<&[String]>, vatspy: Option<&VatSpy>) -> (String, NameSource) {
    let facility = Facility::from_callsign(callsign);
    if let Some(line) = text.and_then(|t| t.first())
        && let Some(name) = name_from_text(line, facility)
    {
        return (name, NameSource::InfoText(line.trim().to_string()));
    }
    let Some(vatspy) = vatspy else { return (callsign.to_string(), NameSource::Callsign) };
    let prefix = callsign.rsplit_once('_').map(|(p, _)| p).unwrap_or(callsign);

    match facility {
        Facility::Center | Facility::Fss => {
            if let Some(fir) = vatspy.fir_for_prefix(prefix) {
                // "Kobenhavn (East)" → "Kobenhavn"
                let fir_name = fir.name.split('(').next().unwrap_or(&fir.name).trim();
                if ends_with_radio_word(fir_name) {
                    return (fir_name.to_string(), NameSource::VatSpy);
                }
                let word = if facility == Facility::Fss { "Radio".to_string() } else { area_word(vatspy, &fir.icao) };
                return (format!("{fir_name} {word}"), NameSource::VatSpy);
            }
            if let Some(name) = vatspy.uirs.get(prefix) {
                return (name.clone(), NameSource::VatSpy);
            }
        }
        Facility::Other => {}
        _ => {
            let airport_id = prefix.split('_').next().unwrap_or(prefix);
            if let Some(airport) = vatspy.airport(airport_id) {
                let fir_city = vatspy.firs.iter().find(|f| f.icao == airport.fir).map(|f| f.name.as_str());
                let word = facility.word().unwrap_or_default();
                return (format!("{} {word}", short_airport_name(&airport.name, fir_city)), NameSource::VatSpy);
            }
        }
    }
    (callsign.to_string(), NameSource::Callsign)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vatspy::SAMPLE;

    fn spy() -> VatSpy {
        VatSpy::parse(SAMPLE)
    }

    #[test]
    fn facilities() {
        assert_eq!(Facility::from_callsign("LON_S_CTR"), Facility::Center);
        assert_eq!(Facility::from_callsign("EGLL_ATIS"), Facility::Atis);
        assert_eq!(Facility::from_callsign("BAW123"), Facility::Other);
    }

    #[test]
    fn names_from_info_text() {
        assert_eq!(name_from_text("Flesland Tower | DCL - ENBR", Facility::Tower).as_deref(), Some("Flesland Tower"));
        assert_eq!(name_from_text("\"Shanwick Radio\" or \"Gander Radio\" | CPDLC (NATX)", Facility::Fss).as_deref(), Some("Shanwick Radio"));
        assert_eq!(name_from_text("THIS IS FLESLAND INFORMATION CHARLIE .. TIME 1720", Facility::Atis).as_deref(), Some("Flesland Information"));
        assert_eq!(name_from_text("London Control (South) | Feedback welcome", Facility::Center).as_deref(), Some("London Control"));
        // A tower mentioning the area controller is not called "London Control".
        assert_eq!(name_from_text("After departure contact London Control", Facility::Tower), None);
        assert_eq!(name_from_text("Expected Logoff Time 2000z", Facility::Approach), None);
        assert_eq!(name_from_text("Callsign Bremen Radar", Facility::Approach).as_deref(), Some("Bremen Radar"));
        assert_eq!(name_from_text("BRUSSELS-NATIONAL INFORMATION B", Facility::Atis).as_deref(), Some("Brussels-National Information"));
        assert_eq!(name_from_text("THIS IS EGCC DEPARTURE INFORMATION", Facility::Atis), None);
        assert_eq!(name_from_text("LEXINGTON TOWER ATIS INFORMATION", Facility::Atis), None);
    }

    #[test]
    fn names_from_vatspy() {
        let v = spy();
        assert_eq!(station_name("EGLL_TWR", None, Some(&v)), "Heathrow Tower");
        assert_eq!(station_name("EGKK_GND", None, Some(&v)), "Gatwick Ground");
        assert_eq!(station_name("EDDF_APP", None, Some(&v)), "Frankfurt Approach");
        assert_eq!(station_name("ENBR_DEL", None, Some(&v)), "Flesland Delivery");
        assert_eq!(station_name("JFK_TWR", None, Some(&v)), "Kennedy Tower");
        assert_eq!(station_name("LON_S_CTR", None, Some(&v)), "London Control");
        assert_eq!(station_name("EDGG_CTR", None, Some(&v)), "Langen Radar");
        assert_eq!(station_name("NY_CTR", None, Some(&v)), "New York Center");
        assert_eq!(station_name("ADR_CTR", None, Some(&v)), "Adria Radar");
        assert_eq!(station_name("EGLL_ATIS", None, Some(&v)), "Heathrow Information");
        assert_eq!(station_name("EGLL_A_ATIS", None, Some(&v)), "Heathrow Arrival Information");
        assert_eq!(station_name("EGLL_D_ATIS", None, Some(&v)), "Heathrow Departure Information");
        assert_eq!(station_name("ZZZZ_TWR", None, Some(&v)), "ZZZZ_TWR");
        assert_eq!(station_name("EGLL_TWR", None, None), "EGLL_TWR");
        assert_eq!(short_airport_name("Moscow (Sheremetyevo)", None), "Sheremetyevo");
        assert_eq!(short_airport_name("Lexington Blue Grass", None), "Lexington");
    }

    #[test]
    fn records_where_names_came_from() {
        let v = spy();
        let text = vec!["Callsign BERLIN APRON - PDC/DCL Logon EDDB".to_string()];
        let (name, source) = station_name_with_source("EDDB_A_GND", Some(&text), Some(&v));
        assert_eq!(name, "Berlin Apron");
        assert_eq!(source, NameSource::InfoText("Callsign BERLIN APRON - PDC/DCL Logon EDDB".into()));
        assert_eq!(station_name_with_source("EGLL_TWR", None, Some(&v)).1, NameSource::VatSpy);
        assert_eq!(station_name_with_source("ZZZZ_TWR", None, Some(&v)).1, NameSource::Callsign);
    }

    #[test]
    fn info_text_wins() {
        let v = spy();
        let text = vec!["Flesland Tower | DCL".to_string()];
        assert_eq!(station_name("ENBR_TWR", Some(&text), Some(&v)), "Flesland Tower");
    }
}
