//! Parses the classic vPilot/xPilot "dot commands" typed into the chat box.

use crate::radio::{is_valid_squawk, parse_com_khz};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Com {
    One,
    Two,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DotCommand {
    /// `.com1 122.8` / `.com2 121.5`
    Tune { com: Com, khz: i32 },
    /// `.x 4721` (also `.xpdr`, `.xpndr`, `.sq`, `.squawk`)
    Squawk(i32),
    /// `.msg CALLSIGN text…` / `.chat CALLSIGN`
    PrivateMessage { to: String, text: Option<String> },
    /// `.atis EGLL_ATIS`
    Atis(String),
    /// `.wx EGLL` / `.metar EGLL`
    Metar(String),
    /// `.clear`
    Clear,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ParseError {
    NotACommand,
    Unknown(String),
    Usage(&'static str),
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ParseError::NotACommand => write!(f, "not a dot command"),
            ParseError::Unknown(name) => write!(f, "unknown command .{name}"),
            ParseError::Usage(usage) => write!(f, "usage: {usage}"),
        }
    }
}

pub fn parse(input: &str) -> Result<DotCommand, ParseError> {
    let input = input.trim();
    let Some(rest) = input.strip_prefix('.') else { return Err(ParseError::NotACommand) };
    let mut parts = rest.splitn(2, char::is_whitespace);
    let name = parts.next().unwrap_or_default().to_ascii_lowercase();
    let args = parts.next().unwrap_or_default().trim();
    let first = args.split_whitespace().next();

    match name.as_str() {
        "com1" | "com2" => {
            let com = if name == "com1" { Com::One } else { Com::Two };
            let khz = first.and_then(parse_com_khz).ok_or(ParseError::Usage(".com1 122.800"))?;
            Ok(DotCommand::Tune { com, khz })
        }
        "x" | "xpdr" | "xpndr" | "sq" | "squawk" => {
            let code = first.filter(|c| is_valid_squawk(c)).ok_or(ParseError::Usage(".x 7000"))?;
            Ok(DotCommand::Squawk(code.parse().expect("validated digits")))
        }
        "msg" | "chat" => {
            let mut split = args.splitn(2, char::is_whitespace);
            let to = split.next().filter(|s| !s.is_empty()).ok_or(ParseError::Usage(".msg CALLSIGN message"))?;
            let text = split.next().map(str::trim).filter(|s| !s.is_empty()).map(String::from);
            Ok(DotCommand::PrivateMessage { to: to.to_ascii_uppercase(), text })
        }
        "atis" => first.map(|s| DotCommand::Atis(s.to_ascii_uppercase())).ok_or(ParseError::Usage(".atis EGLL_ATIS")),
        "wx" | "metar" => first.map(|s| DotCommand::Metar(s.to_ascii_uppercase())).ok_or(ParseError::Usage(".wx EGLL")),
        "clear" => Ok(DotCommand::Clear),
        _ => Err(ParseError::Unknown(name)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tunes() {
        assert_eq!(parse(".com1 122.8"), Ok(DotCommand::Tune { com: Com::One, khz: 122_800 }));
        assert_eq!(parse(".COM2 121.505"), Ok(DotCommand::Tune { com: Com::Two, khz: 121_505 }));
        assert_eq!(parse(".com1"), Err(ParseError::Usage(".com1 122.800")));
        assert_eq!(parse(".com1 150.0"), Err(ParseError::Usage(".com1 122.800")));
    }

    #[test]
    fn squawks() {
        assert_eq!(parse(".x 4721"), Ok(DotCommand::Squawk(4721)));
        assert_eq!(parse(".squawk 0012"), Ok(DotCommand::Squawk(12)));
        assert!(parse(".x 8000").is_err());
    }

    #[test]
    fn messages() {
        assert_eq!(parse(".msg baw123 hello there"), Ok(DotCommand::PrivateMessage { to: "BAW123".into(), text: Some("hello there".into()) }));
        assert_eq!(parse(".chat EGLL_TWR"), Ok(DotCommand::PrivateMessage { to: "EGLL_TWR".into(), text: None }));
    }

    #[test]
    fn misc() {
        assert_eq!(parse(".wx egll"), Ok(DotCommand::Metar("EGLL".into())));
        assert_eq!(parse(".clear"), Ok(DotCommand::Clear));
        assert_eq!(parse("hello"), Err(ParseError::NotACommand));
        assert_eq!(parse(".nope"), Err(ParseError::Unknown("nope".into())));
    }
}
