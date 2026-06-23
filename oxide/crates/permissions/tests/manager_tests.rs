use oxide_permissions::PermissionManager;
use oxide_platform_api::{PermissionDomain, PermissionStatus, Permissions};
use parking_lot::Mutex;
use std::{collections::HashMap, sync::Arc};

type StatusCallback = Box<dyn Fn(PermissionDomain, PermissionStatus) + Send>;

struct FakePermissions {
    statuses: Mutex<HashMap<PermissionDomain, PermissionStatus>>,
    subs: Mutex<Vec<StatusCallback>>,
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
fn manager_cache_tracks_every_domain_slot_and_snapshot() {
    let fake = Arc::new(FakePermissions::new());
    let now = Arc::new(|| 77u64);
    let mgr = PermissionManager::new(fake.clone(), now);
    let cases = [
        (PermissionDomain::Notifications, PermissionStatus::NotDetermined),
        (PermissionDomain::Location, PermissionStatus::Authorized),
        (PermissionDomain::Camera, PermissionStatus::Denied),
        (PermissionDomain::Contacts, PermissionStatus::Limited),
        (PermissionDomain::Bluetooth, PermissionStatus::Authorized),
        (PermissionDomain::Motion, PermissionStatus::Limited),
        (PermissionDomain::Microphone, PermissionStatus::Denied),
        (PermissionDomain::MediaLibrary, PermissionStatus::Authorized),
    ];

    for (domain, status) in cases {
        fake.notify(domain, status);
    }

    for (domain, status) in cases {
        assert_eq!(mgr.status(domain), status);
    }

    let snapshot = mgr.snapshot();
    assert_eq!(snapshot.len(), cases.len());
    for (domain, status) in cases {
        assert!(snapshot
            .iter()
            .any(|state| state.domain == domain && state.status == status && state.last_changed_ms == 77));
    }
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
    fake.notify(PermissionDomain::Camera, PermissionStatus::Denied);
    assert_eq!(*seen.lock(), vec![PermissionStatus::Denied, PermissionStatus::Authorized]);
}
