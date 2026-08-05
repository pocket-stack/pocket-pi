use std::time::{SystemTime, UNIX_EPOCH};

const MIN_SYNCED_UNIX_SECONDS: u64 = 1_735_689_600; // 2025-01-01T00:00:00Z

pub fn unix_seconds() -> Result<u64, String> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .map_err(|error| format!("system clock is before Unix epoch: {error}"))
}

pub fn is_synchronized() -> bool {
    unix_seconds().is_ok_and(|seconds| seconds >= MIN_SYNCED_UNIX_SECONDS)
}

pub fn definitions() -> Vec<serde_json::Value> {
    vec![serde_json::json!({
        "name":"time.now",
        "description":"Read the SNTP-backed device clock as Unix seconds, UTC, and America/New_York local time. Use this before choosing the next agent wake.",
        "parameters":{"type":"object","properties":{},"additionalProperties":false}
    })]
}

pub fn execute(name: &str) -> Option<serde_json::Value> {
    if name != "time.now" {
        return None;
    }
    let now = match unix_seconds() {
        Ok(now) => now,
        Err(error) => {
            return Some(serde_json::json!({"status":"error","message":error}));
        }
    };
    let synchronized = now >= MIN_SYNCED_UNIX_SECONDS;
    let utc = format_epoch(now as i64, 0, "UTC");
    let ny_offset = new_york_offset_seconds(now as i64);
    let ny_zone = if ny_offset == -4 * 3600 { "EDT" } else { "EST" };
    Some(serde_json::json!({
        "status":if synchronized { "ok" } else { "unsynchronized" },
        "synchronized":synchronized,
        "unixSeconds":now,
        "utc":utc,
        "americaNewYork":format_epoch(now as i64, ny_offset, ny_zone),
        "americaNewYorkUtcOffsetSeconds":ny_offset
    }))
}

fn new_york_offset_seconds(unix: i64) -> i64 {
    let utc = date_time(unix);
    let march_first_weekday = weekday(utc.year, 3, 1);
    let second_sunday = 1 + ((7 - march_first_weekday) % 7) + 7;
    let november_first_weekday = weekday(utc.year, 11, 1);
    let first_sunday = 1 + ((7 - november_first_weekday) % 7);
    let dst_start = unix_from_utc(utc.year, 3, second_sunday, 7, 0, 0);
    let dst_end = unix_from_utc(utc.year, 11, first_sunday, 6, 0, 0);
    if unix >= dst_start && unix < dst_end {
        -4 * 3600
    } else {
        -5 * 3600
    }
}

fn format_epoch(unix: i64, offset: i64, zone: &str) -> String {
    let value = date_time(unix + offset);
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02} {}",
        value.year, value.month, value.day, value.hour, value.minute, value.second, zone
    )
}

#[derive(Clone, Copy)]
struct DateTime {
    year: i64,
    month: i64,
    day: i64,
    hour: i64,
    minute: i64,
    second: i64,
}

fn date_time(unix: i64) -> DateTime {
    let days = unix.div_euclid(86_400);
    let seconds = unix.rem_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    DateTime {
        year,
        month,
        day,
        hour: seconds / 3600,
        minute: (seconds % 3600) / 60,
        second: seconds % 60,
    }
}

fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let day_of_era = z - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let mut year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_prime + 2) / 5 + 1;
    let month = month_prime + if month_prime < 10 { 3 } else { -9 };
    if month <= 2 {
        year += 1;
    }
    (year, month, day)
}

fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let adjusted_year = year - i64::from(month <= 2);
    let era = if adjusted_year >= 0 {
        adjusted_year
    } else {
        adjusted_year - 399
    } / 400;
    let year_of_era = adjusted_year - era * 400;
    let month_prime = month + if month > 2 { -3 } else { 9 };
    let day_of_year = (153 * month_prime + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

fn unix_from_utc(year: i64, month: i64, day: i64, hour: i64, minute: i64, second: i64) -> i64 {
    days_from_civil(year, month, day) * 86_400 + hour * 3600 + minute * 60 + second
}

/// Sunday is 0, matching the trading-calendar convention used in the prompt.
fn weekday(year: i64, month: i64, day: i64) -> i64 {
    (days_from_civil(year, month, day) + 4).rem_euclid(7)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_epoch_and_new_york_dst() {
        assert_eq!(format_epoch(0, 0, "UTC"), "1970-01-01T00:00:00 UTC");
        let before = unix_from_utc(2026, 3, 8, 6, 59, 59);
        let after = unix_from_utc(2026, 3, 8, 7, 0, 0);
        assert_eq!(new_york_offset_seconds(before), -5 * 3600);
        assert_eq!(new_york_offset_seconds(after), -4 * 3600);
    }
}
