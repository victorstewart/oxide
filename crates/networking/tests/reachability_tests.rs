use oxideui_networking::{NetworkPath, ReachabilityManager, ReachabilityState};
use std::sync::{atomic::{AtomicU64, AtomicUsize, Ordering}, Arc, Mutex};

fn test_clock() -> (Arc<AtomicU64>, Arc<dyn Fn() -> u64 + Send + Sync>)
{
   let now = Arc::new(AtomicU64::new(0));
   let clock_now = Arc::clone(&now);
   let clock = Arc::new(move || clock_now.load(Ordering::SeqCst));
   (now, clock)
}

#[test]
fn reachability_notifies_on_state_changes()
{
   let (now, clock) = test_clock();
   let manager = ReachabilityManager::new_with_clock(clock);
   let states = Arc::new(Mutex::new(Vec::new()));
   let states_ref = Arc::clone(&states);
   let _subscription = manager.subscribe(move |snapshot| {
      states_ref.lock().expect("state lock").push(snapshot);
   });

   assert_eq!(manager.snapshot().state, ReachabilityState::Offline);

   now.store(10, Ordering::SeqCst);
   manager.update(ReachabilityState::Online { path: NetworkPath::wifi() });

   now.store(25, Ordering::SeqCst);
   manager.update(ReachabilityState::Offline);

   let captured = states.lock().expect("state lock");
   assert_eq!(captured.len(), 3);
   assert_eq!(captured[0].state, ReachabilityState::Offline);
   assert_eq!(captured[1].state, ReachabilityState::Online { path: NetworkPath::wifi() });
   assert_eq!(captured[1].last_changed_ms, 10);
   assert_eq!(captured[2].state, ReachabilityState::Offline);
   assert_eq!(captured[2].last_changed_ms, 25);
}

#[test]
fn duplicate_updates_are_ignored()
{
   let (_, clock) = test_clock();
   let manager = ReachabilityManager::new_with_clock(clock);
   let count = Arc::new(AtomicUsize::new(0));
   let counter = Arc::clone(&count);
   let _subscription = manager.subscribe(move |_| {
      counter.fetch_add(1, Ordering::SeqCst);
   });

   manager.update(ReachabilityState::Online { path: NetworkPath::wifi() });
   manager.update(ReachabilityState::Online { path: NetworkPath::wifi() });
   manager.update(ReachabilityState::Online { path: NetworkPath::cellular(false) });

   assert_eq!(count.load(Ordering::SeqCst), 3);
}
