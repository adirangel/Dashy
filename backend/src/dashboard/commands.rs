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
    let enabled = desktop.settings.current()?.enabled_providers;
    let snapshot = dashboard.get_snapshot_for(force, &enabled).await;
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
    let enabled = desktop.settings.current()?.enabled_providers;
    if !enabled.contains(&provider) {
        return Err("provider is disabled".to_owned());
    }
    let dashboard = state.dashboard.clone();
    Ok(dashboard.refresh_provider(provider).await)
}

#[cfg(test)]
mod tests {
    use super::{dashboard_cache_event_targets, DashboardCacheRevision};

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
}
