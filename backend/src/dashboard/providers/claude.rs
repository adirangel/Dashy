use std::time::Duration;

use async_trait::async_trait;
use chrono::{
    DateTime, Datelike, Duration as ChronoDuration, Local, LocalResult, NaiveDate, NaiveDateTime,
    NaiveTime, TimeZone, Utc, Weekday,
};
use chrono_tz::Tz;
use serde::Deserialize;

use crate::dashboard::{
    models::{ProviderError, UsageData, UsageWindowData, UsageWindowKind},
    process::{AllowedProgram, CaptureRunner, CompletionMarker, InteractiveRunner, ProcessError},
    providers::{remaining_timeout, DataProvider},
};

const AUTH_TIMEOUT: Duration = Duration::from_secs(10);
const CLAUDE_TIMEOUT: Duration = Duration::from_secs(15);
const MAX_RESET_TEXT_SCALARS: usize = 80;
const MAX_RELATIVE_RESET_MINUTES: i64 = 366 * 24 * 60;

pub struct ClaudeProvider<C: CaptureRunner, I: InteractiveRunner> {
    capture_runner: C,
    interactive_runner: I,
}

impl<C: CaptureRunner, I: InteractiveRunner> ClaudeProvider<C, I> {
    pub fn new(capture_runner: C, interactive_runner: I) -> Self {
        Self {
            capture_runner,
            interactive_runner,
        }
    }
}

#[async_trait]
impl<C: CaptureRunner, I: InteractiveRunner> DataProvider<UsageData> for ClaudeProvider<C, I> {
    async fn fetch(&self) -> Result<UsageData, ProviderError> {
        let deadline = tokio::time::Instant::now() + CLAUDE_TIMEOUT;
        let status = self
            .capture_runner
            .capture(
                AllowedProgram::Claude,
                vec!["auth".to_owned(), "status".to_owned(), "--json".to_owned()],
                remaining_timeout(deadline)?.min(AUTH_TIMEOUT),
            )
            .await
            .map_err(map_auth_process_error)?;
        if !parse_auth_status(&status.stdout)? {
            return Err(ProviderError::NotAuthenticated);
        }

        let output = self
            .interactive_runner
            .run_command(
                AllowedProgram::Claude,
                vec!["--safe-mode".to_owned(), "--no-chrome".to_owned()],
                "/usage\r".to_owned(),
                vec![
                    CompletionMarker::Exact("Current session".to_owned()),
                    CompletionMarker::ExactAlternative {
                        first: "All models".to_owned(),
                        second: "Current week (all models)".to_owned(),
                    },
                    CompletionMarker::Prefix("Resets ".to_owned()),
                ],
                Some("/exit\r".to_owned()),
                remaining_timeout(deadline)?,
            )
            .await
            .map_err(map_usage_process_error)?;
        parse_usage(&output)
    }
}

fn map_auth_process_error(error: ProcessError) -> ProviderError {
    match error {
        ProcessError::NotInstalled => ProviderError::NotInstalled,
        ProcessError::Timeout => ProviderError::Timeout,
        ProcessError::NonZero(_) => ProviderError::NotAuthenticated,
        ProcessError::Io => ProviderError::Network,
        ProcessError::OutputLimit | ProcessError::JsonRpc { .. } => ProviderError::Process,
    }
}

fn map_usage_process_error(error: ProcessError) -> ProviderError {
    match error {
        ProcessError::NotInstalled => ProviderError::NotInstalled,
        ProcessError::Timeout => ProviderError::Timeout,
        ProcessError::Io => ProviderError::Network,
        ProcessError::NonZero(_) | ProcessError::OutputLimit | ProcessError::JsonRpc { .. } => {
            ProviderError::Process
        }
    }
}

fn parse_auth_status(response: &str) -> Result<bool, ProviderError> {
    let status: AuthStatus =
        serde_json::from_str(response).map_err(|_| ProviderError::UnsupportedOutput)?;
    Ok(status.logged_in)
}

fn parse_usage(response: &str) -> Result<UsageData, ProviderError> {
    parse_usage_at(response, Local::now())
}

