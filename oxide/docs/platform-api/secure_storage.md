# platform-api `secure_storage.rs`

## Intention and purpose
- Define Oxide's generic secure-storage contract.
- Provide a process-global callback registry for sync compatibility layers that need secure storage without depending on app-local bridge singletons.

## Relation to the rest of the code
- Host crates install platform-native secure-storage implementations through `Platform::secure_storage()`.
- Compatibility callers such as `nametag-storage` use `register_secure_storage_callbacks(...)`, `save_secret(...)`, `load_secret(...)`, and `delete_secret(...)` to share the same generic ownership boundary without importing app code.
- `CallbackSecureStorage` adapts the callback registry back into Oxide's async `SecureStorage` trait where needed.

## Entry points list
- `SecureStorage`
  Async trait for generic secure save/load/delete operations.
- `SecureStorageCallbacks`
  Boxed sync callback set used by host shims and tests.
- `register_secure_storage_callbacks(...)`
  Installs the process-global callback set.
- `clear_secure_storage_callbacks()`
  Clears the callback registry for deterministic teardown.
- `save_secret(...)`, `load_secret(...)`, `delete_secret(...)`
  Sync helper functions over the registered callbacks.
- `CallbackSecureStorage`
  Trait adapter that forwards async calls into the callback registry.

## Logic narrative
- The callback registry is held behind a poisoned-lock-safe `RwLock`.
- Sync callers can use the helper functions directly.
- Async callers can instantiate `CallbackSecureStorage`, which boxes immediately-ready futures around the same helpers.

## Preconditions and postconditions
- Hosts or tests must register callbacks before any sync helper or `CallbackSecureStorage` call can succeed.
- After registration, every helper and adapter method targets the latest installed callback set.

## Edge cases and failure modes
- Missing callbacks return `PlatformError::Unsupported("secure storage unavailable")`.
- Lock poisoning recovers through `into_inner()` so teardown/setup failures do not permanently wedge the registry.

## Testing and benchmarks
- Covered by `crates/platform-api/tests/secure_storage_tests.rs`.

## Changelog
- 2026-03-12: moved generic secure-storage callback ownership into Oxide so apps no longer need app-local secure-storage registries.
