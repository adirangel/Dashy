pub mod dashboard;
pub mod desktop;
pub mod setup;

include!("../app_commands.rs");

macro_rules! generate_app_handler {
    ($($command:ident),* $(,)?) => {
        tauri::generate_handler![$($command),*]
    };
}

use std::sync::Arc;

use dashboard::commands::{emit_dashboard_cache_changed, refreshable_providers, AppState};
use desktop::{
    controller::DesktopController,
    edge::EdgeInteraction,
    menu::{build_menu_spec, build_native_menu},
    platform::DesktopProbe,
    settings::{EdgePlacement, MonitorPreference, SettingsPatch},
    DesktopState,
};
use tauri::{AppHandle, Manager};

const UNAUTHORIZED_WINDOW_ERROR: &str = "command is not available from this window";

pub(crate) fn authorize_caller_label(label: &str, allowed: &[&str]) -> Result<(), String> {
    if allowed.contains(&label) {
        Ok(())
    } else {
        Err(UNAUTHORIZED_WINDOW_ERROR.to_owned())
    }
}

pub(crate) fn authorize_caller(
    window: &tauri::WebviewWindow,
    allowed: &[&str],
) -> Result<(), String> {
    authorize_caller_label(window.label(), allowed)
}

pub fn run() {
    use dashboard::{
        commands::{get_dashboard_snapshot, refresh_dashboard_provider},
        process::SystemProcessRunner,
        providers::{claude::ClaudeProvider, codex::CodexProvider, github::GitHubProvider},
        service::{DashboardService, SystemClock},
    };
    use desktop::{
        commands::{
            begin_notch_exit, complete_notch_exit, complete_onboarding, get_current_edge_view,
            get_settings, list_monitors, open_settings, set_notch_interaction, set_tray_labels,
            show_notch_menu, update_settings,
        },
        controller::{start_controller_runtime, TauriWindowPort, WindowPort},
        menu::TrayState,
        settings::service_from_tauri_store,
    };
    use setup::{
        commands::{get_provider_setup_states, install_provider, login_provider, SetupState},
        service::SetupService,
    };
    use tauri::{
        tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
        WindowEvent,
    };

    let process_runner = SystemProcessRunner;
    let dashboard = Arc::new(DashboardService::new(
        Arc::new(GitHubProvider::new(process_runner)),
        Arc::new(CodexProvider::new(process_runner)),
        Arc::new(ClaudeProvider::new(process_runner)),
        Arc::new(SystemClock),
    ));

    tauri::Builder::default()
        .plugin(tauri_plugin_store::Builder::default().build())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .manage(AppState::new(dashboard))
        .manage(SetupState::new(Arc::new(SetupService::new(Arc::new(
            SystemProcessRunner,
        )))))
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
            let settings_side_effect_gate = Arc::new(std::sync::Mutex::new(()));
            let settings_side_effect_gate_for_refresh = settings_side_effect_gate.clone();
            let runtime = start_controller_runtime(
                controller.clone(),
                settings_changes,
                Arc::new(move || {
                    desktop::with_authoritative_settings_side_effect(
                        &settings_side_effect_gate_for_refresh,
                        || settings_for_refresh.current(),
                        |settings| {
                            let monitors = probe_for_refresh
                                .monitors()
                                .map_err(|error| error.to_string())?;
                            tray_for_refresh.refresh(&app_handle, settings, &monitors)
                        },
                    )
                }),
            );

            app.manage(DesktopState {
                settings,
                provider_selection_gate: tokio::sync::Mutex::new(()),
                settings_side_effect_gate,
                controller,
                probe,
                runtime,
                tray: tray_state,
            });

            let provider_setup_required = current_settings.requires_provider_setup();
            if provider_setup_required {
                desktop::show_onboarding_window(app.handle()).map_err(std::io::Error::other)?;
            }

            let dashboard = app.state::<AppState>().dashboard.clone();
            let enabled_providers = refreshable_providers(&current_settings).to_vec();
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
        .invoke_handler(dashy_app_commands!(generate_app_handler))
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
                Ok(settings) => refreshable_providers(&settings).to_vec(),
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
            let Ok(settings) = state.settings.current() else {
                return;
            };
            let _ = match settings_window_label(&settings) {
                "onboarding" => desktop::show_onboarding_window(app),
                _ => desktop::show_settings_window(app),
            };
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

fn settings_window_label(settings: &desktop::settings::AppSettings) -> &'static str {
    if settings.requires_provider_setup() {
        "onboarding"
    } else {
        "settings"
    }
}

