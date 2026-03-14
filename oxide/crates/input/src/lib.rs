//! Oxide input crate
//!
//! Provides gesture helpers (tap, long-press, pan) over the platform-agnostic
//! input events defined in `oxide-platform-api`.
#![allow(clippy::all, clippy::pedantic)]
#![allow(
    clippy::missing_errors_doc,
    clippy::missing_panics_doc,
    clippy::module_name_repetitions,
    clippy::cast_precision_loss
)]

use oxide_platform_api as api;
use std::collections::HashMap;

/// Configuration for gesture recognition thresholds.
#[derive(Debug, Clone, Copy)]
pub struct GestureConfig {
    pub tap_max_ms: u64,
    pub tap_max_move: f32,
    pub long_ms: u64,
    pub pan_min_move: f32,
    pub double_tap_max_ms: u64,
    pub double_tap_max_move: f32,
}

impl Default for GestureConfig {
    fn default() -> Self {
        Self {
            tap_max_ms: 220,
            tap_max_move: 8.0,
            long_ms: 450,
            pan_min_move: 6.0,
            double_tap_max_ms: 320,
            double_tap_max_move: 12.0,
        }
    }
}

/// High-level gesture outputs.
#[derive(Debug, Clone, PartialEq)]
pub enum GestureEvent {
    Tap { id: api::TouchId, x: f32, y: f32 },
    DoubleTap { id: api::TouchId, x: f32, y: f32 },
    LongPress { id: api::TouchId, x: f32, y: f32 },
    PanStart { id: api::TouchId, x: f32, y: f32 },
    PanMove { id: api::TouchId, x: f32, y: f32, dx: f32, dy: f32 },
    PanEnd { id: api::TouchId, x: f32, y: f32, vx: f32, vy: f32 },
    Cancel { id: api::TouchId },
}

