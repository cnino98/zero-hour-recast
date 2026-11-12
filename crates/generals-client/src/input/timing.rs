//! Platform click recognition; selection semantics remain in `matching`.
use bevy::prelude::Vec2;
use std::time::{Duration, Instant};

#[cfg(any(target_os = "macos", target_os = "linux"))]
use std::process::Command;

pub fn system_interval() -> Option<Duration> {
    let interval = native_interval();
    if interval.is_none() {
        eprintln!(
            "System double-click timing unavailable; double-click selection and group double-tap are disabled."
        );
    }
    interval
}

#[cfg(target_os = "macos")]
fn native_interval() -> Option<Duration> {
    // GlobalData obtains GetDoubleClickTime from the OS; AppKit exposes its
    // corresponding interval without modifying machine preferences.
    let output = Command::new("/usr/bin/osascript")
        .args([
            "-l",
            "JavaScript",
            "-e",
            "ObjC.import(\"AppKit\"); $.NSEvent.doubleClickInterval",
        ])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let seconds = std::str::from_utf8(&output.stdout)
        .ok()?
        .trim()
        .parse::<f64>()
        .ok()?;
    if !seconds.is_finite() || seconds <= 0.0 {
        return None;
    }
    Duration::try_from_secs_f64(seconds).ok()
}

#[cfg(target_os = "windows")]
fn native_interval() -> Option<Duration> {
    use winreg::{RegKey, enums::HKEY_CURRENT_USER};
    let mouse = RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey("Control Panel\\Mouse")
        .ok()?;
    let millis: String = mouse.get_value("DoubleClickSpeed").ok()?;
    let millis = millis.parse::<u64>().ok()?;
    (millis > 0).then_some(Duration::from_millis(millis))
}

#[cfg(target_os = "linux")]
fn native_interval() -> Option<Duration> {
    let desktop = std::env::var("XDG_CURRENT_DESKTOP")
        .unwrap_or_default()
        .to_ascii_lowercase();
    let output = if desktop.contains("kde") {
        Command::new("kreadconfig6")
            .args([
                "--file",
                "kdeglobals",
                "--group",
                "KDE",
                "--key",
                "DoubleClickInterval",
            ])
            .output()
            .ok()?
    } else if desktop.contains("gnome") || desktop.contains("unity") {
        Command::new("gsettings")
            .args(["get", "org.gnome.desktop.peripherals.mouse", "double-click"])
            .output()
            .ok()?
    } else {
        return None;
    };
    if !output.status.success() {
        return None;
    }
    let millis = std::str::from_utf8(&output.stdout)
        .ok()?
        .trim()
        .parse::<u64>()
        .ok()?;
    (millis > 0).then_some(Duration::from_millis(millis))
}

#[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
fn native_interval() -> Option<Duration> {
    None
}

pub(super) struct Clicks {
    interval: Option<Duration>,
    left: Option<(Instant, Vec2)>,
    right: Option<(Instant, Vec2)>,
    pub double_right: bool,
    pub right_origin: Vec2,
    group: Option<(usize, Instant)>,
    pub double_left: bool,
    pub origin: Vec2,
}

impl Clicks {
    pub const fn new(interval: Option<Duration>) -> Self {
        Self {
            interval,
            left: None,
            right: None,
            double_right: false,
            right_origin: Vec2::ZERO,
            group: None,
            double_left: false,
            origin: Vec2::ZERO,
        }
    }

    pub const fn clear(&mut self) {
        self.left = None;
        self.right = None;
        self.double_right = false;
        self.group = None;
        self.double_left = false;
    }

    pub const fn manual_selection(&mut self) {
        // SelectionXlat resets m_lastGroupSelGroup for manual selection without
        // resetting the raw mouse click sequence or the saved hotkey squads.
        self.group = None;
    }

    pub fn left_down(&mut self, now: Instant, point: Vec2, tolerance: f32) {
        // The spatial adapter uses the imported Mouse.ini physical-pixel
        // tolerance; Windows' raw double-click spatial metric is not exposed by
        // Bevy. Pair presses (third click starts a new pair), dispatch on release.
        self.double_left = self.interval.is_some_and(|interval| {
            self.left.is_some_and(|(previous, at)| {
                now.duration_since(previous) < interval
                    && (point - at).abs().max_element() < tolerance
            })
        });
        // MetaEvent preserves the first-down anchor for RAW_*_DOUBLE_CLICK.
        self.origin = if self.double_left {
            self.left.map_or(point, |(_, at)| at)
        } else {
            point
        };
        self.left = (!self.double_left).then_some((now, point));
    }

