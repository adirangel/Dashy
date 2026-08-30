use std::sync::Arc;

use tauri::{AppHandle, Manager};

pub mod commands;
pub mod controller;
pub mod edge;
pub mod menu;
pub mod platform;
pub mod settings;
#[cfg(windows)]
mod windows;

pub struct DesktopState {
    pub settings: Arc<settings::SettingsService>,
    pub provider_selection_gate: tokio::sync::Mutex<()>,
    pub controller: Arc<controller::DesktopController>,
    pub probe: Arc<dyn platform::DesktopProbe>,
    pub runtime: controller::ControllerRuntime,
    pub tray: Arc<menu::TrayState>,
}

impl DesktopState {
    pub fn refresh_tray(
        &self,
        app: &AppHandle,
        settings: &settings::AppSettings,
    ) -> Result<(), String> {
        let monitors = self.probe.monitors().map_err(|error| error.to_string())?;
        self.tray.refresh(app, settings, &monitors)
    }
}

pub fn show_settings_window(app: &AppHandle) -> Result<(), String> {
    let window = app
        .get_webview_window("settings")
        .ok_or_else(|| "settings window is unavailable".to_string())?;
    window
        .show()
        .and_then(|_| window.set_focus())
        .map_err(|error| format!("failed to show settings: {error}"))
}

pub fn show_onboarding_window(app: &AppHandle) -> Result<(), String> {
    let window = app
        .get_webview_window("onboarding")
        .ok_or_else(|| "onboarding window is unavailable".to_string())?;
    window
        .show()
        .and_then(|_| window.set_focus())
        .map_err(|error| format!("failed to show onboarding: {error}"))
}

pub fn hide_onboarding_window(app: &AppHandle) -> Result<(), String> {
    app.get_webview_window("onboarding")
        .ok_or_else(|| "onboarding window is unavailable".to_string())?
        .hide()
        .map_err(|error| format!("failed to hide onboarding: {error}"))
}
