//! Secure storage service.

use crate::PlatformError;
use core::future::Future;
use core::pin::Pin;
use std::sync::{Arc, OnceLock, RwLock};

type SaveFn = dyn Fn(&str, &[u8]) -> Result<(), PlatformError> + Send + Sync;
type LoadFn = dyn Fn(&str) -> Result<Option<alloc::vec::Vec<u8>>, PlatformError> + Send + Sync;
type DeleteFn = dyn Fn(&str) -> Result<(), PlatformError> + Send + Sync;

pub trait SecureStorage: Send + Sync {
    /// Saves a block of data under a given key.
    /// This will overwrite any existing data for the key.
    fn save<'a>(
        &'a self,
        key: &'a str,
        data: &'a [u8],
    ) -> Pin<alloc::boxed::Box<dyn Future<Output = Result<(), PlatformError>> + Send + 'a>>;

    /// Loads a block of data for a given key.
    /// Returns `Ok(None)` if the key does not exist.
    fn load<'a>(
        &'a self,
        key: &'a str,
    ) -> Pin<
        alloc::boxed::Box<
            dyn Future<Output = Result<Option<alloc::vec::Vec<u8>>, PlatformError>> + Send + 'a,
        >,
    >;

    /// Deletes the data associated with a given key.
    fn delete<'a>(
        &'a self,
        key: &'a str,
    ) -> Pin<alloc::boxed::Box<dyn Future<Output = Result<(), PlatformError>> + Send + 'a>>;
}

#[derive(Clone)]
pub struct SecureStorageCallbacks {
    pub save: Arc<SaveFn>,
    pub load: Arc<LoadFn>,
    pub delete: Arc<DeleteFn>,
}

impl SecureStorageCallbacks {
    #[must_use]
    pub fn new<Save, Load, Delete>(save: Save, load: Load, delete: Delete) -> Self
    where
        Save: Fn(&str, &[u8]) -> Result<(), PlatformError> + Send + Sync + 'static,
        Load:
            Fn(&str) -> Result<Option<alloc::vec::Vec<u8>>, PlatformError> + Send + Sync + 'static,
        Delete: Fn(&str) -> Result<(), PlatformError> + Send + Sync + 'static,
    {
        Self { save: Arc::new(save), load: Arc::new(load), delete: Arc::new(delete) }
    }
}

fn callbacks_cell() -> &'static RwLock<Option<SecureStorageCallbacks>> {
    static CALLBACKS: OnceLock<RwLock<Option<SecureStorageCallbacks>>> = OnceLock::new();
    CALLBACKS.get_or_init(|| RwLock::new(None))
}

fn callbacks_write_guard() -> std::sync::RwLockWriteGuard<'static, Option<SecureStorageCallbacks>> {
    callbacks_cell().write().unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn callbacks_read_guard() -> std::sync::RwLockReadGuard<'static, Option<SecureStorageCallbacks>> {
    callbacks_cell().read().unwrap_or_else(std::sync::PoisonError::into_inner)
}

pub fn register_secure_storage_callbacks(callbacks: SecureStorageCallbacks) {
    let mut guard = callbacks_write_guard();
    *guard = Some(callbacks);
}

pub fn clear_secure_storage_callbacks() {
    let mut guard = callbacks_write_guard();
    *guard = None;
}

#[must_use]
pub fn has_secure_storage_callbacks() -> bool {
    callbacks_read_guard().is_some()
}

fn current_callbacks() -> Result<SecureStorageCallbacks, PlatformError> {
    callbacks_read_guard()
        .as_ref()
        .cloned()
        .ok_or(PlatformError::Unsupported("secure storage unavailable"))
}

pub fn save_secret(key: &str, data: &[u8]) -> Result<(), PlatformError> {
    (current_callbacks()?.save)(key, data)
}

pub fn load_secret(key: &str) -> Result<Option<alloc::vec::Vec<u8>>, PlatformError> {
    (current_callbacks()?.load)(key)
}

pub fn delete_secret(key: &str) -> Result<(), PlatformError> {
    (current_callbacks()?.delete)(key)
}

#[derive(Default)]
pub struct CallbackSecureStorage;

impl SecureStorage for CallbackSecureStorage {
    fn save<'a>(
        &'a self,
        key: &'a str,
        data: &'a [u8],
    ) -> Pin<alloc::boxed::Box<dyn Future<Output = Result<(), PlatformError>> + Send + 'a>> {
        alloc::boxed::Box::pin(async move { save_secret(key, data) })
    }

    fn load<'a>(
        &'a self,
        key: &'a str,
    ) -> Pin<
        alloc::boxed::Box<
            dyn Future<Output = Result<Option<alloc::vec::Vec<u8>>, PlatformError>> + Send + 'a,
        >,
    > {
        alloc::boxed::Box::pin(async move { load_secret(key) })
    }

    fn delete<'a>(
        &'a self,
        key: &'a str,
    ) -> Pin<alloc::boxed::Box<dyn Future<Output = Result<(), PlatformError>> + Send + 'a>> {
        alloc::boxed::Box::pin(async move { delete_secret(key) })
    }
}
