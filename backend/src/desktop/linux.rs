//! Linux (and other Unix) desktop backend.
//!
//! GDK is not thread-safe, so every query runs on the GTK main thread through
//! Tauri's `run_on_main_thread` and publishes a snapshot the controller tick
//! reads. One snapshot per tick carries the monitors, the pointer, and the
//! frame of the active window, so the three probe calls in a tick agree with
//! each other.
//!
//! Wayland compositors do not expose the global pointer position or the active
//! window of other clients; there the snapshot degrades to "no cursor, no
//! fullscreen", and the tray's Show Dashy action remains the way in.

use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Condvar, Mutex,
    },
    time::Duration,
};

use gdk::{
    glib::translate::{from_glib_full, ToGlibPtr},
    prelude::*,
};
use tauri::AppHandle;

use crate::desktop::{
    edge::Point,
    fullscreen::{window_covers_monitor, RawRect},
    platform::{
        cursor_point, normalize_generic_monitors, DesktopError, GenericRawMonitor,
        MonitorDescriptor,
    },
};

/// How long a tick waits for the main thread to answer before reusing the last
/// snapshot. Shorter than the tick interval so a busy main thread cannot stall
/// the controller.
const SNAPSHOT_WAIT: Duration = Duration::from_millis(30);
/// The first snapshot has nothing to fall back on, so the initial wait is longer.
const INITIAL_SNAPSHOT_WAIT: Duration = Duration::from_millis(500);
const NET_WM_PID: &str = "_NET_WM_PID";

#[derive(Clone, Debug, PartialEq)]
pub(super) struct DesktopSnapshot {
    monitors: Result<Vec<MonitorDescriptor>, DesktopError>,
    cursor: Option<Point>,
    /// Physical frame of the active window when it belongs to another process.
    active_window: Option<RawRect>,
}

#[derive(Default)]
struct SnapshotState {
    latest: Mutex<(u64, Option<DesktopSnapshot>)>,
    published: Condvar,
    in_flight: AtomicBool,
}

impl SnapshotState {
    fn publish(&self, snapshot: DesktopSnapshot) {
        let mut latest = self.latest.lock().expect("desktop snapshot lock poisoned");
        latest.0 = latest.0.wrapping_add(1);
        latest.1 = Some(snapshot);
        self.in_flight.store(false, Ordering::Release);
        self.published.notify_all();
    }

    fn wait_for_newer(&self, seen: u64, wait: Duration) -> Option<DesktopSnapshot> {
        let latest = self.latest.lock().expect("desktop snapshot lock poisoned");
        let (latest, _) = self
            .published
            .wait_timeout_while(latest, wait, |state| state.0 == seen)
            .expect("desktop snapshot lock poisoned");
        latest.1.clone()
    }

    fn generation(&self) -> u64 {
        self.latest
            .lock()
            .expect("desktop snapshot lock poisoned")
            .0
    }

    fn current(&self) -> Option<DesktopSnapshot> {
        self.latest
            .lock()
            .expect("desktop snapshot lock poisoned")
            .1
            .clone()
    }
}

pub(super) struct LinuxDesktopProbe {
    app: AppHandle,
    state: Arc<SnapshotState>,
}

impl LinuxDesktopProbe {
    pub(super) fn new(app: AppHandle) -> Self {
        Self {
            app,
            state: Arc::new(SnapshotState::default()),
        }
    }

    /// Requests a fresh snapshot and returns the newest one available.
    ///
    /// Only one query is queued at a time; if the previous one has not run yet
    /// the tick reuses the last snapshot rather than piling closures onto the
    /// main thread.
    fn refresh(&self) -> Option<DesktopSnapshot> {
        let seen = self.state.generation();
        let first = self.state.current().is_none();
        if !self.state.in_flight.swap(true, Ordering::AcqRel) {
            let state = self.state.clone();
            if self
                .app
                .run_on_main_thread(move || state.publish(capture_snapshot()))
                .is_err()
            {
                self.state.in_flight.store(false, Ordering::Release);
                return self.state.current();
            }
        }
        let wait = if first {
            INITIAL_SNAPSHOT_WAIT
        } else {
            SNAPSHOT_WAIT
        };
        self.state.wait_for_newer(seen, wait)
    }

