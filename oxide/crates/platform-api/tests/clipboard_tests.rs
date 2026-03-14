use oxide_platform_api::clipboard;
use oxide_platform_api::clipboard::ClipboardProvider;
use std::sync::{Arc, Mutex, OnceLock};

fn global_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(())).lock().expect("clipboard test lock")
}

#[derive(Default)]
struct MemoryClipboard {
    data: Mutex<Option<String>>,
}

impl clipboard::ClipboardProvider for MemoryClipboard {
    fn read_string(&self) -> Option<String> {
        self.data.lock().ok().and_then(|guard| guard.clone())
    }

    fn write_string(&self, value: &str) {
        if let Ok(mut guard) = self.data.lock() {
            *guard = Some(value.to_owned());
        }
    }
}

#[test]
fn clipboard_provider_contract() {
    let _guard = global_lock();

    let rev0 = clipboard::revision();
    assert_eq!(clipboard::read_string(), None);
    assert!(!clipboard::write_string("noop"));

    let provider_a = Arc::new(MemoryClipboard::default());
    clipboard::set_clipboard_provider(provider_a.clone());
    let rev1 = clipboard::revision();
    assert!(rev1 > rev0);

    assert!(clipboard::write_string("hello"));
    assert_eq!(provider_a.read_string(), Some(String::from("hello")));

    let provider_b = Arc::new(MemoryClipboard::default());
    clipboard::set_clipboard_provider(provider_b.clone());
    let rev2 = clipboard::revision();
    assert!(rev2 > rev1);
    assert_eq!(provider_b.read_string(), None);
}

#[cfg(feature = "tokio-runtime")]
mod runtime_tests {
    use super::global_lock;
    use oxide_platform_api::{runtime, HapticPattern, Haptics, Timers, UpdateContext};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Mutex, OnceLock};

    struct NoopHaptics;
    impl Haptics for NoopHaptics {
        fn play(&self, _: HapticPattern) {}
    }

    fn runtime_lock() -> std::sync::MutexGuard<'static, ()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(())).lock().expect("runtime test lock")
    }

    static SPAWN_CALLS: AtomicUsize = AtomicUsize::new(0);

    fn install_spawn_trampoline() {
        static INIT: OnceLock<()> = OnceLock::new();
        INIT.get_or_init(|| {
            runtime::set_spawn(|fut| {
                SPAWN_CALLS.fetch_add(1, Ordering::SeqCst);
                drop(fut);
            })
        });
    }

    #[test]
    fn update_context_spawn_invokes_runtime() {
        let _g1 = global_lock();
        let _g2 = runtime_lock();
        install_spawn_trampoline();
        SPAWN_CALLS.store(0, Ordering::SeqCst);

        let ctx = UpdateContext {
            post_task: Box::new(|job| job()),
            timers: Timers,
            haptics: Box::new(NoopHaptics),
        };

        ctx.spawn(async {});
        assert_eq!(SPAWN_CALLS.load(Ordering::SeqCst), 1);
    }
}
