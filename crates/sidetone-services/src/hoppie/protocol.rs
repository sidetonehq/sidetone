//! Hoppie wire format.
//!
//! Requests: `connect.html?logon=…&from=…&to=…&type=…&packet=…`.
//! Responses: `ok`, `ok {FROM type {packet}} {FROM type {packet}}`, or `error {reason}`.
//! CPDLC packets: `/data2/<msg id>/<replying to id>/<response attribute>/<text>`, where `@` marks
//! variable fields and `@_@` is a line break.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Cpdlc,
    Telex,
    Progress,
    Other,
}

impl Kind {
    pub fn wire(self) -> &'static str {
        match self {
            Kind::Cpdlc => "cpdlc",
            Kind::Telex => "telex",
            Kind::Progress => "progress",
            Kind::Other => "telex",
        }
    }

    fn parse(s: &str) -> Kind {
        match s.to_ascii_lowercase().as_str() {
            "cpdlc" => Kind::Cpdlc,
            "telex" => Kind::Telex,
            "progress" => Kind::Progress,
            _ => Kind::Other,
        }
    }
}

/// What a CPDLC message expects back.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResponseAttr {
    /// WILCO / UNABLE (/ STANDBY)
    WilcoUnable,
    /// AFFIRM / NEGATIVE (/ STANDBY)
    AffirmNegative,
    /// ROGER (/ UNABLE / STANDBY)
    Roger,
    /// Any reply.
    Yes,
    /// No reply.
    No,
    /// Not enabled (informational).
    NotEnabled,
}

impl ResponseAttr {
    pub fn parse(s: &str) -> ResponseAttr {
        match s.to_ascii_uppercase().as_str() {
            "WU" => ResponseAttr::WilcoUnable,
            "AN" => ResponseAttr::AffirmNegative,
            "R" => ResponseAttr::Roger,
            "Y" => ResponseAttr::Yes,
            "NE" => ResponseAttr::NotEnabled,
            _ => ResponseAttr::No,
        }
    }

