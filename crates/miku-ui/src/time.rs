#[cfg(not(target_arch = "wasm32"))]
use std::time::{SystemTime, UNIX_EPOCH};

pub(crate) fn human_age_from_rfc3339(timestamp: &str) -> Option<String> {
    let created_at = parse_rfc3339_utc_seconds(timestamp)?;
    let now = unix_now_seconds();
    Some(human_duration_since_seconds(created_at, now))
}

pub(crate) fn utc_now_rfc3339_seconds() -> String {
    format_unix_seconds_as_rfc3339(unix_now_seconds())
}

#[cfg(not(target_arch = "wasm32"))]
fn unix_now_seconds() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs().min(i64::MAX as u64) as i64)
        .unwrap_or(0)
}

#[cfg(target_arch = "wasm32")]
fn unix_now_seconds() -> i64 {
    let seconds = js_sys::Date::now() / 1_000.0;
    if seconds.is_finite() && seconds >= 0.0 {
        seconds.min(i64::MAX as f64) as i64
    } else {
        0
    }
}

fn human_duration_since_seconds(created_at: i64, now: i64) -> String {
    let elapsed = now.saturating_sub(created_at).max(0);

    match elapsed {
        0..=59 => format_relative_time(elapsed, "second"),
        60..=3_599 => format_relative_time(elapsed / 60, "minute"),
        3_600..=86_399 => format_relative_time(elapsed / 3_600, "hour"),
        86_400..=2_591_999 => format_relative_time(elapsed / 86_400, "day"),
        2_592_000..=31_535_999 => format_relative_time(elapsed / 2_592_000, "month"),
        _ => format_relative_time(elapsed / 31_536_000, "year"),
    }
}

fn format_relative_time(value: i64, unit: &str) -> String {
    let suffix = if value == 1 { "" } else { "s" };
    format!("{value} {unit}{suffix} ago")
}

fn parse_rfc3339_utc_seconds(timestamp: &str) -> Option<i64> {
    let timestamp = timestamp.trim();
    let (date, time) = timestamp.split_once('T')?;
    let (year, month, day) = parse_date(date)?;
    let (hour, minute, second) = parse_utc_time(time)?;
    Some(days_from_civil(year, month, day) * 86_400 + hour * 3_600 + minute * 60 + second)
}

fn parse_date(date: &str) -> Option<(i64, i64, i64)> {
    let mut parts = date.split('-');
    let year = parts.next()?.parse().ok()?;
    let month = parts.next()?.parse().ok()?;
    let day = parts.next()?.parse().ok()?;
    if parts.next().is_some() || !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    Some((year, month, day))
}

fn parse_utc_time(time: &str) -> Option<(i64, i64, i64)> {
    let time = time.strip_suffix('Z')?;
    let mut parts = time.split(':');
    let hour = parts.next()?.parse().ok()?;
    let minute = parts.next()?.parse().ok()?;
    let second = parts.next()?.split('.').next()?.parse::<i64>().ok()?;
    if parts.next().is_some()
        || !(0..=23).contains(&hour)
        || !(0..=59).contains(&minute)
        || !(0..=60).contains(&second)
    {
        return None;
    }
    Some((hour, minute, second.min(59)))
}

fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = year - i64::from(month <= 2);
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let year_of_era = year - era * 400;
    let month_prime = month + if month > 2 { -3 } else { 9 };
    let day_of_year = (153 * month_prime + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let days = days + 719_468;
    let era = if days >= 0 { days } else { days - 146_096 } / 146_097;
    let day_of_era = days - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_prime + 2) / 5 + 1;
    let month = month_prime + if month_prime < 10 { 3 } else { -9 };
    let year = year + i64::from(month <= 2);
    (year, month, day)
}

fn format_unix_seconds_as_rfc3339(seconds: i64) -> String {
    let seconds = seconds.max(0);
    let days = seconds / 86_400;
    let second_of_day = seconds % 86_400;
    let (year, month, day) = civil_from_days(days);
    let hour = second_of_day / 3_600;
    let minute = second_of_day % 3_600 / 60;
    let second = second_of_day % 60;
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}Z")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_recent_age_in_seconds() {
        assert_eq!(human_duration_since_seconds(100, 142), "42 seconds ago");
    }

    #[test]
    fn formats_age_in_larger_units() {
        assert_eq!(human_duration_since_seconds(0, 60), "1 minute ago");
        assert_eq!(human_duration_since_seconds(0, 7_200), "2 hours ago");
        assert_eq!(human_duration_since_seconds(0, 259_200), "3 days ago");
        assert_eq!(human_duration_since_seconds(0, 5_184_000), "2 months ago");
        assert_eq!(human_duration_since_seconds(0, 63_072_000), "2 years ago");
    }

    #[test]
    fn parses_kubernetes_timestamp_as_unix_seconds() {
        assert_eq!(
            parse_rfc3339_utc_seconds("2026-05-18T10:00:00Z"),
            Some(1_779_098_400)
        );
    }

    #[test]
    fn parses_fractional_seconds() {
        assert_eq!(
            parse_rfc3339_utc_seconds("2026-05-18T10:00:00.123456Z"),
            Some(1_779_098_400)
        );
    }

    #[test]
    fn rejects_non_utc_timestamps() {
        assert_eq!(parse_rfc3339_utc_seconds("2026-05-18T10:00:00+08:00"), None);
    }

    #[test]
    fn formats_unix_seconds_as_rfc3339() {
        assert_eq!(
            format_unix_seconds_as_rfc3339(1_779_098_400),
            "2026-05-18T10:00:00Z"
        );
    }
}
