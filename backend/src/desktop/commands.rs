use std::future::Future;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, State, WebviewWindow};

use crate::dashboard::{
    commands::{emit_dashboard_cache_changed, AppState},
    models::ProviderId,
};

use super::{
    controller::ExitToken,
    edge::{EdgeInteraction, EdgeViewState},
    menu::{build_menu_spec, build_native_menu, TrayLabels},
    settings::{AppSettings, SettingsPatch},
    DesktopState,
};

const SETTINGS_CHANGED_EVENT: &str = "dashy://settings-changed";
const SETTINGS_EVENT_TARGETS: [&str; 3] = ["main", "settings", "onboarding"];

pub(crate) fn settings_event_targets() -> [&'static str; 3] {
    SETTINGS_EVENT_TARGETS
}

pub(crate) fn emit_settings_changed(app: &AppHandle, settings: &AppSettings) -> Result<(), String> {
    for target in settings_event_targets() {
        app.emit_to(target, SETTINGS_CHANGED_EVENT, settings)
            .map_err(|error| format!("failed to publish settings: {error}"))?;
    }
    Ok(())
}

fn newly_enabled_providers(previous: &[ProviderId], current: &[ProviderId]) -> Vec<ProviderId> {
    ProviderId::ALL
        .into_iter()
        .filter(|provider| current.contains(provider) && !previous.contains(provider))
        .collect()
}

async fn refresh_newly_enabled_then_notify<R, RF, N>(
    previous: &[ProviderId],
    current: &[ProviderId],
    refresh: R,
    notify: N,
) -> Result<(), String>
where
    R: FnOnce(Vec<ProviderId>) -> RF,
    RF: Future<Output = ()>,
    N: FnOnce() -> Result<(), String>,
{
    let newly_enabled = newly_enabled_providers(previous, current);
    if newly_enabled.is_empty() {
        return Ok(());
    }
    refresh(newly_enabled).await;
    notify()
}

async fn complete_onboarding_lifecycle<P, E, F, FF, R, H>(
    persist: P,
    publish: E,
    refresh_newly_enabled: F,
    refresh_tray: R,
    hide: H,
) -> Result<AppSettings, String>
where
    P: FnOnce() -> Result<AppSettings, String>,
    E: FnOnce(&AppSettings) -> Result<(), String>,
    F: FnOnce(AppSettings) -> FF,
    FF: Future<Output = Result<(), String>>,
    R: FnOnce(&AppSettings) -> Result<(), String>,
    H: FnOnce() -> Result<(), String>,
{
    let settings = persist()?;
    publish(&settings)?;
    refresh_newly_enabled(settings.clone()).await?;
    refresh_tray(&settings)?;
    hide()?;
    Ok(settings)
}

#[derive(Clone, Copy, Debug, serde::Deserialize)]
#[serde(
    tag = "kind",
    content = "provider",
    rename_all = "camelCase",
    deny_unknown_fields
)]
pub enum NotchInteraction {
    EnterSafeRegion,
    LeaveSafeRegion,
    SelectProvider(ProviderId),
    ClearProvider,
    TogglePin(ProviderId),
    OutsideClick,
    Escape,
}