    pub fn right_down(&mut self, now: Instant, point: Vec2, tolerance: f32) {
        self.double_right = self.interval.is_some_and(|interval| {
            self.right.is_some_and(|(previous, at)| {
                now.duration_since(previous) < interval
                    && (point - at).abs().max_element() < tolerance
            })
        });
        self.right_origin = if self.double_right {
            self.right.map_or(point, |(_, at)| at)
        } else {
            point
        };
        self.right = (!self.double_right).then_some((now, point));
    }

    pub fn group_pressed(&mut self, group: usize, now: Instant) -> bool {
        // SelectionXlat shares this history between SelectTeam and AddTeam.
        let double = self.interval.is_some_and(|interval| {
            self.group.is_some_and(|(previous, at)| {
                previous == group && now.duration_since(at) < interval
            })
        });
        self.group = Some((group, now));
        double
    }
}

#[cfg(test)]
mod tests {
    use super::Clicks;
    use bevy::prelude::Vec2;
    use std::time::{Duration, Instant};

    #[test]
    fn click_pairs_respect_system_interval_and_physical_tolerance() {
        let at = Instant::now();
        let mut clicks = Clicks::new(Some(Duration::from_millis(500)));
        clicks.left_down(at, Vec2::ZERO, 4.0);
        clicks.left_down(at + Duration::from_millis(499), Vec2::splat(3.0), 4.0);
        assert!(clicks.double_left);
        assert_eq!(clicks.origin, Vec2::ZERO);
        clicks.left_down(at + Duration::from_millis(510), Vec2::splat(3.0), 4.0);
        assert!(!clicks.double_left);
        clicks.left_down(at + Duration::from_millis(1010), Vec2::splat(3.0), 4.0);
        assert!(!clicks.double_left);
        clicks.left_down(at + Duration::from_millis(1020), Vec2::new(7.0, 3.0), 4.0);
        assert!(!clicks.double_left);
    }

    #[test]
    fn group_double_tap_requires_same_group_and_reset_clears_history() {
        let at = Instant::now();
        let mut clicks = Clicks::new(Some(Duration::from_millis(500)));
        assert!(!clicks.group_pressed(2, at));
        assert!(!clicks.group_pressed(3, at + Duration::from_millis(20)));
        assert!(clicks.group_pressed(3, at + Duration::from_millis(40)));
        clicks.clear();
        assert!(!clicks.group_pressed(3, at + Duration::from_millis(50)));
        let mut unavailable = Clicks::new(None);
        unavailable.left_down(at, Vec2::ZERO, 4.0);
        unavailable.left_down(at, Vec2::ZERO, 4.0);
        assert!(!unavailable.double_left);
        assert!(!unavailable.group_pressed(1, at));
        assert!(!unavailable.group_pressed(1, at));
    }

    #[test]
    fn manual_selection_between_recalls_cancels_camera_double_tap_only() {
        let at = Instant::now();
        let mut clicks = Clicks::new(Some(Duration::from_millis(500)));
        assert!(!clicks.group_pressed(2, at));
        clicks.left_down(at, Vec2::ZERO, 4.0);
        clicks.manual_selection();
        assert!(!clicks.group_pressed(2, at + Duration::from_millis(20)));
        clicks.left_down(at + Duration::from_millis(30), Vec2::ZERO, 4.0);
        assert!(clicks.double_left);
        assert!(clicks.group_pressed(2, at + Duration::from_millis(40)));
    }
    #[test]
    fn alternate_mouse_double_click_history_is_independent_and_clears() {
        let now = Instant::now();
        let mut clicks = Clicks::new(Some(Duration::from_millis(500)));
        clicks.right_down(now, Vec2::ZERO, 4.0);
        clicks.left_down(now + Duration::from_millis(10), Vec2::ZERO, 4.0);
        assert!(!clicks.double_left && !clicks.double_right);
        clicks.right_down(now + Duration::from_millis(20), Vec2::ONE, 4.0);
        assert!(clicks.double_right);
        assert_eq!(clicks.right_origin, Vec2::ZERO);
        clicks.right_down(now + Duration::from_millis(30), Vec2::ONE, 4.0);
        assert!(!clicks.double_right);
        clicks.clear();
        clicks.right_down(now + Duration::from_millis(40), Vec2::ONE, 4.0);
        assert!(!clicks.double_right);
    }
}
