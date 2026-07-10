#[path = "../src/wasm/image_slots.rs"]
mod image_slots;

use image_slots::ImageSlots;

#[test]
fn released_and_malformed_handles_never_resolve_after_slot_reuse()
{
   let mut slots = ImageSlots::new();
   assert!(slots.get(0).is_none());
   assert!(slots.get(1).is_none());
   assert!(slots.get(1 << u16::BITS).is_none());
   assert!(slots.remove(0).is_none());

   let first = slots.insert("first").expect("first slot");
   assert_eq!(slots.get(first), Some(&"first"));
   assert_eq!(slots.remove(first), Some("first"));
   assert!(slots.get(first).is_none());
   assert!(slots.remove(first).is_none());

   let second = slots.insert("second").expect("reused slot");
   assert_ne!(second, first);
   assert_eq!(second & u32::from(u16::MAX), first & u32::from(u16::MAX));
   assert!(slots.get(first).is_none());
   assert_eq!(slots.get(second), Some(&"second"));
}

#[test]
fn double_release_cannot_duplicate_a_free_slot()
{
   let mut slots = ImageSlots::new();
   let released = slots.insert(1_u8).expect("released slot");
   assert_eq!(slots.remove(released), Some(1));
   assert!(slots.remove(released).is_none());

   let reused = slots.insert(2_u8).expect("reused slot");
   let separate = slots.insert(3_u8).expect("separate slot");
   assert_ne!(reused, separate);
   assert_eq!(slots.get(reused), Some(&2));
   assert_eq!(slots.get(separate), Some(&3));
}

#[test]
fn generation_exhaustion_retires_instead_of_wrapping_a_slot()
{
   let mut slots = ImageSlots::new();
   let mut handle = slots.insert(0_u16).expect("initial slot");
   let original_slot = handle & u32::from(u16::MAX);
   for generation in 1..=u16::MAX
   {
      assert_eq!((handle >> u16::BITS) as u16, generation);
      assert_eq!(slots.remove(handle), Some(generation - 1));
      if generation != u16::MAX
      {
         handle = slots.insert(generation).expect("next generation");
         assert_eq!(handle & u32::from(u16::MAX), original_slot);
      }
   }

   let replacement = slots.insert(u16::MAX).expect("replacement slot");
   assert_ne!(replacement & u32::from(u16::MAX), original_slot);
   assert!(slots.get(handle).is_none());
   assert_eq!(slots.get(replacement), Some(&u16::MAX));
}

#[test]
fn live_capacity_is_hard_bounded_and_recoverable_after_release()
{
   let mut slots = ImageSlots::new();
   let mut handles = Vec::with_capacity(u16::MAX as usize);
   for value in 0..u16::MAX
   {
      handles.push(slots.insert(value).expect("live slot capacity"));
   }
   assert!(!slots.has_capacity());
   assert_eq!(slots.insert(u16::MAX), Err(u16::MAX));

   let released = handles[17];
   assert_eq!(slots.remove(released), Some(17));
   assert!(slots.has_capacity());
   let replacement = slots.insert(u16::MAX).expect("released capacity");
   assert_ne!(replacement, released);
   assert_eq!(slots.get(replacement), Some(&u16::MAX));
   assert!(slots.get(released).is_none());
}

#[test]
fn repeated_churn_keeps_resource_table_capacity_at_warm_peak()
{
   let mut slots = ImageSlots::new();
   let handle = slots.insert(1_u32).expect("warm slot");
   assert_eq!(slots.remove(handle), Some(1));
   let warm_capacity = slots.storage_capacity_bytes();

   for value in 0..10_000_u32
   {
      let handle = slots.insert(value).expect("churn slot");
      assert_eq!(slots.remove(handle), Some(value));
   }
   assert_eq!(slots.storage_capacity_bytes(), warm_capacity);
}