#[derive(Debug, Clone, PartialEq)]
pub struct GestureOutcome {
    pub event: GestureEvent,
    pub haptic: Option<api::HapticPattern>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TrackState {
    Pending,
    LongFired,
    Panning,
}

#[derive(Debug, Clone, Copy)]
struct Track {
    start_x: f32,
    start_y: f32,
    last_x: f32,
    last_y: f32,
    start_ms: u64,
    last_ms: u64,
    state: TrackState,
}

impl Track {
    fn new(x: f32, y: f32, t: u64) -> Self {
        Self {
            start_x: x,
            start_y: y,
            last_x: x,
            last_y: y,
            start_ms: t,
            last_ms: t,
            state: TrackState::Pending,
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct TapMemory {
    x: f32,
    y: f32,
    time_ms: u64,
}

/// Stateful recognizer for touch gestures.
pub struct GestureRecognizer {
    cfg: GestureConfig,
    tracks: HashMap<api::TouchId, Track>,
    last_tap: Option<TapMemory>,
}

impl GestureRecognizer {
    pub fn new(cfg: GestureConfig) -> Self {
        Self { cfg, tracks: HashMap::new(), last_tap: None }
    }
    pub fn with_defaults() -> Self {
        Self::new(GestureConfig::default())
    }

    /// Feed a touch event with a monotonic timestamp in milliseconds.
    /// Returns zero or more gesture events (e.g., PanMove can coalesce multiple outputs).
    pub fn on_touch(&mut self, ev: &api::TouchEvent, t_ms: u64) -> alloc::vec::Vec<GestureEvent> {
        self.on_touch_with_feedback(ev, t_ms).into_iter().map(|o| o.event).collect()
    }

    pub fn on_touch_with_feedback(
        &mut self,
        ev: &api::TouchEvent,
        t_ms: u64,
    ) -> alloc::vec::Vec<GestureOutcome> {
        let mut out = alloc::vec::Vec::new();
        match ev.phase {
            api::TouchPhase::Start => {
                self.tracks.insert(ev.id, Track::new(ev.x, ev.y, t_ms));
            }
            api::TouchPhase::Move => {
                if let Some(tr) = self.tracks.get_mut(&ev.id) {
                    let dx = ev.x - tr.last_x;
                    let dy = ev.y - tr.last_y;
                    let mx = ev.x - tr.start_x;
                    let my = ev.y - tr.start_y;
                    let moved = (mx * mx + my * my).sqrt();
                    match tr.state {
                        TrackState::Pending => {
                            if moved >= self.cfg.pan_min_move {
                                tr.state = TrackState::Panning;
                                push_outcome(
                                    &mut out,
                                    GestureEvent::PanStart { id: ev.id, x: ev.x, y: ev.y },
                                );
                                self.last_tap = None;
                            } else if t_ms.saturating_sub(tr.start_ms) >= self.cfg.long_ms
                                && tr.state != TrackState::LongFired
                            {
                                tr.state = TrackState::LongFired;
                                push_outcome(
                                    &mut out,
                                    GestureEvent::LongPress { id: ev.id, x: ev.x, y: ev.y },
                                );
                                self.last_tap = None;
                            }
                        }
                        TrackState::LongFired => {
                            if moved >= self.cfg.pan_min_move {
                                tr.state = TrackState::Panning;
                                push_outcome(
                                    &mut out,
                                    GestureEvent::PanStart { id: ev.id, x: ev.x, y: ev.y },
                                );
                                self.last_tap = None;
                            }
                        }
                        TrackState::Panning => {
                            push_outcome(
                                &mut out,
                                GestureEvent::PanMove { id: ev.id, x: ev.x, y: ev.y, dx, dy },
                            );
                        }
                    }
                    tr.last_x = ev.x;
                    tr.last_y = ev.y;
                    tr.last_ms = t_ms;
                }
            }
            api::TouchPhase::End => {
                if let Some(tr) = self.tracks.remove(&ev.id) {
                    match tr.state {
                        TrackState::Panning => {
                            let dt = (t_ms.saturating_sub(tr.last_ms)).max(1) as f32 / 1000.0;
                            let vx = (ev.x - tr.last_x) / dt;
                            let vy = (ev.y - tr.last_y) / dt;
                            push_outcome(
                                &mut out,
                                GestureEvent::PanEnd { id: ev.id, x: ev.x, y: ev.y, vx, vy },
                            );
                            self.last_tap = None;
                        }
                        TrackState::Pending | TrackState::LongFired => {
                            let moved =
                                ((ev.x - tr.start_x).powi(2) + (ev.y - tr.start_y).powi(2)).sqrt();
                            let dt = t_ms.saturating_sub(tr.start_ms);
                            if moved <= self.cfg.tap_max_move && dt <= self.cfg.tap_max_ms {
                                if !self.try_double_tap(ev, t_ms, &mut out) {
                                    self.last_tap =
                                        Some(TapMemory { x: ev.x, y: ev.y, time_ms: t_ms });
                                    push_outcome(
                                        &mut out,
                                        GestureEvent::Tap { id: ev.id, x: ev.x, y: ev.y },
                                    );
                                }
                            } else {
                                self.last_tap = None;
                            }
                        }
                    }
                }
            }
            api::TouchPhase::Cancel => {
                if self.tracks.remove(&ev.id).is_some() {
                    push_outcome(&mut out, GestureEvent::Cancel { id: ev.id });
                }
                self.last_tap = None;
            }
        }
        out
    }

    fn try_double_tap(
        &mut self,
        ev: &api::TouchEvent,
        t_ms: u64,
        out: &mut alloc::vec::Vec<GestureOutcome>,
    ) -> bool {
        if let Some(prev) = self.last_tap {
            let dt = t_ms.saturating_sub(prev.time_ms);
            if dt <= self.cfg.double_tap_max_ms {
                let dx = ev.x - prev.x;
                let dy = ev.y - prev.y;
                let dist = (dx * dx + dy * dy).sqrt();
                if dist <= self.cfg.double_tap_max_move {
                    self.last_tap = None;
                    push_outcome(out, GestureEvent::DoubleTap { id: ev.id, x: ev.x, y: ev.y });
                    return true;
                }
            }
        }
        false
    }
}

extern crate alloc;

fn push_outcome(out: &mut alloc::vec::Vec<GestureOutcome>, event: GestureEvent) {
    out.push(GestureOutcome { haptic: default_haptic(&event), event });
}

fn default_haptic(event: &GestureEvent) -> Option<api::HapticPattern> {
    match event {
        GestureEvent::Tap { .. } => Some(api::HapticPattern::Selection),
        GestureEvent::DoubleTap { .. } => Some(api::HapticPattern::ImpactLight),
        GestureEvent::LongPress { .. } => Some(api::HapticPattern::ImpactMedium),
        GestureEvent::PanStart { .. } | GestureEvent::PanEnd { .. } => {
            Some(api::HapticPattern::Selection)
        }
        _ => None,
    }
}

// ===== Scroll accumulator (pure CPU helper) =====

/// Scroll phase to model platform semantics (e.g., macOS momentum).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScrollPhase {
    Began,
    Changed,
    Momentum,
    Ended,
}

/// Accumulates high-frequency scroll deltas within a tick, with simple momentum handling.
#[derive(Debug, Default)]
pub struct ScrollAccumulator {
    sum_x: f32,
    sum_y: f32,
    in_momentum: bool,
}

impl ScrollAccumulator {
    pub fn push(&mut self, dx: f32, dy: f32, phase: ScrollPhase) {
        match phase {
            ScrollPhase::Began => {
                self.in_momentum = false;
                self.sum_x += dx;
                self.sum_y += dy;
            }
            ScrollPhase::Changed => {
                self.sum_x += dx;
                self.sum_y += dy;
            }
            ScrollPhase::Momentum => {
                self.in_momentum = true;
                self.sum_x += dx;
                self.sum_y += dy;
            }
            ScrollPhase::Ended => { /* End of gesture; flushing handled by take() */ }
        }
    }

    /// Returns the accumulated delta and resets the accumulator for the next tick.
    pub fn take(&mut self) -> (f32, f32) {
        let out = (self.sum_x, self.sum_y);
        self.sum_x = 0.0;
        self.sum_y = 0.0;
        out
    }

    /// Returns true if the last observed phase indicated momentum.
    pub fn momentum(&self) -> bool {
        self.in_momentum
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tid(n: u64) -> api::TouchId {
        api::TouchId(n)
    }
    fn start(id: u64, x: f32, y: f32) -> api::TouchEvent {
        api::TouchEvent {
            id: tid(id),
            phase: api::TouchPhase::Start,
            x,
            y,
            pressure: None,
            tilt: None,
            device: api::PointerDevice::Finger,
        }
    }
    fn mv(id: u64, x: f32, y: f32) -> api::TouchEvent {
        api::TouchEvent {
            id: tid(id),
            phase: api::TouchPhase::Move,
            x,
            y,
            pressure: None,
            tilt: None,
            device: api::PointerDevice::Finger,
        }
    }
    fn end(id: u64, x: f32, y: f32) -> api::TouchEvent {
        api::TouchEvent {
            id: tid(id),
            phase: api::TouchPhase::End,
            x,
            y,
            pressure: None,
            tilt: None,
            device: api::PointerDevice::Finger,
        }
    }

    #[test]
    fn tap_recognized() {
        let mut g = GestureRecognizer::with_defaults();
        assert!(g.on_touch(&start(1, 10.0, 10.0), 0).is_empty());
        assert!(g.on_touch(&mv(1, 12.0, 10.0), 50).is_empty());
        let out = g.on_touch(&end(1, 12.0, 10.0), 150);
        assert_eq!(out, vec![GestureEvent::Tap { id: tid(1), x: 12.0, y: 10.0 }]);
    }

    #[test]
    fn long_then_pan() {
        let mut g = GestureRecognizer::with_defaults();
        g.on_touch(&start(1, 0.0, 0.0), 0);
        // Hold still beyond long threshold
        let out = g.on_touch(&mv(1, 1.0, 0.0), 500);
        assert_eq!(out, vec![GestureEvent::LongPress { id: tid(1), x: 1.0, y: 0.0 }]);
        // Move enough to start pan
        let out2 = g.on_touch(&mv(1, 20.0, 0.0), 520);
        assert_eq!(out2, vec![GestureEvent::PanStart { id: tid(1), x: 20.0, y: 0.0 }]);
        // Pan moves
        let out3 = g.on_touch(&mv(1, 25.0, 5.0), 540);
        assert_eq!(
            out3,
            vec![GestureEvent::PanMove { id: tid(1), x: 25.0, y: 5.0, dx: 5.0, dy: 5.0 }]
        );
        // End
        let out4 = g.on_touch(&end(1, 30.0, 10.0), 560);
        if let [GestureEvent::PanEnd { id, .. }] = &out4[..] {
            assert_eq!(*id, tid(1));
        } else {
            panic!("expected PanEnd")
        }
    }

    #[test]
    fn cancel_breaks_gesture() {
        let mut g = GestureRecognizer::with_defaults();
        g.on_touch(&start(1, 0.0, 0.0), 0);
        let _ = g.on_touch(&mv(1, 2.0, 2.0), 50);
        let out = g.on_touch(
            &api::TouchEvent {
                id: tid(1),
                phase: api::TouchPhase::Cancel,
                x: 2.0,
                y: 2.0,
                pressure: None,
                tilt: None,
                device: api::PointerDevice::Finger,
            },
            60,
        );
        assert_eq!(out, vec![GestureEvent::Cancel { id: tid(1) }]);
        // End after cancel should not produce Tap or PanEnd
        assert!(g.on_touch(&end(1, 2.0, 2.0), 80).is_empty());
    }

    #[test]
    fn deterministic_move_order_within_tick() {
        let mut g = GestureRecognizer::with_defaults();
        g.on_touch(&start(1, 0.0, 0.0), 0);
        // Move enough to enter panning
        let _ = g.on_touch(&mv(1, 10.0, 0.0), 1);
        // Subsequent moves at the same timestamp should preserve ordering of outputs
        let o1 = g.on_touch(&mv(1, 15.0, 0.0), 2);
        let o2 = g.on_touch(&mv(1, 20.0, 0.0), 2);
        match (&o1[..], &o2[..]) {
            ([GestureEvent::PanMove { x: x1, .. }], [GestureEvent::PanMove { x: x2, .. }]) => {
                assert!(*x1 < *x2);
            }
            _ => panic!("expected PanMove events"),
        }
    }

    #[test]
    fn scroll_accumulates_and_resets() {
        let mut acc = ScrollAccumulator::default();
        acc.push(1.0, 2.0, ScrollPhase::Began);
        acc.push(0.5, 1.0, ScrollPhase::Changed);
        let (dx, dy) = acc.take();
        assert!((dx - 1.5).abs() < 1e-6 && (dy - 3.0).abs() < 1e-6);
        let (dx2, dy2) = acc.take();
        assert!(dx2.abs() < 1e-6 && dy2.abs() < 1e-6);
    }

    #[test]
    fn scroll_momentum_flag() {
        let mut acc = ScrollAccumulator::default();
        acc.push(0.0, -1.0, ScrollPhase::Momentum);
        assert!(acc.momentum());
        let _ = acc.take();
        // After take, momentum remains flagged until a new Began
        assert!(acc.momentum());
        acc.push(0.0, 0.0, ScrollPhase::Began);
        assert!(!acc.momentum());
    }
}
