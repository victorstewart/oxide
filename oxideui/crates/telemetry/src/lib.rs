#![forbid(unsafe_code)]
#![allow(clippy::missing_errors_doc, clippy::missing_panics_doc)]

pub mod crash_reporting;

use oxideui_networking::{
    QuicSessionMetrics, ReachabilitySnapshot, ReachabilityState, SessionPhase,
};
use oxideui_permissions::{sensors::SensorSnapshot, PermissionState};
use oxideui_platform_api::{PermissionDomain, PermissionStatus};
use parking_lot::Mutex;
use std::{
    collections::{HashMap, VecDeque},
    sync::{Arc, Weak},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TelemetryHealth {
    Offline,
    Degraded,
    Nominal,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TelemetryLifecycleState {
    ColdStart,
    Foreground,
    Background,
    Suspended,
}

impl Default for TelemetryLifecycleState {
    fn default() -> Self {
        Self::ColdStart
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TelemetryOpsStatus {
    pub lifecycle: TelemetryLifecycleState,
    pub last_transition_ms: u64,
    pub background_count: u32,
    pub recovery_actions: u32,
}

impl Default for TelemetryOpsStatus {
    fn default() -> Self {
        Self {
            lifecycle: TelemetryLifecycleState::default(),
            last_transition_ms: 0,
            background_count: 0,
            recovery_actions: 0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MemoryPressureLevel {
    Nominal,
    Warning,
    Critical,
}

impl Default for MemoryPressureLevel {
    fn default() -> Self {
        Self::Nominal
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct TelemetrySnapshot {
    pub permissions: Vec<PermissionState>,
    pub reachability: ReachabilitySnapshot,
    pub sensors: Option<SensorSnapshot>,
    pub network: Option<QuicSessionMetrics>,
    pub health: TelemetryHealth,
    pub operations: TelemetryOpsStatus,
    pub memory_pressure: MemoryPressureLevel,
}

impl Default for TelemetrySnapshot {
    fn default() -> Self {
        Self {
            permissions: Vec::new(),
            reachability: ReachabilitySnapshot::default(),
            sensors: None,
            network: None,
            health: TelemetryHealth::Offline,
            operations: TelemetryOpsStatus::default(),
            memory_pressure: MemoryPressureLevel::default(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TelemetryOpsMetrics {
    pub status: TelemetryOpsStatus,
    pub paused_sensors: bool,
    pub paused_network: bool,
    pub pending_commands: usize,
    pub memory_pressure: MemoryPressureLevel,
}

impl Default for TelemetryOpsMetrics {
    fn default() -> Self {
        Self {
            status: TelemetryOpsStatus::default(),
            paused_sensors: false,
            paused_network: false,
            pending_commands: 0,
            memory_pressure: MemoryPressureLevel::default(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TelemetryUpdateKind {
    Initial,
    Permissions,
    Reachability,
    Sensors,
    Network,
    Lifecycle,
    Memory,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TelemetryAction {
    PauseSensors,
    ResumeSensors,
    PauseNetworking,
    ResumeNetworking,
    RefreshPermissions,
    FlushMetrics,
    TrimCaches,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TelemetryCommandReason {
    EnterBackground,
    EnterForeground,
    HealthDegraded,
    ScheduledRecovery,
    GracefulShutdown,
    MemoryPressure(MemoryPressureLevel),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TelemetryCommand {
    pub action: TelemetryAction,
    pub reason: TelemetryCommandReason,
}

impl TelemetryCommand {
    #[must_use]
    pub const fn new(action: TelemetryAction, reason: TelemetryCommandReason) -> Self {
        Self { action, reason }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum TelemetryEvent {
    Snapshot { kind: TelemetryUpdateKind, snapshot: TelemetrySnapshot },
    HealthChanged { from: TelemetryHealth, to: TelemetryHealth, snapshot: TelemetrySnapshot },
    Operations { command: TelemetryCommand, metrics: TelemetryOpsMetrics },
}

#[derive(Clone, Debug, PartialEq)]
pub struct TelemetryEventRecord {
    pub id: u64,
    pub event: TelemetryEvent,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TelemetryRateLimitMetrics {
    pub dropped_events: u64,
    pub events_this_second: usize,
    pub batch_size: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TelemetryConfig {
    pub history_capacity: usize,
    /// Maximum events per second (0 = unlimited)
    pub rate_limit_per_second: usize,
    /// Batch size for batched events
    pub batch_size: usize,
    /// Flush interval in milliseconds
    pub flush_interval_ms: u64,
}

impl Default for TelemetryConfig {
    fn default() -> Self {
        Self {
            history_capacity: 256,
            rate_limit_per_second: 100,  // 100 events per second
            batch_size: 50,              // Batch up to 50 events
            flush_interval_ms: 1000,     // Flush every second
        }
    }
}

type TelemetryCallback = Arc<dyn Fn(TelemetryEvent) + Send + Sync>;

struct TelemetryInner {
    snapshot: TelemetrySnapshot,
    listeners: HashMap<u64, TelemetryCallback>,
    next_listener_id: u64,
    history: VecDeque<TelemetryEventRecord>,
    history_capacity: usize,
    next_event_id: u64,
    // Rate limiting
    config: TelemetryConfig,
    event_batch: Vec<TelemetryEvent>,
    events_this_second: usize,
    last_rate_check_ms: u64,
    last_flush_ms: u64,
    dropped_events: u64,
}

impl TelemetryInner {
    fn new(snapshot: TelemetrySnapshot, config: TelemetryConfig) -> Self {
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);

        let mut inner = Self {
            snapshot,
            listeners: HashMap::new(),
            next_listener_id: 1,
            history: VecDeque::new(),
            history_capacity: config.history_capacity,
            next_event_id: 1,
            config,
            event_batch: Vec::with_capacity(config.batch_size),
            events_this_second: 0,
            last_rate_check_ms: now_ms,
            last_flush_ms: now_ms,
            dropped_events: 0,
        };
        inner.record_event(TelemetryEvent::Snapshot {
            kind: TelemetryUpdateKind::Initial,
            snapshot: inner.snapshot.clone(),
        });
        inner
    }

    fn record_event(&mut self, event: TelemetryEvent) {
        // Check rate limit
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);

        // Reset counter every second
        if now_ms - self.last_rate_check_ms >= 1000 {
            self.events_this_second = 0;
            self.last_rate_check_ms = now_ms;
        }

        // Apply rate limiting
        if self.config.rate_limit_per_second > 0 {
            if self.events_this_second >= self.config.rate_limit_per_second {
                self.dropped_events = self.dropped_events.saturating_add(1);
                return; // Drop event
            }
            self.events_this_second += 1;
        }

        // Add to batch
        if self.config.batch_size > 0 {
            self.event_batch.push(event.clone());

            // Check if batch is full or flush interval reached
            let should_flush = self.event_batch.len() >= self.config.batch_size
                || (self.config.flush_interval_ms > 0
                    && now_ms - self.last_flush_ms >= self.config.flush_interval_ms);

            if should_flush {
                self.flush_batch();
                self.last_flush_ms = now_ms;
            }
        }

        // Record in history
        if self.history_capacity == 0 {
            self.next_event_id = self.next_event_id.saturating_add(1);
            return;
        }
        let id = self.next_event_id;
        self.next_event_id = self.next_event_id.saturating_add(1);
        if self.history.len() == self.history_capacity {
            self.history.pop_front();
        }
        self.history.push_back(TelemetryEventRecord { id, event });
    }

    fn flush_batch(&mut self) {
        if self.event_batch.is_empty() {
            return;
        }

        // Process batched events (send to listeners)
        for event in self.event_batch.drain(..) {
            for listener in self.listeners.values() {
                listener(event.clone());
            }
        }
    }

    fn get_metrics(&self) -> TelemetryRateLimitMetrics {
        TelemetryRateLimitMetrics {
            dropped_events: self.dropped_events,
            events_this_second: self.events_this_second,
            batch_size: self.event_batch.len(),
        }
    }
}

#[derive(Clone)]
pub struct TelemetryHub {
    inner: Arc<Mutex<TelemetryInner>>,
}

impl TelemetryHub {
    #[must_use]
    pub fn new() -> Self {
        Self::with_config(TelemetryConfig::default())
    }

    #[must_use]
    pub fn with_config(config: TelemetryConfig) -> Self {
        Self {
            inner: Arc::new(Mutex::new(TelemetryInner::new(TelemetrySnapshot::default(), config))),
        }
    }

    #[must_use]
    pub fn snapshot(&self) -> TelemetrySnapshot {
        self.inner.lock().snapshot.clone()
    }

    #[must_use]
    pub fn history(&self) -> Vec<TelemetryEventRecord> {
        let inner = self.inner.lock();
        inner.history.iter().cloned().collect()
    }

    #[must_use]
    pub fn history_since(&self, after_id: u64) -> Vec<TelemetryEventRecord> {
        let inner = self.inner.lock();
        inner.history.iter().filter(|record| record.id > after_id).cloned().collect()
    }

    /// Get rate limiting metrics
    #[must_use]
    pub fn rate_limit_metrics(&self) -> TelemetryRateLimitMetrics {
        self.inner.lock().get_metrics()
    }

    /// Manually flush the event batch
    pub fn flush(&self) {
        self.inner.lock().flush_batch();
    }

    pub fn subscribe<F>(&self, listener: F) -> TelemetrySubscription
    where
        F: Fn(TelemetryEvent) + Send + Sync + 'static,
    {
        let callback: TelemetryCallback = Arc::new(listener);
        let (id, snapshot) = {
            let mut inner = self.inner.lock();
            let id = inner.next_listener_id;
            inner.next_listener_id = inner.next_listener_id.saturating_add(1);
            inner.listeners.insert(id, Arc::clone(&callback));
            (id, inner.snapshot.clone())
        };
        callback(TelemetryEvent::Snapshot { kind: TelemetryUpdateKind::Initial, snapshot });
        TelemetrySubscription { id, inner: Arc::clone(&self.inner) }
    }

    pub fn update_permissions(&self, permissions: Vec<PermissionState>) {
        let canonical = canonicalize_permissions(permissions);
        self.mutate(TelemetryUpdateKind::Permissions, move |snapshot| {
            if snapshot.permissions == canonical {
                return false;
            }
            snapshot.permissions = canonical;
            true
        });
    }

    pub fn update_reachability(&self, reachability: ReachabilitySnapshot) {
        self.mutate(TelemetryUpdateKind::Reachability, move |snapshot| {
            if snapshot.reachability == reachability {
                return false;
            }
            snapshot.reachability = reachability;
            true
        });
    }

    pub fn update_sensors(&self, sensors: Option<SensorSnapshot>) {
        self.mutate(TelemetryUpdateKind::Sensors, move |snapshot| {
            if snapshot.sensors == sensors {
                return false;
            }
            snapshot.sensors = sensors;
            true
        });
    }

    pub fn update_network_metrics(&self, metrics: Option<QuicSessionMetrics>) {
        self.mutate(TelemetryUpdateKind::Network, move |snapshot| {
            if snapshot.network == metrics {
                return false;
            }
            snapshot.network = metrics;
            true
        });
    }

    pub fn update_memory_pressure(&self, pressure: MemoryPressureLevel) {
        self.mutate(TelemetryUpdateKind::Memory, move |snapshot| {
            if snapshot.memory_pressure == pressure {
                return false;
            }
            snapshot.memory_pressure = pressure;
            true
        });
    }

    pub fn update_operations(&self, status: TelemetryOpsStatus) {
        self.mutate(TelemetryUpdateKind::Lifecycle, move |snapshot| {
            if snapshot.operations == status {
                return false;
            }
            snapshot.operations = status;
            true
        });
    }

    pub fn emit_operations(&self, command: TelemetryCommand, metrics: TelemetryOpsMetrics) {
        let callbacks = {
            let mut inner = self.inner.lock();
            inner.record_event(TelemetryEvent::Operations {
                command: command.clone(),
                metrics: metrics.clone(),
            });
            inner.listeners.values().map(Arc::clone).collect::<Vec<_>>()
        };
        for cb in callbacks {
            cb(TelemetryEvent::Operations { command: command.clone(), metrics: metrics.clone() });
        }
    }

    fn mutate(
        &self,
        kind: TelemetryUpdateKind,
        mutate: impl FnOnce(&mut TelemetrySnapshot) -> bool,
    ) {
        let (snapshot_event, health_event, callbacks) = {
            let mut inner = self.inner.lock();
            let mut next_snapshot = inner.snapshot.clone();
            if !mutate(&mut next_snapshot) {
                return;
            }
            let prev_health = inner.snapshot.health;
            next_snapshot.health = compute_health(&next_snapshot);
            let health_event = if next_snapshot.health != prev_health {
                Some(TelemetryEvent::HealthChanged {
                    from: prev_health,
                    to: next_snapshot.health,
                    snapshot: next_snapshot.clone(),
                })
            } else {
                None
            };
            inner.snapshot = next_snapshot.clone();
            let snapshot_event = TelemetryEvent::Snapshot { kind, snapshot: next_snapshot.clone() };
            inner.record_event(snapshot_event.clone());
            if let Some(event) = &health_event {
                inner.record_event(event.clone());
            }
            let callbacks = inner.listeners.values().map(Arc::clone).collect::<Vec<_>>();
            (snapshot_event, health_event, callbacks)
        };
        for cb in &callbacks {
            cb(snapshot_event.clone());
        }
        if let Some(event) = health_event {
            for cb in callbacks {
                cb(event.clone());
            }
        }
    }
}

pub struct TelemetrySubscription {
    id: u64,
    inner: Arc<Mutex<TelemetryInner>>,
}

impl Drop for TelemetrySubscription {
    fn drop(&mut self) {
        let mut inner = self.inner.lock();
        inner.listeners.remove(&self.id);
    }
}

struct TelemetryOpsInner {
    status: TelemetryOpsStatus,
    paused_sensors: bool,
    paused_network: bool,
    memory_pressure: MemoryPressureLevel,
    pending_recoveries: Vec<TelemetryCommand>,
    command_queue: VecDeque<TelemetryCommand>,
}

impl TelemetryOpsInner {
    fn new() -> Self {
        Self {
            status: TelemetryOpsStatus::default(),
            paused_sensors: false,
            paused_network: false,
            memory_pressure: MemoryPressureLevel::default(),
            pending_recoveries: Vec::new(),
            command_queue: VecDeque::new(),
        }
    }

    fn metrics(&self) -> TelemetryOpsMetrics {
        TelemetryOpsMetrics {
            status: self.status,
            paused_sensors: self.paused_sensors,
            paused_network: self.paused_network,
            pending_commands: self.command_queue.len() + self.pending_recoveries.len(),
            memory_pressure: self.memory_pressure,
        }
    }

    fn push_pending(&mut self, command: TelemetryCommand) {
        if !self.pending_recoveries.contains(&command) {
            self.pending_recoveries.push(command);
        }
    }
}

fn push_unique(vec: &mut Vec<TelemetryCommand>, command: TelemetryCommand) {
    if !vec.contains(&command) {
        vec.push(command);
    }
}

pub struct TelemetryOperations {
    hub: Arc<TelemetryHub>,
    inner: Arc<Mutex<TelemetryOpsInner>>,
    subscription: Mutex<Option<TelemetrySubscription>>,
}

impl TelemetryOperations {
    #[must_use]
    pub fn new(hub: Arc<TelemetryHub>) -> Arc<Self> {
        let ops = Arc::new(Self {
            hub: Arc::clone(&hub),
            inner: Arc::new(Mutex::new(TelemetryOpsInner::new())),
            subscription: Mutex::new(None),
        });
        TelemetryOperations::install_subscription(&ops);
        hub.update_operations(ops.status());
        hub.update_memory_pressure(MemoryPressureLevel::default());
        ops
    }

    fn install_subscription(this: &Arc<Self>) {
        let weak_ops: Weak<Self> = Arc::downgrade(this);
        let sub = this.hub.subscribe(move |event| {
            if let Some(strong) = weak_ops.upgrade() {
                strong.handle_hub_event(event);
            }
        });
        *this.subscription.lock() = Some(sub);
    }

    pub fn handle_background(&self, now_ms: u64) {
        let (status, commands) = {
            let mut inner = self.inner.lock();
            if inner.status.lifecycle == TelemetryLifecycleState::Background {
                return;
            }
            inner.status.lifecycle = TelemetryLifecycleState::Background;
            inner.status.last_transition_ms = now_ms;
            inner.status.background_count = inner.status.background_count.saturating_add(1);
            let mut commands = Vec::new();
            if !inner.paused_sensors {
                inner.paused_sensors = true;
                push_unique(
                    &mut commands,
                    TelemetryCommand::new(
                        TelemetryAction::PauseSensors,
                        TelemetryCommandReason::EnterBackground,
                    ),
                );
            }
            if !inner.paused_network {
                inner.paused_network = true;
                push_unique(
                    &mut commands,
                    TelemetryCommand::new(
                        TelemetryAction::PauseNetworking,
                        TelemetryCommandReason::EnterBackground,
                    ),
                );
            }
            push_unique(
                &mut commands,
                TelemetryCommand::new(
                    TelemetryAction::FlushMetrics,
                    TelemetryCommandReason::EnterBackground,
                ),
            );
            (inner.status, commands)
        };
        self.hub.update_operations(status);
        for command in commands {
            self.enqueue_command(command);
        }
    }

    pub fn handle_foreground(&self, now_ms: u64) {
        let (changed, status, commands) = {
            let mut inner = self.inner.lock();
            let was_foreground = inner.status.lifecycle == TelemetryLifecycleState::Foreground;
            if !was_foreground {
                inner.status.lifecycle = TelemetryLifecycleState::Foreground;
                inner.status.last_transition_ms = now_ms;
            }
            let mut commands = Vec::new();
            if inner.paused_network {
                let has_pending = inner
                    .pending_recoveries
                    .iter()
                    .any(|cmd| cmd.action == TelemetryAction::ResumeNetworking);
                inner.paused_network = false;
                if !has_pending {
                    push_unique(
                        &mut commands,
                        TelemetryCommand::new(
                            TelemetryAction::ResumeNetworking,
                            TelemetryCommandReason::EnterForeground,
                        ),
                    );
                }
            }
            if inner.paused_sensors {
                let has_pending = inner
                    .pending_recoveries
                    .iter()
                    .any(|cmd| cmd.action == TelemetryAction::ResumeSensors);
                inner.paused_sensors = false;
                if !has_pending {
                    push_unique(
                        &mut commands,
                        TelemetryCommand::new(
                            TelemetryAction::ResumeSensors,
                            TelemetryCommandReason::EnterForeground,
                        ),
                    );
                }
            }
            for pending in inner.pending_recoveries.drain(..) {
                push_unique(&mut commands, pending);
            }
            if !commands.is_empty() || !was_foreground {
                push_unique(
                    &mut commands,
                    TelemetryCommand::new(
                        TelemetryAction::FlushMetrics,
                        TelemetryCommandReason::EnterForeground,
                    ),
                );
            }
            (!was_foreground, inner.status, commands)
        };
        if changed {
            self.hub.update_operations(status);
        } else if !commands.is_empty() {
            self.hub.update_operations(status);
        }
        for command in commands {
            self.enqueue_command(command);
        }
    }

    pub fn handle_memory_pressure(&self, now_ms: u64, level: MemoryPressureLevel) {
        let (status, commands) = {
            let mut inner = self.inner.lock();
            inner.memory_pressure = level;
            let mut commands = Vec::new();
            match level {
                MemoryPressureLevel::Nominal => {
                    if inner.status.lifecycle == TelemetryLifecycleState::Foreground {
                        if inner.paused_network {
                            inner.paused_network = false;
                            push_unique(
                                &mut commands,
                                TelemetryCommand::new(
                                    TelemetryAction::ResumeNetworking,
                                    TelemetryCommandReason::MemoryPressure(level),
                                ),
                            );
                        }
                        if inner.paused_sensors {
                            inner.paused_sensors = false;
                            push_unique(
                                &mut commands,
                                TelemetryCommand::new(
                                    TelemetryAction::ResumeSensors,
                                    TelemetryCommandReason::MemoryPressure(level),
                                ),
                            );
                        }
                        push_unique(
                            &mut commands,
                            TelemetryCommand::new(
                                TelemetryAction::FlushMetrics,
                                TelemetryCommandReason::MemoryPressure(level),
                            ),
                        );
                    }
                }
                MemoryPressureLevel::Warning => {
                    push_unique(
                        &mut commands,
                        TelemetryCommand::new(
                            TelemetryAction::TrimCaches,
                            TelemetryCommandReason::MemoryPressure(level),
                        ),
                    );
                    push_unique(
                        &mut commands,
                        TelemetryCommand::new(
                            TelemetryAction::FlushMetrics,
                            TelemetryCommandReason::MemoryPressure(level),
                        ),
                    );
                }
                MemoryPressureLevel::Critical => {
                    if !inner.paused_sensors {
                        inner.paused_sensors = true;
                        push_unique(
                            &mut commands,
                            TelemetryCommand::new(
                                TelemetryAction::PauseSensors,
                                TelemetryCommandReason::MemoryPressure(level),
                            ),
                        );
                    }
                    if !inner.paused_network {
                        inner.paused_network = true;
                        push_unique(
                            &mut commands,
                            TelemetryCommand::new(
                                TelemetryAction::PauseNetworking,
                                TelemetryCommandReason::MemoryPressure(level),
                            ),
                        );
                    }
                    push_unique(
                        &mut commands,
                        TelemetryCommand::new(
                            TelemetryAction::TrimCaches,
                            TelemetryCommandReason::MemoryPressure(level),
                        ),
                    );
                    push_unique(
                        &mut commands,
                        TelemetryCommand::new(
                            TelemetryAction::FlushMetrics,
                            TelemetryCommandReason::MemoryPressure(level),
                        ),
                    );
                }
            }
            inner.status.last_transition_ms = now_ms;
            (inner.status, commands)
        };
        self.hub.update_memory_pressure(level);
        self.hub.update_operations(status);
        for command in commands {
            self.enqueue_command(command);
        }
    }

    pub fn handle_shutdown(&self, now_ms: u64) {
        let (status, commands) = {
            let mut inner = self.inner.lock();
            inner.status.lifecycle = TelemetryLifecycleState::Suspended;
            inner.status.last_transition_ms = now_ms;
            inner.paused_sensors = true;
            inner.paused_network = true;
            inner.pending_recoveries.clear();
            let mut commands = Vec::new();
            push_unique(
                &mut commands,
                TelemetryCommand::new(
                    TelemetryAction::PauseSensors,
                    TelemetryCommandReason::GracefulShutdown,
                ),
            );
            push_unique(
                &mut commands,
                TelemetryCommand::new(
                    TelemetryAction::PauseNetworking,
                    TelemetryCommandReason::GracefulShutdown,
                ),
            );
            push_unique(
                &mut commands,
                TelemetryCommand::new(
                    TelemetryAction::FlushMetrics,
                    TelemetryCommandReason::GracefulShutdown,
                ),
            );
            (inner.status, commands)
        };
        self.hub.update_operations(status);
        for command in commands {
            self.enqueue_command(command);
        }
    }

    pub fn drain_commands(&self) -> Vec<TelemetryCommand> {
        let mut inner = self.inner.lock();
        inner.command_queue.drain(..).collect()
    }

    #[must_use]
    pub fn status(&self) -> TelemetryOpsStatus {
        self.inner.lock().status
    }

    #[must_use]
    pub fn metrics(&self) -> TelemetryOpsMetrics {
        self.inner.lock().metrics()
    }

    fn handle_hub_event(&self, event: TelemetryEvent) {
        if let TelemetryEvent::HealthChanged { to, snapshot, .. } = event {
            self.handle_health_changed(to, &snapshot);
        }
    }

    fn handle_health_changed(&self, to: TelemetryHealth, snapshot: &TelemetrySnapshot) {
        match to {
            TelemetryHealth::Nominal => {
                let mut inner = self.inner.lock();
                inner.pending_recoveries.clear();
            }
            TelemetryHealth::Offline | TelemetryHealth::Degraded => {
                let lifecycle = self.inner.lock().status.lifecycle;
                let commands = recovery_commands(snapshot, to);
                if commands.is_empty() {
                    return;
                }
                if lifecycle == TelemetryLifecycleState::Foreground {
                    for command in commands {
                        self.enqueue_command(command);
                    }
                    self.enqueue_command(TelemetryCommand::new(
                        TelemetryAction::FlushMetrics,
                        TelemetryCommandReason::HealthDegraded,
                    ));
                } else {
                    let mut inner = self.inner.lock();
                    for command in commands {
                        inner.push_pending(command);
                    }
                }
            }
        }
    }

    fn enqueue_command(&self, command: TelemetryCommand) {
        let (status, metrics) = {
            let mut inner = self.inner.lock();
            inner.command_queue.push_back(command.clone());
            if matches!(
                command.reason,
                TelemetryCommandReason::HealthDegraded
                    | TelemetryCommandReason::ScheduledRecovery
                    | TelemetryCommandReason::MemoryPressure(MemoryPressureLevel::Critical)
            ) {
                inner.status.recovery_actions = inner.status.recovery_actions.saturating_add(1);
            }
            (inner.status, inner.metrics())
        };
        self.hub.update_operations(status);
        self.hub.emit_operations(command, metrics);
    }
}

fn recovery_commands(snapshot: &TelemetrySnapshot, to: TelemetryHealth) -> Vec<TelemetryCommand> {
    let mut commands = Vec::new();
    push_unique(
        &mut commands,
        TelemetryCommand::new(
            TelemetryAction::RefreshPermissions,
            TelemetryCommandReason::HealthDegraded,
        ),
    );
    let sensors_unhealthy = snapshot
        .sensors
        .as_ref()
        .map(|s| s.location.last.is_none() || !s.bluetooth.powered_on || s.push.token.is_none())
        .unwrap_or(true);
    if sensors_unhealthy {
        push_unique(
            &mut commands,
            TelemetryCommand::new(
                TelemetryAction::ResumeSensors,
                TelemetryCommandReason::HealthDegraded,
            ),
        );
    }
    let network_unhealthy = snapshot
        .network
        .as_ref()
        .map(|n| matches!(n.phase, SessionPhase::Failed { .. }))
        .unwrap_or(true)
        || matches!(snapshot.reachability.state, ReachabilityState::Offline);
    if network_unhealthy || matches!(to, TelemetryHealth::Offline) {
        push_unique(
            &mut commands,
            TelemetryCommand::new(
                TelemetryAction::ResumeNetworking,
                TelemetryCommandReason::HealthDegraded,
            ),
        );
    }
    commands
}

fn compute_health(snapshot: &TelemetrySnapshot) -> TelemetryHealth {
    if matches!(snapshot.reachability.state, ReachabilityState::Offline) {
        return TelemetryHealth::Offline;
    }

    let mut degraded = false;
    for perm in &snapshot.permissions {
        if is_critical_permission(perm.domain)
            && !matches!(perm.status, PermissionStatus::Authorized)
        {
            degraded = true;
            break;
        }
    }

    if !degraded {
        if let Some(sensors) = &snapshot.sensors {
            if sensors.location.last.is_none()
                || !sensors.bluetooth.powered_on
                || sensors.push.token.is_none()
            {
                degraded = true;
            }
        } else {
            degraded = true;
        }
    }

    if !degraded {
        if let Some(network) = &snapshot.network {
            if matches!(network.phase, SessionPhase::Failed { .. }) {
                degraded = true;
            }
        } else {
            degraded = true;
        }
    }

    if degraded {
        TelemetryHealth::Degraded
    } else {
        TelemetryHealth::Nominal
    }
}

fn is_critical_permission(domain: PermissionDomain) -> bool {
    matches!(
        domain,
        PermissionDomain::Camera
            | PermissionDomain::Microphone
            | PermissionDomain::Location
            | PermissionDomain::Bluetooth
            | PermissionDomain::Motion
            | PermissionDomain::Notifications
    )
}

fn canonicalize_permissions(states: Vec<PermissionState>) -> Vec<PermissionState> {
    let mut by_domain: HashMap<PermissionDomain, PermissionState> = HashMap::new();
    for state in states {
        by_domain
            .entry(state.domain)
            .and_modify(|existing| {
                if existing.last_changed_ms <= state.last_changed_ms {
                    *existing = state;
                }
            })
            .or_insert(state);
    }
    let mut result: Vec<PermissionState> = by_domain.into_values().collect();
    result.sort_by(|a, b| permission_order_key(a.domain).cmp(&permission_order_key(b.domain)));
    result
}

fn permission_order_key(domain: PermissionDomain) -> u8 {
    match domain {
        PermissionDomain::Notifications => 0,
        PermissionDomain::Location => 1,
        PermissionDomain::Camera => 2,
        PermissionDomain::Contacts => 3,
        PermissionDomain::Bluetooth => 4,
        PermissionDomain::Motion => 5,
        PermissionDomain::Microphone => 6,
        PermissionDomain::MediaLibrary => 7,
    }
}
