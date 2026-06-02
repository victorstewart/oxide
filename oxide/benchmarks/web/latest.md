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
| `web.wasm.webgpu.frame_loop` | `webgpu` | 3 | 12 | 36 | 0.100 | 0.300 | 0.400 | 0.400 | 0.128 | ms/frame | `draws=26;draw_items=26;draw_pipeline_binds=8;draw_bind_group_binds=5;draw_scissor_sets=3;solid_tris=792;image_draws=0;image_mesh_draws=0;nine_slice_draws=0;glyph_quads=60;sdf_glyph_quads=0;clip_depth_peak=1;damage_rects=3;layer_draws=0;scene3d_draws=0;id_mask_draws=0;backdrop_draws=1;visual_effect_draws=0;effect_uniform_writes=1;effect_uniform_bytes=16;effect_uniform_slots=1;spinner_draws=1;camera_bg_draws=0;render_passes=5;passes=clear:1/draw:3/scene3d:0/scene3d_overlay:0/idmask:0+0+0+0/present:1;texture_copies=1;command_buffers=1;gpu_ts_passes=5;gpu_ts_total_ns=110251;gpu_ts_max_ns=34709;buffer_upload_bytes=48696;texture_upload_bytes=0;buffer_grows=0;texture_creates=0;bind_group_creates=0;pipeline_creates=0;sampler_creates=0;mesh3d_creates=0;wasm_alloc_count=236;wasm_alloc_bytes=4652;wasm_realloc_count=0;wasm_realloc_grow_bytes=0;wasm_allocating_frames=36;image_upload_temp_allocs=0;image_upload_temp_bytes=0;image_upload_scratch_bytes=17640;image_upload_scratch_grows=0;cpu_scratch_bytes=162328;cpu_scratch_grows=0;cpu_scratch_growth_bytes=0;missed120=0.000;hitch120=0.000;missed60=0.000;hitch60=0.000` |
| `web.wasm.webgpu.id_mask_compositor.current` | `webgpu-current` | 3 | 12 | 36 | 0.092 | 0.092 | 0.092 | 0.092 | 0.089 | ms/frame | `draws=12;draw_items=0;draw_pipeline_binds=0;draw_bind_group_binds=0;draw_scissor_sets=0;solid_tris=0;image_draws=0;image_mesh_draws=0;nine_slice_draws=0;glyph_quads=0;sdf_glyph_quads=0;clip_depth_peak=0;damage_rects=0;layer_draws=0;scene3d_draws=0;id_mask_draws=1;backdrop_draws=0;visual_effect_draws=0;effect_uniform_writes=0;effect_uniform_bytes=0;effect_uniform_slots=0;spinner_draws=0;camera_bg_draws=0;render_passes=12;passes=clear:0/draw:0/scene3d:0/scene3d_overlay:0/idmask:1+1+9+1/present:0;texture_copies=0;command_buffers=1;gpu_ts_passes=12;gpu_ts_total_ns=1716624;gpu_ts_max_ns=222084;buffer_upload_bytes=1120;texture_upload_bytes=0;buffer_grows=0;texture_creates=0;bind_group_creates=0;pipeline_creates=0;sampler_creates=0;mesh3d_creates=0;wasm_alloc_count=236;wasm_alloc_bytes=4652;wasm_realloc_count=0;wasm_realloc_grow_bytes=0;wasm_allocating_frames=36;image_upload_temp_allocs=0;image_upload_temp_bytes=0;image_upload_scratch_bytes=17640;image_upload_scratch_grows=0;cpu_scratch_bytes=473640;cpu_scratch_grows=0;cpu_scratch_growth_bytes=0;vertices=9600;vertex_bytes=307200;missed120=0.000;hitch120=0.000;missed60=0.000;hitch60=0.000` |
| `web.wasm.webgpu.id_mask_compositor.legacy_upload` | `webgpu-legacy-upload` | 3 | 12 | 36 | 0.175 | 0.208 | 0.208 | 0.208 | 0.183 | ms/frame | `draws=12;draw_items=0;draw_pipeline_binds=0;draw_bind_group_binds=0;draw_scissor_sets=0;solid_tris=0;image_draws=0;image_mesh_draws=0;nine_slice_draws=0;glyph_quads=0;sdf_glyph_quads=0;clip_depth_peak=0;damage_rects=0;layer_draws=0;scene3d_draws=0;id_mask_draws=1;backdrop_draws=0;visual_effect_draws=0;effect_uniform_writes=0;effect_uniform_bytes=0;effect_uniform_slots=0;spinner_draws=0;camera_bg_draws=0;render_passes=12;passes=clear:0/draw:0/scene3d:0/scene3d_overlay:0/idmask:1+1+9+1/present:0;texture_copies=0;command_buffers=1;gpu_ts_passes=12;gpu_ts_total_ns=1723027;gpu_ts_max_ns=222559;buffer_upload_bytes=308320;texture_upload_bytes=0;buffer_grows=0;texture_creates=0;bind_group_creates=0;pipeline_creates=0;sampler_creates=0;mesh3d_creates=0;wasm_alloc_count=236;wasm_alloc_bytes=4652;wasm_realloc_count=0;wasm_realloc_grow_bytes=0;wasm_allocating_frames=36;image_upload_temp_allocs=0;image_upload_temp_bytes=0;image_upload_scratch_bytes=17640;image_upload_scratch_grows=0;cpu_scratch_bytes=473640;cpu_scratch_grows=0;cpu_scratch_growth_bytes=0;vertices=9600;vertex_bytes=307200;missed120=0.000;hitch120=0.000;missed60=0.000;hitch60=0.000` |
| `web.wasm.webgpu.glyph_atlas_upload.current_dirty` | `webgpu-dirty-atlas-update` | 3 | 12 | 36 | 0.050 | 0.050 | 0.050 | 0.050 | 0.050 | ms/frame | `draws=2;draw_items=2;draw_pipeline_binds=2;draw_bind_group_binds=1;draw_scissor_sets=1;solid_tris=36;image_draws=0;image_mesh_draws=0;nine_slice_draws=0;glyph_quads=96;sdf_glyph_quads=0;clip_depth_peak=0;damage_rects=0;layer_draws=0;scene3d_draws=0;id_mask_draws=0;backdrop_draws=0;visual_effect_draws=0;effect_uniform_writes=0;effect_uniform_bytes=0;effect_uniform_slots=0;spinner_draws=0;camera_bg_draws=0;render_passes=1;passes=clear:0/draw:1/scene3d:0/scene3d_overlay:0/idmask:0+0+0+0/present:0;texture_copies=0;command_buffers=1;gpu_ts_passes=1;gpu_ts_total_ns=11082;gpu_ts_max_ns=11082;buffer_upload_bytes=22368;texture_upload_bytes=16384;buffer_grows=0;texture_creates=0;bind_group_creates=0;pipeline_creates=0;sampler_creates=0;mesh3d_creates=0;wasm_alloc_count=236;wasm_alloc_bytes=4652;wasm_realloc_count=0;wasm_realloc_grow_bytes=0;wasm_allocating_frames=36;image_upload_temp_allocs=0;image_upload_temp_bytes=0;image_upload_scratch_bytes=17640;image_upload_scratch_grows=0;cpu_scratch_bytes=484840;cpu_scratch_grows=0;cpu_scratch_growth_bytes=0;atlas=1024x1024;dirty=64x64;missed120=0.000;hitch120=0.000;missed60=0.000;hitch60=0.000` |
| `web.wasm.webgpu.glyph_atlas_upload.legacy_full` | `webgpu-full-atlas-update` | 3 | 12 | 36 | 2.958 | 3.000 | 3.000 | 3.000 | 2.925 | ms/frame | `draws=2;draw_items=2;draw_pipeline_binds=2;draw_bind_group_binds=1;draw_scissor_sets=1;solid_tris=36;image_draws=0;image_mesh_draws=0;nine_slice_draws=0;glyph_quads=96;sdf_glyph_quads=0;clip_depth_peak=0;damage_rects=0;layer_draws=0;scene3d_draws=0;id_mask_draws=0;backdrop_draws=0;visual_effect_draws=0;effect_uniform_writes=0;effect_uniform_bytes=0;effect_uniform_slots=0;spinner_draws=0;camera_bg_draws=0;render_passes=1;passes=clear:0/draw:1/scene3d:0/scene3d_overlay:0/idmask:0+0+0+0/present:0;texture_copies=0;command_buffers=1;gpu_ts_passes=1;gpu_ts_total_ns=14165;gpu_ts_max_ns=14165;buffer_upload_bytes=22368;texture_upload_bytes=4194304;buffer_grows=0;texture_creates=0;bind_group_creates=0;pipeline_creates=0;sampler_creates=0;mesh3d_creates=0;wasm_alloc_count=236;wasm_alloc_bytes=4652;wasm_realloc_count=0;wasm_realloc_grow_bytes=0;wasm_allocating_frames=36;image_upload_temp_allocs=0;image_upload_temp_bytes=0;image_upload_scratch_bytes=4194304;image_upload_scratch_grows=0;cpu_scratch_bytes=4661504;cpu_scratch_grows=0;cpu_scratch_growth_bytes=0;atlas=1024x1024;dirty=1024x1024;missed120=0.000;hitch120=0.000;missed60=0.000;hitch60=0.000` |
| `web.wasm.webgpu.image_upload.current_dirty` | `webgpu-dirty-rgba-update` | 3 | 12 | 36 | 0.017 | 0.033 | 0.033 | 0.033 | 0.022 | ms/frame | `draws=2;draw_items=2;draw_pipeline_binds=2;draw_bind_group_binds=1;draw_scissor_sets=1;solid_tris=36;image_draws=1;image_mesh_draws=0;nine_slice_draws=0;glyph_quads=0;sdf_glyph_quads=0;clip_depth_peak=0;damage_rects=0;layer_draws=0;scene3d_draws=0;id_mask_draws=0;backdrop_draws=0;visual_effect_draws=0;effect_uniform_writes=0;effect_uniform_bytes=0;effect_uniform_slots=0;spinner_draws=0;camera_bg_draws=0;render_passes=1;passes=clear:0/draw:1/scene3d:0/scene3d_overlay:0/idmask:0+0+0+0/present:0;texture_copies=0;command_buffers=1;gpu_ts_passes=1;gpu_ts_total_ns=11749;gpu_ts_max_ns=11749;buffer_upload_bytes=1784;texture_upload_bytes=16384;buffer_grows=0;texture_creates=0;bind_group_creates=0;pipeline_creates=0;sampler_creates=0;mesh3d_creates=0;wasm_alloc_count=236;wasm_alloc_bytes=4652;wasm_realloc_count=0;wasm_realloc_grow_bytes=0;wasm_allocating_frames=36;image_upload_temp_allocs=0;image_upload_temp_bytes=0;image_upload_scratch_bytes=4194304;image_upload_scratch_grows=0;cpu_scratch_bytes=4661504;cpu_scratch_grows=0;cpu_scratch_growth_bytes=0;image=256x256;dirty=64x64;missed120=0.000;hitch120=0.000;missed60=0.000;hitch60=0.000` |
| `web.wasm.webgpu.image_upload.legacy_full` | `webgpu-full-rgba-update` | 3 | 12 | 36 | 0.108 | 0.117 | 0.117 | 0.117 | 0.106 | ms/frame | `draws=2;draw_items=2;draw_pipeline_binds=2;draw_bind_group_binds=1;draw_scissor_sets=1;solid_tris=36;image_draws=1;image_mesh_draws=0;nine_slice_draws=0;glyph_quads=0;sdf_glyph_quads=0;clip_depth_peak=0;damage_rects=0;layer_draws=0;scene3d_draws=0;id_mask_draws=0;backdrop_draws=0;visual_effect_draws=0;effect_uniform_writes=0;effect_uniform_bytes=0;effect_uniform_slots=0;spinner_draws=0;camera_bg_draws=0;render_passes=1;passes=clear:0/draw:1/scene3d:0/scene3d_overlay:0/idmask:0+0+0+0/present:0;texture_copies=0;command_buffers=1;gpu_ts_passes=1;gpu_ts_total_ns=16915;gpu_ts_max_ns=16915;buffer_upload_bytes=1784;texture_upload_bytes=262144;buffer_grows=0;texture_creates=0;bind_group_creates=0;pipeline_creates=0;sampler_creates=0;mesh3d_creates=0;wasm_alloc_count=236;wasm_alloc_bytes=4652;wasm_realloc_count=0;wasm_realloc_grow_bytes=0;wasm_allocating_frames=36;image_upload_temp_allocs=0;image_upload_temp_bytes=0;image_upload_scratch_bytes=4194304;image_upload_scratch_grows=0;cpu_scratch_bytes=4661504;cpu_scratch_grows=0;cpu_scratch_growth_bytes=0;image=256x256;dirty=256x256;missed120=0.000;hitch120=0.000;missed60=0.000;hitch60=0.000` |
| `web.wasm.webgpu.upload_scratch.current_reuse` | `webgpu-upload-scratch-current-reuse` | 3 | 12 | 36 | 0.358 | 0.375 | 0.375 | 0.375 | 0.361 | ms/frame | `draws=3;draw_items=3;draw_pipeline_binds=3;draw_bind_group_binds=2;draw_scissor_sets=1;solid_tris=36;image_draws=1;image_mesh_draws=0;nine_slice_draws=0;glyph_quads=36;sdf_glyph_quads=0;clip_depth_peak=0;damage_rects=0;layer_draws=0;scene3d_draws=0;id_mask_draws=0;backdrop_draws=0;visual_effect_draws=0;effect_uniform_writes=0;effect_uniform_bytes=0;effect_uniform_slots=0;spinner_draws=0;camera_bg_draws=0;render_passes=1;passes=clear:0/draw:1/scene3d:0/scene3d_overlay:0/idmask:0+0+0+0/present:0;texture_copies=0;command_buffers=1;gpu_ts_passes=1;gpu_ts_total_ns=10291;gpu_ts_max_ns=10291;buffer_upload_bytes=9560;texture_upload_bytes=786432;buffer_grows=0;texture_creates=0;bind_group_creates=0;pipeline_creates=0;sampler_creates=0;mesh3d_creates=0;wasm_alloc_count=236;wasm_alloc_bytes=4652;wasm_realloc_count=0;wasm_realloc_grow_bytes=0;wasm_allocating_frames=36;image_upload_temp_allocs=0;image_upload_temp_bytes=0;image_upload_scratch_bytes=4194304;image_upload_scratch_grows=0;cpu_scratch_bytes=4661504;cpu_scratch_grows=0;cpu_scratch_growth_bytes=0;updates=24;missed120=0.000;hitch120=0.000;missed60=0.000;hitch60=0.000` |
| `web.wasm.webgpu.upload_scratch.legacy_temp_alloc` | `webgpu-upload-scratch-legacy-temp-alloc` | 3 | 12 | 36 | 0.425 | 0.500 | 0.500 | 0.500 | 0.442 | ms/frame | `draws=3;draw_items=3;draw_pipeline_binds=3;draw_bind_group_binds=2;draw_scissor_sets=1;solid_tris=36;image_draws=1;image_mesh_draws=0;nine_slice_draws=0;glyph_quads=36;sdf_glyph_quads=0;clip_depth_peak=0;damage_rects=0;layer_draws=0;scene3d_draws=0;id_mask_draws=0;backdrop_draws=0;visual_effect_draws=0;effect_uniform_writes=0;effect_uniform_bytes=0;effect_uniform_slots=0;spinner_draws=0;camera_bg_draws=0;render_passes=1;passes=clear:0/draw:1/scene3d:0/scene3d_overlay:0/idmask:0+0+0+0/present:0;texture_copies=0;command_buffers=1;gpu_ts_passes=1;gpu_ts_total_ns=10542;gpu_ts_max_ns=10542;buffer_upload_bytes=9560;texture_upload_bytes=786432;buffer_grows=0;texture_creates=0;bind_group_creates=0;pipeline_creates=0;sampler_creates=0;mesh3d_creates=0;wasm_alloc_count=2828;wasm_alloc_bytes=31855148;wasm_realloc_count=0;wasm_realloc_grow_bytes=0;wasm_allocating_frames=36;image_upload_temp_allocs=72;image_upload_temp_bytes=884736;image_upload_scratch_bytes=4194304;image_upload_scratch_grows=0;cpu_scratch_bytes=4661504;cpu_scratch_grows=0;cpu_scratch_growth_bytes=0;updates=24;missed120=0.000;hitch120=0.000;missed60=0.000;hitch60=0.000` |
| `web.wasm.webgpu.effect_uniform.current_batched` | `webgpu-effect-uniform-current-batched` | 3 | 12 | 36 | 0.208 | 0.242 | 0.242 | 0.242 | 0.214 | ms/frame | `draws=50;draw_items=50;draw_pipeline_binds=50;draw_bind_group_binds=97;draw_scissor_sets=49;solid_tris=4;image_draws=1;image_mesh_draws=0;nine_slice_draws=0;glyph_quads=0;sdf_glyph_quads=0;clip_depth_peak=0;damage_rects=0;layer_draws=0;scene3d_draws=0;id_mask_draws=0;backdrop_draws=48;visual_effect_draws=0;effect_uniform_writes=1;effect_uniform_bytes=16;effect_uniform_slots=48;spinner_draws=0;camera_bg_draws=0;render_passes=51;passes=clear:1/draw:49/scene3d:0/scene3d_overlay:0/idmask:0+0+0+0/present:1;texture_copies=48;command_buffers=1;gpu_ts_passes=51;gpu_ts_total_ns=1431265;gpu_ts_max_ns=96745;buffer_upload_bytes=7688;texture_upload_bytes=0;buffer_grows=0;texture_creates=0;bind_group_creates=0;pipeline_creates=0;sampler_creates=0;mesh3d_creates=0;wasm_alloc_count=236;wasm_alloc_bytes=4652;wasm_realloc_count=0;wasm_realloc_grow_bytes=0;wasm_allocating_frames=36;image_upload_temp_allocs=0;image_upload_temp_bytes=0;image_upload_scratch_bytes=4194304;image_upload_scratch_grows=0;cpu_scratch_bytes=4663168;cpu_scratch_grows=0;cpu_scratch_growth_bytes=0;expected_backdrops=48;missed120=0.000;hitch120=0.000;missed60=0.000;hitch60=0.000` |
| `web.wasm.webgpu.effect_uniform.legacy_write_each` | `webgpu-effect-uniform-legacy-write-each` | 3 | 12 | 36 | 0.225 | 0.225 | 0.225 | 0.225 | 0.217 | ms/frame | `draws=50;draw_items=50;draw_pipeline_binds=50;draw_bind_group_binds=97;draw_scissor_sets=49;solid_tris=4;image_draws=1;image_mesh_draws=0;nine_slice_draws=0;glyph_quads=0;sdf_glyph_quads=0;clip_depth_peak=0;damage_rects=0;layer_draws=0;scene3d_draws=0;id_mask_draws=0;backdrop_draws=48;visual_effect_draws=0;effect_uniform_writes=48;effect_uniform_bytes=768;effect_uniform_slots=48;spinner_draws=0;camera_bg_draws=0;render_passes=51;passes=clear:1/draw:49/scene3d:0/scene3d_overlay:0/idmask:0+0+0+0/present:1;texture_copies=48;command_buffers=1;gpu_ts_passes=51;gpu_ts_total_ns=1378129;gpu_ts_max_ns=110580;buffer_upload_bytes=8440;texture_upload_bytes=0;buffer_grows=0;texture_creates=0;bind_group_creates=0;pipeline_creates=0;sampler_creates=0;mesh3d_creates=0;wasm_alloc_count=236;wasm_alloc_bytes=4652;wasm_realloc_count=0;wasm_realloc_grow_bytes=0;wasm_allocating_frames=36;image_upload_temp_allocs=0;image_upload_temp_bytes=0;image_upload_scratch_bytes=4194304;image_upload_scratch_grows=0;cpu_scratch_bytes=4663168;cpu_scratch_grows=0;cpu_scratch_growth_bytes=0;expected_backdrops=48;missed120=0.000;hitch120=0.000;missed60=0.000;hitch60=0.000` |
| `web.wasm.webgpu.backdrop_batch.current_coalesced` | `webgpu-backdrop-batch-current-coalesced` | 3 | 12 | 36 | 0.025 | 0.025 | 0.025 | 0.025 | 0.022 | ms/frame | `draws=14;draw_items=14;draw_pipeline_binds=3;draw_bind_group_binds=3;draw_scissor_sets=2;solid_tris=4;image_draws=1;image_mesh_draws=0;nine_slice_draws=0;glyph_quads=0;sdf_glyph_quads=0;clip_depth_peak=0;damage_rects=0;layer_draws=0;scene3d_draws=0;id_mask_draws=0;backdrop_draws=12;visual_effect_draws=0;effect_uniform_writes=1;effect_uniform_bytes=16;effect_uniform_slots=12;spinner_draws=0;camera_bg_draws=0;render_passes=4;passes=clear:1/draw:2/scene3d:0/scene3d_overlay:0/idmask:0+0+0+0/present:1;texture_copies=1;command_buffers=1;gpu_ts_passes=4;gpu_ts_total_ns=79748;gpu_ts_max_ns=36749;buffer_upload_bytes=2216;texture_upload_bytes=0;buffer_grows=0;texture_creates=0;bind_group_creates=0;pipeline_creates=0;sampler_creates=0;mesh3d_creates=0;wasm_alloc_count=236;wasm_alloc_bytes=4652;wasm_realloc_count=0;wasm_realloc_grow_bytes=0;wasm_allocating_frames=36;image_upload_temp_allocs=0;image_upload_temp_bytes=0;image_upload_scratch_bytes=4194304;image_upload_scratch_grows=0;cpu_scratch_bytes=4663168;cpu_scratch_grows=0;cpu_scratch_growth_bytes=0;expected_backdrops=12;missed120=0.000;hitch120=0.000;missed60=0.000;hitch60=0.000` |
| `web.wasm.webgpu.backdrop_batch.legacy_per_backdrop_copy` | `webgpu-backdrop-batch-legacy-per-backdrop-copy` | 3 | 12 | 36 | 0.067 | 0.067 | 0.067 | 0.067 | 0.061 | ms/frame | `draws=14;draw_items=14;draw_pipeline_binds=14;draw_bind_group_binds=25;draw_scissor_sets=13;solid_tris=4;image_draws=1;image_mesh_draws=0;nine_slice_draws=0;glyph_quads=0;sdf_glyph_quads=0;clip_depth_peak=0;damage_rects=0;layer_draws=0;scene3d_draws=0;id_mask_draws=0;backdrop_draws=12;visual_effect_draws=0;effect_uniform_writes=1;effect_uniform_bytes=16;effect_uniform_slots=12;spinner_draws=0;camera_bg_draws=0;render_passes=15;passes=clear:1/draw:13/scene3d:0/scene3d_overlay:0/idmask:0+0+0+0/present:1;texture_copies=12;command_buffers=1;gpu_ts_passes=15;gpu_ts_total_ns=366073;gpu_ts_max_ns=66248;buffer_upload_bytes=2216;texture_upload_bytes=0;buffer_grows=0;texture_creates=0;bind_group_creates=0;pipeline_creates=0;sampler_creates=0;mesh3d_creates=0;wasm_alloc_count=236;wasm_alloc_bytes=4652;wasm_realloc_count=0;wasm_realloc_grow_bytes=0;wasm_allocating_frames=36;image_upload_temp_allocs=0;image_upload_temp_bytes=0;image_upload_scratch_bytes=4194304;image_upload_scratch_grows=0;cpu_scratch_bytes=4663168;cpu_scratch_grows=0;cpu_scratch_growth_bytes=0;expected_backdrops=12;missed120=0.000;hitch120=0.000;missed60=0.000;hitch60=0.000` |
| `web.wasm.webgpu.scene3d.reused_mesh` | `webgpu-scene3d-reused-mesh` | 3 | 12 | 36 | 0.025 | 0.025 | 0.025 | 0.025 | 0.019 | ms/frame | `draws=2;draw_items=0;draw_pipeline_binds=0;draw_bind_group_binds=0;draw_scissor_sets=0;solid_tris=0;image_draws=0;image_mesh_draws=0;nine_slice_draws=0;glyph_quads=0;sdf_glyph_quads=0;clip_depth_peak=0;damage_rects=0;layer_draws=0;scene3d_draws=2;id_mask_draws=0;backdrop_draws=0;visual_effect_draws=0;effect_uniform_writes=0;effect_uniform_bytes=0;effect_uniform_slots=0;spinner_draws=0;camera_bg_draws=0;render_passes=1;passes=clear:0/draw:0/scene3d:1/scene3d_overlay:0/idmask:0+0+0+0/present:0;texture_copies=0;command_buffers=1;gpu_ts_passes=1;gpu_ts_total_ns=12750;gpu_ts_max_ns=12750;buffer_upload_bytes=528;texture_upload_bytes=0;buffer_grows=0;texture_creates=0;bind_group_creates=0;pipeline_creates=0;sampler_creates=0;mesh3d_creates=0;wasm_alloc_count=236;wasm_alloc_bytes=4652;wasm_realloc_count=0;wasm_realloc_grow_bytes=0;wasm_allocating_frames=36;image_upload_temp_allocs=0;image_upload_temp_bytes=0;image_upload_scratch_bytes=4194304;image_upload_scratch_grows=0;cpu_scratch_bytes=4664120;cpu_scratch_grows=0;cpu_scratch_growth_bytes=0;meshes=2;instances=2;missed120=0.000;hitch120=0.000;missed60=0.000;hitch60=0.000` |
| `web.wasm.webgpu.scene3d.recreate_mesh` | `webgpu-scene3d-recreate-mesh` | 3 | 12 | 36 | 0.017 | 0.025 | 0.025 | 0.025 | 0.019 | ms/frame | `draws=2;draw_items=0;draw_pipeline_binds=0;draw_bind_group_binds=0;draw_scissor_sets=0;solid_tris=0;image_draws=0;image_mesh_draws=0;nine_slice_draws=0;glyph_quads=0;sdf_glyph_quads=0;clip_depth_peak=0;damage_rects=0;layer_draws=0;scene3d_draws=2;id_mask_draws=0;backdrop_draws=0;visual_effect_draws=0;effect_uniform_writes=0;effect_uniform_bytes=0;effect_uniform_slots=0;spinner_draws=0;camera_bg_draws=0;render_passes=1;passes=clear:0/draw:0/scene3d:1/scene3d_overlay:0/idmask:0+0+0+0/present:0;texture_copies=0;command_buffers=1;gpu_ts_passes=1;gpu_ts_total_ns=19292;gpu_ts_max_ns=19292;buffer_upload_bytes=720;texture_upload_bytes=0;buffer_grows=4;texture_creates=0;bind_group_creates=0;pipeline_creates=0;sampler_creates=0;mesh3d_creates=2;wasm_alloc_count=668;wasm_alloc_bytes=26540;wasm_realloc_count=3;wasm_realloc_grow_bytes=6272;wasm_allocating_frames=36;image_upload_temp_allocs=0;image_upload_temp_bytes=0;image_upload_scratch_bytes=4194304;image_upload_scratch_grows=0;cpu_scratch_bytes=4670840;cpu_scratch_grows=0;cpu_scratch_growth_bytes=0;meshes=2;instances=2;missed120=0.000;hitch120=0.000;missed60=0.000;hitch60=0.000` |
| `web.wasm.webgpu.scene3d.stress_reused_mesh` | `webgpu-scene3d-stress-reused-mesh` | 3 | 12 | 36 | 0.067 | 0.075 | 0.075 | 0.075 | 0.067 | ms/frame | `draws=96;draw_items=0;draw_pipeline_binds=0;draw_bind_group_binds=0;draw_scissor_sets=0;solid_tris=0;image_draws=0;image_mesh_draws=0;nine_slice_draws=0;glyph_quads=0;sdf_glyph_quads=0;clip_depth_peak=0;damage_rects=0;layer_draws=0;scene3d_draws=96;id_mask_draws=0;backdrop_draws=0;visual_effect_draws=0;effect_uniform_writes=0;effect_uniform_bytes=0;effect_uniform_slots=0;spinner_draws=0;camera_bg_draws=0;render_passes=1;passes=clear:0/draw:0/scene3d:1/scene3d_overlay:0/idmask:0+0+0+0/present:0;texture_copies=0;command_buffers=1;gpu_ts_passes=1;gpu_ts_total_ns=11541;gpu_ts_max_ns=11541;buffer_upload_bytes=24592;texture_upload_bytes=0;buffer_grows=0;texture_creates=0;bind_group_creates=0;pipeline_creates=0;sampler_creates=0;mesh3d_creates=0;wasm_alloc_count=236;wasm_alloc_bytes=4652;wasm_realloc_count=0;wasm_realloc_grow_bytes=0;wasm_allocating_frames=36;image_upload_temp_allocs=0;image_upload_temp_bytes=0;image_upload_scratch_bytes=4194304;image_upload_scratch_grows=0;cpu_scratch_bytes=4704584;cpu_scratch_grows=0;cpu_scratch_growth_bytes=0;meshes=2;instances=96;missed120=0.000;hitch120=0.000;missed60=0.000;hitch60=0.000` |
| `web.wasm.webgpu.scene3d.stress_recreate_mesh` | `webgpu-scene3d-stress-recreate-mesh` | 3 | 12 | 36 | 0.075 | 0.083 | 0.083 | 0.083 | 0.075 | ms/frame | `draws=96;draw_items=0;draw_pipeline_binds=0;draw_bind_group_binds=0;draw_scissor_sets=0;solid_tris=0;image_draws=0;image_mesh_draws=0;nine_slice_draws=0;glyph_quads=0;sdf_glyph_quads=0;clip_depth_peak=0;damage_rects=0;layer_draws=0;scene3d_draws=96;id_mask_draws=0;backdrop_draws=0;visual_effect_draws=0;effect_uniform_writes=0;effect_uniform_bytes=0;effect_uniform_slots=0;spinner_draws=0;camera_bg_draws=0;render_passes=1;passes=clear:0/draw:0/scene3d:1/scene3d_overlay:0/idmask:0+0+0+0/present:0;texture_copies=0;command_buffers=1;gpu_ts_passes=1;gpu_ts_total_ns=11875;gpu_ts_max_ns=11875;buffer_upload_bytes=24784;texture_upload_bytes=0;buffer_grows=4;texture_creates=0;bind_group_creates=0;pipeline_creates=0;sampler_creates=0;mesh3d_creates=2;wasm_alloc_count=668;wasm_alloc_bytes=26540;wasm_realloc_count=1;wasm_realloc_grow_bytes=7168;wasm_allocating_frames=36;image_upload_temp_allocs=0;image_upload_temp_bytes=0;image_upload_scratch_bytes=4194304;image_upload_scratch_grows=0;cpu_scratch_bytes=4711752;cpu_scratch_grows=0;cpu_scratch_growth_bytes=0;meshes=2;instances=96;missed120=0.000;hitch120=0.000;missed60=0.000;hitch60=0.000` |
| `web.wasm.webgpu.mixed_text_image_effects` | `webgpu-mixed-effects-current` | 3 | 12 | 36 | 0.067 | 0.067 | 0.067 | 0.067 | 0.064 | ms/frame | `draws=114;draw_items=114;draw_pipeline_binds=8;draw_bind_group_binds=7;draw_scissor_sets=5;solid_tris=472;image_draws=97;image_mesh_draws=0;nine_slice_draws=0;glyph_quads=96;sdf_glyph_quads=0;clip_depth_peak=1;damage_rects=2;layer_draws=1;scene3d_draws=0;id_mask_draws=0;backdrop_draws=1;visual_effect_draws=1;effect_uniform_writes=1;effect_uniform_bytes=272;effect_uniform_slots=2;spinner_draws=1;camera_bg_draws=0;render_passes=6;passes=clear:1/draw:4/scene3d:0/scene3d_overlay:0/idmask:0+0+0+0/present:1;texture_copies=2;command_buffers=1;gpu_ts_passes=6;gpu_ts_total_ns=89955;gpu_ts_max_ns=41331;buffer_upload_bytes=57288;texture_upload_bytes=0;buffer_grows=0;texture_creates=0;bind_group_creates=0;pipeline_creates=0;sampler_creates=0;mesh3d_creates=0;wasm_alloc_count=272;wasm_alloc_bytes=5804;wasm_realloc_count=0;wasm_realloc_grow_bytes=0;wasm_allocating_frames=36;image_upload_temp_allocs=0;image_upload_temp_bytes=0;image_upload_scratch_bytes=4194304;image_upload_scratch_grows=0;cpu_scratch_bytes=4753048;cpu_scratch_grows=0;cpu_scratch_growth_bytes=0;image=256x256;glyphs=96;image_tiles=96;missed120=0.000;hitch120=0.000;missed60=0.000;hitch60=0.000` |
| `web.wasm.webgpu.mixed_text_image_effects.legacy_rebind_unbatched` | `webgpu-mixed-effects-legacy-rebind-unbatched` | 3 | 12 | 36 | 0.075 | 0.083 | 0.083 | 0.083 | 0.078 | ms/frame | `draws=114;draw_items=114;draw_pipeline_binds=114;draw_bind_group_binds=102;draw_scissor_sets=114;solid_tris=472;image_draws=97;image_mesh_draws=0;nine_slice_draws=0;glyph_quads=96;sdf_glyph_quads=0;clip_depth_peak=1;damage_rects=2;layer_draws=1;scene3d_draws=0;id_mask_draws=0;backdrop_draws=1;visual_effect_draws=1;effect_uniform_writes=2;effect_uniform_bytes=32;effect_uniform_slots=2;spinner_draws=1;camera_bg_draws=0;render_passes=6;passes=clear:1/draw:4/scene3d:0/scene3d_overlay:0/idmask:0+0+0+0/present:1;texture_copies=2;command_buffers=1;gpu_ts_passes=6;gpu_ts_total_ns=130245;gpu_ts_max_ns=34707;buffer_upload_bytes=57048;texture_upload_bytes=0;buffer_grows=0;texture_creates=0;bind_group_creates=0;pipeline_creates=0;sampler_creates=0;mesh3d_creates=0;wasm_alloc_count=272;wasm_alloc_bytes=5804;wasm_realloc_count=0;wasm_realloc_grow_bytes=0;wasm_allocating_frames=36;image_upload_temp_allocs=0;image_upload_temp_bytes=0;image_upload_scratch_bytes=4194304;image_upload_scratch_grows=0;cpu_scratch_bytes=4753048;cpu_scratch_grows=0;cpu_scratch_growth_bytes=0;image=256x256;glyphs=96;image_tiles=96;missed120=0.000;hitch120=0.000;missed60=0.000;hitch60=0.000` |
| `web.wasm.webgpu.layer_damage_effects` | `webgpu-layer-damage-effects-current` | 3 | 12 | 36 | 0.058 | 0.058 | 0.058 | 0.058 | 0.056 | ms/frame | `draws=86;draw_items=86;draw_pipeline_binds=6;draw_bind_group_binds=12;draw_scissor_sets=4;solid_tris=508;image_draws=65;image_mesh_draws=0;nine_slice_draws=0;glyph_quads=72;sdf_glyph_quads=0;clip_depth_peak=1;damage_rects=3;layer_draws=3;scene3d_draws=0;id_mask_draws=0;backdrop_draws=4;visual_effect_draws=1;effect_uniform_writes=1;effect_uniform_bytes=1040;effect_uniform_slots=5;spinner_draws=1;camera_bg_draws=0;render_passes=5;passes=clear:1/draw:3/scene3d:0/scene3d_overlay:0/idmask:0+0+0+0/present:1;texture_copies=1;command_buffers=1;gpu_ts_passes=5;gpu_ts_total_ns=52249;gpu_ts_max_ns=20874;buffer_upload_bytes=50080;texture_upload_bytes=0;buffer_grows=0;texture_creates=0;bind_group_creates=0;pipeline_creates=0;sampler_creates=0;mesh3d_creates=0;wasm_alloc_count=272;wasm_alloc_bytes=6380;wasm_realloc_count=0;wasm_realloc_grow_bytes=0;wasm_allocating_frames=36;image_upload_temp_allocs=0;image_upload_temp_bytes=0;image_upload_scratch_bytes=4194304;image_upload_scratch_grows=0;cpu_scratch_bytes=4753816;cpu_scratch_grows=0;cpu_scratch_growth_bytes=0;image=256x256;glyphs=72;image_tiles=64;expected_layers=3;expected_damage_rects=3;expected_backdrops=4;missed120=0.000;hitch120=0.000;missed60=0.000;hitch60=0.000` |
| `web.wasm.webgpu.layer_damage_effects.legacy_rebind_unbatched` | `webgpu-layer-damage-effects-legacy-rebind-unbatched` | 3 | 12 | 36 | 0.075 | 0.083 | 0.083 | 0.083 | 0.078 | ms/frame | `draws=86;draw_items=86;draw_pipeline_binds=86;draw_bind_group_binds=76;draw_scissor_sets=86;solid_tris=508;image_draws=65;image_mesh_draws=0;nine_slice_draws=0;glyph_quads=72;sdf_glyph_quads=0;clip_depth_peak=1;damage_rects=3;layer_draws=3;scene3d_draws=0;id_mask_draws=0;backdrop_draws=4;visual_effect_draws=1;effect_uniform_writes=5;effect_uniform_bytes=80;effect_uniform_slots=5;spinner_draws=1;camera_bg_draws=0;render_passes=9;passes=clear:1/draw:7/scene3d:0/scene3d_overlay:0/idmask:0+0+0+0/present:1;texture_copies=5;command_buffers=1;gpu_ts_passes=9;gpu_ts_total_ns=205036;gpu_ts_max_ns=41833;buffer_upload_bytes=49120;texture_upload_bytes=0;buffer_grows=0;texture_creates=0;bind_group_creates=0;pipeline_creates=0;sampler_creates=0;mesh3d_creates=0;wasm_alloc_count=272;wasm_alloc_bytes=6380;wasm_realloc_count=0;wasm_realloc_grow_bytes=0;wasm_allocating_frames=36;image_upload_temp_allocs=0;image_upload_temp_bytes=0;image_upload_scratch_bytes=4194304;image_upload_scratch_grows=0;cpu_scratch_bytes=4753816;cpu_scratch_grows=0;cpu_scratch_growth_bytes=0;image=256x256;glyphs=72;image_tiles=64;expected_layers=3;expected_damage_rects=3;expected_backdrops=4;missed120=0.000;hitch120=0.000;missed60=0.000;hitch60=0.000` |
| `web.wasm.webgpu.command_family_matrix` | `webgpu-command-family-current` | 3 | 12 | 36 | 0.100 | 0.108 | 0.108 | 0.108 | 0.103 | ms/frame | `draws=649;draw_items=649;draw_pipeline_binds=3;draw_bind_group_binds=2;draw_scissor_sets=1;solid_tris=36;image_draws=640;image_mesh_draws=64;nine_slice_draws=64;glyph_quads=288;sdf_glyph_quads=288;clip_depth_peak=0;damage_rects=0;layer_draws=0;scene3d_draws=0;id_mask_draws=0;backdrop_draws=0;visual_effect_draws=0;effect_uniform_writes=0;effect_uniform_bytes=0;effect_uniform_slots=0;spinner_draws=0;camera_bg_draws=0;render_passes=1;passes=clear:0/draw:1/scene3d:0/scene3d_overlay:0/idmask:0+0+0+0/present:0;texture_copies=0;command_buffers=1;gpu_ts_passes=1;gpu_ts_total_ns=32458;gpu_ts_max_ns=32458;buffer_upload_bytes=165216;texture_upload_bytes=0;buffer_grows=0;texture_creates=0;bind_group_creates=0;pipeline_creates=0;sampler_creates=0;mesh3d_creates=0;wasm_alloc_count=236;wasm_alloc_bytes=4652;wasm_realloc_count=0;wasm_realloc_grow_bytes=0;wasm_allocating_frames=36;image_upload_temp_allocs=0;image_upload_temp_bytes=0;image_upload_scratch_bytes=4194304;image_upload_scratch_grows=0;cpu_scratch_bytes=5105648;cpu_scratch_grows=0;cpu_scratch_growth_bytes=0;image=256x256;expected_image_meshes=64;expected_nine_slices=64;expected_sdf_glyphs=288;expected_sdf_runs=8;expected_camera_bg=0;missed120=0.000;hitch120=0.000;missed60=0.000;hitch60=0.000` |
| `web.wasm.webgpu.command_family_matrix.legacy_rebind` | `webgpu-command-family-legacy-rebind` | 3 | 12 | 36 | 0.167 | 0.175 | 0.175 | 0.175 | 0.167 | ms/frame | `draws=649;draw_items=649;draw_pipeline_binds=649;draw_bind_group_binds=648;draw_scissor_sets=649;solid_tris=36;image_draws=640;image_mesh_draws=64;nine_slice_draws=64;glyph_quads=288;sdf_glyph_quads=288;clip_depth_peak=0;damage_rects=0;layer_draws=0;scene3d_draws=0;id_mask_draws=0;backdrop_draws=0;visual_effect_draws=0;effect_uniform_writes=0;effect_uniform_bytes=0;effect_uniform_slots=0;spinner_draws=0;camera_bg_draws=0;render_passes=1;passes=clear:0/draw:1/scene3d:0/scene3d_overlay:0/idmask:0+0+0+0/present:0;texture_copies=0;command_buffers=1;gpu_ts_passes=1;gpu_ts_total_ns=29125;gpu_ts_max_ns=29125;buffer_upload_bytes=165216;texture_upload_bytes=0;buffer_grows=0;texture_creates=0;bind_group_creates=0;pipeline_creates=0;sampler_creates=0;mesh3d_creates=0;wasm_alloc_count=236;wasm_alloc_bytes=4652;wasm_realloc_count=0;wasm_realloc_grow_bytes=0;wasm_allocating_frames=36;image_upload_temp_allocs=0;image_upload_temp_bytes=0;image_upload_scratch_bytes=4194304;image_upload_scratch_grows=0;cpu_scratch_bytes=5105648;cpu_scratch_grows=0;cpu_scratch_growth_bytes=0;image=256x256;expected_image_meshes=64;expected_nine_slices=64;expected_sdf_glyphs=288;expected_sdf_runs=8;expected_camera_bg=0;missed120=0.000;hitch120=0.000;missed60=0.000;hitch60=0.000` |
| `web.wasm.webgpu.draw_state_cache.current` | `webgpu-draw-state-cache-current` | 3 | 12 | 36 | 0.108 | 0.117 | 0.117 | 0.117 | 0.108 | ms/frame | `draws=1024;draw_items=1024;draw_pipeline_binds=1;draw_bind_group_binds=1;draw_scissor_sets=1;solid_tris=0;image_draws=1024;image_mesh_draws=0;nine_slice_draws=0;glyph_quads=0;sdf_glyph_quads=0;clip_depth_peak=0;damage_rects=0;layer_draws=0;scene3d_draws=0;id_mask_draws=0;backdrop_draws=0;visual_effect_draws=0;effect_uniform_writes=0;effect_uniform_bytes=0;effect_uniform_slots=0;spinner_draws=0;camera_bg_draws=0;render_passes=1;passes=clear:0/draw:1/scene3d:0/scene3d_overlay:0/idmask:0+0+0+0/present:0;texture_copies=0;command_buffers=1;gpu_ts_passes=1;gpu_ts_total_ns=58624;gpu_ts_max_ns=58624;buffer_upload_bytes=155664;texture_upload_bytes=0;buffer_grows=0;texture_creates=0;bind_group_creates=0;pipeline_creates=0;sampler_creates=0;mesh3d_creates=0;wasm_alloc_count=236;wasm_alloc_bytes=4652;wasm_realloc_count=0;wasm_realloc_grow_bytes=0;wasm_allocating_frames=36;image_upload_temp_allocs=0;image_upload_temp_bytes=0;image_upload_scratch_bytes=4194304;image_upload_scratch_grows=0;cpu_scratch_bytes=5128352;cpu_scratch_grows=0;cpu_scratch_growth_bytes=0;image=256x256;expected_draw_items=1024;missed120=0.000;hitch120=0.000;missed60=0.000;hitch60=0.000` |
| `web.wasm.webgpu.draw_state_cache.legacy_rebind` | `webgpu-draw-state-cache-legacy-rebind` | 3 | 12 | 36 | 0.208 | 0.217 | 0.217 | 0.217 | 0.208 | ms/frame | `draws=1024;draw_items=1024;draw_pipeline_binds=1024;draw_bind_group_binds=1024;draw_scissor_sets=1024;solid_tris=0;image_draws=1024;image_mesh_draws=0;nine_slice_draws=0;glyph_quads=0;sdf_glyph_quads=0;clip_depth_peak=0;damage_rects=0;layer_draws=0;scene3d_draws=0;id_mask_draws=0;backdrop_draws=0;visual_effect_draws=0;effect_uniform_writes=0;effect_uniform_bytes=0;effect_uniform_slots=0;spinner_draws=0;camera_bg_draws=0;render_passes=1;passes=clear:0/draw:1/scene3d:0/scene3d_overlay:0/idmask:0+0+0+0/present:0;texture_copies=0;command_buffers=1;gpu_ts_passes=1;gpu_ts_total_ns=57916;gpu_ts_max_ns=57916;buffer_upload_bytes=155664;texture_upload_bytes=0;buffer_grows=0;texture_creates=0;bind_group_creates=0;pipeline_creates=0;sampler_creates=0;mesh3d_creates=0;wasm_alloc_count=236;wasm_alloc_bytes=4652;wasm_realloc_count=0;wasm_realloc_grow_bytes=0;wasm_allocating_frames=36;image_upload_temp_allocs=0;image_upload_temp_bytes=0;image_upload_scratch_bytes=4194304;image_upload_scratch_grows=0;cpu_scratch_bytes=5128352;cpu_scratch_grows=0;cpu_scratch_growth_bytes=0;image=256x256;expected_draw_items=1024;missed120=0.000;hitch120=0.000;missed60=0.000;hitch60=0.000` |
| `web.wasm.webgpu.clip_state_cache.current` | `webgpu-clip-state-cache-current` | 3 | 12 | 36 | 0.067 | 0.075 | 0.075 | 0.075 | 0.064 | ms/frame | `draws=512;draw_items=512;draw_pipeline_binds=1;draw_bind_group_binds=1;draw_scissor_sets=16;solid_tris=0;image_draws=512;image_mesh_draws=0;nine_slice_draws=0;glyph_quads=0;sdf_glyph_quads=0;clip_depth_peak=2;damage_rects=0;layer_draws=0;scene3d_draws=0;id_mask_draws=0;backdrop_draws=0;visual_effect_draws=0;effect_uniform_writes=0;effect_uniform_bytes=0;effect_uniform_slots=0;spinner_draws=0;camera_bg_draws=0;render_passes=1;passes=clear:0/draw:1/scene3d:0/scene3d_overlay:0/idmask:0+0+0+0/present:0;texture_copies=0;command_buffers=1;gpu_ts_passes=1;gpu_ts_total_ns=24209;gpu_ts_max_ns=24209;buffer_upload_bytes=77840;texture_upload_bytes=0;buffer_grows=0;texture_creates=0;bind_group_creates=0;pipeline_creates=0;sampler_creates=0;mesh3d_creates=0;wasm_alloc_count=236;wasm_alloc_bytes=4652;wasm_realloc_count=0;wasm_realloc_grow_bytes=0;wasm_allocating_frames=36;image_upload_temp_allocs=0;image_upload_temp_bytes=0;image_upload_scratch_bytes=4194304;image_upload_scratch_grows=0;cpu_scratch_bytes=5128352;cpu_scratch_grows=0;cpu_scratch_growth_bytes=0;image=256x256;expected_draw_items=512;expected_clip_runs=16;expected_clip_depth=2;missed120=0.000;hitch120=0.000;missed60=0.000;hitch60=0.000` |
| `web.wasm.webgpu.clip_state_cache.legacy_rebind` | `webgpu-clip-state-cache-legacy-rebind` | 3 | 12 | 36 | 0.117 | 0.125 | 0.125 | 0.125 | 0.119 | ms/frame | `draws=512;draw_items=512;draw_pipeline_binds=512;draw_bind_group_binds=512;draw_scissor_sets=512;solid_tris=0;image_draws=512;image_mesh_draws=0;nine_slice_draws=0;glyph_quads=0;sdf_glyph_quads=0;clip_depth_peak=2;damage_rects=0;layer_draws=0;scene3d_draws=0;id_mask_draws=0;backdrop_draws=0;visual_effect_draws=0;effect_uniform_writes=0;effect_uniform_bytes=0;effect_uniform_slots=0;spinner_draws=0;camera_bg_draws=0;render_passes=1;passes=clear:0/draw:1/scene3d:0/scene3d_overlay:0/idmask:0+0+0+0/present:0;texture_copies=0;command_buffers=1;gpu_ts_passes=1;gpu_ts_total_ns=24083;gpu_ts_max_ns=24083;buffer_upload_bytes=77840;texture_upload_bytes=0;buffer_grows=0;texture_creates=0;bind_group_creates=0;pipeline_creates=0;sampler_creates=0;mesh3d_creates=0;wasm_alloc_count=236;wasm_alloc_bytes=4652;wasm_realloc_count=0;wasm_realloc_grow_bytes=0;wasm_allocating_frames=36;image_upload_temp_allocs=0;image_upload_temp_bytes=0;image_upload_scratch_bytes=4194304;image_upload_scratch_grows=0;cpu_scratch_bytes=5128352;cpu_scratch_grows=0;cpu_scratch_growth_bytes=0;image=256x256;expected_draw_items=512;expected_clip_runs=16;expected_clip_depth=2;missed120=0.000;hitch120=0.000;missed60=0.000;hitch60=0.000` |

