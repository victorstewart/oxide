use oxide_platform_api::PlatformError;
use oxide_platform_api::secure_storage::{
    CallbackSecureStorage, SecureStorage, SecureStorageCallbacks, clear_secure_storage_callbacks,
    delete_secret, has_secure_storage_callbacks, load_secret, register_secure_storage_callbacks,
    save_secret,
};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, Wake, Waker};

#[derive(Default)]
struct NoopWake;

impl Wake for NoopWake {
    fn wake(self: Arc<Self>) {}
}

fn poll_ready<F>(future: F) -> F::Output
where
    F: core::future::Future,
{
    let waker = Waker::from(Arc::new(NoopWake));
    let mut context = Context::from_waker(&waker);
    let mut future = core::pin::pin!(future);
    match future.as_mut().poll(&mut context) {
        Poll::Ready(value) => value,
        Poll::Pending => panic!("test future unexpectedly pending"),
    }
}

#[test]
fn callback_secure_storage_round_trips_registered_callbacks() {
    let state = Arc::new(Mutex::new(HashMap::<String, Vec<u8>>::new()));
    clear_secure_storage_callbacks();
    register_secure_storage_callbacks(SecureStorageCallbacks::new(
        {
            let state = state.clone();
            move |key, value| {
                state
                    .lock()
                    .map_err(|_| PlatformError::Unknown("lock poisoned".to_owned()))?
                    .insert(key.to_owned(), value.to_vec());
                Ok(())
            }
        },
        {
            let state = state.clone();
            move |key| {
                Ok(state
                    .lock()
                    .map_err(|_| PlatformError::Unknown("lock poisoned".to_owned()))?
                    .get(key)
                    .cloned())
            }
        },
        {
            let state = state.clone();
            move |key| {
                state
                    .lock()
                    .map_err(|_| PlatformError::Unknown("lock poisoned".to_owned()))?
                    .remove(key);
                Ok(())
            }
        },
    ));

    assert!(has_secure_storage_callbacks());
    save_secret("token", b"abc").expect("save through callbacks");
    assert_eq!(
        load_secret("token").expect("load through callbacks"),
        Some(b"abc".to_vec())
    );
    delete_secret("token").expect("delete through callbacks");
    assert_eq!(load_secret("token").expect("load after delete"), None);

    let storage = CallbackSecureStorage;
    poll_ready(storage.save("session", b"value")).expect("service save");
    assert_eq!(
        poll_ready(storage.load("session")).expect("service load"),
        Some(b"value".to_vec())
    );
    poll_ready(storage.delete("session")).expect("service delete");
    assert_eq!(
        poll_ready(storage.load("session")).expect("service load after delete"),
        None
    );

    clear_secure_storage_callbacks();
}