    pub fn wire(self) -> &'static str {
        match self {
            ResponseAttr::WilcoUnable => "WU",
            ResponseAttr::AffirmNegative => "AN",
            ResponseAttr::Roger => "R",
            ResponseAttr::Yes => "Y",
            ResponseAttr::No => "N",
            ResponseAttr::NotEnabled => "NE",
        }
    }

    /// The standard replies a pilot can pick.
    pub fn replies(self) -> &'static [&'static str] {
        match self {
            ResponseAttr::WilcoUnable => &["WILCO", "UNABLE", "STANDBY"],
            ResponseAttr::AffirmNegative => &["AFFIRM", "NEGATIVE", "STANDBY"],
            ResponseAttr::Roger => &["ROGER", "UNABLE", "STANDBY"],
            ResponseAttr::Yes => &["ROGER", "STANDBY"],
            ResponseAttr::No | ResponseAttr::NotEnabled => &[],
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Cpdlc {
    pub min: u32,
    pub mrn: Option<u32>,
    pub ra: ResponseAttr,
    /// Raw text, with `@` field markers.
    pub text: String,
}

impl Cpdlc {
    pub fn parse(packet: &str) -> Option<Cpdlc> {
        let rest = packet.trim().strip_prefix("/data2/")?;
        let mut parts = rest.splitn(4, '/');
        let min = parts.next()?.parse().ok()?;
        let mrn = parts.next()?.parse().ok();
        let ra = ResponseAttr::parse(parts.next()?);
        let text = parts.next().unwrap_or_default().to_string();
        Some(Cpdlc { min, mrn, ra, text })
    }

    pub fn encode(&self) -> String {
        let mrn = self.mrn.map(|m| m.to_string()).unwrap_or_default();
        format!("/data2/{}/{}/{}/{}", self.min, mrn, self.ra.wire(), self.text)
    }
}

/// Human-readable message text: `@_@` → line break, `@` markers removed.
pub fn display_text(raw: &str) -> String {
    raw.replace("@_@", "\n").replace('@', "").trim().to_string()
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Incoming {
    pub from: String,
    pub kind: Kind,
    pub packet: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Outgoing {
    pub to: String,
    pub kind: Kind,
    pub packet: String,
}

/// Parses a server response. `Err` carries Hoppie's error text.
pub fn parse_response(body: &str) -> Result<Vec<Incoming>, String> {
    let body = body.trim();
    if let Some(rest) = body.strip_prefix("error") {
        return Err(unbrace(rest.trim()).to_string());
    }
    let Some(rest) = body.strip_prefix("ok") else {
        return Err(format!("unexpected response: {}", body.chars().take(80).collect::<String>()));
    };
    let mut out = Vec::new();
    for message in top_level_groups(rest) {
        // "FROM type {packet}"
        let mut head = message.splitn(3, char::is_whitespace);
        let (Some(from), Some(kind)) = (head.next(), head.next()) else { continue };
        let packet = head.next().map(|p| unbrace(p.trim())).unwrap_or_default();
        out.push(Incoming { from: from.to_string(), kind: Kind::parse(kind), packet: packet.to_string() });
    }
    Ok(out)
}

fn unbrace(s: &str) -> &str {
    s.strip_prefix('{').and_then(|s| s.strip_suffix('}')).unwrap_or(s)
}

/// Contents of each top-level `{…}` group, respecting nesting.
fn top_level_groups(s: &str) -> Vec<&str> {
    let mut groups = Vec::new();
    let mut depth = 0usize;
    let mut start = 0;
    for (i, c) in s.char_indices() {
        match c {
            '{' => {
                if depth == 0 {
                    start = i + 1;
                }
                depth += 1;
            }
            '}' if depth > 0 => {
                depth -= 1;
                if depth == 0 {
                    groups.push(&s[start..i]);
                }
            }
            _ => {}
        }
    }
    groups
}

/// The de-facto VATSIM pre-departure clearance request telex.
pub fn pdc_request(callsign: &str, aircraft: &str, departure: &str, destination: &str, stand: &str, atis: &str) -> String {
    let mut text = format!("REQUEST PREDEP CLEARANCE {callsign} {aircraft} TO {destination} AT {departure}");
    if !stand.trim().is_empty() {
        text.push_str(&format!(" STAND {}", stand.trim()));
    }
    if !atis.trim().is_empty() {
        text.push_str(&format!(" ATIS {}", atis.trim()));
    }
    text.to_ascii_uppercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_poll_responses() {
        assert_eq!(parse_response("ok"), Ok(vec![]));
        let body = "ok {EGTT cpdlc {/data2/12/3/WU/CLIMB TO @FL350@}} {EGLL telex {CLD 1234 EGLL PDC @BAW123@ CLRD TO @ENBR@}}";
        let msgs = parse_response(body).unwrap();
        assert_eq!(msgs.len(), 2);
        assert_eq!(msgs[0], Incoming { from: "EGTT".into(), kind: Kind::Cpdlc, packet: "/data2/12/3/WU/CLIMB TO @FL350@".into() });
        assert_eq!(msgs[1].kind, Kind::Telex);
        assert!(msgs[1].packet.starts_with("CLD 1234"));
        assert_eq!(parse_response("error {illegal logon code}"), Err("illegal logon code".into()));
    }

    #[test]
    fn cpdlc_round_trip() {
        let c = Cpdlc::parse("/data2/12/3/WU/CLIMB TO @FL350@").unwrap();
        assert_eq!((c.min, c.mrn, c.ra), (12, Some(3), ResponseAttr::WilcoUnable));
        assert_eq!(display_text(&c.text), "CLIMB TO FL350");
        let logon = Cpdlc { min: 1, mrn: None, ra: ResponseAttr::Yes, text: "REQUEST LOGON".into() };
        assert_eq!(logon.encode(), "/data2/1//Y/REQUEST LOGON");
        assert_eq!(Cpdlc::parse("/data2/5//NE/LOGON ACCEPTED").unwrap().mrn, None);
        assert_eq!(display_text("CURRENT ATC UNIT@_@EGTT@_@LONDON CTL"), "CURRENT ATC UNIT\nEGTT\nLONDON CTL");
    }

    #[test]
    fn pdc_text() {
        assert_eq!(pdc_request("baw123", "A20N", "EGLL", "ENBR", "512", "C"), "REQUEST PREDEP CLEARANCE BAW123 A20N TO ENBR AT EGLL STAND 512 ATIS C");
        assert_eq!(pdc_request("BAW1", "A20N", "EGLL", "ENBR", "", ""), "REQUEST PREDEP CLEARANCE BAW1 A20N TO ENBR AT EGLL");
    }
}
