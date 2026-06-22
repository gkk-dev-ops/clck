use chrono::{DateTime, Datelike, FixedOffset, LocalResult, NaiveDate, NaiveTime, TimeZone, Utc};
use regex::Regex;
use std::{collections::HashSet, fmt::Display, time::Duration};
use thiserror::Error;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Candidate {
    pub source: String,
    pub target: DateTime<FixedOffset>,
    pub timezone: String,
}

impl Candidate {
    pub fn new(
        source: impl Into<String>,
        target: DateTime<FixedOffset>,
        timezone: impl Into<String>,
    ) -> Self {
        Self {
            source: source.into(),
            target,
            timezone: timezone.into(),
        }
    }

    pub fn display_target(&self) -> String {
        format!(
            "{} ({} {})",
            self.target.format("%Y-%m-%d %H:%M"),
            self.timezone,
            self.target.format("%:z")
        )
    }
}

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum ScheduleError {
    #[error("could not find an explicit date and time; try examples such as `2:50pm`, `tomorrow at 9am`, or `June 12 at 09:00`")]
    InvalidExpression,
    #[error("resolved target is not in the future: {0}")]
    PastTarget(DateTime<FixedOffset>),
    #[error("local time does not exist because of a daylight-saving transition")]
    NonexistentLocalTime,
    #[error("local time is ambiguous because of a daylight-saving transition")]
    AmbiguousLocalTime,
    #[error("target is too far in the future to represent as a countdown")]
    DurationOutOfRange,
    #[error("could not determine the system's local IANA time zone")]
    LocalTimeZoneUnavailable,
}

pub fn duration_until(
    now: DateTime<FixedOffset>,
    target: DateTime<FixedOffset>,
) -> Result<Duration, ScheduleError> {
    let delta = target.signed_duration_since(now);
    if delta <= chrono::Duration::zero() {
        return Err(ScheduleError::PastTarget(target));
    }
    delta
        .to_std()
        .map_err(|_| ScheduleError::DurationOutOfRange)
}

pub fn parse_direct(input: &str) -> Result<Candidate, ScheduleError> {
    let timezone = local_timezone()?;
    let now = Utc::now().with_timezone(&timezone);
    parse_direct_in(input, now, timezone)
}

pub fn parse_direct_in<Tz>(
    input: &str,
    now: DateTime<Tz>,
    timezone: Tz,
) -> Result<Candidate, ScheduleError>
where
    Tz: TimeZone + Copy + Display,
    Tz::Offset: Display,
{
    let normalized = normalize_expression(input);
    let (date, time) = parse_date_and_time(&normalized, now.date_naive())
        .ok_or(ScheduleError::InvalidExpression)?;
    let resolved = match timezone.from_local_datetime(&date.and_time(time)) {
        LocalResult::Single(value) => value,
        LocalResult::None => return Err(ScheduleError::NonexistentLocalTime),
        LocalResult::Ambiguous(_, _) => return Err(ScheduleError::AmbiguousLocalTime),
    };
    let target = resolved.fixed_offset();
    duration_until(now.fixed_offset(), target)?;
    Ok(Candidate::new(input.trim(), target, timezone.to_string()))
}

pub fn extract_candidates(text: &str) -> Result<Vec<Candidate>, ScheduleError> {
    let timezone = local_timezone()?;
    let now = Utc::now().with_timezone(&timezone);
    Ok(extract_candidates_in(text, now, timezone))
}

pub fn extract_candidates_in<Tz>(text: &str, now: DateTime<Tz>, timezone: Tz) -> Vec<Candidate>
where
    Tz: TimeZone + Copy + Display,
    Tz::Offset: Display,
{
    let mut seen = HashSet::new();
    let mut occupied = Vec::new();
    let mut candidates = Vec::new();
    for matched in candidate_patterns()
        .iter()
        .flat_map(|pattern| pattern.find_iter(text))
    {
        let mut end = matched.end();
        if text.as_bytes().get(end) == Some(&b'.')
            && (matched.as_str().ends_with("a.m") || matched.as_str().ends_with("p.m"))
        {
            end += 1;
        }
        if occupied
            .iter()
            .any(|(start, occupied_end)| matched.start() < *occupied_end && end > *start)
        {
            continue;
        }
        if let Ok(candidate) = parse_direct_in(&text[matched.start()..end], now.clone(), timezone) {
            occupied.push((matched.start(), end));
            if seen.insert(candidate.target) {
                candidates.push(candidate);
            }
        }
    }
    candidates.sort_by_key(|candidate| candidate.target);
    candidates
}

