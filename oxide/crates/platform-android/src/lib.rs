//! Android platform selection gate.

#![forbid(unsafe_code)]

#[cfg(target_os = "android")]
compile_error!("Oxide Android shipping is disabled: select and implement a production asynchronous HttpClient host before building Android");

/// Non-Android marker proving the workspace audits the Android shipping gate.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct AndroidProductionHttpRequired;
