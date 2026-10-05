//! The pilot side of a CPDLC/ACARS session: logon to an ATC unit, handovers, logoff, answering
//! uplinks and tracking which messages are still open. Pure state — the worker does the I/O.

use super::protocol::{Cpdlc, Incoming, Kind, Outgoing, ResponseAttr, display_text, pdc_request};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LogonState {
    Off,
    /// Logon requested; waiting for the station to accept.
    Pending {
        station: String,
    },
    Connected {
        station: String,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    Up,
    Down,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Message {
    /// Local id for the UI.
    pub id: u64,
    pub direction: Direction,
    pub station: String,
    pub kind: Kind,
    /// CPDLC message number (ours for downlinks, theirs for uplinks).
    pub min: Option<u32>,
    pub ra: ResponseAttr,
    pub text: String,
    /// Unix seconds.
    pub time: u64,
    /// The pilot's reply to an uplink, once sent ("WILCO").
    pub reply: Option<String>,
    /// For downlinks: ATC's answer has arrived.
    pub answered: bool,
}

impl Message {
    /// An uplink still waiting for the pilot (STANDBY keeps it open).
    pub fn needs_reply(&self) -> bool {
        self.direction == Direction::Up && !self.ra.replies().is_empty() && self.reply.as_deref().is_none_or(|r| r == "STANDBY")
    }
}

/// Something the UI should tell the pilot about.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Notice {
    Uplink { station: String, text: String },
    LoggedOn(String),
    LogonRejected(String),
    LoggedOff(String),
}

pub struct Session {
    pub callsign: String,
    pub state: LogonState,
    pub messages: Vec<Message>,
    next_min: u32,
    next_id: u64,
}

impl Default for Session {
    fn default() -> Self {
        Session { callsign: String::new(), state: LogonState::Off, messages: Vec::new(), next_min: 1, next_id: 1 }
    }
}

fn now_secs() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

impl Session {
    pub fn station(&self) -> Option<&str> {
        match &self.state {
            LogonState::Off => None,
            LogonState::Pending { station } | LogonState::Connected { station } => Some(station),
        }
    }

    pub fn open_uplinks(&self) -> usize {
        self.messages.iter().filter(|m| m.needs_reply()).count()
    }

    fn take_min(&mut self) -> u32 {
        let min = self.next_min;
        self.next_min = if self.next_min >= 63 { 1 } else { self.next_min + 1 };
        min
    }

    fn record(&mut self, direction: Direction, station: &str, kind: Kind, min: Option<u32>, ra: ResponseAttr, text: &str) {
        let id = self.next_id;
        self.next_id += 1;
        self.messages.push(Message {
            id,
            direction,
            station: station.to_string(),
            kind,
            min,
            ra,
            text: display_text(text),
            time: now_secs(),
            reply: None,
            answered: false,
        });
    }

    fn cpdlc(&mut self, to: &str, mrn: Option<u32>, ra: ResponseAttr, text: &str) -> Outgoing {
        let min = self.take_min();
        self.record(Direction::Down, to, Kind::Cpdlc, Some(min), ra, text);
        Outgoing { to: to.to_string(), kind: Kind::Cpdlc, packet: Cpdlc { min, mrn, ra, text: text.to_string() }.encode() }
    }

    pub fn logon(&mut self, station: &str) -> Outgoing {
        let station = station.trim().to_ascii_uppercase();
        self.state = LogonState::Pending { station: station.clone() };
        self.cpdlc(&station, None, ResponseAttr::Yes, "REQUEST LOGON")
    }

    pub fn logoff(&mut self) -> Option<Outgoing> {
        let station = self.station()?.to_string();
        self.state = LogonState::Off;
        Some(self.cpdlc(&station, None, ResponseAttr::No, "LOGOFF"))
    }

    /// A free-text or structured request to the current station ("REQUEST CLIMB TO FL350").
    pub fn request(&mut self, text: &str) -> Option<Outgoing> {
        let station = match &self.state {
            LogonState::Connected { station } => station.clone(),
            _ => return None,
        };
        Some(self.cpdlc(&station, None, ResponseAttr::Yes, &text.trim().to_ascii_uppercase()))
    }

    /// Replies to an uplink by local id.
    pub fn reply(&mut self, id: u64, reply: &str) -> Option<Outgoing> {
        let index = self.messages.iter().position(|m| m.id == id && m.direction == Direction::Up)?;
        let (station, mrn) = (self.messages[index].station.clone(), self.messages[index].min);
        self.messages[index].reply = Some(reply.to_string());
        Some(self.cpdlc(&station, mrn, ResponseAttr::No, reply))
    }

    /// Sends a pre-departure clearance request telex to the departure airport's station.
    #[allow(clippy::too_many_arguments)]
    pub fn request_pdc(&mut self, station: &str, aircraft: &str, departure: &str, destination: &str, stand: &str, atis: &str) -> Outgoing {
        let station = station.trim().to_ascii_uppercase();
        let text = pdc_request(&self.callsign, aircraft, departure, destination, stand, atis);
        self.record(Direction::Down, &station, Kind::Telex, None, ResponseAttr::No, &text);
        Outgoing { to: station, kind: Kind::Telex, packet: text }
    }

    pub fn telex(&mut self, to: &str, text: &str) -> Outgoing {
        let to = to.trim().to_ascii_uppercase();
        let text = text.trim().to_ascii_uppercase();
        self.record(Direction::Down, &to, Kind::Telex, None, ResponseAttr::No, &text);
        Outgoing { to, kind: Kind::Telex, packet: text }
    }

    /// Applies received messages. Returns notices, plus any automatic replies to send
    /// (a HANDOVER triggers a logon to the next unit, as real FANS avionics do).
    pub fn on_incoming(&mut self, incoming: Vec<Incoming>) -> (Vec<Notice>, Vec<Outgoing>) {
        let mut notices = Vec::new();
        let mut outgoing = Vec::new();
        for msg in incoming {
            match msg.kind {
                Kind::Cpdlc => {
                    let Some(c) = Cpdlc::parse(&msg.packet) else { continue };
                    if let Some(mrn) = c.mrn
                        && let Some(down) = self.messages.iter_mut().rev().find(|m| m.direction == Direction::Down && m.min == Some(mrn))
                    {
                        down.answered = true;
                    }
                    let upper = c.text.to_ascii_uppercase();
                    let pending = matches!(&self.state, LogonState::Pending { station } if *station == msg.from);
                    if pending && upper.contains("LOGON ACCEPTED") {
                        self.state = LogonState::Connected { station: msg.from.clone() };
                        notices.push(Notice::LoggedOn(msg.from.clone()));
                    } else if pending && (upper.starts_with("UNABLE") || upper.contains("LOGON REJECTED")) {
                        self.state = LogonState::Off;
                        notices.push(Notice::LogonRejected(msg.from.clone()));
                    } else if upper.starts_with("HANDOVER") {
                        let next: String =
                            display_text(&c.text).trim_start_matches("HANDOVER").trim().chars().take_while(|c| c.is_ascii_alphanumeric()).collect();
                        if !next.is_empty() {
                            outgoing.push(self.logon(&next));
                        }
                    } else if upper.contains("LOGOFF") || upper.contains("SERVICE TERMINATED") {
                        if self.station() == Some(msg.from.as_str()) {
                            self.state = LogonState::Off;
                            notices.push(Notice::LoggedOff(msg.from.clone()));
                        }
                    } else if upper.starts_with("CURRENT ATC UNIT") {
                        let unit = c.text.split("@_@").nth(1).map(|s| s.replace('@', "").trim().to_string());
                        if let Some(unit) = unit.filter(|u| !u.is_empty()) {
                            self.state = LogonState::Connected { station: unit };
                        }
                    }
                    self.record(Direction::Up, &msg.from, Kind::Cpdlc, Some(c.min), c.ra, &c.text);
                    if !c.ra.replies().is_empty() || !upper.contains("LOGON ACCEPTED") {
                        notices.push(Notice::Uplink { station: msg.from.clone(), text: display_text(&c.text) });
                    }
                }
                Kind::Telex | Kind::Other => {
                    self.record(Direction::Up, &msg.from, Kind::Telex, None, ResponseAttr::No, &msg.packet);
                    notices.push(Notice::Uplink { station: msg.from.clone(), text: display_text(&msg.packet) });
                }
                Kind::Progress => {}
            }
        }
        (notices, outgoing)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn up(from: &str, packet: &str) -> Incoming {
        Incoming { from: from.into(), kind: Kind::Cpdlc, packet: packet.into() }
    }

    #[test]
    fn logon_accept_and_reply() {
        let mut s = Session { callsign: "BAW123".into(), ..Default::default() };
        let out = s.logon("egtt");
        assert_eq!(out.to, "EGTT");
        assert_eq!(out.packet, "/data2/1//Y/REQUEST LOGON");
        assert_eq!(s.state, LogonState::Pending { station: "EGTT".into() });

        let (notices, _) = s.on_incoming(vec![up("EGTT", "/data2/7/1/NE/LOGON ACCEPTED")]);
        assert_eq!(notices, vec![Notice::LoggedOn("EGTT".into())]);
        assert_eq!(s.state, LogonState::Connected { station: "EGTT".into() });
        assert!(s.messages[0].answered);

        s.on_incoming(vec![up("EGTT", "/data2/8//WU/CLIMB TO @FL350@")]);
        assert_eq!(s.open_uplinks(), 1);
        let id = s.messages.last().unwrap().id;
        let out = s.reply(id, "STANDBY").unwrap();
        assert_eq!(out.packet, "/data2/2/8/N/STANDBY");
        assert_eq!(s.open_uplinks(), 1, "STANDBY keeps it open");
        let out = s.reply(id, "WILCO").unwrap();
        assert_eq!(out.packet, "/data2/3/8/N/WILCO");
        assert_eq!(s.open_uplinks(), 0);
    }

    #[test]
    fn handover_logs_on_to_next_unit() {
        let mut s = Session::default();
        s.logon("EGTT");
        s.on_incoming(vec![up("EGTT", "/data2/2/1/NE/LOGON ACCEPTED")]);
        let (_, auto) = s.on_incoming(vec![up("EGTT", "/data2/3//NE/HANDOVER @EGPX")]);
        assert_eq!(auto.len(), 1);
        assert_eq!(auto[0].to, "EGPX");
        assert_eq!(s.state, LogonState::Pending { station: "EGPX".into() });
    }

    #[test]
    fn rejected_and_logoff() {
        let mut s = Session::default();
        s.logon("EDYY");
        let (n, _) = s.on_incoming(vec![up("EDYY", "/data2/9/1/NE/UNABLE")]);
        assert!(n.contains(&Notice::LogonRejected("EDYY".into())));
        assert_eq!(s.state, LogonState::Off);

        s.logon("EDYY");
        s.on_incoming(vec![up("EDYY", "/data2/10/2/NE/LOGON ACCEPTED")]);
        let (n, _) = s.on_incoming(vec![up("EDYY", "/data2/11//N/LOGOFF")]);
        assert!(n.contains(&Notice::LoggedOff("EDYY".into())));
        assert_eq!(s.state, LogonState::Off);
    }

    #[test]
    fn requests_need_a_connection() {
        let mut s = Session::default();
        assert!(s.request("request climb to fl350").is_none());
        s.logon("EGTT");
        s.on_incoming(vec![up("EGTT", "/data2/2/1/NE/LOGON ACCEPTED")]);
        assert_eq!(s.request("request climb to fl350").unwrap().packet, "/data2/2//Y/REQUEST CLIMB TO FL350");
    }

    #[test]
    fn telex_uplinks_are_recorded() {
        let mut s = Session { callsign: "BAW123".into(), ..Default::default() };
        let out = s.request_pdc("egll", "A20N", "EGLL", "ENBR", "", "C");
        assert_eq!(out.kind, Kind::Telex);
        let (n, _) = s.on_incoming(vec![Incoming { from: "EGLL".into(), kind: Kind::Telex, packet: "CLD 1200 EGLL PDC 001 @BAW123@ CLRD TO @ENBR@".into() }]);
        assert_eq!(n.len(), 1);
        assert_eq!(s.messages.last().unwrap().text, "CLD 1200 EGLL PDC 001 BAW123 CLRD TO ENBR");
    }
}
