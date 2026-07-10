const SLOT_INDEX_MASK: u32 = u16::MAX as u32;
const SLOT_CAPACITY: usize = SLOT_INDEX_MASK as usize;

struct Slot<T>
{
   value: Option<T>,
   generation: u16,
}

pub(crate) struct ImageSlots<T>
{
   slots: Vec<Slot<T>>,
   free: Vec<u16>,
}

impl<T> ImageSlots<T>
{
   pub(crate) const fn new() -> Self
   {
      Self { slots: Vec::new(), free: Vec::new() }
   }

   pub(crate) fn has_capacity(&self) -> bool
   {
      !self.free.is_empty() || self.slots.len() < SLOT_CAPACITY
   }

   pub(crate) fn insert(&mut self, value: T) -> Result<u32, T>
   {
      let slot_index = if let Some(slot_index) = self.free.pop()
      {
         usize::from(slot_index)
      }
      else
      {
         if self.slots.len() >= SLOT_CAPACITY
         {
            return Err(value);
         }
         let slot_index = self.slots.len();
         self.slots.push(Slot { value: None, generation: 1 });
         slot_index
      };
      let slot = &mut self.slots[slot_index];
      debug_assert!(slot.value.is_none());
      slot.value = Some(value);
      Ok(encode_handle(slot_index, slot.generation))
   }

   pub(crate) fn get(&self, handle: u32) -> Option<&T>
   {
      let (slot_index, generation) = decode_handle(handle)?;
      self.slots
         .get(slot_index)
         .filter(|slot| slot.generation == generation)
         .and_then(|slot| slot.value.as_ref())
   }

   pub(crate) fn remove(&mut self, handle: u32) -> Option<T>
   {
      let (slot_index, generation) = decode_handle(handle)?;
      let slot = self
         .slots
         .get_mut(slot_index)
         .filter(|slot| slot.generation == generation)?;
      let value = slot.value.take()?;
      if let Some(next_generation) = slot.generation.checked_add(1)
      {
         slot.generation = next_generation;
         self.free.push(slot_index as u16);
      }
      Some(value)
   }

   pub(crate) fn storage_capacity_bytes(&self) -> usize
   {
      self.slots
         .capacity()
         .saturating_mul(core::mem::size_of::<Slot<T>>())
         .saturating_add(
            self.free
               .capacity()
               .saturating_mul(core::mem::size_of::<u16>()),
         )
   }
}

fn decode_handle(handle: u32) -> Option<(usize, u16)>
{
   let encoded_slot = handle & SLOT_INDEX_MASK;
   let generation = (handle >> u16::BITS) as u16;
   (encoded_slot != 0 && generation != 0).then(|| ((encoded_slot - 1) as usize, generation))
}

fn encode_handle(slot_index: usize, generation: u16) -> u32
{
   (u32::from(generation) << u16::BITS) | (slot_index as u32 + 1)
}
