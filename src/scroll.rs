//! Shared wheel scrolling for pixel-based virtual views and custom canvases.
//!
//! By default, accumulates the full wheel delta and eases toward that destination
//! over 120 ms. Reversal discards pending travel in the old direction;
//! bounds, keyboard navigation and scrollbar jumps cancel stale travel.
//! Call `position` when drawing and, while `running`, `anim::request_redraw`.
//! This uses the toolkit's shared animation timer, never a timer per view.
//!
//! Adaptive motion is opt-in. FLTK 1.5.23
//! exposes integer deltas without Wayland axis source/stop information. A
//! backend supplying precise pixels with native momentum should apply those
//! pixels directly and `jump`, rather than feeding a second inertia model.

use std::time::Instant;
use fltk::{app, prelude::*};

/// Vertical wheel distance when the pointer is inside the widget and its clips.
pub fn wheel_delta(widget: &impl WidgetExt, pixels_per_notch: f64) -> Option<f64> {
    if !app::event_inside_widget(widget) { return None; }
    let mut parent = widget.parent();
    while let Some(group) = parent {
        if !app::event_inside_widget(&group) { return None; }
        parent = group.parent();
    }
    let dy = app::event_dy_value();
    (dy != 0).then_some(dy as f64 * pixels_per_notch)
}

/// Wheel response. Native applies deltas immediately; Smooth is the default.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Feel { Native, #[default] Smooth, Adaptive }


#[derive(Debug, Clone, Default)]
pub struct Scroll {
    value: f64,
    from: f64,
    target: f64,
    reported: i64,
    start: Option<Instant>,
    feel: Feel,
    last_wheel: Option<Instant>,
    gain: f64,
    direction: f64,
}

impl Scroll {
    pub fn new() -> Self {
        Self::default()
    }

    /// Select a wheel response; adaptive is opt-in because FLTK cannot identify OS momentum.
    pub fn with_feel(mut self, feel: Feel) -> Self {
        self.feel = feel;
        self
    }

    /// Immediately adopts an external position (keyboard, scrollbar, location).
    pub fn jump(&mut self, position: i64) {
        self.value = position.max(0) as f64;
        self.from = self.value;
        self.target = self.value;
        self.reported = position.max(0);
        self.start = None;
        self.last_wheel = None;
        self.gain = 1.0;
        self.direction = 0.0;
    }

    /// Add a signed pixel distance; preserve magnitude, not only direction.
    /// `position` is the last painted offset; `max` is the current scroll limit.
    pub fn wheel(&mut self, position: i64, delta: f64, max: i64) -> i64 {
        self.wheel_at(position, delta, max, crate::anim::now(), crate::anim::enabled())
    }

    /// Current painted offset. A changed external position cancels animation.
    pub fn position(&mut self, position: i64, max: i64) -> i64 {
        self.position_at(position, max, crate::anim::now(), crate::anim::enabled())
    }

    /// Moves the motion by `delta` pixels, keeping a running animation:
    /// content was inserted or removed above the view (scroll anchoring).
    pub fn shift(&mut self, delta: i64) {
        let d = delta as f64;
        self.value = (self.value + d).max(0.0);
        self.from = (self.from + d).max(0.0);
        self.target = (self.target + d).max(0.0);
        self.reported = (self.reported + delta).max(0);
    }

    pub fn running(&self) -> bool {
        self.start.is_some()
    }

    /// True when a wheel step of `delta` has nowhere to go: the view is
    /// at the edge it points to (0 or `max`) with no travel pending. A
    /// nested view returns the wheel event unused then, so the scroll area
    /// around it scrolls instead (o3-code's `T3Wheel` hand-off). A zero
    /// delta (a horizontal wheel on a vertical view) also passes.
    pub fn passes(&self, position: i64, delta: f64, max: i64) -> bool {
        if delta == 0.0 {
            return true;
        }
        let max = max.max(0);
        let at = if self.start.is_some() { self.target.round() as i64 } else { position.clamp(0, max) };
        (delta < 0.0 && at <= 0) || (delta > 0.0 && at >= max)
    }