## GPU Stage Attribution

| Field | Value |
| --- | --- |
| Timestamp query | `adapter-supported` |
| Status | `timestamp-query-collected` |
| Source | `adapter.features+renderer.timestamp_writes` |
| Collected rows | `27` |
| Collected passes | `192` |
| Total ns | `7639219` |

## Browser Trace

| Field | Value |
| --- | --- |
| Status | `collected` |
| Artifact | `/tmp/oxide-webgpu-browser-router-overlay-scratch-budget192.json` |
| Capture phase | `benchmark-report` |
| Timing source | `untraced-baseline-report` |
| Events | `77568` |
| GPU-related events | `47107` |
| WebGPU/Dawn events | `1395` |
| ANGLE events | `1` |
| Renderer events | `681` |
| Duration us | `1212857` |
| Category count | `25` |
| Sample categories | `WebCore,__metadata,benchmark,blink,blink.resource,blink.user_timing,blink_style,cc,devtools.timeline,disabled-by-default-blink.debug.layout,disabled-by-default-devtools.timeline,disabled-by-default-devtools.timeline.frame,disabled-by-default-display.framedisplayed,disabled-by-default-gpu.service,gpu,graphics.pipeline,input.scrolling,loading,navigation,raf_investigation,rail,shutdown,startup,toplevel.flow` |
| Benchmark trace mark status | `collected` |
| Benchmark trace mark events | `48` |
| Benchmark trace mark labels | `backdrop_batch_ab,clip_state_cache_ab,command_family_matrix,draw_state_cache_ab,effect_uniform_ab,frame_loop,id_mask_ab,layer_effects_matrix,mixed_matrix,scene3d_ab,upload_ab,upload_scratch_ab` |

