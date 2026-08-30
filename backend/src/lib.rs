pub mod dashboard;
pub mod desktop;
pub mod setup;

use std::sync::Arc;

use dashboard::commands::{emit_dashboard_cache_changed, AppState};
use desktop::{
    controller::DesktopController,
    edge::EdgeInteraction,
    menu::{build_menu_spec, build_native_menu},
    platform::DesktopProbe,
    settings::{EdgePlacement, MonitorPreference, SettingsPatch},
    DesktopState,
};
use tauri::{AppHandle, Manager};

pub fn run() {
    use dashboard::{
        commands::{get_dashboard_snapshot, refresh_dashboard_provider},
        process::SystemProcessRunner,
        providers::{claude::ClaudeProvider, codex::CodexProvider, github::GitHubProvider},
        service::{DashboardService, SystemClock},
    };
    use desktop::{
        commands::{
            begin_notch_exit, complete_notch_exit, get_current_edge_view, get_settings,
            list_monitors, set_notch_interaction, set_tray_labels, show_notch_menu,
            update_settings,
        },
        controller::{start_controller_runtime, TauriWindowPort, WindowPort},
        menu::TrayState,
        settings::service_from_tauri_store,
    };
    use tauri::{
        tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
        WindowEvent,
    };

    let process_runner = SystemProcessRunner;
    let dashboard = Arc::new(DashboardService::new(
        Arc::new(GitHubProvider::new(process_runner)),
        Arc::new(CodexProvider::new(process_runner)),
        Arc::new(ClaudeProvider::new(process_runner, process_runner)),
        Arc::new(SystemClock),
    ));

    tauri::Builder::default()
        .plugin(tauri_plugin_store::Builder::default().build())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .manage(AppState::new(dashboard))
        .on_menu_event(|app, event| handle_menu_action(app, event.id.as_ref()))
        .setup(|app| {
            let settings = Arc::new(service_from_tauri_store(app).map_err(std::io::Error::other)?);
            let settings_changes = settings.subscribe();
            let probe: Arc<dyn DesktopProbe> =
                Arc::new(desktop::platform::PlatformDesktopProbe::new());
            let window: Arc<dyn WindowPort> =
                Arc::new(TauriWindowPort::from_manager(app).map_err(std::io::Error::other)?);
            let controller = Arc::new(DesktopController::new(
                probe.clone(),
                window,
                settings.clone(),
            ));

            let tray_state = Arc::new(TrayState::default());
            let current_settings = settings.current().map_err(std::io::Error::other)?;
            let monitors = probe.monitors().unwrap_or_default();
            let spec = build_menu_spec(
                &tray_state.labels().map_err(std::io::Error::other)?,
                &current_settings,
                &monitors,
            )
            .map_err(std::io::Error::other)?;
            let menu = build_native_menu(app, &spec)?;
            let mut tray_builder = TrayIconBuilder::with_id("dashy")
                .tooltip("Dashy")
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_tray_icon_event(|tray, event| {
                    if matches!(
                        event,
                        TrayIconEvent::Click {
                            button: MouseButton::Left,
                            button_state: MouseButtonState::Up,
                            ..
                        }
                    ) {
                        if let Some(state) = tray.app_handle().try_state::<DesktopState>() {
                            state.controller.show_explicit();
                        }
                    }
                });
            if let Some(icon) = app.default_window_icon() {
                tray_builder = tray_builder.icon(icon.clone());
            }
            let tray = tray_builder.build(app)?;
            tray_state.attach(tray).map_err(std::io::Error::other)?;
            let app_handle = app.handle().clone();
            let settings_for_refresh = settings.clone();
            let probe_for_refresh = probe.clone();
            let tray_for_refresh = tray_state.clone();
            let runtime = start_controller_runtime(
                controller.clone(),
                settings_changes,
                Arc::new(move || {
                    let settings = settings_for_refresh.current()?;
                    let monitors = probe_for_refresh
                        .monitors()
                        .map_err(|error| error.to_string())?;
                    tray_for_refresh.refresh(&app_handle, &settings, &monitors)
                }),
            );

            app.manage(DesktopState {
                settings,
                controller,
                probe,
                runtime,
                tray: tray_state,
            });

            let dashboard = app.state::<AppState>().dashboard.clone();
            let enabled_providers = current_settings.enabled_providers.clone();
            tauri::async_runtime::spawn(async move {
                dashboard.get_snapshot_for(false, &enabled_providers).await;
            });
            Ok(())
        })
        .on_window_event(|window, event| match event {
            WindowEvent::CloseRequested { api, .. } => {
                api.prevent_close();
                let _ = window.hide();
                if window.label() == "main" {
                    if let Some(state) = window.app_handle().try_state::<DesktopState>() {
                        state.controller.queue_interaction(EdgeInteraction::Dismiss);
                    }
                }
            }
            WindowEvent::Focused(false) if window.label() == "main" => {
                if let Some(state) = window.app_handle().try_state::<DesktopState>() {
                    state.controller.focus_lost();
                }
            }
            _ => {}
        })
        .invoke_handler(tauri::generate_handler![
            get_dashboard_snapshot,
            refresh_dashboard_provider,
            get_settings,
            get_current_edge_view,
            update_settings,
            set_notch_interaction,
            begin_notch_exit,
            complete_notch_exit,
            list_monitors,
            show_notch_menu,
            set_tray_labels
        ])
        .run(tauri::generate_context!())
        .expect("failed to run Dashy");
}

