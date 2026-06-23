use oxide_platform_api::{
    BleCacheEntry, BluetoothEvent, LocationEvent, LocationReading, MotionSample, PeripheralId,
    PermissionDomain, PermissionStatus, PushNotification, PushToken,
};
use parking_lot::Mutex;
use std::{
    collections::{BTreeMap, VecDeque},
    sync::Arc,
};

use super::{PermissionManager, PermissionState};
use crate::PermissionSubscription;

const DEFAULT_LOCATION_HISTORY_MAX: usize = 64;
const DEFAULT_LOCATION_MAX_AGE_MS: u64 = 60_000;
const DEFAULT_MOTION_HISTORY_MAX: usize = 64;
const DEFAULT_BLE_MAX_AGE_MS: u64 = 5 * 60_000;
const DEFAULT_PUSH_HISTORY_MAX: usize = 8;
const PERMISSION_DOMAIN_COUNT: usize = 8;

#[derive(Clone, Debug)]
pub struct SensorBridgeConfig {
    pub location_history_max: usize,
    pub location_max_age_ms: u64,
    pub motion_history_max: usize,
    pub bluetooth_max_age_ms: u64,
    pub push_history_max: usize,
    pub bluetooth_cache_max: usize,
}

impl Default for SensorBridgeConfig {
    fn default() -> Self {
        Self {
            location_history_max: DEFAULT_LOCATION_HISTORY_MAX,
            location_max_age_ms: DEFAULT_LOCATION_MAX_AGE_MS,
            motion_history_max: DEFAULT_MOTION_HISTORY_MAX,
            bluetooth_max_age_ms: DEFAULT_BLE_MAX_AGE_MS,
            push_history_max: DEFAULT_PUSH_HISTORY_MAX,
            bluetooth_cache_max: 128,
        }
    }
}

#[derive(Default)]
struct LocationState {
    last: Option<LocationReading>,
    history: VecDeque<LocationReading>,
}

#[derive(Default)]
struct MotionState {
    last: Option<MotionSample>,
    history: VecDeque<MotionSample>,
}

#[derive(Default)]
struct BluetoothState {
    powered_on: bool,
    devices: BTreeMap<PeripheralId, BleCacheEntry>,
}

#[derive(Default)]
struct PushState {
    token: Option<PushToken>,
    notifications: VecDeque<PushNotification>,
}

#[derive(Default)]
struct PermissionCache {
    statuses: [Option<PermissionStatus>; PERMISSION_DOMAIN_COUNT],
}

impl PermissionCache {
    #[inline]
    fn domain_index(domain: PermissionDomain) -> usize {
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

    #[inline]
    fn set(&mut self, domain: PermissionDomain, status: PermissionStatus) {
        self.statuses[Self::domain_index(domain)] = Some(status);
    }

    #[inline]
    fn get(&self, domain: PermissionDomain) -> Option<PermissionStatus> {
        self.statuses[Self::domain_index(domain)]
    }
}

pub struct SensorBridge {
    clock: Arc<dyn Fn() -> u64 + Send + Sync>,
    config: SensorBridgeConfig,
    location: Mutex<LocationState>,
    motion: Mutex<MotionState>,
    bluetooth: Mutex<BluetoothState>,
    push: Mutex<PushState>,
    permissions: Mutex<PermissionCache>,
}

impl SensorBridge {
    pub fn new_with_config(
        clock: Arc<dyn Fn() -> u64 + Send + Sync>,
        config: SensorBridgeConfig,
    ) -> Self {
        Self {
            clock,
            config,
            location: Mutex::new(LocationState::default()),
            motion: Mutex::new(MotionState::default()),
            bluetooth: Mutex::new(BluetoothState::default()),
            push: Mutex::new(PushState::default()),
            permissions: Mutex::new(PermissionCache::default()),
        }
    }

    pub fn new(clock: Arc<dyn Fn() -> u64 + Send + Sync>) -> Self {
        Self::with_clock(clock)
    }

    pub fn with_clock(clock: Arc<dyn Fn() -> u64 + Send + Sync>) -> Self {
        Self::new_with_config(clock, SensorBridgeConfig::default())
    }

    pub fn with_default_clock() -> Self {
        Self::with_clock(Arc::new(super::default_clock))
    }

    pub fn with_config(config: SensorBridgeConfig) -> Self {
        Self::new_with_config(Arc::new(super::default_clock), config)
    }

    #[inline]
    fn permission_allows(status: PermissionStatus) -> bool {
        matches!(status, PermissionStatus::Authorized | PermissionStatus::Limited)
    }

    fn permission_status_internal(&self, domain: PermissionDomain) -> Option<PermissionStatus> {
        self.permissions.lock().get(domain)
    }