### Browser Trace Benchmark Intervals

| Mark | Duration us | Events | GPU events | WebGPU/Dawn events | Renderer events | Event duration us |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| `frame_loop` | 18712 | 3152 | 1828 | 56 | 34 | 201176 |
| `id_mask_ab` | 55130 | 6613 | 4128 | 109 | 83 | 498729 |
| `upload_ab` | 144377 | 11276 | 7468 | 183 | 128 | 1309273 |
| `upload_scratch_ab` | 46649 | 3839 | 2773 | 89 | 18 | 436943 |
| `effect_uniform_ab` | 220462 | 5957 | 3177 | 88 | 18 | 1715675 |
| `backdrop_batch_ab` | 45991 | 3713 | 2679 | 88 | 18 | 379159 |
| `scene3d_ab` | 36652 | 6695 | 5134 | 172 | 36 | 244344 |
| `mixed_matrix` | 38189 | 3599 | 2647 | 88 | 18 | 269337 |
| `layer_effects_matrix` | 37600 | 3592 | 2640 | 88 | 18 | 328576 |
| `command_family_matrix` | 20782 | 3434 | 2580 | 88 | 18 | 210163 |
| `draw_state_cache_ab` | 29659 | 3606 | 2631 | 88 | 18 | 237561 |
| `clip_state_cache_ab` | 21232 | 3431 | 2588 | 88 | 18 | 144473 |

