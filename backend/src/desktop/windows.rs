use std::{
    mem::size_of,
    panic::{catch_unwind, AssertUnwindSafe},
    ptr,
};

use windows::{
    core::BOOL,
    Win32::{
        Foundation::{HWND, LPARAM, POINT, RECT},
        Graphics::Gdi::{
            EnumDisplayMonitors, GetMonitorInfoW, MonitorFromWindow, HDC, HMONITOR, MONITORINFOEXW,
            MONITOR_DEFAULTTONULL,
        },
        UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI},
        UI::WindowsAndMessaging::{
            GetCursorPos, GetForegroundWindow, GetWindowRect, SetWindowPos, HWND_NOTOPMOST,
            HWND_TOPMOST, MONITORINFOF_PRIMARY, SWP_HIDEWINDOW, SWP_NOACTIVATE, SWP_NOOWNERZORDER,
            SWP_SHOWWINDOW,
        },
    },
};

use crate::desktop::{
    edge::{MonitorScale, MonitorWorkArea, Point, Rect, WindowLayout},
    platform::{DesktopError, MonitorDescriptor, NativeWindowHandle},
};

const FULLSCREEN_EDGE_TOLERANCE_PX: i64 = 2;
const MAX_DISPLAY_LABEL_SCALARS: usize = 80;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum WindowZOrder {
    Topmost,
    NotTopmost,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum WindowVisibility {
    Show,
    Hide,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct WindowBoundsCall {
    handle: NativeWindowHandle,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
    z_order: WindowZOrder,
    visibility: WindowVisibility,
    no_activate: bool,
}

trait WindowBoundsApi {
    fn set_window_pos(&self, call: WindowBoundsCall) -> Result<(), DesktopError>;
}

struct Win32WindowBoundsApi;

impl WindowBoundsApi for Win32WindowBoundsApi {
    fn set_window_pos(&self, call: WindowBoundsCall) -> Result<(), DesktopError> {
        let insert_after = match call.z_order {
            WindowZOrder::Topmost => HWND_TOPMOST,
            WindowZOrder::NotTopmost => HWND_NOTOPMOST,
        };
        let mut flags = SWP_NOOWNERZORDER;
        if call.no_activate {
            flags |= SWP_NOACTIVATE;
        }
        flags |= match call.visibility {
            WindowVisibility::Show => SWP_SHOWWINDOW,
            WindowVisibility::Hide => SWP_HIDEWINDOW,
        };
        // SAFETY: `handle` is obtained from Tauri for the live main window. Bounds are checked when
        // the request is constructed, and SetWindowPos does not retain pointers.
        unsafe {
            SetWindowPos(
                HWND(call.handle as *mut core::ffi::c_void),
                Some(insert_after),
                call.x,
                call.y,
                call.width,
                call.height,
                flags,
            )
        }
        .map_err(|_| DesktopError::WindowOperationFailed)
    }
}

pub(super) fn apply_window_bounds(
    handle: NativeWindowHandle,
    layout: &WindowLayout,
) -> Result<(), DesktopError> {
    apply_window_bounds_with(&Win32WindowBoundsApi, handle, layout)
}

fn apply_window_bounds_with(
    api: &impl WindowBoundsApi,
    handle: NativeWindowHandle,
    layout: &WindowLayout,
) -> Result<(), DesktopError> {
    api.set_window_pos(WindowBoundsCall {
        handle,
        x: layout.position.x,
        y: layout.position.y,
        width: i32::try_from(layout.size.width).map_err(|_| DesktopError::WindowOperationFailed)?,
        height: i32::try_from(layout.size.height)
            .map_err(|_| DesktopError::WindowOperationFailed)?,
        z_order: if layout.always_on_top {
            WindowZOrder::Topmost
        } else {
            WindowZOrder::NotTopmost
        },
        visibility: if layout.visible {
            WindowVisibility::Show
        } else {
            WindowVisibility::Hide
        },
        no_activate: true,
    })
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct RawRect {
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
}

impl RawRect {
    const fn new(left: i32, top: i32, right: i32, bottom: i32) -> Self {
        Self {
            left,
            top,
            right,
            bottom,
        }
    }

    fn from_rect(rect: Rect) -> Option<Self> {
        if rect.width == 0 || rect.height == 0 {
            return None;
        }
        Some(Self {
            left: rect.x,
            top: rect.y,
            right: rect.x.checked_add_unsigned(rect.width)?,
            bottom: rect.y.checked_add_unsigned(rect.height)?,
        })
    }

    fn has_positive_extent(self) -> bool {
        self.right > self.left && self.bottom > self.top
    }
}

impl From<RECT> for RawRect {
    fn from(rect: RECT) -> Self {
        Self::new(rect.left, rect.top, rect.right, rect.bottom)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct RawMonitor {
    id: String,
    monitor_rect: RawRect,
    work_rect: RawRect,
    dpi_x: u32,
    dpi_y: u32,
    primary: bool,
}

struct MonitorEnumerationState {
    monitors: Vec<RawMonitor>,
    error: Option<DesktopError>,
}

pub(super) fn cursor_position() -> Result<Point, DesktopError> {
    let mut point = POINT::default();
    // SAFETY: `point` is a valid writable POINT for the duration of this synchronous call.
    unsafe { GetCursorPos(ptr::addr_of_mut!(point)) }
        .map_err(|_| DesktopError::CursorQueryFailed)?;
    Ok(Point {
        x: point.x,
        y: point.y,
    })
}

pub(super) fn monitors() -> Result<Vec<MonitorDescriptor>, DesktopError> {
    let mut state = MonitorEnumerationState {
        monitors: Vec::new(),
        error: None,
    };
    let state_ptr = ptr::addr_of_mut!(state);

    // SAFETY: EnumDisplayMonitors invokes the callback synchronously. `state_ptr` remains valid and
    // uniquely borrowed until the call returns, and the callback never stores the pointer.
    let succeeded = unsafe {
        EnumDisplayMonitors(
            None,
            None,
            Some(enumerate_monitor),
            LPARAM(state_ptr as isize),
        )
    }
    .as_bool();

    if let Some(error) = state.error {
        return Err(error);
    }
    if !succeeded || state.monitors.is_empty() {
        return Err(DesktopError::MonitorEnumerationFailed);
    }
    normalize_monitors(state.monitors)
}

pub(super) fn foreground_is_fullscreen(
    selected_monitor: &MonitorDescriptor,
    dashy_window_handles: &[NativeWindowHandle],
) -> bool {
    // GetForegroundWindow legitimately returns NULL while activation is changing.
    // SAFETY: This function has no pointer parameters and only reads desktop state.
    let foreground = unsafe { GetForegroundWindow() };
    if foreground.0.is_null() {
        return false;
    }

    let foreground_handle = foreground.0 as isize;
    if dashy_window_handles.contains(&foreground_handle) {
        return false;
    }

    let mut window_rect = RECT::default();
    // SAFETY: `foreground` came from Windows and `window_rect` is writable for this call.
    if unsafe { GetWindowRect(foreground, ptr::addr_of_mut!(window_rect)) }.is_err() {
        return false;
    }

    // MONITOR_DEFAULTTONULL prevents a stale/off-screen foreground window from being assigned to
    // an unrelated monitor.
    // SAFETY: `foreground` is used only as an opaque handle.
    let foreground_monitor = unsafe { MonitorFromWindow(foreground, MONITOR_DEFAULTTONULL) };
    if foreground_monitor.0.is_null() {
        return false;
    }

    // Query failures are deliberately treated as not fullscreen so a transient Win32 error cannot
    // suppress Dashy indefinitely.
    let Ok(foreground_monitor) = (unsafe { monitor_from_handle(foreground_monitor) }) else {
        return false;
    };
    let Some(selected_rect) = RawRect::from_rect(selected_monitor.monitor_rect) else {
        return false;
    };

    classify_fullscreen(
        foreground_handle,
        dashy_window_handles,
        &foreground_monitor.id,
        &selected_monitor.id,
        window_rect.into(),
        selected_rect,
    )
}

unsafe extern "system" fn enumerate_monitor(
    monitor: HMONITOR,
    _device_context: HDC,
    _monitor_rect: *mut RECT,
    data: LPARAM,
) -> BOOL {
    if data.0 == 0 {
        return false.into();
    }

    // SAFETY: `data` was created from a live, uniquely borrowed MonitorEnumerationState immediately
    // before the synchronous EnumDisplayMonitors call. The callback does not retain this reference.
    let Some(state) = (unsafe { (data.0 as *mut MonitorEnumerationState).as_mut() }) else {
        return false.into();
    };

    let result = catch_unwind(AssertUnwindSafe(|| {
        // SAFETY: Windows supplies a non-NULL HMONITOR to each enumeration callback.
        let monitor = unsafe { monitor_from_handle(monitor) }?;
        state.monitors.push(monitor);
        Ok::<_, DesktopError>(())
    }));

    match result {
        Ok(Ok(())) => true.into(),
        Ok(Err(error)) => {
            state.error = Some(error);
            false.into()
        }
        Err(_) => {
            state.error = Some(DesktopError::MonitorCallbackFailed);
            false.into()
        }
    }
}

unsafe fn monitor_from_handle(monitor: HMONITOR) -> Result<RawMonitor, DesktopError> {
    let mut info = MONITORINFOEXW::default();
    info.monitorInfo.cbSize = size_of::<MONITORINFOEXW>() as u32;

    // SAFETY: MONITORINFOEXW begins with MONITORINFO, cbSize identifies the full allocation, and
    // `info` remains valid and writable for this synchronous call.
    let succeeded =
        unsafe { GetMonitorInfoW(monitor, ptr::addr_of_mut!(info.monitorInfo)) }.as_bool();
    if !succeeded {
        return Err(DesktopError::MonitorQueryFailed);
    }

    let id = device_name_from_utf16(&info.szDevice)?;
    let mut dpi_x = 0;
    let mut dpi_y = 0;
    // SAFETY: `monitor` is the live handle supplied by Windows, and both DPI outputs are valid
    // writable integers for this synchronous query.
    unsafe {
        GetDpiForMonitor(
            monitor,
            MDT_EFFECTIVE_DPI,
            ptr::addr_of_mut!(dpi_x),
            ptr::addr_of_mut!(dpi_y),
        )
    }
    .map_err(|_| DesktopError::InvalidMonitorScale)?;
    Ok(RawMonitor {
        id,
        monitor_rect: info.monitorInfo.rcMonitor.into(),
        work_rect: info.monitorInfo.rcWork.into(),
        dpi_x,
        dpi_y,
        primary: info.monitorInfo.dwFlags & MONITORINFOF_PRIMARY != 0,
    })
}

fn device_name_from_utf16(device_name: &[u16]) -> Result<String, DesktopError> {
    let nul = device_name
        .iter()
        .position(|code_unit| *code_unit == 0)
        .unwrap_or(device_name.len());
    if nul == 0 {
        return Err(DesktopError::InvalidMonitorName);
    }
    String::from_utf16(&device_name[..nul]).map_err(|_| DesktopError::InvalidMonitorName)
}

fn display_label_from_device_id(device_id: &str) -> String {
    device_id
        .strip_prefix(r"\\.\DISPLAY")
        .filter(|suffix| {
            !suffix.is_empty()
                && suffix.len() <= MAX_DISPLAY_LABEL_SCALARS - "Display ".len()
                && suffix.chars().all(|character| character.is_ascii_digit())
        })
        .map(|suffix| format!("Display {suffix}"))
        .unwrap_or_else(|| "Display".to_string())
}

fn normalize_monitors(
    raw_monitors: Vec<RawMonitor>,
) -> Result<Vec<MonitorDescriptor>, DesktopError> {
    let mut monitors: Vec<MonitorDescriptor> = Vec::with_capacity(raw_monitors.len());

    for raw_monitor in raw_monitors {
        let descriptor = normalize_monitor(raw_monitor)?;
        if let Some(existing) = monitors
            .iter_mut()
            .find(|monitor| monitor.id == descriptor.id)
        {
            if descriptor.primary && !existing.primary {
                *existing = descriptor;
            }
        } else {
            monitors.push(descriptor);
        }
    }

    Ok(monitors)
}

fn normalize_monitor(raw: RawMonitor) -> Result<MonitorDescriptor, DesktopError> {
    let monitor_rect = checked_work_area(raw.monitor_rect)?.rect();
    let work_rect = checked_work_area(raw.work_rect)?;
    if !monitor_rect.contains_rect(work_rect.rect()) {
        return Err(DesktopError::InvalidMonitorGeometry);
    }
    let name = display_label_from_device_id(&raw.id);
    let scale = MonitorScale::from_effective_dpi(raw.dpi_x, raw.dpi_y)
        .map_err(|_| DesktopError::InvalidMonitorScale)?;
    Ok(MonitorDescriptor {
        id: raw.id,
        name,
        monitor_rect,
        work_rect,
        scale,
        primary: raw.primary,
    })
}

fn checked_work_area(rect: RawRect) -> Result<MonitorWorkArea, DesktopError> {
    let width = u32::try_from(i64::from(rect.right) - i64::from(rect.left))
        .map_err(|_| DesktopError::InvalidMonitorGeometry)?;
    let height = u32::try_from(i64::from(rect.bottom) - i64::from(rect.top))
        .map_err(|_| DesktopError::InvalidMonitorGeometry)?;
    MonitorWorkArea::new(rect.left, rect.top, width, height)
        .map_err(|_| DesktopError::InvalidMonitorGeometry)
}

fn classify_fullscreen(
    foreground_handle: NativeWindowHandle,
    dashy_window_handles: &[NativeWindowHandle],
    foreground_monitor_id: &str,
    selected_monitor_id: &str,
    window_rect: RawRect,
    monitor_rect: RawRect,
) -> bool {
    if dashy_window_handles.contains(&foreground_handle)
        || foreground_monitor_id != selected_monitor_id
        || !window_rect.has_positive_extent()
        || !monitor_rect.has_positive_extent()
    {
        return false;
    }

    (i64::from(window_rect.left) - i64::from(monitor_rect.left)).abs()
        <= FULLSCREEN_EDGE_TOLERANCE_PX
        && (i64::from(window_rect.top) - i64::from(monitor_rect.top)).abs()
            <= FULLSCREEN_EDGE_TOLERANCE_PX
        && (i64::from(window_rect.right) - i64::from(monitor_rect.right)).abs()
            <= FULLSCREEN_EDGE_TOLERANCE_PX
        && (i64::from(window_rect.bottom) - i64::from(monitor_rect.bottom)).abs()
            <= FULLSCREEN_EDGE_TOLERANCE_PX
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use serde_json::Value;

    use super::*;
    use crate::{
        dashboard::models::ProviderId,
        desktop::{
            edge::{window_layout, EdgeUiState},
            settings::{
                AppSettings, EdgePlacement, MonitorPreference, SettingsPatch, SettingsPersistence,
                SettingsService,
            },
        },
    };

    #[derive(Default)]
    struct CapturingPersistence {
        saved: Mutex<Option<AppSettings>>,
    }

    impl SettingsPersistence for CapturingPersistence {
        fn load(&self) -> Result<Option<Value>, String> {
            Ok(None)
        }

        fn save(&self, settings: &AppSettings) -> Result<(), String> {
            *self.saved.lock().unwrap() = Some(settings.clone());
            Ok(())
        }
    }

    #[derive(Default)]
    struct CapturingWindowBoundsApi {
        calls: Mutex<Vec<WindowBoundsCall>>,
    }

    impl WindowBoundsApi for CapturingWindowBoundsApi {
        fn set_window_pos(&self, call: WindowBoundsCall) -> Result<(), DesktopError> {
            self.calls.lock().unwrap().push(call);
            Ok(())
        }
    }

    fn raw_monitor(
        id: &str,
        monitor_rect: RawRect,
        work_rect: RawRect,
        primary: bool,
    ) -> RawMonitor {
        RawMonitor {
            id: id.to_owned(),
            monitor_rect,
            work_rect,
            dpi_x: 96,
            dpi_y: 96,
            primary,
        }
    }

    fn raw_monitor_at_dpi(
        id: &str,
        monitor_rect: RawRect,
        work_rect: RawRect,
        primary: bool,
        dpi_x: u32,
        dpi_y: u32,
    ) -> RawMonitor {
        RawMonitor {
            id: id.to_owned(),
            monitor_rect,
            work_rect,
            dpi_x,
            dpi_y,
            primary,
        }
    }

    #[test]
    fn duplicate_device_ids_collapse_to_the_primary_descriptor() {
        let duplicate = RawRect::new(0, 0, 1920, 1080);
        let monitors = normalize_monitors(vec![
            raw_monitor("\\\\.\\DISPLAY1", duplicate, duplicate, false),
            raw_monitor("\\\\.\\DISPLAY1", duplicate, duplicate, true),
        ])
        .unwrap();

        assert_eq!(monitors.len(), 1);
        assert_eq!(monitors[0].id, "\\\\.\\DISPLAY1");
        assert!(monitors[0].primary);
    }

    #[test]
    fn native_device_id_produces_a_bounded_safe_display_label() {
        assert_eq!(display_label_from_device_id(r"\\.\DISPLAY1"), "Display 1");
        assert_eq!(
            display_label_from_device_id(r"\\.\DISPLAY2048"),
            "Display 2048"
        );

        let fallback = display_label_from_device_id(r"\\.\MONITOR/unsafe");
        assert_eq!(fallback, "Display");
        let oversized = format!(r"\\.\DISPLAY{}", "9".repeat(300));
        assert_eq!(display_label_from_device_id(&oversized), "Display");
        assert!(fallback.chars().count() <= 80);
        assert!(!fallback
            .chars()
            .any(|character| character.is_control() || matches!(character, '/' | '\\')));
    }

    #[test]
    fn normalized_windows_descriptor_can_be_saved_as_a_monitor_preference() {
        let bounds = RawRect::new(0, 0, 1920, 1080);
        let descriptor = normalize_monitors(vec![raw_monitor(
            r"\\.\DISPLAY1",
            bounds,
            RawRect::new(0, 0, 1920, 1040),
            true,
        )])
        .unwrap()
        .remove(0);
        let preference = MonitorPreference::from(&descriptor);
        let persistence = Arc::new(CapturingPersistence::default());
        let service = SettingsService::load(persistence.clone());

        let saved = service
            .update(SettingsPatch {
                monitor: Some(Some(preference.clone())),
                ..Default::default()
            })
            .unwrap();

        assert_eq!(descriptor.id, r"\\.\DISPLAY1");
        assert_eq!(descriptor.name, "Display 1");
        assert_eq!(saved.monitor, Some(preference));
        assert_eq!(persistence.saved.lock().unwrap().as_ref(), Some(&saved));
    }

    #[test]
    fn window_layout_maps_to_one_atomic_nonactivating_bounds_call() {
        let api = CapturingWindowBoundsApi::default();
        let visible = window_layout(
            EdgePlacement::Right,
            MonitorWorkArea::new(-1920, 40, 1920, 1040).unwrap(),
            EdgeUiState::CardVisible,
            Some(ProviderId::Codex),
            3,
        );

        apply_window_bounds_with(&api, 77, &visible).unwrap();

        assert_eq!(
            *api.calls.lock().unwrap(),
            vec![WindowBoundsCall {
                handle: 77,
                x: visible.position.x,
                y: visible.position.y,
                width: i32::try_from(visible.size.width).unwrap(),
                height: i32::try_from(visible.size.height).unwrap(),
                z_order: WindowZOrder::Topmost,
                visibility: WindowVisibility::Show,
                no_activate: true,
            }]
        );

        let api = CapturingWindowBoundsApi::default();
        let hidden = window_layout(
            EdgePlacement::Right,
            MonitorWorkArea::new(0, 0, 1920, 1040).unwrap(),
            EdgeUiState::Hidden,
            None,
            3,
        );
        apply_window_bounds_with(&api, 88, &hidden).unwrap();
        let calls = api.calls.lock().unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].z_order, WindowZOrder::NotTopmost);
        assert_eq!(calls[0].visibility, WindowVisibility::Hide);
        assert!(calls[0].no_activate);
    }

    #[test]
    fn primary_flag_selects_only_the_monitor_reported_primary_by_windows() {
        let left = RawRect::new(-1920, 0, 0, 1080);
        let right = RawRect::new(0, 0, 1920, 1080);
        let monitors = normalize_monitors(vec![
            raw_monitor("\\\\.\\DISPLAY2", left, left, false),
            raw_monitor("\\\\.\\DISPLAY1", right, right, true),
        ])
        .unwrap();

        let primary = monitors.iter().find(|monitor| monitor.primary).unwrap();
        assert_eq!(primary.id, "\\\\.\\DISPLAY1");
        assert_eq!(monitors.iter().filter(|monitor| monitor.primary).count(), 1);
    }

    #[test]
    fn negative_virtual_screen_coordinates_are_preserved() {
        let monitors = normalize_monitors(vec![raw_monitor(
            "\\\\.\\DISPLAY2",
            RawRect::new(-1920, -200, -320, 700),
            RawRect::new(-1920, -160, -320, 660),
            false,
        )])
        .unwrap();

        assert_eq!(
            monitors[0].monitor_rect,
            Rect {
                x: -1920,
                y: -200,
                width: 1600,
                height: 900,
            }
        );
        assert_eq!(monitors[0].work_rect.x(), -1920);
        assert_eq!(monitors[0].work_rect.y(), -160);
        assert_eq!(monitors[0].work_rect.width(), 1600);
        assert_eq!(monitors[0].work_rect.height(), 820);
        assert_eq!(monitors[0].scale, MonitorScale::ONE);
    }

    #[test]
    fn windows_effective_dpi_is_validated_and_carried_into_the_descriptor() {
        let bounds = RawRect::new(0, 0, 2560, 1440);
        for (dpi, expected_factor) in [(96, 1.0), (120, 1.25), (144, 1.5), (192, 2.0)] {
            let descriptor = normalize_monitor(raw_monitor_at_dpi(
                r"\\.\DISPLAY1",
                bounds,
                bounds,
                true,
                dpi,
                dpi,
            ))
            .unwrap();

            assert_eq!(descriptor.scale.effective_dpi(), dpi);
            assert_eq!(
                descriptor.scale,
                MonitorScale::try_from_scale_factor(expected_factor).unwrap()
            );
        }

        for (dpi_x, dpi_y) in [(0, 0), (120, 144), (47, 47), (769, 769)] {
            assert_eq!(
                normalize_monitor(raw_monitor_at_dpi(
                    r"\\.\DISPLAY1",
                    bounds,
                    bounds,
                    true,
                    dpi_x,
                    dpi_y,
                )),
                Err(DesktopError::InvalidMonitorScale)
            );
        }
    }

    #[test]
    fn dpi_only_descriptor_changes_are_visible_to_topology_equality() {
        let bounds = RawRect::new(0, 0, 1920, 1080);
        let at_100 = normalize_monitor(raw_monitor_at_dpi(
            r"\\.\DISPLAY1",
            bounds,
            bounds,
            true,
            96,
            96,
        ))
        .unwrap();
        let at_150 = normalize_monitor(raw_monitor_at_dpi(
            r"\\.\DISPLAY1",
            bounds,
            bounds,
            true,
            144,
            144,
        ))
        .unwrap();

        assert_ne!(at_100, at_150);
    }

    #[test]
    fn invalid_native_geometry_is_a_recoverable_error() {
        let error = normalize_monitors(vec![raw_monitor(
            "\\\\.\\DISPLAY1",
            RawRect::new(0, 0, 0, 1080),
            RawRect::new(0, 0, 1920, 1040),
            true,
        )])
        .unwrap_err();

        assert_eq!(error, DesktopError::InvalidMonitorGeometry);
    }

    #[test]
    fn work_area_outside_the_monitor_is_a_recoverable_error() {
        let error = normalize_monitors(vec![raw_monitor(
            "\\\\.\\DISPLAY1",
            RawRect::new(0, 0, 1920, 1080),
            RawRect::new(0, 0, 1920, 1081),
            true,
        )])
        .unwrap_err();

        assert_eq!(error, DesktopError::InvalidMonitorGeometry);
    }

    #[test]
    fn exact_monitor_coverage_is_fullscreen() {
        assert!(classify_fullscreen(
            41,
            &[],
            "\\\\.\\DISPLAY1",
            "\\\\.\\DISPLAY1",
            RawRect::new(0, 0, 1920, 1080),
            RawRect::new(0, 0, 1920, 1080),
        ));
    }

    #[test]
    fn two_pixel_inset_on_every_edge_is_tolerated() {
        assert!(classify_fullscreen(
            41,
            &[],
            "\\\\.\\DISPLAY1",
            "\\\\.\\DISPLAY1",
            RawRect::new(2, 2, 1918, 1078),
            RawRect::new(0, 0, 1920, 1080),
        ));
    }

    #[test]
    fn a_three_pixel_gap_is_not_fullscreen() {
        assert!(!classify_fullscreen(
            41,
            &[],
            "\\\\.\\DISPLAY1",
            "\\\\.\\DISPLAY1",
            RawRect::new(0, 0, 1920, 1077),
            RawRect::new(0, 0, 1920, 1080),
        ));
    }

    #[test]
    fn a_window_extending_far_beyond_a_monitor_is_not_fullscreen() {
        assert!(!classify_fullscreen(
            41,
            &[],
            "\\\\.\\DISPLAY1",
            "\\\\.\\DISPLAY1",
            RawRect::new(-100, 0, 1920, 1080),
            RawRect::new(0, 0, 1920, 1080),
        ));
    }

    #[test]
    fn maximized_to_work_area_does_not_count_as_fullscreen() {
        assert!(!classify_fullscreen(
            41,
            &[],
            "\\\\.\\DISPLAY1",
            "\\\\.\\DISPLAY1",
            RawRect::new(0, 0, 1920, 1040),
            RawRect::new(0, 0, 1920, 1080),
        ));
    }

    #[test]
    fn fullscreen_on_another_monitor_does_not_suppress_the_selected_monitor() {
        assert!(!classify_fullscreen(
            41,
            &[],
            "\\\\.\\DISPLAY2",
            "\\\\.\\DISPLAY1",
            RawRect::new(-1920, 0, 0, 1080),
            RawRect::new(0, 0, 1920, 1080),
        ));
    }

    #[test]
    fn dashys_own_hwnd_never_suppresses_itself() {
        assert!(!classify_fullscreen(
            41,
            &[17, 41],
            "\\\\.\\DISPLAY1",
            "\\\\.\\DISPLAY1",
            RawRect::new(0, 0, 1920, 1080),
            RawRect::new(0, 0, 1920, 1080),
        ));
    }

    #[test]
    fn utf16_device_name_stops_at_nul_and_preserves_unicode() {
        let mut device_name = [0_u16; 32];
        let encoded: Vec<u16> = "\\\\.\\DISPLAY一".encode_utf16().collect();
        device_name[..encoded.len()].copy_from_slice(&encoded);
        device_name[encoded.len() + 1] = b'X' as u16;

        assert_eq!(
            device_name_from_utf16(&device_name).unwrap(),
            "\\\\.\\DISPLAY一"
        );
    }

    #[test]
    fn invalid_utf16_device_name_is_a_bounded_error() {
        let mut device_name = [0_u16; 32];
        device_name[0] = 0xD800;

        assert_eq!(
            device_name_from_utf16(&device_name),
            Err(DesktopError::InvalidMonitorName)
        );
    }
}