    fn prune_location_locked(&self, state: &mut LocationState) {
        let limit = self.config.location_history_max;
        while state.history.len() > limit {
            let _ = state.history.pop_front();
        }
        if self.config.location_max_age_ms == 0 {
            return;
        }
        let now = (self.clock)();
        while let Some(front) = state.history.front() {
            if now.saturating_sub(front.timestamp_ms) > self.config.location_max_age_ms {
                let _ = state.history.pop_front();
            } else {
                break;
            }
        }
    }

    fn prune_motion_locked(&self, state: &mut MotionState) {
        let limit = self.config.motion_history_max;
        while state.history.len() > limit {
            let _ = state.history.pop_front();
        }
    }

    fn prune_bluetooth_locked(&self, state: &mut BluetoothState) {
        let now = (self.clock)();
        state.devices.retain(|_, entry| {
            now.saturating_sub(entry.last_seen_ms) <= self.config.bluetooth_max_age_ms
        });
        while state.devices.len() > self.config.bluetooth_cache_max {
            if let Some(oldest) =
                state.devices.iter().min_by_key(|(_, entry)| entry.last_seen_ms).map(|(id, _)| *id)
            {
                state.devices.remove(&oldest);
            } else {
                break;
            }
        }
    }

    fn prune_push_locked(&self, state: &mut PushState) {
        while state.notifications.len() > self.config.push_history_max {
            let _ = state.notifications.pop_front();
        }
    }

    pub fn permission_status(&self, domain: PermissionDomain) -> Option<PermissionStatus> {
        self.permission_status_internal(domain)
    }

    pub fn update_permission(&self, state: PermissionState) {
        {
            let mut cache = self.permissions.lock();
            cache.set(state.domain, state.status);
        }
        if !Self::permission_allows(state.status) {
            match state.domain {
                PermissionDomain::Location => {
                    let mut loc = self.location.lock();
                    loc.last = None;
                    loc.history.clear();
                }
                PermissionDomain::Motion => {
                    let mut motion = self.motion.lock();
                    motion.last = None;
                    motion.history.clear();
                }
                PermissionDomain::Bluetooth => {
                    let mut bt = self.bluetooth.lock();
                    bt.devices.clear();
                    bt.powered_on = false;
                }
                PermissionDomain::Notifications => {
                    let mut push = self.push.lock();
                    push.token = None;
                    push.notifications.clear();
                }
                _ => {}
            }
        }
    }

    pub fn bind_permissions(
        self: &Arc<Self>,
        manager: &Arc<PermissionManager>,
    ) -> SensorPermissionBinding {
        const DOMAINS: [PermissionDomain; 4] = [
            PermissionDomain::Location,
            PermissionDomain::Motion,
            PermissionDomain::Bluetooth,
            PermissionDomain::Notifications,
        ];
        let mut subs = Vec::new();
        for domain in DOMAINS {
            let weak = Arc::downgrade(self);
            let sub = manager.subscribe(domain, move |state| {
                if let Some(bridge) = weak.upgrade() {
                    bridge.update_permission(state);
                }
            });
            subs.push(sub);
        }
        for state in manager.snapshot() {
            if DOMAINS.contains(&state.domain) {
                self.update_permission(state);
            }
        }
        SensorPermissionBinding { _subs: subs }
    }

    pub fn handle_location_event(&self, event: LocationEvent) {
        if !self
            .permission_status_internal(PermissionDomain::Location)
            .map(Self::permission_allows)
            .unwrap_or(false)
        {
            return;
        }
        match event {
            LocationEvent::Update(reading) => {
                let mut loc = self.location.lock();
                loc.last = Some(reading);
                loc.history.push_back(reading);
                self.prune_location_locked(&mut loc);
            }
            LocationEvent::EnteredRegion(_)
            | LocationEvent::ExitedRegion(_)
            | LocationEvent::Error(_) => {}
        }
    }

    pub fn last_location(&self) -> Option<LocationReading> {
        self.location.lock().last
    }

    pub fn location_history(&self) -> Vec<LocationReading> {
        self.location.lock().history.iter().copied().collect()
    }

    pub fn handle_motion_sample(&self, sample: MotionSample) {
        if !self
            .permission_status_internal(PermissionDomain::Motion)
            .map(Self::permission_allows)
            .unwrap_or(false)
        {
            return;
        }
        let mut motion = self.motion.lock();
        motion.last = Some(sample);
        motion.history.push_back(sample);
        self.prune_motion_locked(&mut motion);
    }

    pub fn last_motion(&self) -> Option<MotionSample> {
        self.motion.lock().last
    }

    pub fn motion_history(&self) -> Vec<MotionSample> {
        self.motion.lock().history.iter().copied().collect()
    }