## Benchmark Marks

| Mark | Duration ms | Trace label | WASM before bytes | WASM after bytes | WASM growth bytes | JS heap before bytes | JS heap after bytes | JS heap growth bytes | JS heap sampled | GC exposed |
| --- | ---: | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| `frame_loop` | 22.400 | `yes` | 13041664 | 13041664 | 0 | 1217712 | 1253543 | 35831 | 1 | 1 |
| `id_mask_ab` | 62.600 | `yes` | 13041664 | 13041664 | 0 | 1254446 | 1301330 | 46884 | 1 | 1 |
| `upload_ab` | 145.000 | `yes` | 13041664 | 13041664 | 0 | 1301466 | 1326579 | 25113 | 1 | 1 |
| `upload_scratch_ab` | 46.600 | `yes` | 13041664 | 13041664 | 0 | 1326937 | 1338421 | 11484 | 1 | 1 |
| `effect_uniform_ab` | 220.400 | `yes` | 13041664 | 13041664 | 0 | 1338543 | 1358273 | 19730 | 1 | 1 |
| `backdrop_batch_ab` | 54.200 | `yes` | 13041664 | 13041664 | 0 | 1358415 | 1383411 | 24996 | 1 | 1 |
| `scene3d_ab` | 36.700 | `yes` | 13041664 | 13041664 | 0 | 1383517 | 1409287 | 25770 | 1 | 1 |
| `mixed_matrix` | 37.500 | `yes` | 13041664 | 13041664 | 0 | 1409411 | 1420325 | 10914 | 1 | 1 |
| `layer_effects_matrix` | 37.600 | `yes` | 13041664 | 13041664 | 0 | 1420457 | 1430882 | 10425 | 1 | 1 |
| `command_family_matrix` | 21.000 | `yes` | 13041664 | 13041664 | 0 | 1440362 | 1450497 | 10135 | 1 | 1 |
| `draw_state_cache_ab` | 29.400 | `yes` | 13041664 | 13041664 | 0 | 1450655 | 1462930 | 12275 | 1 | 1 |
| `clip_state_cache_ab` | 20.500 | `yes` | 13041664 | 13041664 | 0 | 1463160 | 1474802 | 11642 | 1 | 1 |

