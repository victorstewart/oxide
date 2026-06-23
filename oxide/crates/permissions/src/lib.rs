#![forbid(unsafe_code)]
#![allow(clippy::missing_errors_doc, clippy::missing_panics_doc)]

use oxide_platform_api::{PermissionDomain, PermissionStatus, Permissions};
use parking_lot::Mutex;
use std::{collections::HashMap, sync::Arc};

type PermissionCallback = Arc<dyn Fn(PermissionState) + Send + Sync>;
type PermissionListeners = HashMap<PermissionDomain, Vec<(u64, PermissionCallback)>>;
const PERMISSION_DOMAIN_COUNT: usize = 8;

pub mod sensors;
pub use sensors::SensorBridge;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PermissionState {
    pub domain: PermissionDomain,
    pub status: PermissionStatus,
    pub last_changed_ms: u64,
}

impl PermissionState {
    pub fn new(domain: PermissionDomain, status: PermissionStatus, timestamp_ms: u64) -> Self {
        Self { domain, status, last_changed_ms: timestamp_ms }
    }
}

#[derive(Clone)]
pub struct PermissionManager {
    permissions: Arc<dyn Permissions + Send + Sync>,
    clock: Arc<dyn Fn() -> u64 + Send + Sync>,
    inner: Arc<Mutex<PermissionInner>>,
}

struct PermissionInner {
    states: PermissionStates,
    listeners: PermissionListeners,
    next_listener_id: u64,
}

impl PermissionInner {
    fn new() -> Self {
        Self { states: PermissionStates::default(), listeners: HashMap::new(), next_listener_id: 1 }
    }
}

#[derive(Default)]
struct PermissionStates {
    slots: [Option<PermissionState>; PERMISSION_DOMAIN_COUNT],
}

impl PermissionStates {
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
    fn insert(&mut self, state: PermissionState) {
        self.slots[Self::domain_index(state.domain)] = Some(state);
    }

    #[inline]
    fn get(&self, domain: PermissionDomain) -> Option<PermissionState> {
        self.slots[Self::domain_index(domain)]
    }

    fn values(&self) -> impl Iterator<Item = PermissionState> + '_ {
        self.slots.iter().filter_map(|state| *state)
    }
}

pub struct PermissionSubscription {
    domain: PermissionDomain,
    id: u64,
    inner: Arc<Mutex<PermissionInner>>,
}

impl Drop for PermissionSubscription {
    fn drop(&mut self) {
        let mut inner = self.inner.lock();
        if let Some(list) = inner.listeners.get_mut(&self.domain) {
            list.retain(|(id, _)| *id != self.id);
        }
    }
}

impl PermissionManager {
    pub fn new(
        permissions: Arc<dyn Permissions + Send + Sync>,
        clock: Arc<dyn Fn() -> u64 + Send + Sync>,
    ) -> Self {
        let inner = Arc::new(Mutex::new(PermissionInner::new()));
        let manager = Self {
            permissions: permissions.clone(),
            clock: Arc::clone(&clock),
            inner: inner.clone(),
        };
        let weak_inner = Arc::downgrade(&inner);
        let clock_cb = Arc::clone(&manager.clock);
        permissions.subscribe(Box::new(move |domain, status| {
            if let Some(upgraded) = weak_inner.upgrade() {
                let mut guard = upgraded.lock();
                let now = clock_cb();
                let state = PermissionState::new(domain, status, now);
                guard.states.insert(state);
                let callbacks: Vec<_> = guard
                    .listeners
                    .get(&domain)
                    .map(|ls| ls.iter().map(|(_, cb)| Arc::clone(cb)).collect())
                    .unwrap_or_default();
                drop(guard);
                for cb in callbacks {
                    cb(state);
                }
            }
        }));
        manager
    }

    pub fn with_default_clock(permissions: Arc<dyn Permissions + Send + Sync>) -> Self {
        Self::new(permissions, Arc::new(default_clock))
    }

    pub fn status(&self, domain: PermissionDomain) -> PermissionStatus {
        {
            let guard = self.inner.lock();
            if let Some(state) = guard.states.get(domain) {
                return state.status;
            }
        }
        let status = self.permissions.status(domain);
        let now = (self.clock)();
        let mut guard = self.inner.lock();
        guard.states.insert(PermissionState::new(domain, status, now));
        status
    }

    pub fn request(&self, domain: PermissionDomain) {
        self.permissions.request(domain);
    }

    pub fn snapshot(&self) -> Vec<PermissionState> {
        let guard = self.inner.lock();
        guard.states.values().collect()
    }

    pub fn subscribe<F>(&self, domain: PermissionDomain, callback: F) -> PermissionSubscription
    where
        F: Fn(PermissionState) + Send + Sync + 'static,
    {
        let _ = self.status(domain);
        let mut guard = self.inner.lock();
        let id = guard.next_listener_id;
        guard.next_listener_id = guard.next_listener_id.saturating_add(1);
        let entry = guard.listeners.entry(domain).or_default();
        let arc_cb: PermissionCallback = Arc::new(callback);
        entry.push((id, Arc::clone(&arc_cb)));
        let current = guard.states.get(domain);
        drop(guard);
        if let Some(state) = current {
            arc_cb(state);
        }
        PermissionSubscription { domain, id, inner: Arc::clone(&self.inner) }
    }
}

fn default_clock() -> u64 {
    use std::time::{Duration, SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_else(|_| Duration::from_millis(0))
        .as_millis() as u64
}