fn local_timezone() -> Result<chrono_tz::Tz, ScheduleError> {
    let name =
        iana_time_zone::get_timezone().map_err(|_| ScheduleError::LocalTimeZoneUnavailable)?;
    name.parse()
        .map_err(|_| ScheduleError::LocalTimeZoneUnavailable)
}

fn normalize_expression(input: &str) -> String {
    input
        .trim()
        .to_ascii_lowercase()
        .replace("a.m.", "am")
        .replace("p.m.", "pm")
        .replace("a.m", "am")
        .replace("p.m", "pm")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn parse_date_and_time(input: &str, today: NaiveDate) -> Option<(NaiveDate, NaiveTime)> {
    let (date, time_input) = if let Some(rest) = input.strip_prefix("today") {
        (today, strip_at(rest).to_owned())
    } else if let Some(rest) = input.strip_prefix("tomorrow") {
        (today.succ_opt()?, strip_at(rest).to_owned())
    } else {
        let mut parts = input.split_whitespace();
        let first = parts.next()?;
        if let Some(month) = parse_month(first) {
            let day = parts.next()?.parse().ok()?;
            let date = NaiveDate::from_ymd_opt(today.year(), month, day)?;
            let rest = parts.collect::<Vec<_>>().join(" ");
            (date, strip_at(&rest).to_owned())
        } else {
            (today, input.to_owned())
        }
    };
    Some((date, parse_time(&time_input)?))
}

fn strip_at(input: &str) -> &str {
    input.trim().strip_prefix("at ").unwrap_or(input.trim())
}

fn parse_month(input: &str) -> Option<u32> {
    [
        "january",
        "february",
        "march",
        "april",
        "may",
        "june",
        "july",
        "august",
        "september",
        "october",
        "november",
        "december",
    ]
    .iter()
    .position(|month| *month == input)
    .map(|index| index as u32 + 1)
}

fn parse_time(input: &str) -> Option<NaiveTime> {
    let input = input.trim();
    let (clock, suffix) = if let Some(clock) = input.strip_suffix("am") {
        (clock.trim(), Some("am"))
    } else if let Some(clock) = input.strip_suffix("pm") {
        (clock.trim(), Some("pm"))
    } else {
        (input, None)
    };
    if suffix.is_none() && !clock.contains(':') {
        let hour: u32 = clock.parse().ok()?;
        return if hour < 24 {
            NaiveTime::from_hms_opt(hour, 0, 0)
        } else {
            None
        };
    }
    let mut parts = clock.split(':');
    let mut hour: u32 = parts.next()?.trim().parse().ok()?;
    let minute: u32 = parts.next().unwrap_or("0").trim().parse().ok()?;
    if parts.next().is_some() || minute >= 60 {
        return None;
    }
    match suffix {
        Some("am") if (1..=12).contains(&hour) => hour %= 12,
        Some("pm") if (1..=12).contains(&hour) => hour = hour % 12 + 12,
        Some(_) => return None,
        None if hour < 24 => {}
        None => return None,
    }
    NaiveTime::from_hms_opt(hour, minute, 0)
}

fn candidate_patterns() -> Vec<Regex> {
    let time =
        r"(?:(?:1[0-2]|0?[1-9])(?::[0-5]\d)?\s*(?:a\.?m\.?|p\.?m\.?)|(?:[01]?\d|2[0-3]):[0-5]\d)";
    let month = r"(?:january|february|march|april|may|june|july|august|september|october|november|december)";
    [
        format!(r"(?i)\b(?:today|tomorrow)\s+(?:at\s+)?{time}\b"),
        format!(r"(?i)\b{month}\s+\d{{1,2}}\s+(?:at\s+)?{time}\b"),
        r"(?i)\b(?:1[0-2]|0?[1-9])(?::[0-5]\d)?\s*(?:a\.?m\.?|p\.?m\.?)\b".to_owned(),
        r"(?i)\b(?:[01]?\d|2[0-3]):[0-5]\d\b".to_owned(),
    ]
    .into_iter()
    .map(|pattern| Regex::new(&pattern).expect("candidate regex is valid"))
    .collect()
}

#[cfg(test)]
mod tests {
    use super::{duration_until, extract_candidates_in, parse_direct_in, Candidate, ScheduleError};
    use chrono::{Datelike, FixedOffset, TimeZone, Timelike};
    use chrono_tz::America::New_York;
    use std::time::Duration;

    fn fixed(hour: u32, minute: u32) -> chrono::DateTime<FixedOffset> {
        FixedOffset::east_opt(7_200)
            .unwrap()
            .with_ymd_and_hms(2026, 6, 10, hour, minute, 0)
            .unwrap()
    }

    fn context_now() -> chrono::DateTime<chrono_tz::Tz> {
        New_York.with_ymd_and_hms(2026, 6, 10, 8, 0, 0).unwrap()
    }

    #[test]
    fn converts_future_target_to_countdown_duration() {
        assert_eq!(
            duration_until(fixed(12, 0), fixed(12, 1)).unwrap(),
            Duration::from_secs(60)
        );
    }

    #[test]
    fn rejects_past_and_equal_targets() {
        assert_eq!(
            duration_until(fixed(12, 0), fixed(11, 59)).unwrap_err(),
            ScheduleError::PastTarget(fixed(11, 59))
        );
        assert!(duration_until(fixed(12, 0), fixed(12, 0)).is_err());
    }

    #[test]
    fn candidate_formats_resolved_target_with_zone() {
        let candidate = Candidate::new("tomorrow at 9am", fixed(9, 0), "Europe/Warsaw");
        assert_eq!(
            candidate.display_target(),
            "2026-06-10 09:00 (Europe/Warsaw +02:00)"
        );
    }

    #[test]
    fn parses_supported_direct_expressions() {
        for (input, expected_day, expected_hour, expected_minute) in [
            ("2:50pm", 10, 14, 50),
            ("3 p.m.", 10, 15, 0),
            ("14:30", 10, 14, 30),
            ("today at 9 AM", 10, 9, 0),
            ("tomorrow at 9am", 11, 9, 0),
            ("June 12 at 09:00", 12, 9, 0),
            ("11", 10, 11, 0),
            ("11am", 10, 11, 0),
            ("11 a.m.", 10, 11, 0),
        ] {
            let candidate = parse_direct_in(input, context_now(), New_York).unwrap();
            assert_eq!(candidate.target.day(), expected_day);
            assert_eq!(candidate.target.hour(), expected_hour);
            assert_eq!(candidate.target.minute(), expected_minute);
            assert_eq!(candidate.target.second(), 0);
        }
    }

    #[test]
    fn rejects_past_explicit_and_time_only_values() {
        assert!(matches!(
            parse_direct_in("7am", context_now(), New_York),
            Err(ScheduleError::PastTarget(_))
        ));
        assert!(matches!(
            parse_direct_in("June 9 at 09:00", context_now(), New_York),
            Err(ScheduleError::PastTarget(_))
        ));
    }

    #[test]
    fn rejects_nonexistent_and_ambiguous_dst_values() {
        let spring = New_York.with_ymd_and_hms(2026, 3, 7, 12, 0, 0).unwrap();
        assert_eq!(
            parse_direct_in("March 8 at 02:30", spring, New_York).unwrap_err(),
            ScheduleError::NonexistentLocalTime
        );
        let fall = New_York.with_ymd_and_hms(2026, 10, 31, 12, 0, 0).unwrap();
        assert_eq!(
            parse_direct_in("November 1 at 01:30", fall, New_York).unwrap_err(),
            ScheduleError::AmbiguousLocalTime
        );
    }

    #[test]
    fn extracts_one_and_multiple_explicit_candidates() {
        let one = extract_candidates_in("Please call me tomorrow at 9am.", context_now(), New_York);
        assert_eq!(one.len(), 1);
        assert_eq!(one[0].source, "tomorrow at 9am");

        let dotted = extract_candidates_in("Please call me at 3 p.m.", context_now(), New_York);
        assert_eq!(dotted.len(), 1);
        assert_eq!(dotted[0].source, "3 p.m.");

        let many = extract_candidates_in(
            "Try tomorrow at 9am or June 12 at 14:30.",
            context_now(),
            New_York,
        );
        assert_eq!(many.len(), 2);
    }

    #[test]
    fn ignores_vague_phrases_and_deduplicates_resolved_targets() {
        assert!(extract_candidates_in(
            "Let's talk later in the afternoon after lunch.",
            context_now(),
            New_York,
        )
        .is_empty());
        let candidates = extract_candidates_in(
            "Use tomorrow at 9am, or June 11 at 09:00.",
            context_now(),
            New_York,
        );
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].source, "tomorrow at 9am");
    }
}