fn parse_usage_at<Tz>(response: &str, now: DateTime<Tz>) -> Result<UsageData, ProviderError>
where
    Tz: TimeZone,
{
    let mut sections = response.split('\n').peekable();
    let mut session = None;
    let mut week = None;
    let mut saw_usage_heading = false;

    while let Some(line) = sections.next() {
        let heading = line.trim_end_matches('\r');
        if heading.is_empty() {
            continue;
        }

        let target = match heading {
            "Current session" => {
                saw_usage_heading = true;
                Some(&mut session)
            }
            "All models" | "Current week (all models)" => {
                saw_usage_heading = true;
                Some(&mut week)
            }
            _ if is_excluded_heading(heading) => {
                saw_usage_heading = true;
                None
            }
            _ if heading.starts_with("Current ") || saw_usage_heading => {
                return Err(ProviderError::UnsupportedOutput);
            }
            _ => continue,
        };

        let mut lines = Vec::new();
        while let Some(next) = sections.peek() {
            let next = next.trim_end_matches('\r');
            if next.is_empty()
                || next.starts_with("Current ")
                || matches!(next, "All models" | "Current week (all models)")
            {
                break;
            }
            lines.push(next);
            sections.next();
        }

        if let Some(slot) = target {
            if slot.is_some() {
                return Err(ProviderError::UnsupportedOutput);
            }
            *slot = Some(parse_general_window(&lines, &now)?);
        }
    }

    let session = session.ok_or(ProviderError::UnsupportedOutput)?;
    let week = week.ok_or(ProviderError::UnsupportedOutput)?;
    UsageData::try_new(
        Some(UsageWindowData {
            label_key: UsageWindowKind::Short,
            remaining_percent: 100 - session.used_percent,
            resets_at: Some(session.resets_at),
        }),
        Some(UsageWindowData {
            label_key: UsageWindowKind::Weekly,
            remaining_percent: 100 - week.used_percent,
            resets_at: Some(week.resets_at),
        }),
    )
}

fn is_excluded_heading(heading: &str) -> bool {
    heading.starts_with("Current ")
        && (heading.contains("model-specific")
            || heading.contains("Sonnet")
            || heading.contains("Opus")
            || heading.contains("Haiku"))
}

fn parse_general_window<Tz>(
    lines: &[&str],
    now: &DateTime<Tz>,
) -> Result<UsageWindow, ProviderError>
where
    Tz: TimeZone,
{
    if lines.len() != 2 {
        return Err(ProviderError::UnsupportedOutput);
    }

    let used_percent = lines[0]
        .strip_suffix("% used")
        .and_then(|value| value.parse::<u8>().ok())
        .filter(|value| *value <= 100)
        .ok_or(ProviderError::UnsupportedOutput)?;
    let reset_text = lines[1]
        .strip_prefix("Resets ")
        .filter(|value| !value.is_empty())
        .ok_or(ProviderError::UnsupportedOutput)?;
    let resets_at = parse_reset_timestamp(reset_text, now)?;

    Ok(UsageWindow {
        used_percent,
        resets_at,
    })
}

fn parse_reset_timestamp<Tz>(text: &str, now: &DateTime<Tz>) -> Result<DateTime<Utc>, ProviderError>
where
    Tz: TimeZone,
{
    if text.is_empty()
        || text.chars().count() > MAX_RESET_TEXT_SCALARS
        || !text
            .chars()
            .all(|character| character.is_ascii() && !character.is_control())
    {
        return Err(ProviderError::UnsupportedOutput);
    }

    if let Some(relative) = text.strip_prefix("in ") {
        return parse_relative_reset(relative, now);
    }

    let tokens = text.split_ascii_whitespace().collect::<Vec<_>>();
    if let Some((local_tokens, timezone)) = parse_safe_iana_zone(&tokens) {
        let zoned_now = now.with_timezone(&timezone);
        if local_tokens.len() == 1 {
            let time =
                parse_compact_clock(local_tokens[0]).ok_or(ProviderError::UnsupportedOutput)?;
            let today = zoned_now.date_naive();
            if let Some(candidate) = future_strict_local_datetime(&zoned_now, today, time)? {
                return Ok(candidate);
            }
            let tomorrow = today
                .checked_add_signed(ChronoDuration::days(1))
                .ok_or(ProviderError::UnsupportedOutput)?;
            return future_strict_local_datetime(&zoned_now, tomorrow, time)?
                .ok_or(ProviderError::UnsupportedOutput);
        }

        if local_tokens.len() == 3 {
            let month = parse_month(local_tokens[0]).ok_or(ProviderError::UnsupportedOutput)?;
            let day = local_tokens[1]
                .strip_suffix(',')
                .and_then(|value| value.parse::<u32>().ok())
                .filter(|day| (1..=31).contains(day))
                .ok_or(ProviderError::UnsupportedOutput)?;
            let time =
                parse_compact_clock(local_tokens[2]).ok_or(ProviderError::UnsupportedOutput)?;
            for year_offset in 0..=8 {
                let Some(year) = zoned_now.year().checked_add(year_offset) else {
                    break;
                };
                let Some(date) = NaiveDate::from_ymd_opt(year, month, day) else {
                    continue;
                };
                if let Some(candidate) = future_strict_local_datetime(&zoned_now, date, time)? {
                    return Ok(candidate);
                }
            }
        }

        return Err(ProviderError::UnsupportedOutput);
    }

    if let Some(time) = parse_clock(&tokens) {
        let today = now.date_naive();
        if let Some(candidate) = future_local_datetime(now, today, time) {
            return Ok(candidate);
        }
        let tomorrow = today
            .checked_add_signed(ChronoDuration::days(1))
            .ok_or(ProviderError::UnsupportedOutput)?;
        return future_local_datetime(now, tomorrow, time).ok_or(ProviderError::UnsupportedOutput);
    }

    if tokens.len() == 3 {
        let weekday = parse_weekday(tokens[0]).ok_or(ProviderError::UnsupportedOutput)?;
        let time = parse_clock(&tokens[1..]).ok_or(ProviderError::UnsupportedOutput)?;
        let today = now.date_naive();
        let current = i64::from(today.weekday().num_days_from_monday());
        let target = i64::from(weekday.num_days_from_monday());
        let mut days_ahead = (target - current).rem_euclid(7);
        let mut date = today
            .checked_add_signed(ChronoDuration::days(days_ahead))
            .ok_or(ProviderError::UnsupportedOutput)?;
        if future_local_datetime(now, date, time).is_none() {
            days_ahead += 7;
            date = today
                .checked_add_signed(ChronoDuration::days(days_ahead))
                .ok_or(ProviderError::UnsupportedOutput)?;
        }
        return future_local_datetime(now, date, time).ok_or(ProviderError::UnsupportedOutput);
    }

    if tokens.len() == 5 && tokens[2] == "at" {
        let month = parse_month(tokens[0]).ok_or(ProviderError::UnsupportedOutput)?;
        let day = tokens[1]
            .parse::<u32>()
            .ok()
            .filter(|day| (1..=31).contains(day))
            .ok_or(ProviderError::UnsupportedOutput)?;
        let time = parse_clock(&tokens[3..]).ok_or(ProviderError::UnsupportedOutput)?;
        for year_offset in 0..=8 {
            let Some(year) = now.year().checked_add(year_offset) else {
                break;
            };
            let Some(date) = NaiveDate::from_ymd_opt(year, month, day) else {
                continue;
            };
            if let Some(candidate) = future_local_datetime(now, date, time) {
                return Ok(candidate);
            }
        }
    }

    Err(ProviderError::UnsupportedOutput)
}

