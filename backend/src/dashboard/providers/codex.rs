use std::time::Duration;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::Deserialize;

use crate::dashboard::{
    models::{ProviderError, UsageData, UsageWindowData, UsageWindowKind},
    process::{AllowedProgram, JsonRpcRunner, ProcessError},
    providers::DataProvider,
};

const CODEX_TIMEOUT: Duration = Duration::from_secs(30);
const AUTHENTICATION_ERROR_CODE: i64 = -32001;
const AUTHENTICATION_ERROR_MESSAGE: &str = "Authentication required";
const DASHY_VERSION: &str = env!("CARGO_PKG_VERSION");

pub struct CodexProvider<R: JsonRpcRunner> {
    runner: R,
}

impl<R: JsonRpcRunner> CodexProvider<R> {
    pub fn new(runner: R) -> Self {
        Self { runner }
    }
}

#[async_trait]
impl<R: JsonRpcRunner> DataProvider<UsageData> for CodexProvider<R> {
    async fn fetch(&self) -> Result<UsageData, ProviderError> {
        let value = self
            .runner
            .request(
                AllowedProgram::Codex,
                vec!["app-server".to_owned(), "--stdio".to_owned()],
                vec![
                    serde_json::json!({
                        "jsonrpc": "2.0",
                        "id": 1,
                        "method": "initialize",
                        "params": {
                            "clientInfo": {"name": "dashy", "version": DASHY_VERSION},
                            "capabilities": {"experimentalApi": true}
                        }
                    }),
                    serde_json::json!({
                        "jsonrpc": "2.0",
                        "id": 2,
                        "method": "account/rateLimits/read"
                    }),
                ],
                2,
                CODEX_TIMEOUT,
            )
            .await
            .map_err(map_process_error)?;

        parse_value(value)
    }
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

fn parse_value(value: serde_json::Value) -> Result<UsageData, ProviderError> {
    let response: RateLimitsResponse =
        serde_json::from_value(value).map_err(|_| ProviderError::UnsupportedOutput)?;
    let bucket = response
        .rate_limits_by_limit_id
        .as_ref()
        .and_then(|buckets| buckets.get("codex"))
        .or(response.rate_limits.as_ref())
        .ok_or(ProviderError::UnsupportedOutput)?;
    let bucket: RateLimitBucket =
        serde_json::from_value(bucket.clone()).map_err(|_| ProviderError::UnsupportedOutput)?;

    if bucket.limit_id != "codex" {
        return Err(ProviderError::UnsupportedOutput);
    }

    let mut short_window = None;
    let mut weekly_window = None;
    for window in [&bucket.primary, &bucket.secondary].into_iter().flatten() {
        let window = parse_window(window)?;
        let usage_window = UsageWindowData {
            label_key: classify_window_kind(window.window_duration_mins)?,
            remaining_percent: window.remaining_percent,
            resets_at: Some(window.resets_at),
        };
        let slot = match usage_window.label_key {
            UsageWindowKind::Short => &mut short_window,
            UsageWindowKind::Weekly => &mut weekly_window,
            // classify_window_kind never produces Monthly for codex windows.
            UsageWindowKind::Monthly => return Err(ProviderError::UnsupportedOutput),
        };
        if slot.replace(usage_window).is_some() {
            return Err(ProviderError::UnsupportedOutput);
        }
    }

    UsageData::try_new(short_window, weekly_window)
}

fn classify_window_kind(window_duration_mins: u32) -> Result<UsageWindowKind, ProviderError> {
    match window_duration_mins {
        1..10080 => Ok(UsageWindowKind::Short),
        10080 => Ok(UsageWindowKind::Weekly),
        _ => Err(ProviderError::UnsupportedOutput),
    }
}

fn parse_window(window: &RateLimitWindow) -> Result<ParsedWindow, ProviderError> {
    if window.used_percent > 100 || window.window_duration_mins == 0 {
        return Err(ProviderError::UnsupportedOutput);
    }

    let resets_at =
        DateTime::from_timestamp(window.resets_at, 0).ok_or(ProviderError::UnsupportedOutput)?;
    Ok(ParsedWindow {
        remaining_percent: 100 - window.used_percent,
        resets_at,
        window_duration_mins: window.window_duration_mins,
    })
}

struct ParsedWindow {
    remaining_percent: u8,
    resets_at: DateTime<Utc>,
    window_duration_mins: u32,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RateLimitsResponse {
    #[serde(rename = "rateLimits")]
    rate_limits: Option<serde_json::Value>,
    #[serde(rename = "rateLimitsByLimitId")]
    rate_limits_by_limit_id: Option<std::collections::BTreeMap<String, serde_json::Value>>,
    #[serde(rename = "rateLimitResetCredits")]
    _rate_limit_reset_credits: Option<RateLimitResetCredits>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RateLimitBucket {
    #[serde(rename = "credits")]
    _credits: Option<Credits>,
    #[serde(rename = "individualLimit")]
    _individual_limit: Option<()>,
    #[serde(rename = "limitId")]
    limit_id: String,
    #[serde(rename = "limitName")]
    _limit_name: Option<String>,
    #[serde(rename = "planType")]
    _plan_type: Option<String>,
    primary: Option<RateLimitWindow>,
    #[serde(rename = "rateLimitReachedType")]
    _rate_limit_reached_type: Option<String>,
    secondary: Option<RateLimitWindow>,
    #[serde(rename = "spendControlReached")]
    _spend_control_reached: Option<bool>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Credits {
    #[serde(rename = "balance")]
    _balance: String,
    #[serde(rename = "hasCredits")]
    _has_credits: bool,
    #[serde(rename = "unlimited")]
    _unlimited: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RateLimitResetCredits {
    #[serde(rename = "availableCount")]
    _available_count: u64,
    #[serde(rename = "credits")]
    _credits: Vec<ResetCredit>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ResetCredit {
    #[serde(rename = "description")]
    _description: String,
    #[serde(rename = "expiresAt")]
    _expires_at: i64,
    #[serde(rename = "grantedAt")]
    _granted_at: i64,
    #[serde(rename = "id")]
    _id: String,
    #[serde(rename = "resetType")]
    _reset_type: String,
    #[serde(rename = "status")]
    _status: String,
    #[serde(rename = "title")]
    _title: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RateLimitWindow {
    #[serde(rename = "usedPercent")]
    used_percent: u8,
    #[serde(rename = "windowDurationMins")]
    window_duration_mins: u32,
    #[serde(rename = "resetsAt")]
    resets_at: i64,
}

#[cfg(test)]
mod tests {
    use std::{collections::VecDeque, sync::Mutex, time::Duration};

    use async_trait::async_trait;
    use serde::Deserialize;
    use serde_json::json;

    use super::*;
    use crate::dashboard::{
        models::ProviderError,
        process::{AllowedProgram, JsonRpcRunner, ProcessError},
        providers::DataProvider,
    };

    fn fixture_with_general_windows(primary_used: u8, secondary_used: u8) -> String {
        serde_json::to_string(&json!({
            "rateLimitsByLimitId": {
                "codex": {
                    "limitId": "codex",
                    "limitName": null,
                    "primary": {
                        "usedPercent": primary_used,
                        "windowDurationMins": 60,
                        "resetsAt": 1788000000
                    },
                    "secondary": {
                        "usedPercent": secondary_used,
                        "windowDurationMins": 10080,
                        "resetsAt": 1788532560
                    }
                }
            }
        }))
        .unwrap()
    }

    fn parse_rate_limits(response: &str) -> Result<UsageData, ProviderError> {
        let value = serde_json::from_str(response).map_err(|_| ProviderError::UnsupportedOutput)?;
        parse_value(value)
    }

    fn parse_raw_rate_limits(response: &str) -> Result<UsageData, ProviderError> {
        let _: RawRateLimitsResponse =
            serde_json::from_str(response).map_err(|_| ProviderError::UnsupportedOutput)?;
        parse_rate_limits(response)
    }

    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct RawRateLimitsResponse {
        #[serde(rename = "rateLimits")]
        _rate_limits: Option<RateLimitBucket>,
        #[serde(rename = "rateLimitsByLimitId")]
        _rate_limits_by_limit_id: Option<std::collections::BTreeMap<String, serde_json::Value>>,
        #[serde(rename = "rateLimitResetCredits")]
        _rate_limit_reset_credits: Option<RateLimitResetCredits>,
    }

    #[test]
    fn preserves_the_short_and_weekly_general_windows() {
        let usage = parse_rate_limits(&fixture_with_general_windows(28, 61)).unwrap();
        assert_eq!(usage.short_window.unwrap().remaining_percent, 72);
        assert_eq!(usage.weekly_window.unwrap().remaining_percent, 39);
    }

    #[test]
    fn preserves_each_general_window_reset() {
        let usage = parse_rate_limits(&fixture_with_general_windows(20, 45)).unwrap();
        assert_eq!(
            usage.short_window.unwrap().resets_at.unwrap().timestamp(),
            1788000000
        );
        assert_eq!(
            usage.weekly_window.unwrap().resets_at.unwrap().timestamp(),
            1788532560
        );
    }

    #[test]
    fn falls_back_to_the_top_level_general_bucket() {
        let value = json!({
            "rateLimits": {
                "limitId": "codex",
                "primary": {
                    "usedPercent": 31,
                    "windowDurationMins": 60,
                    "resetsAt": 1788000000
                },
                "secondary": {
                    "usedPercent": 62,
                    "windowDurationMins": 10080,
                    "resetsAt": 1788532560
                }
            },
            "rateLimitsByLimitId": {}
        });

        let usage = parse_value(value).unwrap();
        assert_eq!(usage.short_window.unwrap().remaining_percent, 69);
        assert_eq!(usage.weekly_window.unwrap().remaining_percent, 38);
    }

    #[test]
    fn prefers_the_shared_limit_id_bucket_without_combining_the_top_level_copy() {
        let value = json!({
            "rateLimits": {
                "limitId": "codex",
                "primary": {
                    "usedPercent": 50,
                    "windowDurationMins": 60,
                    "resetsAt": 1788000000
                },
                "secondary": {
                    "usedPercent": 50,
                    "windowDurationMins": 10080,
                    "resetsAt": 1788532560
                }
            },
            "rateLimitsByLimitId": {
                "codex": {
                    "limitId": "codex",
                    "primary": {
                        "usedPercent": 10,
                        "windowDurationMins": 60,
                        "resetsAt": 1788000000
                    },
                    "secondary": {
                        "usedPercent": 20,
                        "windowDurationMins": 10080,
                        "resetsAt": 1788532560
                    }
                }
            }
        });

        let usage = parse_value(value).unwrap();
        assert_eq!(usage.short_window.unwrap().remaining_percent, 90);
        assert_eq!(usage.weekly_window.unwrap().remaining_percent, 80);
    }

    #[test]
    fn ignores_malformed_preview_limits_when_the_general_bucket_is_valid() {
        let value = json!({
            "rateLimitsByLimitId": {
                "codex": {
                    "limitId": "codex",
                    "primary": {
                        "usedPercent": 10,
                        "windowDurationMins": 60,
                        "resetsAt": 1788000000
                    },
                    "secondary": {
                        "usedPercent": 25,
                        "windowDurationMins": 10080,
                        "resetsAt": 1788532560
                    }
                },
                "codex_preview": {"unexpected": true}
            }
        });

        let usage = parse_value(value).unwrap();
        assert_eq!(usage.short_window.unwrap().remaining_percent, 90);
        assert_eq!(usage.weekly_window.unwrap().remaining_percent, 75);
    }

    #[test]
    fn falls_back_when_unrelated_limit_entries_are_malformed() {
        let value = json!({
            "rateLimits": {
                "limitId": "codex",
                "primary": {
                    "usedPercent": 31,
                    "windowDurationMins": 60,
                    "resetsAt": 1788000000
                },
                "secondary": {
                    "usedPercent": 62,
                    "windowDurationMins": 10080,
                    "resetsAt": 1788532560
                }
            },
            "rateLimitsByLimitId": {
                "codex_preview": {"futureExtension": ["anything"]}
            }
        });

        let usage = parse_value(value).unwrap();
        assert_eq!(usage.short_window.unwrap().remaining_percent, 69);
        assert_eq!(usage.weekly_window.unwrap().remaining_percent, 38);
    }

    #[test]
    fn rejects_missing_general_windows() {
        assert_eq!(
            parse_value(json!({})),
            Err(ProviderError::UnsupportedOutput)
        );
    }

    #[test]
    fn accepts_a_weekly_only_primary_window() {
        let usage = parse_value(json!({
            "rateLimits": {
                "limitId": "codex",
                "primary": {
                    "usedPercent": 20,
                    "windowDurationMins": 10080,
                    "resetsAt": 1788532560
                },
                "secondary": null
            }
        }))
        .unwrap();

        assert!(usage.short_window.is_none());
        assert_eq!(usage.weekly_window.unwrap().remaining_percent, 80);
    }

    #[test]
    fn accepts_a_weekly_only_secondary_window_when_primary_is_absent() {
        let usage = parse_value(json!({
            "rateLimits": {
                "limitId": "codex",
                "secondary": {
                    "usedPercent": 20,
                    "windowDurationMins": 10080,
                    "resetsAt": 1788532560
                }
            }
        }))
        .unwrap();

        assert!(usage.short_window.is_none());
        assert_eq!(usage.weekly_window.unwrap().remaining_percent, 80);
    }

    #[test]
    fn accepts_a_short_only_primary_window() {
        let usage = parse_value(json!({
            "rateLimits": {
                "limitId": "codex",
                "primary": {
                    "usedPercent": 10,
                    "windowDurationMins": 60,
                    "resetsAt": 1788000000
                },
                "secondary": null
            }
        }))
        .unwrap();

        assert_eq!(usage.short_window.unwrap().remaining_percent, 90);
        assert!(usage.weekly_window.is_none());
    }

    #[test]
    fn rejects_duplicate_or_absent_general_window_kinds() {
        let duplicate_short = json!({
            "rateLimits": {
                "limitId": "codex",
                "primary": {
                    "usedPercent": 10,
                    "windowDurationMins": 60,
                    "resetsAt": 1788000000
                },
                "secondary": {
                    "usedPercent": 20,
                    "windowDurationMins": 300,
                    "resetsAt": 1788532560
                }
            }
        });
        let no_windows = json!({
            "rateLimits": {
                "limitId": "codex",
                "primary": null,
                "secondary": null
            }
        });

        assert_eq!(
            parse_value(duplicate_short),
            Err(ProviderError::UnsupportedOutput)
        );
        assert_eq!(
            parse_value(no_windows),
            Err(ProviderError::UnsupportedOutput)
        );
    }

    #[test]
    fn rejects_duplicate_required_fields_in_a_raw_selected_shared_bucket() {
        let duplicate_primary = r#"{
            "rateLimits": {
                "limitId": "codex",
                "primary": {"usedPercent": 10, "windowDurationMins": 60, "resetsAt": 1788000000},
                "primary": {"usedPercent": 20, "windowDurationMins": 60, "resetsAt": 1788003600},
                "secondary": {"usedPercent": 20, "windowDurationMins": 10080, "resetsAt": 1788532560}
            }
        }"#;
        let duplicate_secondary = r#"{
            "rateLimits": {
                "limitId": "codex",
                "primary": {"usedPercent": 10, "windowDurationMins": 60, "resetsAt": 1788000000},
                "secondary": {"usedPercent": 20, "windowDurationMins": 10080, "resetsAt": 1788532560},
                "secondary": {"usedPercent": 30, "windowDurationMins": 10080, "resetsAt": 1788536160}
            }
        }"#;

        assert_eq!(
            parse_raw_rate_limits(duplicate_primary),
            Err(ProviderError::UnsupportedOutput)
        );
        assert_eq!(
            parse_raw_rate_limits(duplicate_secondary),
            Err(ProviderError::UnsupportedOutput)
        );
    }

    #[test]
    fn rejects_unknown_fields_and_out_of_range_percentages() {
        let unknown_field = json!({
            "rateLimits": {
                "limitId": "codex",
                "primary": {
                    "usedPercent": 10,
                    "windowDurationMins": 60,
                    "resetsAt": 1788000000,
                    "unexpected": true
                },
                "secondary": {
                    "usedPercent": 20,
                    "windowDurationMins": 10080,
                    "resetsAt": 1788532560
                }
            },
            "rateLimitsByLimitId": {}
        });
        let invalid_percent = json!({
            "rateLimits": {
                "limitId": "codex",
                "primary": {
                    "usedPercent": 101,
                    "windowDurationMins": 60,
                    "resetsAt": 1788000000
                },
                "secondary": {
                    "usedPercent": 20,
                    "windowDurationMins": 10080,
                    "resetsAt": 1788532560
                }
            },
            "rateLimitsByLimitId": {}
        });

        assert_eq!(
            parse_value(unknown_field),
            Err(ProviderError::UnsupportedOutput)
        );
        assert_eq!(
            parse_value(invalid_percent),
            Err(ProviderError::UnsupportedOutput)
        );
    }

    #[test]
    fn rejects_an_invalid_reset_timestamp() {
        let value = json!({
            "rateLimits": {
                "limitId": "codex",
                "primary": {
                    "usedPercent": 10,
                    "windowDurationMins": 60,
                    "resetsAt": 9223372036854775807i64
                },
                "secondary": {
                    "usedPercent": 20,
                    "windowDurationMins": 10080,
                    "resetsAt": 1788532560
                }
            },
            "rateLimitsByLimitId": {}
        });

        assert_eq!(parse_value(value), Err(ProviderError::UnsupportedOutput));
    }

    #[test]
    fn accepts_known_app_server_fields_without_reading_account_details() {
        let value = json!({
            "rateLimitResetCredits": {
                "availableCount": 0,
                "credits": []
            },
            "rateLimits": {
                "credits": {
                    "balance": "0",
                    "hasCredits": false,
                    "unlimited": false
                },
                "individualLimit": null,
                "limitId": "codex",
                "limitName": null,
                "planType": "plus",
                "primary": {
                    "usedPercent": 10,
                    "windowDurationMins": 60,
                    "resetsAt": 1788532560
                },
                "rateLimitReachedType": null,
                "secondary": {
                    "usedPercent": 20,
                    "windowDurationMins": 10080,
                    "resetsAt": 1788532560
                },
                "spendControlReached": false
            },
            "rateLimitsByLimitId": {}
        });

        let usage = parse_value(value).unwrap();
        assert_eq!(usage.short_window.unwrap().remaining_percent, 90);
        assert_eq!(usage.weekly_window.unwrap().remaining_percent, 80);
    }

    type JsonRpcCall = (
        AllowedProgram,
        Vec<String>,
        Vec<serde_json::Value>,
        u64,
        Duration,
    );

    #[derive(Clone)]
    struct RecordingRunner {
        calls: std::sync::Arc<Mutex<Vec<JsonRpcCall>>>,
        results: std::sync::Arc<Mutex<VecDeque<Result<serde_json::Value, ProcessError>>>>,
    }

    impl RecordingRunner {
        fn with_results(results: Vec<Result<serde_json::Value, ProcessError>>) -> Self {
            Self {
                calls: std::sync::Arc::new(Mutex::new(Vec::new())),
                results: std::sync::Arc::new(Mutex::new(results.into())),
            }
        }
    }

    #[async_trait]
    impl JsonRpcRunner for RecordingRunner {
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

    #[tokio::test]
    async fn sends_the_exact_app_server_handshake_and_rate_limit_request() {
        let runner = RecordingRunner::with_results(vec![Ok(serde_json::from_str(
            &fixture_with_general_windows(10, 20),
        )
        .unwrap())]);
        let provider = CodexProvider::new(runner.clone());

        provider.fetch().await.unwrap();

        let calls = runner.calls.lock().unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].0, AllowedProgram::Codex);
        assert_eq!(calls[0].1, ["app-server", "--stdio"]);
        assert_eq!(
            calls[0].2,
            vec![
                json!({
                    "jsonrpc": "2.0",
                    "id": 1,
                    "method": "initialize",
                    "params": {
                        "clientInfo": {"name": "dashy", "version": DASHY_VERSION},
                        "capabilities": {"experimentalApi": true}
                    }
                }),
                json!({
                    "jsonrpc": "2.0",
                    "id": 2,
                    "method": "account/rateLimits/read"
                })
            ]
        );
        assert_eq!(calls[0].3, 2);
        assert_eq!(calls[0].4, Duration::from_secs(30));
    }

    #[tokio::test]
    async fn maps_runner_failures_without_exposing_json_rpc_messages() {
        let cases = [
            (ProcessError::NotInstalled, ProviderError::NotInstalled),
            (ProcessError::Timeout, ProviderError::Timeout),
            (
                ProcessError::JsonRpc {
                    code: -32001,
                    message: "Authentication required".to_owned(),
                },
                ProviderError::NotAuthenticated,
            ),
            (
                ProcessError::JsonRpc {
                    code: -32001,
                    message: "Unexpected server message".to_owned(),
                },
                ProviderError::Process,
            ),
            (
                ProcessError::JsonRpc {
                    code: -32601,
                    message: "Authentication required".to_owned(),
                },
                ProviderError::Process,
            ),
            (ProcessError::NonZero(1), ProviderError::Process),
            (ProcessError::OutputLimit, ProviderError::Process),
            (ProcessError::Io, ProviderError::Network),
        ];

        for (error, expected) in cases {
            let runner = RecordingRunner::with_results(vec![Err(error)]);
            let provider = CodexProvider::new(runner);
            assert_eq!(provider.fetch().await, Err(expected));
        }
    }

    #[tokio::test]
    async fn maps_schema_drift_to_unsupported_output() {
        let runner = RecordingRunner::with_results(vec![Ok(json!({"rateLimits": {}}))]);
        let provider = CodexProvider::new(runner);

        assert_eq!(
            provider.fetch().await,
            Err(ProviderError::UnsupportedOutput)
        );
    }

    #[tokio::test]
    #[ignore]
    async fn live_codex_rate_limits() {
        let provider = CodexProvider::new(crate::dashboard::process::SystemProcessRunner);
        let data = provider.fetch().await.unwrap();

        let windows = [data.short_window, data.weekly_window];
        assert!(windows.iter().any(Option::is_some));
        for window in windows.into_iter().flatten() {
            assert!(window.remaining_percent <= 100);
            assert!(window
                .resets_at
                .is_some_and(|reset| reset > chrono::Utc::now()));
        }
    }
}
