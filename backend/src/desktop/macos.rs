//! macOS desktop backend.
//!
//! Monitors and the cursor come from Tauri (AppKit under the hood). Fullscreen
//! detection walks the on-screen window list from CoreGraphics: the frontmost
//! normal-layer window of another process that sits on the selected display is
//! compared against that display's bounds, mirroring the Win32 foreground check.

use core_foundation::{
    array::CFArray,
    base::{CFType, TCFType},
    dictionary::CFDictionary,
    number::CFNumber,
    string::CFString,
};
use core_graphics::{
    geometry::CGRect,
    window::{
        kCGNullWindowID, kCGWindowAlpha, kCGWindowBounds, kCGWindowLayer,
        kCGWindowListExcludeDesktopElements, kCGWindowListOptionOnScreenOnly, kCGWindowOwnerPID,
        CGWindowListCopyWindowInfo,
    },
};
use objc2::{msg_send, runtime::AnyObject};
use tauri::{AppHandle, Manager, Runtime};

use crate::desktop::platform::{
    normalize_generic_monitors, DesktopError, GenericRawMonitor, MonitorDescriptor,
};

/// Fullscreen frames are compared in points, where a window can sit a point or
/// two inside the display; the Win32 backend tolerates the same two pixels.
const FULLSCREEN_EDGE_TOLERANCE_POINTS: f64 = 2.0;
const BASE_DPI: f64 = 96.0;

// NSWindowCollectionBehavior flags (AppKit/NSWindow.h).
const NS_WINDOW_COLLECTION_BEHAVIOR_CAN_JOIN_ALL_SPACES: usize = 1 << 0;
const NS_WINDOW_COLLECTION_BEHAVIOR_STATIONARY: usize = 1 << 4;
const NS_WINDOW_COLLECTION_BEHAVIOR_FULL_SCREEN_AUXILIARY: usize = 1 << 8;

/// A rectangle in global CoreGraphics coordinates (points, origin at the top-left
/// of the primary display, y growing downwards).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct PointRect {
    left: f64,
    top: f64,
    right: f64,
    bottom: f64,
}

impl PointRect {
    pub(super) const fn new(left: f64, top: f64, right: f64, bottom: f64) -> Self {
        Self {
            left,
            top,
            right,
            bottom,
        }
    }

    fn has_positive_extent(self) -> bool {
        self.right > self.left && self.bottom > self.top
    }

    fn intersects(self, other: Self) -> bool {
        self.has_positive_extent()
            && other.has_positive_extent()
            && self.left < other.right
            && self.right > other.left
            && self.top < other.bottom
            && self.bottom > other.top
    }

    fn covers(self, monitor: Self) -> bool {
        self.has_positive_extent()
            && monitor.has_positive_extent()
            && (self.left - monitor.left).abs() <= FULLSCREEN_EDGE_TOLERANCE_POINTS
            && (self.top - monitor.top).abs() <= FULLSCREEN_EDGE_TOLERANCE_POINTS
            && (self.right - monitor.right).abs() <= FULLSCREEN_EDGE_TOLERANCE_POINTS
            && (self.bottom - monitor.bottom).abs() <= FULLSCREEN_EDGE_TOLERANCE_POINTS
    }
}

/// The subset of a CoreGraphics window description that the fullscreen rule reads.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct OnScreenWindow {
    pub owner_pid: i64,
    pub layer: i64,
    pub alpha: f64,
    pub bounds: PointRect,
}

pub(super) fn monitors<R: Runtime>(
    app: &AppHandle<R>,
) -> Result<Vec<MonitorDescriptor>, DesktopError> {
    let available = app
        .available_monitors()
        .map_err(|_| DesktopError::MonitorEnumerationFailed)?;
    let primary = app
        .primary_monitor()
        .map_err(|_| DesktopError::MonitorQueryFailed)?;
    let raw = available
        .iter()
        .map(|monitor| {
            let position = monitor.position();
            let size = monitor.size();
            let work_area = monitor.work_area();
            GenericRawMonitor {
                name: monitor.name().cloned(),
                x: position.x,
                y: position.y,
                width: size.width,
                height: size.height,
                work_x: work_area.position.x,
                work_y: work_area.position.y,
                work_width: work_area.size.width,
                work_height: work_area.size.height,
                scale_factor: monitor.scale_factor(),
                primary: primary.as_ref().is_some_and(|primary| {
                    primary.name() == monitor.name()
                        && primary.position() == position
                        && primary.size() == size
                }),
            }
        })
        .collect();
    normalize_generic_monitors(raw)
}