fn parse_relative_reset<Tz>(text: &str, now: &DateTime<Tz>) -> Result<DateTime<Utc>, ProviderError>
where
    Tz: TimeZone,
{
    let tokens = text.split_ascii_whitespace().collect::<Vec<_>>();
    if !matches!(tokens.len(), 2 | 4) {
        return Err(ProviderError::UnsupportedOutput);
    }

    let mut total_minutes = 0_i64;
    for pair in tokens.as_chunks::<2>().0 {
        let amount = pair[0]
            .parse::<i64>()
            .ok()
            .filter(|amount| *amount > 0)
            .ok_or(ProviderError::UnsupportedOutput)?;
        let multiplier = match pair[1] {
            "min" | "mins" | "minute" | "minutes" => 1,
            "hr" | "hrs" | "hour" | "hours" => 60,
            "day" | "days" => 24 * 60,
            _ => return Err(ProviderError::UnsupportedOutput),
        };
        total_minutes = total_minutes
            .checked_add(
                amount
                    .checked_mul(multiplier)
                    .ok_or(ProviderError::UnsupportedOutput)?,
            )
            .ok_or(ProviderError::UnsupportedOutput)?;
    }
    if total_minutes > MAX_RELATIVE_RESET_MINUTES {
        return Err(ProviderError::UnsupportedOutput);
    }
    now.clone()
        .checked_add_signed(ChronoDuration::minutes(total_minutes))
        .map(|value| value.with_timezone(&Utc))
        .ok_or(ProviderError::UnsupportedOutput)
}

fn parse_clock(tokens: &[&str]) -> Option<NaiveTime> {
    if tokens.len() != 2 || !matches!(tokens[1], "AM" | "PM") {
        return None;
    }
    let (hour, minute) = tokens[0].split_once(':')?;
    if hour.is_empty() || hour.len() > 2 || minute.len() != 2 {
        return None;
    }
    let hour = hour
        .parse::<u32>()
        .ok()
        .filter(|hour| (1..=12).contains(hour))?;
    let minute = minute.parse::<u32>().ok().filter(|minute| *minute < 60)?;
    let hour = match (hour, tokens[1]) {
        (12, "AM") => 0,
        (12, "PM") => 12,
        (hour, "PM") => hour + 12,
        (hour, "AM") => hour,
        _ => return None,
    };
    NaiveTime::from_hms_opt(hour, minute, 0)
}

fn parse_compact_clock(value: &str) -> Option<NaiveTime> {
    let (clock, meridiem) = value
        .strip_suffix("am")
        .map(|clock| (clock, "am"))
        .or_else(|| value.strip_suffix("pm").map(|clock| (clock, "pm")))?;
    let (hour, minute) = match clock.split_once(':') {
        Some((hour, minute)) => {
            let minute = (minute.len() == 2)
                .then(|| minute.parse::<u32>().ok())
                .flatten()
                .filter(|minute| *minute < 60)?;
            (hour, minute)
        }
        None => (clock, 0),
    };
    if hour.is_empty() || hour.len() > 2 {
        return None;
    }
    let hour = hour
        .parse::<u32>()
        .ok()
        .filter(|hour| (1..=12).contains(hour))?;
    let hour = match (hour, meridiem) {
        (12, "am") => 0,
        (12, "pm") => 12,
        (hour, "pm") => hour + 12,
        (hour, "am") => hour,
        _ => return None,
    };
    NaiveTime::from_hms_opt(hour, minute, 0)
}

