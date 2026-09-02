use std::{collections::BTreeMap, time::Duration};

use async_trait::async_trait;
use chrono::{Local, NaiveDate};
use serde::Deserialize;

use crate::dashboard::{
    models::{ContributionDay, GitHubData, ProviderError},
    process::{AllowedProgram, CaptureRunner, ProcessError},
    providers::{remaining_timeout, DataProvider},
};

const GITHUB_TIMEOUT: Duration = Duration::from_secs(40);
const CONTRIBUTIONS_QUERY: &str = "query DashyContributions {\n  viewer {\n    login\n    contributionsCollection {\n      contributionCalendar {\n        weeks {\n          contributionDays { date contributionCount contributionLevel }\n        }\n      }\n    }\n  }\n}";

pub struct GitHubProvider<R: CaptureRunner> {
    runner: R,
}

impl<R: CaptureRunner> GitHubProvider<R> {
    pub fn new(runner: R) -> Self {
        Self { runner }
    }

    async fn fetch_at(&self, today: NaiveDate) -> Result<GitHubData, ProviderError> {
        let deadline = tokio::time::Instant::now() + GITHUB_TIMEOUT;
        self.runner
            .capture(
                AllowedProgram::Gh,
                vec![
                    "auth".to_owned(),
                    "status".to_owned(),
                    "--active".to_owned(),
                ],
                remaining_timeout(deadline)?,
            )
            .await
            .map_err(map_auth_process_error)?;

        let output = self
            .runner
            .capture(
                AllowedProgram::Gh,
                vec![
                    "api".to_owned(),
                    "graphql".to_owned(),
                    "-f".to_owned(),
                    format!("query={CONTRIBUTIONS_QUERY}"),
                ],
                remaining_timeout(deadline)?,
            )
            .await
            .map_err(map_graphql_process_error)?;

        parse_response(&output.stdout, today)
    }
}

