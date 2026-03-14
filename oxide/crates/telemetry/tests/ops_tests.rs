use oxide_networking::{
    NetworkPath, QuicSessionMetrics, ReachabilitySnapshot, ReachabilityState, SessionPhase,
};
use oxide_permissions::{
    sensors::{BluetoothSnapshot, LocationSnapshot, MotionSnapshot, PushSnapshot, SensorSnapshot},
    PermissionState,
};
use oxide_platform_api::{
    LocationReading, MotionSample, PermissionDomain, PermissionStatus, PushProvider, PushToken,
};
use oxide_telemetry::{
    MemoryPressureLevel, TelemetryAction, TelemetryCommandReason, TelemetryHealth, TelemetryHub,
    TelemetryLifecycleState, TelemetryOperations,
};
use std::sync::Arc;

fn sample_permissions(status: PermissionStatus) -> Vec<PermissionState> {
    let domains = [
        PermissionDomain::Camera,
        PermissionDomain::Microphone,
        PermissionDomain::Location,
        PermissionDomain::Bluetooth,
        PermissionDomain::Motion,
        PermissionDomain::Notifications,
    ];
    domains.iter().map(|&d| PermissionState::new(d, status, 0)).collect()
}

fn sample_sensors() -> SensorSnapshot {
    let location = LocationSnapshot {
        last: Some(LocationReading {
            latitude_deg: 1.0,
            longitude_deg: 2.0,
            altitude_m: 3.0,
            horizontal_accuracy_m: 1.0,
            vertical_accuracy_m: 1.0,
            speed_mps: 0.0,
            course_deg: 0.0,
            timestamp_ms: 42,
        }),
        history: Vec::new(),
    };
    let motion = MotionSnapshot {
        last: Some(MotionSample {
            pressure_pa: Some(101_325.0),
            relative_altitude_m: Some(0.2),
            timestamp_ms: 42,
        }),
        history: Vec::new(),
    };
    let bluetooth = BluetoothSnapshot { powered_on: true, devices: Vec::new() };
    let push = PushSnapshot {
        token: Some(PushToken { provider: PushProvider::Apns, value: "token".into() }),
        notifications: Vec::new(),
    };
    SensorSnapshot { location, motion, bluetooth, push }
}

fn nominal_network_metrics() -> QuicSessionMetrics {
    QuicSessionMetrics {
        phase: SessionPhase::Established { session_id: 7 },
        last_handshake_ms: Some(10),
        time_sync: Default::default(),
    }
}

fn prime_nominal_state(hub: &TelemetryHub) {
    let reachability = ReachabilitySnapshot {
        state: ReachabilityState::Online { path: NetworkPath::wifi() },
        ..ReachabilitySnapshot::default()
    };
    hub.update_reachability(reachability);
    hub.update_permissions(sample_permissions(PermissionStatus::Authorized));
    hub.update_sensors(Some(sample_sensors()));
    hub.update_network_metrics(Some(nominal_network_metrics()));
}

#[test]
fn background_transition_emits_pause_commands() {
    let hub = Arc::new(TelemetryHub::new());
    let ops = TelemetryOperations::new(Arc::clone(&hub));

    ops.handle_background(25);
    let commands = ops.drain_commands();
    assert!(commands.iter().any(|cmd| cmd.action == TelemetryAction::PauseSensors
        && cmd.reason == TelemetryCommandReason::EnterBackground));
    assert!(commands.iter().any(|cmd| cmd.action == TelemetryAction::PauseNetworking
        && cmd.reason == TelemetryCommandReason::EnterBackground));
    assert!(commands.iter().any(|cmd| cmd.action == TelemetryAction::FlushMetrics
        && cmd.reason == TelemetryCommandReason::EnterBackground));

    let snapshot = hub.snapshot();
    assert_eq!(snapshot.operations.lifecycle, TelemetryLifecycleState::Background);
    assert_eq!(snapshot.operations.background_count, 1);
}