## Warm Resource Churn

| Check | Rows | Buffer Grows | Texture Creates | Bind Groups | Pipelines | Samplers | Meshes | Temp Allocs | Temp Bytes | Image Scratch Grows | CPU Scratch Grows | CPU Scratch Growth Bytes |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| `web.wasm.webgpu.warm_resource_churn.current_rows` | 14 checked / 13 excluded | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |

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
| `web.wasm.webgpu.draw_state_cache.current` | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| `web.wasm.webgpu.clip_state_cache.current` | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |

## WASM Allocation Audit

| Check | Rows | Total Allocs | Total Bytes | Reallocs | Realloc Grow Bytes | Max Allocs/Frame | Max Bytes/Frame | Max Peak Frame Bytes | Budget Allocs/Frame | Budget Bytes/Frame |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| `web.wasm.webgpu.wasm_allocation_audit.current_rows` | 14 checked / 13 excluded | 3376 | 68008 | 0 | 0 | 7.556 | 177.222 | 222 | 8 | 192 |

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
| `web.wasm.webgpu.mixed_text_image_effects` | 36 | 272 | 5804 | 7.556 | 161.222 | 0 | 0 | 36 | 206 |
| `web.wasm.webgpu.layer_damage_effects` | 36 | 272 | 6380 | 7.556 | 177.222 | 0 | 0 | 36 | 222 |
| `web.wasm.webgpu.command_family_matrix` | 36 | 236 | 4652 | 6.556 | 129.222 | 0 | 0 | 36 | 174 |
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
| `draw_state_cache` | `covered` | `current_vs_legacy` | 2 | 5 |
| `clip_state_cache` | `covered` | `current_vs_legacy` | 2 | 5 |

