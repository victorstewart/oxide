//! Secure storage service.

use crate::PlatformError;
use core::future::Future;

pub trait SecureStorage: Send + Sync {
    /// Saves a block of data under a given key.
    /// This will overwrite any existing data for the key.
    fn save(&self, key: &str, data: &[u8]) -> impl Future<Output = Result<(), PlatformError>> + Send;

    /// Loads a block of data for a given key.
    /// Returns `Ok(None)` if the key does not exist.
    fn load(&self, key: &str) -> impl Future<Output = Result<Option<alloc::vec::Vec<u8>>, PlatformError>> + Send;

    /// Deletes the data associated with a given key.
    fn delete(&self, key: &str) -> impl Future<Output = Result<(), PlatformError>> + Send;
}
