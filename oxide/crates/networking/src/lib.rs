#![forbid(unsafe_code)]
#![allow(clippy::missing_errors_doc, clippy::missing_panics_doc)]

use parking_lot::Mutex;
use std::{
    collections::{HashMap, VecDeque},
    error::Error,
    fmt,
    sync::Arc,
};

// ===== Reachability core =====

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NetworkPathKind {
    Wifi,
    Cellular,
    Wired,
    Other,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NetworkPath {
    pub kind: NetworkPathKind,
    pub expensive: bool,
}

impl NetworkPath {
    #[must_use]
    pub const fn wifi() -> Self {
        Self { kind: NetworkPathKind::Wifi, expensive: false }
    }

    #[must_use]
    pub const fn cellular(expensive: bool) -> Self {
        Self { kind: NetworkPathKind::Cellular, expensive }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReachabilityState {
    Offline,
    Online { path: NetworkPath },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReachabilitySnapshot {
    pub state: ReachabilityState,
    pub last_changed_ms: u64,
}

impl Default for ReachabilitySnapshot {
    fn default() -> Self {
        Self { state: ReachabilityState::Offline, last_changed_ms: 0 }
    }
}

type ReachabilityCallback = Arc<dyn Fn(ReachabilitySnapshot) + Send + Sync>;

struct ReachabilityInner {
    snapshot: ReachabilitySnapshot,
    listeners: HashMap<u64, ReachabilityCallback>,
    next_id: u64,
}

impl ReachabilityInner {
    fn new(initial: ReachabilitySnapshot) -> Self {
        Self { snapshot: initial, listeners: HashMap::new(), next_id: 1 }
    }
}

pub struct ReachabilityManager {
    clock: Arc<dyn Fn() -> u64 + Send + Sync>,
    inner: Arc<Mutex<ReachabilityInner>>,
}

impl ReachabilityManager {
    #[must_use]
    pub fn new_with_clock(clock: Arc<dyn Fn() -> u64 + Send + Sync>) -> Self {
        let now = clock();
        let initial =
            ReachabilitySnapshot { state: ReachabilityState::Offline, last_changed_ms: now };
        Self { clock, inner: Arc::new(Mutex::new(ReachabilityInner::new(initial))) }
    }

    #[must_use]
    pub fn with_default_clock() -> Self {
        Self::new_with_clock(Arc::new(default_clock))
    }

    #[must_use]
    pub fn snapshot(&self) -> ReachabilitySnapshot {
        self.inner.lock().snapshot
    }

    pub fn update(&self, state: ReachabilityState) {
        let mut inner = self.inner.lock();
        if inner.snapshot.state == state {
            return;
        }
        let now = (self.clock)();
        inner.snapshot = ReachabilitySnapshot { state, last_changed_ms: now };
        let callbacks: Vec<_> = inner.listeners.values().map(Arc::clone).collect();
        let snapshot = inner.snapshot;
        drop(inner);
        for cb in callbacks {
            cb(snapshot);
        }
    }

    pub fn subscribe<F>(&self, listener: F) -> ReachabilitySubscription
    where
        F: Fn(ReachabilitySnapshot) + Send + Sync + 'static,
    {
        let callback: ReachabilityCallback = Arc::new(listener);
        let mut inner = self.inner.lock();
        let id = inner.next_id;
        inner.next_id = inner.next_id.saturating_add(1);
        inner.listeners.insert(id, Arc::clone(&callback));
        let snapshot = inner.snapshot;
        drop(inner);
        callback(snapshot);
        ReachabilitySubscription { id, inner: Arc::clone(&self.inner) }
    }
}

pub struct ReachabilitySubscription {
    id: u64,
    inner: Arc<Mutex<ReachabilityInner>>,
}

impl Drop for ReachabilitySubscription {
    fn drop(&mut self) {
        let mut inner = self.inner.lock();
        inner.listeners.remove(&self.id);
    }
}

fn default_clock() -> u64 {
    use std::time::{Duration, SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_else(|_| Duration::from_millis(0))
        .as_millis() as u64
}

// ===== Connection configuration =====

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Endpoint {
    pub host: String,
    pub port: u16,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConnectionConfig {
    pub application_id: String,
    pub endpoint: Endpoint,
    pub alpn: Vec<String>,
    pub idle_timeout_ms: u32,
    pub max_datagram_size: u16,
    pub allow_local_fallback: bool,
}

impl ConnectionConfig {
    pub fn encode(&self) -> Result<Vec<u8>, ConnectionConfigError> {
        let mut out = Vec::with_capacity(128);
        write_string(&mut out, &self.application_id)?;
        write_string(&mut out, &self.endpoint.host)?;
        out.extend(self.endpoint.port.to_le_bytes());
        out.extend(self.idle_timeout_ms.to_le_bytes());
        out.extend(self.max_datagram_size.to_le_bytes());
        out.push(if self.allow_local_fallback { 1 } else { 0 });

        if self.alpn.len() > u8::MAX as usize {
            return Err(ConnectionConfigError::TooManyProtocols);
        }
        out.push(self.alpn.len() as u8);
        for proto in &self.alpn {
            write_string(&mut out, proto)?;
        }
        Ok(out)
    }

    pub fn decode(input: &[u8]) -> Result<Self, ConnectionConfigError> {
        let mut cursor = Cursor::new(input);
        let application_id = read_string(&mut cursor)?;
        let host = read_string(&mut cursor)?;
        let port = cursor.read_u16()?;
        let idle_timeout_ms = cursor.read_u32()?;
        let max_datagram_size = cursor.read_u16()?;
        let allow_local_fallback = cursor.read_u8()? != 0;

        let count = cursor.read_u8()? as usize;
        let mut alpn = Vec::with_capacity(count);
        for _ in 0..count {
            alpn.push(read_string(&mut cursor)?);
        }

        cursor.ensure_fully_consumed()?;
        Ok(Self {
            application_id,
            endpoint: Endpoint { host, port },
            alpn,
            idle_timeout_ms,
            max_datagram_size,
            allow_local_fallback,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConnectionConfigError {
    UnexpectedEof,
    InvalidUtf8,
    FieldTooLong,
    TooManyProtocols,
}

impl fmt::Display for ConnectionConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConnectionConfigError::UnexpectedEof => f.write_str("unexpected end of buffer"),
            ConnectionConfigError::InvalidUtf8 => f.write_str("invalid UTF-8 data"),
            ConnectionConfigError::FieldTooLong => f.write_str("field exceeds maximum length"),
            ConnectionConfigError::TooManyProtocols => f.write_str("too many ALPN protocols"),
        }
    }
}

impl Error for ConnectionConfigError {}

fn write_string(out: &mut Vec<u8>, value: &str) -> Result<(), ConnectionConfigError> {
    let len = value.len();
    if len > u16::MAX as usize {
        return Err(ConnectionConfigError::FieldTooLong);
    }
    out.extend((len as u16).to_le_bytes());
    out.extend(value.as_bytes());
    Ok(())
}

struct Cursor<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Cursor<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    fn read_u8(&mut self) -> Result<u8, ConnectionConfigError> {
        if let Some(byte) = self.data.get(self.pos) {
            self.pos += 1;
            Ok(*byte)
        } else {
            Err(ConnectionConfigError::UnexpectedEof)
        }
    }

    fn read_u16(&mut self) -> Result<u16, ConnectionConfigError> {
        if self.pos + 2 > self.data.len() {
            return Err(ConnectionConfigError::UnexpectedEof);
        }
        let value = u16::from_le_bytes([self.data[self.pos], self.data[self.pos + 1]]);
        self.pos += 2;
        Ok(value)
    }

    fn read_u32(&mut self) -> Result<u32, ConnectionConfigError> {
        if self.pos + 4 > self.data.len() {
            return Err(ConnectionConfigError::UnexpectedEof);
        }
        let bytes = [
            self.data[self.pos],
            self.data[self.pos + 1],
            self.data[self.pos + 2],
            self.data[self.pos + 3],
        ];
        self.pos += 4;
        Ok(u32::from_le_bytes(bytes))
    }

    fn read_slice(&mut self, len: usize) -> Result<&'a [u8], ConnectionConfigError> {
        if self.pos + len > self.data.len() {
            return Err(ConnectionConfigError::UnexpectedEof);
        }
        let slice = &self.data[self.pos..self.pos + len];
        self.pos += len;
        Ok(slice)
    }

    fn ensure_fully_consumed(&self) -> Result<(), ConnectionConfigError> {
        if self.pos == self.data.len() {
            Ok(())
        } else {
            Err(ConnectionConfigError::UnexpectedEof)
        }
    }
}

fn read_string(cursor: &mut Cursor<'_>) -> Result<String, ConnectionConfigError> {
    let len = cursor.read_u16()? as usize;
    let bytes = cursor.read_slice(len)?;
    std::str::from_utf8(bytes).map(|s| s.to_owned()).map_err(|_| ConnectionConfigError::InvalidUtf8)
}

// ===== QUIC session manager =====

#[repr(u8)]
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum PacketKind {
    HandshakeInit,
    HandshakeRetry,
    TimeSyncProbe,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OutboundPacket {
    pub kind: PacketKind,
    pub payload: Vec<u8>,
    pub timestamp_ms: u64,
}

#[derive(Clone, Debug)]
pub struct QuicSessionConfig {
    pub handshake_timeout_ms: u32,
    pub max_retries: u32,
    pub timesync_interval_ms: u32,
    pub timesync_window: usize,
    pub initial_retry_ms: u32,
    pub max_retry_ms: u32,
    pub backoff_multiplier: f32,
}

impl Default for QuicSessionConfig {
    fn default() -> Self {
        Self {
            handshake_timeout_ms: 1_000,
            max_retries: 3,
            timesync_interval_ms: 5_000,
            timesync_window: 4,
            initial_retry_ms: 1_000,
            max_retry_ms: 30_000,
            backoff_multiplier: 2.0,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SessionPhase {
    Idle,
    Handshaking { attempts: u32 },
    Established { session_id: u64 },
    Failed { reason: String },
}

impl SessionPhase {
    fn is_established(&self) -> bool {
        matches!(self, SessionPhase::Established { .. })
    }
}

struct HandshakeAttempt {
    attempts: u32,
    last_sent_ms: u64,
    current_delay_ms: u32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct HandshakeResponse {
    pub accepted: bool,
    pub session_id: Option<u64>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TimeSyncSample {
    pub client_send_ms: u64,
    pub server_recv_ms: u64,
    pub server_send_ms: u64,
    pub client_recv_ms: u64,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct TimeSyncMetrics {
    pub offset_ms: Option<f64>,
    pub rtt_ms: Option<f64>,
    pub sample_count: usize,
}

struct TimeSyncClient {
    last_probe_ms: Option<u64>,
    samples: VecDeque<(f64, f64)>,
    max_samples: usize,
    metrics: TimeSyncMetrics,
}

impl TimeSyncClient {
    fn new(max_samples: usize) -> Self {
        Self {
            last_probe_ms: None,
            samples: VecDeque::with_capacity(max_samples),
            max_samples,
            metrics: TimeSyncMetrics::default(),
        }
    }

    fn mark_probe(&mut self, now_ms: u64) {
        self.last_probe_ms = Some(now_ms);
    }

    fn should_probe(&self, now_ms: u64, interval_ms: u32) -> bool {
        match self.last_probe_ms {
            Some(last) => now_ms.saturating_sub(last) >= u64::from(interval_ms),
            None => true,
        }
    }

    fn record(&mut self, sample: TimeSyncSample) {
        let turn_around_ms = (sample.server_send_ms as i64 - sample.server_recv_ms as i64) as f64;
        let rtt =
            (sample.client_recv_ms as i64 - sample.client_send_ms as i64) as f64 - turn_around_ms;
        let offset = ((sample.server_recv_ms as i64 - sample.client_send_ms as i64)
            + (sample.server_send_ms as i64 - sample.client_recv_ms as i64))
            as f64
            / 2.0;

        if self.samples.len() == self.max_samples {
            self.samples.pop_front();
        }
        self.samples.push_back((offset, rtt));
        self.metrics.sample_count = self.samples.len();
        let (sum_offset, sum_rtt) =
            self.samples.iter().fold((0.0, 0.0), |acc, (off, r)| (acc.0 + off, acc.1 + r));
        self.metrics.offset_ms = Some(sum_offset / self.samples.len() as f64);
        self.metrics.rtt_ms = Some(sum_rtt / self.samples.len() as f64);
    }

    fn metrics(&self) -> TimeSyncMetrics {
        self.metrics.clone()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct QuicSessionMetrics {
    pub phase: SessionPhase,
    pub last_handshake_ms: Option<u64>,
    pub time_sync: TimeSyncMetrics,
}

pub struct QuicSessionManager {
    _clock: Arc<dyn Fn() -> u64 + Send + Sync>,
    config: QuicSessionConfig,
    phase: SessionPhase,
    handshake: Option<HandshakeAttempt>,
    last_handshake_ms: Option<u64>,
    outbound: VecDeque<OutboundPacket>,
    time_sync: TimeSyncClient,
}

impl QuicSessionManager {
    pub fn new_with_clock(
        clock: Arc<dyn Fn() -> u64 + Send + Sync>,
        config: QuicSessionConfig,
    ) -> Self {
        let window = config.timesync_window.max(1);
        Self {
            _clock: clock,
            config,
            phase: SessionPhase::Idle,
            handshake: None,
            last_handshake_ms: None,
            outbound: VecDeque::new(),
            time_sync: TimeSyncClient::new(window),
        }
    }

    pub fn with_default_clock() -> Self {
        Self::new_with_clock(Arc::new(default_clock), QuicSessionConfig::default())
    }

    pub fn tick(&mut self, now_ms: u64) {
        match self.phase {
            SessionPhase::Idle => {
                self.send_handshake(now_ms, PacketKind::HandshakeInit);
            }
            SessionPhase::Handshaking { .. } => {
                if let Some(attempt) = &mut self.handshake {
                    let elapsed = now_ms.saturating_sub(attempt.last_sent_ms);
                    // Use exponential backoff for retries
                    let retry_delay = u64::from(attempt.current_delay_ms);
                    if elapsed >= retry_delay {
                        if attempt.attempts < self.config.max_retries {
                            let next_attempt = attempt.attempts + 1;
                            // Calculate next delay with exponential backoff
                            let next_delay = (attempt.current_delay_ms as f32
                                * self.config.backoff_multiplier)
                                .min(self.config.max_retry_ms as f32)
                                as u32;
                            attempt.current_delay_ms = next_delay;
                            self.send_handshake(now_ms, PacketKind::HandshakeRetry);
                            self.phase = SessionPhase::Handshaking { attempts: next_attempt };
                        } else {
                            self.phase =
                                SessionPhase::Failed { reason: "handshake timeout".to_owned() };
                            self.handshake = None;
                        }
                    }
                }
            }
            SessionPhase::Established { .. } => {
                if self.time_sync.should_probe(now_ms, self.config.timesync_interval_ms) {
                    self.enqueue_packet(PacketKind::TimeSyncProbe, vec![0xAA], now_ms);
                    self.time_sync.mark_probe(now_ms);
                }
            }
            SessionPhase::Failed { .. } => {}
        }
    }

    fn send_handshake(&mut self, now_ms: u64, kind: PacketKind) {
        self.enqueue_packet(kind, vec![0x01, kind as u8], now_ms);
        let (attempts, delay_ms) = match kind {
            PacketKind::HandshakeInit => (1, self.config.initial_retry_ms),
            PacketKind::HandshakeRetry => {
                let existing = self.handshake.as_ref();
                let attempts = existing.map(|h| h.attempts + 1).unwrap_or(1);
                let delay =
                    existing.map(|h| h.current_delay_ms).unwrap_or(self.config.initial_retry_ms);
                (attempts, delay)
            }
            PacketKind::TimeSyncProbe => (0, 0),
        };
        self.handshake =
            Some(HandshakeAttempt { attempts, last_sent_ms: now_ms, current_delay_ms: delay_ms });
        if !matches!(kind, PacketKind::TimeSyncProbe) {
            self.phase = SessionPhase::Handshaking { attempts };
        }
    }

    fn enqueue_packet(&mut self, kind: PacketKind, payload: Vec<u8>, timestamp_ms: u64) {
        self.outbound.push_back(OutboundPacket { kind, payload, timestamp_ms });
    }

    pub fn drain_outbound(&mut self) -> Vec<OutboundPacket> {
        self.outbound.drain(..).collect()
    }

    pub fn on_handshake_response(&mut self, response: HandshakeResponse, now_ms: u64) {
        if !matches!(self.phase, SessionPhase::Handshaking { .. }) {
            return;
        }
        if response.accepted {
            let session_id = response.session_id.unwrap_or(0);
            self.phase = SessionPhase::Established { session_id };
            self.handshake = None;
            self.last_handshake_ms = Some(now_ms);
        } else {
            self.phase = SessionPhase::Failed { reason: "handshake rejected".to_owned() };
            self.handshake = None;
        }
    }

    pub fn on_time_sync_response(&mut self, sample: TimeSyncSample) {
        if !self.phase.is_established() {
            return;
        }
        self.time_sync.record(sample);
    }

    pub fn metrics(&self) -> QuicSessionMetrics {
        QuicSessionMetrics {
            phase: self.phase.clone(),
            last_handshake_ms: self.last_handshake_ms,
            time_sync: self.time_sync.metrics(),
        }
    }
}
