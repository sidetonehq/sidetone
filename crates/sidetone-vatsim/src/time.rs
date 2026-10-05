//! Minimal UTC timestamp handling (VATSIM APIs use ISO 8601 `…Z` strings).

/// Parses `2026-10-05T17:00:00.000000Z` (fraction optional) into Unix seconds.
pub fn parse_iso8601(s: &str) -> Option<i64> {
    let s = s.trim().trim_end_matches('Z');
    let (date, time) = s.split_once('T')?;
    let mut d = date.split('-').map(|p| p.parse::<i64>());
    let (y, m, day) = (d.next()?.ok()?, d.next()?.ok()?, d.next()?.ok()?);
    let time = time.split('.').next()?;
    let mut t = time.split(':').map(|p| p.parse::<i64>());
    let (hh, mm, ss) = (t.next()?.ok()?, t.next()?.ok()?, t.next().and_then(|r| r.ok()).unwrap_or(0));
    Some(days_from_civil(y, m, day) * 86_400 + hh * 3600 + mm * 60 + ss)
}

/// Days since 1970-01-01 (Howard Hinnant's algorithm).
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

pub fn now_unix() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

/// "17:00z" or "Tue 17:00z" when not today.
pub fn format_utc(secs: i64, now: i64) -> String {
    let hhmm = format!("{:02}:{:02}z", secs.rem_euclid(86_400) / 3600, secs.rem_euclid(3600) / 60);
    if secs.div_euclid(86_400) == now.div_euclid(86_400) {
        hhmm
    } else {
        const DAYS: [&str; 7] = ["Thu", "Fri", "Sat", "Sun", "Mon", "Tue", "Wed"];
        format!("{} {hhmm}", DAYS[secs.div_euclid(86_400).rem_euclid(7) as usize])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses() {
        assert_eq!(parse_iso8601("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(parse_iso8601("2026-10-05T17:00:00.000000Z"), Some(1_791_219_600));
        assert_eq!(parse_iso8601("nope"), None);
    }

    #[test]
    fn formats() {
        let t = 1_791_219_600; // Monday 2026-10-05 17:00z
        assert_eq!(format_utc(t, t), "17:00z");
        assert_eq!(format_utc(t + 86_400, t), "Tue 17:00z");
    }
}
