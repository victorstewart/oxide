use oxide_telemetry::{MemoryPressureLevel, TelemetryHub, TelemetryOpsStatus, TelemetrySnapshot};
use std::sync::Arc;

#[derive(Clone)]
pub struct TelemetryView {
    hub: Arc<TelemetryHub>,
}

impl TelemetryView {
    #[must_use]
    pub fn new(hub: Arc<TelemetryHub>) -> Self {
        Self { hub }
    }

    #[must_use]
    pub fn snapshot(&self) -> TelemetrySnapshot {
        self.hub.snapshot()
    }

    #[must_use]
    pub fn operations_status(&self) -> TelemetryOpsStatus {
        self.hub.snapshot().operations
    }

    #[must_use]
    pub fn memory_pressure(&self) -> MemoryPressureLevel {
        self.hub.snapshot().memory_pressure
    }

    #[must_use]
    pub fn hub(&self) -> &Arc<TelemetryHub> {
        &self.hub
    }
}
