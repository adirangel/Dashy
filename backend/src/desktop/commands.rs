use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, State};

use crate::dashboard::models::ProviderId;

use super::{
    controller::ExitToken,
    edge::{EdgeInteraction, EdgeViewState},
    menu::{build_menu_spec, build_native_menu, TrayLabels},
    settings::{AppSettings, SettingsPatch},
    DesktopState,
};

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
    app: AppHandle,
    state: State<'_, DesktopState>,
    patch: SettingsPatch,
) -> Result<AppSettings, String> {
    let settings = state.settings.update(patch)?;
    state.refresh_tray(&app, &settings)?;
    Ok(settings)
}

#[tauri::command]
pub async fn set_notch_interaction(
    state: State<'_, DesktopState>,
    interaction: NotchInteraction,
) -> Result<(), String> {
    state
        .controller
        .queue_interaction(interaction.edge_interaction());
    Ok(())
}

#[tauri::command]
pub async fn begin_notch_exit(
    state: State<'_, DesktopState>,
    request: ExitRequest,
) -> Result<bool, String> {
    Ok(state.controller.begin_exit(request.token))
}

#[tauri::command]
pub async fn complete_notch_exit(
    state: State<'_, DesktopState>,
    request: ExitRequest,
) -> Result<bool, String> {
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
pub async fn show_notch_menu(app: AppHandle, state: State<'_, DesktopState>) -> Result<(), String> {
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
pub async fn show_settings(app: AppHandle) -> Result<(), String> {
    super::show_settings_window(&app)
}

#[tauri::command]
pub async fn set_tray_labels(
    app: AppHandle,
    state: State<'_, DesktopState>,
    labels: TrayLabels,
) -> Result<(), String> {
    state.tray.replace_labels(labels)?;
    let settings = state.settings.current()?;
    state.refresh_tray(&app, &settings)
}

#[tauri::command]
pub async fn quit_dashy(app: AppHandle, state: State<'_, DesktopState>) -> Result<(), String> {
    state.runtime.cancel();
    app.exit(0);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{ExitRequest, NotchInteraction};
    use crate::dashboard::models::ProviderId;

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