#[async_trait]
impl<R: CaptureRunner> DataProvider<GitHubData> for GitHubProvider<R> {
    async fn fetch(&self) -> Result<GitHubData, ProviderError> {
        self.fetch_at(Local::now().date_naive()).await
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

fn map_graphql_process_error(error: ProcessError) -> ProviderError {
    match error {
        ProcessError::NotInstalled => ProviderError::NotInstalled,
        ProcessError::Timeout => ProviderError::Timeout,
        ProcessError::NonZero(_) | ProcessError::OutputLimit | ProcessError::JsonRpc { .. } => {
            ProviderError::Process
        }
        ProcessError::Io => ProviderError::Network,
    }
}

fn parse_response(response: &str, today: NaiveDate) -> Result<GitHubData, ProviderError> {
    let response: GraphqlResponse =
        serde_json::from_str(response).map_err(|_| ProviderError::UnsupportedOutput)?;
    let viewer = response.data.viewer;
    if !is_safe_login(&viewer.login) {
        return Err(ProviderError::UnsupportedOutput);
    }

    let mut dates = BTreeMap::new();
    for week in viewer.contributions_collection.contribution_calendar.weeks {
        for day in week.contribution_days {
            let level = contribution_level(&day.contribution_level)?;
            if dates
                .insert(
                    day.date,
                    ContributionDay {
                        date: day.date,
                        count: day.contribution_count,
                        level,
                    },
                )
                .is_some()
            {
                return Err(ProviderError::UnsupportedOutput);
            }
        }
    }

    if !dates.contains_key(&today) {
        return Err(ProviderError::UnsupportedOutput);
    }

    let contribution_days: Vec<_> = dates.into_values().collect();
    Ok(GitHubData {
        account_login: viewer.login,
        current_streak_days: calculate_streak(&contribution_days, today),
        contribution_days,
    })
}

fn is_safe_login(login: &str) -> bool {
    !login.is_empty()
        && login.len() <= 39
        && !login.starts_with('-')
        && !login.ends_with('-')
        && login
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
}

fn contribution_level(level: &str) -> Result<u8, ProviderError> {
    match level {
        "NONE" => Ok(0),
        "FIRST_QUARTILE" => Ok(1),
        "SECOND_QUARTILE" => Ok(2),
        "THIRD_QUARTILE" => Ok(3),
        "FOURTH_QUARTILE" => Ok(4),
        _ => Err(ProviderError::UnsupportedOutput),
    }
}

fn calculate_streak(days: &[ContributionDay], today: NaiveDate) -> u32 {
    let counts: BTreeMap<_, _> = days.iter().map(|day| (day.date, day.count)).collect();
    let mut current_date = if counts.get(&today).is_some_and(|count| *count > 0) {
        Some(today)
    } else {
        today.pred_opt()
    };
    let mut streak = 0;

    while let Some(date) = current_date {
        if counts.get(&date).is_none_or(|count| *count == 0) {
            break;
        }
        streak += 1;
        current_date = date.pred_opt();
    }

    streak
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct GraphqlResponse {
    data: GraphqlData,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct GraphqlData {
    viewer: Viewer,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Viewer {
    login: String,
    #[serde(rename = "contributionsCollection")]
    contributions_collection: ContributionsCollection,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ContributionsCollection {
    #[serde(rename = "contributionCalendar")]
    contribution_calendar: ContributionCalendar,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ContributionCalendar {
    weeks: Vec<ContributionWeek>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ContributionWeek {
    #[serde(rename = "contributionDays")]
    contribution_days: Vec<RawContributionDay>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawContributionDay {
    date: NaiveDate,
    #[serde(rename = "contributionCount")]
    contribution_count: u32,
    #[serde(rename = "contributionLevel")]
    contribution_level: String,
}

#[cfg(test)]
mod tests {
    use std::{collections::VecDeque, sync::Mutex, time::Duration};

    use async_trait::async_trait;
    use chrono::NaiveDate;

    use super::*;
    use crate::dashboard::{
        models::{ContributionDay, ProviderError},
        process::{AllowedProgram, CaptureRunner, CapturedOutput, ProcessError},
        providers::DataProvider,
    };

    fn date(year: i32, month: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(year, month, day).unwrap()
    }

    fn fixture_days(values: &[(&str, u32)]) -> Vec<ContributionDay> {
        values
            .iter()
            .map(|(value, count)| ContributionDay {
                date: NaiveDate::parse_from_str(value, "%Y-%m-%d").unwrap(),
                count: *count,
                level: 0,
            })
            .collect()
    }

    #[test]
    fn keeps_yesterdays_streak_active_until_today_ends() {
        let days = fixture_days(&[("2026-08-27", 1), ("2026-08-28", 2), ("2026-08-29", 0)]);
        assert_eq!(calculate_streak(&days, date(2026, 8, 29)), 2);
    }

    #[test]
    fn crosses_year_boundary() {
        let days = fixture_days(&[("2025-12-31", 1), ("2026-01-01", 1)]);
        assert_eq!(calculate_streak(&days, date(2026, 1, 1)), 2);
    }

    #[test]
    fn parses_fixture_levels_and_login() {
        let data = parse_response(
            include_str!("../../../tests/fixtures/github-contributions.json"),
            date(2026, 8, 29),
        )
        .unwrap();
        assert_eq!(data.account_login, "fixture-user");
        assert_eq!(data.current_streak_days, 3);
        assert_eq!(
            data.contribution_days
                .iter()
                .map(|day| day.level)
                .collect::<Vec<_>>(),
            vec![1, 2, 3, 0]
        );
    }

    #[test]
    fn accepts_every_documented_contribution_level() {
        assert_eq!(contribution_level("NONE"), Ok(0));
        assert_eq!(contribution_level("FIRST_QUARTILE"), Ok(1));
        assert_eq!(contribution_level("SECOND_QUARTILE"), Ok(2));
        assert_eq!(contribution_level("THIRD_QUARTILE"), Ok(3));
        assert_eq!(contribution_level("FOURTH_QUARTILE"), Ok(4));
    }

    #[test]
    fn rejects_unrecognized_contribution_levels() {
        assert_eq!(
            contribution_level("FIFTH_QUARTILE"),
            Err(ProviderError::UnsupportedOutput)
        );
    }

    #[test]
    fn rejects_unsanitized_response_fields() {
        let response = r#"{
            "data": {
                "viewer": {
                    "login": "fixture-user",
                    "unexpected": true,
                    "contributionsCollection": {
                        "contributionCalendar": { "weeks": [] }
                    }
                }
            }
        }"#;
        assert_eq!(
            parse_response(response, date(2026, 8, 29)),
            Err(ProviderError::UnsupportedOutput)
        );
    }

    #[test]
    fn rejects_calendar_without_a_record_for_today() {
        let response = r#"{
            "data": {
                "viewer": {
                    "login": "fixture-user",
                    "contributionsCollection": {
                        "contributionCalendar": {
                            "weeks": [{
                                "contributionDays": [{
                                    "date": "2026-08-28",
                                    "contributionCount": 2,
                                    "contributionLevel": "SECOND_QUARTILE"
                                }]
                            }]
                        }
                    }
                }
            }
        }"#;

        assert_eq!(
            parse_response(response, date(2026, 8, 29)),
            Err(ProviderError::UnsupportedOutput)
        );
    }

    type CaptureCall = (AllowedProgram, Vec<String>, Duration);

    #[derive(Clone)]
    struct RecordingRunner {
        calls: std::sync::Arc<Mutex<Vec<CaptureCall>>>,
        results: std::sync::Arc<Mutex<VecDeque<Result<CapturedOutput, ProcessError>>>>,
        delays: std::sync::Arc<Mutex<VecDeque<Duration>>>,
    }

    impl RecordingRunner {
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
    impl CaptureRunner for RecordingRunner {
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

    fn successful_output() -> CapturedOutput {
        CapturedOutput {
            stdout: include_str!("../../../tests/fixtures/github-contributions.json").to_owned(),
            stderr: String::new(),
            exit_code: 0,
        }
    }

    #[tokio::test]
    async fn checks_active_auth_before_sending_graphql_within_the_twenty_second_budget() {
        let runner = RecordingRunner::with_results(vec![
            Ok(CapturedOutput {
                stdout: String::new(),
                stderr: String::new(),
                exit_code: 0,
            }),
            Ok(successful_output()),
        ]);
        let provider = GitHubProvider::new(runner.clone());

        provider.fetch_at(date(2026, 8, 29)).await.unwrap();

        let calls = runner.calls.lock().unwrap();
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0].0, AllowedProgram::Gh);
        assert_eq!(calls[0].1, ["auth", "status", "--active"]);
        assert!(calls[0].2 > Duration::ZERO);
        assert!(calls[0].2 <= GITHUB_TIMEOUT);
        assert_eq!(calls[1].0, AllowedProgram::Gh);
        assert_eq!(calls[1].1[0..3], ["api", "graphql", "-f"]);
        assert_eq!(calls[1].1.len(), 4);
        assert!(calls[1].1[3].starts_with("query="));
        assert!(calls[1].1[3].contains("query DashyContributions"));
        assert!(calls[1].2 > Duration::ZERO);
        assert!(calls[1].2 <= calls[0].2);
    }

    #[tokio::test(start_paused = true)]
    async fn graphql_receives_only_the_remaining_provider_timeout_after_auth_delay() {
        let runner = RecordingRunner::with_delayed_results(
            vec![
                Ok(CapturedOutput {
                    stdout: String::new(),
                    stderr: String::new(),
                    exit_code: 0,
                }),
                Ok(successful_output()),
            ],
            vec![Duration::from_secs(7), Duration::ZERO],
        );
        let provider = GitHubProvider::new(runner.clone());

        provider.fetch_at(date(2026, 8, 29)).await.unwrap();

        let calls = runner.calls.lock().unwrap();
        assert_eq!(calls[0].2, Duration::from_secs(40));
        assert_eq!(calls[1].2, Duration::from_secs(33));
    }

    #[tokio::test(start_paused = true)]
    async fn exhausted_provider_deadline_stops_before_graphql() {
        let runner = RecordingRunner::with_delayed_results(
            vec![
                Ok(CapturedOutput {
                    stdout: String::new(),
                    stderr: String::new(),
                    exit_code: 0,
                }),
                Ok(successful_output()),
            ],
            vec![Duration::from_secs(40), Duration::ZERO],
        );
        let provider = GitHubProvider::new(runner.clone());

        assert_eq!(provider.fetch().await, Err(ProviderError::Timeout));
        assert_eq!(runner.calls.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn classifies_auth_status_failures_before_graphql() {
        let cases = [
            (ProcessError::NotInstalled, ProviderError::NotInstalled),
            (ProcessError::Timeout, ProviderError::Timeout),
            (ProcessError::NonZero(1), ProviderError::NotAuthenticated),
            (ProcessError::Io, ProviderError::Network),
            (ProcessError::OutputLimit, ProviderError::Process),
        ];

        for (process_error, expected) in cases {
            let runner = RecordingRunner::with_results(vec![Err(process_error)]);
            let provider = GitHubProvider::new(runner.clone());
            assert_eq!(provider.fetch().await, Err(expected));
            let calls = runner.calls.lock().unwrap();
            assert_eq!(calls.len(), 1);
            assert_eq!(calls[0].0, AllowedProgram::Gh);
            assert_eq!(calls[0].1, ["auth", "status", "--active"]);
            assert!(calls[0].2 > Duration::ZERO);
            assert!(calls[0].2 <= GITHUB_TIMEOUT);
        }
    }

    #[tokio::test]
    async fn classifies_graphql_nonzero_as_process_after_active_auth() {
        let runner = RecordingRunner::with_results(vec![
            Ok(CapturedOutput {
                stdout: String::new(),
                stderr: String::new(),
                exit_code: 0,
            }),
            Err(ProcessError::NonZero(1)),
        ]);
        let provider = GitHubProvider::new(runner.clone());

        assert_eq!(provider.fetch().await, Err(ProviderError::Process));
        assert_eq!(runner.calls.lock().unwrap().len(), 2);
    }

    #[tokio::test]
    async fn preserves_network_and_timeout_errors_after_active_auth() {
        let cases = [
            (ProcessError::Timeout, ProviderError::Timeout),
            (ProcessError::Io, ProviderError::Network),
            (ProcessError::OutputLimit, ProviderError::Process),
        ];

        for (process_error, expected) in cases {
            let runner = RecordingRunner::with_results(vec![
                Ok(CapturedOutput {
                    stdout: String::new(),
                    stderr: String::new(),
                    exit_code: 0,
                }),
                Err(process_error),
            ]);
            let provider = GitHubProvider::new(runner);
            assert_eq!(provider.fetch().await, Err(expected));
        }
    }

    #[tokio::test]
    #[ignore]
    async fn live_github_contributions() {
        let provider = GitHubProvider::new(crate::dashboard::process::SystemProcessRunner);
        let data = provider.fetch().await.unwrap();

        assert!(!data.contribution_days.is_empty());
        assert!(
            data.account_login == "adirangel",
            "authenticated GitHub login did not match the expected test account"
        );
    }
}
