#![forbid(unsafe_code)]
#![allow(clippy::missing_errors_doc, clippy::missing_panics_doc)]

use oxide_networking::{QuicSessionMetrics, ReachabilitySnapshot, ReachabilityState, SessionPhase};
use oxide_permissions::{sensors::SensorSnapshot, PermissionState};
use oxide_platform_api::{PermissionDomain, PermissionStatus};
use parking_lot::Mutex;
use std::{collections::HashMap, sync::Arc};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TelemetryHealth
{
   Offline,
   Degraded,
   Nominal,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TelemetrySnapshot
{
   pub permissions: Vec<PermissionState>,
   pub reachability: ReachabilitySnapshot,
   pub sensors: Option<SensorSnapshot>,
   pub network: Option<QuicSessionMetrics>,
   pub health: TelemetryHealth,
}

impl Default for TelemetrySnapshot
{
   fn default() -> Self
   {
      Self
      {
         permissions: Vec::new(),
         reachability: ReachabilitySnapshot::default(),
         sensors: None,
         network: None,
         health: TelemetryHealth::Offline,
      }
   }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TelemetryUpdateKind
{
   Initial,
   Permissions,
   Reachability,
   Sensors,
   Network,
}

#[derive(Clone, Debug, PartialEq)]
pub enum TelemetryEvent
{
   Snapshot { kind: TelemetryUpdateKind, snapshot: TelemetrySnapshot },
   HealthChanged { from: TelemetryHealth, to: TelemetryHealth, snapshot: TelemetrySnapshot },
}

type TelemetryCallback = Arc<dyn Fn(TelemetryEvent) + Send + Sync>;

struct TelemetryInner
{
   snapshot: TelemetrySnapshot,
   listeners: HashMap<u64, TelemetryCallback>,
   next_id: u64,
}

impl TelemetryInner
{
   fn new(snapshot: TelemetrySnapshot) -> Self
   {
      Self { snapshot, listeners: HashMap::new(), next_id: 1 }
   }
}

#[derive(Clone)]
pub struct TelemetryHub
{
   inner: Arc<Mutex<TelemetryInner>>,
}

impl TelemetryHub
{
   #[must_use]
   pub fn new() -> Self
   {
      Self { inner: Arc::new(Mutex::new(TelemetryInner::new(TelemetrySnapshot::default()))) }
   }

   #[must_use]
   pub fn snapshot(&self) -> TelemetrySnapshot
   {
      self.inner.lock().snapshot.clone()
   }

   pub fn subscribe<F>(&self, listener: F) -> TelemetrySubscription
   where
      F: Fn(TelemetryEvent) + Send + Sync + 'static,
   {
      let callback: TelemetryCallback = Arc::new(listener);
      let mut inner = self.inner.lock();
      let id = inner.next_id;
      inner.next_id = inner.next_id.saturating_add(1);
      inner.listeners.insert(id, Arc::clone(&callback));
      let snapshot = inner.snapshot.clone();
      drop(inner);
      callback(TelemetryEvent::Snapshot { kind: TelemetryUpdateKind::Initial, snapshot });
      TelemetrySubscription { id, inner: Arc::clone(&self.inner) }
   }

   pub fn update_permissions(&self, permissions: Vec<PermissionState>)
   {
      self.mutate(TelemetryUpdateKind::Permissions, |snapshot| {
         if snapshot.permissions == permissions
         {
            false
         }
         else
         {
            snapshot.permissions = permissions;
            true
         }
      });
   }

   pub fn update_reachability(&self, reachability: ReachabilitySnapshot)
   {
      self.mutate(TelemetryUpdateKind::Reachability, |snapshot| {
         if snapshot.reachability == reachability
         {
            false
         }
         else
         {
            snapshot.reachability = reachability;
            true
         }
      });
   }

   pub fn update_sensors(&self, sensors: Option<SensorSnapshot>)
   {
      self.mutate(TelemetryUpdateKind::Sensors, |snapshot| {
         if snapshot.sensors == sensors
         {
            false
         }
         else
         {
            snapshot.sensors = sensors;
            true
         }
      });
   }

   pub fn update_network_metrics(&self, metrics: Option<QuicSessionMetrics>)
   {
      self.mutate(TelemetryUpdateKind::Network, |snapshot| {
         if snapshot.network == metrics
         {
            false
         }
         else
         {
            snapshot.network = metrics;
            true
         }
      });
   }

   fn mutate(&self, kind: TelemetryUpdateKind, mutate: impl FnOnce(&mut TelemetrySnapshot) -> bool)
   {
      let mut inner = self.inner.lock();
      let mut next_snapshot = inner.snapshot.clone();
      if !mutate(&mut next_snapshot)
      {
         return;
      }
      let prev_health = inner.snapshot.health;
      next_snapshot.health = compute_health(&next_snapshot);
      let health_transition = if next_snapshot.health != prev_health
      {
         Some((prev_health, next_snapshot.health))
      }
      else
      {
         None
      };
      inner.snapshot = next_snapshot.clone();
      let callbacks: Vec<TelemetryCallback> = inner.listeners.values().map(Arc::clone).collect();
      drop(inner);
      for cb in &callbacks
      {
         cb(TelemetryEvent::Snapshot { kind, snapshot: next_snapshot.clone() });
      }
      if let Some((from, to)) = health_transition
      {
         for cb in callbacks
         {
            cb(TelemetryEvent::HealthChanged { from, to, snapshot: next_snapshot.clone() });
         }
      }
   }
}

pub struct TelemetrySubscription
{
   id: u64,
   inner: Arc<Mutex<TelemetryInner>>,
}

impl Drop for TelemetrySubscription
{
   fn drop(&mut self)
   {
      let mut inner = self.inner.lock();
      inner.listeners.remove(&self.id);
   }
}

fn compute_health(snapshot: &TelemetrySnapshot) -> TelemetryHealth
{
   if matches!(snapshot.reachability.state, ReachabilityState::Offline)
   {
      return TelemetryHealth::Offline;
   }

   let mut degraded = false;
   for perm in &snapshot.permissions
   {
      if is_critical_permission(perm.domain)
         && !matches!(perm.status, PermissionStatus::Authorized)
      {
         degraded = true;
         break;
      }
   }

   if !degraded
   {
      if let Some(sensors) = &snapshot.sensors
      {
         if sensors.location.last.is_none()
            || !sensors.bluetooth.powered_on
            || sensors.push.token.is_none()
         {
            degraded = true;
         }
      }
      else
      {
         degraded = true;
      }
   }

   if !degraded
   {
      if let Some(network) = &snapshot.network
      {
         if matches!(network.phase, SessionPhase::Failed { .. })
         {
            degraded = true;
         }
      }
      else
      {
         degraded = true;
      }
   }

   if degraded
   {
      TelemetryHealth::Degraded
   }
   else
   {
      TelemetryHealth::Nominal
   }
}

fn is_critical_permission(domain: PermissionDomain) -> bool
{
   matches!(
      domain,
      PermissionDomain::Camera
         | PermissionDomain::Microphone
         | PermissionDomain::Location
         | PermissionDomain::Bluetooth
         | PermissionDomain::Motion
         | PermissionDomain::Notifications
   )
}
