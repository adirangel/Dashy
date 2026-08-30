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

async fn run_provider_selection_lifecycle<L, P, E, R, RF, N, A>(
    gate: &tokio::sync::Mutex<()>,
    load_previous: L,
    persist: P,
    publish: E,
    refresh: R,
    notify: N,
    after: A,
) -> Result<AppSettings, String>
where
    L: FnOnce() -> Result<Vec<ProviderId>, String>,
    P: FnOnce() -> Result<AppSettings, String>,
    E: FnOnce(&[ProviderId], &AppSettings) -> Result<(), String>,
    R: FnOnce(Vec<ProviderId>) -> RF,
    RF: Future<Output = ()>,
    N: FnOnce() -> Result<(), String>,
    A: FnOnce(&AppSettings) -> Result<(), String>,
{
    let _selection_guard = gate.lock().await;
    let previous = load_previous()?;
    let settings = persist()?;
    publish(&previous, &settings)?;
    refresh_newly_enabled_then_notify(&previous, &settings.enabled_providers, refresh, notify)
        .await?;
    after(&settings)?;
    Ok(settings)
}

async fn complete_onboarding_lifecycle<S, H>(
    selection_lifecycle: S,
    hide: H,
) -> Result<AppSettings, String>
where
    S: Future<Output = Result<AppSettings, String>>,
    H: FnOnce() -> Result<(), String>,
{
    let settings = selection_lifecycle.await?;
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
    if patch.enabled_providers.is_none() {
        let settings = state.settings.update(patch)?;
        emit_settings_changed(&app, &settings)?;
        state.refresh_tray(&app, &settings)?;
        return Ok(settings);
    }

    let dashboard = dashboard.dashboard.clone();
    let cache_event_app = app.clone();
    run_provider_selection_lifecycle(
        &state.provider_selection_gate,
        || Ok(state.settings.current()?.enabled_providers),
        || state.settings.update(patch),
        |previous, settings| {
            if settings.enabled_providers != previous {
                state.controller.queue_interaction(EdgeInteraction::Dismiss);
            }
            emit_settings_changed(&app, settings)
        },
        move |providers| async move {
            dashboard.get_snapshot_for(true, &providers).await;
        },
        move || emit_dashboard_cache_changed(&cache_event_app),
        |settings| state.refresh_tray(&app, settings),
    )
    .await
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
    let dashboard = dashboard.dashboard.clone();
    let cache_event_app = app.clone();
    complete_onboarding_lifecycle(
        run_provider_selection_lifecycle(
            &state.provider_selection_gate,
            || Ok(state.settings.current()?.enabled_providers),
            || {
                state.settings.update(SettingsPatch {
                    onboarding_completed: Some(true),
                    enabled_providers: Some(enabled_providers),
                    ..Default::default()
                })
            },
            |_, settings| {
                state.controller.queue_interaction(EdgeInteraction::Dismiss);
                emit_settings_changed(&app, settings)
            },
            move |providers| async move {
                dashboard.get_snapshot_for(true, &providers).await;
            },
            move || emit_dashboard_cache_changed(&cache_event_app),
            |settings| state.refresh_tray(&app, settings),
        ),
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
        run_provider_selection_lifecycle, settings_event_targets, ExitRequest, NotchInteraction,
    };
    use crate::dashboard::models::ProviderId;
    use crate::desktop::settings::AppSettings;

    #[tokio::test]
    async fn onboarding_completion_runs_persist_publish_refresh_cache_tray_and_hide_in_order() {
        let calls = Mutex::new(Vec::new());
        let gate = tokio::sync::Mutex::new(());
        let settings = complete_onboarding_lifecycle(
            run_provider_selection_lifecycle(
                &gate,
                || Ok(Vec::new()),
                || {
                    calls.lock().unwrap().push("persist");
                    Ok(AppSettings {
                        onboarding_completed: true,
                        enabled_providers: vec![ProviderId::Codex],
                        ..Default::default()
                    })
                },
                |_, _| {
                    calls.lock().unwrap().push("publish");
                    Ok(())
                },
                |_| async {
                    calls.lock().unwrap().push("refresh");
                },
                || {
                    calls.lock().unwrap().push("cache");
                    Ok(())
                },
                |_| {
                    calls.lock().unwrap().push("tray");
                    Ok(())
                },
            ),
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
            ["persist", "publish", "refresh", "cache", "tray", "hide"]
        );
    }

    #[tokio::test]
    async fn onboarding_persistence_failure_keeps_the_window_open() {
        let calls = Mutex::new(Vec::new());
        let gate = tokio::sync::Mutex::new(());
        let result = complete_onboarding_lifecycle(
            run_provider_selection_lifecycle(
                &gate,
                || Ok(Vec::new()),
                || Err("save failed".to_owned()),
                |_, _| {
                    calls.lock().unwrap().push("publish");
                    Ok(())
                },
                |_| async {
                    calls.lock().unwrap().push("refresh");
                },
                || {
                    calls.lock().unwrap().push("cache");
                    Ok(())
                },
                |_| {
                    calls.lock().unwrap().push("tray");
                    Ok(())
                },
            ),
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

    #[tokio::test]
    async fn concurrent_provider_selection_commands_serialize_final_state_and_fetch_scope() {
        let gate = Arc::new(tokio::sync::Mutex::new(()));
        let selected = Arc::new(Mutex::new(Vec::<ProviderId>::new()));
        let fetch_scopes = Arc::new(Mutex::new(Vec::<Vec<ProviderId>>::new()));
        let first_refresh_started = Arc::new(tokio::sync::Notify::new());
        let release_first_refresh = Arc::new(tokio::sync::Notify::new());

        let first = tokio::spawn({
            let gate = gate.clone();
            let selected_for_load = selected.clone();
            let selected_for_persist = selected.clone();
            let fetch_scopes = fetch_scopes.clone();
            let first_refresh_started = first_refresh_started.clone();
            let release_first_refresh = release_first_refresh.clone();
            async move {
                run_provider_selection_lifecycle(
                    &gate,
                    move || Ok(selected_for_load.lock().unwrap().clone()),
                    move || {
                        *selected_for_persist.lock().unwrap() = vec![ProviderId::Claude];
                        Ok(AppSettings {
                            enabled_providers: vec![ProviderId::Claude],
                            ..Default::default()
                        })
                    },
                    |_, _| Ok(()),
                    move |providers| async move {
                        first_refresh_started.notify_one();
                        release_first_refresh.notified().await;
                        fetch_scopes.lock().unwrap().push(providers);
                    },
                    || Ok(()),
                    |_| Ok(()),
                )
                .await
            }
        });

        first_refresh_started.notified().await;
        let second = tokio::spawn({
            let gate = gate.clone();
            let selected_for_load = selected.clone();
            let selected_for_persist = selected.clone();
            let fetch_scopes = fetch_scopes.clone();
            async move {
                run_provider_selection_lifecycle(
                    &gate,
                    move || Ok(selected_for_load.lock().unwrap().clone()),
                    move || {
                        *selected_for_persist.lock().unwrap() = vec![ProviderId::Codex];
                        Ok(AppSettings {
                            enabled_providers: vec![ProviderId::Codex],
                            ..Default::default()
                        })
                    },
                    |_, _| Ok(()),
                    move |providers| async move {
                        fetch_scopes.lock().unwrap().push(providers);
                    },
                    || Ok(()),
                    |_| Ok(()),
                )
                .await
            }
        });

        tokio::task::yield_now().await;
        assert_eq!(*selected.lock().unwrap(), [ProviderId::Claude]);
        assert!(fetch_scopes.lock().unwrap().is_empty());
        assert!(!second.is_finished());

        release_first_refresh.notify_one();
        first.await.unwrap().unwrap();
        second.await.unwrap().unwrap();
        assert_eq!(*selected.lock().unwrap(), [ProviderId::Codex]);
        assert_eq!(
            *fetch_scopes.lock().unwrap(),
            [vec![ProviderId::Claude], vec![ProviderId::Codex]]
        );
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
