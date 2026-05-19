use std::time::{SystemTime, UNIX_EPOCH};

pub(crate) fn human_age_from_rfc3339(timestamp: &str) -> Option<String> {
    let created_at = parse_rfc3339_utc_seconds(timestamp)?;
    let now = unix_now_seconds();
    Some(human_duration_since_seconds(created_at, now))
}

fn unix_now_seconds() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs().min(i64::MAX as u64) as i64)
        .unwrap_or(0)
}

fn human_duration_since_seconds(created_at: i64, now: i64) -> String {
    let elapsed = now.saturating_sub(created_at).max(0);

    match elapsed {
        0..=59 => format!("{elapsed}秒前"),
        60..=3_599 => format!("{}分钟前", elapsed / 60),
        3_600..=86_399 => format!("{}小时前", elapsed / 3_600),
        86_400..=2_591_999 => format!("{}天前", elapsed / 86_400),
        2_592_000..=31_535_999 => format!("{}个月前", elapsed / 2_592_000),
        _ => format!("{}年前", elapsed / 31_536_000),
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_recent_age_in_seconds() {
        assert_eq!(human_duration_since_seconds(100, 142), "42秒前");
    }

    #[test]
    fn formats_age_in_larger_units() {
        assert_eq!(human_duration_since_seconds(0, 60), "1分钟前");
        assert_eq!(human_duration_since_seconds(0, 7_200), "2小时前");
        assert_eq!(human_duration_since_seconds(0, 259_200), "3天前");
        assert_eq!(human_duration_since_seconds(0, 5_184_000), "2个月前");
        assert_eq!(human_duration_since_seconds(0, 63_072_000), "2年前");
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
}
