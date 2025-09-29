use oxideui_ui_core::layout_async::AsyncLayoutCoordinator;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

#[test]
fn async_coordinator_returns_latest() {
    let mut coord = AsyncLayoutCoordinator::new();
    let flag = Arc::new(Mutex::new(0usize));
    let slow_flag = flag.clone();
    let seq1 = coord.request(move || {
        thread::sleep(Duration::from_millis(30));
        *slow_flag.lock().unwrap() = 1;
        1
    });
    assert_eq!(seq1, 1);
    let seq2 = coord.request(|| 2);
    assert_eq!(seq2, 2);
    thread::sleep(Duration::from_millis(40));
    let result = coord.poll_latest().expect("result");
    assert_eq!(result.0, 2);
    assert_eq!(result.1, 2);
    // Slow job may still finish, but coordinator should not surface it again
    assert!(coord.poll_latest().is_none());
    assert_eq!(*flag.lock().unwrap(), 1);
}

#[test]
fn async_coordinator_applies_intermediate_when_no_newer_ready() {
    let mut coord = AsyncLayoutCoordinator::new();
    let seq1 = coord.request(|| 5);
    assert_eq!(seq1, 1);
    thread::sleep(Duration::from_millis(5));
    let res = coord.poll_latest().expect("res");
    assert_eq!(res.0, 1);
    assert_eq!(res.1, 5);
    assert!(coord.poll_latest().is_none());
}