## A/B Summary

| Comparison | Current p50 ms | Legacy p50 ms | Legacy / Current | Current Passes | Legacy Passes | Current Upload Bytes | Legacy Upload Bytes | Vertices | Vertex Bytes |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| `web.wasm.webgpu.id_mask_compositor.current_vs_legacy_upload` | 0.092 | 0.175 | 1.909 | 12 | 12 | 1120 | 308320 | 9600 | 307200 |

## Upload Summary

| Comparison | Glyph Current p50 ms | Glyph Legacy p50 ms | Glyph Legacy / Current | Glyph Current Texture Bytes | Glyph Legacy Texture Bytes | Glyph Current GPU ns | Glyph Legacy GPU ns | Glyph Legacy / Current GPU | Image Current p50 ms | Image Legacy p50 ms | Image Legacy / Current | Image Current Texture Bytes | Image Legacy Texture Bytes | Image Current GPU ns | Image Legacy GPU ns | Image Legacy / Current GPU |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| `web.wasm.webgpu.upload.current_dirty_vs_legacy_full` | 0.050 | 2.958 | 59.167 | 16384 | 4194304 | 11082 | 14165 | 1.278 | 0.017 | 0.108 | 6.500 | 16384 | 262144 | 11749 | 16915 | 1.440 |

