# Oxide WebAssembly Browser Baseline

Date: 2026-06-02

Target: Chrome arm64 via headless CLI

Capture target: `app`

URL: `http://127.0.0.1:<ephemeral>/?frame_samples=3&frames_per_sample=12&id_mask_samples=3&id_mask_frames=12&upload_samples=3&upload_frames=12&scene3d_samples=3&scene3d_frames=12&mixed_samples=3&mixed_frames=12&capture_target=app&capture_width=320&capture_height=240&report_endpoint=1`

Status: browser-baseline. This is the browser-specific WebGPU/WebAssembly baseline for the current web backend. It is not an official device parity report.

## Smoke

| Check | Result |
| --- | --- |
| Platform | `caps=40;online=true;location=not-determined;webview=ok` |
| WebGPU probe | `webgpu=device-ok` |
| WebGPU timing | `timestamp_query=adapter-supported;gpu_stage_attribution=renderer-timestamp-query-enabled;source=adapter.features` |
| Renderer backend | `webgpu` |
| Renderer | `draws=512` |
| Capture target | `app` |

## Cases

| Case | Variant | Samples | Frames/Sample | Frames | p50 ms | p95 ms | p99 ms | Peak ms | Avg ms | Unit | Notes |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | --- | --- |
| `web.wasm.webgpu.frame_loop` | `webgpu` | 3 | 12 | 36 | 0.100 | 0.300 | 0.300 | 0.300 | 0.119 | ms/frame | `draws=26;draw_items=26;draw_pipeline_binds=8;draw_bind_group_binds=5;draw_scissor_sets=3;solid_tris=792;image_draws=0;image_mesh_draws=0;nine_slice_draws=0;glyph_quads=60;sdf_glyph_quads=0;clip_depth_peak=1;damage_rects=3;layer_draws=0;scene3d_draws=0;id_mask_draws=0;backdrop_draws=1;visual_effect_draws=0;effect_uniform_writes=1;effect_uniform_bytes=16;effect_uniform_slots=1;spinner_draws=1;camera_bg_draws=0;render_passes=5;passes=clear:1/draw:3/scene3d:0/scene3d_overlay:0/idmask:0+0+0+0/present:1;texture_copies=1;command_buffers=1;gpu_ts_passes=5;gpu_ts_total_ns=146958;gpu_ts_max_ns=61458;buffer_upload_bytes=48696;texture_upload_bytes=0;buffer_grows=0;texture_creates=0;bind_group_creates=0;pipeline_creates=0;sampler_creates=0;mesh3d_creates=0;wasm_alloc_count=236;wasm_alloc_bytes=4652;wasm_realloc_count=0;wasm_realloc_grow_bytes=0;wasm_allocating_frames=36;image_upload_temp_allocs=0;image_upload_temp_bytes=0;image_upload_scratch_bytes=17640;image_upload_scratch_grows=0;cpu_scratch_bytes=162328;cpu_scratch_grows=0;cpu_scratch_growth_bytes=0;missed120=0.000;hitch120=0.000;missed60=0.000;hitch60=0.000` |
| `web.wasm.webgpu.id_mask_compositor.current` | `webgpu-current` | 3 | 12 | 36 | 0.083 | 0.083 | 0.083 | 0.083 | 0.081 | ms/frame | `draws=12;draw_items=0;draw_pipeline_binds=0;draw_bind_group_binds=0;draw_scissor_sets=0;solid_tris=0;image_draws=0;image_mesh_draws=0;nine_slice_draws=0;glyph_quads=0;sdf_glyph_quads=0;clip_depth_peak=0;damage_rects=0;layer_draws=0;scene3d_draws=0;id_mask_draws=1;backdrop_draws=0;visual_effect_draws=0;effect_uniform_writes=0;effect_uniform_bytes=0;effect_uniform_slots=0;spinner_draws=0;camera_bg_draws=0;render_passes=12;passes=clear:0/draw:0/scene3d:0/scene3d_overlay:0/idmask:1+1+9+1/present:0;texture_copies=0;command_buffers=1;gpu_ts_passes=12;gpu_ts_total_ns=2668501;gpu_ts_max_ns=402500;buffer_upload_bytes=1120;texture_upload_bytes=0;buffer_grows=0;texture_creates=0;bind_group_creates=0;pipeline_creates=0;sampler_creates=0;mesh3d_creates=0;wasm_alloc_count=236;wasm_alloc_bytes=4652;wasm_realloc_count=0;wasm_realloc_grow_bytes=0;wasm_allocating_frames=36;image_upload_temp_allocs=0;image_upload_temp_bytes=0;image_upload_scratch_bytes=17640;image_upload_scratch_grows=0;cpu_scratch_bytes=473640;cpu_scratch_grows=0;cpu_scratch_growth_bytes=0;vertices=9600;vertex_bytes=307200;missed120=0.000;hitch120=0.000;missed60=0.000;hitch60=0.000` |
| `web.wasm.webgpu.id_mask_compositor.legacy_upload` | `webgpu-legacy-upload` | 3 | 12 | 36 | 0.167 | 0.192 | 0.192 | 0.192 | 0.167 | ms/frame | `draws=12;draw_items=0;draw_pipeline_binds=0;draw_bind_group_binds=0;draw_scissor_sets=0;solid_tris=0;image_draws=0;image_mesh_draws=0;nine_slice_draws=0;glyph_quads=0;sdf_glyph_quads=0;clip_depth_peak=0;damage_rects=0;layer_draws=0;scene3d_draws=0;id_mask_draws=1;backdrop_draws=0;visual_effect_draws=0;effect_uniform_writes=0;effect_uniform_bytes=0;effect_uniform_slots=0;spinner_draws=0;camera_bg_draws=0;render_passes=12;passes=clear:0/draw:0/scene3d:0/scene3d_overlay:0/idmask:1+1+9+1/present:0;texture_copies=0;command_buffers=1;gpu_ts_passes=12;gpu_ts_total_ns=1025560;gpu_ts_max_ns=126152;buffer_upload_bytes=308320;texture_upload_bytes=0;buffer_grows=0;texture_creates=0;bind_group_creates=0;pipeline_creates=0;sampler_creates=0;mesh3d_creates=0;wasm_alloc_count=236;wasm_alloc_bytes=4652;wasm_realloc_count=0;wasm_realloc_grow_bytes=0;wasm_allocating_frames=36;image_upload_temp_allocs=0;image_upload_temp_bytes=0;image_upload_scratch_bytes=17640;image_upload_scratch_grows=0;cpu_scratch_bytes=473640;cpu_scratch_grows=0;cpu_scratch_growth_bytes=0;vertices=9600;vertex_bytes=307200;missed120=0.000;hitch120=0.000;missed60=0.000;hitch60=0.000` |
| `web.wasm.webgpu.glyph_atlas_upload.current_dirty` | `webgpu-dirty-atlas-update` | 3 | 12 | 36 | 0.050 | 0.050 | 0.050 | 0.050 | 0.050 | ms/frame | `draws=2;draw_items=2;draw_pipeline_binds=2;draw_bind_group_binds=1;draw_scissor_sets=1;solid_tris=36;image_draws=0;image_mesh_draws=0;nine_slice_draws=0;glyph_quads=96;sdf_glyph_quads=0;clip_depth_peak=0;damage_rects=0;layer_draws=0;scene3d_draws=0;id_mask_draws=0;backdrop_draws=0;visual_effect_draws=0;effect_uniform_writes=0;effect_uniform_bytes=0;effect_uniform_slots=0;spinner_draws=0;camera_bg_draws=0;render_passes=1;passes=clear:0/draw:1/scene3d:0/scene3d_overlay:0/idmask:0+0+0+0/present:0;texture_copies=0;command_buffers=1;gpu_ts_passes=1;gpu_ts_total_ns=23165;gpu_ts_max_ns=23165;buffer_upload_bytes=22368;texture_upload_bytes=16384;buffer_grows=0;texture_creates=0;bind_group_creates=0;pipeline_creates=0;sampler_creates=0;mesh3d_creates=0;wasm_alloc_count=236;wasm_alloc_bytes=4652;wasm_realloc_count=0;wasm_realloc_grow_bytes=0;wasm_allocating_frames=36;image_upload_temp_allocs=0;image_upload_temp_bytes=0;image_upload_scratch_bytes=17640;image_upload_scratch_grows=0;cpu_scratch_bytes=484840;cpu_scratch_grows=0;cpu_scratch_growth_bytes=0;atlas=1024x1024;dirty=64x64;missed120=0.000;hitch120=0.000;missed60=0.000;hitch60=0.000` |
| `web.wasm.webgpu.glyph_atlas_upload.legacy_full` | `webgpu-full-atlas-update` | 3 | 12 | 36 | 2.700 | 2.700 | 2.700 | 2.700 | 2.661 | ms/frame | `draws=2;draw_items=2;draw_pipeline_binds=2;draw_bind_group_binds=1;draw_scissor_sets=1;solid_tris=36;image_draws=0;image_mesh_draws=0;nine_slice_draws=0;glyph_quads=96;sdf_glyph_quads=0;clip_depth_peak=0;damage_rects=0;layer_draws=0;scene3d_draws=0;id_mask_draws=0;backdrop_draws=0;visual_effect_draws=0;effect_uniform_writes=0;effect_uniform_bytes=0;effect_uniform_slots=0;spinner_draws=0;camera_bg_draws=0;render_passes=1;passes=clear:0/draw:1/scene3d:0/scene3d_overlay:0/idmask:0+0+0+0/present:0;texture_copies=0;command_buffers=1;gpu_ts_passes=1;gpu_ts_total_ns=11832;gpu_ts_max_ns=11832;buffer_upload_bytes=22368;texture_upload_bytes=4194304;buffer_grows=0;texture_creates=0;bind_group_creates=0;pipeline_creates=0;sampler_creates=0;mesh3d_creates=0;wasm_alloc_count=236;wasm_alloc_bytes=4652;wasm_realloc_count=0;wasm_realloc_grow_bytes=0;wasm_allocating_frames=36;image_upload_temp_allocs=0;image_upload_temp_bytes=0;image_upload_scratch_bytes=4194304;image_upload_scratch_grows=0;cpu_scratch_bytes=4661504;cpu_scratch_grows=0;cpu_scratch_growth_bytes=0;atlas=1024x1024;dirty=1024x1024;missed120=0.000;hitch120=0.000;missed60=0.000;hitch60=0.000` |
| `web.wasm.webgpu.image_upload.current_dirty` | `webgpu-dirty-rgba-update` | 3 | 12 | 36 | 0.025 | 0.033 | 0.033 | 0.033 | 0.025 | ms/frame | `draws=2;draw_items=2;draw_pipeline_binds=2;draw_bind_group_binds=1;draw_scissor_sets=1;solid_tris=36;image_draws=1;image_mesh_draws=0;nine_slice_draws=0;glyph_quads=0;sdf_glyph_quads=0;clip_depth_peak=0;damage_rects=0;layer_draws=0;scene3d_draws=0;id_mask_draws=0;backdrop_draws=0;visual_effect_draws=0;effect_uniform_writes=0;effect_uniform_bytes=0;effect_uniform_slots=0;spinner_draws=0;camera_bg_draws=0;render_passes=1;passes=clear:0/draw:1/scene3d:0/scene3d_overlay:0/idmask:0+0+0+0/present:0;texture_copies=0;command_buffers=1;gpu_ts_passes=1;gpu_ts_total_ns=11499;gpu_ts_max_ns=11499;buffer_upload_bytes=1784;texture_upload_bytes=16384;buffer_grows=0;texture_creates=0;bind_group_creates=0;pipeline_creates=0;sampler_creates=0;mesh3d_creates=0;wasm_alloc_count=236;wasm_alloc_bytes=4652;wasm_realloc_count=0;wasm_realloc_grow_bytes=0;wasm_allocating_frames=36;image_upload_temp_allocs=0;image_upload_temp_bytes=0;image_upload_scratch_bytes=4194304;image_upload_scratch_grows=0;cpu_scratch_bytes=4661504;cpu_scratch_grows=0;cpu_scratch_growth_bytes=0;image=256x256;dirty=64x64;missed120=0.000;hitch120=0.000;missed60=0.000;hitch60=0.000` |
| `web.wasm.webgpu.image_upload.legacy_full` | `webgpu-full-rgba-update` | 3 | 12 | 36 | 0.100 | 0.292 | 0.292 | 0.292 | 0.161 | ms/frame | `draws=2;draw_items=2;draw_pipeline_binds=2;draw_bind_group_binds=1;draw_scissor_sets=1;solid_tris=36;image_draws=1;image_mesh_draws=0;nine_slice_draws=0;glyph_quads=0;sdf_glyph_quads=0;clip_depth_peak=0;damage_rects=0;layer_draws=0;scene3d_draws=0;id_mask_draws=0;backdrop_draws=0;visual_effect_draws=0;effect_uniform_writes=0;effect_uniform_bytes=0;effect_uniform_slots=0;spinner_draws=0;camera_bg_draws=0;render_passes=1;passes=clear:0/draw:1/scene3d:0/scene3d_overlay:0/idmask:0+0+0+0/present:0;texture_copies=0;command_buffers=1;gpu_ts_passes=1;gpu_ts_total_ns=11833;gpu_ts_max_ns=11833;buffer_upload_bytes=1784;texture_upload_bytes=262144;buffer_grows=0;texture_creates=0;bind_group_creates=0;pipeline_creates=0;sampler_creates=0;mesh3d_creates=0;wasm_alloc_count=236;wasm_alloc_bytes=4652;wasm_realloc_count=0;wasm_realloc_grow_bytes=0;wasm_allocating_frames=36;image_upload_temp_allocs=0;image_upload_temp_bytes=0;image_upload_scratch_bytes=4194304;image_upload_scratch_grows=0;cpu_scratch_bytes=4661504;cpu_scratch_grows=0;cpu_scratch_growth_bytes=0;image=256x256;dirty=256x256;missed120=0.000;hitch120=0.000;missed60=0.000;hitch60=0.000` |
| `web.wasm.webgpu.upload_scratch.current_reuse` | `webgpu-upload-scratch-current-reuse` | 3 | 12 | 36 | 0.350 | 0.367 | 0.367 | 0.367 | 0.353 | ms/frame | `draws=3;draw_items=3;draw_pipeline_binds=3;draw_bind_group_binds=2;draw_scissor_sets=1;solid_tris=36;image_draws=1;image_mesh_draws=0;nine_slice_draws=0;glyph_quads=36;sdf_glyph_quads=0;clip_depth_peak=0;damage_rects=0;layer_draws=0;scene3d_draws=0;id_mask_draws=0;backdrop_draws=0;visual_effect_draws=0;effect_uniform_writes=0;effect_uniform_bytes=0;effect_uniform_slots=0;spinner_draws=0;camera_bg_draws=0;render_passes=1;passes=clear:0/draw:1/scene3d:0/scene3d_overlay:0/idmask:0+0+0+0/present:0;texture_copies=0;command_buffers=1;gpu_ts_passes=1;gpu_ts_total_ns=9749;gpu_ts_max_ns=9749;buffer_upload_bytes=9560;texture_upload_bytes=786432;buffer_grows=0;texture_creates=0;bind_group_creates=0;pipeline_creates=0;sampler_creates=0;mesh3d_creates=0;wasm_alloc_count=236;wasm_alloc_bytes=4652;wasm_realloc_count=0;wasm_realloc_grow_bytes=0;wasm_allocating_frames=36;image_upload_temp_allocs=0;image_upload_temp_bytes=0;image_upload_scratch_bytes=4194304;image_upload_scratch_grows=0;cpu_scratch_bytes=4661504;cpu_scratch_grows=0;cpu_scratch_growth_bytes=0;updates=24;missed120=0.000;hitch120=0.000;missed60=0.000;hitch60=0.000` |
| `web.wasm.webgpu.upload_scratch.legacy_temp_alloc` | `webgpu-upload-scratch-legacy-temp-alloc` | 3 | 12 | 36 | 0.417 | 0.458 | 0.458 | 0.458 | 0.425 | ms/frame | `draws=3;draw_items=3;draw_pipeline_binds=3;draw_bind_group_binds=2;draw_scissor_sets=1;solid_tris=36;image_draws=1;image_mesh_draws=0;nine_slice_draws=0;glyph_quads=36;sdf_glyph_quads=0;clip_depth_peak=0;damage_rects=0;layer_draws=0;scene3d_draws=0;id_mask_draws=0;backdrop_draws=0;visual_effect_draws=0;effect_uniform_writes=0;effect_uniform_bytes=0;effect_uniform_slots=0;spinner_draws=0;camera_bg_draws=0;render_passes=1;passes=clear:0/draw:1/scene3d:0/scene3d_overlay:0/idmask:0+0+0+0/present:0;texture_copies=0;command_buffers=1;gpu_ts_passes=1;gpu_ts_total_ns=10209;gpu_ts_max_ns=10209;buffer_upload_bytes=9560;texture_upload_bytes=786432;buffer_grows=0;texture_creates=0;bind_group_creates=0;pipeline_creates=0;sampler_creates=0;mesh3d_creates=0;wasm_alloc_count=2828;wasm_alloc_bytes=31855148;wasm_realloc_count=0;wasm_realloc_grow_bytes=0;wasm_allocating_frames=36;image_upload_temp_allocs=72;image_upload_temp_bytes=884736;image_upload_scratch_bytes=4194304;image_upload_scratch_grows=0;cpu_scratch_bytes=4661504;cpu_scratch_grows=0;cpu_scratch_growth_bytes=0;updates=24;missed120=0.000;hitch120=0.000;missed60=0.000;hitch60=0.000` |
| `web.wasm.webgpu.effect_uniform.current_batched` | `webgpu-effect-uniform-current-batched` | 3 | 12 | 36 | 0.192 | 0.233 | 0.233 | 0.233 | 0.206 | ms/frame | `draws=50;draw_items=50;draw_pipeline_binds=50;draw_bind_group_binds=97;draw_scissor_sets=49;solid_tris=4;image_draws=1;image_mesh_draws=0;nine_slice_draws=0;glyph_quads=0;sdf_glyph_quads=0;clip_depth_peak=0;damage_rects=0;layer_draws=0;scene3d_draws=0;id_mask_draws=0;backdrop_draws=48;visual_effect_draws=0;effect_uniform_writes=1;effect_uniform_bytes=16;effect_uniform_slots=48;spinner_draws=0;camera_bg_draws=0;render_passes=51;passes=clear:1/draw:49/scene3d:0/scene3d_overlay:0/idmask:0+0+0+0/present:1;texture_copies=48;command_buffers=1;gpu_ts_passes=51;gpu_ts_total_ns=1382227;gpu_ts_max_ns=160118;buffer_upload_bytes=7688;texture_upload_bytes=0;buffer_grows=0;texture_creates=0;bind_group_creates=0;pipeline_creates=0;sampler_creates=0;mesh3d_creates=0;wasm_alloc_count=236;wasm_alloc_bytes=4652;wasm_realloc_count=0;wasm_realloc_grow_bytes=0;wasm_allocating_frames=36;image_upload_temp_allocs=0;image_upload_temp_bytes=0;image_upload_scratch_bytes=4194304;image_upload_scratch_grows=0;cpu_scratch_bytes=4663168;cpu_scratch_grows=0;cpu_scratch_growth_bytes=0;expected_backdrops=48;missed120=0.000;hitch120=0.000;missed60=0.000;hitch60=0.000` |
| `web.wasm.webgpu.effect_uniform.legacy_write_each` | `webgpu-effect-uniform-legacy-write-each` | 3 | 12 | 36 | 0.208 | 0.233 | 0.233 | 0.233 | 0.211 | ms/frame | `draws=50;draw_items=50;draw_pipeline_binds=50;draw_bind_group_binds=97;draw_scissor_sets=49;solid_tris=4;image_draws=1;image_mesh_draws=0;nine_slice_draws=0;glyph_quads=0;sdf_glyph_quads=0;clip_depth_peak=0;damage_rects=0;layer_draws=0;scene3d_draws=0;id_mask_draws=0;backdrop_draws=48;visual_effect_draws=0;effect_uniform_writes=48;effect_uniform_bytes=768;effect_uniform_slots=48;spinner_draws=0;camera_bg_draws=0;render_passes=51;passes=clear:1/draw:49/scene3d:0/scene3d_overlay:0/idmask:0+0+0+0/present:1;texture_copies=48;command_buffers=1;gpu_ts_passes=51;gpu_ts_total_ns=1276376;gpu_ts_max_ns=90331;buffer_upload_bytes=8440;texture_upload_bytes=0;buffer_grows=0;texture_creates=0;bind_group_creates=0;pipeline_creates=0;sampler_creates=0;mesh3d_creates=0;wasm_alloc_count=236;wasm_alloc_bytes=4652;wasm_realloc_count=0;wasm_realloc_grow_bytes=0;wasm_allocating_frames=36;image_upload_temp_allocs=0;image_upload_temp_bytes=0;image_upload_scratch_bytes=4194304;image_upload_scratch_grows=0;cpu_scratch_bytes=4663168;cpu_scratch_grows=0;cpu_scratch_growth_bytes=0;expected_backdrops=48;missed120=0.000;hitch120=0.000;missed60=0.000;hitch60=0.000` |
| `web.wasm.webgpu.backdrop_batch.current_coalesced` | `webgpu-backdrop-batch-current-coalesced` | 3 | 12 | 36 | 0.025 | 0.025 | 0.025 | 0.025 | 0.022 | ms/frame | `draws=14;draw_items=14;draw_pipeline_binds=3;draw_bind_group_binds=3;draw_scissor_sets=2;solid_tris=4;image_draws=1;image_mesh_draws=0;nine_slice_draws=0;glyph_quads=0;sdf_glyph_quads=0;clip_depth_peak=0;damage_rects=0;layer_draws=0;scene3d_draws=0;id_mask_draws=0;backdrop_draws=12;visual_effect_draws=0;effect_uniform_writes=1;effect_uniform_bytes=16;effect_uniform_slots=12;spinner_draws=0;camera_bg_draws=0;render_passes=4;passes=clear:1/draw:2/scene3d:0/scene3d_overlay:0/idmask:0+0+0+0/present:1;texture_copies=1;command_buffers=1;gpu_ts_passes=4;gpu_ts_total_ns=59623;gpu_ts_max_ns=24332;buffer_upload_bytes=2216;texture_upload_bytes=0;buffer_grows=0;texture_creates=0;bind_group_creates=0;pipeline_creates=0;sampler_creates=0;mesh3d_creates=0;wasm_alloc_count=236;wasm_alloc_bytes=4652;wasm_realloc_count=0;wasm_realloc_grow_bytes=0;wasm_allocating_frames=36;image_upload_temp_allocs=0;image_upload_temp_bytes=0;image_upload_scratch_bytes=4194304;image_upload_scratch_grows=0;cpu_scratch_bytes=4663168;cpu_scratch_grows=0;cpu_scratch_growth_bytes=0;expected_backdrops=12;missed120=0.000;hitch120=0.000;missed60=0.000;hitch60=0.000` |
| `web.wasm.webgpu.backdrop_batch.legacy_per_backdrop_copy` | `webgpu-backdrop-batch-legacy-per-backdrop-copy` | 3 | 12 | 36 | 0.058 | 0.058 | 0.058 | 0.058 | 0.056 | ms/frame | `draws=14;draw_items=14;draw_pipeline_binds=14;draw_bind_group_binds=25;draw_scissor_sets=13;solid_tris=4;image_draws=1;image_mesh_draws=0;nine_slice_draws=0;glyph_quads=0;sdf_glyph_quads=0;clip_depth_peak=0;damage_rects=0;layer_draws=0;scene3d_draws=0;id_mask_draws=0;backdrop_draws=12;visual_effect_draws=0;effect_uniform_writes=1;effect_uniform_bytes=16;effect_uniform_slots=12;spinner_draws=0;camera_bg_draws=0;render_passes=15;passes=clear:1/draw:13/scene3d:0/scene3d_overlay:0/idmask:0+0+0+0/present:1;texture_copies=12;command_buffers=1;gpu_ts_passes=15;gpu_ts_total_ns=332534;gpu_ts_max_ns=34916;buffer_upload_bytes=2216;texture_upload_bytes=0;buffer_grows=0;texture_creates=0;bind_group_creates=0;pipeline_creates=0;sampler_creates=0;mesh3d_creates=0;wasm_alloc_count=236;wasm_alloc_bytes=4652;wasm_realloc_count=0;wasm_realloc_grow_bytes=0;wasm_allocating_frames=36;image_upload_temp_allocs=0;image_upload_temp_bytes=0;image_upload_scratch_bytes=4194304;image_upload_scratch_grows=0;cpu_scratch_bytes=4663168;cpu_scratch_grows=0;cpu_scratch_growth_bytes=0;expected_backdrops=12;missed120=0.000;hitch120=0.000;missed60=0.000;hitch60=0.000` |
| `web.wasm.webgpu.scene3d.reused_mesh` | `webgpu-scene3d-reused-mesh` | 3 | 12 | 36 | 0.017 | 0.017 | 0.017 | 0.017 | 0.014 | ms/frame | `draws=2;draw_items=0;draw_pipeline_binds=0;draw_bind_group_binds=0;draw_scissor_sets=0;solid_tris=0;image_draws=0;image_mesh_draws=0;nine_slice_draws=0;glyph_quads=0;sdf_glyph_quads=0;clip_depth_peak=0;damage_rects=0;layer_draws=0;scene3d_draws=2;id_mask_draws=0;backdrop_draws=0;visual_effect_draws=0;effect_uniform_writes=0;effect_uniform_bytes=0;effect_uniform_slots=0;spinner_draws=0;camera_bg_draws=0;render_passes=1;passes=clear:0/draw:0/scene3d:1/scene3d_overlay:0/idmask:0+0+0+0/present:0;texture_copies=0;command_buffers=1;gpu_ts_passes=1;gpu_ts_total_ns=9291;gpu_ts_max_ns=9291;buffer_upload_bytes=528;texture_upload_bytes=0;buffer_grows=0;texture_creates=0;bind_group_creates=0;pipeline_creates=0;sampler_creates=0;mesh3d_creates=0;wasm_alloc_count=236;wasm_alloc_bytes=4652;wasm_realloc_count=0;wasm_realloc_grow_bytes=0;wasm_allocating_frames=36;image_upload_temp_allocs=0;image_upload_temp_bytes=0;image_upload_scratch_bytes=4194304;image_upload_scratch_grows=0;cpu_scratch_bytes=4664120;cpu_scratch_grows=0;cpu_scratch_growth_bytes=0;meshes=2;instances=2;missed120=0.000;hitch120=0.000;missed60=0.000;hitch60=0.000` |
| `web.wasm.webgpu.scene3d.recreate_mesh` | `webgpu-scene3d-recreate-mesh` | 3 | 12 | 36 | 0.025 | 0.025 | 0.025 | 0.025 | 0.022 | ms/frame | `draws=2;draw_items=0;draw_pipeline_binds=0;draw_bind_group_binds=0;draw_scissor_sets=0;solid_tris=0;image_draws=0;image_mesh_draws=0;nine_slice_draws=0;glyph_quads=0;sdf_glyph_quads=0;clip_depth_peak=0;damage_rects=0;layer_draws=0;scene3d_draws=2;id_mask_draws=0;backdrop_draws=0;visual_effect_draws=0;effect_uniform_writes=0;effect_uniform_bytes=0;effect_uniform_slots=0;spinner_draws=0;camera_bg_draws=0;render_passes=1;passes=clear:0/draw:0/scene3d:1/scene3d_overlay:0/idmask:0+0+0+0/present:0;texture_copies=0;command_buffers=1;gpu_ts_passes=1;gpu_ts_total_ns=8250;gpu_ts_max_ns=8250;buffer_upload_bytes=720;texture_upload_bytes=0;buffer_grows=4;texture_creates=0;bind_group_creates=0;pipeline_creates=0;sampler_creates=0;mesh3d_creates=2;wasm_alloc_count=668;wasm_alloc_bytes=26540;wasm_realloc_count=3;wasm_realloc_grow_bytes=6272;wasm_allocating_frames=36;image_upload_temp_allocs=0;image_upload_temp_bytes=0;image_upload_scratch_bytes=4194304;image_upload_scratch_grows=0;cpu_scratch_bytes=4670840;cpu_scratch_grows=0;cpu_scratch_growth_bytes=0;meshes=2;instances=2;missed120=0.000;hitch120=0.000;missed60=0.000;hitch60=0.000` |
| `web.wasm.webgpu.scene3d.stress_reused_mesh` | `webgpu-scene3d-stress-reused-mesh` | 3 | 12 | 36 | 0.058 | 0.067 | 0.067 | 0.067 | 0.061 | ms/frame | `draws=96;draw_items=0;draw_pipeline_binds=0;draw_bind_group_binds=0;draw_scissor_sets=0;solid_tris=0;image_draws=0;image_mesh_draws=0;nine_slice_draws=0;glyph_quads=0;sdf_glyph_quads=0;clip_depth_peak=0;damage_rects=0;layer_draws=0;scene3d_draws=96;id_mask_draws=0;backdrop_draws=0;visual_effect_draws=0;effect_uniform_writes=0;effect_uniform_bytes=0;effect_uniform_slots=0;spinner_draws=0;camera_bg_draws=0;render_passes=1;passes=clear:0/draw:0/scene3d:1/scene3d_overlay:0/idmask:0+0+0+0/present:0;texture_copies=0;command_buffers=1;gpu_ts_passes=1;gpu_ts_total_ns=11792;gpu_ts_max_ns=11792;buffer_upload_bytes=24592;texture_upload_bytes=0;buffer_grows=0;texture_creates=0;bind_group_creates=0;pipeline_creates=0;sampler_creates=0;mesh3d_creates=0;wasm_alloc_count=236;wasm_alloc_bytes=4652;wasm_realloc_count=0;wasm_realloc_grow_bytes=0;wasm_allocating_frames=36;image_upload_temp_allocs=0;image_upload_temp_bytes=0;image_upload_scratch_bytes=4194304;image_upload_scratch_grows=0;cpu_scratch_bytes=4704584;cpu_scratch_grows=0;cpu_scratch_growth_bytes=0;meshes=2;instances=96;missed120=0.000;hitch120=0.000;missed60=0.000;hitch60=0.000` |
| `web.wasm.webgpu.scene3d.stress_recreate_mesh` | `webgpu-scene3d-stress-recreate-mesh` | 3 | 12 | 36 | 0.067 | 0.092 | 0.092 | 0.092 | 0.075 | ms/frame | `draws=96;draw_items=0;draw_pipeline_binds=0;draw_bind_group_binds=0;draw_scissor_sets=0;solid_tris=0;image_draws=0;image_mesh_draws=0;nine_slice_draws=0;glyph_quads=0;sdf_glyph_quads=0;clip_depth_peak=0;damage_rects=0;layer_draws=0;scene3d_draws=96;id_mask_draws=0;backdrop_draws=0;visual_effect_draws=0;effect_uniform_writes=0;effect_uniform_bytes=0;effect_uniform_slots=0;spinner_draws=0;camera_bg_draws=0;render_passes=1;passes=clear:0/draw:0/scene3d:1/scene3d_overlay:0/idmask:0+0+0+0/present:0;texture_copies=0;command_buffers=1;gpu_ts_passes=1;gpu_ts_total_ns=11916;gpu_ts_max_ns=11916;buffer_upload_bytes=24784;texture_upload_bytes=0;buffer_grows=4;texture_creates=0;bind_group_creates=0;pipeline_creates=0;sampler_creates=0;mesh3d_creates=2;wasm_alloc_count=668;wasm_alloc_bytes=26540;wasm_realloc_count=1;wasm_realloc_grow_bytes=7168;wasm_allocating_frames=36;image_upload_temp_allocs=0;image_upload_temp_bytes=0;image_upload_scratch_bytes=4194304;image_upload_scratch_grows=0;cpu_scratch_bytes=4711752;cpu_scratch_grows=0;cpu_scratch_growth_bytes=0;meshes=2;instances=96;missed120=0.000;hitch120=0.000;missed60=0.000;hitch60=0.000` |
| `web.wasm.webgpu.mixed_text_image_effects` | `webgpu-mixed-effects-current` | 3 | 12 | 36 | 0.050 | 0.067 | 0.067 | 0.067 | 0.056 | ms/frame | `draws=114;draw_items=114;draw_pipeline_binds=8;draw_bind_group_binds=7;draw_scissor_sets=5;solid_tris=472;image_draws=97;image_mesh_draws=0;nine_slice_draws=0;glyph_quads=96;sdf_glyph_quads=0;clip_depth_peak=1;damage_rects=2;layer_draws=1;scene3d_draws=0;id_mask_draws=0;backdrop_draws=1;visual_effect_draws=1;effect_uniform_writes=1;effect_uniform_bytes=272;effect_uniform_slots=2;spinner_draws=1;camera_bg_draws=0;render_passes=6;passes=clear:1/draw:4/scene3d:0/scene3d_overlay:0/idmask:0+0+0+0/present:1;texture_copies=2;command_buffers=1;gpu_ts_passes=6;gpu_ts_total_ns=85165;gpu_ts_max_ns=27583;buffer_upload_bytes=57288;texture_upload_bytes=0;buffer_grows=0;texture_creates=0;bind_group_creates=0;pipeline_creates=0;sampler_creates=0;mesh3d_creates=0;wasm_alloc_count=236;wasm_alloc_bytes=4652;wasm_realloc_count=0;wasm_realloc_grow_bytes=0;wasm_allocating_frames=36;image_upload_temp_allocs=0;image_upload_temp_bytes=0;image_upload_scratch_bytes=4194304;image_upload_scratch_grows=0;cpu_scratch_bytes=4753048;cpu_scratch_grows=0;cpu_scratch_growth_bytes=0;image=256x256;glyphs=96;image_tiles=96;missed120=0.000;hitch120=0.000;missed60=0.000;hitch60=0.000` |
| `web.wasm.webgpu.mixed_text_image_effects.legacy_rebind_unbatched` | `webgpu-mixed-effects-legacy-rebind-unbatched` | 3 | 12 | 36 | 0.075 | 0.083 | 0.083 | 0.083 | 0.078 | ms/frame | `draws=114;draw_items=114;draw_pipeline_binds=114;draw_bind_group_binds=102;draw_scissor_sets=114;solid_tris=472;image_draws=97;image_mesh_draws=0;nine_slice_draws=0;glyph_quads=96;sdf_glyph_quads=0;clip_depth_peak=1;damage_rects=2;layer_draws=1;scene3d_draws=0;id_mask_draws=0;backdrop_draws=1;visual_effect_draws=1;effect_uniform_writes=2;effect_uniform_bytes=32;effect_uniform_slots=2;spinner_draws=1;camera_bg_draws=0;render_passes=6;passes=clear:1/draw:4/scene3d:0/scene3d_overlay:0/idmask:0+0+0+0/present:1;texture_copies=2;command_buffers=1;gpu_ts_passes=6;gpu_ts_total_ns=122746;gpu_ts_max_ns=33374;buffer_upload_bytes=57048;texture_upload_bytes=0;buffer_grows=0;texture_creates=0;bind_group_creates=0;pipeline_creates=0;sampler_creates=0;mesh3d_creates=0;wasm_alloc_count=236;wasm_alloc_bytes=4652;wasm_realloc_count=0;wasm_realloc_grow_bytes=0;wasm_allocating_frames=36;image_upload_temp_allocs=0;image_upload_temp_bytes=0;image_upload_scratch_bytes=4194304;image_upload_scratch_grows=0;cpu_scratch_bytes=4753048;cpu_scratch_grows=0;cpu_scratch_growth_bytes=0;image=256x256;glyphs=96;image_tiles=96;missed120=0.000;hitch120=0.000;missed60=0.000;hitch60=0.000` |
| `web.wasm.webgpu.layer_damage_effects` | `webgpu-layer-damage-effects-current` | 3 | 12 | 36 | 0.050 | 0.058 | 0.058 | 0.058 | 0.053 | ms/frame | `draws=86;draw_items=86;draw_pipeline_binds=6;draw_bind_group_binds=12;draw_scissor_sets=4;solid_tris=508;image_draws=65;image_mesh_draws=0;nine_slice_draws=0;glyph_quads=72;sdf_glyph_quads=0;clip_depth_peak=1;damage_rects=3;layer_draws=3;scene3d_draws=0;id_mask_draws=0;backdrop_draws=4;visual_effect_draws=1;effect_uniform_writes=1;effect_uniform_bytes=1040;effect_uniform_slots=5;spinner_draws=1;camera_bg_draws=0;render_passes=5;passes=clear:1/draw:3/scene3d:0/scene3d_overlay:0/idmask:0+0+0+0/present:1;texture_copies=1;command_buffers=1;gpu_ts_passes=5;gpu_ts_total_ns=74164;gpu_ts_max_ns=25874;buffer_upload_bytes=50080;texture_upload_bytes=0;buffer_grows=0;texture_creates=0;bind_group_creates=0;pipeline_creates=0;sampler_creates=0;mesh3d_creates=0;wasm_alloc_count=236;wasm_alloc_bytes=4652;wasm_realloc_count=0;wasm_realloc_grow_bytes=0;wasm_allocating_frames=36;image_upload_temp_allocs=0;image_upload_temp_bytes=0;image_upload_scratch_bytes=4194304;image_upload_scratch_grows=0;cpu_scratch_bytes=4753816;cpu_scratch_grows=0;cpu_scratch_growth_bytes=0;image=256x256;glyphs=72;image_tiles=64;expected_layers=3;expected_damage_rects=3;expected_backdrops=4;missed120=0.000;hitch120=0.000;missed60=0.000;hitch60=0.000` |
| `web.wasm.webgpu.layer_damage_effects.legacy_rebind_unbatched` | `webgpu-layer-damage-effects-legacy-rebind-unbatched` | 3 | 12 | 36 | 0.075 | 0.083 | 0.083 | 0.083 | 0.078 | ms/frame | `draws=86;draw_items=86;draw_pipeline_binds=86;draw_bind_group_binds=76;draw_scissor_sets=86;solid_tris=508;image_draws=65;image_mesh_draws=0;nine_slice_draws=0;glyph_quads=72;sdf_glyph_quads=0;clip_depth_peak=1;damage_rects=3;layer_draws=3;scene3d_draws=0;id_mask_draws=0;backdrop_draws=4;visual_effect_draws=1;effect_uniform_writes=5;effect_uniform_bytes=80;effect_uniform_slots=5;spinner_draws=1;camera_bg_draws=0;render_passes=9;passes=clear:1/draw:7/scene3d:0/scene3d_overlay:0/idmask:0+0+0+0/present:1;texture_copies=5;command_buffers=1;gpu_ts_passes=9;gpu_ts_total_ns=179409;gpu_ts_max_ns=42915;buffer_upload_bytes=49120;texture_upload_bytes=0;buffer_grows=0;texture_creates=0;bind_group_creates=0;pipeline_creates=0;sampler_creates=0;mesh3d_creates=0;wasm_alloc_count=236;wasm_alloc_bytes=4652;wasm_realloc_count=0;wasm_realloc_grow_bytes=0;wasm_allocating_frames=36;image_upload_temp_allocs=0;image_upload_temp_bytes=0;image_upload_scratch_bytes=4194304;image_upload_scratch_grows=0;cpu_scratch_bytes=4753816;cpu_scratch_grows=0;cpu_scratch_growth_bytes=0;image=256x256;glyphs=72;image_tiles=64;expected_layers=3;expected_damage_rects=3;expected_backdrops=4;missed120=0.000;hitch120=0.000;missed60=0.000;hitch60=0.000` |
| `web.wasm.webgpu.command_family_matrix` | `webgpu-command-family-current` | 3 | 12 | 36 | 0.100 | 0.100 | 0.100 | 0.100 | 0.100 | ms/frame | `draws=649;draw_items=649;draw_pipeline_binds=3;draw_bind_group_binds=2;draw_scissor_sets=1;solid_tris=36;image_draws=640;image_mesh_draws=64;nine_slice_draws=64;glyph_quads=288;sdf_glyph_quads=288;clip_depth_peak=0;damage_rects=0;layer_draws=0;scene3d_draws=0;id_mask_draws=0;backdrop_draws=0;visual_effect_draws=0;effect_uniform_writes=0;effect_uniform_bytes=0;effect_uniform_slots=0;spinner_draws=0;camera_bg_draws=0;render_passes=1;passes=clear:0/draw:1/scene3d:0/scene3d_overlay:0/idmask:0+0+0+0/present:0;texture_copies=0;command_buffers=1;gpu_ts_passes=1;gpu_ts_total_ns=33873;gpu_ts_max_ns=33873;buffer_upload_bytes=165216;texture_upload_bytes=0;buffer_grows=0;texture_creates=0;bind_group_creates=0;pipeline_creates=0;sampler_creates=0;mesh3d_creates=0;wasm_alloc_count=236;wasm_alloc_bytes=4652;wasm_realloc_count=0;wasm_realloc_grow_bytes=0;wasm_allocating_frames=36;image_upload_temp_allocs=0;image_upload_temp_bytes=0;image_upload_scratch_bytes=4194304;image_upload_scratch_grows=0;cpu_scratch_bytes=5105648;cpu_scratch_grows=0;cpu_scratch_growth_bytes=0;image=256x256;expected_image_meshes=64;expected_nine_slices=64;expected_sdf_glyphs=288;expected_sdf_runs=8;expected_camera_bg=0;missed120=0.000;hitch120=0.000;missed60=0.000;hitch60=0.000` |
| `web.wasm.webgpu.command_family_matrix.legacy_rebind` | `webgpu-command-family-legacy-rebind` | 3 | 12 | 36 | 0.158 | 0.167 | 0.167 | 0.167 | 0.158 | ms/frame | `draws=649;draw_items=649;draw_pipeline_binds=649;draw_bind_group_binds=648;draw_scissor_sets=649;solid_tris=36;image_draws=640;image_mesh_draws=64;nine_slice_draws=64;glyph_quads=288;sdf_glyph_quads=288;clip_depth_peak=0;damage_rects=0;layer_draws=0;scene3d_draws=0;id_mask_draws=0;backdrop_draws=0;visual_effect_draws=0;effect_uniform_writes=0;effect_uniform_bytes=0;effect_uniform_slots=0;spinner_draws=0;camera_bg_draws=0;render_passes=1;passes=clear:0/draw:1/scene3d:0/scene3d_overlay:0/idmask:0+0+0+0/present:0;texture_copies=0;command_buffers=1;gpu_ts_passes=1;gpu_ts_total_ns=29583;gpu_ts_max_ns=29583;buffer_upload_bytes=165216;texture_upload_bytes=0;buffer_grows=0;texture_creates=0;bind_group_creates=0;pipeline_creates=0;sampler_creates=0;mesh3d_creates=0;wasm_alloc_count=236;wasm_alloc_bytes=4652;wasm_realloc_count=0;wasm_realloc_grow_bytes=0;wasm_allocating_frames=36;image_upload_temp_allocs=0;image_upload_temp_bytes=0;image_upload_scratch_bytes=4194304;image_upload_scratch_grows=0;cpu_scratch_bytes=5105648;cpu_scratch_grows=0;cpu_scratch_growth_bytes=0;image=256x256;expected_image_meshes=64;expected_nine_slices=64;expected_sdf_glyphs=288;expected_sdf_runs=8;expected_camera_bg=0;missed120=0.000;hitch120=0.000;missed60=0.000;hitch60=0.000` |
| `web.wasm.webgpu.glyph_run.current` | `webgpu-glyph-run-current` | 3 | 12 | 36 | 0.067 | 0.075 | 0.075 | 0.075 | 0.069 | ms/frame | `draws=65;draw_items=65;draw_pipeline_binds=3;draw_bind_group_binds=1;draw_scissor_sets=1;solid_tris=4;image_draws=0;image_mesh_draws=0;nine_slice_draws=0;glyph_quads=512;sdf_glyph_quads=256;clip_depth_peak=0;damage_rects=0;layer_draws=0;scene3d_draws=0;id_mask_draws=0;backdrop_draws=0;visual_effect_draws=0;effect_uniform_writes=0;effect_uniform_bytes=0;effect_uniform_slots=0;spinner_draws=0;camera_bg_draws=0;render_passes=1;passes=clear:0/draw:1/scene3d:0/scene3d_overlay:0/idmask:0+0+0+0/present:0;texture_copies=0;command_buffers=1;gpu_ts_passes=1;gpu_ts_total_ns=12875;gpu_ts_max_ns=12875;buffer_upload_bytes=110816;texture_upload_bytes=0;buffer_grows=0;texture_creates=0;bind_group_creates=0;pipeline_creates=0;sampler_creates=0;mesh3d_creates=0;wasm_alloc_count=236;wasm_alloc_bytes=4652;wasm_realloc_count=0;wasm_realloc_grow_bytes=0;wasm_allocating_frames=36;image_upload_temp_allocs=0;image_upload_temp_bytes=0;image_upload_scratch_bytes=4194304;image_upload_scratch_grows=0;cpu_scratch_bytes=5105648;cpu_scratch_grows=0;cpu_scratch_growth_bytes=0;atlas=1024x1024;expected_sdf_runs=32;expected_draw_items=65;missed120=0.000;hitch120=0.000;missed60=0.000;hitch60=0.000` |
| `web.wasm.webgpu.glyph_run.legacy_rebind` | `webgpu-glyph-run-legacy-rebind` | 3 | 12 | 36 | 0.083 | 0.092 | 0.092 | 0.092 | 0.083 | ms/frame | `draws=65;draw_items=65;draw_pipeline_binds=65;draw_bind_group_binds=64;draw_scissor_sets=65;solid_tris=4;image_draws=0;image_mesh_draws=0;nine_slice_draws=0;glyph_quads=512;sdf_glyph_quads=256;clip_depth_peak=0;damage_rects=0;layer_draws=0;scene3d_draws=0;id_mask_draws=0;backdrop_draws=0;visual_effect_draws=0;effect_uniform_writes=0;effect_uniform_bytes=0;effect_uniform_slots=0;spinner_draws=0;camera_bg_draws=0;render_passes=1;passes=clear:0/draw:1/scene3d:0/scene3d_overlay:0/idmask:0+0+0+0/present:0;texture_copies=0;command_buffers=1;gpu_ts_passes=1;gpu_ts_total_ns=28374;gpu_ts_max_ns=28374;buffer_upload_bytes=110816;texture_upload_bytes=0;buffer_grows=0;texture_creates=0;bind_group_creates=0;pipeline_creates=0;sampler_creates=0;mesh3d_creates=0;wasm_alloc_count=236;wasm_alloc_bytes=4652;wasm_realloc_count=0;wasm_realloc_grow_bytes=0;wasm_allocating_frames=36;image_upload_temp_allocs=0;image_upload_temp_bytes=0;image_upload_scratch_bytes=4194304;image_upload_scratch_grows=0;cpu_scratch_bytes=5105648;cpu_scratch_grows=0;cpu_scratch_growth_bytes=0;atlas=1024x1024;expected_sdf_runs=32;expected_draw_items=65;missed120=0.000;hitch120=0.000;missed60=0.000;hitch60=0.000` |
| `web.wasm.webgpu.neon_marker.current` | `webgpu-neon-marker-current` | 3 | 12 | 36 | 0.233 | 0.233 | 0.233 | 0.233 | 0.231 | ms/frame | `draws=192;draw_items=192;draw_pipeline_binds=1;draw_bind_group_binds=0;draw_scissor_sets=1;solid_tris=6912;image_draws=0;image_mesh_draws=0;nine_slice_draws=0;glyph_quads=0;sdf_glyph_quads=0;clip_depth_peak=0;damage_rects=0;layer_draws=0;scene3d_draws=0;id_mask_draws=0;backdrop_draws=0;visual_effect_draws=0;effect_uniform_writes=0;effect_uniform_bytes=0;effect_uniform_slots=0;spinner_draws=0;camera_bg_draws=0;render_passes=1;passes=clear:0/draw:1/scene3d:0/scene3d_overlay:0/idmask:0+0+0+0/present:0;texture_copies=0;command_buffers=1;gpu_ts_passes=1;gpu_ts_total_ns=11291;gpu_ts_max_ns=11291;buffer_upload_bytes=310288;texture_upload_bytes=0;buffer_grows=0;texture_creates=0;bind_group_creates=0;pipeline_creates=0;sampler_creates=0;mesh3d_creates=0;wasm_alloc_count=236;wasm_alloc_bytes=4652;wasm_realloc_count=0;wasm_realloc_grow_bytes=0;wasm_allocating_frames=36;image_upload_temp_allocs=0;image_upload_temp_bytes=0;image_upload_scratch_bytes=4194304;image_upload_scratch_grows=0;cpu_scratch_bytes=5399616;cpu_scratch_grows=0;cpu_scratch_growth_bytes=0;expected_draw_items=192;missed120=0.000;hitch120=0.000;missed60=0.000;hitch60=0.000` |
| `web.wasm.webgpu.neon_marker.legacy_rebind` | `webgpu-neon-marker-legacy-rebind` | 3 | 12 | 36 | 0.233 | 0.242 | 0.242 | 0.242 | 0.236 | ms/frame | `draws=192;draw_items=192;draw_pipeline_binds=192;draw_bind_group_binds=0;draw_scissor_sets=192;solid_tris=6912;image_draws=0;image_mesh_draws=0;nine_slice_draws=0;glyph_quads=0;sdf_glyph_quads=0;clip_depth_peak=0;damage_rects=0;layer_draws=0;scene3d_draws=0;id_mask_draws=0;backdrop_draws=0;visual_effect_draws=0;effect_uniform_writes=0;effect_uniform_bytes=0;effect_uniform_slots=0;spinner_draws=0;camera_bg_draws=0;render_passes=1;passes=clear:0/draw:1/scene3d:0/scene3d_overlay:0/idmask:0+0+0+0/present:0;texture_copies=0;command_buffers=1;gpu_ts_passes=1;gpu_ts_total_ns=11292;gpu_ts_max_ns=11292;buffer_upload_bytes=310288;texture_upload_bytes=0;buffer_grows=0;texture_creates=0;bind_group_creates=0;pipeline_creates=0;sampler_creates=0;mesh3d_creates=0;wasm_alloc_count=236;wasm_alloc_bytes=4652;wasm_realloc_count=0;wasm_realloc_grow_bytes=0;wasm_allocating_frames=36;image_upload_temp_allocs=0;image_upload_temp_bytes=0;image_upload_scratch_bytes=4194304;image_upload_scratch_grows=0;cpu_scratch_bytes=5399616;cpu_scratch_grows=0;cpu_scratch_growth_bytes=0;expected_draw_items=192;missed120=0.000;hitch120=0.000;missed60=0.000;hitch60=0.000` |
| `web.wasm.webgpu.direct_surface.current` | `webgpu-direct-surface-current` | 3 | 12 | 36 | 0.050 | 0.058 | 0.058 | 0.058 | 0.053 | ms/frame | `draws=385;draw_items=385;draw_pipeline_binds=2;draw_bind_group_binds=1;draw_scissor_sets=1;solid_tris=4;image_draws=384;image_mesh_draws=0;nine_slice_draws=0;glyph_quads=0;sdf_glyph_quads=0;clip_depth_peak=0;damage_rects=0;layer_draws=0;scene3d_draws=0;id_mask_draws=0;backdrop_draws=0;visual_effect_draws=0;effect_uniform_writes=0;effect_uniform_bytes=0;effect_uniform_slots=0;spinner_draws=0;camera_bg_draws=0;render_passes=1;passes=clear:0/draw:1/scene3d:0/scene3d_overlay:0/idmask:0+0+0+0/present:0;texture_copies=0;command_buffers=1;gpu_ts_passes=1;gpu_ts_total_ns=21625;gpu_ts_max_ns=21625;buffer_upload_bytes=58592;texture_upload_bytes=0;buffer_grows=0;texture_creates=0;bind_group_creates=0;pipeline_creates=0;sampler_creates=0;mesh3d_creates=0;wasm_alloc_count=236;wasm_alloc_bytes=4652;wasm_realloc_count=0;wasm_realloc_grow_bytes=0;wasm_allocating_frames=36;image_upload_temp_allocs=0;image_upload_temp_bytes=0;image_upload_scratch_bytes=4194304;image_upload_scratch_grows=0;cpu_scratch_bytes=5399616;cpu_scratch_grows=0;cpu_scratch_growth_bytes=0;image=256x256;expected_draw_items=385;missed120=0.000;hitch120=0.000;missed60=0.000;hitch60=0.000` |
| `web.wasm.webgpu.direct_surface.legacy_scene_present` | `webgpu-direct-surface-legacy-scene-present` | 3 | 12 | 36 | 0.058 | 0.058 | 0.058 | 0.058 | 0.058 | ms/frame | `draws=385;draw_items=385;draw_pipeline_binds=2;draw_bind_group_binds=1;draw_scissor_sets=1;solid_tris=4;image_draws=384;image_mesh_draws=0;nine_slice_draws=0;glyph_quads=0;sdf_glyph_quads=0;clip_depth_peak=0;damage_rects=0;layer_draws=0;scene3d_draws=0;id_mask_draws=0;backdrop_draws=0;visual_effect_draws=0;effect_uniform_writes=0;effect_uniform_bytes=0;effect_uniform_slots=0;spinner_draws=0;camera_bg_draws=0;render_passes=3;passes=clear:1/draw:1/scene3d:0/scene3d_overlay:0/idmask:0+0+0+0/present:1;texture_copies=0;command_buffers=1;gpu_ts_passes=3;gpu_ts_total_ns=48582;gpu_ts_max_ns=24499;buffer_upload_bytes=58592;texture_upload_bytes=0;buffer_grows=0;texture_creates=0;bind_group_creates=0;pipeline_creates=0;sampler_creates=0;mesh3d_creates=0;wasm_alloc_count=236;wasm_alloc_bytes=4652;wasm_realloc_count=0;wasm_realloc_grow_bytes=0;wasm_allocating_frames=36;image_upload_temp_allocs=0;image_upload_temp_bytes=0;image_upload_scratch_bytes=4194304;image_upload_scratch_grows=0;cpu_scratch_bytes=5399616;cpu_scratch_grows=0;cpu_scratch_growth_bytes=0;image=256x256;expected_draw_items=385;missed120=0.000;hitch120=0.000;missed60=0.000;hitch60=0.000` |
| `web.wasm.webgpu.draw_state_cache.current` | `webgpu-draw-state-cache-current` | 3 | 12 | 36 | 0.117 | 0.117 | 0.117 | 0.117 | 0.117 | ms/frame | `draws=1024;draw_items=1024;draw_pipeline_binds=1;draw_bind_group_binds=1;draw_scissor_sets=1;solid_tris=0;image_draws=1024;image_mesh_draws=0;nine_slice_draws=0;glyph_quads=0;sdf_glyph_quads=0;clip_depth_peak=0;damage_rects=0;layer_draws=0;scene3d_draws=0;id_mask_draws=0;backdrop_draws=0;visual_effect_draws=0;effect_uniform_writes=0;effect_uniform_bytes=0;effect_uniform_slots=0;spinner_draws=0;camera_bg_draws=0;render_passes=1;passes=clear:0/draw:1/scene3d:0/scene3d_overlay:0/idmask:0+0+0+0/present:0;texture_copies=0;command_buffers=1;gpu_ts_passes=1;gpu_ts_total_ns=58208;gpu_ts_max_ns=58208;buffer_upload_bytes=155664;texture_upload_bytes=0;buffer_grows=0;texture_creates=0;bind_group_creates=0;pipeline_creates=0;sampler_creates=0;mesh3d_creates=0;wasm_alloc_count=236;wasm_alloc_bytes=4652;wasm_realloc_count=0;wasm_realloc_grow_bytes=0;wasm_allocating_frames=36;image_upload_temp_allocs=0;image_upload_temp_bytes=0;image_upload_scratch_bytes=4194304;image_upload_scratch_grows=0;cpu_scratch_bytes=5399616;cpu_scratch_grows=0;cpu_scratch_growth_bytes=0;image=256x256;expected_draw_items=1024;missed120=0.000;hitch120=0.000;missed60=0.000;hitch60=0.000` |
| `web.wasm.webgpu.draw_state_cache.legacy_rebind` | `webgpu-draw-state-cache-legacy-rebind` | 3 | 12 | 36 | 0.192 | 0.225 | 0.225 | 0.225 | 0.203 | ms/frame | `draws=1024;draw_items=1024;draw_pipeline_binds=1024;draw_bind_group_binds=1024;draw_scissor_sets=1024;solid_tris=0;image_draws=1024;image_mesh_draws=0;nine_slice_draws=0;glyph_quads=0;sdf_glyph_quads=0;clip_depth_peak=0;damage_rects=0;layer_draws=0;scene3d_draws=0;id_mask_draws=0;backdrop_draws=0;visual_effect_draws=0;effect_uniform_writes=0;effect_uniform_bytes=0;effect_uniform_slots=0;spinner_draws=0;camera_bg_draws=0;render_passes=1;passes=clear:0/draw:1/scene3d:0/scene3d_overlay:0/idmask:0+0+0+0/present:0;texture_copies=0;command_buffers=1;gpu_ts_passes=1;gpu_ts_total_ns=58458;gpu_ts_max_ns=58458;buffer_upload_bytes=155664;texture_upload_bytes=0;buffer_grows=0;texture_creates=0;bind_group_creates=0;pipeline_creates=0;sampler_creates=0;mesh3d_creates=0;wasm_alloc_count=236;wasm_alloc_bytes=4652;wasm_realloc_count=0;wasm_realloc_grow_bytes=0;wasm_allocating_frames=36;image_upload_temp_allocs=0;image_upload_temp_bytes=0;image_upload_scratch_bytes=4194304;image_upload_scratch_grows=0;cpu_scratch_bytes=5399616;cpu_scratch_grows=0;cpu_scratch_growth_bytes=0;image=256x256;expected_draw_items=1024;missed120=0.000;hitch120=0.000;missed60=0.000;hitch60=0.000` |
| `web.wasm.webgpu.clip_state_cache.current` | `webgpu-clip-state-cache-current` | 3 | 12 | 36 | 0.067 | 0.067 | 0.067 | 0.067 | 0.064 | ms/frame | `draws=512;draw_items=512;draw_pipeline_binds=1;draw_bind_group_binds=1;draw_scissor_sets=16;solid_tris=0;image_draws=512;image_mesh_draws=0;nine_slice_draws=0;glyph_quads=0;sdf_glyph_quads=0;clip_depth_peak=2;damage_rects=0;layer_draws=0;scene3d_draws=0;id_mask_draws=0;backdrop_draws=0;visual_effect_draws=0;effect_uniform_writes=0;effect_uniform_bytes=0;effect_uniform_slots=0;spinner_draws=0;camera_bg_draws=0;render_passes=1;passes=clear:0/draw:1/scene3d:0/scene3d_overlay:0/idmask:0+0+0+0/present:0;texture_copies=0;command_buffers=1;gpu_ts_passes=1;gpu_ts_total_ns=23916;gpu_ts_max_ns=23916;buffer_upload_bytes=77840;texture_upload_bytes=0;buffer_grows=0;texture_creates=0;bind_group_creates=0;pipeline_creates=0;sampler_creates=0;mesh3d_creates=0;wasm_alloc_count=236;wasm_alloc_bytes=4652;wasm_realloc_count=0;wasm_realloc_grow_bytes=0;wasm_allocating_frames=36;image_upload_temp_allocs=0;image_upload_temp_bytes=0;image_upload_scratch_bytes=4194304;image_upload_scratch_grows=0;cpu_scratch_bytes=5399616;cpu_scratch_grows=0;cpu_scratch_growth_bytes=0;image=256x256;expected_draw_items=512;expected_clip_runs=16;expected_clip_depth=2;missed120=0.000;hitch120=0.000;missed60=0.000;hitch60=0.000` |
| `web.wasm.webgpu.clip_state_cache.legacy_rebind` | `webgpu-clip-state-cache-legacy-rebind` | 3 | 12 | 36 | 0.108 | 0.108 | 0.108 | 0.108 | 0.108 | ms/frame | `draws=512;draw_items=512;draw_pipeline_binds=512;draw_bind_group_binds=512;draw_scissor_sets=512;solid_tris=0;image_draws=512;image_mesh_draws=0;nine_slice_draws=0;glyph_quads=0;sdf_glyph_quads=0;clip_depth_peak=2;damage_rects=0;layer_draws=0;scene3d_draws=0;id_mask_draws=0;backdrop_draws=0;visual_effect_draws=0;effect_uniform_writes=0;effect_uniform_bytes=0;effect_uniform_slots=0;spinner_draws=0;camera_bg_draws=0;render_passes=1;passes=clear:0/draw:1/scene3d:0/scene3d_overlay:0/idmask:0+0+0+0/present:0;texture_copies=0;command_buffers=1;gpu_ts_passes=1;gpu_ts_total_ns=23917;gpu_ts_max_ns=23917;buffer_upload_bytes=77840;texture_upload_bytes=0;buffer_grows=0;texture_creates=0;bind_group_creates=0;pipeline_creates=0;sampler_creates=0;mesh3d_creates=0;wasm_alloc_count=236;wasm_alloc_bytes=4652;wasm_realloc_count=0;wasm_realloc_grow_bytes=0;wasm_allocating_frames=36;image_upload_temp_allocs=0;image_upload_temp_bytes=0;image_upload_scratch_bytes=4194304;image_upload_scratch_grows=0;cpu_scratch_bytes=5399616;cpu_scratch_grows=0;cpu_scratch_growth_bytes=0;image=256x256;expected_draw_items=512;expected_clip_runs=16;expected_clip_depth=2;missed120=0.000;hitch120=0.000;missed60=0.000;hitch60=0.000` |

