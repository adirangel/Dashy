use std::sync::Arc;

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
    setup.setup.install(request.provider).await?;
    refresh_provider_setup_state(&app, &dashboard, request.provider).await
}

#[tauri::command]
pub async fn login_provider(
    app: AppHandle,
    dashboard: State<'_, AppState>,
    setup: State<'_, SetupState>,
    request: ProviderSetupRequest,
) -> Result<ProviderSetupState, String> {
    setup.setup.login(request.provider).await?;
    refresh_provider_setup_state(&app, &dashboard, request.provider).await
}

async fn refresh_provider_setup_state(
    app: &AppHandle,
    dashboard: &AppState,
    provider: ProviderId,
) -> Result<ProviderSetupState, String> {
    let snapshot = dashboard.dashboard.refresh_provider(provider).await;
    emit_dashboard_cache_changed(app)?;
    Ok(provider_setup_state(provider, &snapshot))
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
    use super::ProviderSetupRequest;

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
}