    pub(super) fn monitors(&self) -> Result<Vec<MonitorDescriptor>, DesktopError> {
        match self.refresh() {
            Some(snapshot) => snapshot.monitors,
            None => Err(DesktopError::MonitorEnumerationFailed),
        }
    }

    pub(super) fn cursor_position(&self) -> Result<Option<Point>, DesktopError> {
        Ok(self.state.current().and_then(|snapshot| snapshot.cursor))
    }

    pub(super) fn foreground_is_fullscreen(&self, selected_monitor: &MonitorDescriptor) -> bool {
        let Some(active) = self
            .state
            .current()
            .and_then(|snapshot| snapshot.active_window)
        else {
            return false;
        };
        classify_active_window(active, selected_monitor)
    }
}

pub(super) fn classify_active_window(
    active_window: RawRect,
    selected_monitor: &MonitorDescriptor,
) -> bool {
    let Some(monitor) = RawRect::from_rect(selected_monitor.monitor_rect) else {
        return false;
    };
    active_window.intersects(monitor) && window_covers_monitor(active_window, monitor)
}

/// Runs on the GTK main thread.
fn capture_snapshot() -> DesktopSnapshot {
    let Some(display) = gdk::Display::default() else {
        return DesktopSnapshot {
            monitors: Err(DesktopError::MonitorEnumerationFailed),
            cursor: None,
            active_window: None,
        };
    };
    let monitors = normalize_generic_monitors(raw_monitors(&display));
    let cursor = pointer_position(&display);
    let active_window = active_window_frame(&display);
    DesktopSnapshot {
        monitors,
        cursor,
        active_window,
    }
}

fn raw_monitors(display: &gdk::Display) -> Vec<GenericRawMonitor> {
    (0..display.n_monitors())
        .filter_map(|index| display.monitor(index))
        .filter_map(|monitor| {
            let scale = monitor.scale_factor().max(1);
            let geometry = physical_rect(monitor.geometry(), scale)?;
            let work_area = physical_rect(monitor.workarea(), scale).unwrap_or(geometry);
            Some(GenericRawMonitor {
                name: monitor.model().map(|model| model.to_string()),
                x: geometry.0,
                y: geometry.1,
                width: geometry.2,
                height: geometry.3,
                work_x: work_area.0,
                work_y: work_area.1,
                work_width: work_area.2,
                work_height: work_area.3,
                scale_factor: f64::from(scale),
                primary: monitor.is_primary(),
            })
        })
        .collect()
}

/// GDK reports logical pixels; Dashy positions windows in physical pixels.
fn physical_rect(rectangle: gdk::Rectangle, scale: i32) -> Option<(i32, i32, u32, u32)> {
    let x = rectangle.x().checked_mul(scale)?;
    let y = rectangle.y().checked_mul(scale)?;
    let width = u32::try_from(rectangle.width().checked_mul(scale)?).ok()?;
    let height = u32::try_from(rectangle.height().checked_mul(scale)?).ok()?;
    Some((x, y, width, height))
}

fn pointer_position(display: &gdk::Display) -> Option<Point> {
    let pointer = display.default_seat()?.pointer()?;
    let (_, x, y) = pointer.position();
    let scale = display
        .monitor_at_point(x, y)
        .map(|monitor| monitor.scale_factor())
        .unwrap_or(1)
        .max(1);
    cursor_point(
        f64::from(x) * f64::from(scale),
        f64::from(y) * f64::from(scale),
    )
}

/// The active window's physical frame, unless the window is Dashy's own or the
/// windowing system does not expose one (Wayland, no EWMH window manager).
fn active_window_frame(display: &gdk::Display) -> Option<RawRect> {
    let window = active_window(&display.default_screen())?;
    if window_owner_pid(&window) == Some(std::process::id()) {
        return None;
    }
    let scale = window.scale_factor().max(1);
    let (x, y, width, height) = physical_rect(window.frame_extents(), scale)?;
    Some(RawRect::new(
        x,
        y,
        x.checked_add_unsigned(width)?,
        y.checked_add_unsigned(height)?,
    ))
}