pub(super) fn foreground_is_fullscreen(selected_monitor: &MonitorDescriptor) -> bool {
    let Some(monitor) = monitor_rect_in_points(selected_monitor) else {
        return false;
    };
    let windows = on_screen_windows();
    classify_front_window(&windows, i64::from(std::process::id()), monitor)
}

/// Converts the descriptor's physical-pixel rectangle back into CoreGraphics points.
fn monitor_rect_in_points(monitor: &MonitorDescriptor) -> Option<PointRect> {
    let scale = f64::from(monitor.scale.effective_dpi()) / BASE_DPI;
    if !scale.is_finite() || scale <= 0.0 {
        return None;
    }
    let rect = monitor.monitor_rect;
    if rect.width == 0 || rect.height == 0 {
        return None;
    }
    let left = f64::from(rect.x) / scale;
    let top = f64::from(rect.y) / scale;
    Some(PointRect::new(
        left,
        top,
        left + f64::from(rect.width) / scale,
        top + f64::from(rect.height) / scale,
    ))
}

/// Finds the frontmost normal-layer window of another process on the selected
/// display and reports whether it covers that display. Windows come front to
/// back, so the first intersecting candidate is the one the user sees on top.
pub(super) fn classify_front_window(
    windows: &[OnScreenWindow],
    own_pid: i64,
    monitor: PointRect,
) -> bool {
    windows
        .iter()
        .filter(|window| window.layer == 0 && window.alpha > 0.0 && window.owner_pid != own_pid)
        .find(|window| window.bounds.intersects(monitor))
        .is_some_and(|front| front.bounds.covers(monitor))
}

fn on_screen_windows() -> Vec<OnScreenWindow> {
    // SAFETY: CGWindowListCopyWindowInfo takes plain option flags and returns a
    // +1 CFArray (or NULL); the array is adopted under the create rule and
    // released when dropped. No pointer is retained beyond this function.
    let array = unsafe {
        let raw = CGWindowListCopyWindowInfo(
            kCGWindowListOptionOnScreenOnly | kCGWindowListExcludeDesktopElements,
            kCGNullWindowID,
        );
        if raw.is_null() {
            return Vec::new();
        }
        CFArray::<CFDictionary<CFString, CFType>>::wrap_under_create_rule(raw)
    };
    array
        .iter()
        .filter_map(|description| on_screen_window(&description))
        .collect()
}

fn on_screen_window(description: &CFDictionary<CFString, CFType>) -> Option<OnScreenWindow> {
    // SAFETY: the kCGWindow* symbols are immortal CFString constants exported by
    // CoreGraphics; wrapping them under the get rule only retains them.
    let (layer_key, pid_key, alpha_key, bounds_key) = unsafe {
        (
            CFString::wrap_under_get_rule(kCGWindowLayer),
            CFString::wrap_under_get_rule(kCGWindowOwnerPID),
            CFString::wrap_under_get_rule(kCGWindowAlpha),
            CFString::wrap_under_get_rule(kCGWindowBounds),
        )
    };
    let layer = description
        .find(&layer_key)
        .and_then(|value| value.downcast::<CFNumber>())
        .and_then(|number| number.to_i64())?;
    let owner_pid = description
        .find(&pid_key)
        .and_then(|value| value.downcast::<CFNumber>())
        .and_then(|number| number.to_i64())?;
    let alpha = description
        .find(&alpha_key)
        .and_then(|value| value.downcast::<CFNumber>())
        .and_then(|number| number.to_f64())
        .unwrap_or(1.0);
    let bounds = description
        .find(&bounds_key)
        .and_then(|value| value.downcast::<CFDictionary>())
        .and_then(|dictionary| CGRect::from_dict_representation(&dictionary))?;
    if !(bounds.origin.x.is_finite()
        && bounds.origin.y.is_finite()
        && bounds.size.width.is_finite()
        && bounds.size.height.is_finite())
    {
        return None;
    }
    Some(OnScreenWindow {
        owner_pid,
        layer,
        alpha,
        bounds: PointRect::new(
            bounds.origin.x,
            bounds.origin.y,
            bounds.origin.x + bounds.size.width,
            bounds.origin.y + bounds.size.height,
        ),
    })
}

