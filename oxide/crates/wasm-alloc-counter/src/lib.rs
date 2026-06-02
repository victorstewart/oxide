//! Counting global allocator used by Oxide WebAssembly benchmark builds.

#![deny(unsafe_op_in_unsafe_fn)]

use std::alloc::{GlobalAlloc, Layout};
use std::sync::atomic::{AtomicU64, Ordering};

static ALLOC_COUNT: AtomicU64 = AtomicU64::new(0);
static ALLOC_BYTES: AtomicU64 = AtomicU64::new(0);
static DEALLOC_COUNT: AtomicU64 = AtomicU64::new(0);
static DEALLOC_BYTES: AtomicU64 = AtomicU64::new(0);
static REALLOC_COUNT: AtomicU64 = AtomicU64::new(0);
static REALLOC_GROW_BYTES: AtomicU64 = AtomicU64::new(0);
static REALLOC_SHRINK_BYTES: AtomicU64 = AtomicU64::new(0);
static LIVE_BYTES: AtomicU64 = AtomicU64::new(0);
static PEAK_LIVE_BYTES: AtomicU64 = AtomicU64::new(0);

/// Point-in-time allocation counters.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct AllocationSnapshot
{
   pub alloc_count: u64,
   pub alloc_bytes: u64,
   pub dealloc_count: u64,
   pub dealloc_bytes: u64,
   pub realloc_count: u64,
   pub realloc_grow_bytes: u64,
   pub realloc_shrink_bytes: u64,
   pub live_bytes: u64,
   pub peak_live_bytes: u64,
}

/// Global allocator wrapper that records successful allocation activity.
pub struct CountingAllocator<A>
{
   inner: A,
}

impl<A> CountingAllocator<A>
{
   #[must_use]
   pub const fn new(inner: A) -> Self
   {
      Self { inner }
   }
}

/// Returns the current allocation counters.
#[must_use]
pub fn snapshot() -> AllocationSnapshot
{
   AllocationSnapshot {
      alloc_count: ALLOC_COUNT.load(Ordering::Relaxed),
      alloc_bytes: ALLOC_BYTES.load(Ordering::Relaxed),
      dealloc_count: DEALLOC_COUNT.load(Ordering::Relaxed),
      dealloc_bytes: DEALLOC_BYTES.load(Ordering::Relaxed),
      realloc_count: REALLOC_COUNT.load(Ordering::Relaxed),
      realloc_grow_bytes: REALLOC_GROW_BYTES.load(Ordering::Relaxed),
      realloc_shrink_bytes: REALLOC_SHRINK_BYTES.load(Ordering::Relaxed),
      live_bytes: LIVE_BYTES.load(Ordering::Relaxed),
      peak_live_bytes: PEAK_LIVE_BYTES.load(Ordering::Relaxed),
   }
}

fn add_live_bytes(bytes: u64)
{
   let live = LIVE_BYTES.fetch_add(bytes, Ordering::Relaxed).saturating_add(bytes);
   let mut peak = PEAK_LIVE_BYTES.load(Ordering::Relaxed);
   while live > peak {
      match PEAK_LIVE_BYTES.compare_exchange_weak(
         peak,
         live,
         Ordering::Relaxed,
         Ordering::Relaxed,
      ) {
         Ok(_) => break,
         Err(observed) => peak = observed,
      }
   }
}

fn sub_live_bytes(bytes: u64)
{
   let _ = LIVE_BYTES.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |live| {
      Some(live.saturating_sub(bytes))
   });
}

fn record_alloc(bytes: u64)
{
   ALLOC_COUNT.fetch_add(1, Ordering::Relaxed);
   ALLOC_BYTES.fetch_add(bytes, Ordering::Relaxed);
   add_live_bytes(bytes);
}

fn record_dealloc(bytes: u64)
{
   DEALLOC_COUNT.fetch_add(1, Ordering::Relaxed);
   DEALLOC_BYTES.fetch_add(bytes, Ordering::Relaxed);
   sub_live_bytes(bytes);
}

fn record_realloc(old_bytes: u64, new_bytes: u64)
{
   REALLOC_COUNT.fetch_add(1, Ordering::Relaxed);
   if new_bytes > old_bytes {
      let delta = new_bytes.saturating_sub(old_bytes);
      REALLOC_GROW_BYTES.fetch_add(delta, Ordering::Relaxed);
      add_live_bytes(delta);
   } else if old_bytes > new_bytes {
      let delta = old_bytes.saturating_sub(new_bytes);
      REALLOC_SHRINK_BYTES.fetch_add(delta, Ordering::Relaxed);
      sub_live_bytes(delta);
   }
}

// SAFETY: This wrapper forwards all allocation requests to `inner` with the
// original layouts and pointers. It only records counters after successful
// allocation/reallocation and before deallocation; it does not change ownership,
// alignment, size, or lifetime requirements of the wrapped allocator.
unsafe impl<A: GlobalAlloc> GlobalAlloc for CountingAllocator<A>
{
   unsafe fn alloc(&self, layout: Layout) -> *mut u8
   {
      let ptr = unsafe { self.inner.alloc(layout) };
      if !ptr.is_null() {
         record_alloc(layout.size() as u64);
      }
      ptr
   }

   unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout)
   {
      record_dealloc(layout.size() as u64);
      unsafe { self.inner.dealloc(ptr, layout) };
   }

   unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8
   {
      let ptr = unsafe { self.inner.alloc_zeroed(layout) };
      if !ptr.is_null() {
         record_alloc(layout.size() as u64);
      }
      ptr
   }

   unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8
   {
      let new_ptr = unsafe { self.inner.realloc(ptr, layout, new_size) };
      if !new_ptr.is_null() {
         record_realloc(layout.size() as u64, new_size as u64);
      }
      new_ptr
   }
}
