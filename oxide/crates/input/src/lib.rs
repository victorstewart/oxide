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

/// Raw-touch surface gestures derived inside Oxide.
#[derive(Debug, Clone, PartialEq)]
pub enum TouchSurfaceEvent {
    ActiveTouchesChanged {
        touch_count: u8,
        x: f32,
        y: f32,
    },
    Pan {
        touch_count: u8,
        x: f32,
        y: f32,
        dx: f32,
        dy: f32,
    },
    Pinch {
        x: f32,
        y: f32,
        scale_delta: f32,
        log2_scale_delta: f32,
        gesture_scale: f32,
        log2_gesture_scale: f32,
    },
}

pub fn touch_phase_from_raw(phase: u32) -> Option<api::TouchPhase>
{
   match phase
   {
      0 => Some(api::TouchPhase::Start),
      1 => Some(api::TouchPhase::Move),
      2 => Some(api::TouchPhase::End),
      3 => Some(api::TouchPhase::Cancel),
      _ => None,
   }
}

pub fn pointer_device_from_raw(device: u32) -> api::PointerDevice
{
   match device
   {
      1 => api::PointerDevice::Pencil,
      2 => api::PointerDevice::Mouse,
      _ => api::PointerDevice::Finger,
   }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PrimaryPointerSample
{
   pub x: f32,
   pub y: f32,
   pub dx: f32,
   pub dy: f32,
   pub buttons: u32,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct PrimaryTouchResult
{
   pub pointer: Option<PrimaryPointerSample>,
   pub double_tap: bool,
}

#[derive(Debug, Clone, Copy)]
struct PrimaryTouchTrack
{
   id: api::TouchId,
   start_x: f32,
   start_y: f32,
   last_x: f32,
   last_y: f32,
   start_ms: u64,
   last_ms: u64,
}

impl PrimaryTouchTrack
{
   fn new(id: api::TouchId, x: f32, y: f32, ts_ns: u64) -> Self
   {
      let ms = ts_ns / 1_000_000;
      Self { id, start_x: x, start_y: y, last_x: x, last_y: y, start_ms: ms, last_ms: ms }
   }
}

#[derive(Debug, Clone, Copy)]
struct PrimaryTapRecord
{
   ts_ms: u64,
   x: f32,
   y: f32,
}

#[derive(Debug, Clone, Default)]
pub struct PrimaryTouchTracker
{
   active: Option<PrimaryTouchTrack>,
   last_tap: Option<PrimaryTapRecord>,
}

impl PrimaryTouchTracker
{
   pub fn new() -> Self
   {
      Self::default()
   }

   pub fn reset(&mut self)
   {
      self.active = None;
      self.last_tap = None;
   }

   pub fn on_touch(&mut self, ev: &api::TouchEvent, ts_ns: u64) -> PrimaryTouchResult
   {
      let mut result = PrimaryTouchResult::default();
      let ms = ts_ns / 1_000_000;
      match ev.phase
      {
         api::TouchPhase::Start =>
         {
            if self.active.is_none()
            {
               self.active = Some(PrimaryTouchTrack::new(ev.id, ev.x, ev.y, ts_ns));
               result.pointer = Some(PrimaryPointerSample {
                  x: ev.x,
                  y: ev.y,
                  dx: 0.0,
                  dy: 0.0,
                  buttons: 1,
               });
            }
         }
         api::TouchPhase::Move =>
         {
            if let Some(mut track) = self.active
            {
               if track.id == ev.id
               {
                  let dx = ev.x - track.last_x;
                  let dy = ev.y - track.last_y;
                  track.last_x = ev.x;
                  track.last_y = ev.y;
                  track.last_ms = ms;
                  result.pointer = Some(PrimaryPointerSample {
                     x: ev.x,
                     y: ev.y,
                     dx,
                     dy,
                     buttons: 1,
                  });
                  self.active = Some(track);
               }
            }
         }
         api::TouchPhase::End | api::TouchPhase::Cancel =>
         {
            if let Some(track) = self.active
            {
               if track.id == ev.id
               {
                  let dx = ev.x - track.last_x;
                  let dy = ev.y - track.last_y;
                  result.pointer = Some(PrimaryPointerSample {
                     x: ev.x,
                     y: ev.y,
                     dx,
                     dy,
                     buttons: 0,
                  });
                  let total_dx = ev.x - track.start_x;
                  let total_dy = ev.y - track.start_y;
                  let moved_sq = total_dx * total_dx + total_dy * total_dy;
                  let dur_ms = ms.saturating_sub(track.start_ms);
                  if dur_ms <= 300 && moved_sq <= 36.0
                  {
                     let tapped = PrimaryTapRecord { ts_ms: ms, x: ev.x, y: ev.y };
                     if let Some(prev) = self.last_tap
                     {
                        let dt = tapped.ts_ms.saturating_sub(prev.ts_ms);
                        let dx = tapped.x - prev.x;
                        let dy = tapped.y - prev.y;
                        if dt <= 360 && (dx * dx + dy * dy) <= 144.0
                        {
                           result.double_tap = true;
                        }
                     }
                     self.last_tap = Some(tapped);
                  }
                  self.active = None;
               }
            }
            if matches!(ev.phase, api::TouchPhase::Cancel)
            {
               self.active = None;
            }
         }
      }
      result
   }
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

#[derive(Debug, Clone, Copy)]
struct GestureTrackSlot {
    id: api::TouchId,
    track: Track,
}

#[derive(Debug, Clone, Copy)]
struct SurfaceTouch {
    id: api::TouchId,
    x: f32,
    y: f32,
}

#[derive(Debug, Clone, Copy, Default)]
struct TouchSurfaceFrame {
    count: u8,
    x: f32,
    y: f32,
    distance: f32,
}

const INLINE_TOUCH_CAP: usize = 4;

/// Tracks raw touch contacts and emits pan/pinch deltas for a continuous surface.
#[derive(Clone, Debug, Default)]
pub struct TouchSurfaceRecognizer {
    touches: [Option<SurfaceTouch>; INLINE_TOUCH_CAP],
    overflow: alloc::vec::Vec<SurfaceTouch>,
    gesture_start: Option<TouchSurfaceFrame>,
}

impl TouchSurfaceRecognizer {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn active_count(&self) -> usize {
        let inline = self.touches.iter().filter(|touch| touch.is_some()).count();
        inline + self.overflow.len()
    }

    pub fn reset(&mut self) {
        for touch in &mut self.touches {
            *touch = None;
        }
        self.overflow.clear();
        self.gesture_start = None;
    }

    pub fn on_touch(&mut self, ev: &api::TouchEvent) -> alloc::vec::Vec<TouchSurfaceEvent> {
        let mut out = alloc::vec::Vec::new();
        if !ev.x.is_finite() || !ev.y.is_finite() {
            return out;
        }

        let before = self.frame();
        match ev.phase {
            api::TouchPhase::Start => {
                self.upsert_touch(SurfaceTouch { id: ev.id, x: ev.x, y: ev.y });
            }
            api::TouchPhase::Move => {
                if let Some(touch) = self.touch_mut(ev.id) {
                    touch.x = ev.x;
                    touch.y = ev.y;
                }
            }
            api::TouchPhase::End | api::TouchPhase::Cancel => {
                let _ = self.remove_touch(ev.id);
            }
        }

        let after = self.frame();
        if before.count != after.count {
            self.gesture_start =
                if after.count == 2 && after.distance > 1.0 { Some(after) } else { None };
            out.push(TouchSurfaceEvent::ActiveTouchesChanged {
                touch_count: after.count,
                x: after.x,
                y: after.y,
            });
        }
        if !matches!(ev.phase, api::TouchPhase::Move) {
            return out;
        }

        match (before.count, after.count) {
            (1, 1) => out.push(TouchSurfaceEvent::Pan {
                touch_count: 1,
                x: after.x,
                y: after.y,
                dx: after.x - before.x,
                dy: after.y - before.y,
            }),
            (2, 2) => {
                if before.distance > 1.0 && after.distance > 1.0 {
                    let scale_delta = after.distance / before.distance;
                    let gesture_start_distance = self
                        .gesture_start
                        .filter(|start| start.count == 2 && start.distance > 1.0)
                        .map(|start| start.distance)
                        .unwrap_or(before.distance);
                    let gesture_scale = after.distance / gesture_start_distance;
                    out.push(TouchSurfaceEvent::Pinch {
                        x: after.x,
                        y: after.y,
                        scale_delta,
                        log2_scale_delta: scale_delta.log2(),
                        gesture_scale,
                        log2_gesture_scale: gesture_scale.log2(),
                    });
                }
                out.push(TouchSurfaceEvent::Pan {
                    touch_count: 2,
                    x: after.x,
                    y: after.y,
                    dx: after.x - before.x,
                    dy: after.y - before.y,
                });
            }
            _ => {}
        }
        out
    }

    fn frame(&self) -> TouchSurfaceFrame {
        let mut first: Option<SurfaceTouch> = None;
        let mut second: Option<SurfaceTouch> = None;
        for touch in self.touches.iter().copied().flatten() {
            select_surface_touch(touch, &mut first, &mut second);
        }
        for touch in self.overflow.iter().copied() {
            select_surface_touch(touch, &mut first, &mut second);
        }
        match (first, second) {
            (None, _) => TouchSurfaceFrame::default(),
            (Some(first), None) => {
                TouchSurfaceFrame { count: 1, x: first.x, y: first.y, distance: 0.0 }
            }
            (Some(first), Some(second)) => {
                let x = (first.x + second.x) * 0.5;
                let y = (first.y + second.y) * 0.5;
                let dx = second.x - first.x;
                let dy = second.y - first.y;
                TouchSurfaceFrame { count: 2, x, y, distance: (dx * dx + dy * dy).sqrt() }
            }
        }
    }

    fn upsert_touch(&mut self, next: SurfaceTouch) {
        if let Some(touch) = self.touch_mut(next.id) {
            *touch = next;
            return;
        }

        for slot in &mut self.touches {
            if slot.is_none() {
                *slot = Some(next);
                return;
            }
        }

        self.overflow.push(next);
    }

    fn touch_mut(&mut self, id: api::TouchId) -> Option<&mut SurfaceTouch> {
        for touch in self.touches.iter_mut().flatten() {
            if touch.id == id {
                return Some(touch);
            }
        }
        self.overflow.iter_mut().find(|touch| touch.id == id)
    }

    fn remove_touch(&mut self, id: api::TouchId) -> bool {
        for slot in &mut self.touches {
            if slot.as_ref().is_some_and(|touch| touch.id == id) {
                *slot = None;
                return true;
            }
        }

        if let Some(index) = self.overflow.iter().position(|touch| touch.id == id) {
            self.overflow.swap_remove(index);
            return true;
        }

        false
    }
}

fn select_surface_touch(
    touch: SurfaceTouch,
    first: &mut Option<SurfaceTouch>,
    second: &mut Option<SurfaceTouch>,
) {
    match *first {
        None => *first = Some(touch),
        Some(current) if touch.id.0 < current.id.0 => {
            *second = *first;
            *first = Some(touch);
        }
        _ => match *second {
            None => *second = Some(touch),
            Some(current) if touch.id.0 < current.id.0 => *second = Some(touch),
            _ => {}
        },
    }
}

/// Stateful recognizer for touch gestures.
pub struct GestureRecognizer {
    cfg: GestureConfig,
    tracks: [Option<GestureTrackSlot>; INLINE_TOUCH_CAP],
    overflow: alloc::vec::Vec<GestureTrackSlot>,
    last_tap: Option<TapMemory>,
}

impl GestureRecognizer {
    pub fn new(cfg: GestureConfig) -> Self {
        Self {
            cfg,
            tracks: [None; INLINE_TOUCH_CAP],
            overflow: alloc::vec::Vec::new(),
            last_tap: None,
        }
    }
    pub fn with_defaults() -> Self {
        Self::new(GestureConfig::default())
    }

    /// Feed a touch event with a monotonic timestamp in milliseconds.
    /// Returns zero or more gesture events (e.g., PanMove can coalesce multiple outputs).
    pub fn on_touch(&mut self, ev: &api::TouchEvent, t_ms: u64) -> alloc::vec::Vec<GestureEvent> {
        let mut out = alloc::vec::Vec::new();
        self.on_touch_into(ev, t_ms, &mut out);
        out
    }

    pub fn on_touch_with_feedback(
        &mut self,
        ev: &api::TouchEvent,
        t_ms: u64,
    ) -> alloc::vec::Vec<GestureOutcome> {
        let mut out = alloc::vec::Vec::new();
        self.on_touch_into(ev, t_ms, &mut out);
        out
    }

    fn on_touch_into<S: GestureSink>(&mut self, ev: &api::TouchEvent, t_ms: u64, out: &mut S) {
        match ev.phase {
            api::TouchPhase::Start => {
                self.upsert_track(ev.id, Track::new(ev.x, ev.y, t_ms));
            }
            api::TouchPhase::Move => {
                let pan_min_move = self.cfg.pan_min_move;
                let long_ms = self.cfg.long_ms;
                let mut clear_last_tap = false;
                if let Some(tr) = self.track_mut(ev.id) {
                    let dx = ev.x - tr.last_x;
                    let dy = ev.y - tr.last_y;
                    let mx = ev.x - tr.start_x;
                    let my = ev.y - tr.start_y;
                    let moved = (mx * mx + my * my).sqrt();
                    match tr.state {
                        TrackState::Pending | TrackState::LongFired if moved >= pan_min_move => {
                            tr.state = TrackState::Panning;
                            out.push_gesture(GestureEvent::PanStart {
                                id: ev.id,
                                x: ev.x,
                                y: ev.y,
                            });
                            clear_last_tap = true;
                        }
                        TrackState::Pending if t_ms.saturating_sub(tr.start_ms) >= long_ms => {
                            tr.state = TrackState::LongFired;
                            out.push_gesture(GestureEvent::LongPress {
                                id: ev.id,
                                x: ev.x,
                                y: ev.y,
                            });
                            clear_last_tap = true;
                        }
                        TrackState::Panning => {
                            out.push_gesture(GestureEvent::PanMove {
                                id: ev.id,
                                x: ev.x,
                                y: ev.y,
                                dx,
                                dy,
                            });
                        }
                        TrackState::Pending | TrackState::LongFired => {}
                    }
                    tr.last_x = ev.x;
                    tr.last_y = ev.y;
                    tr.last_ms = t_ms;
                }
                if clear_last_tap {
                    self.last_tap = None;
                }
            }
            api::TouchPhase::End => {
                if let Some(tr) = self.remove_track(ev.id) {
                    match tr.state {
                        TrackState::Panning => {
                            let dt = (t_ms.saturating_sub(tr.last_ms)).max(1) as f32 / 1000.0;
                            let vx = (ev.x - tr.last_x) / dt;
                            let vy = (ev.y - tr.last_y) / dt;
                            out.push_gesture(GestureEvent::PanEnd {
                                id: ev.id,
                                x: ev.x,
                                y: ev.y,
                                vx,
                                vy,
                            });
                            self.last_tap = None;
                        }
                        TrackState::Pending | TrackState::LongFired => {
                            let moved =
                                ((ev.x - tr.start_x).powi(2) + (ev.y - tr.start_y).powi(2)).sqrt();
                            let dt = t_ms.saturating_sub(tr.start_ms);
                            if moved <= self.cfg.tap_max_move && dt <= self.cfg.tap_max_ms {
                                if !self.try_double_tap(ev, t_ms, out) {
                                    self.last_tap =
                                        Some(TapMemory { x: ev.x, y: ev.y, time_ms: t_ms });
                                    out.push_gesture(GestureEvent::Tap {
                                        id: ev.id,
                                        x: ev.x,
                                        y: ev.y,
                                    });
                                }
                            } else {
                                self.last_tap = None;
                            }
                        }
                    }
                }
            }
            api::TouchPhase::Cancel => {
                if self.remove_track(ev.id).is_some() {
                    out.push_gesture(GestureEvent::Cancel { id: ev.id });
                }
                self.last_tap = None;
            }
        }
    }

    fn try_double_tap<S: GestureSink>(
        &mut self,
        ev: &api::TouchEvent,
        t_ms: u64,
        out: &mut S,
    ) -> bool {
        if let Some(prev) = self.last_tap {
            let dt = t_ms.saturating_sub(prev.time_ms);
            if dt <= self.cfg.double_tap_max_ms {
                let dx = ev.x - prev.x;
                let dy = ev.y - prev.y;
                let dist = (dx * dx + dy * dy).sqrt();
                if dist <= self.cfg.double_tap_max_move {
                    self.last_tap = None;
                    out.push_gesture(GestureEvent::DoubleTap { id: ev.id, x: ev.x, y: ev.y });
                    return true;
                }
            }
        }
        false
    }

    fn upsert_track(&mut self, id: api::TouchId, track: Track) {
        if let Some(existing) = self.track_mut(id) {
            *existing = track;
            return;
        }

        for slot in &mut self.tracks {
            if slot.is_none() {
                *slot = Some(GestureTrackSlot { id, track });
                return;
            }
        }

        self.overflow.push(GestureTrackSlot { id, track });
    }

    fn track_mut(&mut self, id: api::TouchId) -> Option<&mut Track> {
        for entry in self.tracks.iter_mut().flatten() {
            if entry.id == id {
                return Some(&mut entry.track);
            }
        }
        self.overflow.iter_mut().find(|entry| entry.id == id).map(|entry| &mut entry.track)
    }

    fn remove_track(&mut self, id: api::TouchId) -> Option<Track> {
        for slot in &mut self.tracks {
            if slot.as_ref().is_some_and(|entry| entry.id == id) {
                return slot.take().map(|entry| entry.track);
            }
        }

        self.overflow
            .iter()
            .position(|entry| entry.id == id)
            .map(|index| self.overflow.swap_remove(index).track)
    }
}

extern crate alloc;

trait GestureSink {
    fn push_gesture(&mut self, event: GestureEvent);
}

impl GestureSink for alloc::vec::Vec<GestureEvent> {
    #[inline]
    fn push_gesture(&mut self, event: GestureEvent) {
        self.push(event);
    }
}

impl GestureSink for alloc::vec::Vec<GestureOutcome> {
    #[inline]
    fn push_gesture(&mut self, event: GestureEvent) {
        self.push(GestureOutcome { haptic: default_haptic(&event), event });
    }
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