/// Makes the notch follow the user across Spaces and lets it appear beside
/// fullscreen applications when the user opts into that. Must run on the main
/// thread (Tauri's setup hook does).
pub fn configure_main_window(window: &tauri::WebviewWindow) -> Result<(), String> {
    let ns_window = window
        .ns_window()
        .map_err(|error| format!("main window handle is unavailable: {error}"))?;
    if ns_window.is_null() {
        return Err("main window handle is unavailable".to_owned());
    }
    let ns_window: *mut AnyObject = ns_window.cast();
    // SAFETY: `ns_window` is the live NSWindow Tauri owns for the main window and
    // this runs on the AppKit main thread during setup. collectionBehavior and
    // setCollectionBehavior: take and return an NSUInteger bitmask.
    unsafe {
        let behavior: usize = msg_send![ns_window, collectionBehavior];
        let behavior = behavior
            | NS_WINDOW_COLLECTION_BEHAVIOR_CAN_JOIN_ALL_SPACES
            | NS_WINDOW_COLLECTION_BEHAVIOR_STATIONARY
            | NS_WINDOW_COLLECTION_BEHAVIOR_FULL_SCREEN_AUXILIARY;
        let () = msg_send![ns_window, setCollectionBehavior: behavior];
    }
    Ok(())
}

pub fn main_window<R: Runtime>(app: &AppHandle<R>) -> Option<tauri::WebviewWindow<R>> {
    app.get_webview_window("main")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::desktop::edge::{MonitorScale, MonitorWorkArea, Rect};

    const MONITOR: PointRect = PointRect::new(0.0, 0.0, 1728.0, 1117.0);

    fn window(owner_pid: i64, layer: i64, bounds: PointRect) -> OnScreenWindow {
        OnScreenWindow {
            owner_pid,
            layer,
            alpha: 1.0,
            bounds,
        }
    }

    #[test]
    fn the_frontmost_normal_window_covering_the_display_is_fullscreen() {
        let windows = [
            window(500, 25, PointRect::new(0.0, 0.0, 1728.0, 24.0)),
            window(600, 0, PointRect::new(0.0, 0.0, 1728.0, 1117.0)),
            window(700, 0, PointRect::new(100.0, 100.0, 900.0, 700.0)),
        ];
        assert!(classify_front_window(&windows, 42, MONITOR));
    }

    #[test]
    fn a_smaller_front_window_hides_a_fullscreen_window_behind_it() {
        let windows = [
            window(700, 0, PointRect::new(100.0, 100.0, 900.0, 700.0)),
            window(600, 0, PointRect::new(0.0, 0.0, 1728.0, 1117.0)),
        ];
        assert!(!classify_front_window(&windows, 42, MONITOR));
    }

    #[test]
    fn dashys_own_windows_and_invisible_windows_are_skipped() {
        let own = window(42, 0, PointRect::new(0.0, 0.0, 1728.0, 1117.0));
        let mut ghost = window(900, 0, PointRect::new(0.0, 0.0, 1728.0, 1117.0));
        ghost.alpha = 0.0;
        assert!(!classify_front_window(&[own, ghost], 42, MONITOR));

        let behind = window(600, 0, PointRect::new(0.0, 0.0, 1728.0, 1117.0));
        assert!(classify_front_window(&[own, ghost, behind], 42, MONITOR));
    }

    #[test]
    fn fullscreen_on_another_display_does_not_suppress_the_selected_one() {
        let other_display = window(600, 0, PointRect::new(1728.0, 0.0, 4288.0, 1440.0));
        assert!(!classify_front_window(&[other_display], 42, MONITOR));
    }

    #[test]
    fn two_point_inset_is_tolerated_but_a_menu_bar_gap_is_not() {
        let inset = window(600, 0, PointRect::new(2.0, 2.0, 1726.0, 1115.0));
        assert!(classify_front_window(&[inset], 42, MONITOR));

        let below_menu_bar = window(600, 0, PointRect::new(0.0, 25.0, 1728.0, 1117.0));
        assert!(!classify_front_window(&[below_menu_bar], 42, MONITOR));
    }

    #[test]
    fn physical_descriptor_geometry_converts_back_to_points() {
        let descriptor = MonitorDescriptor {
            id: "Built-in Retina Display".into(),
            name: "Built-in Retina Display".into(),
            monitor_rect: Rect {
                x: 0,
                y: 0,
                width: 3456,
                height: 2234,
            },
            work_rect: MonitorWorkArea::new(0, 74, 3456, 2160).unwrap(),
            scale: MonitorScale::try_from_scale_factor(2.0).unwrap(),
            primary: true,
        };
        assert_eq!(monitor_rect_in_points(&descriptor), Some(MONITOR));
    }
}