## GPU Stage Attribution

| Field | Value |
| --- | --- |
| Timestamp query | `adapter-supported` |
| Status | `timestamp-query-collected` |
| Source | `adapter.features+renderer.timestamp_writes` |
| Collected rows | `33` |
| Collected passes | `200` |
| Total ns | `7834793` |

## Browser Trace

| Field | Value |
| --- | --- |
| Status | `collected` |
| Artifact | `/tmp/oxide-webgpu-browser-glyph-run-matrix.json` |
| Capture phase | `benchmark-report` |
| Timing source | `untraced-baseline-report` |
| Events | `89273` |
| GPU-related events | `55727` |
| WebGPU/Dawn events | `1670` |
| ANGLE events | `1` |
| Renderer events | `787` |
| Duration us | `1244147` |
| Category count | `25` |
| Sample categories | `WebCore,__metadata,benchmark,blink,blink.resource,blink.user_timing,blink_style,cc,devtools.timeline,disabled-by-default-blink.debug.layout,disabled-by-default-devtools.timeline,disabled-by-default-devtools.timeline.frame,disabled-by-default-display.framedisplayed,disabled-by-default-gpu.service,gpu,graphics.pipeline,input.scrolling,loading,navigation,raf_investigation,rail,shutdown,startup,toplevel.flow` |
| Benchmark trace mark status | `collected` |
| Benchmark trace mark events | `60` |
| Benchmark trace mark labels | `backdrop_batch_ab,clip_state_cache_ab,command_family_matrix,direct_surface_ab,draw_state_cache_ab,effect_uniform_ab,frame_loop,glyph_run_ab,id_mask_ab,layer_effects_matrix,mixed_matrix,neon_marker_ab,scene3d_ab,upload_ab,upload_scratch_ab` |

