//! Platform-neutral "does the front window cover the selected monitor" rule.
//!
//! Every desktop backend (Win32, CoreGraphics, GDK) resolves its own notion of
//! the front window and the monitor it sits on, then asks this module whether
//! that window's frame covers the monitor closely enough to count as a
//! fullscreen application. Keeping the rule here means all three platforms
//! suppress Dashy for the same geometry.

use crate::desktop::edge::Rect;

/// Window managers and compositors routinely report fullscreen frames a pixel
/// or two inside the monitor, so exact equality would never match.
pub(super) const FULLSCREEN_EDGE_TOLERANCE_PX: i64 = 2;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct RawRect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl RawRect {
    pub const fn new(left: i32, top: i32, right: i32, bottom: i32) -> Self {
        Self {
            left,
            top,
            right,
            bottom,
        }
    }

    pub fn from_rect(rect: Rect) -> Option<Self> {
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

    pub fn has_positive_extent(self) -> bool {
        self.right > self.left && self.bottom > self.top
    }

    /// Used by the backends that must first find the front window on the
    /// selected monitor; Win32 asks the OS which monitor a window belongs to.
    #[cfg(not(windows))]
    pub fn intersects(self, other: Self) -> bool {
        self.has_positive_extent()
            && other.has_positive_extent()
            && i64::from(self.left) < i64::from(other.right)
            && i64::from(self.right) > i64::from(other.left)
            && i64::from(self.top) < i64::from(other.bottom)
            && i64::from(self.bottom) > i64::from(other.top)
    }
}

/// True when `window` matches `monitor` on every edge within the tolerance.
///
/// A window that extends past the monitor is not fullscreen either: it is a
/// window spanning several displays, and Dashy must keep working on this one.
pub(super) fn window_covers_monitor(window: RawRect, monitor: RawRect) -> bool {
    if !window.has_positive_extent() || !monitor.has_positive_extent() {
        return false;
    }
    let within = |window_edge: i32, monitor_edge: i32| {
        (i64::from(window_edge) - i64::from(monitor_edge)).abs() <= FULLSCREEN_EDGE_TOLERANCE_PX
    };
    within(window.left, monitor.left)
        && within(window.top, monitor.top)
        && within(window.right, monitor.right)
        && within(window.bottom, monitor.bottom)
}

#[cfg(test)]
mod tests {
    use super::*;

    const MONITOR: RawRect = RawRect::new(0, 0, 1920, 1080);

    #[test]
    fn exact_monitor_coverage_is_fullscreen() {
        assert!(window_covers_monitor(
            RawRect::new(0, 0, 1920, 1080),
            MONITOR
        ));
    }

    #[test]
    fn two_pixel_inset_on_every_edge_is_tolerated() {
        assert!(window_covers_monitor(
            RawRect::new(2, 2, 1918, 1078),
            MONITOR
        ));
    }

    #[test]
    fn a_three_pixel_gap_is_not_fullscreen() {
        assert!(!window_covers_monitor(
            RawRect::new(0, 0, 1920, 1077),
            MONITOR
        ));
    }

    #[test]
    fn a_window_extending_far_beyond_a_monitor_is_not_fullscreen() {
        assert!(!window_covers_monitor(
            RawRect::new(-100, 0, 1920, 1080),
            MONITOR
        ));
    }

    #[test]
    fn maximized_to_work_area_does_not_count_as_fullscreen() {
        assert!(!window_covers_monitor(
            RawRect::new(0, 0, 1920, 1040),
            MONITOR
        ));
    }

    #[test]
    fn empty_or_inverted_rectangles_never_count_as_fullscreen() {
        assert!(!window_covers_monitor(RawRect::new(0, 0, 0, 0), MONITOR));
        assert!(!window_covers_monitor(MONITOR, RawRect::new(10, 10, 5, 5)));
    }

    #[test]
    fn negative_virtual_screen_coordinates_are_compared_edge_by_edge() {
        let monitor = RawRect::new(-1920, -200, -320, 700);
        assert!(window_covers_monitor(
            RawRect::new(-1919, -199, -321, 699),
            monitor
        ));
        assert!(!window_covers_monitor(
            RawRect::new(0, 0, 1600, 900),
            monitor
        ));
    }

    #[test]
    fn rect_conversion_rejects_empty_and_overflowing_geometry() {
        assert_eq!(
            RawRect::from_rect(Rect {
                x: 0,
                y: 0,
                width: 0,
                height: 10
            }),
            None
        );
        assert_eq!(
            RawRect::from_rect(Rect {
                x: i32::MAX,
                y: 0,
                width: 1,
                height: 10
            }),
            None
        );
        assert_eq!(
            RawRect::from_rect(Rect {
                x: -10,
                y: 5,
                width: 20,
                height: 30
            }),
            Some(RawRect::new(-10, 5, 10, 35))
        );
    }

    #[cfg(not(windows))]
    #[test]
    fn intersection_requires_overlap_in_both_axes() {
        assert!(MONITOR.intersects(RawRect::new(1900, 1000, 2500, 1500)));
        assert!(!MONITOR.intersects(RawRect::new(1920, 0, 3840, 1080)));
        assert!(!MONITOR.intersects(RawRect::new(0, 1080, 1920, 2160)));
    }
}