    fn position_at(&mut self, position: i64, max: i64, now: Instant, smooth: bool) -> i64 {
        let max = max.max(0);
        if position != self.reported {
            self.jump(position.clamp(0, max));
        }
        let limit = max as f64;
        self.from = self.from.clamp(0.0, limit);
        self.target = self.target.clamp(0.0, limit);
        if let Some(start) = self.start {
            let t = now.saturating_duration_since(start).as_secs_f64() / if self.feel == Feel::Adaptive { 0.6 } else { 0.12 };
            if !smooth || t >= 1.0 {
                self.value = self.target;
                self.start = None;
            } else {
                let ease = 1.0 - (1.0 - t).powi(3);
                self.value = self.from + (self.target - self.from) * ease;
            }
        }
        self.value = self.value.clamp(0.0, limit);
        if (self.target - self.value).abs() < 0.01 {
            self.value = self.target;
            self.start = None;
        }
        self.reported = (self.value.round() as i64).clamp(0, max);
        self.reported
    }

    fn wheel_at(&mut self, position: i64, delta: f64, max: i64, now: Instant, smooth: bool) -> i64 {
        self.position_at(position, max, now, smooth);
        if !delta.is_finite() || delta == 0.0 {
            return self.reported;
        }
        let smooth = smooth && self.feel != Feel::Native;
        let remaining = self.target - self.value;
        let delta = if smooth && self.feel == Feel::Adaptive {
            let same_direction = self.direction * delta > 0.0;
            let recent = self.last_wheel.is_some_and(|last| now.saturating_duration_since(last).as_millis() <= 100);
            self.gain = if same_direction && recent { (self.gain.max(1.0) + 0.5).min(3.0) } else { 1.0 };
            self.last_wheel = Some(now);
            self.direction = delta.signum();
            delta * self.gain
        } else { delta };
        let base = if remaining * delta < 0.0 {
            self.value
        } else {
            self.target
        };
        self.from = self.value;
        self.target = (base + delta).clamp(0.0, max.max(0) as f64);
        self.start = (self.target - self.value).abs().ge(&0.01).then_some(now);
        self.position_at(self.reported, max, now, smooth)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn adaptive_coasts_caps_bursts_and_cancels_reversal() {
        let t = Instant::now();
        let mut s = Scroll::new().with_feel(Feel::Adaptive);
        s.wheel_at(0, 100.0, 10000, t, true);
        let mut p = s.position_at(0, 10000, later(t, 120), true);
        assert!(p > 0 && p < 100 && s.running());
        for ms in [130, 140, 150, 160, 170, 180] {
            p = s.wheel_at(p, 10.0, 10000, later(t, ms), true);
        }
        assert_eq!(s.gain, 3.0);
        p = s.wheel_at(p, -20.0, 10000, later(t, 190), true);
        assert_eq!(s.gain, 1.0);
        assert!(s.position_at(p, 10000, later(t, 300), true) < p);
        let p = s.position_at(s.reported, 10000, later(t, 800), true);
        assert!(!s.running());
        assert_eq!(s.wheel_at(p, 100.0, 10000, later(t, 900), false), p + 100);
        assert!(!s.running());
        s.jump(10000);
        assert_eq!(s.wheel_at(10000, 100.0, 10000, later(t, 1000), true), 10000);
        assert!(!s.running());
        s.wheel_at(10000, -100.0, 10000, later(t, 1010), true);
        s.shift(-10);
        assert!(s.position_at(9990, 9000, later(t, 1020), true) <= 9000);
        s.jump(7);
        assert!(!s.running());
    }

    #[test]
    fn passes_at_the_edges() {
        let s = super::Scroll::new();
        assert!(s.passes(0, -30.0, 500), "up at the top");
        assert!(!s.passes(0, 30.0, 500), "down from the top scrolls");
        assert!(s.passes(500, 30.0, 500), "down at the bottom");
        assert!(!s.passes(250, -30.0, 500));
        assert!(s.passes(250, 0.0, 500), "horizontal wheel");
        assert!(s.passes(0, 30.0, 0), "nothing to scroll");
    }
    use super::*;
    use std::time::Duration;
    fn later(t: Instant, ms: u64) -> Instant {
        t + Duration::from_millis(ms)
    }

    #[test]
    fn keeps_magnitude_and_accumulated_distance_without_extra_momentum() {
        let t = Instant::now();
        let mut s = Scroll::new();
        assert_eq!(s.wheel_at(0, 114.0, 1000, t, true), 0);
        let p = s.position_at(0, 1000, later(t, 30), true);
        assert!(p > 0 && p < 114);
        s.wheel_at(p, 76.0, 1000, later(t, 30), true);
        assert_eq!(s.position_at(p, 1000, later(t, 200), true), 190);
        assert!(!s.running());
    }

    #[test]
    fn frame_rate_does_not_change_motion_or_distance() {
        let t = Instant::now();
        let mut a = Scroll::new();
        let mut b = Scroll::new();
        a.wheel_at(0, 200.0, 1000, t, true);
        b.wheel_at(0, 200.0, 1000, t, true);
        let mut p = 0;
        for ms in [10, 20, 30, 40, 50] {
            p = a.position_at(p, 1000, later(t, ms), true);
        }
        assert_eq!(
            a.position_at(p, 1000, later(t, 60), true),
            b.position_at(0, 1000, later(t, 60), true)
        );
    }

    #[test]
    fn reversal_moves_back_immediately_instead_of_finishing_old_travel() {
        let t = Instant::now();
        let mut s = Scroll::new();
        s.wheel_at(0, 400.0, 1000, t, true);
        let p = s.position_at(0, 1000, later(t, 30), true);
        s.wheel_at(p, -30.0, 1000, later(t, 30), true);
        let q = s.position_at(p, 1000, later(t, 45), true);
        assert!(q < p);
        assert!((s.position_at(q, 1000, later(t, 200), true) - (p - 30)).abs() <= 1);
    }

    #[test]
    fn external_navigation_and_explicit_jump_cancel_travel() {
        let t = Instant::now();
        let mut s = Scroll::new();
        s.wheel_at(0, 300.0, 1000, t, true);
        assert_eq!(s.position_at(700, 1000, later(t, 20), true), 700);
        assert!(!s.running());
        s.wheel_at(700, 200.0, 1000, later(t, 30), true);
        s.jump(0);
        assert_eq!(s.position_at(0, 1000, later(t, 80), true), 0);
        assert!(!s.running());
    }

    #[test]
    fn clamps_huge_deltas_and_changed_bounds_without_overshoot() {
        let t = Instant::now();
        let mut s = Scroll::new();
        s.wheel_at(0, f64::MAX, 100, t, true);
        let mut p = 0;
        for ms in 1..150 {
            p = s.position_at(p, 100, later(t, ms), true);
            assert!((0..=100).contains(&p));
        }
        assert_eq!(p, 100);
        assert_eq!(s.position_at(p, 20, later(t, 160), true), 20);
        s.wheel_at(20, -1e10, 20, later(t, 170), true);
        assert_eq!(s.position_at(20, 20, later(t, 400), true), 0);
        assert!(!s.running());
    }

    #[test]
    fn reduced_motion_is_immediate_and_invalid_deltas_are_ignored() {
        let t = Instant::now();
        let mut s = Scroll::new();
        assert_eq!(s.wheel_at(0, 114.0, 1000, t, false), 114);
        assert!(!s.running());
        assert_eq!(s.wheel_at(114, f64::NAN, 1000, t, false), 114);
        assert_eq!(s.wheel_at(114, f64::INFINITY, 1000, t, false), 114);
        assert_eq!(s.wheel_at(114, 100.0, 0, t, true), 0);
        assert!(!s.running());
    }
}
