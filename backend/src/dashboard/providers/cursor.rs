use std::time::Duration;

use async_trait::async_trait;
use serde::Deserialize;

use crate::dashboard::{
    models::{AccountData, ProviderError},
    process::{AllowedProgram, CaptureRunner, ProcessError},
    providers::{remaining_timeout, DataProvider},
};

// Two local Node processes back to back; neither touches the network for data,
// so one shared deadline comfortably covers both cold starts.
const CURSOR_TIMEOUT: Duration = Duration::from_secs(30);
const MAX_TIER_CHARS: usize = 64;
const MAX_EMAIL_CHARS: usize = 254;

pub struct CursorProvider<C: CaptureRunner> {
    capture_runner: C,
}

impl<C: CaptureRunner> CursorProvider<C> {
    pub fn new(capture_runner: C) -> Self {
        Self { capture_runner }
    }
}

#[async_trait]
impl<C: CaptureRunner> DataProvider<AccountData> for CursorProvider<C> {
    async fn fetch(&self) -> Result<AccountData, ProviderError> {
        let deadline = tokio::time::Instant::now() + CURSOR_TIMEOUT;
        let status = self
            .capture_runner
            .capture(
                AllowedProgram::CursorAgent,
                vec![
                    "status".to_owned(),
                    "--format".to_owned(),
                    "json".to_owned(),
                ],
                remaining_timeout(deadline)?,
            )
            .await
            .map_err(map_process_error)?;
        if !parse_status(&status.stdout)? {
            return Err(ProviderError::NotAuthenticated);
        }

        let about = self
            .capture_runner
            .capture(
                AllowedProgram::CursorAgent,
                vec!["about".to_owned(), "--format".to_owned(), "json".to_owned()],
                remaining_timeout(deadline)?,
            )
            .await
            .map_err(map_process_error)?;
        parse_about(&about.stdout)
    }
}

// Unlike claude/github, cursor reports its auth state in JSON at exit 0 for both
// signed-in and signed-out sessions. A nonzero exit therefore means a broken
// invocation (for example a build without --format json), never "signed out".
fn map_process_error(error: ProcessError) -> ProviderError {
    match error {
        ProcessError::NotInstalled => ProviderError::NotInstalled,
        ProcessError::Timeout => ProviderError::Timeout,
        ProcessError::Io => ProviderError::Network,
        ProcessError::NonZero(_) | ProcessError::OutputLimit | ProcessError::JsonRpc { .. } => {
            ProviderError::Process
        }
    }
}

fn parse_status(response: &str) -> Result<bool, ProviderError> {
    let status: StatusResponse =
        serde_json::from_str(response).map_err(|_| ProviderError::UnsupportedOutput)?;
    match (status.is_authenticated, status.status.as_deref()) {
        (Some(authenticated), _) => Ok(authenticated),
        (None, Some("authenticated")) => Ok(true),
        (None, Some("unauthenticated")) => Ok(false),
        _ => Err(ProviderError::UnsupportedOutput),
    }
}

fn parse_about(response: &str) -> Result<AccountData, ProviderError> {
    let about: AboutResponse =
        serde_json::from_str(response).map_err(|_| ProviderError::UnsupportedOutput)?;

    let subscription_tier = about
        .subscription_tier
        .map(|tier| safe_display_value(tier, MAX_TIER_CHARS))
        .transpose()?;
    let account_email = about.user_email.map(safe_email).transpose()?;

    Ok(AccountData {
        subscription_tier,
        account_email,
    })
}

fn safe_display_value(value: String, max_chars: usize) -> Result<String, ProviderError> {
    let chars = value.chars().count();
    if chars == 0
        || chars > max_chars
        || value
            .chars()
            .any(|character| character.is_control() || is_invisible_format_character(character))
    {
        return Err(ProviderError::UnsupportedOutput);
    }
    Ok(value)
}

fn safe_email(value: String) -> Result<String, ProviderError> {
    let chars = value.chars().count();
    if !(3..=MAX_EMAIL_CHARS).contains(&chars)
        || !value.contains('@')
        || value.chars().any(|character| {
            character.is_control()
                || character.is_whitespace()
                || is_invisible_format_character(character)
        })
    {
        return Err(ProviderError::UnsupportedOutput);
    }
    Ok(value)
}

