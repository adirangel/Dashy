use std::{future::Future, sync::Arc};

use serde::Deserialize;
use tauri::{AppHandle, State};

use crate::dashboard::{
    commands::{emit_dashboard_cache_changed, AppState},
    models::{DashboardSnapshot, ProviderId},
};
use crate::setup::{
    models::{ProviderSetupDefinition, ProviderSetupState},
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
    let snapshot = dashboard.dashboard.get_snapshot(true).await;
    Ok(ProviderId::ALL
        .into_iter()
        .map(|provider| provider_setup_state(provider, &snapshot))
        .collect())
}

#[tauri::command]
pub async fn install_provider(
    app: AppHandle,
    dashboard: State<'_, AppState>,
    setup: State<'_, SetupState>,
    request: ProviderSetupRequest,
) -> Result<ProviderSetupState, String> {
    let provider = request.provider;
    let dashboard = dashboard.dashboard.clone();
    run_provider_setup_action(
        provider,
        setup.setup.install(provider),
        move |provider| async move {
            let snapshot = dashboard.refresh_provider(provider).await;
            provider_setup_state(provider, &snapshot)
        },
        || emit_dashboard_cache_changed(&app),
    )
    .await
}

#[tauri::command]
pub async fn login_provider(
    app: AppHandle,
    dashboard: State<'_, AppState>,
    setup: State<'_, SetupState>,
    request: ProviderSetupRequest,
) -> Result<ProviderSetupState, String> {
    let provider = request.provider;
    let dashboard = dashboard.dashboard.clone();
    run_provider_setup_action(
        provider,
        setup.setup.login(provider),
        move |provider| async move {
            let snapshot = dashboard.refresh_provider(provider).await;
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
    let status = match provider {
        ProviderId::Claude => snapshot.claude.status,
        ProviderId::Codex => snapshot.codex.status,
        ProviderId::GitHub => snapshot.github.status,
    };
    ProviderSetupState {
        definition: ProviderSetupDefinition::for_provider(provider),
        status,
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use async_trait::async_trait;

    use super::{run_provider_setup_action, ProviderSetupRequest};
    use crate::dashboard::{
        models::{ProviderId, ProviderStatus},
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
