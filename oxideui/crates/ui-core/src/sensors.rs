use oxideui_permissions::SensorBridge;
use std::sync::Arc;

/// Immutable view over the shared sensor bridge.
#[derive(Clone)]
pub struct SensorView {
    bridge: Arc<SensorBridge>,
}

impl SensorView {
    #[inline]
    pub fn new(bridge: Arc<SensorBridge>) -> Self {
        Self { bridge }
    }

    #[inline]
    pub fn snapshot(&self) -> oxideui_permissions::sensors::SensorSnapshot {
        self.bridge.snapshot()
    }
}

pub use oxideui_permissions::sensors::{
    BluetoothSnapshot, LocationSnapshot, MotionSnapshot, PushSnapshot, SensorBridgeConfig,
    SensorPermissionBinding, SensorSnapshot,
};
