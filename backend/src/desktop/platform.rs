use crate::desktop::edge::{MonitorScale, MonitorWorkArea, Point, Rect};

pub type NativeWindowHandle = isize;

/// Display labels are user-visible in the tray and Settings; keep them short and
/// free of control characters whatever the platform reports.
#[cfg(not(windows))]
const MAX_DISPLAY_LABEL_SCALARS: usize = 80;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MonitorDescriptor {
    pub id: String,
    pub name: String,
    pub monitor_rect: Rect,
    pub work_rect: MonitorWorkArea,
    pub scale: MonitorScale,
    pub primary: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum DesktopError {
    #[error("desktop cursor query failed")]
    CursorQueryFailed,
    #[error("desktop monitor enumeration failed")]
    MonitorEnumerationFailed,
    #[error("desktop monitor query failed")]
    MonitorQueryFailed,
    #[error("desktop monitor callback failed")]
    MonitorCallbackFailed,
    #[error("desktop monitor geometry is invalid")]
    InvalidMonitorGeometry,
    #[error("desktop monitor scale is invalid")]
    InvalidMonitorScale,
    #[error("desktop monitor name is invalid")]
    InvalidMonitorName,
    #[error("desktop window operation failed")]
    WindowOperationFailed,
    #[error("desktop event emission failed")]
    EventEmissionFailed,
    #[error("desktop settings are unavailable")]
    SettingsUnavailable,
    #[error("no desktop monitor is available")]
    NoMonitorAvailable,
}

pub trait DesktopProbe: Send + Sync {
    fn cursor_position(&self) -> Result<Option<Point>, DesktopError>;

    fn monitors(&self) -> Result<Vec<MonitorDescriptor>, DesktopError>;

    fn foreground_is_fullscreen(
        &self,
        selected_monitor: &MonitorDescriptor,
        dashy_window_handles: &[NativeWindowHandle],
    ) -> bool;
}

/// A monitor as reported by a toolkit-level enumeration (Tauri on macOS, GDK on
/// Linux): physical pixel geometry, a human-readable name that may repeat, and
/// a floating-point scale factor. Win32 keeps its own richer descriptor.
#[cfg(not(windows))]
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct GenericRawMonitor {
    pub name: Option<String>,
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub work_x: i32,
    pub work_y: i32,
    pub work_width: u32,
    pub work_height: u32,
    pub scale_factor: f64,
    pub primary: bool,
}

/// Turns toolkit monitors into descriptors with stable, unique, safe identifiers.
///
/// Toolkits identify a display by its model name, which two identical monitors
/// share; the second occurrence gets a numbered suffix so preferences can still
/// address either one. A work area the toolkit reports outside the monitor (some
/// Wayland compositors do) is clamped to the monitor instead of failing the whole
/// probe, because a probe failure hides Dashy entirely.
#[cfg(not(windows))]
pub(crate) fn normalize_generic_monitors(
    raw_monitors: Vec<GenericRawMonitor>,
) -> Result<Vec<MonitorDescriptor>, DesktopError> {
    if raw_monitors.is_empty() {
        return Err(DesktopError::MonitorEnumerationFailed);
    }
    let mut monitors: Vec<MonitorDescriptor> = Vec::with_capacity(raw_monitors.len());
    for (index, raw) in raw_monitors.into_iter().enumerate() {
        let name = display_label(raw.name.as_deref(), index);
        let id = unique_id(&name, &monitors);
        let monitor_rect = Rect {
            x: raw.x,
            y: raw.y,
            width: raw.width,
            height: raw.height,
        };
        let monitor_area = MonitorWorkArea::new(raw.x, raw.y, raw.width, raw.height)
            .map_err(|_| DesktopError::InvalidMonitorGeometry)?;
        let work_rect = clamp_work_area(
            monitor_area,
            raw.work_x,
            raw.work_y,
            raw.work_width,
            raw.work_height,
        );
        let scale = MonitorScale::try_from_scale_factor(raw.scale_factor)
            .map_err(|_| DesktopError::InvalidMonitorScale)?;
        monitors.push(MonitorDescriptor {
            id,
            name,
            monitor_rect,
            work_rect,
            scale,
            primary: raw.primary,
        });
    }
    if !monitors.iter().any(|monitor| monitor.primary) {
        if let Some(origin) = monitors
            .iter_mut()
            .find(|monitor| monitor.monitor_rect.x == 0 && monitor.monitor_rect.y == 0)
        {
            origin.primary = true;
        }
    }
    Ok(monitors)
}

#[cfg(not(windows))]
fn display_label(name: Option<&str>, index: usize) -> String {
    let sanitized: String = name
        .unwrap_or_default()
        .chars()
        .filter(|character| !character.is_control())
        .take(MAX_DISPLAY_LABEL_SCALARS)
        .collect::<String>()
        .trim()
        .to_owned();
    if sanitized.is_empty() {
        format!("Display {}", index + 1)
    } else {
        sanitized
    }
}

#[cfg(not(windows))]
fn unique_id(name: &str, existing: &[MonitorDescriptor]) -> String {
    if !existing.iter().any(|monitor| monitor.id == name) {
        return name.to_owned();
    }
    (2..)
        .map(|ordinal| format!("{name} #{ordinal}"))
        .find(|candidate| !existing.iter().any(|monitor| &monitor.id == candidate))
        .expect("an unused numbered display id always exists")
}

#[cfg(not(windows))]
fn clamp_work_area(
    monitor: MonitorWorkArea,
    x: i32,
    y: i32,
    width: u32,
    height: u32,
) -> MonitorWorkArea {
    let monitor_rect = monitor.rect();
    let Ok(work) = MonitorWorkArea::new(x, y, width, height) else {
        return monitor;
    };
    if monitor_rect.contains_rect(work.rect()) {
        return work;
    }
    let left = i64::from(work.x()).max(i64::from(monitor.x()));
    let top = i64::from(work.y()).max(i64::from(monitor.y()));
    let right = i64::from(work.right()).min(i64::from(monitor.right()));
    let bottom = i64::from(work.bottom()).min(i64::from(monitor.bottom()));
    if right <= left || bottom <= top {
        return monitor;
    }
    let clamped = (|| {
        MonitorWorkArea::new(
            i32::try_from(left).ok()?,
            i32::try_from(top).ok()?,
            u32::try_from(right - left).ok()?,
            u32::try_from(bottom - top).ok()?,
        )
        .ok()
    })();
    clamped.unwrap_or(monitor)
}

/// Converts a toolkit cursor position (floating-point physical pixels) into the
/// integer point the edge machine consumes, rejecting non-finite values.
#[cfg(not(windows))]
pub(crate) fn cursor_point(x: f64, y: f64) -> Option<Point> {
    if !x.is_finite() || !y.is_finite() {
        return None;
    }
    let x = x.round();
    let y = y.round();
    if x < f64::from(i32::MIN)
        || x > f64::from(i32::MAX)
        || y < f64::from(i32::MIN)
        || y > f64::from(i32::MAX)
    {
        return None;
    }
    Some(Point {
        x: x as i32,
        y: y as i32,
    })
}

/// The desktop probe for the platform Dashy is running on.
///
/// - Windows talks to Win32 directly (monitor DPI, foreground window, cursor).
/// - macOS reads monitors and the cursor through Tauri and inspects the on-screen
///   window list through CoreGraphics.
/// - Linux and other Unix desktops query GDK on the main thread and cache the
///   result for the controller tick.
pub struct PlatformDesktopProbe {
    #[cfg(target_os = "macos")]
    app: tauri::AppHandle,
    #[cfg(all(unix, not(target_os = "macos")))]
    inner: crate::desktop::linux::LinuxDesktopProbe,
}

impl PlatformDesktopProbe {
    pub fn new(app: &tauri::AppHandle) -> Self {
        #[cfg(windows)]
        {
            let _ = app;
            Self {}
        }
        #[cfg(target_os = "macos")]
        {
            Self { app: app.clone() }
        }
        #[cfg(all(unix, not(target_os = "macos")))]
        {
            Self {
                inner: crate::desktop::linux::LinuxDesktopProbe::new(app.clone()),
            }
        }
    }
}

#[cfg(windows)]
impl DesktopProbe for PlatformDesktopProbe {
    fn cursor_position(&self) -> Result<Option<Point>, DesktopError> {
        crate::desktop::windows::cursor_position().map(Some)
    }

    fn monitors(&self) -> Result<Vec<MonitorDescriptor>, DesktopError> {
        crate::desktop::windows::monitors()
    }

    fn foreground_is_fullscreen(
        &self,
        selected_monitor: &MonitorDescriptor,
        dashy_window_handles: &[NativeWindowHandle],
    ) -> bool {
        crate::desktop::windows::foreground_is_fullscreen(selected_monitor, dashy_window_handles)
    }
}

#[cfg(target_os = "macos")]
impl DesktopProbe for PlatformDesktopProbe {
    fn cursor_position(&self) -> Result<Option<Point>, DesktopError> {
        let position = self
            .app
            .cursor_position()
            .map_err(|_| DesktopError::CursorQueryFailed)?;
        Ok(cursor_point(position.x, position.y))
    }

    fn monitors(&self) -> Result<Vec<MonitorDescriptor>, DesktopError> {
        crate::desktop::macos::monitors(&self.app)
    }

    fn foreground_is_fullscreen(
        &self,
        selected_monitor: &MonitorDescriptor,
        _dashy_window_handles: &[NativeWindowHandle],
    ) -> bool {
        crate::desktop::macos::foreground_is_fullscreen(selected_monitor)
    }
}

#[cfg(all(unix, not(target_os = "macos")))]
impl DesktopProbe for PlatformDesktopProbe {
    fn cursor_position(&self) -> Result<Option<Point>, DesktopError> {
        self.inner.cursor_position()
    }

    fn monitors(&self) -> Result<Vec<MonitorDescriptor>, DesktopError> {
        self.inner.monitors()
    }

    fn foreground_is_fullscreen(
        &self,
        selected_monitor: &MonitorDescriptor,
        _dashy_window_handles: &[NativeWindowHandle],
    ) -> bool {
        self.inner.foreground_is_fullscreen(selected_monitor)
    }
}

#[cfg(all(test, not(windows)))]
mod tests {
    use super::*;

    fn raw(name: Option<&str>, x: i32, y: i32, width: u32, height: u32) -> GenericRawMonitor {
        GenericRawMonitor {
            name: name.map(str::to_owned),
            x,
            y,
            width,
            height,
            work_x: x,
            work_y: y,
            work_width: width,
            work_height: height,
            scale_factor: 1.0,
            primary: false,
        }
    }

    #[test]
    fn toolkit_monitors_keep_their_names_and_get_unique_ids() {
        let monitors = normalize_generic_monitors(vec![
            GenericRawMonitor {
                primary: true,
                ..raw(Some("Built-in Retina Display"), 0, 0, 2880, 1800)
            },
            raw(Some("DELL U2719D"), 2880, 0, 2560, 1440),
            raw(Some("DELL U2719D"), 5440, 0, 2560, 1440),
        ])
        .unwrap();

        assert_eq!(monitors[0].id, "Built-in Retina Display");
        assert_eq!(monitors[0].name, "Built-in Retina Display");
        assert!(monitors[0].primary);
        assert_eq!(monitors[1].id, "DELL U2719D");
        assert_eq!(monitors[2].id, "DELL U2719D #2");
        assert_eq!(monitors[2].name, "DELL U2719D");
        assert_eq!(monitors[2].monitor_rect.x, 5440);
    }

    #[test]
    fn missing_or_unsafe_names_fall_back_to_a_bounded_label() {
        let monitors = normalize_generic_monitors(vec![
            raw(None, 0, 0, 1920, 1080),
            raw(Some("  \u{7}\n  "), 1920, 0, 1920, 1080),
            raw(Some(&"x".repeat(300)), 3840, 0, 1920, 1080),
        ])
        .unwrap();

        assert_eq!(monitors[0].name, "Display 1");
        assert_eq!(monitors[1].name, "Display 2");
        assert_eq!(monitors[2].name.chars().count(), MAX_DISPLAY_LABEL_SCALARS);
        assert!(monitors
            .iter()
            .all(|monitor| !monitor.name.chars().any(char::is_control)));
    }

    #[test]
    fn the_origin_monitor_becomes_primary_when_the_toolkit_names_none() {
        let monitors = normalize_generic_monitors(vec![
            raw(Some("Left"), -1920, 0, 1920, 1080),
            raw(Some("Main"), 0, 0, 1920, 1080),
        ])
        .unwrap();

        assert!(!monitors[0].primary);
        assert!(monitors[1].primary);
    }

    #[test]
    fn work_area_outside_the_monitor_is_clamped_instead_of_failing() {
        let mut spill = raw(Some("Main"), 0, 0, 1920, 1080);
        spill.work_x = -10;
        spill.work_y = 30;
        spill.work_width = 1940;
        spill.work_height = 1060;
        let mut disjoint = raw(Some("Second"), 1920, 0, 1920, 1080);
        disjoint.work_x = 0;
        disjoint.work_y = 0;
        disjoint.work_width = 100;
        disjoint.work_height = 100;
        let mut empty = raw(Some("Third"), 3840, 0, 1920, 1080);
        empty.work_width = 0;

        let monitors = normalize_generic_monitors(vec![spill, disjoint, empty]).unwrap();

        assert_eq!(
            (
                monitors[0].work_rect.x(),
                monitors[0].work_rect.y(),
                monitors[0].work_rect.width(),
                monitors[0].work_rect.height()
            ),
            (0, 30, 1920, 1050)
        );
        assert_eq!(monitors[1].work_rect.rect(), monitors[1].monitor_rect);
        assert_eq!(monitors[2].work_rect.rect(), monitors[2].monitor_rect);
    }

    #[test]
    fn scale_factor_is_carried_into_the_descriptor() {
        let mut retina = raw(Some("Built-in Retina Display"), 0, 0, 2880, 1800);
        retina.scale_factor = 2.0;
        let monitors = normalize_generic_monitors(vec![retina]).unwrap();
        assert_eq!(monitors[0].scale.effective_dpi(), 192);

        let mut broken = raw(Some("Broken"), 0, 0, 1920, 1080);
        broken.scale_factor = f64::NAN;
        assert_eq!(
            normalize_generic_monitors(vec![broken]),
            Err(DesktopError::InvalidMonitorScale)
        );
    }

    #[test]
    fn empty_enumeration_and_invalid_geometry_are_recoverable_errors() {
        assert_eq!(
            normalize_generic_monitors(Vec::new()),
            Err(DesktopError::MonitorEnumerationFailed)
        );
        assert_eq!(
            normalize_generic_monitors(vec![raw(Some("Zero"), 0, 0, 0, 1080)]),
            Err(DesktopError::InvalidMonitorGeometry)
        );
    }

    #[test]
    fn cursor_points_are_rounded_and_bounded() {
        assert_eq!(cursor_point(10.4, -2.6), Some(Point { x: 10, y: -3 }));
        assert_eq!(cursor_point(f64::NAN, 0.0), None);
        assert_eq!(cursor_point(0.0, f64::INFINITY), None);
        assert_eq!(cursor_point(1e12, 0.0), None);
    }
}
