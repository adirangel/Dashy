use std::time::Duration;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::Deserialize;

use crate::dashboard::{
    models::{ProviderError, UsageData, UsageWindowData, UsageWindowKind},
    process::{AllowedProgram, JsonRpcRunner, ProcessError},
    providers::{remaining_timeout, DataProvider},
};

// One shared deadline for the whole fetch: the billing spawn plus, only when the
// billing method is missing, one read-only initialize probe spawn.
const GROK_TIMEOUT: Duration = Duration::from_secs(30);
// Captured live from grok 1.0.13 `agent stdio`: authenticated methods reject with
// exactly this pair while no account is signed in.
const AUTHENTICATION_ERROR_CODE: i64 = -32000;
const AUTHENTICATION_ERROR_MESSAGE: &str = "Authentication required";
const METHOD_NOT_FOUND_CODE: i64 = -32601;
const DASHY_VERSION: &str = env!("CARGO_PKG_VERSION");

pub struct GrokProvider<R: JsonRpcRunner> {
    runner: R,
}

impl<R: JsonRpcRunner> GrokProvider<R> {
    pub fn new(runner: R) -> Self {
        Self { runner }
    }
}

fn initialize_request() -> serde_json::Value {
    // grok's stdio router rejects ANY request without a params object as
    // -32602 Invalid params, so every request here carries explicit params.
    serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": 1,
            "clientCapabilities": {},
            "clientInfo": {"name": "dashy", "version": DASHY_VERSION}
        }
    })
}

#[async_trait]
impl<R: JsonRpcRunner> DataProvider<UsageData> for GrokProvider<R> {
    async fn fetch(&self) -> Result<UsageData, ProviderError> {
        let deadline = tokio::time::Instant::now() + GROK_TIMEOUT;
        let billing = self
            .runner
            .request(
                AllowedProgram::Grok,
                vec!["agent".to_owned(), "stdio".to_owned()],
                vec![
                    initialize_request(),
                    serde_json::json!({
                        "jsonrpc": "2.0",
                        "id": 2,
                        "method": "x.ai/billing",
                        "params": {}
                    }),
                ],
                2,
                remaining_timeout(deadline)?,
            )
            .await;

        match billing {
            Ok(value) => parse_billing(value),
            // Builds without stdio billing wiring answer -32601. Distinguish a
            // signed-out user from a billing-less build by re-reading initialize:
            // its _meta.defaultAuthMethodId is null until an account signs in.
            // (Live-probed alternatives all fail: session/new persists an abandoned
            // session on disk even when it rejects, and session/list answers without
            // authentication, so neither is a safe probe.)
            Err(ProcessError::JsonRpc {
                code: METHOD_NOT_FOUND_CODE,
                ..
            }) => {
                if self.probe_authentication(deadline).await? {
                    Err(ProviderError::UnsupportedOutput)
                } else {
                    Err(ProviderError::NotAuthenticated)
                }
            }
            Err(error) => Err(map_process_error(error)),
        }
    }
}

impl<R: JsonRpcRunner> GrokProvider<R> {
    async fn probe_authentication(
        &self,
        deadline: tokio::time::Instant,
    ) -> Result<bool, ProviderError> {
        let probe = self
            .runner
            .request(
                AllowedProgram::Grok,
                vec!["agent".to_owned(), "stdio".to_owned()],
                vec![initialize_request()],
                1,
                remaining_timeout(deadline)?,
            )
            .await;

        match probe {
            Ok(value) => Ok(parse_default_auth_method(&value)),
            Err(error) => Err(map_process_error(error)),
        }
    }
}

// The initialize result is upstream-owned; read only the auth signal and treat
// anything unexpected as signed out, which keeps the actionable state.
fn parse_default_auth_method(value: &serde_json::Value) -> bool {
    !value["_meta"]["defaultAuthMethodId"].is_null()
}

