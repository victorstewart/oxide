//! Network status service.

use bitflags::bitflags;

bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub struct NetworkInterface: u32 {
        const WIFI     = 1 << 0;
        const CELLULAR = 1 << 1;
        const WIRED    = 1 << 2;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NetworkStatus {
    /// True if any network connection (Wi-Fi, cellular, etc.) is available.
    pub is_connected: bool,
    /// The set of currently active network interface types.
    pub interfaces: NetworkInterface,
}

pub trait NetworkStatusService: Send + Sync {
    /// Gets the current network status.
    fn current_status(&self) -> NetworkStatus;

    /// Subscribes to network status changes.
    /// The provided callback will be invoked whenever the network status changes.
    fn subscribe(&self, f: alloc::boxed::Box<dyn Fn(NetworkStatus) + Send>);
}