### Browser Trace Benchmark Intervals

| Mark | Duration us | Events | GPU events | WebGPU/Dawn events | Renderer events | Event duration us |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| `frame_loop` | 18159 | 3111 | 1806 | 53 | 30 | 177332 |
| `id_mask_ab` | 54344 | 6596 | 4146 | 113 | 86 | 464372 |
| `upload_ab` | 137628 | 11852 | 7766 | 185 | 146 | 1200614 |
| `upload_scratch_ab` | 46252 | 3867 | 2793 | 88 | 18 | 383655 |
| `effect_uniform_ab` | 212950 | 5871 | 3166 | 88 | 18 | 1611338 |
| `backdrop_batch_ab` | 44906 | 3730 | 2667 | 88 | 18 | 371790 |
| `scene3d_ab` | 38042 | 6646 | 5136 | 172 | 36 | 244989 |
| `mixed_matrix` | 37949 | 3590 | 2641 | 88 | 18 | 260650 |
| `layer_effects_matrix` | 38046 | 3591 | 2643 | 88 | 18 | 302221 |
| `command_family_matrix` | 21306 | 3511 | 2632 | 88 | 18 | 195620 |
| `glyph_run_ab` | 20817 | 3505 | 2665 | 88 | 18 | 166018 |
| `neon_marker_ab` | 29926 | 3820 | 2804 | 89 | 18 | 162893 |
| `direct_surface_ab` | 20903 | 3365 | 2579 | 88 | 18 | 162908 |
| `draw_state_cache_ab` | 22608 | 3495 | 2603 | 88 | 18 | 220691 |
| `clip_state_cache_ab` | 21402 | 3428 | 2587 | 88 | 18 | 131336 |