/// `_NET_ACTIVE_WINDOW` through GDK, which wraps the X round trip in an error
/// trap so a window that closes mid-query cannot abort the process. The Rust
/// bindings omit this deprecated function, so it is called through the FFI.
fn active_window(screen: &gdk::Screen) -> Option<gdk::Window> {
    // SAFETY: `screen` is a live GdkScreen owned by the default display, the call
    // happens on the GTK main thread, and gdk_screen_get_active_window returns a
    // new reference (or NULL) that `from_glib_full` adopts.
    unsafe {
        from_glib_full(gdk::ffi::gdk_screen_get_active_window(
            screen.to_glib_none().0,
        ))
    }
}

fn window_owner_pid(window: &gdk::Window) -> Option<u32> {
    let property = gdk::Atom::intern(NET_WM_PID);
    let cardinal = gdk::Atom::intern("CARDINAL");
    let (_, format, data) = gdk::property_get(window, &property, &cardinal, 0, 1, 0)?;
    if format != 32 {
        return None;
    }
    // GDK hands 32-bit properties back as native longs, which are 8 bytes on
    // 64-bit X11 clients and 4 bytes elsewhere.
    match data.len() {
        8 => u32::try_from(u64::from_ne_bytes(data[..8].try_into().ok()?)).ok(),
        4 => Some(u32::from_ne_bytes(data[..4].try_into().ok()?)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::desktop::edge::{MonitorScale, MonitorWorkArea, Rect};

    fn monitor(x: i32, y: i32, width: u32, height: u32) -> MonitorDescriptor {
        MonitorDescriptor {
            id: "eDP-1".into(),
            name: "eDP-1".into(),
            monitor_rect: Rect {
                x,
                y,
                width,
                height,
            },
            work_rect: MonitorWorkArea::new(x, y + 32, width, height - 32).unwrap(),
            scale: MonitorScale::ONE,
            primary: true,
        }
    }

    #[test]
    fn an_active_window_covering_the_monitor_is_fullscreen() {
        assert!(classify_active_window(
            RawRect::new(0, 0, 1920, 1080),
            &monitor(0, 0, 1920, 1080)
        ));
        assert!(!classify_active_window(
            RawRect::new(0, 32, 1920, 1080),
            &monitor(0, 0, 1920, 1080)
        ));
    }

    #[test]
    fn an_active_window_on_another_monitor_does_not_suppress_this_one() {
        assert!(!classify_active_window(
            RawRect::new(1920, 0, 3840, 1080),
            &monitor(0, 0, 1920, 1080)
        ));
    }

    #[test]
    fn logical_gdk_geometry_scales_to_physical_pixels() {
        assert_eq!(
            physical_rect(gdk::Rectangle::new(-960, 20, 960, 540), 2),
            Some((-1920, 40, 1920, 1080))
        );
        assert_eq!(physical_rect(gdk::Rectangle::new(0, 0, -1, 10), 1), None);
    }

    #[test]
    fn snapshot_state_hands_out_newer_generations_and_falls_back_to_the_latest() {
        let state = SnapshotState::default();
        assert_eq!(state.wait_for_newer(0, Duration::from_millis(1)), None);

        let snapshot = DesktopSnapshot {
            monitors: Ok(vec![monitor(0, 0, 1920, 1080)]),
            cursor: Some(Point { x: 5, y: 6 }),
            active_window: None,
        };
        state.in_flight.store(true, Ordering::Release);
        state.publish(snapshot.clone());

        assert!(!state.in_flight.load(Ordering::Acquire));
        assert_eq!(state.generation(), 1);
        assert_eq!(
            state.wait_for_newer(0, Duration::from_millis(1)),
            Some(snapshot.clone())
        );
        // A wait for a generation that never arrives still returns the latest snapshot.
        assert_eq!(
            state.wait_for_newer(1, Duration::from_millis(1)),
            Some(snapshot)
        );
    }
}
