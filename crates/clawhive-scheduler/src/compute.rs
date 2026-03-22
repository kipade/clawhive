use std::str::FromStr;

use anyhow::{anyhow, Result};
use chrono::{DateTime, TimeZone, Utc};
use chrono_tz::Tz;
use cron::Schedule as CronSchedule;

use crate::ScheduleType;

pub fn compute_next_run_at_ms(schedule: &ScheduleType, now_ms: i64) -> Result<Option<i64>> {
    match schedule {
        ScheduleType::Cron { expr, tz } => {
            let tz: Tz = tz.parse().map_err(|_| anyhow!("invalid timezone: {tz}"))?;
            let cron = CronSchedule::from_str(&normalize_cron_expr(expr))?;
            let now_dt = tz
                .timestamp_millis_opt(now_ms)
                .single()
                .ok_or_else(|| anyhow!("invalid timestamp: {now_ms}"))?;
            let next = cron.after(&now_dt).next();
            Ok(next.map(|dt| dt.with_timezone(&Utc).timestamp_millis()))
        }
        ScheduleType::At { at } => {
            let at_ms = parse_absolute_or_relative_ms(at, now_ms)?;
            Ok((at_ms > now_ms).then_some(at_ms))
        }
        ScheduleType::Every {
            interval_ms,
            anchor_ms,
        } => {
            let interval = *interval_ms as i64;
            if interval <= 0 {
                return Err(anyhow!("interval_ms must be positive"));
            }

            let anchor = anchor_ms.map(|value| value as i64).unwrap_or(now_ms);
            if now_ms < anchor {
                return Ok(Some(anchor));
            }

            let elapsed = now_ms - anchor;
            let steps = (elapsed + interval - 1) / interval;
            Ok(Some(anchor + steps * interval))
        }
    }
}

fn normalize_cron_expr(expr: &str) -> String {
    let fields: Vec<&str> = expr.split_whitespace().collect();
    if fields.len() == 5 {
        // 5-field POSIX cron: min hour dom month dow
        // Remap dow from POSIX (0=Sun,1=Mon..6=Sat) to cron crate (1=Sun,2=Mon..7=Sat)
        let dow = remap_dow_posix_to_cron(fields[4]);
        format!(
            "0 {} {} {} {} {dow}",
            fields[0], fields[1], fields[2], fields[3]
        )
    } else {
        expr.to_string()
    }
}

/// Remap POSIX day-of-week values to the `cron` crate convention.
///
/// POSIX: 0=Sun, 1=Mon, ..., 6=Sat (7=Sun alternate)
/// cron crate: 1=Sun, 2=Mon, ..., 7=Sat
///
/// Handles lists (`0,6`), ranges (`1-5`), steps (`1-5/2`, `*/2`),
/// hash (`2#1`), and named days (`MON-FRI`) which pass through unchanged.
fn remap_dow_posix_to_cron(dow: &str) -> String {
    if dow == "*" || dow == "?" {
        return dow.to_string();
    }

    dow.split(',')
        .map(|part| {
            // Handle hash: 2#1 → 3#1
            if let Some((day, nth)) = part.split_once('#') {
                return format!("{}#{nth}", remap_single_dow(day));
            }

            // Handle step: 1-5/2 → 2-6/2, */2 → */2
            let (range_part, step) = match part.split_once('/') {
                Some((r, s)) => (r, Some(s)),
                None => (part, None),
            };

            let remapped = if range_part == "*" {
                "*".to_string()
            } else if let Some((start, end)) = range_part.split_once('-') {
                format!("{}-{}", remap_single_dow(start), remap_single_dow(end))
            } else {
                remap_single_dow(range_part)
            };

            match step {
                Some(s) => format!("{remapped}/{s}"),
                None => remapped,
            }
        })
        .collect::<Vec<_>>()
        .join(",")
}

fn remap_single_dow(val: &str) -> String {
    match val.parse::<u8>() {
        Ok(0 | 7) => "1".to_string(),
        Ok(n @ 1..=6) => (n + 1).to_string(),
        _ => val.to_string(),
    }
}

fn parse_absolute_or_relative_ms(input: &str, now_ms: i64) -> Result<i64> {
    if let Some(ms) = try_parse_relative_ms(input) {
        return Ok(now_ms + ms);
    }

    let dt = DateTime::parse_from_rfc3339(input)
        .or_else(|_| DateTime::parse_from_str(input, "%Y-%m-%dT%H:%M:%S%z"))?;
    Ok(dt.with_timezone(&Utc).timestamp_millis())
}