#[test]
fn foreground_resumes_after_background() {
    let hub = Arc::new(TelemetryHub::new());
    let ops = TelemetryOperations::new(Arc::clone(&hub));

    ops.handle_background(10);
    ops.drain_commands();

    ops.handle_foreground(20);
    let commands = ops.drain_commands();
    assert!(commands.iter().any(|cmd| cmd.action == TelemetryAction::ResumeSensors
        && cmd.reason == TelemetryCommandReason::EnterForeground));
    assert!(commands.iter().any(|cmd| cmd.action == TelemetryAction::ResumeNetworking
        && cmd.reason == TelemetryCommandReason::EnterForeground));
    assert!(commands.iter().any(|cmd| cmd.action == TelemetryAction::FlushMetrics
        && cmd.reason == TelemetryCommandReason::EnterForeground));

    let snapshot = hub.snapshot();
    assert_eq!(snapshot.operations.lifecycle, TelemetryLifecycleState::Foreground);
}

#[test]
fn degraded_while_background_executes_on_foreground() {
    let hub = Arc::new(TelemetryHub::new());
    prime_nominal_state(&hub);
    let ops = TelemetryOperations::new(Arc::clone(&hub));

    ops.handle_foreground(5);
    ops.drain_commands();

    ops.handle_background(10);
    ops.drain_commands();

    // Permission regression while background should queue recoveries without emitting commands immediately.
    hub.update_permissions(sample_permissions(PermissionStatus::Denied));
    assert!(ops.drain_commands().is_empty());

    ops.handle_foreground(20);
    let commands = ops.drain_commands();
    assert!(commands.iter().any(|cmd| cmd.action == TelemetryAction::RefreshPermissions
        && cmd.reason == TelemetryCommandReason::HealthDegraded));
    assert!(commands.iter().any(|cmd| cmd.action == TelemetryAction::ResumeSensors));
    assert!(commands.iter().any(|cmd| cmd.action == TelemetryAction::ResumeNetworking));
    assert!(commands.iter().any(|cmd| cmd.action == TelemetryAction::FlushMetrics));

    let snapshot = hub.snapshot();
    assert_eq!(snapshot.health, TelemetryHealth::Degraded);
    assert_eq!(snapshot.operations.recovery_actions, 1);
}

#[test]
fn memory_pressure_sequences_issue_trim_and_resume() {
    let hub = Arc::new(TelemetryHub::new());
    let ops = TelemetryOperations::new(Arc::clone(&hub));

    ops.handle_foreground(0);
    ops.drain_commands();

    ops.handle_memory_pressure(10, MemoryPressureLevel::Warning);
    let commands = ops.drain_commands();
    assert!(commands.iter().any(|cmd| cmd.action == TelemetryAction::TrimCaches));
    assert!(commands.iter().any(|cmd| cmd.action == TelemetryAction::FlushMetrics));
    assert_eq!(hub.snapshot().memory_pressure, MemoryPressureLevel::Warning);

    ops.handle_memory_pressure(20, MemoryPressureLevel::Critical);
    let commands = ops.drain_commands();
    assert!(commands.iter().any(|cmd| cmd.action == TelemetryAction::PauseSensors));
    assert!(commands.iter().any(|cmd| cmd.action == TelemetryAction::PauseNetworking));
    assert!(commands.iter().any(|cmd| cmd.action == TelemetryAction::TrimCaches));
    assert_eq!(hub.snapshot().memory_pressure, MemoryPressureLevel::Critical);

    ops.handle_memory_pressure(30, MemoryPressureLevel::Nominal);
    let commands = ops.drain_commands();
    assert!(commands.iter().any(|cmd| cmd.action == TelemetryAction::ResumeSensors));
    assert!(commands.iter().any(|cmd| cmd.action == TelemetryAction::ResumeNetworking));
    assert!(commands.iter().any(|cmd| cmd.action == TelemetryAction::FlushMetrics));
    assert_eq!(hub.snapshot().memory_pressure, MemoryPressureLevel::Nominal);
}
