use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ProviderId {
    #[serde(rename = "github")]
    GitHub,
    Codex,
    Claude,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ProviderStatus {
    Connected,
    Stale,
    NotInstalled,
    NotAuthenticated,
    Unavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ProviderErrorKind {
    MissingExecutable,
    Authentication,
    Network,
    Timeout,
    UnsupportedOutput,
    Process,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ProviderError {
    #[error("required executable is not installed")]
    NotInstalled,
    #[error("provider authentication is unavailable")]
    NotAuthenticated,
    #[error("provider request timed out")]
    Timeout,
    #[error("provider output is unsupported")]
    UnsupportedOutput,
    #[error("provider process failed")]
    Process,
    #[error("provider network request failed")]
    Network,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContributionDay {
    pub date: NaiveDate,
    pub count: u32,
    pub level: u8,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitHubData {
    pub account_login: String,
    pub contribution_days: Vec<ContributionDay>,
    pub current_streak_days: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum UsageWindowKind {
    Short,
    Weekly,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageWindowData {
    pub label_key: UsageWindowKind,
    pub remaining_percent: u8,
    pub resets_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UsageData {
    pub short_window: Option<UsageWindowData>,
    pub weekly_window: Option<UsageWindowData>,
}

impl UsageData {
    pub fn try_new(
        short_window: Option<UsageWindowData>,
        weekly_window: Option<UsageWindowData>,
    ) -> Result<Self, ProviderError> {
        if short_window.is_none() && weekly_window.is_none() {
            return Err(ProviderError::UnsupportedOutput);
        }

        Ok(Self {
            short_window,
            weekly_window,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitHubSnapshot {
    pub status: ProviderStatus,
    pub account_login: Option<String>,
    pub contribution_days: Option<Vec<ContributionDay>>,
    pub current_streak_days: Option<u32>,
    pub last_successful_refresh: Option<DateTime<Utc>>,
    pub error_kind: Option<ProviderErrorKind>,
}

impl GitHubSnapshot {
    pub fn connected(data: GitHubData, refreshed_at: DateTime<Utc>) -> Self {
        Self {
            status: ProviderStatus::Connected,
            account_login: Some(data.account_login),
            contribution_days: Some(data.contribution_days),
            current_streak_days: Some(data.current_streak_days),
            last_successful_refresh: Some(refreshed_at),
            error_kind: None,
        }
    }

    pub fn failed(status: ProviderStatus, error_kind: ProviderErrorKind) -> Self {
        Self {
            status,
            account_login: None,
            contribution_days: None,
            current_streak_days: None,
            last_successful_refresh: None,
            error_kind: Some(error_kind),
        }
    }

    pub fn stale_from(
        data: GitHubData,
        last_successful_refresh: DateTime<Utc>,
        error_kind: ProviderErrorKind,
    ) -> Self {
        Self {
            status: ProviderStatus::Stale,
            account_login: Some(data.account_login),
            contribution_days: Some(data.contribution_days),
            current_streak_days: Some(data.current_streak_days),
            last_successful_refresh: Some(last_successful_refresh),
            error_kind: Some(error_kind),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageSnapshot {
    pub status: ProviderStatus,
    pub remaining_percent: Option<u8>,
    pub short_window: Option<UsageWindowData>,
    pub weekly_window: Option<UsageWindowData>,
    pub last_successful_refresh: Option<DateTime<Utc>>,
    pub error_kind: Option<ProviderErrorKind>,
}

impl UsageSnapshot {
    pub fn connected(data: UsageData, refreshed_at: DateTime<Utc>) -> Self {
        let (short_window, weekly_window) = clamped_windows(data);
        Self {
            status: ProviderStatus::Connected,
            remaining_percent: summary_remaining_percent(
                short_window.as_ref(),
                weekly_window.as_ref(),
            ),
            short_window,
            weekly_window,
            last_successful_refresh: Some(refreshed_at),
            error_kind: None,
        }
    }

    pub fn failed(status: ProviderStatus, error_kind: ProviderErrorKind) -> Self {
        Self {
            status,
            remaining_percent: None,
            short_window: None,
            weekly_window: None,
            last_successful_refresh: None,
            error_kind: Some(error_kind),
        }
    }

    pub fn stale_from(
        data: UsageData,
        last_successful_refresh: DateTime<Utc>,
        error_kind: ProviderErrorKind,
    ) -> Self {
        let (short_window, weekly_window) = clamped_windows(data);
        Self {
            status: ProviderStatus::Stale,
            remaining_percent: summary_remaining_percent(
                short_window.as_ref(),
                weekly_window.as_ref(),
            ),
            short_window,
            weekly_window,
            last_successful_refresh: Some(last_successful_refresh),
            error_kind: Some(error_kind),
        }
    }
}

fn clamped_windows(data: UsageData) -> (Option<UsageWindowData>, Option<UsageWindowData>) {
    (
        data.short_window.map(clamp_window),
        data.weekly_window.map(clamp_window),
    )
}

fn clamp_window(mut window: UsageWindowData) -> UsageWindowData {
    window.remaining_percent = window.remaining_percent.min(100);
    window
}

fn summary_remaining_percent(
    short_window: Option<&UsageWindowData>,
    weekly_window: Option<&UsageWindowData>,
) -> Option<u8> {
    short_window
        .into_iter()
        .chain(weekly_window)
        .map(|window| window.remaining_percent)
        .min()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DashboardSnapshot {
    pub github: GitHubSnapshot,
    pub codex: UsageSnapshot,
    pub claude: UsageSnapshot,
    pub refreshed_at: DateTime<Utc>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};

    #[test]
    fn serializes_provider_ids_for_the_frontend_contract() {
        assert_eq!(serde_json::to_value(ProviderId::GitHub).unwrap(), "github");
        assert_eq!(serde_json::to_value(ProviderId::Codex).unwrap(), "codex");
        assert_eq!(serde_json::to_value(ProviderId::Claude).unwrap(), "claude");
    }

    #[test]
    fn serializes_remaining_allowance_in_camel_case() {
        let snapshot = UsageSnapshot::connected(
            UsageData {
                short_window: Some(UsageWindowData {
                    label_key: UsageWindowKind::Short,
                    remaining_percent: 59,
                    resets_at: Some(Utc.with_ymd_and_hms(2026, 9, 3, 11, 0, 0).unwrap()),
                }),
                weekly_window: None,
            },
            Utc.with_ymd_and_hms(2026, 8, 29, 9, 0, 0).unwrap(),
        );
        let json = serde_json::to_value(snapshot).unwrap();
        assert_eq!(json["status"], "connected");
        assert_eq!(json["remainingPercent"], 59);
        assert!(json.get("usedPercent").is_none());
    }

    #[test]
    fn usage_snapshot_keeps_both_windows_and_summarizes_the_lower_remaining_value() {
        let snapshot = UsageSnapshot::connected(
            UsageData {
                short_window: Some(UsageWindowData {
                    label_key: UsageWindowKind::Short,
                    remaining_percent: 71,
                    resets_at: Some(Utc.with_ymd_and_hms(2026, 8, 29, 18, 0, 0).unwrap()),
                }),
                weekly_window: Some(UsageWindowData {
                    label_key: UsageWindowKind::Weekly,
                    remaining_percent: 42,
                    resets_at: Some(Utc.with_ymd_and_hms(2026, 9, 3, 0, 0, 0).unwrap()),
                }),
            },
            Utc.with_ymd_and_hms(2026, 8, 29, 9, 0, 0).unwrap(),
        );

        let json = serde_json::to_value(snapshot).unwrap();
        assert_eq!(json["remainingPercent"], 42);
        assert_eq!(json["shortWindow"]["labelKey"], "short");
        assert_eq!(json["weeklyWindow"]["labelKey"], "weekly");
        assert_eq!(json["shortWindow"]["remainingPercent"], 71);
        assert_eq!(json["weeklyWindow"]["remainingPercent"], 42);
        assert!(json.get("usedPercent").is_none());
    }

    #[test]
    fn unavailable_usage_has_no_numeric_value() {
        let snapshot = UsageSnapshot::failed(
            ProviderStatus::Unavailable,
            ProviderErrorKind::UnsupportedOutput,
        );
        assert!(serde_json::to_value(snapshot).unwrap()["remainingPercent"].is_null());
    }

    #[test]
    fn one_window_falls_back_to_that_window_for_the_summary() {
        let snapshot = UsageSnapshot::connected(
            UsageData {
                short_window: None,
                weekly_window: Some(UsageWindowData {
                    label_key: UsageWindowKind::Weekly,
                    remaining_percent: 42,
                    resets_at: None,
                }),
            },
            Utc.with_ymd_and_hms(2026, 8, 29, 9, 0, 0).unwrap(),
        );

        assert_eq!(snapshot.remaining_percent, Some(42));
        assert!(snapshot.short_window.is_none());
        assert_eq!(snapshot.weekly_window.unwrap().remaining_percent, 42);
    }

    #[test]
    fn usage_data_rejects_input_with_no_windows() {
        assert_eq!(
            UsageData::try_new(None, None),
            Err(ProviderError::UnsupportedOutput)
        );
    }

    #[test]
    fn failed_snapshot_has_no_numeric_value_or_windows() {
        let snapshot = UsageSnapshot::failed(
            ProviderStatus::Unavailable,
            ProviderErrorKind::UnsupportedOutput,
        );
        let json = serde_json::to_value(snapshot).unwrap();

        assert!(json["remainingPercent"].is_null());
        assert!(json["shortWindow"].is_null());
        assert!(json["weeklyWindow"].is_null());
    }

    #[test]
    fn stale_snapshot_retains_both_windows_and_summarizes_them() {
        let snapshot = UsageSnapshot::stale_from(
            UsageData {
                short_window: Some(UsageWindowData {
                    label_key: UsageWindowKind::Short,
                    remaining_percent: 88,
                    resets_at: None,
                }),
                weekly_window: Some(UsageWindowData {
                    label_key: UsageWindowKind::Weekly,
                    remaining_percent: 37,
                    resets_at: None,
                }),
            },
            Utc.with_ymd_and_hms(2026, 8, 29, 8, 0, 0).unwrap(),
            ProviderErrorKind::Timeout,
        );

        assert_eq!(snapshot.status, ProviderStatus::Stale);
        assert_eq!(snapshot.remaining_percent, Some(37));
        assert!(snapshot.short_window.is_some());
        assert!(snapshot.weekly_window.is_some());
    }

    #[test]
    fn connected_and_stale_windows_clamp_values_above_hundred() {
        let data = UsageData {
            short_window: Some(UsageWindowData {
                label_key: UsageWindowKind::Short,
                remaining_percent: 101,
                resets_at: None,
            }),
            weekly_window: Some(UsageWindowData {
                label_key: UsageWindowKind::Weekly,
                remaining_percent: 255,
                resets_at: None,
            }),
        };

        let connected = UsageSnapshot::connected(
            data.clone(),
            Utc.with_ymd_and_hms(2026, 8, 29, 9, 0, 0).unwrap(),
        );
        let stale = UsageSnapshot::stale_from(
            data,
            Utc.with_ymd_and_hms(2026, 8, 29, 8, 0, 0).unwrap(),
            ProviderErrorKind::Timeout,
        );

        assert_eq!(connected.remaining_percent, Some(100));
        assert_eq!(connected.short_window.unwrap().remaining_percent, 100);
        assert_eq!(connected.weekly_window.unwrap().remaining_percent, 100);
        assert_eq!(stale.remaining_percent, Some(100));
        assert_eq!(stale.short_window.unwrap().remaining_percent, 100);
        assert_eq!(stale.weekly_window.unwrap().remaining_percent, 100);
    }

    fn serialized_remaining_percent(value: u8) -> u8 {
        let snapshot = UsageSnapshot::connected(
            UsageData {
                short_window: Some(UsageWindowData {
                    label_key: UsageWindowKind::Short,
                    remaining_percent: value,
                    resets_at: None,
                }),
                weekly_window: None,
            },
            Utc.with_ymd_and_hms(2026, 8, 29, 9, 0, 0).unwrap(),
        );
        serde_json::to_value(snapshot).unwrap()["remainingPercent"]
            .as_u64()
            .unwrap() as u8
    }

    #[test]
    fn remaining_allowance_preserves_zero() {
        assert_eq!(serialized_remaining_percent(0), 0);
    }

    #[test]
    fn remaining_allowance_preserves_hundred() {
        assert_eq!(serialized_remaining_percent(100), 100);
    }

    #[test]
    fn remaining_allowance_clamps_values_above_hundred() {
        assert_eq!(serialized_remaining_percent(101), 100);
    }
}
