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

/// The channel name in kHz for a frequency in Hz, as a radio shows it: 129_525_000 → 129525,
/// 132_841_667 → 132840 (an 8.33 kHz channel). The inverse of [`channel_to_hz`] for the 25 kHz
/// spacing (where 8.33 kHz radios also have an "…05" name for the same frequency).
pub fn hz_to_channel(hz: i64) -> i32 {
    let base_khz = (hz / 25_000 * 25) as i32;
    let offset = match hz % 25_000 {
        r if r < 4_167 => 0,
        r if r < 12_500 => 10,
        r if r < 20_833 => 15,
        _ => 25,
    };
    base_khz + offset
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
    fn channel_from_hz() {
        assert_eq!(hz_to_channel(129_525_000), 129_525);
        assert_eq!(hz_to_channel(133_615_000), 133_615);
        for khz in [118_505, 121_705, 132_840, 132_835, 124_230, 127_955] {
            assert!(same_frequency(channel_to_hz(hz_to_channel(channel_to_hz(khz))), channel_to_hz(khz)), "{khz}");
        }
    }

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
