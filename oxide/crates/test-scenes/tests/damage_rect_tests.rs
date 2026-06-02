use oxide_renderer_api as gfx;
use oxide_test_scenes::Router;
use oxide_ui_core::DrawListBuilder;
use oxide_wasm_alloc_counter::{snapshot, CountingAllocator};
use std::alloc::System;
use std::sync::Mutex;

mod helpers;

use helpers::NullUploader;

#[global_allocator]
static ALLOCATOR: CountingAllocator<System> = CountingAllocator::new(System);
static ALLOCATION_TEST_LOCK: Mutex<()> = Mutex::new(());

fn viewport() -> gfx::RectF {
    gfx::RectF::new(0.0, 0.0, 390.0, 844.0)
}

fn viewport_damage() -> gfx::RectI {
    gfx::RectI::new(0, 0, 390, 844)
}

#[test]
fn damage_lab_scene_switch_forces_one_full_redraw_before_partial_damage() {
    let _guard = ALLOCATION_TEST_LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut router = Router::new(NullUploader);
    router.toggle_overlay();

    assert!(router.prepare_onscreen_benchmark("damage_lab_frame"));

    let mut builder = DrawListBuilder::new();
    router.draw(viewport(), 1.0, &mut builder);
    assert_eq!(router.take_damage(), vec![viewport_damage()]);

    assert!(router.step_onscreen_benchmark("damage_lab_frame", 0));

    let mut builder = DrawListBuilder::new();
    router.draw(viewport(), 1.0, &mut builder);
    let damage = router.take_damage();

    assert!(!damage.iter().any(|rect| *rect == viewport_damage()));
    assert_eq!(damage, vec![gfx::RectI::new(8, 8, 374, 128)]);
}

#[test]
fn damage_handoff_can_reuse_caller_storage() {
    let _guard = ALLOCATION_TEST_LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut router = Router::new(NullUploader);
    let mut builder = DrawListBuilder::new();
    let mut damage = Vec::with_capacity(8);
    let original_capacity = damage.capacity();

    router.draw(viewport(), 1.0, &mut builder);
    router.take_damage_into(&mut damage);

    assert!(!damage.is_empty());

    damage.clear();
    router.draw(viewport(), 1.0, &mut builder);
    router.take_damage_into(&mut damage);

    assert!(damage.capacity() >= original_capacity);
    assert!(!damage.is_empty());
}

#[test]
fn warmed_overlay_draw_reuses_text_scratch_without_allocating() {
    let _guard = ALLOCATION_TEST_LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut router = Router::new(NullUploader);
    let mut builder = DrawListBuilder::new();
    let mut damage = Vec::with_capacity(8);

    router.draw(viewport(), 1.0, &mut builder);
    router.take_damage_into(&mut damage);

    builder.clear();
    let before = snapshot();
    router.draw(viewport(), 1.0, &mut builder);
    let after = snapshot();

    assert_eq!(after.alloc_count - before.alloc_count, 0);
    assert_eq!(after.realloc_count - before.realloc_count, 0);
    assert!(!builder.drawlist().items.is_empty());
}