## Benchmark Marks

| Mark | Duration ms | Trace label | WASM before bytes | WASM after bytes | WASM growth bytes | JS heap before bytes | JS heap after bytes | JS heap growth bytes | JS heap sampled | GC exposed |
| --- | ---: | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| `frame_loop` | 21.200 | `yes` | 13041664 | 13041664 | 0 | 1223309 | 1259705 | 36396 | 1 | 1 |
| `id_mask_ab` | 63.200 | `yes` | 13041664 | 13041664 | 0 | 1260608 | 1307491 | 46883 | 1 | 1 |
| `upload_ab` | 137.800 | `yes` | 13041664 | 13041664 | 0 | 1307627 | 1332176 | 24549 | 1 | 1 |
| `upload_scratch_ab` | 46.600 | `yes` | 13041664 | 13041664 | 0 | 1332534 | 1344015 | 11481 | 1 | 1 |
| `effect_uniform_ab` | 205.800 | `yes` | 13041664 | 13041664 | 0 | 1344137 | 1363878 | 19741 | 1 | 1 |
| `backdrop_batch_ab` | 43.700 | `yes` | 13041664 | 13041664 | 0 | 1364020 | 1389001 | 24981 | 1 | 1 |
| `scene3d_ab` | 38.200 | `yes` | 13041664 | 13041664 | 0 | 1389123 | 1414863 | 25740 | 1 | 1 |
| `mixed_matrix` | 38.100 | `yes` | 13041664 | 13041664 | 0 | 1414999 | 1425856 | 10857 | 1 | 1 |
| `layer_effects_matrix` | 38.300 | `yes` | 13041664 | 13041664 | 0 | 1425988 | 1436345 | 10357 | 1 | 1 |
| `command_family_matrix` | 21.200 | `yes` | 13041664 | 13041664 | 0 | 1445825 | 1455972 | 10147 | 1 | 1 |
| `glyph_run_ab` | 21.600 | `yes` | 13041664 | 13041664 | 0 | 1456130 | 1478945 | 22815 | 1 | 1 |
| `neon_marker_ab` | 29.900 | `yes` | 13041664 | 13041664 | 0 | 1466853 | 1480340 | 13487 | 1 | 1 |
| `direct_surface_ab` | 30.000 | `yes` | 13041664 | 13041664 | 0 | 1480468 | 1490253 | 9785 | 1 | 1 |
| `draw_state_cache_ab` | 29.700 | `yes` | 13041664 | 13041664 | 0 | 1490375 | 1500131 | 9756 | 1 | 1 |
| `clip_state_cache_ab` | 21.500 | `yes` | 13041664 | 13041664 | 0 | 1500249 | 1510083 | 9834 | 1 | 1 |