fn try_parse_relative_ms(input: &str) -> Option<i64> {
    let input = input.trim();
    if input.len() < 2 {
        return None;
    }

    let (num_str, unit) = input.split_at(input.len() - 1);
    let num: i64 = num_str.parse().ok()?;

    match unit {
        "s" => Some(num * 1_000),
        "m" => Some(num * 60_000),
        "h" => Some(num * 3_600_000),
        "d" => Some(num * 86_400_000),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use chrono::Utc;

    use super::*;

    #[test]
    fn test_cron_next_run() {
        let schedule = ScheduleType::Cron {
            expr: "* * * * *".into(),
            tz: "UTC".into(),
        };
        let now_ms = Utc::now().timestamp_millis();
        let next = compute_next_run_at_ms(&schedule, now_ms).unwrap().unwrap();
        assert!(next > now_ms);
        assert!(next - now_ms <= 60_000);
    }

    #[test]
    fn test_at_relative() {
        let schedule = ScheduleType::At { at: "20m".into() };
        let now_ms = 1_000_000;
        let next = compute_next_run_at_ms(&schedule, now_ms).unwrap().unwrap();
        assert_eq!(next, 1_000_000 + 20 * 60_000);
    }

    #[test]
    fn test_at_past_returns_none() {
        let schedule = ScheduleType::At {
            at: "2020-01-01T00:00:00Z".into(),
        };
        let now_ms = Utc::now().timestamp_millis();
        assert!(compute_next_run_at_ms(&schedule, now_ms).unwrap().is_none());
    }

    #[test]
    fn test_every_with_anchor() {
        let schedule = ScheduleType::Every {
            interval_ms: 60_000,
            anchor_ms: Some(0),
        };
        let next = compute_next_run_at_ms(&schedule, 90_000).unwrap().unwrap();
        assert_eq!(next, 120_000);
    }

    #[test]
    fn test_normalize_cron_dow_remap() {
        // POSIX 1-5 (Mon-Fri) → cron crate 2-6
        assert_eq!(normalize_cron_expr("30 10 * * 1-5"), "0 30 10 * * 2-6");
        // POSIX 0 (Sun) → cron crate 1
        assert_eq!(normalize_cron_expr("0 9 * * 0"), "0 0 9 * * 1");
        // POSIX 7 (Sun alternate) → cron crate 1
        assert_eq!(normalize_cron_expr("0 9 * * 7"), "0 0 9 * * 1");
        // POSIX 6 (Sat) → cron crate 7
        assert_eq!(normalize_cron_expr("0 9 * * 6"), "0 0 9 * * 7");
        // List: POSIX 0,6 → cron crate 1,7
        assert_eq!(normalize_cron_expr("0 9 * * 0,6"), "0 0 9 * * 1,7");
        // Step: */2 → */2 (wildcard unchanged)
        assert_eq!(normalize_cron_expr("0 9 * * */2"), "0 0 9 * * */2");
        // Range with step: 1-5/2 → 2-6/2
        assert_eq!(normalize_cron_expr("0 9 * * 1-5/2"), "0 0 9 * * 2-6/2");
        // Wildcard: * → *
        assert_eq!(normalize_cron_expr("* * * * *"), "0 * * * * *");
        // Named days: MON-FRI unchanged
        assert_eq!(
            normalize_cron_expr("30 10 * * MON-FRI"),
            "0 30 10 * * MON-FRI"
        );
        // Hash: 2#1 → 3#1
        assert_eq!(normalize_cron_expr("0 9 * * 2#1"), "0 0 9 * * 3#1");
        // 6-field expressions are NOT remapped
        assert_eq!(normalize_cron_expr("0 30 10 * * 1-5"), "0 30 10 * * 1-5");
    }

    #[test]
    fn test_cron_weekday_only_excludes_sunday() {
        use chrono::{Datelike, TimeZone};

        // 2026-03-22 is a Sunday in UTC
        let sunday_10_30 = Utc
            .with_ymd_and_hms(2026, 3, 22, 2, 30, 0) // 10:30 SGT = 02:30 UTC
            .unwrap();
        assert_eq!(sunday_10_30.weekday(), chrono::Weekday::Sun);

        let schedule = ScheduleType::Cron {
            expr: "30 10 * * 1-5".into(),
            tz: "Asia/Singapore".into(),
        };
        let next = compute_next_run_at_ms(&schedule, sunday_10_30.timestamp_millis())
            .unwrap()
            .unwrap();

        // Next fire should be Monday 2026-03-23 10:30 SGT = 02:30 UTC
        let next_dt = Utc.timestamp_millis_opt(next).unwrap();
        assert_eq!(next_dt.weekday(), chrono::Weekday::Mon);
        assert_eq!(next_dt.day(), 23);
    }
}