fn update_placement(app: &AppHandle, state: &DesktopState, placement: EdgePlacement) {
    let Ok(_side_effect_guard) = state.settings_side_effect_gate.lock() else {
        return;
    };
    if let Ok(settings) = state.settings.update(SettingsPatch {
        placement: Some(placement),
        ..Default::default()
    }) {
        let _ = desktop::commands::emit_settings_changed(app, &settings);
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
    let Ok(_side_effect_guard) = state.settings_side_effect_gate.lock() else {
        return;
    };
    if let Ok(settings) = state.settings.update(SettingsPatch {
        monitor: Some(monitor),
        ..Default::default()
    }) {
        let _ = desktop::commands::emit_settings_changed(app, &settings);
        let _ = state.refresh_tray(app, &settings);
    }
}

#[cfg(test)]
mod config_tests {
    use super::{authorize_caller_label, settings_window_label};
    use crate::desktop::settings::AppSettings;

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

    #[test]
    fn windows_bundle_is_an_upgradeable_x64_msi_with_online_webview_bootstrap() {
        let config_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tauri.conf.json");
        let config: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(config_path).unwrap()).unwrap();
        let bundle = &config["bundle"];
        let windows = &bundle["windows"];

        assert_eq!(bundle["targets"], serde_json::json!(["msi"]));
        assert_eq!(windows["allowDowngrades"], false);
        assert_eq!(
            windows["webviewInstallMode"],
            serde_json::json!({
                "type": "downloadBootstrapper",
                "silent": true
            })
        );
        assert_eq!(windows["wix"]["language"], "en-US");
        assert_eq!(
            windows["wix"]["upgradeCode"],
            "ea949cb5-35f6-540d-a3ee-0cc7721c122c"
        );
        let windows = windows.as_object().unwrap();
        for signing_key in [
            "certificateThumbprint",
            "digestAlgorithm",
            "signCommand",
            "timestampUrl",
            "tsp",
        ] {
            assert!(
                windows.get(signing_key).is_none(),
                "unsigned bundles must omit the {signing_key} signing field"
            );
        }
    }

    #[test]
    fn onboarding_window_is_hidden_safe_and_uses_least_privilege_commands() {
        let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let config: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(manifest_dir.join("tauri.conf.json")).unwrap(),
        )
        .unwrap();
        let onboarding = config["app"]["windows"]
            .as_array()
            .unwrap()
            .iter()
            .find(|window| window["label"] == "onboarding")
            .unwrap();

        assert_eq!(onboarding["title"], "Set up Dashy");
        assert_eq!(onboarding["width"], 680);
        assert_eq!(onboarding["height"], 640);
        assert_eq!(onboarding["minWidth"], 520);
        assert_eq!(onboarding["minHeight"], 560);
        assert_eq!(onboarding["visible"], false);
        assert_eq!(onboarding["center"], true);
        assert_eq!(onboarding["resizable"], true);
        assert_eq!(onboarding["decorations"], true);
        assert_eq!(onboarding["transparent"], false);
        assert_eq!(onboarding["skipTaskbar"], false);

        let capabilities: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(manifest_dir.join("capabilities/default.json")).unwrap(),
        )
        .unwrap();
        let onboarding_capability = capabilities
            .as_array()
            .unwrap()
            .iter()
            .find(|capability| capability["identifier"] == "onboarding-capability")
            .unwrap();
        assert_eq!(
            onboarding_capability["windows"],
            serde_json::json!(["onboarding"])
        );
        assert_eq!(
            onboarding_capability["permissions"],
            serde_json::json!([
                "core:event:allow-listen",
                "core:event:allow-unlisten",
                "core:event:allow-emit-to",
                "core:window:allow-is-visible",
                "core:window:allow-is-focused",
                "allow-get-settings",
                "allow-get-provider-setup-states",
                "allow-install-provider",
                "allow-login-provider",
                "allow-complete-onboarding",
                "allow-set-tray-labels",
                {
                    "identifier": "opener:allow-open-url",
                    "allow": [
                        { "url": "https://code.claude.com/docs/en/setup" },
                        { "url": "https://learn.chatgpt.com/docs/codex/cli" },
                        { "url": "https://cli.github.com/" }
                    ]
                }
            ])
        );
    }

    #[test]
    fn custom_command_capabilities_are_least_privilege_per_window() {
        let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let capabilities: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(manifest_dir.join("capabilities/default.json")).unwrap(),
        )
        .unwrap();
        let capability = |identifier: &str| {
            capabilities
                .as_array()
                .unwrap()
                .iter()
                .find(|capability| capability["identifier"] == identifier)
                .unwrap()
        };

        let main = capability("main-capability");
        assert_eq!(
            main["permissions"],
            serde_json::json!([
                "core:event:allow-listen",
                "core:event:allow-unlisten",
                "allow-get-dashboard-snapshot",
                "allow-refresh-dashboard-provider",
                "allow-get-settings",
                "allow-get-current-edge-view",
                "allow-set-notch-interaction",
                "allow-begin-notch-exit",
                "allow-complete-notch-exit",
                "allow-show-notch-menu",
                "allow-open-settings"
            ])
        );

        let settings = capability("settings-capability");
        assert_eq!(
            settings["permissions"],
            serde_json::json!([
                "core:event:allow-listen",
                "core:event:allow-unlisten",
                "core:event:allow-emit-to",
                "core:window:allow-is-visible",
                "core:window:allow-is-focused",
                "autostart:allow-enable",
                "autostart:allow-disable",
                "autostart:allow-is-enabled",
                "allow-get-settings",
                "allow-update-settings",
                "allow-list-monitors",
                "allow-set-tray-labels",
                "allow-get-dashboard-snapshot",
                "allow-get-provider-setup-states",
                "allow-install-provider",
                "allow-login-provider",
                {
                    "identifier": "opener:allow-open-url",
                    "allow": [
                        { "url": "https://code.claude.com/docs/en/setup" },
                        { "url": "https://learn.chatgpt.com/docs/codex/cli" },
                        { "url": "https://cli.github.com/" }
                    ]
                }
            ])
        );

        for capability in capabilities.as_array().unwrap() {
            let serialized = serde_json::to_string(&capability["permissions"]).unwrap();
            for forbidden in [
                "core:default",
                "core:menu:default",
                "core:tray:default",
                "core:path:default",
                "core:image:default",
                "core:webview:default",
                "core:window:allow-set-position",
                "core:window:allow-start-dragging",
            ] {
                assert!(
                    !serialized.contains(forbidden),
                    "{} unexpectedly grants {forbidden}",
                    capability["identifier"]
                );
            }
        }
    }

    #[test]
    fn build_and_runtime_handlers_share_one_command_registry() {
        let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let build = std::fs::read_to_string(manifest_dir.join("build.rs")).unwrap();
        let runtime = std::fs::read_to_string(manifest_dir.join("src/lib.rs")).unwrap();
        let registry = manifest_dir.join("app_commands.rs");

        assert!(registry.is_file());
        assert!(build.contains("include!(\"app_commands.rs\")"));
        assert!(runtime.contains("include!(\"../app_commands.rs\")"));
        assert!(runtime.contains(".invoke_handler(dashy_app_commands!(generate_app_handler))"));
    }

    #[test]
    fn app_manifest_generates_permissions_for_every_registered_custom_command() {
        let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let manifests: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(manifest_dir.join("gen/schemas/acl-manifests.json")).unwrap(),
        )
        .unwrap();
        let permissions = manifests["__app-acl__"]["permissions"].as_object().unwrap();
        let generated = permissions
            .keys()
            .filter_map(|identifier| identifier.strip_prefix("allow-"))
            .collect::<std::collections::BTreeSet<_>>();
        let expected = [
            "begin-notch-exit",
            "complete-notch-exit",
            "complete-onboarding",
            "get-current-edge-view",
            "get-dashboard-snapshot",
            "get-provider-setup-states",
            "get-settings",
            "install-provider",
            "list-monitors",
            "login-provider",
            "open-settings",
            "refresh-dashboard-provider",
            "set-notch-interaction",
            "set-tray-labels",
            "show-notch-menu",
            "update-settings",
        ]
        .into_iter()
        .collect::<std::collections::BTreeSet<_>>();
        assert_eq!(generated, expected);
    }

    #[test]
    fn mutation_caller_validation_rejects_other_windows_with_a_sanitized_error() {
        assert!(authorize_caller_label("settings", &["settings"]).is_ok());
        assert!(authorize_caller_label("onboarding", &["onboarding", "settings"]).is_ok());
        assert_eq!(
            authorize_caller_label("main", &["settings"]),
            Err("command is not available from this window".to_owned())
        );
        assert_eq!(
            authorize_caller_label("untrusted", &["main"]),
            Err("command is not available from this window".to_owned())
        );
    }

    #[test]
    fn settings_action_targets_onboarding_until_setup_is_completed() {
        use crate::desktop::settings::CURRENT_PROVIDER_SETUP_VERSION;

        let mut settings = AppSettings::default();
        assert_eq!(settings_window_label(&settings), "onboarding");

        settings.onboarding_completed = true;
        assert_eq!(settings_window_label(&settings), "onboarding");

        settings.provider_setup_version = CURRENT_PROVIDER_SETUP_VERSION;
        assert_eq!(settings_window_label(&settings), "settings");
    }
}
