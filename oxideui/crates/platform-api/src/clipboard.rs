//! Clipboard provider registry for OxideUI platform bridges.

#![allow(clippy::module_name_repetitions)]

use alloc::string::String;
use core::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};

/// Trait implemented by host platforms to provide clipboard access.
pub trait ClipboardProvider: Send + Sync + 'static {
    fn read_string(&self) -> Option<String>;
    fn write_string(&self, value: &str);
}

static PROVIDER: RwLock<Option<Arc<dyn ClipboardProvider>>> = RwLock::new(None);
static REVISION: AtomicU64 = AtomicU64::new(0);

/// Install a clipboard provider for the process. Replaces any previously registered provider.
pub fn set_clipboard_provider(provider: Arc<dyn ClipboardProvider>) {
    {
        let mut guard = PROVIDER.write().expect("clipboard provider lock");
        *guard = Some(provider);
    }
    REVISION.fetch_add(1, Ordering::SeqCst);
}

fn provider() -> Option<Arc<dyn ClipboardProvider>> {
    PROVIDER.read().ok().and_then(|guard| guard.as_ref().map(Arc::clone))
}

/// Read UTF-8 text from the clipboard if a provider is installed.
#[must_use]
pub fn read_string() -> Option<String> {
    provider().and_then(|p| p.read_string())
}

/// Write UTF-8 text to the clipboard if a provider is installed.
#[must_use]
pub fn write_string<S: AsRef<str>>(value: S) -> bool {
    if let Some(p) = provider() {
        p.write_string(value.as_ref());
        true
    } else {
        false
    }
}

/// Returns a monotonically increasing revision that changes whenever the provider is replaced.
#[must_use]
pub fn revision() -> u64 {
    REVISION.load(Ordering::SeqCst)
}

extern crate alloc;
