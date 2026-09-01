use std::{future::Future, sync::Arc};

use serde::Deserialize;
use tauri::{AppHandle, State, WebviewWindow};

use crate::dashboard::{
    commands::{emit_dashboard_cache_changed, AppState},
    models::{DashboardSnapshot, ProviderId},
};
use crate::setup::{
    models::{ProviderRepairAction, ProviderSetupDefinition, ProviderSetupState},
    service::SetupService,
};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProviderSetupRequest {
    provider: ProviderId,
}

pub struct SetupState {
    setup: Arc<SetupService>,
}

impl SetupState {
    pub fn new(setup: Arc<SetupService>) -> Self {
        Self { setup }
    }
}

#[tauri::command]
pub async fn get_provider_setup_states(
    dashboard: State<'_, AppState>,
) -> Result<Vec<ProviderSetupState>, String> {
    // Read through the cache instead of forcing a refresh: opening the settings or
    // onboarding surface must not spawn a CLI process storm. Providers past the cache
    // TTL (including the empty first-run cache) still refresh for real.
    let mut snapshot = dashboard.dashboard.get_snapshot(false).await;
    // A provider whose last probe failed may have been repaired outside the app
    // (e.g. `claude login` in a terminal), so re-probe only the failed ones; healthy
    // providers keep their cached state.
    let failed = providers_needing_reprobe(&snapshot);
    if !failed.is_empty() {
        snapshot = dashboard.dashboard.get_snapshot_for(true, &failed).await;
    }
    Ok(ProviderId::ALL
        .into_iter()
        .map(|provider| provider_setup_state(provider, &snapshot))
        .collect())
}

fn providers_needing_reprobe(snapshot: &DashboardSnapshot) -> Vec<ProviderId> {
    ProviderId::ALL
        .into_iter()
        .filter(|provider| {
            matches!(
                provider_snapshot_status(*provider, snapshot),
                crate::dashboard::models::ProviderStatus::NotInstalled
                    | crate::dashboard::models::ProviderStatus::NotAuthenticated
                    | crate::dashboard::models::ProviderStatus::Unavailable
            )
        })
        .collect()
}

fn provider_snapshot_status(
    provider: ProviderId,
    snapshot: &DashboardSnapshot,
) -> crate::dashboard::models::ProviderStatus {
    snapshot.provider_status_and_error(provider).0
}

#[tauri::command]
pub async fn install_provider(
    window: WebviewWindow,
    app: AppHandle,
    dashboard: State<'_, AppState>,
    setup: State<'_, SetupState>,
    request: ProviderSetupRequest,
) -> Result<ProviderSetupState, String> {
    crate::authorize_caller(&window, &["onboarding", "settings"])?;
    let provider = request.provider;
    let dashboard = dashboard.dashboard.clone();
    run_provider_setup_action(
        provider,
        setup.setup.install(provider),
        move |provider| async move {
            let snapshot = dashboard.refresh_provider_after_mutation(provider).await;
            provider_setup_state(provider, &snapshot)
        },
        || emit_dashboard_cache_changed(&app),
    )
    .await
}

#[tauri::command]
pub async fn login_provider(
    window: WebviewWindow,
    app: AppHandle,
    dashboard: State<'_, AppState>,
    setup: State<'_, SetupState>,
    request: ProviderSetupRequest,
) -> Result<ProviderSetupState, String> {
    crate::authorize_caller(&window, &["onboarding", "settings"])?;
    let provider = request.provider;
    let dashboard = dashboard.dashboard.clone();
    run_provider_setup_action(
        provider,
        setup.setup.login(provider),
        move |provider| async move {
            let snapshot = dashboard.refresh_provider_after_mutation(provider).await;
            provider_setup_state(provider, &snapshot)
        },
        || emit_dashboard_cache_changed(&app),
    )
    .await
}

async fn run_provider_setup_action<S, R, RF, N>(
    provider: ProviderId,
    setup_action: S,
    reconcile: R,
    notify: N,
) -> Result<ProviderSetupState, String>
where
    S: Future<Output = Result<(), String>>,
    R: FnOnce(ProviderId) -> RF,
    RF: Future<Output = ProviderSetupState>,
    N: FnOnce() -> Result<(), String>,
{
    let setup_result = setup_action.await;
    let state = reconcile(provider).await;
    let notification_result = notify();
    setup_result?;
    notification_result?;
    Ok(state)
}

