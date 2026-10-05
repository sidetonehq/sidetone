//! Radio and transponder formatting.

/// Formats a COM frequency given in kHz (X-Plane's `*_frequency_hz_833` datarefs) as `122.800`.
pub fn format_com_khz(khz: i32) -> String {
    if khz <= 0 {
        return "---.---".into();
    }
    format!("{}.{:03}", khz / 1000, khz % 1000)
}

/// Parses `122.8`, `122.80`, `122.800` or `122800` into kHz. Rejects anything outside the VHF airband.
pub fn parse_com_khz(text: &str) -> Option<i32> {
    let text = text.trim();
    let khz = if let Some((mhz, frac)) = text.split_once('.') {
        if frac.is_empty() || frac.len() > 3 || !frac.chars().all(|c| c.is_ascii_digit()) {
            return None;
        }
        let mhz: i32 = mhz.parse().ok()?;
        let frac: i32 = format!("{frac:0<3}").parse().ok()?;
        mhz * 1000 + frac
    } else if text.len() == 6 {
        text.parse().ok()?
    } else {
        let mhz: i32 = text.parse().ok()?;
        mhz * 1000
    };
    (118_000..=136_990).contains(&khz).then_some(khz)
}

/// X-Plane transponder modes (`sim/cockpit2/radios/actuators/transponder_mode`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TransponderMode {
    #[default]
    Off,
    Standby,
    On,
    Alt,
    Test,
    Ground,
}

impl TransponderMode {
    pub fn from_dataref(value: i32) -> TransponderMode {
        match value {
            1 => TransponderMode::Standby,
            2 => TransponderMode::On,
            3 => TransponderMode::Alt,
            4 => TransponderMode::Test,
            5 => TransponderMode::Ground,
            _ => TransponderMode::Off,
        }
    }

    /// Whether VATSIM would see Mode C (altitude reporting) for this mode.
    pub fn is_mode_c(self) -> bool {
        matches!(self, TransponderMode::On | TransponderMode::Alt)
    }

    pub fn label(self) -> &'static str {
        match self {
            TransponderMode::Off => "OFF",
            TransponderMode::Standby => "STBY",
            TransponderMode::On => "ON",
            TransponderMode::Alt => "ALT",
            TransponderMode::Test => "TEST",
            TransponderMode::Ground => "GND",
        }
    }
}

/// Formats a squawk (X-Plane stores it as a decimal number whose digits are the code).
pub fn format_squawk(code: i32) -> String {
    format!("{:04}", code.clamp(0, 7777))
}

/// A squawk is valid when it has four digits, each 0–7.
pub fn is_valid_squawk(text: &str) -> bool {
    text.len() == 4 && text.chars().all(|c| ('0'..='7').contains(&c))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_frequencies() {
        assert_eq!(format_com_khz(122_800), "122.800");
        assert_eq!(format_com_khz(118_005), "118.005");
        assert_eq!(format_com_khz(0), "---.---");
    }

    #[test]
    fn parses_frequencies() {
        assert_eq!(parse_com_khz("122.8"), Some(122_800));
        assert_eq!(parse_com_khz("124.35"), Some(124_350));
        assert_eq!(parse_com_khz("121.505"), Some(121_505));
        assert_eq!(parse_com_khz("121500"), Some(121_500));
        assert_eq!(parse_com_khz("121"), Some(121_000));
        assert_eq!(parse_com_khz("99.5"), None);
        assert_eq!(parse_com_khz("122.8000"), None);
        assert_eq!(parse_com_khz("abc"), None);
    }

    #[test]
    fn squawks() {
        assert_eq!(format_squawk(1200), "1200");
        assert_eq!(format_squawk(7), "0007");
        assert!(is_valid_squawk("7000"));
        assert!(!is_valid_squawk("7800"));
        assert!(!is_valid_squawk("123"));
    }

    #[test]
    fn transponder_modes() {
        assert_eq!(TransponderMode::from_dataref(3), TransponderMode::Alt);
        assert!(TransponderMode::Alt.is_mode_c());
        assert!(!TransponderMode::Standby.is_mode_c());
    }
}
