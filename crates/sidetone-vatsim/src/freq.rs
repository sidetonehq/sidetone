//! VHF frequency handling, including 8.33 kHz channel names.
//!
//! Radios and the VATSIM feed use *channel names* ("119.105"), while transceivers report the
//! real frequency in Hz (119_100_000). Everything is compared in Hz after normalising.

/// Parses a feed frequency string such as `"119.105"` into kHz.
pub fn parse_mhz(text: &str) -> Option<i32> {
    let (mhz, frac) = text.trim().split_once('.')?;
    let mhz: i32 = mhz.parse().ok()?;
    if frac.is_empty() || frac.len() > 3 || !frac.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let frac: i32 = format!("{frac:0<3}").parse().ok()?;
    Some(mhz * 1000 + frac)
}

/// Converts a channel name in kHz (25 kHz or 8.33 kHz style) to the real frequency in Hz.
pub fn channel_to_hz(khz: i32) -> i64 {
    let base = khz - khz.rem_euclid(25);
    let offset_hz = match khz.rem_euclid(25) {
        0 | 5 => 0,
        10 => 8_333,
        15 => 16_667,
        // Not a valid channel ending; treat the digits as a literal frequency.
        other => other as i64 * 1000,
    };
    base as i64 * 1000 + offset_hz
}

/// True when two frequencies in Hz are the same channel (transceivers carry a few Hz of noise).
pub fn same_frequency(a_hz: i64, b_hz: i64) -> bool {
    (a_hz - b_hz).abs() <= 2_000
}

/// VATSIM's "no frequency" placeholder used by observers and pilots.
pub fn is_placeholder(khz: i32) -> bool {
    khz >= 199_000
}

pub const UNICOM_KHZ: i32 = 122_800;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses() {
        assert_eq!(parse_mhz("119.105"), Some(119_105));
        assert_eq!(parse_mhz("122.8"), Some(122_800));
        assert_eq!(parse_mhz("x"), None);
    }

    #[test]
    fn channels_map_to_real_frequencies() {
        assert_eq!(channel_to_hz(118_700), 118_700_000);
        assert_eq!(channel_to_hz(118_705), 118_700_000);
        assert_eq!(channel_to_hz(119_105), 119_100_000);
        assert_eq!(channel_to_hz(132_860), 132_858_333);
        assert_eq!(channel_to_hz(132_865), 132_866_667);
        assert!(same_frequency(channel_to_hz(122_800), 122_800_015));
        assert!(!same_frequency(channel_to_hz(132_855), channel_to_hz(132_860)));
    }
}
