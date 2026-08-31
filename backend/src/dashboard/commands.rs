use std::sync::{
    atomic::{AtomicU32, Ordering},
    Arc,
};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use crate::dashboard::{
    models::{DashboardSnapshot, ProviderId},
    service::DashboardService,
};
use crate::desktop::settings::AppSettings;

pub(crate) fn refreshable_providers(settings: &AppSettings) -> &[ProviderId] {
    if settings.requires_provider_setup() {
        &[]
    } else {
        &settings.enabled_providers
    }
}

pub struct AppState {
    pub dashboard: Arc<DashboardService>,
    cache_change_revision: DashboardCacheRevision,
}

impl AppState {
    pub fn new(dashboard: Arc<DashboardService>) -> Self {
        Self {
            dashboard,
            cache_change_revision: DashboardCacheRevision::default(),
        }
    }
}

#[derive(Default)]
pub struct DashboardCacheRevision(AtomicU32);

impl DashboardCacheRevision {
    #[cfg(test)]
    fn starting_at(revision: u32) -> Self {
        Self(AtomicU32::new(revision))
    }

    pub fn next(&self) -> u32 {
        let mut current = self.0.load(Ordering::Acquire);
        loop {
            let next = if current == u32::MAX { 1 } else { current + 1 };
            match self
                .0
                .compare_exchange_weak(current, next, Ordering::AcqRel, Ordering::Acquire)
            {
                Ok(_) => return next,
                Err(observed) => current = observed,
            }
        }
    }
}

const DASHBOARD_CACHE_CHANGED_EVENT: &str = "dashy://dashboard-cache-changed";
const DASHBOARD_CACHE_EVENT_TARGETS: [&str; 3] = ["main", "settings", "onboarding"];

pub(crate) fn dashboard_cache_event_targets() -> [&'static str; 3] {
    DASHBOARD_CACHE_EVENT_TARGETS
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
struct DashboardCacheChangedEvent {
    revision: u32,
}

pub fn emit_dashboard_cache_changed(app: &AppHandle) -> Result<(), String> {
    let revision = app.state::<AppState>().cache_change_revision.next();
    for target in dashboard_cache_event_targets() {
        app.emit_to(
            target,
            DASHBOARD_CACHE_CHANGED_EVENT,
            DashboardCacheChangedEvent { revision },
        )
        .map_err(|error| format!("failed to notify the dashboard cache: {error}"))?;
    }
    Ok(())
}

#[tauri::command]
pub async fn get_dashboard_snapshot(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
    desktop: tauri::State<'_, crate::desktop::DesktopState>,
    force: Option<bool>,
) -> Result<DashboardSnapshot, String> {
    let dashboard = state.dashboard.clone();
    let force = force.unwrap_or(false);
    let settings = desktop.settings.current()?;
    let snapshot = dashboard
        .get_snapshot_for(force, refreshable_providers(&settings))
        .await;
    if force {
        emit_dashboard_cache_changed(&app)?;
    }
    Ok(snapshot)
}

#[tauri::command]
pub async fn refresh_dashboard_provider(
    window: tauri::WebviewWindow,
    state: tauri::State<'_, AppState>,
    desktop: tauri::State<'_, crate::desktop::DesktopState>,
    provider: ProviderId,
) -> Result<DashboardSnapshot, String> {
    crate::authorize_caller(&window, &["main"])?;
    let settings = desktop.settings.current()?;
    if !refreshable_providers(&settings).contains(&provider) {
        return Err("provider is disabled".to_owned());
    }
    let dashboard = state.dashboard.clone();
    Ok(dashboard.refresh_provider(provider).await)
}

#[cfg(test)]
mod tests {
    use crate::{
        dashboard::models::ProviderId,
        desktop::settings::{AppSettings, CURRENT_PROVIDER_SETUP_VERSION},
    };

    use super::{dashboard_cache_event_targets, refreshable_providers, DashboardCacheRevision};

    #[test]
    fn cache_change_revision_is_bounded_nonzero_and_wraps_safely() {
        let revisions = DashboardCacheRevision::starting_at(u32::MAX - 1);
        assert_eq!(revisions.next(), u32::MAX);
        assert_eq!(revisions.next(), 1);
        assert_eq!(revisions.next(), 2);
    }

    #[test]
    fn cache_events_target_all_existing_app_windows() {
        assert_eq!(
            dashboard_cache_event_targets(),
            ["main", "settings", "onboarding"]
        );
    }

    #[test]
    fn setup_gate_hides_legacy_provider_selection_from_dashboard_refreshes() {
        let settings = AppSettings {
            onboarding_completed: true,
            enabled_providers: ProviderId::ALL.to_vec(),
            provider_setup_version: CURRENT_PROVIDER_SETUP_VERSION - 1,
            ..AppSettings::default()
        };

        assert!(settings.requires_provider_setup());
        assert!(refreshable_providers(&settings).is_empty());
    }

    #[test]
    fn setup_gate_hides_incomplete_new_user_selection_from_selected_refreshes() {
        let settings = AppSettings {
            enabled_providers: vec![ProviderId::Claude],
            provider_setup_version: CURRENT_PROVIDER_SETUP_VERSION,
            ..AppSettings::default()
        };

        assert!(settings.requires_provider_setup());
        assert!(!refreshable_providers(&settings).contains(&ProviderId::Claude));
    }

    #[test]
    fn completed_setup_preserves_the_exact_provider_selection() {
        let settings = AppSettings {
            onboarding_completed: true,
            enabled_providers: vec![ProviderId::Codex, ProviderId::GitHub],
            provider_setup_version: CURRENT_PROVIDER_SETUP_VERSION,
            ..AppSettings::default()
        };

        assert_eq!(
            refreshable_providers(&settings),
            [ProviderId::Codex, ProviderId::GitHub]
        );
    }
}