## Upload Scratch Summary

| Comparison | Current p50 ms | Legacy p50 ms | Legacy / Current | Current Temp Allocs | Legacy Temp Allocs | Current Temp Bytes | Legacy Temp Bytes | Current Scratch Bytes | Legacy Scratch Bytes | Current Texture Bytes | Legacy Texture Bytes | Updates |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| `web.wasm.webgpu.upload_scratch.current_reuse_vs_legacy_temp_alloc` | 0.358 | 0.425 | 1.186 | 0 | 72 | 0 | 884736 | 4194304 | 4194304 | 786432 | 786432 | 24 |

## Effect Uniform Summary

| Comparison | Current p50 ms | Legacy p50 ms | Legacy / Current p50 | Current GPU ns | Legacy GPU ns | Legacy / Current GPU | Current Writes | Legacy Writes | Current Slots | Legacy Slots | Current Backdrops | Legacy Backdrops | Current Texture Copies | Legacy Texture Copies | Current Passes | Legacy Passes |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| `web.wasm.webgpu.effect_uniform.batched_vs_legacy_write_each` | 0.208 | 0.225 | 1.080 | 1431265 | 1378129 | 0.963 | 1 | 48 | 48 | 48 | 48 | 48 | 48 | 48 | 51 | 51 |

## Backdrop Batch Summary

