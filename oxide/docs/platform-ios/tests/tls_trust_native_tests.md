# platform-ios native TLS trust tests

## Purpose

Compile and execute the production Objective-C trust implementation against macOS Security.framework without adding test hooks to the shipping archive.

## Coverage

- Matching hostname with an exact private root anchor succeeds.
- A wrong hostname fails with normal policy and succeeds only when hostname matching is suppressed.
- Hostname suppression without the private root still rejects the untrusted chain.
- Null trust and configured/parsed anchor-count mismatch fail closed.
- Null, empty, malformed, oversized, trailing-byte, and mixed valid/invalid anchor sets are rejected atomically.

The fixture is an embedded test-only DER root and 365-day server certificate for `expected.oxide.test`. The harness fixes Security.framework's verification date inside that validity window, so wall-clock passage cannot expire the test. No private key or network access is required.

## Boundary

The Rust integration test locates Apple clang, compiles `tests/native/tls_trust.m`, runs the temporary executable, and removes it. The harness textually includes `network.m`, so it calls the same crate-private functions without exporting test ABI or compiling test behavior into the product library.

## Changelog

- 2026-08-02: added native trust-policy and exact-anchor behavior coverage.