    pub fn handle_bluetooth_event(&self, event: BluetoothEvent) {
        if !self
            .permission_status_internal(PermissionDomain::Bluetooth)
            .map(Self::permission_allows)
            .unwrap_or(false)
        {
            if matches!(event, BluetoothEvent::StateChanged { powered_on } if !powered_on) {
                let mut bt = self.bluetooth.lock();
                bt.powered_on = false;
            }
            return;
        }
        let mut bt = self.bluetooth.lock();
        match event {
            BluetoothEvent::StateChanged { powered_on } => {
                bt.powered_on = powered_on;
                if !powered_on {
                    bt.devices.clear();
                }
            }
            BluetoothEvent::Discovered(info) => {
                let id = info.id;
                let entry = BleCacheEntry { peripheral: info, last_seen_ms: (self.clock)() };
                bt.devices.insert(id, entry);
            }
            BluetoothEvent::CacheUpdated(entry) => {
                bt.devices.insert(entry.peripheral.id, entry);
            }
            BluetoothEvent::Connected(id) | BluetoothEvent::Disconnected(id) => {
                if let Some(entry) = bt.devices.get_mut(&id) {
                    entry.last_seen_ms = (self.clock)();
                }
            }
            BluetoothEvent::Notified { id, .. } => {
                if let Some(entry) = bt.devices.get_mut(&id) {
                    entry.last_seen_ms = (self.clock)();
                }
            }
            BluetoothEvent::Restored(restoration) => {
                for peripheral in restoration.peripherals {
                    bt.devices.insert(
                        peripheral.id,
                        BleCacheEntry { peripheral, last_seen_ms: (self.clock)() },
                    );
                }
            }
        }
        self.prune_bluetooth_locked(&mut bt);
    }

    pub fn bluetooth_snapshot(&self) -> BluetoothSnapshot {
        let bt = self.bluetooth.lock();
        BluetoothSnapshot {
            powered_on: bt.powered_on,
            devices: bt.devices.values().cloned().collect(),
        }
    }

    pub fn prune_bluetooth(&self) {
        let mut bt = self.bluetooth.lock();
        self.prune_bluetooth_locked(&mut bt);
    }

    pub fn trim_memory(&self) {
        {
            let mut loc = self.location.lock();
            let keep = (self.config.location_history_max / 4).max(1);
            while loc.history.len() > keep {
                loc.history.pop_front();
            }
        }
        {
            let mut motion = self.motion.lock();
            let keep = (self.config.motion_history_max / 4).max(1);
            while motion.history.len() > keep {
                motion.history.pop_front();
            }
        }
        self.prune_bluetooth();
        {
            let mut push = self.push.lock();
            let keep = (self.config.push_history_max / 2).max(1);
            while push.notifications.len() > keep {
                push.notifications.pop_front();
            }
        }
    }

    pub fn set_push_token(&self, token: Option<PushToken>) {
        let allowed = self
            .permission_status_internal(PermissionDomain::Notifications)
            .map(Self::permission_allows)
            .unwrap_or(false);
        let mut push = self.push.lock();
        if allowed {
            push.token = token;
        } else if token.is_none() {
            push.token = None;
        }
    }

    pub fn handle_push_notification(&self, notification: PushNotification) {
        if !self
            .permission_status_internal(PermissionDomain::Notifications)
            .map(Self::permission_allows)
            .unwrap_or(false)
        {
            return;
        }
        let mut push = self.push.lock();
        push.notifications.push_back(notification);
        self.prune_push_locked(&mut push);
    }

    pub fn push_token(&self) -> Option<PushToken> {
        self.push.lock().token.clone()
    }

    pub fn push_notifications(&self) -> Vec<PushNotification> {
        self.push.lock().notifications.iter().cloned().collect()
    }

    pub fn snapshot(&self) -> SensorSnapshot {
        let location = {
            let lock = self.location.lock();
            LocationSnapshot { last: lock.last, history: lock.history.iter().copied().collect() }
        };
        let motion = {
            let lock = self.motion.lock();
            MotionSnapshot { last: lock.last, history: lock.history.iter().copied().collect() }
        };
        let bluetooth = self.bluetooth_snapshot();
        let push = {
            let lock = self.push.lock();
            PushSnapshot {
                token: lock.token.clone(),
                notifications: lock.notifications.iter().cloned().collect(),
            }
        };
        SensorSnapshot { location, motion, bluetooth, push }
    }
}

pub struct SensorPermissionBinding {
    _subs: Vec<PermissionSubscription>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct LocationSnapshot {
    pub last: Option<LocationReading>,
    pub history: Vec<LocationReading>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MotionSnapshot {
    pub last: Option<MotionSample>,
    pub history: Vec<MotionSample>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct BluetoothSnapshot {
    pub powered_on: bool,
    pub devices: Vec<BleCacheEntry>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PushSnapshot {
    pub token: Option<PushToken>,
    pub notifications: Vec<PushNotification>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SensorSnapshot {
    pub location: LocationSnapshot,
    pub motion: MotionSnapshot,
    pub bluetooth: BluetoothSnapshot,
    pub push: PushSnapshot,
}