fn map_process_error(error: ProcessError) -> ProviderError {
    match error {
        ProcessError::NotInstalled => ProviderError::NotInstalled,
        ProcessError::Timeout => ProviderError::Timeout,
        ProcessError::JsonRpc { code, message }
            if code == AUTHENTICATION_ERROR_CODE && message == AUTHENTICATION_ERROR_MESSAGE =>
        {
            ProviderError::NotAuthenticated
        }
        ProcessError::NonZero(_) | ProcessError::OutputLimit | ProcessError::JsonRpc { .. } => {
            ProviderError::Process
        }
        ProcessError::Io => ProviderError::Network,
    }
}

fn parse_billing(value: serde_json::Value) -> Result<UsageData, ProviderError> {
    // Upstream owns this shape; keep the parse tolerant of new fields and read
    // only what the monthly window needs.
    let response: BillingResponse =
        serde_json::from_value(value).map_err(|_| ProviderError::UnsupportedOutput)?;

    let monthly_limit = response
        .monthly_limit
        .and_then(|limit| limit.val)
        .filter(|cents| cents.is_finite() && *cents > 0.0)
        .ok_or(ProviderError::UnsupportedOutput)?;
    let total_used = response
        .usage
        .and_then(|usage| usage.total_used)
        .and_then(|used| used.val)
        .filter(|cents| cents.is_finite() && *cents >= 0.0)
        .ok_or(ProviderError::UnsupportedOutput)?;

    let remaining_percent =
        (100.0 - (total_used / monthly_limit * 100.0).round()).clamp(0.0, 100.0) as u8;
    let resets_at = response
        .billing_cycle
        .and_then(|cycle| cycle.billing_period_end)
        .and_then(parse_reset_instant);

    UsageData::try_new(
        None,
        Some(UsageWindowData {
            label_key: UsageWindowKind::Monthly,
            remaining_percent,
            resets_at,
        }),
    )
}

