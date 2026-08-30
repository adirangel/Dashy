use std::sync::{Mutex, RwLock};

use serde::{Deserialize, Serialize};
use tauri::{
    menu::{CheckMenuItem, IsMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu},
    tray::TrayIcon,
    Manager, Runtime,
};

use super::{
    platform::MonitorDescriptor,
    settings::{AppSettings, EdgePlacement, MonitorPreference},
};

const MAX_LABEL_SCALARS: usize = 80;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TrayLabels {
    pub show: String,
    pub refresh_all: String,
    pub placement: String,
    pub right: String,
    pub left: String,
    pub top: String,
    pub monitor: String,
    pub primary_monitor: String,
    pub unavailable: String,
    pub settings: String,
    pub quit: String,
}

impl Default for TrayLabels {
    fn default() -> Self {
        Self {
            show: "Show Dashy".into(),
            refresh_all: "Refresh all providers".into(),
            placement: "Placement".into(),
            right: "Right".into(),
            left: "Left".into(),
            top: "Top".into(),
            monitor: "Monitor".into(),
            primary_monitor: "Primary".into(),
            unavailable: "Unavailable".into(),
            settings: "Settings".into(),
            quit: "Quit Dashy".into(),
        }
    }
}

impl TrayLabels {
    pub fn validate(&self) -> Result<(), String> {
        for (name, value) in [
            ("show", &self.show),
            ("refresh_all", &self.refresh_all),
            ("placement", &self.placement),
            ("right", &self.right),
            ("left", &self.left),
            ("top", &self.top),
            ("monitor", &self.monitor),
            ("primary_monitor", &self.primary_monitor),
            ("unavailable", &self.unavailable),
            ("settings", &self.settings),
            ("quit", &self.quit),
        ] {
            let scalar_count = value.chars().count();
            if scalar_count == 0 || scalar_count > MAX_LABEL_SCALARS {
                return Err(format!(
                    "tray label {name} must contain 1 to {MAX_LABEL_SCALARS} characters"
                ));
            }
            if value.chars().any(char::is_control) {
                return Err(format!("tray label {name} contains control characters"));
            }
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MonitorResolution {
    ExactId,
    RecoveredNameAndGeometry,
    PrimaryDefault,
    PrimaryFallback,
    FirstAvailable,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolvedMonitor {
    pub monitor: MonitorDescriptor,
    pub resolution: MonitorResolution,
}

pub fn resolve_monitor(
    preference: Option<&MonitorPreference>,
    monitors: &[MonitorDescriptor],
) -> Option<ResolvedMonitor> {
    if let Some(preference) = preference {
        if let Some(monitor) = monitors.iter().find(|monitor| monitor.id == preference.id) {
            return Some(ResolvedMonitor {
                monitor: monitor.clone(),
                resolution: MonitorResolution::ExactId,
            });
        }

        let recovered = monitors
            .iter()
            .filter(|monitor| monitor.name == preference.name)
            .filter(|monitor| work_area_matches(preference, monitor))
            .collect::<Vec<_>>();
        if let [monitor] = recovered.as_slice() {
            return Some(ResolvedMonitor {
                monitor: (*monitor).clone(),
                resolution: MonitorResolution::RecoveredNameAndGeometry,
            });
        }
    }

    monitors
        .iter()
        .find(|monitor| monitor.primary)
        .or_else(|| monitors.first())
        .map(|monitor| ResolvedMonitor {
            monitor: monitor.clone(),
            resolution: if monitor.primary {
                if preference.is_some() {
                    MonitorResolution::PrimaryFallback
                } else {
                    MonitorResolution::PrimaryDefault
                }
            } else {
                MonitorResolution::FirstAvailable
            },
        })
}

fn work_area_matches(preference: &MonitorPreference, monitor: &MonitorDescriptor) -> bool {
    let saved = &preference.last_work_area;
    monitor.work_rect.x() == saved.x
        && monitor.work_rect.y() == saved.y
        && monitor.work_rect.width() == saved.width
        && monitor.work_rect.height() == saved.height
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MenuSection {
    Action,
    Placement,
    Monitor,
    Application,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MenuItemSpec {
    pub id: String,
    pub label: String,
    pub checked: bool,
    pub enabled: bool,
    pub section: MenuSection,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MenuSpec {
    pub placement_label: String,
    pub monitor_label: String,
    pub items: Vec<MenuItemSpec>,
}

impl MenuSpec {
    pub fn item(&self, id: &str) -> Option<&MenuItemSpec> {
        self.items.iter().find(|item| item.id == id)
    }
}

pub fn build_menu_spec(
    labels: &TrayLabels,
    settings: &AppSettings,
    monitors: &[MonitorDescriptor],
) -> Result<MenuSpec, String> {
    labels.validate()?;
    let resolved = resolve_monitor(settings.monitor.as_ref(), monitors);
    let resolved_id = resolved
        .as_ref()
        .map(|resolved| resolved.monitor.id.as_str());
    let mut items = vec![
        item("show", &labels.show, false, true, MenuSection::Action),
        item(
            "refresh_all",
            &labels.refresh_all,
            false,
            true,
            MenuSection::Action,
        ),
        item(
            "placement_right",
            &labels.right,
            settings.placement == EdgePlacement::Right,
            true,
            MenuSection::Placement,
        ),
        item(
            "placement_left",
            &labels.left,
            settings.placement == EdgePlacement::Left,
            true,
            MenuSection::Placement,
        ),
        item(
            "placement_top",
            &labels.top,
            settings.placement == EdgePlacement::Top,
            true,
            MenuSection::Placement,
        ),
        item(
            "monitor_primary",
            &labels.primary_monitor,
            resolved.as_ref().is_some_and(|resolved| {
                resolved.monitor.primary
                    && matches!(
                        resolved.resolution,
                        MonitorResolution::PrimaryDefault | MonitorResolution::PrimaryFallback
                    )
            }),
            true,
            MenuSection::Monitor,
        ),
    ];

    for monitor in monitors {
        items.push(item(
            format!("monitor_{}", monitor.id),
            &monitor.name,
            resolved_id == Some(monitor.id.as_str())
                && !matches!(
                    resolved.as_ref().map(|resolved| resolved.resolution),
                    Some(MonitorResolution::PrimaryDefault | MonitorResolution::PrimaryFallback)
                ),
            true,
            MenuSection::Monitor,
        ));
    }

    if let Some(saved) = settings.monitor.as_ref() {
        let available_or_recovered = resolved.as_ref().is_some_and(|resolved| {
            matches!(
                resolved.resolution,
                MonitorResolution::ExactId | MonitorResolution::RecoveredNameAndGeometry
            )
        });
        if !available_or_recovered {
            items.push(item(
                format!("monitor_{}", saved.id),
                format!("{} ({})", saved.name, labels.unavailable),
                false,
                false,
                MenuSection::Monitor,
            ));
        }
    }

    items.extend([
        item(
            "settings",
            &labels.settings,
            false,
            true,
            MenuSection::Application,
        ),
        item("quit", &labels.quit, false, true, MenuSection::Application),
    ]);

    Ok(MenuSpec {
        placement_label: labels.placement.clone(),
        monitor_label: labels.monitor.clone(),
        items,
    })
}

fn item(
    id: impl Into<String>,
    label: impl Into<String>,
    checked: bool,
    enabled: bool,
    section: MenuSection,
) -> MenuItemSpec {
    MenuItemSpec {
        id: id.into(),
        label: label.into(),
        checked,
        enabled,
        section,
    }
}

pub fn build_native_menu<R: Runtime>(
    manager: &impl Manager<R>,
    spec: &MenuSpec,
) -> tauri::Result<Menu<R>> {
    let menu = Menu::new(manager)?;
    for item in spec
        .items
        .iter()
        .filter(|item| item.section == MenuSection::Action)
    {
        menu.append(&native_plain_item(manager, item)?)?;
    }
    menu.append(&PredefinedMenuItem::separator(manager)?)?;

    let placement = Submenu::new(manager, &spec.placement_label, true)?;
    append_checked_section(manager, &placement, spec, MenuSection::Placement)?;
    menu.append(&placement)?;

    let monitor = Submenu::new(manager, &spec.monitor_label, true)?;
    append_checked_section(manager, &monitor, spec, MenuSection::Monitor)?;
    menu.append(&monitor)?;
    menu.append(&PredefinedMenuItem::separator(manager)?)?;

    for item in spec
        .items
        .iter()
        .filter(|item| item.section == MenuSection::Application)
    {
        menu.append(&native_plain_item(manager, item)?)?;
    }
    Ok(menu)
}

fn native_plain_item<R: Runtime>(
    manager: &impl Manager<R>,
    item: &MenuItemSpec,
) -> tauri::Result<MenuItem<R>> {
    MenuItem::with_id(
        manager,
        item.id.clone(),
        &item.label,
        item.enabled,
        None::<&str>,
    )
}

fn append_checked_section<R: Runtime>(
    manager: &impl Manager<R>,
    submenu: &Submenu<R>,
    spec: &MenuSpec,
    section: MenuSection,
) -> tauri::Result<()> {
    for item in spec.items.iter().filter(|item| item.section == section) {
        let native = CheckMenuItem::with_id(
            manager,
            item.id.clone(),
            &item.label,
            item.enabled,
            item.checked,
            None::<&str>,
        )?;
        submenu.append(&native as &dyn IsMenuItem<R>)?;
    }
    Ok(())
}

pub struct TrayState {
    labels: RwLock<TrayLabels>,
    tray: Mutex<Option<TrayIcon>>,
}

impl Default for TrayState {
    fn default() -> Self {
        Self {
            labels: RwLock::new(TrayLabels::default()),
            tray: Mutex::new(None),
        }
    }
}

impl TrayState {
    pub fn labels(&self) -> Result<TrayLabels, String> {
        self.labels
            .read()
            .map(|labels| labels.clone())
            .map_err(|_| "tray label lock poisoned".to_string())
    }

    pub fn replace_labels(&self, labels: TrayLabels) -> Result<(), String> {
        labels.validate()?;
        *self
            .labels
            .write()
            .map_err(|_| "tray label lock poisoned".to_string())? = labels;
        Ok(())
    }

    pub fn attach(&self, tray: TrayIcon) -> Result<(), String> {
        *self
            .tray
            .lock()
            .map_err(|_| "tray state lock poisoned".to_string())? = Some(tray);
        Ok(())
    }

    pub fn refresh(
        &self,
        manager: &impl Manager<tauri::Wry>,
        settings: &AppSettings,
        monitors: &[MonitorDescriptor],
    ) -> Result<(), String> {
        let labels = self.labels()?;
        let spec = build_menu_spec(&labels, settings, monitors)?;
        let menu = build_native_menu(manager, &spec)
            .map_err(|error| format!("failed to build tray menu: {error}"))?;
        let tray = self
            .tray
            .lock()
            .map_err(|_| "tray state lock poisoned".to_string())?
            .clone();
        if let Some(tray) = tray {
            tray.set_menu(Some(menu))
                .map_err(|error| format!("failed to update tray menu: {error}"))?;
        }
        Ok(())
    }
}