## Warm Resource Churn

| Check | Rows | Buffer Grows | Texture Creates | Bind Groups | Pipelines | Samplers | Meshes | Temp Allocs | Temp Bytes | Image Scratch Grows | CPU Scratch Grows | CPU Scratch Growth Bytes |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| `web.wasm.webgpu.warm_resource_churn.current_rows` | 17 checked / 16 excluded | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |

### Warm Resource Churn Rows

| Row | Buffer Grows | Texture Creates | Bind Groups | Pipelines | Samplers | Meshes | Temp Allocs | Temp Bytes | Image Scratch Grows | CPU Scratch Grows | CPU Scratch Growth Bytes |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| `web.wasm.webgpu.frame_loop` | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| `web.wasm.webgpu.id_mask_compositor.current` | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| `web.wasm.webgpu.glyph_atlas_upload.current_dirty` | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| `web.wasm.webgpu.image_upload.current_dirty` | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| `web.wasm.webgpu.upload_scratch.current_reuse` | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| `web.wasm.webgpu.effect_uniform.current_batched` | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| `web.wasm.webgpu.backdrop_batch.current_coalesced` | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| `web.wasm.webgpu.scene3d.reused_mesh` | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| `web.wasm.webgpu.scene3d.stress_reused_mesh` | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| `web.wasm.webgpu.mixed_text_image_effects` | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| `web.wasm.webgpu.layer_damage_effects` | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| `web.wasm.webgpu.command_family_matrix` | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| `web.wasm.webgpu.glyph_run.current` | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| `web.wasm.webgpu.neon_marker.current` | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| `web.wasm.webgpu.direct_surface.current` | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| `web.wasm.webgpu.draw_state_cache.current` | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| `web.wasm.webgpu.clip_state_cache.current` | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |

### Warm GPU Resource Family Churn

| Row | Draw Buffers | Image Textures | Image Bind Groups | Target Textures | Target Bind Groups | Scene3D Buffers | Scene3D Bind Groups | Effect Buffers | Effect Bind Groups | ID Mask Textures | ID Mask Buffers | ID Mask Bind Groups |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| `web.wasm.webgpu.warm_resource_churn.current_rows` | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| `web.wasm.webgpu.frame_loop` | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| `web.wasm.webgpu.id_mask_compositor.current` | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| `web.wasm.webgpu.glyph_atlas_upload.current_dirty` | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| `web.wasm.webgpu.image_upload.current_dirty` | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| `web.wasm.webgpu.upload_scratch.current_reuse` | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| `web.wasm.webgpu.effect_uniform.current_batched` | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| `web.wasm.webgpu.backdrop_batch.current_coalesced` | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| `web.wasm.webgpu.scene3d.reused_mesh` | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| `web.wasm.webgpu.scene3d.stress_reused_mesh` | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| `web.wasm.webgpu.mixed_text_image_effects` | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| `web.wasm.webgpu.layer_damage_effects` | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| `web.wasm.webgpu.command_family_matrix` | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| `web.wasm.webgpu.glyph_run.current` | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| `web.wasm.webgpu.neon_marker.current` | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| `web.wasm.webgpu.direct_surface.current` | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| `web.wasm.webgpu.draw_state_cache.current` | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| `web.wasm.webgpu.clip_state_cache.current` | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |

### Warm Scratch Family Churn

| Row | Draw Grows | Draw Bytes | Scene3D Grows | Scene3D Bytes | Effect Grows | Effect Bytes | ID Mask Grows | ID Mask Bytes | Image Upload Grows | Image Upload Bytes | Resource Table Grows | Resource Table Bytes |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| `web.wasm.webgpu.warm_resource_churn.current_rows` | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| `web.wasm.webgpu.frame_loop` | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| `web.wasm.webgpu.id_mask_compositor.current` | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| `web.wasm.webgpu.glyph_atlas_upload.current_dirty` | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| `web.wasm.webgpu.image_upload.current_dirty` | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| `web.wasm.webgpu.upload_scratch.current_reuse` | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| `web.wasm.webgpu.effect_uniform.current_batched` | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| `web.wasm.webgpu.backdrop_batch.current_coalesced` | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| `web.wasm.webgpu.scene3d.reused_mesh` | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| `web.wasm.webgpu.scene3d.stress_reused_mesh` | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| `web.wasm.webgpu.mixed_text_image_effects` | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| `web.wasm.webgpu.layer_damage_effects` | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| `web.wasm.webgpu.command_family_matrix` | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| `web.wasm.webgpu.glyph_run.current` | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| `web.wasm.webgpu.neon_marker.current` | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| `web.wasm.webgpu.direct_surface.current` | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| `web.wasm.webgpu.draw_state_cache.current` | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| `web.wasm.webgpu.clip_state_cache.current` | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |

## WASM Allocation Audit

| Check | Rows | Total Allocs | Total Bytes | Reallocs | Realloc Grow Bytes | Max Allocs/Frame | Max Bytes/Frame | Max Peak Frame Bytes | Budget Allocs/Frame | Budget Bytes/Frame |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| `web.wasm.webgpu.wasm_allocation_audit.current_rows` | 17 checked / 16 excluded | 4012 | 79084 | 0 | 0 | 6.556 | 129.222 | 174 | 7 | 144 |

### WASM Allocation Invariance

| Check | Status | Reference Row | Rows | Unique Signatures | Shared Allocs | Shared Bytes | Shared Reallocs | Shared Peak Frame Bytes |
| --- | --- | --- | ---: | ---: | ---: | ---: | ---: | ---: |
| `web.wasm.webgpu.wasm_allocation_invariance.current_rows` | `shared-submit-boundary-profile` | `web.wasm.webgpu.frame_loop` | 17 | 1 | 236 | 4652 | 0 | 174 |

### WASM Allocation Rows

| Row | Frames | Allocs | Bytes | Allocs/Frame | Bytes/Frame | Reallocs | Realloc Grow Bytes | Allocating Frames | Peak Frame Bytes |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| `web.wasm.webgpu.frame_loop` | 36 | 236 | 4652 | 6.556 | 129.222 | 0 | 0 | 36 | 174 |
| `web.wasm.webgpu.id_mask_compositor.current` | 36 | 236 | 4652 | 6.556 | 129.222 | 0 | 0 | 36 | 174 |
| `web.wasm.webgpu.glyph_atlas_upload.current_dirty` | 36 | 236 | 4652 | 6.556 | 129.222 | 0 | 0 | 36 | 174 |
| `web.wasm.webgpu.image_upload.current_dirty` | 36 | 236 | 4652 | 6.556 | 129.222 | 0 | 0 | 36 | 174 |
| `web.wasm.webgpu.upload_scratch.current_reuse` | 36 | 236 | 4652 | 6.556 | 129.222 | 0 | 0 | 36 | 174 |
| `web.wasm.webgpu.effect_uniform.current_batched` | 36 | 236 | 4652 | 6.556 | 129.222 | 0 | 0 | 36 | 174 |
| `web.wasm.webgpu.backdrop_batch.current_coalesced` | 36 | 236 | 4652 | 6.556 | 129.222 | 0 | 0 | 36 | 174 |
| `web.wasm.webgpu.scene3d.reused_mesh` | 36 | 236 | 4652 | 6.556 | 129.222 | 0 | 0 | 36 | 174 |
| `web.wasm.webgpu.scene3d.stress_reused_mesh` | 36 | 236 | 4652 | 6.556 | 129.222 | 0 | 0 | 36 | 174 |
| `web.wasm.webgpu.mixed_text_image_effects` | 36 | 236 | 4652 | 6.556 | 129.222 | 0 | 0 | 36 | 174 |
| `web.wasm.webgpu.layer_damage_effects` | 36 | 236 | 4652 | 6.556 | 129.222 | 0 | 0 | 36 | 174 |
| `web.wasm.webgpu.command_family_matrix` | 36 | 236 | 4652 | 6.556 | 129.222 | 0 | 0 | 36 | 174 |
| `web.wasm.webgpu.glyph_run.current` | 36 | 236 | 4652 | 6.556 | 129.222 | 0 | 0 | 36 | 174 |
| `web.wasm.webgpu.neon_marker.current` | 36 | 236 | 4652 | 6.556 | 129.222 | 0 | 0 | 36 | 174 |
| `web.wasm.webgpu.direct_surface.current` | 36 | 236 | 4652 | 6.556 | 129.222 | 0 | 0 | 36 | 174 |
| `web.wasm.webgpu.draw_state_cache.current` | 36 | 236 | 4652 | 6.556 | 129.222 | 0 | 0 | 36 | 174 |
| `web.wasm.webgpu.clip_state_cache.current` | 36 | 236 | 4652 | 6.556 | 129.222 | 0 | 0 | 36 | 174 |

## Frame Loop WASM Allocation Stages

| Stage | Allocs | Bytes | Reallocs | Realloc Grow Bytes | Peak Frame Bytes |
| --- | ---: | ---: | ---: | ---: | ---: |
| `canvas_resize` | 0 | 0 | 0 | 0 | 0 |
| `frame_timing` | 0 | 0 | 0 | 0 | 0 |
| `builder_clear` | 0 | 0 | 0 | 0 | 0 |
| `router_update` | 0 | 0 | 0 | 0 | 0 |
| `router_draw` | 0 | 0 | 0 | 0 | 0 |
| `damage_handoff` | 0 | 0 | 0 | 0 | 0 |
| `draw_coalesce` | 0 | 0 | 0 | 0 | 0 |
| `begin_frame` | 0 | 0 | 0 | 0 | 0 |
| `encode_pass` | 0 | 0 | 0 | 0 | 0 |
| `submit` | 236 | 4652 | 0 | 0 | 174 |
| `post_submit` | 0 | 0 | 0 | 0 | 0 |

## Backend Path Coverage

| Path | Status | Comparison | Rows | Counters |
| --- | --- | --- | ---: | ---: |
| `frame_loop` | `covered` | `coverage` | 1 | 6 |
| `id_mask_compositor` | `covered` | `current_vs_legacy` | 2 | 8 |
| `glyph_atlas_upload` | `covered` | `current_vs_legacy` | 2 | 4 |
| `image_upload` | `covered` | `current_vs_legacy` | 2 | 4 |
| `upload_scratch` | `covered` | `current_vs_legacy` | 2 | 5 |
| `effect_uniform` | `covered` | `current_vs_legacy` | 2 | 7 |
| `backdrop_batch` | `covered` | `current_vs_legacy` | 2 | 5 |
| `scene3d_mesh_reuse` | `covered` | `current_vs_legacy` | 2 | 5 |
| `scene3d_stress_mesh_reuse` | `covered` | `current_vs_legacy` | 2 | 5 |
| `mixed_text_image_effects` | `covered` | `current_vs_legacy` | 2 | 15 |
| `layer_damage_effects` | `covered` | `current_vs_legacy` | 2 | 16 |
| `command_family_matrix` | `covered` | `current_vs_legacy` | 2 | 10 |
| `glyph_run` | `covered` | `current_vs_legacy` | 2 | 12 |
| `neon_marker` | `covered` | `current_vs_legacy` | 2 | 8 |
| `direct_surface` | `covered` | `current_vs_legacy` | 2 | 10 |
| `draw_state_cache` | `covered` | `current_vs_legacy` | 2 | 5 |
| `clip_state_cache` | `covered` | `current_vs_legacy` | 2 | 5 |

## A/B Summary

| Comparison | Current p50 ms | Legacy p50 ms | Legacy / Current | Current Passes | Legacy Passes | Current Upload Bytes | Legacy Upload Bytes | Vertices | Vertex Bytes |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| `web.wasm.webgpu.id_mask_compositor.current_vs_legacy_upload` | 0.083 | 0.167 | 2.000 | 12 | 12 | 1120 | 308320 | 9600 | 307200 |