fn provider_setup_state(provider: ProviderId, snapshot: &DashboardSnapshot) -> ProviderSetupState {
    let (status, error_kind) = snapshot.provider_status_and_error(provider);
    ProviderSetupState {
        definition: ProviderSetupDefinition::for_provider(provider),
        status,
        repair_action: match error_kind {
            Some(crate::dashboard::models::ProviderErrorKind::MissingExecutable) => {
                Some(ProviderRepairAction::Install)
            }
            Some(crate::dashboard::models::ProviderErrorKind::Authentication) => {
                Some(ProviderRepairAction::Login)
            }
            _ => match status {
                crate::dashboard::models::ProviderStatus::NotInstalled => {
                    Some(ProviderRepairAction::Install)
                }
                crate::dashboard::models::ProviderStatus::NotAuthenticated => {
                    Some(ProviderRepairAction::Login)
                }
                _ => None,
            },
        },
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use async_trait::async_trait;

    use super::{provider_setup_state, run_provider_setup_action, ProviderSetupRequest};
    use crate::dashboard::{
        models::{
            AccountSnapshot, DashboardSnapshot, GitHubSnapshot, ProviderErrorKind, ProviderId,
            ProviderStatus, UsageSnapshot,
        },
        process::{AllowedProgram, VisibleProcessError, VisibleRunner},
    };
    use crate::setup::{
        models::{ProviderSetupDefinition, ProviderSetupState},
        service::SetupService,
    };

    struct FailingRunner {
        trace: Arc<Mutex<Vec<&'static str>>>,
    }

    #[async_trait]
    impl VisibleRunner for FailingRunner {
        async fn run_visible(
            &self,
            _program: AllowedProgram,
            _args: Vec<String>,
        ) -> Result<(), VisibleProcessError> {
            self.trace.lock().unwrap().push("setup");
            Err(VisibleProcessError::Failed)
        }
    }

    #[test]
    fn setup_request_rejects_command_injection_fields() {
        for value in [
            serde_json::json!({"provider":"codex","executable":"powershell"}),
            serde_json::json!({"provider":"github","packageId":"Other.Package"}),
            serde_json::json!({"provider":"unknown"}),
        ] {
            assert!(serde_json::from_value::<ProviderSetupRequest>(value).is_err());
        }
    }

    fn stale_snapshot(provider: ProviderId, cause: ProviderErrorKind) -> DashboardSnapshot {
        let now = chrono::Utc::now();
        let mut snapshot = DashboardSnapshot {
            github: GitHubSnapshot::failed(ProviderStatus::Unavailable, ProviderErrorKind::Process),
            codex: UsageSnapshot::failed(ProviderStatus::Unavailable, ProviderErrorKind::Process),
            claude: UsageSnapshot::failed(ProviderStatus::Unavailable, ProviderErrorKind::Process),
            grok: UsageSnapshot::failed(ProviderStatus::Unavailable, ProviderErrorKind::Process),
            cursor: AccountSnapshot::failed(
                ProviderStatus::Unavailable,
                ProviderErrorKind::Process,
            ),
            refreshed_at: now,
        };
        match provider {
            ProviderId::GitHub => {
                snapshot.github = GitHubSnapshot::failed(ProviderStatus::Stale, cause)
            }
            ProviderId::Codex => {
                snapshot.codex = UsageSnapshot::failed(ProviderStatus::Stale, cause)
            }
            ProviderId::Claude => {
                snapshot.claude = UsageSnapshot::failed(ProviderStatus::Stale, cause)
            }
            ProviderId::Grok => snapshot.grok = UsageSnapshot::failed(ProviderStatus::Stale, cause),
            ProviderId::Cursor => {
                snapshot.cursor = AccountSnapshot::failed(ProviderStatus::Stale, cause)
            }
        }
        snapshot
    }

    #[test]
    fn only_failed_providers_are_selected_for_a_setup_reprobe() {
        let snapshot = DashboardSnapshot {
            github: GitHubSnapshot::failed(
                ProviderStatus::NotInstalled,
                ProviderErrorKind::MissingExecutable,
            ),
            codex: UsageSnapshot::failed(ProviderStatus::Stale, ProviderErrorKind::Timeout),
            claude: UsageSnapshot::failed(
                ProviderStatus::NotAuthenticated,
                ProviderErrorKind::Authentication,
            ),
            grok: UsageSnapshot::failed(ProviderStatus::Stale, ProviderErrorKind::Timeout),
            cursor: AccountSnapshot::failed(
                ProviderStatus::NotInstalled,
                ProviderErrorKind::MissingExecutable,
            ),
            refreshed_at: chrono::Utc::now(),
        };

        assert_eq!(
            super::providers_needing_reprobe(&snapshot),
            vec![ProviderId::Claude, ProviderId::GitHub, ProviderId::Cursor],
            "stale keeps its cached last-good state; failed probes re-run"
        );
    }

    #[test]
    fn stale_setup_states_preserve_the_actionable_repair_for_every_provider() {
        for provider in ProviderId::ALL {
            let install = serde_json::to_value(provider_setup_state(
                provider,
                &stale_snapshot(provider, ProviderErrorKind::MissingExecutable),
            ))
            .unwrap();
            assert_eq!(install["repairAction"], "install", "{provider:?}");

            let login = serde_json::to_value(provider_setup_state(
                provider,
                &stale_snapshot(provider, ProviderErrorKind::Authentication),
            ))
            .unwrap();
            assert_eq!(login["repairAction"], "login", "{provider:?}");
        }
    }

    #[tokio::test]
    async fn failed_setup_still_reconciles_then_notifies_and_preserves_sanitized_error() {
        let trace = Arc::new(Mutex::new(Vec::new()));
        let setup = SetupService::new(Arc::new(FailingRunner {
            trace: trace.clone(),
        }));
        let refresh_trace = trace.clone();
        let event_trace = trace.clone();

        let result = run_provider_setup_action(
            ProviderId::Codex,
            setup.install(ProviderId::Codex),
            move |provider| {
                refresh_trace.lock().unwrap().push("refresh");
                std::future::ready(ProviderSetupState {
                    definition: ProviderSetupDefinition::for_provider(provider),
                    status: ProviderStatus::NotInstalled,
                    repair_action: Some(crate::setup::models::ProviderRepairAction::Install),
                })
            },
            move || {
                event_trace.lock().unwrap().push("event");
                Err("cache event failed".to_owned())
            },
        )
        .await;

        assert_eq!(
            result,
            Err("provider setup process did not complete".to_owned())
        );
        assert_eq!(*trace.lock().unwrap(), vec!["setup", "refresh", "event"]);
    }
}
