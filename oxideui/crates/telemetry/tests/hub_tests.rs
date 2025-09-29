use oxideui_networking::{
    NetworkPath, QuicSessionMetrics, ReachabilitySnapshot, ReachabilityState, SessionPhase,
};
use oxideui_permissions::{
    sensors::{BluetoothSnapshot, LocationSnapshot, MotionSnapshot, PushSnapshot, SensorSnapshot},
    PermissionState,
};
use oxideui_platform_api::{
    LocationReading, MotionSample, PermissionDomain, PermissionStatus, PushProvider, PushToken,
};
use oxideui_telemetry::{
    TelemetryConfig, TelemetryEvent, TelemetryHealth, TelemetryHub, TelemetryUpdateKind,
};
use std::sync::{Arc, Mutex};

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

fn failed_network_metrics() -> QuicSessionMetrics {
    QuicSessionMetrics {
        phase: SessionPhase::Failed { reason: "handshake".into() },
        last_handshake_ms: Some(20),
        time_sync: Default::default(),
    }
}

#[test]
fn health_transitions_to_nominal_when_all_inputs_good() {
    let hub = TelemetryHub::new();
    let events = Arc::new(Mutex::new(Vec::new()));
    let events_ref = Arc::clone(&events);
    let _sub = hub.subscribe(move |event| {
        events_ref.lock().expect("events mutex").push(event);
    });

    hub.update_sensors(Some(sample_sensors()));
    hub.update_network_metrics(Some(nominal_network_metrics()));
    hub.update_permissions(sample_permissions(PermissionStatus::Authorized));

    let mut reachability = ReachabilitySnapshot::default();
    reachability.state = ReachabilityState::Online { path: NetworkPath::wifi() };
    hub.update_reachability(reachability);

    let events = events.lock().expect("events mutex");
    let health_event = events
        .iter()
        .find_map(|event| {
            if let TelemetryEvent::HealthChanged { from, to, .. } = event {
                Some((*from, *to))
            } else {
                None
            }
        })
        .expect("health event");
    assert_eq!(health_event.0, TelemetryHealth::Offline);
    assert_eq!(health_event.1, TelemetryHealth::Nominal);

    let snapshot_event = events
        .iter()
        .rev()
        .find_map(|event| {
            if let TelemetryEvent::Snapshot { kind: TelemetryUpdateKind::Reachability, snapshot } =
                event
            {
                Some(snapshot.clone())
            } else {
                None
            }
        })
        .expect("reachability snapshot");
    assert_eq!(snapshot_event.health, TelemetryHealth::Nominal);
}

#[test]
fn denying_permission_degrades_health() {
    let hub = TelemetryHub::new();
    let events = Arc::new(Mutex::new(Vec::new()));
    let events_ref = Arc::clone(&events);
    let _sub = hub.subscribe(move |event| {
        events_ref.lock().expect("events mutex").push(event);
    });

    hub.update_sensors(Some(sample_sensors()));
    hub.update_network_metrics(Some(nominal_network_metrics()));
    let mut reachability = ReachabilitySnapshot::default();
    reachability.state = ReachabilityState::Online { path: NetworkPath::wifi() };
    hub.update_reachability(reachability);
    hub.update_permissions(sample_permissions(PermissionStatus::Authorized));

    events.lock().expect("events mutex").clear();
    hub.update_permissions(sample_permissions(PermissionStatus::Denied));

    let events = events.lock().expect("events mutex");
    let to = events
        .iter()
        .find_map(|event| {
            if let TelemetryEvent::HealthChanged { to, .. } = event {
                Some(*to)
            } else {
                None
            }
        })
        .expect("health changed");
    assert_eq!(to, TelemetryHealth::Degraded);
}

#[test]
fn network_failure_sets_degraded() {
    let hub = TelemetryHub::new();
    let events = Arc::new(Mutex::new(Vec::new()));
    let events_ref = Arc::clone(&events);
    let _sub = hub.subscribe(move |event| {
        events_ref.lock().expect("events mutex").push(event);
    });

    hub.update_permissions(sample_permissions(PermissionStatus::Authorized));
    hub.update_sensors(Some(sample_sensors()));

    let mut reachability = ReachabilitySnapshot::default();
    reachability.state = ReachabilityState::Online { path: NetworkPath::wifi() };
    hub.update_reachability(reachability);
    hub.update_network_metrics(Some(nominal_network_metrics()));

    events.lock().expect("events mutex").clear();
    hub.update_network_metrics(Some(failed_network_metrics()));

    let events = events.lock().expect("events mutex");
    let to = events
        .iter()
        .find_map(|event| {
            if let TelemetryEvent::HealthChanged { to, .. } = event {
                Some(*to)
            } else {
                None
            }
        })
        .expect("health event");
    assert_eq!(to, TelemetryHealth::Degraded);
}

#[test]
fn history_retains_recent_events_only() {
    let hub = TelemetryHub::with_config(TelemetryConfig { history_capacity: 4 });

    let mut reachability = ReachabilitySnapshot::default();
    reachability.state = ReachabilityState::Online { path: NetworkPath::wifi() };
    hub.update_reachability(reachability);
    hub.update_sensors(Some(sample_sensors()));
    hub.update_network_metrics(Some(nominal_network_metrics()));

    let history = hub.history();
    assert_eq!(history.len(), 4);
    assert_eq!(history[0].id, 3);
    assert_eq!(history.last().map(|record| record.id), Some(6));

    let since = hub.history_since(4);
    let ids: Vec<u64> = since.iter().map(|record| record.id).collect();
    assert_eq!(ids, vec![5, 6]);

    let events: Vec<&TelemetryEvent> = history.iter().map(|record| &record.event).collect();
    assert!(matches!(events[0], TelemetryEvent::HealthChanged { .. }));
    assert!(matches!(events.last().unwrap(), TelemetryEvent::HealthChanged { .. }));
}