## Upload Summary

| Comparison | Glyph Current p50 ms | Glyph Legacy p50 ms | Glyph Legacy / Current | Glyph Current Texture Bytes | Glyph Legacy Texture Bytes | Glyph Current GPU ns | Glyph Legacy GPU ns | Glyph Legacy / Current GPU | Image Current p50 ms | Image Legacy p50 ms | Image Legacy / Current | Image Current Texture Bytes | Image Legacy Texture Bytes | Image Current GPU ns | Image Legacy GPU ns | Image Legacy / Current GPU |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| `web.wasm.webgpu.upload.current_dirty_vs_legacy_full` | 0.050 | 2.700 | 54.000 | 16384 | 4194304 | 23165 | 11832 | 0.511 | 0.025 | 0.100 | 4.000 | 16384 | 262144 | 11499 | 11833 | 1.029 |

## Upload Scratch Summary

| Comparison | Current p50 ms | Legacy p50 ms | Legacy / Current | Current Temp Allocs | Legacy Temp Allocs | Current Temp Bytes | Legacy Temp Bytes | Current Scratch Bytes | Legacy Scratch Bytes | Current Texture Bytes | Legacy Texture Bytes | Updates |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| `web.wasm.webgpu.upload_scratch.current_reuse_vs_legacy_temp_alloc` | 0.350 | 0.417 | 1.190 | 0 | 72 | 0 | 884736 | 4194304 | 4194304 | 786432 | 786432 | 24 |

## Effect Uniform Summary

| Comparison | Current p50 ms | Legacy p50 ms | Legacy / Current p50 | Current GPU ns | Legacy GPU ns | Legacy / Current GPU | Current Writes | Legacy Writes | Current Slots | Legacy Slots | Current Backdrops | Legacy Backdrops | Current Texture Copies | Legacy Texture Copies | Current Passes | Legacy Passes |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| `web.wasm.webgpu.effect_uniform.batched_vs_legacy_write_each` | 0.192 | 0.208 | 1.087 | 1382227 | 1276376 | 0.923 | 1 | 48 | 48 | 48 | 48 | 48 | 48 | 48 | 51 | 51 |

## Backdrop Batch Summary

| Comparison | Current p50 ms | Legacy p50 ms | Legacy / Current | Current Writes | Legacy Writes | Current Slots | Legacy Slots | Current Backdrops | Legacy Backdrops | Current Texture Copies | Legacy Texture Copies | Current Passes | Legacy Passes |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| `web.wasm.webgpu.backdrop_batch.coalesced_vs_per_backdrop_copy` | 0.025 | 0.058 | 2.333 | 1 | 1 | 12 | 12 | 12 | 12 | 1 | 12 | 4 | 15 |

## Scene3D Summary

| Comparison | Reused p50 ms | Recreate p50 ms | Recreate / Reused | Reused Mesh Creates | Recreate Mesh Creates | Reused Buffer Grows | Recreate Buffer Grows | Reused CPU Scratch Grows | Recreate CPU Scratch Grows | Reused CPU Scratch Growth Bytes | Recreate CPU Scratch Growth Bytes | Meshes | Instances |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| `web.wasm.webgpu.scene3d.reused_mesh_vs_recreate_mesh` | 0.017 | 0.025 | 1.500 | 0 | 2 | 0 | 4 | 0 | 0 | 0 | 0 | 2 | 2 |

## Scene3D Stress Summary

| Comparison | Reused p50 ms | Recreate p50 ms | Recreate / Reused | Reused Mesh Creates | Recreate Mesh Creates | Reused Buffer Grows | Recreate Buffer Grows | Reused CPU Scratch Grows | Recreate CPU Scratch Grows | Reused CPU Scratch Growth Bytes | Recreate CPU Scratch Growth Bytes | Meshes | Instances |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| `web.wasm.webgpu.scene3d.stress_reused_mesh_vs_stress_recreate_mesh` | 0.058 | 0.067 | 1.143 | 0 | 2 | 0 | 4 | 0 | 0 | 0 | 0 | 2 | 96 |

## Mixed Scene Summary

| Comparison | Current p50 ms | Legacy p50 ms | Legacy / Current | Current Items | Legacy Items | Current Pipeline Binds | Legacy Pipeline Binds | Current Bind Groups | Legacy Bind Groups | Current Scissors | Legacy Scissors | Current Writes | Legacy Writes | Current Texture Copies | Legacy Texture Copies | Current Passes | Legacy Passes |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| `web.wasm.webgpu.mixed_text_image_effects.current_vs_legacy_rebind_unbatched` | 0.050 | 0.075 | 1.500 | 114 | 114 | 8 | 114 | 7 | 102 | 5 | 114 | 1 | 2 | 2 | 2 | 6 | 6 |

## Layer Effects Summary

| Comparison | Current p50 ms | Legacy p50 ms | Legacy / Current | Current Items | Legacy Items | Current Pipeline Binds | Legacy Pipeline Binds | Current Bind Groups | Legacy Bind Groups | Current Scissors | Legacy Scissors | Current Writes | Legacy Writes | Current Texture Copies | Legacy Texture Copies | Current Passes | Legacy Passes |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| `web.wasm.webgpu.layer_damage_effects.current_vs_legacy_rebind_unbatched` | 0.050 | 0.075 | 1.500 | 86 | 86 | 6 | 86 | 12 | 76 | 4 | 86 | 1 | 5 | 1 | 5 | 5 | 9 |

## Command Family Summary

| Comparison | Current p50 ms | Legacy p50 ms | Legacy / Current | Current Items | Legacy Items | Current Pipeline Binds | Legacy Pipeline Binds | Current Bind Groups | Legacy Bind Groups | Current Scissors | Legacy Scissors | Image Meshes | Nine Slices | SDF Glyphs | CameraBg Draws |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| `web.wasm.webgpu.command_family_matrix.current_vs_legacy_rebind` | 0.100 | 0.158 | 1.583 | 649 | 649 | 3 | 649 | 2 | 648 | 1 | 649 | 64/64 | 64/64 | 288/288 | 0/0 |

## Glyph Run Summary

| Comparison | Current p50 ms | Legacy p50 ms | Legacy / Current | Runs | Glyphs/Run | Current Items | Legacy Items | Current Glyph Quads | Legacy Glyph Quads | Current SDF Glyphs | Legacy SDF Glyphs | Current Pipeline Binds | Legacy Pipeline Binds | Current Bind Groups | Legacy Bind Groups | Current Scissors | Legacy Scissors |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| `web.wasm.webgpu.glyph_run.current_vs_legacy_rebind` | 0.067 | 0.083 | 1.250 | 64 | 8 | 65 | 65 | 512 | 512 | 256 | 256 | 3 | 65 | 1 | 64 | 1 | 65 |

## Neon Marker Summary

| Comparison | Current p50 ms | Legacy p50 ms | Legacy / Current | Markers | Current Items | Legacy Items | Current Solid Tris | Legacy Solid Tris | Current Pipeline Binds | Legacy Pipeline Binds | Current Bind Groups | Legacy Bind Groups | Current Scissors | Legacy Scissors |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| `web.wasm.webgpu.neon_marker.current_vs_legacy_rebind` | 0.233 | 0.233 | 1.000 | 64 | 192 | 192 | 6912 | 6912 | 1 | 192 | 0 | 0 | 1 | 192 |

## Direct Surface Summary

| Comparison | Current p50 ms | Legacy p50 ms | Legacy / Current | Current Items | Legacy Items | Current Images | Legacy Images | Current Render Passes | Legacy Render Passes | Current Draw Passes | Legacy Draw Passes | Current Clear Passes | Legacy Clear Passes | Current Present Passes | Legacy Present Passes | Current GPU ns | Legacy GPU ns |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| `web.wasm.webgpu.direct_surface.current_vs_legacy_scene_present` | 0.050 | 0.058 | 1.167 | 385 | 385 | 384 | 384 | 1 | 3 | 1 | 1 | 0 | 1 | 0 | 1 | 21625 | 48582 |

## Draw State Cache Summary

| Comparison | Current p50 ms | Legacy p50 ms | Legacy / Current | Current Items | Legacy Items | Current Pipeline Binds | Legacy Pipeline Binds | Current Bind Groups | Legacy Bind Groups | Current Scissors | Legacy Scissors |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| `web.wasm.webgpu.draw_state_cache.current_vs_legacy_rebind` | 0.117 | 0.192 | 1.643 | 1024 | 1024 | 1 | 1024 | 1 | 1024 | 1 | 1024 |

## Clip State Cache Summary

| Comparison | Current p50 ms | Legacy p50 ms | Legacy / Current | Current Items | Legacy Items | Current Clip Depth | Legacy Clip Depth | Current Pipeline Binds | Legacy Pipeline Binds | Current Bind Groups | Legacy Bind Groups | Current Scissors | Legacy Scissors |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| `web.wasm.webgpu.clip_state_cache.current_vs_legacy_rebind` | 0.067 | 0.108 | 1.625 | 512 | 512 | 2 | 2 | 1 | 512 | 1 | 512 | 16 | 512 |

## Pixel Check

| Target | Viewport | Pixdiff | Max Err | MSE | Artifact |
| --- | --- | ---: | ---: | ---: | --- |
| `app` | 320x240 | 0 | 0 | 0.000 | `/tmp/oxide-webgpu-browser-glyph-run-matrix.png` |

## Notes

- BrowserRenderer selected the WebGPU backend through async renderer initialization.
- This baseline was collected from a release wasm build served through the static web host.
- Production web visual startup is WebGPU-only; unsupported browsers return NOT SUPPORTED instead of drawing through Canvas2D.
- The WebGPU ID-mask current and legacy upload rows are captured in the same browser process and scene contract.
- The WebGPU effect-uniform A/B rows draw the same backdrop scene while comparing one batched dynamic-uniform upload against one queue write per backdrop.
- The WebGPU backdrop-batch A/B rows draw separated consecutive backdrops while comparing one shared scene-copy pass against the legacy per-backdrop copy path.
- The WebGPU layer/damage/effects A/B rows draw the same nested layer, damage, image, glyph, backdrop, visual-effect, and spinner workload while comparing current state/effect batching against legacy rebinding/unbatched toggles.
- The WebGPU command-family A/B rows draw the same generic ImageMesh, NineSlice, and SDF glyph workload while comparing current draw-state caching against a legacy rebind path and keeping web CameraBg work unavailable.
- The WebGPU glyph-run A/B rows draw the same atlas-backed A8 and SDF GlyphRun workload while comparing current draw-state caching against a legacy rebind path.
- The WebGPU direct-surface A/B rows draw the same no-effect image workload while comparing direct surface rendering against a benchmark-only forced scene-present path.
- The WebGPU clip-state A/B rows use real Oxide ClipPush/ClipPop commands to measure scissor-state caching.
- Pass-family counters provide browser GPU-stage attribution when direct timestamp queries are unavailable.
- Warm current-path WebGPU rows are gated against post-warmup resource creation, buffer growth, mesh creation, image-upload temp allocation, and CPU/image scratch growth.
- WASM allocation counters measure Rust allocator activity inside each post-warmup benchmark frame loop and are reported separately from renderer-owned resource churn.
- Chrome startup tracing is captured from a duplicate benchmark-report run so GPU/browser-process activity is tied to the same report workload without perturbing persisted timing rows.
- Browser User Timing marks surround every benchmark family and are persisted to prove the traced report run exercised the expected workload phases.
