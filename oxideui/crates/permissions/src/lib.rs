#![forbid(unsafe_code)]
#![allow(clippy::missing_errors_doc, clippy::missing_panics_doc)]

use oxideui_platform_api::{PermissionDomain, PermissionStatus, Permissions};
use parking_lot::Mutex;
use std::{collections::HashMap, sync::Arc};

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
    states: HashMap<PermissionDomain, PermissionState>,
    listeners: HashMap<PermissionDomain, Vec<(u64, Arc<dyn Fn(PermissionState) + Send + Sync>)>>,
    next_listener_id: u64,
}

impl PermissionInner {
    fn new() -> Self {
        Self { states: HashMap::new(), listeners: HashMap::new(), next_listener_id: 1 }
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
                guard.states.insert(domain, state);
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
            if let Some(state) = guard.states.get(&domain) {
                return state.status;
            }
        }
        let status = self.permissions.status(domain);
        let now = (self.clock)();
        let mut guard = self.inner.lock();
        guard.states.insert(domain, PermissionState::new(domain, status, now));
        status
    }

    pub fn request(&self, domain: PermissionDomain) {
        self.permissions.request(domain);
    }

    pub fn snapshot(&self) -> Vec<PermissionState> {
        let guard = self.inner.lock();
        guard.states.values().copied().collect()
    }

    pub fn subscribe<F>(&self, domain: PermissionDomain, callback: F) -> PermissionSubscription
    where
        F: Fn(PermissionState) + Send + Sync + 'static,
    {
        let _ = self.status(domain);
        let mut guard = self.inner.lock();
        let id = guard.next_listener_id;
        guard.next_listener_id = guard.next_listener_id.saturating_add(1);
        let entry = guard.listeners.entry(domain).or_insert_with(Vec::new);
        let arc_cb: Arc<dyn Fn(PermissionState) + Send + Sync> = Arc::new(callback);
        entry.push((id, Arc::clone(&arc_cb)));
        let current = guard.states.get(&domain).copied();
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

#[cfg(test)]
mod tests {
    use super::*;
    use parking_lot::Mutex;
    use std::{collections::HashMap, sync::Arc};

    struct FakePermissions {
        statuses: Mutex<HashMap<PermissionDomain, PermissionStatus>>,
        subs: Mutex<Vec<Box<dyn Fn(PermissionDomain, PermissionStatus) + Send>>>,
    }

    impl FakePermissions {
        fn new() -> Self {
            let mut statuses = HashMap::new();
            statuses.insert(PermissionDomain::Camera, PermissionStatus::Denied);
            Self { statuses: Mutex::new(statuses), subs: Mutex::new(Vec::new()) }
        }

        fn notify(&self, domain: PermissionDomain, status: PermissionStatus) {
            self.statuses.lock().insert(domain, status);
            for cb in self.subs.lock().iter() {
                cb(domain, status);
            }
        }
    }

    impl Permissions for FakePermissions {
        fn request(&self, _domain: PermissionDomain) {}

        fn status(&self, domain: PermissionDomain) -> PermissionStatus {
            *self.statuses.lock().get(&domain).unwrap_or(&PermissionStatus::NotDetermined)
        }

        fn subscribe(&self, f: Box<dyn Fn(PermissionDomain, PermissionStatus) + Send>) {
            self.subs.lock().push(f);
        }
    }

    #[test]
    fn manager_tracks_status_updates() {
        let fake = Arc::new(FakePermissions::new());
        let now = Arc::new(|| 42u64);
        let mgr = PermissionManager::new(fake.clone(), now);
        assert_eq!(mgr.status(PermissionDomain::Camera), PermissionStatus::Denied);
        fake.notify(PermissionDomain::Camera, PermissionStatus::Authorized);
        assert_eq!(mgr.status(PermissionDomain::Camera), PermissionStatus::Authorized);
    }

    #[test]
    fn subscription_receives_initial_and_updates() {
        let fake = Arc::new(FakePermissions::new());
        let now = Arc::new(|| 100u64);
        let mgr = PermissionManager::new(fake.clone(), now);
        let seen = Arc::new(Mutex::new(Vec::new()));
        let subscription = {
            let seen_clone = Arc::clone(&seen);
            mgr.subscribe(PermissionDomain::Camera, move |state| {
                seen_clone.lock().push(state.status);
            })
        };
        fake.notify(PermissionDomain::Camera, PermissionStatus::Authorized);
        let statuses = seen.lock().clone();
        assert_eq!(statuses, vec![PermissionStatus::Denied, PermissionStatus::Authorized]);
        drop(subscription);
    }
}
