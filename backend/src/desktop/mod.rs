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
    pub settings_side_effect_gate: Arc<std::sync::Mutex<()>>,
    pub controller: Arc<controller::DesktopController>,
    pub probe: Arc<dyn platform::DesktopProbe>,
    pub runtime: controller::ControllerRuntime,
    pub tray: Arc<menu::TrayState>,
}

pub(crate) fn with_authoritative_settings_side_effect<T, L, F>(
    gate: &std::sync::Mutex<()>,
    load_current: L,
    side_effect: F,
) -> Result<T, String>
where
    L: FnOnce() -> Result<settings::AppSettings, String>,
    F: FnOnce(&settings::AppSettings) -> Result<T, String>,
{
    let _side_effect_guard = gate
        .lock()
        .map_err(|_| "settings side-effect lock poisoned".to_owned())?;
    let current = load_current()?;
    side_effect(&current)
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

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::with_authoritative_settings_side_effect;
    use crate::desktop::settings::{AppSettings, EdgePlacement, LocaleCode};

    #[test]
    fn runtime_style_side_effect_locks_before_loading_authoritative_settings() {
        let gate = Mutex::new(());
        let expected = AppSettings {
            placement: EdgePlacement::Left,
            locale: LocaleCode::He,
            ..Default::default()
        };
        let loaded = expected.clone();

        let returned = with_authoritative_settings_side_effect(
            &gate,
            || {
                assert!(
                    gate.try_lock().is_err(),
                    "authoritative settings must be loaded only after acquiring the side-effect gate"
                );
                Ok(loaded)
            },
            |settings| Ok(settings.clone()),
        )
        .unwrap();

        assert_eq!(returned, expected);
    }
}
