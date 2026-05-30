use oxide_ui_core::layout_async::AsyncLayoutCoordinator;
use std::sync::mpsc;
use std::time::Duration;

struct ReleaseOnDrop
{
   sender: Option<mpsc::Sender<()>>,
}

impl ReleaseOnDrop
{
   fn new(sender: mpsc::Sender<()>) -> Self
   {
      Self { sender: Some(sender) }
   }

   fn release(&mut self)
   {
      if let Some(sender) = self.sender.take()
      {
         let _ = sender.send(());
      }
   }
}

impl Drop for ReleaseOnDrop
{
   fn drop(&mut self)
   {
      self.release();
   }
}

#[test]
fn async_coordinator_returns_latest() {
    let mut coord = AsyncLayoutCoordinator::new();
    let (slow_started_tx, slow_started_rx) = mpsc::channel();
    let (release_slow_tx, release_slow_rx) = mpsc::channel();
    let (sentinel_started_tx, sentinel_started_rx) = mpsc::channel();
    let (release_sentinel_tx, release_sentinel_rx) = mpsc::channel();
    let mut release_slow = ReleaseOnDrop::new(release_slow_tx);
    let mut release_sentinel = ReleaseOnDrop::new(release_sentinel_tx);
    let seq1 = coord.request(move || {
        slow_started_tx.send(()).expect("slow started");
        release_slow_rx.recv().expect("release slow");
        1
    });
    assert_eq!(seq1, 1);
    slow_started_rx.recv_timeout(Duration::from_secs(1)).expect("slow job started");
    let seq2 = coord.request(|| 2);
    assert_eq!(seq2, 2);
    let seq3 = coord.request(move || {
        sentinel_started_tx.send(()).expect("sentinel started");
        release_sentinel_rx.recv().expect("release sentinel");
        3
    });
    assert_eq!(seq3, 3);
    release_slow.release();
    sentinel_started_rx.recv_timeout(Duration::from_secs(1)).expect("sentinel job started");
    let result = coord.poll_latest().expect("result");
    assert_eq!(result.0, 2);
    assert_eq!(result.1, 2);
    assert!(coord.poll_latest().is_none());
    release_sentinel.release();
}

#[test]
fn async_coordinator_applies_intermediate_when_no_newer_ready() {
    let mut coord = AsyncLayoutCoordinator::new();
    let (sentinel_started_tx, sentinel_started_rx) = mpsc::channel();
    let (release_sentinel_tx, release_sentinel_rx) = mpsc::channel();
    let mut release_sentinel = ReleaseOnDrop::new(release_sentinel_tx);
    let seq1 = coord.request(|| 5);
    assert_eq!(seq1, 1);
    let seq2 = coord.request(move || {
        sentinel_started_tx.send(()).expect("sentinel started");
        release_sentinel_rx.recv().expect("release sentinel");
        7
    });
    assert_eq!(seq2, 2);
    sentinel_started_rx.recv_timeout(Duration::from_secs(1)).expect("sentinel job started");
    let res = coord.poll_latest().expect("res");
    assert_eq!(res.0, 1);
    assert_eq!(res.1, 5);
    assert!(coord.poll_latest().is_none());
    release_sentinel.release();
}