fn parse_safe_iana_zone<'a>(tokens: &'a [&'a str]) -> Option<(&'a [&'a str], Tz)> {
    let (zone, local_tokens) = tokens.split_last()?;
    let zone = zone.strip_prefix('(')?.strip_suffix(')')?;
    let components = zone.split('/').collect::<Vec<_>>();
    if !(2..=4).contains(&components.len())
        || components.iter().any(|component| {
            component.is_empty()
                || component.len() > 32
                || !component
                    .chars()
                    .next()
                    .is_some_and(|character| character.is_ascii_alphabetic())
                || !component.chars().all(|character| {
                    character.is_ascii_alphanumeric() || matches!(character, '_' | '-' | '+')
                })
        })
    {
        return None;
    }
    let timezone = zone.parse::<Tz>().ok()?;
    Some((local_tokens, timezone))
}

fn parse_weekday(value: &str) -> Option<Weekday> {
    match value {
        "Mon" => Some(Weekday::Mon),
        "Tue" => Some(Weekday::Tue),
        "Wed" => Some(Weekday::Wed),
        "Thu" => Some(Weekday::Thu),
        "Fri" => Some(Weekday::Fri),
        "Sat" => Some(Weekday::Sat),
        "Sun" => Some(Weekday::Sun),
        _ => None,
    }
}

fn parse_month(value: &str) -> Option<u32> {
    match value {
        "Jan" => Some(1),
        "Feb" => Some(2),
        "Mar" => Some(3),
        "Apr" => Some(4),
        "May" => Some(5),
        "Jun" => Some(6),
        "Jul" => Some(7),
        "Aug" => Some(8),
        "Sep" => Some(9),
        "Oct" => Some(10),
        "Nov" => Some(11),
        "Dec" => Some(12),
        _ => None,
    }
}

fn future_local_datetime<Tz>(
    now: &DateTime<Tz>,
    date: NaiveDate,
    time: NaiveTime,
) -> Option<DateTime<Utc>>
where
    Tz: TimeZone,
{
    let naive = NaiveDateTime::new(date, time);
    let candidate = match now.timezone().from_local_datetime(&naive) {
        LocalResult::Single(candidate) => Some(candidate),
        LocalResult::Ambiguous(first, second) => [first, second]
            .into_iter()
            .filter(|candidate| candidate.timestamp() > now.timestamp())
            .min_by_key(DateTime::timestamp),
        LocalResult::None => None,
    }?;
    (candidate.timestamp() > now.timestamp()).then(|| candidate.with_timezone(&Utc))
}

fn future_strict_local_datetime<Tz>(
    now: &DateTime<Tz>,
    date: NaiveDate,
    time: NaiveTime,
) -> Result<Option<DateTime<Utc>>, ProviderError>
where
    Tz: TimeZone,
{
    let naive = NaiveDateTime::new(date, time);
    let candidate = match now.timezone().from_local_datetime(&naive) {
        LocalResult::Single(candidate) => {
            (candidate.timestamp() > now.timestamp()).then_some(candidate)
        }
        LocalResult::Ambiguous(first, second) => [first, second]
            .into_iter()
            .filter(|candidate| candidate.timestamp() > now.timestamp())
            .min_by_key(DateTime::timestamp),
        LocalResult::None => return Err(ProviderError::UnsupportedOutput),
    };
    Ok(candidate.map(|candidate| candidate.with_timezone(&Utc)))
}

#[derive(Deserialize)]
struct AuthStatus {
    #[serde(rename = "loggedIn")]
    logged_in: bool,
}

struct UsageWindow {
    used_percent: u8,
    resets_at: DateTime<Utc>,
}

#[cfg(test)]
mod tests {
    use std::{collections::VecDeque, sync::Mutex, time::Duration};

    use async_trait::async_trait;
    use chrono::{FixedOffset, TimeZone, Utc};

    use super::*;
    use crate::dashboard::process::{AllowedProgram, CapturedOutput, ProcessError};

    fn general_usage(session_used_percent: u8, all_models_used_percent: u8) -> String {
        format!(
            "Current session\n{session_used_percent}% used\nResets 8:40 PM\nAll models\n{all_models_used_percent}% used\nResets Thu 12:00 AM\n"
        )
    }

    #[test]
    fn unauthenticated_status_is_detected() {
        assert!(!parse_auth_status(r#"{"loggedIn":false,"authMethod":"none"}"#).unwrap());
    }

    #[test]
    fn preserves_the_short_and_weekly_general_windows() {
        let usage = parse_usage(
            "Current session\n51% used\nResets 8:40 PM\nAll models\n36% used\nResets Thu 12:00 AM\n",
        )
        .unwrap();
        assert_eq!(usage.short_window.unwrap().remaining_percent, 49);
        assert_eq!(usage.weekly_window.unwrap().remaining_percent, 64);
    }

    #[test]
    fn converts_allowlisted_relative_and_weekday_resets_to_structured_utc_times() {
        let now = FixedOffset::east_opt(3 * 3600)
            .unwrap()
            .with_ymd_and_hms(2026, 8, 29, 12, 0, 0)
            .unwrap();
        let usage = parse_usage_at(
            "Current session\n23% used\nResets in 2 hr 15 min\n\nAll models\n41% used\nResets Thu 12:00 AM\n",
            now,
        )
        .unwrap();

        assert_eq!(
            usage.short_window.unwrap().resets_at,
            Some(Utc.with_ymd_and_hms(2026, 8, 29, 11, 15, 0).unwrap())
        );
        assert_eq!(
            usage.weekly_window.unwrap().resets_at,
            Some(Utc.with_ymd_and_hms(2026, 9, 2, 21, 0, 0).unwrap())
        );
    }

    #[test]
    fn converts_current_compact_local_reset_forms_to_structured_utc_times() {
        let offset = FixedOffset::east_opt(3 * 3600).unwrap();
        let now = offset.with_ymd_and_hms(2026, 8, 29, 9, 0, 0).unwrap();
        let usage = parse_usage_at(
            "Current session\n23% used\nResets 4:00pm (Asia/Jerusalem)\n\nAll models\n41% used\nResets Aug 30, 7pm (Asia/Jerusalem)\n",
            now,
        )
        .unwrap();

        assert_eq!(
            usage.short_window.unwrap().resets_at,
            Some(Utc.with_ymd_and_hms(2026, 8, 29, 13, 0, 0).unwrap())
        );
        assert_eq!(
            usage.weekly_window.unwrap().resets_at,
            Some(Utc.with_ymd_and_hms(2026, 8, 30, 16, 0, 0).unwrap())
        );
    }

    #[test]
    fn resolves_named_iana_zone_clocks_with_winter_and_summer_dst() {
        let injected_offset = FixedOffset::east_opt(9 * 3600).unwrap();
        let cases = [
            (
                injected_offset
                    .with_ymd_and_hms(2026, 1, 15, 21, 0, 0)
                    .unwrap(),
                Utc.with_ymd_and_hms(2026, 1, 15, 14, 0, 0).unwrap(),
            ),
            (
                injected_offset
                    .with_ymd_and_hms(2026, 7, 15, 20, 0, 0)
                    .unwrap(),
                Utc.with_ymd_and_hms(2026, 7, 15, 13, 0, 0).unwrap(),
            ),
        ];

        for (now, expected) in cases {
            assert_eq!(
                parse_reset_timestamp("9am (America/New_York)", &now),
                Ok(expected)
            );
        }
    }

    #[test]
    fn derives_named_zone_calendar_date_across_the_injected_year_boundary() {
        let now = FixedOffset::east_opt(14 * 3600)
            .unwrap()
            .with_ymd_and_hms(2027, 1, 1, 2, 0, 0)
            .unwrap();

        assert_eq!(
            parse_reset_timestamp("Dec 31, 8am (America/New_York)", &now),
            Ok(Utc.with_ymd_and_hms(2026, 12, 31, 13, 0, 0).unwrap())
        );
    }

    #[test]
    fn chooses_the_earliest_future_occurrence_for_an_ambiguous_named_zone_clock() {
        let injected_offset = FixedOffset::east_opt(9 * 3600).unwrap();
        let before_both = injected_offset
            .with_ymd_and_hms(2026, 11, 1, 12, 0, 0)
            .unwrap();
        let between_occurrences = injected_offset
            .with_ymd_and_hms(2026, 11, 1, 15, 0, 0)
            .unwrap();

        assert_eq!(
            parse_reset_timestamp("1:30am (America/New_York)", &before_both),
            Ok(Utc.with_ymd_and_hms(2026, 11, 1, 5, 30, 0).unwrap())
        );
        assert_eq!(
            parse_reset_timestamp("1:30am (America/New_York)", &between_occurrences),
            Ok(Utc.with_ymd_and_hms(2026, 11, 1, 6, 30, 0).unwrap())
        );
    }

    #[test]
    fn rejects_nonexistent_local_clocks_and_unknown_named_zones() {
        let now = FixedOffset::east_opt(9 * 3600)
            .unwrap()
            .with_ymd_and_hms(2026, 3, 8, 14, 0, 0)
            .unwrap();

        assert_eq!(
            parse_reset_timestamp("2:30am (America/New_York)", &now),
            Err(ProviderError::UnsupportedOutput)
        );
        assert_eq!(
            parse_reset_timestamp("9am (America/Not_A_Zone)", &now),
            Err(ProviderError::UnsupportedOutput)
        );
    }

    #[test]
    fn reset_parser_handles_local_day_and_year_boundaries_without_guessing_past_times() {
        let offset = FixedOffset::west_opt(5 * 3600).unwrap();
        let late = offset.with_ymd_and_hms(2026, 12, 31, 23, 30, 0).unwrap();
        let usage = parse_usage_at(
            "Current session\n23% used\nResets 8:40 PM\n\nAll models\n41% used\nResets Jan 1 at 1:00 AM\n",
            late,
        )
        .unwrap();

        assert_eq!(
            usage.short_window.unwrap().resets_at,
            Some(Utc.with_ymd_and_hms(2027, 1, 2, 1, 40, 0).unwrap())
        );
        assert_eq!(
            usage.weekly_window.unwrap().resets_at,
            Some(Utc.with_ymd_and_hms(2027, 1, 1, 6, 0, 0).unwrap())
        );
    }

    #[test]
    fn rejects_unrecognized_control_and_oversized_reset_text_before_ipc() {
        for reset in [
            "whenever Claude feels ready".to_string(),
            "Thu 12:00 AM\u{0007}".to_string(),
            "x".repeat(81),
            "in 999999999 hr".to_string(),
            "4:00pm (Asia/../Jerusalem)".to_string(),
            "4:00PM (Asia/Jerusalem)".to_string(),
            "0001:00pm (Asia/Jerusalem)".to_string(),
            "0001:00 PM".to_string(),
            "1:000 PM".to_string(),
            "Aug 30 7pm (Asia/Jerusalem)".to_string(),
            "4:00pm (A/B/C/D/E)".to_string(),
            "4:00pm Asia/Jerusalem".to_string(),
        ] {
            let output = format!(
                "Current session\n23% used\nResets {reset}\n\nAll models\n41% used\nResets Thu 12:00 AM\n"
            );
            assert_eq!(
                parse_usage_at(&output, Utc.with_ymd_and_hms(2026, 8, 29, 9, 0, 0).unwrap(),),
                Err(ProviderError::UnsupportedOutput),
                "accepted reset label {reset:?}"
            );
        }
    }

    #[test]
    fn claude_usage_serializes_only_structured_reset_times() {
        let usage = parse_usage_at(
            "Current session\n23% used\nResets in 2 hr\n\nAll models\n41% used\nResets Sep 3 at 2:00 PM\n",
            Utc.with_ymd_and_hms(2026, 8, 29, 9, 0, 0).unwrap(),
        )
        .unwrap();
        let snapshot = crate::dashboard::models::UsageSnapshot::connected(
            usage,
            Utc.with_ymd_and_hms(2026, 8, 29, 9, 0, 0).unwrap(),
        );
        let json = serde_json::to_value(snapshot).unwrap();

        assert!(json["shortWindow"]["resetsAt"].is_string());
        assert!(json["weeklyWindow"]["resetsAt"].is_string());
        assert!(json["shortWindow"].get("resetLabel").is_none());
        assert!(json["weeklyWindow"].get("resetLabel").is_none());
    }

    #[test]
    fn accepts_the_legacy_all_models_general_heading() {
        let usage = parse_usage(
            "Current session\n51% used\nResets 8:40 PM\nCurrent week (all models)\n36% used\nResets Thu 12:00 AM\n",
        )
        .unwrap();

        assert_eq!(usage.short_window.unwrap().remaining_percent, 49);
        assert_eq!(usage.weekly_window.unwrap().remaining_percent, 64);
    }

    #[test]
    fn rejects_non_plan_output() {
        assert_eq!(
            parse_usage("Session cost: $0.00"),
            Err(ProviderError::UnsupportedOutput)
        );
    }

    #[test]
    fn excludes_model_specific_windows_from_the_remaining_allowance() {
        let output = "Current session\n10% used\nResets in 1 hr\n\nAll models\n20% used\nResets Sep 3 at 2:00 PM\n\nCurrent week (Sonnet)\n99% used\nResets Sep 3 at 2:00 PM\n";
        let usage = parse_usage(output).unwrap();

        assert_eq!(usage.short_window.unwrap().remaining_percent, 90);
        assert_eq!(usage.weekly_window.unwrap().remaining_percent, 80);
    }

    #[test]
    fn rejects_localized_or_drifted_general_headings() {
        let localized = "Current session\n23% used\nResets in 2 hr\n\nAll models\n41% used\nאיפוס Sep 3 at 2:00 PM\n";
        let drifted = "Current session\n23% used\nResets in 2 hr\n\nCurrent week\n41% used\nResets Sep 3 at 2:00 PM\n";

        assert_eq!(
            parse_usage(localized),
            Err(ProviderError::UnsupportedOutput)
        );
        assert_eq!(parse_usage(drifted), Err(ProviderError::UnsupportedOutput));
    }

    #[test]
    fn rejects_malformed_general_window_percentages() {
        let output = "Current session\n101% used\nResets in 2 hr\n\nAll models\n41% used\nResets Sep 3 at 2:00 PM\n";

        assert_eq!(parse_usage(output), Err(ProviderError::UnsupportedOutput));
    }

    #[test]
    fn rejects_duplicate_or_missing_required_general_headings() {
        let duplicate = "Current session\n23% used\nResets in 2 hr\n\nCurrent session\n41% used\nResets Sep 3 at 2:00 PM\n\nAll models\n41% used\nResets Sep 3 at 2:00 PM\n";
        let missing = "Current session\n23% used\nResets in 2 hr\n";

        assert_eq!(
            parse_usage(duplicate),
            Err(ProviderError::UnsupportedOutput)
        );
        assert_eq!(parse_usage(missing), Err(ProviderError::UnsupportedOutput));
    }

    #[test]
    fn ignores_the_interactive_command_echo_before_the_usage_windows() {
        let output = "/usage\r\nCurrent session\n23% used\nResets in 2 hr\n\nAll models\n41% used\nResets Sep 3 at 2:00 PM\n\nCurrent week (model-specific preview)\n";
        let usage = parse_usage(output).unwrap();

        assert_eq!(usage.short_window.unwrap().remaining_percent, 77);
        assert_eq!(usage.weekly_window.unwrap().remaining_percent, 59);
    }

    type CaptureCall = (AllowedProgram, Vec<String>, Duration);
    type InteractiveCall = (
        AllowedProgram,
        Vec<String>,
        String,
        Vec<CompletionMarker>,
        Option<String>,
        Duration,
    );

    #[derive(Clone)]
    struct RecordingCaptureRunner {
        calls: std::sync::Arc<Mutex<Vec<CaptureCall>>>,
        results: std::sync::Arc<Mutex<VecDeque<Result<CapturedOutput, ProcessError>>>>,
        delays: std::sync::Arc<Mutex<VecDeque<Duration>>>,
    }

    impl RecordingCaptureRunner {
        fn with_results(results: Vec<Result<CapturedOutput, ProcessError>>) -> Self {
            let delays = vec![Duration::ZERO; results.len()];
            Self::with_delayed_results(results, delays)
        }

        fn with_delayed_results(
            results: Vec<Result<CapturedOutput, ProcessError>>,
            delays: Vec<Duration>,
        ) -> Self {
            Self {
                calls: std::sync::Arc::new(Mutex::new(Vec::new())),
                results: std::sync::Arc::new(Mutex::new(results.into())),
                delays: std::sync::Arc::new(Mutex::new(delays.into())),
            }
        }
    }

    #[async_trait]
    impl CaptureRunner for RecordingCaptureRunner {
        async fn capture(
            &self,
            program: AllowedProgram,
            args: Vec<String>,
            timeout: Duration,
        ) -> Result<CapturedOutput, ProcessError> {
            self.calls.lock().unwrap().push((program, args, timeout));
            let delay = self.delays.lock().unwrap().pop_front().unwrap_or_default();
            if !delay.is_zero() {
                tokio::time::sleep(delay).await;
            }
            self.results.lock().unwrap().pop_front().unwrap()
        }
    }

    #[derive(Clone, Default)]
    struct RecordingInteractiveRunner {
        calls: std::sync::Arc<Mutex<Vec<InteractiveCall>>>,
        results: std::sync::Arc<Mutex<VecDeque<Result<String, ProcessError>>>>,
    }

    impl RecordingInteractiveRunner {
        fn with_results(results: Vec<Result<String, ProcessError>>) -> Self {
            Self {
                calls: std::sync::Arc::new(Mutex::new(Vec::new())),
                results: std::sync::Arc::new(Mutex::new(results.into())),
            }
        }
    }

    #[async_trait]
    impl InteractiveRunner for RecordingInteractiveRunner {
        async fn run_command(
            &self,
            program: AllowedProgram,
            args: Vec<String>,
            input: String,
            completion_markers: Vec<CompletionMarker>,
            exit_input: Option<String>,
            timeout: Duration,
        ) -> Result<String, ProcessError> {
            self.calls.lock().unwrap().push((
                program,
                args,
                input,
                completion_markers,
                exit_input,
                timeout,
            ));
            self.results.lock().unwrap().pop_front().unwrap()
        }
    }

    fn auth_status(logged_in: bool) -> CapturedOutput {
        CapturedOutput {
            stdout: format!(r#"{{"loggedIn":{logged_in}}}"#),
            stderr: String::new(),
            exit_code: 0,
        }
    }

    #[tokio::test]
    async fn unauthenticated_status_stops_before_starting_a_pty() {
        let capture = RecordingCaptureRunner::with_results(vec![Ok(auth_status(false))]);
        let interactive = RecordingInteractiveRunner::default();
        let provider = ClaudeProvider::new(capture.clone(), interactive.clone());

        assert_eq!(provider.fetch().await, Err(ProviderError::NotAuthenticated));
        assert_eq!(
            capture.calls.lock().unwrap().as_slice(),
            &[(
                AllowedProgram::Claude,
                vec!["auth".to_owned(), "status".to_owned(), "--json".to_owned()],
                Duration::from_secs(10),
            )]
        );
        assert!(interactive.calls.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn authenticated_usage_uses_safe_mode_and_bounded_interaction() {
        let capture = RecordingCaptureRunner::with_results(vec![Ok(auth_status(true))]);
        let interactive = RecordingInteractiveRunner::with_results(vec![Ok(general_usage(23, 41))]);
        let provider = ClaudeProvider::new(capture, interactive.clone());

        let usage = provider.fetch().await.unwrap();
        assert_eq!(usage.short_window.unwrap().remaining_percent, 77);
        assert_eq!(usage.weekly_window.unwrap().remaining_percent, 59);
        let calls = interactive.calls.lock().unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].0, AllowedProgram::Claude);
        assert_eq!(calls[0].1, ["--safe-mode", "--no-chrome"]);
        assert_eq!(calls[0].2, "/usage\r");
        assert_eq!(
            calls[0].3,
            [
                CompletionMarker::Exact("Current session".to_owned()),
                CompletionMarker::ExactAlternative {
                    first: "All models".to_owned(),
                    second: "Current week (all models)".to_owned(),
                },
                CompletionMarker::Prefix("Resets ".to_owned()),
            ]
        );
        assert_eq!(calls[0].4.as_deref(), Some("/exit\r"));
        assert!(calls[0].5 > Duration::ZERO);
        assert!(calls[0].5 <= CLAUDE_TIMEOUT);
    }

    #[tokio::test(start_paused = true)]
    async fn interactive_usage_receives_only_the_remaining_provider_timeout_after_auth_delay() {
        let capture = RecordingCaptureRunner::with_delayed_results(
            vec![Ok(auth_status(true))],
            vec![Duration::from_secs(6)],
        );
        let interactive = RecordingInteractiveRunner::with_results(vec![Ok(general_usage(23, 41))]);
        let provider = ClaudeProvider::new(capture, interactive.clone());

        provider.fetch().await.unwrap();

        let calls = interactive.calls.lock().unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].5, Duration::from_secs(9));
    }

    #[tokio::test(start_paused = true)]
    async fn exhausted_provider_deadline_stops_before_starting_the_pty() {
        let capture = RecordingCaptureRunner::with_delayed_results(
            vec![Ok(auth_status(true))],
            vec![Duration::from_secs(15)],
        );
        let interactive = RecordingInteractiveRunner::with_results(vec![Ok(general_usage(23, 41))]);
        let provider = ClaudeProvider::new(capture, interactive.clone());

        assert_eq!(provider.fetch().await, Err(ProviderError::Timeout));
        assert!(interactive.calls.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn maps_auth_and_usage_runner_failures_without_leaking_output() {
        let auth_cases = [
            (ProcessError::NotInstalled, ProviderError::NotInstalled),
            (ProcessError::Timeout, ProviderError::Timeout),
            (ProcessError::NonZero(1), ProviderError::NotAuthenticated),
            (ProcessError::OutputLimit, ProviderError::Process),
            (ProcessError::Io, ProviderError::Network),
        ];
        for (error, expected) in auth_cases {
            let provider = ClaudeProvider::new(
                RecordingCaptureRunner::with_results(vec![Err(error)]),
                RecordingInteractiveRunner::default(),
            );
            assert_eq!(provider.fetch().await, Err(expected));
        }

        let usage_cases = [
            (ProcessError::NotInstalled, ProviderError::NotInstalled),
            (ProcessError::Timeout, ProviderError::Timeout),
            (ProcessError::NonZero(1), ProviderError::Process),
            (ProcessError::OutputLimit, ProviderError::Process),
            (ProcessError::Io, ProviderError::Network),
        ];
        for (error, expected) in usage_cases {
            let provider = ClaudeProvider::new(
                RecordingCaptureRunner::with_results(vec![Ok(auth_status(true))]),
                RecordingInteractiveRunner::with_results(vec![Err(error)]),
            );
            assert_eq!(provider.fetch().await, Err(expected));
        }
    }

    #[tokio::test]
    #[ignore]
    async fn live_claude_usage() {
        let provider = ClaudeProvider::new(
            crate::dashboard::process::SystemProcessRunner,
            crate::dashboard::process::SystemProcessRunner,
        );
        let data = provider.fetch().await.unwrap();

        assert!(data.short_window.unwrap().remaining_percent <= 100);
        assert!(data.weekly_window.unwrap().resets_at.is_some());
    }
}
