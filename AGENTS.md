# AGENTS.md — Repo rules (Rust)

## Activation
This is a Rust workspace. Load exactly this profile:
- Read `~/.codex/rules/rust/GPT5_RULES.md` and follow it verbatim.
- Do not load any other language profile.

## Local overrides (optional, narrow)
- Use only when a task explicitly opts into a subfolder-specific behavior.
- Document any local override at the top of the PR description.

## Formatting
- **NEVER run `cargo fmt` or `cargo clippy` in this repository.** Our manual style cannot be expressed via rustfmt; automated formatting will churn every file.
- Mirror the canonical snippet below for indentation (3 spaces), brace placement (Allman), and import grouping/wrapping.

```rust
use databento::dbn;
use databento::{HistoricalClient, Symbols};
use dbn::{Dataset, HasRType, SType, Schema};
use std::sync::Arc;
use thingbuf::ThingBuf;
use time::{Duration, OffsetDateTime, Time};

use super::frontier::{frontier_merge, spawn_batched, Ring, WorkerEvt};
use super::handlers::{batches, ContinuousRolled, CorpRow, HasTimestamp, ProcessMessage};
use super::helpers::{ring_pair, symbols_len};
use super::limits::{DEF_HIST_CONN_SEM, HIST_CONN_SEM, HIST_WM_GROUP_SIZE, LIVE_CONN_SEM, LIVE_WM_GROUP_SIZE, PayloadBatchSpec, WORKER_RING_CAP};
use super::workers::{hist_worker, live_worker};
use crate::types::Span;

pub type Sender<P> = Arc<ThingBuf<P>>;
pub type Receiver<P> = Arc<ThingBuf<P>>;

#[allow(dead_code)]
pub trait RxExt
{
   fn is_finished(&self) -> bool;
}

#[allow(dead_code)]
impl<T> RxExt for Receiver<T>
{
   #[inline]
   fn is_finished(&self) -> bool
   {
      Arc::strong_count(self) == 1 && self.is_empty()
   }
}

#[derive(Clone)]
pub struct Databento
{
   pub key: String,
   pub historical: HistoricalClient,
}
```

- Functions and impl blocks use one-line signatures, braces on their own lines, and 3-space indentation inside the block.
- Imports are grouped and wrapped exactly as shown (std → external crates → crate-local).
- Manual edits only—review diffs carefully to preserve legibility.

## Tooling
- Manual formatting fixes must maintain the style canon above; no automated formatter may be used.
- Deprecation warnings are blockers: whenever a deprecated API is observed in touched code or command output, replace it with the upstream-recommended supported API in the same change.
