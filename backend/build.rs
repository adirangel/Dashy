const APP_COMMANDS: &[&str] = &[
    "get_dashboard_snapshot",
    "refresh_dashboard_provider",
    "get_provider_setup_states",
    "install_provider",
    "login_provider",
    "complete_onboarding",
    "get_settings",
    "get_current_edge_view",
    "update_settings",
    "set_notch_interaction",
    "begin_notch_exit",
    "complete_notch_exit",
    "list_monitors",
    "show_notch_menu",
    "set_tray_labels",
];

fn main() {
    tauri_build::try_build(
        tauri_build::Attributes::new()
            .app_manifest(tauri_build::AppManifest::new().commands(APP_COMMANDS)),
    )
    .expect("failed to build Dashy's Tauri manifest")
}