| Comparison | Current p50 ms | Legacy p50 ms | Legacy / Current | Current Writes | Legacy Writes | Current Slots | Legacy Slots | Current Backdrops | Legacy Backdrops | Current Texture Copies | Legacy Texture Copies | Current Passes | Legacy Passes |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| `web.wasm.webgpu.backdrop_batch.coalesced_vs_per_backdrop_copy` | 0.025 | 0.067 | 2.667 | 1 | 1 | 12 | 12 | 12 | 12 | 1 | 12 | 4 | 15 |

## Scene3D Summary

| Comparison | Reused p50 ms | Recreate p50 ms | Recreate / Reused | Reused Mesh Creates | Recreate Mesh Creates | Reused Buffer Grows | Recreate Buffer Grows | Reused CPU Scratch Grows | Recreate CPU Scratch Grows | Reused CPU Scratch Growth Bytes | Recreate CPU Scratch Growth Bytes | Meshes | Instances |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| `web.wasm.webgpu.scene3d.reused_mesh_vs_recreate_mesh` | 0.025 | 0.017 | 0.667 | 0 | 2 | 0 | 4 | 0 | 0 | 0 | 0 | 2 | 2 |

## Scene3D Stress Summary

| Comparison | Reused p50 ms | Recreate p50 ms | Recreate / Reused | Reused Mesh Creates | Recreate Mesh Creates | Reused Buffer Grows | Recreate Buffer Grows | Reused CPU Scratch Grows | Recreate CPU Scratch Grows | Reused CPU Scratch Growth Bytes | Recreate CPU Scratch Growth Bytes | Meshes | Instances |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| `web.wasm.webgpu.scene3d.stress_reused_mesh_vs_stress_recreate_mesh` | 0.067 | 0.075 | 1.125 | 0 | 2 | 0 | 4 | 0 | 0 | 0 | 0 | 2 | 96 |

## Mixed Scene Summary

| Comparison | Current p50 ms | Legacy p50 ms | Legacy / Current | Current Items | Legacy Items | Current Pipeline Binds | Legacy Pipeline Binds | Current Bind Groups | Legacy Bind Groups | Current Scissors | Legacy Scissors | Current Writes | Legacy Writes | Current Texture Copies | Legacy Texture Copies | Current Passes | Legacy Passes |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| `web.wasm.webgpu.mixed_text_image_effects.current_vs_legacy_rebind_unbatched` | 0.067 | 0.075 | 1.125 | 114 | 114 | 8 | 114 | 7 | 102 | 5 | 114 | 1 | 2 | 2 | 2 | 6 | 6 |

## Layer Effects Summary

| Comparison | Current p50 ms | Legacy p50 ms | Legacy / Current | Current Items | Legacy Items | Current Pipeline Binds | Legacy Pipeline Binds | Current Bind Groups | Legacy Bind Groups | Current Scissors | Legacy Scissors | Current Writes | Legacy Writes | Current Texture Copies | Legacy Texture Copies | Current Passes | Legacy Passes |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| `web.wasm.webgpu.layer_damage_effects.current_vs_legacy_rebind_unbatched` | 0.058 | 0.075 | 1.286 | 86 | 86 | 6 | 86 | 12 | 76 | 4 | 86 | 1 | 5 | 1 | 5 | 5 | 9 |

## Command Family Summary

| Comparison | Current p50 ms | Legacy p50 ms | Legacy / Current | Current Items | Legacy Items | Current Pipeline Binds | Legacy Pipeline Binds | Current Bind Groups | Legacy Bind Groups | Current Scissors | Legacy Scissors | Image Meshes | Nine Slices | SDF Glyphs | CameraBg Draws |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| `web.wasm.webgpu.command_family_matrix.current_vs_legacy_rebind` | 0.100 | 0.167 | 1.667 | 649 | 649 | 3 | 649 | 2 | 648 | 1 | 649 | 64/64 | 64/64 | 288/288 | 0/0 |

## Draw State Cache Summary

| Comparison | Current p50 ms | Legacy p50 ms | Legacy / Current | Current Items | Legacy Items | Current Pipeline Binds | Legacy Pipeline Binds | Current Bind Groups | Legacy Bind Groups | Current Scissors | Legacy Scissors |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| `web.wasm.webgpu.draw_state_cache.current_vs_legacy_rebind` | 0.108 | 0.208 | 1.923 | 1024 | 1024 | 1 | 1024 | 1 | 1024 | 1 | 1024 |

## Clip State Cache Summary

| Comparison | Current p50 ms | Legacy p50 ms | Legacy / Current | Current Items | Legacy Items | Current Clip Depth | Legacy Clip Depth | Current Pipeline Binds | Legacy Pipeline Binds | Current Bind Groups | Legacy Bind Groups | Current Scissors | Legacy Scissors |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| `web.wasm.webgpu.clip_state_cache.current_vs_legacy_rebind` | 0.067 | 0.117 | 1.750 | 512 | 512 | 2 | 2 | 1 | 512 | 1 | 512 | 16 | 512 |

## Pixel Check

| Target | Viewport | Pixdiff | Max Err | MSE | Artifact |
| --- | --- | ---: | ---: | ---: | --- |
| `app` | 320x240 | 0 | 0 | 0.000 | `/tmp/oxide-webgpu-browser-router-overlay-scratch-budget192.png` |

## Notes

- BrowserRenderer selected the WebGPU backend through async renderer initialization.
- This baseline was collected from a release wasm build served through the static web host.
- Production web visual startup is WebGPU-only; unsupported browsers return NOT SUPPORTED instead of drawing through Canvas2D.
- The WebGPU ID-mask current and legacy upload rows are captured in the same browser process and scene contract.
- The WebGPU effect-uniform A/B rows draw the same backdrop scene while comparing one batched dynamic-uniform upload against one queue write per backdrop.
- The WebGPU backdrop-batch A/B rows draw separated consecutive backdrops while comparing one shared scene-copy pass against the legacy per-backdrop copy path.
- The WebGPU layer/damage/effects A/B rows draw the same nested layer, damage, image, glyph, backdrop, visual-effect, and spinner workload while comparing current state/effect batching against legacy rebinding/unbatched toggles.
- The WebGPU command-family A/B rows draw the same generic ImageMesh, NineSlice, and SDF glyph workload while comparing current draw-state caching against a legacy rebind path and keeping web CameraBg work unavailable.
- The WebGPU clip-state A/B rows use real Oxide ClipPush/ClipPop commands to measure scissor-state caching.
- Pass-family counters provide browser GPU-stage attribution when direct timestamp queries are unavailable.
- Warm current-path WebGPU rows are gated against post-warmup resource creation, buffer growth, mesh creation, image-upload temp allocation, and CPU/image scratch growth.
- WASM allocation counters measure Rust allocator activity inside each post-warmup benchmark frame loop and are reported separately from renderer-owned resource churn.
- Chrome startup tracing is captured from a duplicate benchmark-report run so GPU/browser-process activity is tied to the same report workload without perturbing persisted timing rows.
- Browser User Timing marks surround every benchmark family and are persisted to prove the traced report run exercised the expected workload phases.
