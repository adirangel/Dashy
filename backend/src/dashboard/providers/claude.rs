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
    process::{AllowedProgram, CaptureRunner, ProcessError},
    providers::{remaining_timeout, DataProvider},
};

const AUTH_TIMEOUT: Duration = Duration::from_secs(10);
const CLAUDE_TIMEOUT: Duration = Duration::from_secs(30);
const MAX_RESET_TEXT_SCALARS: usize = 80;
const MAX_RELATIVE_RESET_MINUTES: i64 = 366 * 24 * 60;
const SESSION_SUMMARY_PREFIX: &str = "Current session: ";
const WEEK_SUMMARY_PREFIX: &str = "Current week (all models): ";
const COMPACT_WINDOW_DELIMITER: &str = "% used · resets ";

pub struct ClaudeProvider<C: CaptureRunner> {
    capture_runner: C,
}

impl<C: CaptureRunner> ClaudeProvider<C> {
    pub fn new(capture_runner: C) -> Self {
        Self { capture_runner }
    }
}

#[async_trait]
impl<C: CaptureRunner> DataProvider<UsageData> for ClaudeProvider<C> {
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
            .capture_runner
            .capture(
                AllowedProgram::Claude,
                [
                    "--safe-mode",
                    "--no-chrome",
                    "--no-session-persistence",
                    "--print",
                    "/usage",
                    "--output-format",
                    "json",
                ]
                .into_iter()
                .map(str::to_owned)
                .collect(),
                remaining_timeout(deadline)?,
            )
            .await
            .map_err(map_usage_process_error)?;
        parse_usage_response(&output.stdout)
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

fn parse_usage_response(response: &str) -> Result<UsageData, ProviderError> {
    parse_usage_response_at(response, Local::now())
}

fn parse_usage_response_at<Tz>(
    response: &str,
    now: DateTime<Tz>,
) -> Result<UsageData, ProviderError>
where
    Tz: TimeZone,
{
    let response: UsageCommandResponse =
        serde_json::from_str(response).map_err(|_| ProviderError::UnsupportedOutput)?;

    if response.response_type != "result" {
        return Err(ProviderError::UnsupportedOutput);
    }
    if response.is_error {
        return Err(ProviderError::Process);
    }
    if response.subtype != "success" || response.result.trim().is_empty() {
        return Err(ProviderError::UnsupportedOutput);
    }

    parse_usage_summary_at(&response.result, now.clone())
        .or_else(|_| parse_legacy_usage_summary_at(&response.result, now))
}

fn parse_usage_summary_at<Tz>(summary: &str, now: DateTime<Tz>) -> Result<UsageData, ProviderError>
where
    Tz: TimeZone,
{
    let mut session = None;
    let mut week = None;

    for raw_line in summary.lines() {
        let line = raw_line.trim_end_matches('\r');
        let (slot, value) = if let Some(value) = line.strip_prefix(SESSION_SUMMARY_PREFIX) {
            (&mut session, value)
        } else if let Some(value) = line.strip_prefix(WEEK_SUMMARY_PREFIX) {
            (&mut week, value)
        } else {
            if line.starts_with("Current session") || line.starts_with("Current week (all models)")
            {
                return Err(ProviderError::UnsupportedOutput);
            }
            continue;
        };

        if slot.is_some() {
            return Err(ProviderError::UnsupportedOutput);
        }
        *slot = Some(parse_compact_window(value, &now)?);
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

fn parse_compact_window<Tz>(value: &str, now: &DateTime<Tz>) -> Result<UsageWindow, ProviderError>
where
    Tz: TimeZone,
{
    let (percent, reset_text) = value
        .split_once(COMPACT_WINDOW_DELIMITER)
        .filter(|(_, reset)| !reset.is_empty() && !reset.contains(COMPACT_WINDOW_DELIMITER))
        .ok_or(ProviderError::UnsupportedOutput)?;
    let used_percent = percent
        .parse::<u8>()
        .ok()
        .filter(|parsed| *parsed <= 100 && parsed.to_string() == percent)
        .ok_or(ProviderError::UnsupportedOutput)?;
    let resets_at = parse_reset_timestamp(reset_text, now)?;

    Ok(UsageWindow {
        used_percent,
        resets_at,
    })
}

fn parse_legacy_usage_summary_at<Tz>(
    summary: &str,
    now: DateTime<Tz>,
) -> Result<UsageData, ProviderError>
where
    Tz: TimeZone,
{
    let mut sections = summary.split('\n').peekable();
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
            _ if is_excluded_legacy_heading(heading) => {
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
            *slot = Some(parse_legacy_window(&lines, &now)?);
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

fn is_excluded_legacy_heading(heading: &str) -> bool {
    heading.starts_with("Current week (") && heading.ends_with(')')
}

fn parse_legacy_window<Tz>(lines: &[&str], now: &DateTime<Tz>) -> Result<UsageWindow, ProviderError>
where
    Tz: TimeZone,
{
    if lines.len() != 2 {
        return Err(ProviderError::UnsupportedOutput);
    }

    let percent = lines[0]
        .strip_suffix("% used")
        .ok_or(ProviderError::UnsupportedOutput)?;
    let used_percent = percent
        .parse::<u8>()
        .ok()
        .filter(|parsed| *parsed <= 100 && parsed.to_string() == percent)
        .ok_or(ProviderError::UnsupportedOutput)?;
    let reset_text = lines[1]
        .strip_prefix("Resets ")
        .filter(|value| !value.is_empty())
        .ok_or(ProviderError::UnsupportedOutput)?;

    Ok(UsageWindow {
        used_percent,
        resets_at: parse_reset_timestamp(reset_text, now)?,
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

#[derive(Deserialize)]
struct UsageCommandResponse {
    #[serde(rename = "type")]
    response_type: String,
    subtype: String,
    is_error: bool,
    result: String,
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

    fn compact_summary(session_used_percent: u8, week_used_percent: u8) -> String {
        format!(
            "You are currently using your subscription to power your Claude Code usage\n\nCurrent session: {session_used_percent}% used · resets in 2 hr\nCurrent week (all models): {week_used_percent}% used · resets Sep 3 at 2:00 PM\nCurrent week (Fable): 99% used\n"
        )
    }

    fn usage_response(result: &str) -> String {
        serde_json::json!({
            "is_error": false,
            "duration_api_ms": 0,
            "num_turns": 0,
            "stop_reason": null,
            "session_id": "fixture",
            "total_cost_usd": 0,
            "usage": {},
            "modelUsage": {},
            "permission_denials": [],
            "subtype": "success",
            "result": result,
            "type": "result",
            "duration_ms": 4495,
            "uuid": "fixture",
            "queued_turn_count": 0
        })
        .to_string()
    }

    fn auth_status(logged_in: bool) -> CapturedOutput {
        CapturedOutput {
            stdout: format!(r#"{{"loggedIn":{logged_in}}}"#),
            stderr: String::new(),
            exit_code: 0,
        }
    }

    fn usage_output(result: &str) -> CapturedOutput {
        CapturedOutput {
            stdout: usage_response(result),
            stderr: String::new(),
            exit_code: 0,
        }
    }

    #[test]
    fn unauthenticated_status_is_detected() {
        assert!(!parse_auth_status(r#"{"loggedIn":false,"authMethod":"none"}"#).unwrap());
    }

    #[test]
    fn parses_v2_1_251_json_and_keeps_only_general_windows() {
        let now = FixedOffset::east_opt(3 * 3600)
            .unwrap()
            .with_ymd_and_hms(2026, 8, 31, 12, 0, 0)
            .unwrap();
        let response = usage_response(
            "You are currently using your subscription to power your Claude Code usage\n\nCurrent session: 2% used · resets Aug 31, 10:20pm (Asia/Jerusalem)\nCurrent week (all models): 4% used · resets Sep 6, 6pm (Asia/Jerusalem)\nCurrent week (Fable): 87% used\n\nWhat's contributing to your limits usage?\nLast 7d · 1425 requests · 6 sessions",
        );

        let usage = parse_usage_response_at(&response, now).unwrap();
        let short = usage.short_window.unwrap();
        let weekly = usage.weekly_window.unwrap();

        assert_eq!(short.remaining_percent, 98);
        assert_eq!(
            short.resets_at,
            Some(Utc.with_ymd_and_hms(2026, 8, 31, 19, 20, 0).unwrap())
        );
        assert_eq!(weekly.remaining_percent, 96);
        assert_eq!(
            weekly.resets_at,
            Some(Utc.with_ymd_and_hms(2026, 9, 6, 15, 0, 0).unwrap())
        );
    }

    #[test]
    fn accepts_the_legacy_multiline_usage_result_without_restoring_pty_scraping() {
        let now = FixedOffset::east_opt(3 * 3600)
            .unwrap()
            .with_ymd_and_hms(2026, 8, 29, 12, 0, 0)
            .unwrap();
        let response = usage_response(
            "Current session\n23% used\nResets in 2 hr\n\nAll models\n41% used\nResets Thu 12:00 AM\n\nCurrent week (Fable)\n99% used\nResets Thu 12:00 AM",
        );

        let usage = parse_usage_response_at(&response, now).unwrap();

        assert_eq!(usage.short_window.unwrap().remaining_percent, 77);
        assert_eq!(usage.weekly_window.unwrap().remaining_percent, 59);
    }

    #[test]
    fn rejects_malformed_error_and_non_result_json_envelopes() {
        assert_eq!(
            parse_usage_response("{not-json"),
            Err(ProviderError::UnsupportedOutput)
        );
        assert_eq!(
            parse_usage_response(
                r#"{"type":"result","subtype":"error","is_error":true,"result":"authentication failed"}"#
            ),
            Err(ProviderError::Process)
        );
        assert_eq!(
            parse_usage_response(
                r#"{"type":"assistant","subtype":"success","is_error":false,"result":"ignored"}"#
            ),
            Err(ProviderError::UnsupportedOutput)
        );
        assert_eq!(
            parse_usage_response(
                r#"{"type":"result","subtype":"success","is_error":false,"result":""}"#
            ),
            Err(ProviderError::UnsupportedOutput)
        );
        assert_eq!(
            parse_usage_response(r#"{"type":"result","subtype":"success","is_error":false}"#),
            Err(ProviderError::UnsupportedOutput)
        );
        let valid = usage_response(&compact_summary(23, 41));
        assert_eq!(
            parse_usage_response(&(valid + "\n{}")),
            Err(ProviderError::UnsupportedOutput)
        );
    }

    #[test]
    fn rejects_duplicate_missing_and_malformed_required_summaries() {
        let duplicate = "Current session: 23% used · resets in 2 hr\nCurrent session: 24% used · resets in 3 hr\nCurrent week (all models): 41% used · resets Sep 3 at 2:00 PM";
        let missing = "Current session: 23% used · resets in 2 hr";
        let malformed = "Current session: 023% used · resets in 2 hr\nCurrent week (all models): 41% used - resets Sep 3 at 2:00 PM";

        for summary in [duplicate, missing, malformed] {
            assert_eq!(
                parse_usage_response(&usage_response(summary)),
                Err(ProviderError::UnsupportedOutput)
            );
        }
    }

    #[test]
    fn ignores_other_model_buckets_without_accepting_drifted_general_headings() {
        let usage = parse_usage_response(&usage_response(
            "Current session: 10% used · resets in 1 hr\nCurrent week (all models): 20% used · resets Sep 3 at 2:00 PM\nCurrent week (Sonnet): 99% used · resets Sep 3 at 2:00 PM\nCurrent week (Opus): malformed",
        ))
        .unwrap();

        assert_eq!(usage.short_window.unwrap().remaining_percent, 90);
        assert_eq!(usage.weekly_window.unwrap().remaining_percent, 80);

        for summary in [
            "Current session 10% used · resets in 1 hr\nCurrent week (all models): 20% used · resets Sep 3 at 2:00 PM",
            "Current session: 10% used · resets in 1 hr\nCurrent week (all models) 20% used · resets Sep 3 at 2:00 PM",
            "Current session: 101% used · resets in 1 hr\nCurrent week (all models): 20% used · resets Sep 3 at 2:00 PM",
        ] {
            assert_eq!(
                parse_usage_response(&usage_response(summary)),
                Err(ProviderError::UnsupportedOutput)
            );
        }
    }

    #[test]
    fn converts_allowlisted_relative_and_weekday_resets_to_structured_utc_times() {
        let now = FixedOffset::east_opt(3 * 3600)
            .unwrap()
            .with_ymd_and_hms(2026, 8, 29, 12, 0, 0)
            .unwrap();
        let usage = parse_usage_summary_at(
            "Current session: 23% used · resets in 2 hr 15 min\nCurrent week (all models): 41% used · resets Thu 12:00 AM",
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
    fn reset_parser_rejects_nonexistent_unknown_control_and_oversized_values() {
        let now = FixedOffset::east_opt(9 * 3600)
            .unwrap()
            .with_ymd_and_hms(2026, 3, 8, 14, 0, 0)
            .unwrap();

        for reset in [
            "2:30am (America/New_York)".to_owned(),
            "9am (America/Not_A_Zone)".to_owned(),
            "Thu 12:00 AM\u{0007}".to_owned(),
            "x".repeat(81),
            "in 999999999 hr".to_owned(),
            "4:00pm (Asia/../Jerusalem)".to_owned(),
            "4:00PM (Asia/Jerusalem)".to_owned(),
            "Aug 30 7pm (Asia/Jerusalem)".to_owned(),
        ] {
            assert_eq!(
                parse_reset_timestamp(&reset, &now),
                Err(ProviderError::UnsupportedOutput),
                "accepted reset label {reset:?}"
            );
        }
    }

    #[test]
    fn reset_parser_handles_local_day_and_year_boundaries() {
        let offset = FixedOffset::west_opt(5 * 3600).unwrap();
        let late = offset.with_ymd_and_hms(2026, 12, 31, 23, 30, 0).unwrap();
        let usage = parse_usage_summary_at(
            "Current session: 23% used · resets 8:40 PM\nCurrent week (all models): 41% used · resets Jan 1 at 1:00 AM",
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
    fn claude_usage_serializes_only_structured_reset_times() {
        let usage = parse_usage_summary_at(
            "Current session: 23% used · resets in 2 hr\nCurrent week (all models): 41% used · resets Sep 3 at 2:00 PM",
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

    type CaptureCall = (AllowedProgram, Vec<String>, Duration);

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

    #[tokio::test]
    async fn unauthenticated_status_stops_before_usage_capture() {
        let capture = RecordingCaptureRunner::with_results(vec![Ok(auth_status(false))]);
        let provider = ClaudeProvider::new(capture.clone());

        assert_eq!(provider.fetch().await, Err(ProviderError::NotAuthenticated));
        assert_eq!(
            capture.calls.lock().unwrap().as_slice(),
            &[(
                AllowedProgram::Claude,
                vec!["auth".to_owned(), "status".to_owned(), "--json".to_owned()],
                Duration::from_secs(10),
            )]
        );
    }

    #[tokio::test]
    async fn authenticated_usage_uses_the_exact_noninteractive_command() {
        let capture = RecordingCaptureRunner::with_results(vec![
            Ok(auth_status(true)),
            Ok(usage_output(&compact_summary(23, 41))),
        ]);
        let provider = ClaudeProvider::new(capture.clone());

        let usage = provider.fetch().await.unwrap();
        assert_eq!(usage.short_window.unwrap().remaining_percent, 77);
        assert_eq!(usage.weekly_window.unwrap().remaining_percent, 59);

        let calls = capture.calls.lock().unwrap();
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[1].0, AllowedProgram::Claude,);
        assert_eq!(
            calls[1].1,
            [
                "--safe-mode",
                "--no-chrome",
                "--no-session-persistence",
                "--print",
                "/usage",
                "--output-format",
                "json",
            ]
        );
        assert!(calls[1].2 > Duration::ZERO);
        assert!(calls[1].2 <= CLAUDE_TIMEOUT);
    }

    #[tokio::test(start_paused = true)]
    async fn usage_capture_receives_only_the_remaining_total_timeout_after_auth_delay() {
        let capture = RecordingCaptureRunner::with_delayed_results(
            vec![
                Ok(auth_status(true)),
                Ok(usage_output(&compact_summary(23, 41))),
            ],
            vec![Duration::from_secs(6), Duration::ZERO],
        );
        let provider = ClaudeProvider::new(capture.clone());

        provider.fetch().await.unwrap();

        let calls = capture.calls.lock().unwrap();
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[1].2, Duration::from_secs(24));
    }

    #[tokio::test(start_paused = true)]
    async fn exhausted_provider_deadline_stops_before_usage_capture() {
        let capture = RecordingCaptureRunner::with_delayed_results(
            vec![Ok(auth_status(true))],
            vec![Duration::from_secs(30)],
        );
        let provider = ClaudeProvider::new(capture.clone());

        assert_eq!(provider.fetch().await, Err(ProviderError::Timeout));
        assert_eq!(capture.calls.lock().unwrap().len(), 1);
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
            let provider =
                ClaudeProvider::new(RecordingCaptureRunner::with_results(vec![Err(error)]));
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
            let provider = ClaudeProvider::new(RecordingCaptureRunner::with_results(vec![
                Ok(auth_status(true)),
                Err(error),
            ]));
            assert_eq!(provider.fetch().await, Err(expected));
        }
    }

    #[tokio::test]
    #[ignore]
    async fn live_claude_usage() {
        let provider = ClaudeProvider::new(crate::dashboard::process::SystemProcessRunner);
        let data = provider.fetch().await.unwrap();

        assert!(data.short_window.unwrap().remaining_percent <= 100);
        assert!(data.weekly_window.unwrap().resets_at.is_some());
    }
}
