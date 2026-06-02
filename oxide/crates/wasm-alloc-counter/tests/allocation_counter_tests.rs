use std::alloc::{GlobalAlloc, Layout, System};

use oxide_wasm_alloc_counter::{snapshot, CountingAllocator};

static TEST_ALLOCATOR: CountingAllocator<System> = CountingAllocator::new(System);

#[test]
fn direct_alloc_and_dealloc_update_counters()
{
   let layout = Layout::from_size_align(64, 8).unwrap();
   let before = snapshot();

   let ptr = unsafe { TEST_ALLOCATOR.alloc(layout) };
   assert!(!ptr.is_null());

   let after_alloc = snapshot();
   assert_eq!(after_alloc.alloc_count.saturating_sub(before.alloc_count), 1);
   assert_eq!(after_alloc.alloc_bytes.saturating_sub(before.alloc_bytes), 64);
   assert!(after_alloc.live_bytes >= before.live_bytes.saturating_add(64));
   assert!(after_alloc.peak_live_bytes >= after_alloc.live_bytes);

   unsafe { TEST_ALLOCATOR.dealloc(ptr, layout) };

   let after_dealloc = snapshot();
   assert_eq!(
      after_dealloc.dealloc_count.saturating_sub(after_alloc.dealloc_count),
      1,
   );
   assert_eq!(
      after_dealloc.dealloc_bytes.saturating_sub(after_alloc.dealloc_bytes),
      64,
   );
}

#[test]
fn direct_realloc_updates_realloc_and_live_byte_deltas()
{
   let layout = Layout::from_size_align(32, 8).unwrap();
   let before = snapshot();

   let ptr = unsafe { TEST_ALLOCATOR.alloc(layout) };
   assert!(!ptr.is_null());

   let grown = unsafe { TEST_ALLOCATOR.realloc(ptr, layout, 96) };
   assert!(!grown.is_null());

   let after_grow = snapshot();
   assert_eq!(after_grow.realloc_count.saturating_sub(before.realloc_count), 1);
   assert_eq!(
      after_grow.realloc_grow_bytes.saturating_sub(before.realloc_grow_bytes),
      64,
   );

   let grown_layout = Layout::from_size_align(96, 8).unwrap();
   let shrunk = unsafe { TEST_ALLOCATOR.realloc(grown, grown_layout, 48) };
   assert!(!shrunk.is_null());

   let after_shrink = snapshot();
   assert_eq!(
      after_shrink.realloc_count.saturating_sub(after_grow.realloc_count),
      1,
   );
   assert_eq!(
      after_shrink
         .realloc_shrink_bytes
         .saturating_sub(after_grow.realloc_shrink_bytes),
      48,
   );

   let shrunk_layout = Layout::from_size_align(48, 8).unwrap();
   unsafe { TEST_ALLOCATOR.dealloc(shrunk, shrunk_layout) };
}
