use crate::desktop::edge::{MonitorScale, MonitorWorkArea, Point, Rect};

pub type NativeWindowHandle = isize;

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

#[derive(Clone, Copy, Debug, Default)]
pub struct PlatformDesktopProbe;

impl PlatformDesktopProbe {
    pub const fn new() -> Self {
        Self
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

#[cfg(not(windows))]
impl DesktopProbe for PlatformDesktopProbe {
    fn cursor_position(&self) -> Result<Option<Point>, DesktopError> {
        Ok(None)
    }

    fn monitors(&self) -> Result<Vec<MonitorDescriptor>, DesktopError> {
        Ok(Vec::new())
    }

    fn foreground_is_fullscreen(
        &self,
        _selected_monitor: &MonitorDescriptor,
        _dashy_window_handles: &[NativeWindowHandle],
    ) -> bool {
        false
    }
}

#[cfg(all(test, not(windows)))]
mod tests {
    use super::*;

    #[test]
    fn fallback_does_not_fabricate_global_desktop_state() {
        let probe = PlatformDesktopProbe::new();

        assert_eq!(probe.cursor_position(), Ok(None));
        assert_eq!(probe.monitors(), Ok(Vec::new()));
    }
}
