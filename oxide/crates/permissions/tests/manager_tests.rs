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