// Zero-width and bidirectional format characters render invisibly or reorder
// surrounding text, so a hostile CLI payload could disguise what the tile shows.
fn is_invisible_format_character(character: char) -> bool {
    matches!(
        character,
        '\u{200B}'..='\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{2060}'..='\u{2064}' | '\u{2066}'..='\u{2069}' | '\u{FEFF}'
    )
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct StatusResponse {
    status: Option<String>,
    is_authenticated: Option<bool>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AboutResponse {
    subscription_tier: Option<String>,
    user_email: Option<String>,
}

#[cfg(test)]
mod tests {
    use std::{
        collections::VecDeque,
        sync::{Arc, Mutex},
        time::Duration,
    };

    use async_trait::async_trait;

    use super::*;
    use crate::dashboard::{
        models::ProviderError,
        process::{AllowedProgram, CaptureRunner, CapturedOutput, ProcessError},
        providers::DataProvider,
    };

    const STATUS_AUTHENTICATED: &str =
        include_str!("../../../tests/fixtures/cursor-status-authenticated.json");
    const STATUS_UNAUTHENTICATED: &str =
        include_str!("../../../tests/fixtures/cursor-status-unauthenticated.json");
    const ABOUT: &str = include_str!("../../../tests/fixtures/cursor-about.json");

    fn output(stdout: &str) -> CapturedOutput {
        CapturedOutput {
            stdout: stdout.to_owned(),
            stderr: String::new(),
            exit_code: 0,
        }
    }

    type RecordedCall = (AllowedProgram, Vec<String>, Duration);

    #[derive(Clone, Default)]
    struct RecordingCaptureRunner {
        calls: Arc<Mutex<Vec<RecordedCall>>>,
        results: Arc<Mutex<VecDeque<Result<CapturedOutput, ProcessError>>>>,
        delays: Arc<Mutex<VecDeque<Duration>>>,
    }

    impl RecordingCaptureRunner {
        fn with_results(results: Vec<Result<CapturedOutput, ProcessError>>) -> Self {
            Self {
                calls: Arc::new(Mutex::new(Vec::new())),
                results: Arc::new(Mutex::new(results.into())),
                delays: Arc::new(Mutex::new(VecDeque::new())),
            }
        }

        fn with_delayed_results(
            results: Vec<Result<CapturedOutput, ProcessError>>,
            delays: Vec<Duration>,
        ) -> Self {
            Self {
                calls: Arc::new(Mutex::new(Vec::new())),
                results: Arc::new(Mutex::new(results.into())),
                delays: Arc::new(Mutex::new(delays.into())),
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

    #[tokio::test(start_paused = true)]
    async fn authenticated_account_uses_the_exact_status_then_about_commands() {
        let capture = RecordingCaptureRunner::with_results(vec![
            Ok(output(STATUS_AUTHENTICATED)),
            Ok(output(ABOUT)),
        ]);
        let provider = CursorProvider::new(capture.clone());

        let account = provider.fetch().await.unwrap();
        assert_eq!(account.subscription_tier.as_deref(), Some("pro"));
        assert_eq!(
            account.account_email.as_deref(),
            Some("fixture@example.com")
        );

        let calls = capture.calls.lock().unwrap();
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0].0, AllowedProgram::CursorAgent);
        assert_eq!(calls[0].1, ["status", "--format", "json"]);
        assert_eq!(calls[0].2, Duration::from_secs(30));
        assert_eq!(calls[1].0, AllowedProgram::CursorAgent);
        assert_eq!(calls[1].1, ["about", "--format", "json"]);
        assert!(calls[1].2 <= Duration::from_secs(30));
    }

    #[tokio::test]
    async fn exit_zero_unauthenticated_status_stops_before_about() {
        let capture =
            RecordingCaptureRunner::with_results(vec![Ok(output(STATUS_UNAUTHENTICATED))]);
        let provider = CursorProvider::new(capture.clone());

        assert_eq!(provider.fetch().await, Err(ProviderError::NotAuthenticated));
        assert_eq!(capture.calls.lock().unwrap().len(), 1);
    }

    #[tokio::test(start_paused = true)]
    async fn about_receives_only_the_remaining_shared_deadline_after_a_slow_status() {
        let capture = RecordingCaptureRunner::with_delayed_results(
            vec![Ok(output(STATUS_AUTHENTICATED)), Ok(output(ABOUT))],
            vec![Duration::from_secs(6), Duration::ZERO],
        );
        let provider = CursorProvider::new(capture.clone());

        provider.fetch().await.unwrap();

        let calls = capture.calls.lock().unwrap();
        assert_eq!(calls[1].2, Duration::from_secs(24));
    }

    #[tokio::test(start_paused = true)]
    async fn exhausted_deadline_stops_before_about() {
        let capture = RecordingCaptureRunner::with_delayed_results(
            vec![Ok(output(STATUS_AUTHENTICATED))],
            vec![Duration::from_secs(30)],
        );
        let provider = CursorProvider::new(capture.clone());

        assert_eq!(provider.fetch().await, Err(ProviderError::Timeout));
        assert_eq!(capture.calls.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn nonzero_exits_map_to_process_never_not_authenticated() {
        for stage_results in [
            vec![Err(ProcessError::NonZero(1))],
            vec![
                Ok(output(STATUS_AUTHENTICATED)),
                Err(ProcessError::NonZero(2)),
            ],
        ] {
            let capture = RecordingCaptureRunner::with_results(stage_results);
            let provider = CursorProvider::new(capture);
            assert_eq!(provider.fetch().await, Err(ProviderError::Process));
        }
    }

    #[tokio::test]
    async fn maps_runner_failures_without_leaking_output() {
        let cases = [
            (ProcessError::NotInstalled, ProviderError::NotInstalled),
            (ProcessError::Timeout, ProviderError::Timeout),
            (ProcessError::OutputLimit, ProviderError::Process),
            (ProcessError::Io, ProviderError::Network),
        ];
        for (error, expected) in cases {
            let capture = RecordingCaptureRunner::with_results(vec![Err(error)]);
            let provider = CursorProvider::new(capture);
            assert_eq!(provider.fetch().await, Err(expected));
        }
    }

    #[test]
    fn status_parsing_prefers_the_boolean_and_falls_back_to_the_status_string() {
        assert!(parse_status(STATUS_AUTHENTICATED).unwrap());
        assert!(!parse_status(STATUS_UNAUTHENTICATED).unwrap());
        assert!(parse_status(r#"{"status":"authenticated"}"#).unwrap());
        assert!(!parse_status(r#"{"status":"unauthenticated"}"#).unwrap());
        assert_eq!(
            parse_status(r#"{"message":"Not logged in"}"#),
            Err(ProviderError::UnsupportedOutput)
        );
        assert_eq!(
            parse_status("Not logged in"),
            Err(ProviderError::UnsupportedOutput)
        );
    }

    #[test]
    fn absent_tier_and_email_stay_absent() {
        let account =
            parse_about(r#"{"cliVersion":"1","subscriptionTier":null,"userEmail":null}"#).unwrap();
        assert!(account.subscription_tier.is_none());
        assert!(account.account_email.is_none());
    }

    #[test]
    fn unsafe_tier_or_email_values_are_rejected() {
        for about in [
            r#"{"subscriptionTier":""}"#,
            "{\"subscriptionTier\":\"bad\ttier\"}",
            r#"{"userEmail":"no-at-sign"}"#,
            r#"{"userEmail":"two words@example.com"}"#,
        ] {
            assert_eq!(parse_about(about), Err(ProviderError::UnsupportedOutput));
        }
    }

    #[test]
    fn invisible_format_characters_are_rejected() {
        for about in [
            "{\"subscriptionTier\":\"pro\u{200B}\"}",
            "{\"subscriptionTier\":\"\u{202E}orp\"}",
            "{\"subscriptionTier\":\"\u{FEFF}pro\"}",
            "{\"userEmail\":\"user\u{200D}@example.com\"}",
            "{\"userEmail\":\"user@\u{2066}example.com\u{2069}\"}",
        ] {
            assert_eq!(parse_about(about), Err(ProviderError::UnsupportedOutput));
        }
    }
}