// The billing-period end format is not documented; accept RFC3339 strings and
// epoch numbers (seconds or milliseconds), and drop anything else rather than
// failing the whole window over a cosmetic timestamp.
fn parse_reset_instant(value: serde_json::Value) -> Option<DateTime<Utc>> {
    match value {
        serde_json::Value::String(text) => DateTime::parse_from_rfc3339(&text)
            .ok()
            .map(|parsed| parsed.with_timezone(&Utc)),
        serde_json::Value::Number(number) => {
            let epoch = number.as_i64()?;
            if epoch > 100_000_000_000 {
                DateTime::from_timestamp_millis(epoch)
            } else {
                DateTime::from_timestamp(epoch, 0)
            }
        }
        _ => None,
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct BillingResponse {
    billing_cycle: Option<BillingCycle>,
    monthly_limit: Option<CentsValue>,
    usage: Option<BillingUsage>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct BillingCycle {
    billing_period_end: Option<serde_json::Value>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct BillingUsage {
    total_used: Option<CentsValue>,
}

#[derive(Deserialize)]
struct CentsValue {
    val: Option<f64>,
}

#[cfg(test)]
mod tests {
    use std::{collections::VecDeque, sync::Mutex, time::Duration};

    use async_trait::async_trait;
    use serde_json::json;

    use super::*;
    use crate::dashboard::{
        models::ProviderError,
        process::{AllowedProgram, JsonRpcRunner, ProcessError},
        providers::DataProvider,
    };

    const BILLING_FIXTURE: &str = include_str!("../../../tests/fixtures/grok-billing.json");

    type RecordedCall = (
        AllowedProgram,
        Vec<String>,
        Vec<serde_json::Value>,
        u64,
        Duration,
    );

    #[derive(Default)]
    struct RecordingRunner {
        calls: Mutex<Vec<RecordedCall>>,
        results: Mutex<VecDeque<Result<serde_json::Value, ProcessError>>>,
    }

    impl RecordingRunner {
        fn with_results(results: Vec<Result<serde_json::Value, ProcessError>>) -> Self {
            Self {
                calls: Mutex::new(Vec::new()),
                results: Mutex::new(results.into()),
            }
        }
    }

    #[async_trait]
    impl JsonRpcRunner for &RecordingRunner {
        async fn request(
            &self,
            program: AllowedProgram,
            args: Vec<String>,
            requests: Vec<serde_json::Value>,
            response_id: u64,
            timeout: Duration,
        ) -> Result<serde_json::Value, ProcessError> {
            self.calls
                .lock()
                .unwrap()
                .push((program, args, requests, response_id, timeout));
            self.results.lock().unwrap().pop_front().unwrap()
        }
    }

    fn billing_value() -> serde_json::Value {
        serde_json::from_str(BILLING_FIXTURE).unwrap()
    }

    fn method_not_found() -> ProcessError {
        ProcessError::JsonRpc {
            code: -32601,
            message: "Method not found".to_owned(),
        }
    }

    fn initialize_result(default_auth_method: Option<&str>) -> serde_json::Value {
        json!({
            "protocolVersion": 1,
            "agentCapabilities": {"loadSession": true},
            "authMethods": [{"id": "grok.com", "name": "Grok"}],
            "_meta": {
                "grokShell": true,
                "defaultAuthMethodId": default_auth_method,
                "agentVersion": "1.0.13"
            }
        })
    }

    fn authentication_required() -> ProcessError {
        ProcessError::JsonRpc {
            code: -32000,
            message: "Authentication required".to_owned(),
        }
    }

    #[tokio::test(start_paused = true)]
    async fn billing_success_uses_one_spawn_with_the_exact_handshake() {
        let runner = RecordingRunner::with_results(vec![Ok(billing_value())]);
        let provider = GrokProvider::new(&runner);

        let usage = provider.fetch().await.unwrap();
        let window = usage.weekly_window.unwrap();
        assert_eq!(window.label_key, UsageWindowKind::Monthly);
        assert_eq!(window.remaining_percent, 85);
        assert_eq!(
            window.resets_at.unwrap().to_rfc3339(),
            "2026-10-01T00:00:00+00:00"
        );
        assert!(usage.short_window.is_none());

        let calls = runner.calls.lock().unwrap();
        assert_eq!(calls.len(), 1);
        let (program, args, requests, response_id, timeout) = &calls[0];
        assert_eq!(*program, AllowedProgram::Grok);
        assert_eq!(args, &["agent", "stdio"]);
        assert_eq!(*response_id, 2);
        assert_eq!(*timeout, Duration::from_secs(30));
        assert_eq!(requests[0]["method"], "initialize");
        assert_eq!(requests[0]["params"]["protocolVersion"], 1);
        assert_eq!(requests[0]["params"]["clientInfo"]["name"], "dashy");
        assert_eq!(requests[1]["method"], "x.ai/billing");
        assert_eq!(requests[1]["params"], json!({}));
    }

    #[tokio::test]
    async fn missing_billing_method_on_an_authenticated_build_degrades_to_unsupported() {
        let runner = RecordingRunner::with_results(vec![
            Err(method_not_found()),
            Ok(initialize_result(Some("grok.com"))),
        ]);
        let provider = GrokProvider::new(&runner);

        assert_eq!(
            provider.fetch().await,
            Err(ProviderError::UnsupportedOutput)
        );

        // The probe must be a read-only initialize: session/new persists an
        // abandoned session on disk even when it rejects (live-probed on 1.0.13).
        let calls = runner.calls.lock().unwrap();
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[1].2.len(), 1);
        assert_eq!(calls[1].2[0]["method"], "initialize");
        assert_eq!(calls[1].3, 1);
    }

    #[tokio::test]
    async fn missing_billing_method_while_signed_out_maps_to_not_authenticated() {
        let runner = RecordingRunner::with_results(vec![
            Err(method_not_found()),
            Ok(initialize_result(None)),
        ]);
        let provider = GrokProvider::new(&runner);

        assert_eq!(provider.fetch().await, Err(ProviderError::NotAuthenticated));
    }

    #[tokio::test]
    async fn probe_stage_runner_failures_keep_their_own_mapping() {
        for (probe_error, expected) in [
            (ProcessError::Timeout, ProviderError::Timeout),
            (ProcessError::Io, ProviderError::Network),
            (ProcessError::NonZero(3), ProviderError::Process),
            (authentication_required(), ProviderError::NotAuthenticated),
        ] {
            let runner =
                RecordingRunner::with_results(vec![Err(method_not_found()), Err(probe_error)]);
            let provider = GrokProvider::new(&runner);
            assert_eq!(provider.fetch().await, Err(expected));
        }
    }

    #[test]
    fn unexpected_initialize_shapes_read_as_signed_out() {
        assert!(!parse_default_auth_method(&json!({})));
        assert!(!parse_default_auth_method(&json!({"_meta": {}})));
        assert!(!parse_default_auth_method(&json!([1, 2])));
        assert!(parse_default_auth_method(&initialize_result(Some(
            "grok.com"
        ))));
    }

    #[tokio::test]
    async fn direct_authentication_error_skips_the_session_probe() {
        let runner = RecordingRunner::with_results(vec![Err(authentication_required())]);
        let provider = GrokProvider::new(&runner);

        assert_eq!(provider.fetch().await, Err(ProviderError::NotAuthenticated));
        assert_eq!(runner.calls.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn maps_runner_failures_without_exposing_json_rpc_messages() {
        let cases = [
            (ProcessError::NotInstalled, ProviderError::NotInstalled),
            (ProcessError::Timeout, ProviderError::Timeout),
            (
                ProcessError::JsonRpc {
                    code: -32602,
                    message: "Invalid params".to_owned(),
                },
                ProviderError::Process,
            ),
            (ProcessError::NonZero(9), ProviderError::Process),
            (ProcessError::OutputLimit, ProviderError::Process),
            (ProcessError::Io, ProviderError::Network),
        ];

        for (process_error, expected) in cases {
            let runner = RecordingRunner::with_results(vec![Err(process_error)]);
            let provider = GrokProvider::new(&runner);
            assert_eq!(provider.fetch().await, Err(expected));
        }
    }

    #[test]
    fn unknown_extra_fields_are_tolerated() {
        let mut value = billing_value();
        value["experimentalNewField"] = json!({"anything": true});
        value["usage"]["surprise"] = json!(1);

        let usage = parse_billing(value).unwrap();
        assert_eq!(usage.weekly_window.unwrap().remaining_percent, 85);
    }

    #[test]
    fn over_limit_usage_clamps_to_zero_remaining() {
        let usage = parse_billing(json!({
            "monthlyLimit": {"val": 3000},
            "usage": {"totalUsed": {"val": 4500}},
        }))
        .unwrap();
        assert_eq!(usage.weekly_window.unwrap().remaining_percent, 0);
    }

    #[test]
    fn missing_billing_cycle_still_connects_without_a_reset_instant() {
        let usage = parse_billing(json!({
            "monthlyLimit": {"val": 3000},
            "usage": {"totalUsed": {"val": 450}},
        }))
        .unwrap();
        let window = usage.weekly_window.unwrap();
        assert_eq!(window.remaining_percent, 85);
        assert!(window.resets_at.is_none());
    }

    #[test]
    fn epoch_second_and_millisecond_reset_instants_are_supported() {
        let seconds = parse_reset_instant(json!(1_790_812_800)).unwrap();
        let millis = parse_reset_instant(json!(1_790_812_800_000_i64)).unwrap();
        assert_eq!(seconds, millis);
        assert!(parse_reset_instant(json!(true)).is_none());
        assert!(parse_reset_instant(json!("not-a-date")).is_none());
    }

    #[test]
    fn zero_or_missing_monthly_limit_is_unsupported() {
        for value in [
            json!({"monthlyLimit": {"val": 0}, "usage": {"totalUsed": {"val": 1}}}),
            json!({"usage": {"totalUsed": {"val": 1}}}),
            json!({"monthlyLimit": {"val": 3000}}),
            json!([1, 2, 3]),
        ] {
            assert_eq!(parse_billing(value), Err(ProviderError::UnsupportedOutput));
        }
    }
}
