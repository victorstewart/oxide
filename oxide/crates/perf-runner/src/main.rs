use oxide_renderer_api as api;
use oxide_renderer_api::Renderer;
use oxide_renderer_metal as metal;
use oxide_test_scenes as scenes;
use oxide_timing as timing;
use oxide_ui_core as ui;

struct Uploader {
    r: *mut metal::MetalRenderer,
}
unsafe impl Send for Uploader {}
unsafe impl Sync for Uploader {}
impl ui::elements::ImageUploader for Uploader {
    fn create_a8(&mut self, w: u32, h: u32, data: &[u8], row_bytes: usize) -> api::ImageHandle {
        unsafe { (*self.r).image_create_a8(w, h, data, row_bytes) }
    }
    fn update_a8(
        &mut self,
        handle: api::ImageHandle,
        x: u32,
        y: u32,
        w: u32,
        h: u32,
        data: &[u8],
        row_bytes: usize,
    ) {
        unsafe { (*self.r).image_update_a8(handle, x, y, w, h, data, row_bytes) }
    }
}

fn gen_checker_rgba(w: u32, h: u32) -> (u32, u32, Vec<u8>) {
    let mut v = vec![0u8; (w as usize) * (h as usize) * 4];
    for y in 0..h {
        for x in 0..w {
            let i = ((y * w + x) * 4) as usize;
            let c = if ((x / 16) + (y / 16)) % 2 == 0 { 220 } else { 180 };
            v[i] = c;
            v[i + 1] = c;
            v[i + 2] = c;
            v[i + 3] = 255;
        }
    }
    (w, h, v)
}

fn run_scene(
    r: &mut metal::MetalRenderer,
    router: &mut scenes::Router<Uploader>,
    frames: usize,
    vp: api::RectF,
    scale: f32,
) {
    let mut builder = ui::DrawListBuilder::new();
    for _ in 0..frames {
        let now = timing::now_ms();
        router.update(now, 16);
        builder.clear();
        router.draw(vp, scale, &mut builder);
        let damage = api::Damage { rects: router.take_damage() };
        let token = r.begin_frame(&api::FrameTarget, Some(&damage));
        // Optional coalesce pass
        ui::coalesce_adjacent_draws(builder.drawlist_mut());
        r.encode_pass(builder.drawlist());
        let _ = r.submit(token);
    }
}

fn main() {
    // Ensure damage is enabled for profiling
    std::env::set_var("OXIDE_ENABLE_DAMAGE", "1");
    // Allow tuning thresholds via env before process start
    let mut r = metal::MetalRenderer::new_default().expect("metal");
    let (w, h, scale) = (1200u32, 800u32, 2.0f32);
    let _ = r.resize(w, h, scale);
    let mut boxed = Box::new(r);
    let ptr: *mut metal::MetalRenderer = &mut *boxed;
    let uploader = Uploader { r: ptr };
    let mut router = scenes::Router::new(uploader);
    // Provide an image for the zoom scene
    let (zw, zh, zrgba) = gen_checker_rgba(512, 512);
    unsafe {
        let tex = (*ptr).image_create_rgba8(zw, zh, &zrgba, (zw as usize) * 4);
        router.set_zoom_image(tex, zw, zh);
    }
    // Sweep thresholds sets if provided via env lists, else run once
    let use_list = std::env::var("OXIDE_SWEEP_USE").ok();
    let pf_list = std::env::var("OXIDE_SWEEP_PREFILTER").ok();
    let vp = api::RectF::new(0.0, 0.0, (w as f32) / scale, (h as f32) / scale);
    if let (Some(ul), Some(pl)) = (use_list, pf_list) {
        let use_vals: Vec<f32> = ul.split(',').filter_map(|s| s.parse().ok()).collect();
        let pf_vals: Vec<f32> = pl.split(',').filter_map(|s| s.parse().ok()).collect();
        for &u in &use_vals {
            for &p in &pf_vals {
                // Recreate renderer to pick up env
                drop(boxed);
                std::env::set_var("OXIDE_DAMAGE_USE_THRESH", format!("{}", u));
                std::env::set_var("OXIDE_DAMAGE_PREFILTER_THRESH", format!("{}", p));
                let mut r2 = metal::MetalRenderer::new_default().expect("metal");
                let _ = r2.resize(w, h, scale);
                boxed = Box::new(r2);
                let rref: &mut metal::MetalRenderer = &mut boxed;
                let uploader2 = Uploader { r: rref as *mut _ };
                // Fresh router per sweep to keep state consistent
                let mut router2 = scenes::Router::new(uploader2);
                let tex = rref.image_create_rgba8(zw, zh, &zrgba, (zw as usize) * 4);
                router2.set_zoom_image(tex, zw, zh);
                // Controls
                router2.set_scene(0);
                run_scene(rref, &mut router2, 60, vp, scale);
                // AnimTimeline
                router2.set_scene(3);
                run_scene(rref, &mut router2, 60, vp, scale);
                // Collection
                router2.set_scene(4);
                run_scene(rref, &mut router2, 60, vp, scale);
                // ZoomImage
                router2.set_scene(2);
                run_scene(rref, &mut router2, 60, vp, scale);
                let s = rref.last_stats();
                println!(
                    "u={:.2} p={:.2} -> enc_ms={:.2} draws={} inst={} culled={} dmg%={:.0}",
                    u,
                    p,
                    s.encode_ms,
                    s.draws,
                    s.instanced,
                    s.culled,
                    (s.damage_pct * 100.0).round()
                );
            }
        }
    } else {
        // Single run for quick smoke
        router.set_scene(0);
        run_scene(&mut boxed, &mut router, 120, vp, scale);
        router.set_scene(3);
        run_scene(&mut boxed, &mut router, 120, vp, scale);
        router.set_scene(4);
        run_scene(&mut boxed, &mut router, 120, vp, scale);
        router.set_scene(2);
        run_scene(&mut boxed, &mut router, 120, vp, scale);
        let s = boxed.last_stats();
        println!(
            "enc_ms={:.2} draws={} inst={} culled={} dmg%={:.0}",
            s.encode_ms,
            s.draws,
            s.instanced,
            s.culled,
            (s.damage_pct * 100.0).round()
        );
    }
}