fn handle_menu_action(app: &AppHandle, id: &str) {
    let Some(state) = app.try_state::<DesktopState>() else {
        return;
    };
    match id {
        "show" => state.controller.show_explicit(),
        "refresh_all" => {
            let dashboard = app.state::<AppState>().dashboard.clone();
            let app_handle = app.clone();
            let enabled_providers = match state.settings.current() {
                Ok(settings) => settings.enabled_providers.clone(),
                Err(_) => return,
            };
            tauri::async_runtime::spawn(async move {
                dashboard.get_snapshot_for(true, &enabled_providers).await;
                let _ = emit_dashboard_cache_changed(&app_handle);
            });
        }
        "placement_right" => update_placement(app, &state, EdgePlacement::Right),
        "placement_left" => update_placement(app, &state, EdgePlacement::Left),
        "placement_top" => update_placement(app, &state, EdgePlacement::Top),
        "monitor_primary" => update_monitor(app, &state, None),
        "settings" => {
            let _ = desktop::show_settings_window(app);
        }
        "quit" => {
            state.runtime.cancel();
            app.exit(0);
        }
        _ if id.starts_with("monitor_") => {
            select_monitor(app, &state, &id["monitor_".len()..]);
        }
        _ => {}
    }
}

fn update_placement(app: &AppHandle, state: &DesktopState, placement: EdgePlacement) {
    if let Ok(settings) = state.settings.update(SettingsPatch {
        placement: Some(placement),
        ..Default::default()
    }) {
        let _ = state.refresh_tray(app, &settings);
    }
}

fn select_monitor(app: &AppHandle, state: &DesktopState, id: &str) {
    let selected = state
        .probe
        .monitors()
        .ok()
        .and_then(|monitors| monitors.into_iter().find(|monitor| monitor.id == id));
    let Some(selected) = selected else {
        return;
    };
    update_monitor(app, state, Some(MonitorPreference::from(&selected)));
}

fn update_monitor(app: &AppHandle, state: &DesktopState, monitor: Option<MonitorPreference>) {
    if let Ok(settings) = state.settings.update(SettingsPatch {
        monitor: Some(monitor),
        ..Default::default()
    }) {
        let _ = state.refresh_tray(app, &settings);
    }
}

#[cfg(test)]
mod config_tests {
    #[test]
    fn main_window_transparency_disables_native_full_rectangle_chrome() {
        let config_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tauri.conf.json");
        let config: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(config_path).unwrap()).unwrap();
        let main = config["app"]["windows"]
            .as_array()
            .unwrap()
            .iter()
            .find(|window| window["label"] == "main")
            .unwrap();

        assert_eq!(main["transparent"], true);
        assert_eq!(
            main["shadow"], false,
            "an undecorated Windows shadow draws a frame around the full native window rectangle"
        );
        assert!(
            main.get("windowEffects").is_none(),
            "a native acrylic effect paints the entire rectangular window behind the shaped CSS surface"
        );
    }

    #[test]
    fn windows_bundle_uses_only_the_windows_icon() {
        let config_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tauri.conf.json");
        let config: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(config_path).unwrap()).unwrap();

        assert_eq!(
            config["bundle"]["icon"],
            serde_json::json!(["icons/icon.ico"])
        );
    }
}