impl NotchInteraction {
    fn edge_interaction(self) -> EdgeInteraction {
        match self {
            Self::EnterSafeRegion => EdgeInteraction::EnterSafeRegion,
            Self::LeaveSafeRegion => EdgeInteraction::LeaveSafeRegion,
            Self::SelectProvider(provider) => EdgeInteraction::SelectProvider(provider),
            Self::ClearProvider => EdgeInteraction::ClearProvider,
            Self::TogglePin(provider) => EdgeInteraction::TogglePin(provider),
            Self::OutsideClick => EdgeInteraction::OutsideClick,
            Self::Escape => EdgeInteraction::Escape,
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExitRequest {
    token: ExitToken,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MonitorInfo {
    pub id: String,
    pub name: String,
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub primary: bool,
}

#[tauri::command]
pub async fn get_settings(state: State<'_, DesktopState>) -> Result<AppSettings, String> {
    state.settings.current()
}

#[tauri::command]
pub async fn get_current_edge_view(
    state: State<'_, DesktopState>,
) -> Result<EdgeViewState, String> {
    state.controller.current_edge_view()
}

#[tauri::command]
pub async fn update_settings(
    window: WebviewWindow,
    app: AppHandle,
    dashboard: State<'_, AppState>,
    state: State<'_, DesktopState>,
    patch: SettingsPatch,
) -> Result<AppSettings, String> {
    crate::authorize_caller(&window, &["settings"])?;
    let previous_enabled_providers = state.settings.current()?.enabled_providers;
    let settings = state.settings.update(patch)?;
    if settings.enabled_providers != previous_enabled_providers {
        state.controller.queue_interaction(EdgeInteraction::Dismiss);
    }
    emit_settings_changed(&app, &settings)?;
    let dashboard = dashboard.dashboard.clone();
    let cache_event_app = app.clone();
    refresh_newly_enabled_then_notify(
        &previous_enabled_providers,
        &settings.enabled_providers,
        move |providers| async move {
            dashboard.get_snapshot_for(true, &providers).await;
        },
        move || emit_dashboard_cache_changed(&cache_event_app),
    )
    .await?;
    state.refresh_tray(&app, &settings)?;
    Ok(settings)
}

#[tauri::command]
pub async fn complete_onboarding(
    window: WebviewWindow,
    app: AppHandle,
    dashboard: State<'_, AppState>,
    state: State<'_, DesktopState>,
    enabled_providers: Vec<ProviderId>,
) -> Result<AppSettings, String> {
    crate::authorize_caller(&window, &["onboarding"])?;
    let previous_enabled_providers = state.settings.current()?.enabled_providers;
    let dashboard = dashboard.dashboard.clone();
    let cache_event_app = app.clone();
    complete_onboarding_lifecycle(
        || {
            state.settings.update(SettingsPatch {
                onboarding_completed: Some(true),
                enabled_providers: Some(enabled_providers),
                ..Default::default()
            })
        },
        |settings| {
            state.controller.queue_interaction(EdgeInteraction::Dismiss);
            emit_settings_changed(&app, settings)
        },
        move |settings| async move {
            refresh_newly_enabled_then_notify(
                &previous_enabled_providers,
                &settings.enabled_providers,
                move |providers| async move {
                    dashboard.get_snapshot_for(true, &providers).await;
                },
                move || emit_dashboard_cache_changed(&cache_event_app),
            )
            .await
        },
        |settings| state.refresh_tray(&app, settings),
        || super::hide_onboarding_window(&app),
    )
    .await
}

#[tauri::command]
pub async fn set_notch_interaction(
    window: WebviewWindow,
    state: State<'_, DesktopState>,
    interaction: NotchInteraction,
) -> Result<(), String> {
    crate::authorize_caller(&window, &["main"])?;
    let provider = match interaction {
        NotchInteraction::SelectProvider(provider) | NotchInteraction::TogglePin(provider) => {
            Some(provider)
        }
        _ => None,
    };
    if let Some(provider) = provider {
        let enabled_providers = state.settings.current()?.enabled_providers;
        if !enabled_providers.contains(&provider) {
            return Err("provider is disabled".to_owned());
        }
    }
    state
        .controller
        .queue_interaction(interaction.edge_interaction());
    Ok(())
}

#[tauri::command]
pub async fn begin_notch_exit(
    window: WebviewWindow,
    state: State<'_, DesktopState>,
    request: ExitRequest,
) -> Result<bool, String> {
    crate::authorize_caller(&window, &["main"])?;
    Ok(state.controller.begin_exit(request.token))
}

#[tauri::command]
pub async fn complete_notch_exit(
    window: WebviewWindow,
    state: State<'_, DesktopState>,
    request: ExitRequest,
) -> Result<bool, String> {
    crate::authorize_caller(&window, &["main"])?;
    Ok(state.controller.exit_animation_complete(request.token))
}

#[tauri::command]
pub async fn list_monitors(state: State<'_, DesktopState>) -> Result<Vec<MonitorInfo>, String> {
    state
        .probe
        .monitors()
        .map(|monitors| {
            monitors
                .into_iter()
                .map(|monitor| MonitorInfo {
                    id: monitor.id,
                    name: monitor.name,
                    x: monitor.work_rect.x(),
                    y: monitor.work_rect.y(),
                    width: monitor.work_rect.width(),
                    height: monitor.work_rect.height(),
                    primary: monitor.primary,
                })
                .collect()
        })
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn show_notch_menu(
    window: WebviewWindow,
    app: AppHandle,
    state: State<'_, DesktopState>,
) -> Result<(), String> {
    crate::authorize_caller(&window, &["main"])?;
    let settings = state.settings.current()?;
    let monitors = state.probe.monitors().map_err(|error| error.to_string())?;
    let labels = state.tray.labels()?;
    let spec = build_menu_spec(&labels, &settings, &monitors)?;
    let menu = build_native_menu(&app, &spec)
        .map_err(|error| format!("failed to build notch menu: {error}"))?;
    app.get_webview_window("main")
        .ok_or_else(|| "notch window is unavailable".to_string())?
        .popup_menu(&menu)
        .map_err(|error| format!("failed to show notch menu: {error}"))
}

#[tauri::command]
pub async fn set_tray_labels(
    window: WebviewWindow,
    app: AppHandle,
    state: State<'_, DesktopState>,
    labels: TrayLabels,
) -> Result<(), String> {
    crate::authorize_caller(&window, &["settings"])?;
    state.tray.replace_labels(labels)?;
    let settings = state.settings.current()?;
    state.refresh_tray(&app, &settings)
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use super::{
        complete_onboarding_lifecycle, newly_enabled_providers, refresh_newly_enabled_then_notify,
        settings_event_targets, ExitRequest, NotchInteraction,
    };
    use crate::dashboard::models::ProviderId;
    use crate::desktop::settings::AppSettings;

    #[tokio::test]
    async fn onboarding_completion_runs_persist_publish_refresh_tray_and_hide_in_order() {
        let calls = Mutex::new(Vec::new());
        let settings = complete_onboarding_lifecycle(
            || {
                calls.lock().unwrap().push("persist");
                Ok(AppSettings {
                    onboarding_completed: true,
                    enabled_providers: vec![ProviderId::Codex],
                    ..Default::default()
                })
            },
            |_| {
                calls.lock().unwrap().push("publish");
                Ok(())
            },
            |_| async {
                calls.lock().unwrap().push("refresh");
                Ok(())
            },
            |_| {
                calls.lock().unwrap().push("tray");
                Ok(())
            },
            || {
                calls.lock().unwrap().push("hide");
                Ok(())
            },
        )
        .await
        .unwrap();

        assert_eq!(settings.enabled_providers, vec![ProviderId::Codex]);
        assert_eq!(
            *calls.lock().unwrap(),
            ["persist", "publish", "refresh", "tray", "hide"]
        );
    }

    #[tokio::test]
    async fn onboarding_persistence_failure_keeps_the_window_open() {
        let calls = Mutex::new(Vec::new());
        let result = complete_onboarding_lifecycle(
            || Err("save failed".to_owned()),
            |_| {
                calls.lock().unwrap().push("publish");
                Ok(())
            },
            |_| async {
                calls.lock().unwrap().push("refresh");
                Ok(())
            },
            |_| {
                calls.lock().unwrap().push("tray");
                Ok(())
            },
            || {
                calls.lock().unwrap().push("hide");
                Ok(())
            },
        )
        .await;

        assert_eq!(result.unwrap_err(), "save failed");
        assert!(calls.lock().unwrap().is_empty());
    }

    #[test]
    fn newly_enabled_selection_ignores_empty_and_disabled_providers() {
        assert!(newly_enabled_providers(&[], &[]).is_empty());
        assert_eq!(
            newly_enabled_providers(&[], &[ProviderId::Claude, ProviderId::GitHub]),
            [ProviderId::Claude, ProviderId::GitHub]
        );
        assert_eq!(
            newly_enabled_providers(
                &[ProviderId::Claude],
                &[ProviderId::Claude, ProviderId::Codex]
            ),
            [ProviderId::Codex]
        );
        assert!(newly_enabled_providers(
            &[ProviderId::Claude, ProviderId::Codex],
            &[ProviderId::Codex]
        )
        .is_empty());
    }

    #[tokio::test]
    async fn newly_enabled_refresh_finishes_before_cache_notification() {
        let trace = Arc::new(Mutex::new(Vec::new()));
        let refresh_trace = trace.clone();
        let notify_trace = trace.clone();
        refresh_newly_enabled_then_notify(
            &[ProviderId::Claude],
            &[ProviderId::Claude, ProviderId::GitHub],
            move |providers| async move {
                assert_eq!(providers, [ProviderId::GitHub]);
                refresh_trace.lock().unwrap().push("refresh");
            },
            move || {
                notify_trace.lock().unwrap().push("notify");
                Ok(())
            },
        )
        .await
        .unwrap();
        assert_eq!(*trace.lock().unwrap(), ["refresh", "notify"]);
    }

    #[tokio::test]
    async fn no_new_provider_skips_refresh_and_cache_notification() {
        let trace = Arc::new(Mutex::new(Vec::new()));
        let refresh_trace = trace.clone();
        let notify_trace = trace.clone();
        refresh_newly_enabled_then_notify(
            &[ProviderId::Claude, ProviderId::Codex],
            &[ProviderId::Codex],
            move |_| async move { refresh_trace.lock().unwrap().push("refresh") },
            move || {
                notify_trace.lock().unwrap().push("notify");
                Ok(())
            },
        )
        .await
        .unwrap();
        assert!(trace.lock().unwrap().is_empty());
    }

    #[test]
    fn settings_events_target_all_existing_app_windows() {
        assert_eq!(settings_event_targets(), ["main", "settings", "onboarding"]);
    }

    #[test]
    fn semantic_interaction_payloads_deserialize_without_native_pointer_data() {
        let selected: NotchInteraction = serde_json::from_value(serde_json::json!({
            "kind": "selectProvider",
            "provider": "codex"
        }))
        .unwrap();
        assert!(matches!(
            selected,
            NotchInteraction::SelectProvider(ProviderId::Codex)
        ));
    }

    #[test]
    fn semantic_interactions_reject_cursor_coordinates_and_unknown_fields() {
        let raw_pointer = serde_json::json!({
            "kind": "enterSafeRegion",
            "cursor": { "x": 1919, "y": 500 }
        });
        assert!(serde_json::from_value::<NotchInteraction>(raw_pointer).is_err());
    }

    #[test]
    fn semantic_interaction_wire_contract_is_strict_and_exact() {
        let valid = serde_json::json!({ "kind": "togglePin", "provider": "github" });
        assert!(matches!(
            serde_json::from_value::<NotchInteraction>(valid).unwrap(),
            NotchInteraction::TogglePin(ProviderId::GitHub)
        ));

        for invalid in [
            serde_json::json!({ "kind": "togglePin", "provider": "github", "extra": true }),
            serde_json::json!({ "kind": "togglePin", "provider": "github", "cursor": { "x": 1, "y": 2 } }),
            serde_json::json!({ "kind": "togglePin", "provider": "gemini" }),
            serde_json::json!({ "kind": "togglePin" }),
            serde_json::json!({ "kind": "enterSafeRegion", "provider": "codex" }),
            serde_json::json!({ "kind": "selectProvider", "provider": null }),
            serde_json::json!({ "kind": "unknown", "provider": "codex" }),
        ] {
            assert!(
                serde_json::from_value::<NotchInteraction>(invalid.clone()).is_err(),
                "accepted invalid interaction payload: {invalid}"
            );
        }
    }

    #[test]
    fn exit_request_requires_one_bounded_exact_token() {
        assert!(serde_json::from_value::<ExitRequest>(serde_json::json!({
            "token": "exit-a1"
        }))
        .is_ok());

        for invalid in [
            serde_json::json!({}),
            serde_json::json!({ "token": "exit-a1", "extra": true }),
            serde_json::json!({ "token": null }),
            serde_json::json!({ "token": 4 }),
            serde_json::json!({ "token": "" }),
            serde_json::json!({ "token": "x".repeat(33) }),
            serde_json::json!({ "token": "UPPER" }),
            serde_json::json!({ "token": "slash/token" }),
        ] {
            assert!(
                serde_json::from_value::<ExitRequest>(invalid.clone()).is_err(),
                "accepted invalid exit request: {invalid}"
            );
        }
    }
}
