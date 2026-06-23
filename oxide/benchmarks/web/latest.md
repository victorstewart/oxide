# Oxide WebAssembly Browser Baseline

Date: 2026-06-22

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
| Renderer | `draws=385` |
| Capture target | `app` |

## Browser Startup

| Field | Value |
| --- | ---: |
| Page start ms | 0.000 |
| WASM init start ms | 0.100 |
| WASM init ms | 11.200 |
| App init start ms | 29.700 |
| App init ms | 9.300 |
| First frame start ms | 39.000 |
| First frame ms | 16.300 |
| Report ready ms | 432.200 |
| WASM memory bytes | 13041664 |
| Package bytes | 3077030 |

### Browser Package Files

| File | Kind | Bytes |
| --- | --- | ---: |
| `host/web-app/www/pkg/oxide_host_web_bg.wasm` | `wasm` | 2961627 |
| `host/web-app/www/pkg/oxide_host_web.js` | `js` | 101950 |
| `host/web-app/www/pkg/oxide_host_web.d.ts` | `typescript` | 8816 |
| `host/web-app/www/pkg/oxide_host_web_bg.wasm.d.ts` | `wasm_typescript` | 4637 |

## Cases

| Case | Variant | Samples | Frames/Sample | Frames | p50 ms | p95 ms | p99 ms | Peak ms | Avg ms | Unit | Notes |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | --- | --- |
| `web.wasm.webgpu.frame_loop` | `webgpu` | 3 | 12 | 36 | 0.100 | 0.200 | 0.200 | 0.200 | 0.111 | ms/frame | `draws=8;draw_items=8;draw_pipeline_binds=8;draw_bind_group_binds=5;draw_scissor_sets=3;solid_tris=792;image_draws=0;image_mesh_draws=0;nine_slice_draws=0;glyph_quads=60;sdf_glyph_quads=0;clip_depth_peak=1;damage_rects=3;layer_draws=0;scene3d_draws=0;id_mask_draws=0;backdrop_draws=1;visual_effect_draws=0;effect_uniform_writes=1;effect_uniform_bytes=16;effect_uniform_slots=1;spinner_draws=1;camera_bg_draws=0;render_passes=5;passes=clear:1/draw:3/scene3d:0/scene3d_overlay:0/idmask:0+0+0+0/present:1;texture_copies=1;command_buffers=1;gpu_ts_passes=5;gpu_ts_total_ns=111251;gpu_ts_max_ns=65250;buffer_upload_bytes=44856;texture_upload_bytes=0;buffer_grows=0;texture_creates=0;bind_group_creates=0;pipeline_creates=0;sampler_creates=0;mesh3d_creates=0;wasm_alloc_count=236;wasm_alloc_bytes=4652;wasm_realloc_count=0;wasm_realloc_grow_bytes=0;wasm_allocating_frames=36;image_upload_temp_allocs=0;image_upload_temp_bytes=0;image_upload_scratch_bytes=17640;image_upload_scratch_grows=0;cpu_scratch_bytes=135344;cpu_scratch_grows=0;cpu_scratch_growth_bytes=0;missed120=0.000;hitch120=0.000;missed60=0.000;hitch60=0.000` |
| `web.wasm.webgpu.id_mask_compositor.current` | `webgpu-current` | 3 | 12 | 36 | 0.075 | 0.083 | 0.083 | 0.083 | 0.075 | ms/frame | `draws=12;draw_items=0;draw_pipeline_binds=0;draw_bind_group_binds=0;draw_scissor_sets=0;solid_tris=0;image_draws=0;image_mesh_draws=0;nine_slice_draws=0;glyph_quads=0;sdf_glyph_quads=0;clip_depth_peak=0;damage_rects=0;layer_draws=0;scene3d_draws=0;id_mask_draws=1;backdrop_draws=0;visual_effect_draws=0;effect_uniform_writes=0;effect_uniform_bytes=0;effect_uniform_slots=0;spinner_draws=0;camera_bg_draws=0;render_passes=12;passes=clear:0/draw:0/scene3d:0/scene3d_overlay:0/idmask:1+1+9+1/present:0;texture_copies=0;command_buffers=1;gpu_ts_passes=12;gpu_ts_total_ns=2350501;gpu_ts_max_ns=358791;buffer_upload_bytes=1120;texture_upload_bytes=0;buffer_grows=0;texture_creates=0;bind_group_creates=0;pipeline_creates=0;sampler_creates=0;mesh3d_creates=0;wasm_alloc_count=236;wasm_alloc_bytes=4652;wasm_realloc_count=0;wasm_realloc_grow_bytes=0;wasm_allocating_frames=36;image_upload_temp_allocs=0;image_upload_temp_bytes=0;image_upload_scratch_bytes=17640;image_upload_scratch_grows=0;cpu_scratch_bytes=446816;cpu_scratch_grows=0;cpu_scratch_growth_bytes=0;vertices=9600;vertex_bytes=307200;missed120=0.000;hitch120=0.000;missed60=0.000;hitch60=0.000` |
| `web.wasm.webgpu.glyph_atlas_upload.current_dirty` | `webgpu-dirty-atlas-update` | 3 | 12 | 36 | 0.042 | 0.042 | 0.042 | 0.042 | 0.042 | ms/frame | `draws=2;draw_items=2;draw_pipeline_binds=2;draw_bind_group_binds=1;draw_scissor_sets=1;solid_tris=36;image_draws=0;image_mesh_draws=0;nine_slice_draws=0;glyph_quads=96;sdf_glyph_quads=0;clip_depth_peak=0;damage_rects=0;layer_draws=0;scene3d_draws=0;id_mask_draws=0;backdrop_draws=0;visual_effect_draws=0;effect_uniform_writes=0;effect_uniform_bytes=0;effect_uniform_slots=0;spinner_draws=0;camera_bg_draws=0;render_passes=1;passes=clear:0/draw:1/scene3d:0/scene3d_overlay:0/idmask:0+0+0+0/present:0;texture_copies=0;command_buffers=1;gpu_ts_passes=1;gpu_ts_total_ns=11125;gpu_ts_max_ns=11125;buffer_upload_bytes=16224;texture_upload_bytes=16384;buffer_grows=0;texture_creates=0;bind_group_creates=0;pipeline_creates=0;sampler_creates=0;mesh3d_creates=0;wasm_alloc_count=236;wasm_alloc_bytes=4652;wasm_realloc_count=0;wasm_realloc_grow_bytes=0;wasm_allocating_frames=36;image_upload_temp_allocs=0;image_upload_temp_bytes=0;image_upload_scratch_bytes=17640;image_upload_scratch_grows=0;cpu_scratch_bytes=455784;cpu_scratch_grows=0;cpu_scratch_growth_bytes=0;atlas=1024x1024;dirty=64x64;missed120=0.000;hitch120=0.000;missed60=0.000;hitch60=0.000` |
| `web.wasm.webgpu.image_upload.current_dirty` | `webgpu-dirty-rgba-update` | 3 | 12 | 36 | 0.033 | 0.042 | 0.042 | 0.042 | 0.036 | ms/frame | `draws=2;draw_items=2;draw_pipeline_binds=2;draw_bind_group_binds=1;draw_scissor_sets=1;solid_tris=36;image_draws=1;image_mesh_draws=0;nine_slice_draws=0;glyph_quads=0;sdf_glyph_quads=0;clip_depth_peak=0;damage_rects=0;layer_draws=0;scene3d_draws=0;id_mask_draws=0;backdrop_draws=0;visual_effect_draws=0;effect_uniform_writes=0;effect_uniform_bytes=0;effect_uniform_slots=0;spinner_draws=0;camera_bg_draws=0;render_passes=1;passes=clear:0/draw:1/scene3d:0/scene3d_overlay:0/idmask:0+0+0+0/present:0;texture_copies=0;command_buffers=1;gpu_ts_passes=1;gpu_ts_total_ns=12333;gpu_ts_max_ns=12333;buffer_upload_bytes=1784;texture_upload_bytes=16384;buffer_grows=0;texture_creates=0;bind_group_creates=0;pipeline_creates=0;sampler_creates=0;mesh3d_creates=0;wasm_alloc_count=236;wasm_alloc_bytes=4652;wasm_realloc_count=0;wasm_realloc_grow_bytes=0;wasm_allocating_frames=36;image_upload_temp_allocs=0;image_upload_temp_bytes=0;image_upload_scratch_bytes=17640;image_upload_scratch_grows=0;cpu_scratch_bytes=455784;cpu_scratch_grows=0;cpu_scratch_growth_bytes=0;image=256x256;dirty=64x64;missed120=0.000;hitch120=0.000;missed60=0.000;hitch60=0.000` |
| `web.wasm.webgpu.effect_uniform.current_batched` | `webgpu-effect-uniform-current-batched` | 3 | 12 | 36 | 0.217 | 0.258 | 0.258 | 0.258 | 0.228 | ms/frame | `draws=50;draw_items=50;draw_pipeline_binds=50;draw_bind_group_binds=97;draw_scissor_sets=49;solid_tris=4;image_draws=1;image_mesh_draws=0;nine_slice_draws=0;glyph_quads=0;sdf_glyph_quads=0;clip_depth_peak=0;damage_rects=0;layer_draws=0;scene3d_draws=0;id_mask_draws=0;backdrop_draws=48;visual_effect_draws=0;effect_uniform_writes=1;effect_uniform_bytes=16;effect_uniform_slots=48;spinner_draws=0;camera_bg_draws=0;render_passes=51;passes=clear:1/draw:49/scene3d:0/scene3d_overlay:0/idmask:0+0+0+0/present:1;texture_copies=48;command_buffers=1;gpu_ts_passes=51;gpu_ts_total_ns=1633034;gpu_ts_max_ns=170482;buffer_upload_bytes=7688;texture_upload_bytes=0;buffer_grows=0;texture_creates=0;bind_group_creates=0;pipeline_creates=0;sampler_creates=0;mesh3d_creates=0;wasm_alloc_count=236;wasm_alloc_bytes=4652;wasm_realloc_count=0;wasm_realloc_grow_bytes=0;wasm_allocating_frames=36;image_upload_temp_allocs=0;image_upload_temp_bytes=0;image_upload_scratch_bytes=17640;image_upload_scratch_grows=0;cpu_scratch_bytes=459144;cpu_scratch_grows=0;cpu_scratch_growth_bytes=0;expected_backdrops=48;missed120=0.000;hitch120=0.000;missed60=0.000;hitch60=0.000` |
| `web.wasm.webgpu.backdrop_batch.current_coalesced` | `webgpu-backdrop-batch-current-coalesced` | 3 | 12 | 36 | 0.025 | 0.025 | 0.025 | 0.025 | 0.025 | ms/frame | `draws=14;draw_items=14;draw_pipeline_binds=3;draw_bind_group_binds=3;draw_scissor_sets=2;solid_tris=4;image_draws=1;image_mesh_draws=0;nine_slice_draws=0;glyph_quads=0;sdf_glyph_quads=0;clip_depth_peak=0;damage_rects=0;layer_draws=0;scene3d_draws=0;id_mask_draws=0;backdrop_draws=12;visual_effect_draws=0;effect_uniform_writes=1;effect_uniform_bytes=16;effect_uniform_slots=12;spinner_draws=0;camera_bg_draws=0;render_passes=4;passes=clear:1/draw:2/scene3d:0/scene3d_overlay:0/idmask:0+0+0+0/present:1;texture_copies=1;command_buffers=1;gpu_ts_passes=4;gpu_ts_total_ns=67162;gpu_ts_max_ns=31123;buffer_upload_bytes=2216;texture_upload_bytes=0;buffer_grows=0;texture_creates=0;bind_group_creates=0;pipeline_creates=0;sampler_creates=0;mesh3d_creates=0;wasm_alloc_count=236;wasm_alloc_bytes=4652;wasm_realloc_count=0;wasm_realloc_grow_bytes=0;wasm_allocating_frames=36;image_upload_temp_allocs=0;image_upload_temp_bytes=0;image_upload_scratch_bytes=17640;image_upload_scratch_grows=0;cpu_scratch_bytes=459144;cpu_scratch_grows=0;cpu_scratch_growth_bytes=0;expected_backdrops=12;missed120=0.000;hitch120=0.000;missed60=0.000;hitch60=0.000` |
| `web.wasm.webgpu.scene3d.reused_mesh` | `webgpu-scene3d-reused-mesh` | 3 | 12 | 36 | 0.017 | 0.017 | 0.017 | 0.017 | 0.014 | ms/frame | `draws=2;draw_items=0;draw_pipeline_binds=0;draw_bind_group_binds=0;draw_scissor_sets=0;solid_tris=0;image_draws=0;image_mesh_draws=0;nine_slice_draws=0;glyph_quads=0;sdf_glyph_quads=0;clip_depth_peak=0;damage_rects=0;layer_draws=0;scene3d_draws=2;id_mask_draws=0;backdrop_draws=0;visual_effect_draws=0;effect_uniform_writes=0;effect_uniform_bytes=0;effect_uniform_slots=0;spinner_draws=0;camera_bg_draws=0;render_passes=1;passes=clear:0/draw:0/scene3d:1/scene3d_overlay:0/idmask:0+0+0+0/present:0;texture_copies=0;command_buffers=1;gpu_ts_passes=1;gpu_ts_total_ns=8499;gpu_ts_max_ns=8499;buffer_upload_bytes=528;texture_upload_bytes=0;buffer_grows=0;texture_creates=0;bind_group_creates=0;pipeline_creates=0;sampler_creates=0;mesh3d_creates=0;wasm_alloc_count=236;wasm_alloc_bytes=4652;wasm_realloc_count=0;wasm_realloc_grow_bytes=0;wasm_allocating_frames=36;image_upload_temp_allocs=0;image_upload_temp_bytes=0;image_upload_scratch_bytes=17640;image_upload_scratch_grows=0;cpu_scratch_bytes=460096;cpu_scratch_grows=0;cpu_scratch_growth_bytes=0;meshes=2;instances=2;missed120=0.000;hitch120=0.000;missed60=0.000;hitch60=0.000` |
| `web.wasm.webgpu.scene3d.recreate_mesh` | `webgpu-scene3d-recreate-mesh` | 3 | 12 | 36 | 0.025 | 0.025 | 0.025 | 0.025 | 0.025 | ms/frame | `draws=2;draw_items=0;draw_pipeline_binds=0;draw_bind_group_binds=0;draw_scissor_sets=0;solid_tris=0;image_draws=0;image_mesh_draws=0;nine_slice_draws=0;glyph_quads=0;sdf_glyph_quads=0;clip_depth_peak=0;damage_rects=0;layer_draws=0;scene3d_draws=2;id_mask_draws=0;backdrop_draws=0;visual_effect_draws=0;effect_uniform_writes=0;effect_uniform_bytes=0;effect_uniform_slots=0;spinner_draws=0;camera_bg_draws=0;render_passes=1;passes=clear:0/draw:0/scene3d:1/scene3d_overlay:0/idmask:0+0+0+0/present:0;texture_copies=0;command_buffers=1;gpu_ts_passes=1;gpu_ts_total_ns=9375;gpu_ts_max_ns=9375;buffer_upload_bytes=720;texture_upload_bytes=0;buffer_grows=4;texture_creates=0;bind_group_creates=0;pipeline_creates=0;sampler_creates=0;mesh3d_creates=2;wasm_alloc_count=668;wasm_alloc_bytes=26540;wasm_realloc_count=3;wasm_realloc_grow_bytes=6272;wasm_allocating_frames=36;image_upload_temp_allocs=0;image_upload_temp_bytes=0;image_upload_scratch_bytes=17640;image_upload_scratch_grows=0;cpu_scratch_bytes=466816;cpu_scratch_grows=0;cpu_scratch_growth_bytes=0;meshes=2;instances=2;missed120=0.000;hitch120=0.000;missed60=0.000;hitch60=0.000` |
| `web.wasm.webgpu.scene3d.stress_reused_mesh` | `webgpu-scene3d-stress-reused-mesh` | 3 | 12 | 36 | 0.067 | 0.075 | 0.075 | 0.075 | 0.069 | ms/frame | `draws=96;draw_items=0;draw_pipeline_binds=0;draw_bind_group_binds=0;draw_scissor_sets=0;solid_tris=0;image_draws=0;image_mesh_draws=0;nine_slice_draws=0;glyph_quads=0;sdf_glyph_quads=0;clip_depth_peak=0;damage_rects=0;layer_draws=0;scene3d_draws=96;id_mask_draws=0;backdrop_draws=0;visual_effect_draws=0;effect_uniform_writes=0;effect_uniform_bytes=0;effect_uniform_slots=0;spinner_draws=0;camera_bg_draws=0;render_passes=1;passes=clear:0/draw:0/scene3d:1/scene3d_overlay:0/idmask:0+0+0+0/present:0;texture_copies=0;command_buffers=1;gpu_ts_passes=1;gpu_ts_total_ns=11332;gpu_ts_max_ns=11332;buffer_upload_bytes=24592;texture_upload_bytes=0;buffer_grows=0;texture_creates=0;bind_group_creates=0;pipeline_creates=0;sampler_creates=0;mesh3d_creates=0;wasm_alloc_count=236;wasm_alloc_bytes=4652;wasm_realloc_count=0;wasm_realloc_grow_bytes=0;wasm_allocating_frames=36;image_upload_temp_allocs=0;image_upload_temp_bytes=0;image_upload_scratch_bytes=17640;image_upload_scratch_grows=0;cpu_scratch_bytes=500560;cpu_scratch_grows=0;cpu_scratch_growth_bytes=0;meshes=2;instances=96;missed120=0.000;hitch120=0.000;missed60=0.000;hitch60=0.000` |
| `web.wasm.webgpu.scene3d.stress_recreate_mesh` | `webgpu-scene3d-stress-recreate-mesh` | 3 | 12 | 36 | 0.083 | 0.092 | 0.092 | 0.092 | 0.083 | ms/frame | `draws=96;draw_items=0;draw_pipeline_binds=0;draw_bind_group_binds=0;draw_scissor_sets=0;solid_tris=0;image_draws=0;image_mesh_draws=0;nine_slice_draws=0;glyph_quads=0;sdf_glyph_quads=0;clip_depth_peak=0;damage_rects=0;layer_draws=0;scene3d_draws=96;id_mask_draws=0;backdrop_draws=0;visual_effect_draws=0;effect_uniform_writes=0;effect_uniform_bytes=0;effect_uniform_slots=0;spinner_draws=0;camera_bg_draws=0;render_passes=1;passes=clear:0/draw:0/scene3d:1/scene3d_overlay:0/idmask:0+0+0+0/present:0;texture_copies=0;command_buffers=1;gpu_ts_passes=1;gpu_ts_total_ns=11957;gpu_ts_max_ns=11957;buffer_upload_bytes=24784;texture_upload_bytes=0;buffer_grows=4;texture_creates=0;bind_group_creates=0;pipeline_creates=0;sampler_creates=0;mesh3d_creates=2;wasm_alloc_count=668;wasm_alloc_bytes=26540;wasm_realloc_count=1;wasm_realloc_grow_bytes=7168;wasm_allocating_frames=36;image_upload_temp_allocs=0;image_upload_temp_bytes=0;image_upload_scratch_bytes=17640;image_upload_scratch_grows=0;cpu_scratch_bytes=507728;cpu_scratch_grows=0;cpu_scratch_growth_bytes=0;meshes=2;instances=96;missed120=0.000;hitch120=0.000;missed60=0.000;hitch60=0.000` |
| `web.wasm.webgpu.mixed_text_image_effects` | `webgpu-mixed-effects-current` | 3 | 12 | 36 | 0.067 | 0.075 | 0.075 | 0.075 | 0.067 | ms/frame | `draws=115;draw_items=115;draw_pipeline_binds=9;draw_bind_group_binds=8;draw_scissor_sets=5;solid_tris=472;image_draws=97;image_mesh_draws=0;nine_slice_draws=0;glyph_quads=96;sdf_glyph_quads=0;clip_depth_peak=1;damage_rects=2;layer_draws=1;scene3d_draws=0;id_mask_draws=0;backdrop_draws=1;visual_effect_draws=1;effect_uniform_writes=1;effect_uniform_bytes=272;effect_uniform_slots=2;spinner_draws=1;camera_bg_draws=0;render_passes=6;passes=clear:1/draw:5/scene3d:0/scene3d_overlay:0/idmask:0+0+0+0/present:0;texture_copies=2;command_buffers=1;gpu_ts_passes=6;gpu_ts_total_ns=108536;gpu_ts_max_ns=35873;buffer_upload_bytes=51296;texture_upload_bytes=0;buffer_grows=0;texture_creates=0;bind_group_creates=0;pipeline_creates=0;sampler_creates=0;mesh3d_creates=0;wasm_alloc_count=236;wasm_alloc_bytes=4652;wasm_realloc_count=0;wasm_realloc_grow_bytes=0;wasm_allocating_frames=36;image_upload_temp_allocs=0;image_upload_temp_bytes=0;image_upload_scratch_bytes=17640;image_upload_scratch_grows=0;cpu_scratch_bytes=545760;cpu_scratch_grows=0;cpu_scratch_growth_bytes=0;image=256x256;glyphs=96;image_tiles=96;missed120=0.000;hitch120=0.000;missed60=0.000;hitch60=0.000` |
| `web.wasm.webgpu.layer_damage_effects` | `webgpu-layer-damage-effects-current` | 3 | 12 | 36 | 0.067 | 0.067 | 0.067 | 0.067 | 0.067 | ms/frame | `draws=89;draw_items=89;draw_pipeline_binds=8;draw_bind_group_binds=15;draw_scissor_sets=6;solid_tris=508;image_draws=65;image_mesh_draws=0;nine_slice_draws=0;glyph_quads=72;sdf_glyph_quads=0;clip_depth_peak=1;damage_rects=3;layer_draws=3;scene3d_draws=0;id_mask_draws=0;backdrop_draws=4;visual_effect_draws=1;effect_uniform_writes=1;effect_uniform_bytes=1040;effect_uniform_slots=5;spinner_draws=1;camera_bg_draws=0;render_passes=9;passes=clear:3/draw:6/scene3d:0/scene3d_overlay:0/idmask:0+0+0+0/present:0;texture_copies=1;command_buffers=1;gpu_ts_passes=9;gpu_ts_total_ns=106532;gpu_ts_max_ns=36080;buffer_upload_bytes=45928;texture_upload_bytes=0;buffer_grows=0;texture_creates=0;bind_group_creates=0;pipeline_creates=0;sampler_creates=0;mesh3d_creates=0;wasm_alloc_count=236;wasm_alloc_bytes=4652;wasm_realloc_count=0;wasm_realloc_grow_bytes=0;wasm_allocating_frames=36;image_upload_temp_allocs=0;image_upload_temp_bytes=0;image_upload_scratch_bytes=17640;image_upload_scratch_grows=0;cpu_scratch_bytes=546528;cpu_scratch_grows=0;cpu_scratch_growth_bytes=0;image=256x256;glyphs=72;image_tiles=64;expected_layers=3;expected_damage_rects=3;expected_backdrops=4;missed120=0.000;hitch120=0.000;missed60=0.000;hitch60=0.000` |
| `web.wasm.webgpu.clean_layer.clean_reuse` | `webgpu-clean-layer-clean-reuse` | 3 | 12 | 36 | 0.017 | 0.017 | 0.017 | 0.017 | 0.017 | ms/frame | `draws=3;draw_items=3;draw_pipeline_binds=3;draw_bind_group_binds=1;draw_scissor_sets=1;solid_tris=40;image_draws=0;image_mesh_draws=0;nine_slice_draws=0;glyph_quads=0;sdf_glyph_quads=0;clip_depth_peak=0;damage_rects=0;layer_draws=1;scene3d_draws=0;id_mask_draws=0;backdrop_draws=0;visual_effect_draws=0;effect_uniform_writes=0;effect_uniform_bytes=0;effect_uniform_slots=0;spinner_draws=0;camera_bg_draws=0;render_passes=1;passes=clear:0/draw:1/scene3d:0/scene3d_overlay:0/idmask:0+0+0+0/present:0;texture_copies=0;command_buffers=1;gpu_ts_passes=1;gpu_ts_total_ns=11415;gpu_ts_max_ns=11415;buffer_upload_bytes=1992;texture_upload_bytes=0;buffer_grows=0;texture_creates=0;bind_group_creates=0;pipeline_creates=0;sampler_creates=0;mesh3d_creates=0;wasm_alloc_count=236;wasm_alloc_bytes=4652;wasm_realloc_count=0;wasm_realloc_grow_bytes=0;wasm_allocating_frames=36;image_upload_temp_allocs=0;image_upload_temp_bytes=0;image_upload_scratch_bytes=17640;image_upload_scratch_grows=0;cpu_scratch_bytes=554208;cpu_scratch_grows=0;cpu_scratch_growth_bytes=0;image=256x256;glyphs=96;image_tiles=144;expected_layers=1;missed120=0.000;hitch120=0.000;missed60=0.000;hitch60=0.000` |
| `web.wasm.webgpu.command_family_matrix` | `webgpu-command-family-current` | 3 | 12 | 36 | 0.108 | 0.108 | 0.108 | 0.108 | 0.106 | ms/frame | `draws=649;draw_items=649;draw_pipeline_binds=3;draw_bind_group_binds=2;draw_scissor_sets=1;solid_tris=36;image_draws=640;image_mesh_draws=64;nine_slice_draws=64;glyph_quads=288;sdf_glyph_quads=288;clip_depth_peak=0;damage_rects=0;layer_draws=0;scene3d_draws=0;id_mask_draws=0;backdrop_draws=0;visual_effect_draws=0;effect_uniform_writes=0;effect_uniform_bytes=0;effect_uniform_slots=0;spinner_draws=0;camera_bg_draws=0;render_passes=1;passes=clear:0/draw:1/scene3d:0/scene3d_overlay:0/idmask:0+0+0+0/present:0;texture_copies=0;command_buffers=1;gpu_ts_passes=1;gpu_ts_total_ns=28708;gpu_ts_max_ns=28708;buffer_upload_bytes=142688;texture_upload_bytes=0;buffer_grows=0;texture_creates=0;bind_group_creates=0;pipeline_creates=0;sampler_creates=0;mesh3d_creates=0;wasm_alloc_count=236;wasm_alloc_bytes=4652;wasm_realloc_count=0;wasm_realloc_grow_bytes=0;wasm_allocating_frames=36;image_upload_temp_allocs=0;image_upload_temp_bytes=0;image_upload_scratch_bytes=17640;image_upload_scratch_grows=0;cpu_scratch_bytes=848696;cpu_scratch_grows=0;cpu_scratch_growth_bytes=0;image=256x256;expected_image_meshes=64;expected_nine_slices=64;expected_sdf_glyphs=288;expected_sdf_runs=8;expected_camera_bg=0;missed120=0.000;hitch120=0.000;missed60=0.000;hitch60=0.000` |
| `web.wasm.webgpu.glyph_run.current` | `webgpu-glyph-run-current` | 3 | 12 | 36 | 0.058 | 0.058 | 0.058 | 0.058 | 0.058 | ms/frame | `draws=65;draw_items=65;draw_pipeline_binds=3;draw_bind_group_binds=1;draw_scissor_sets=1;solid_tris=4;image_draws=0;image_mesh_draws=0;nine_slice_draws=0;glyph_quads=512;sdf_glyph_quads=256;clip_depth_peak=0;damage_rects=0;layer_draws=0;scene3d_draws=0;id_mask_draws=0;backdrop_draws=0;visual_effect_draws=0;effect_uniform_writes=0;effect_uniform_bytes=0;effect_uniform_slots=0;spinner_draws=0;camera_bg_draws=0;render_passes=1;passes=clear:0/draw:1/scene3d:0/scene3d_overlay:0/idmask:0+0+0+0/present:0;texture_copies=0;command_buffers=1;gpu_ts_passes=1;gpu_ts_total_ns=15249;gpu_ts_max_ns=15249;buffer_upload_bytes=78048;texture_upload_bytes=0;buffer_grows=0;texture_creates=0;bind_group_creates=0;pipeline_creates=0;sampler_creates=0;mesh3d_creates=0;wasm_alloc_count=236;wasm_alloc_bytes=4652;wasm_realloc_count=0;wasm_realloc_grow_bytes=0;wasm_allocating_frames=36;image_upload_temp_allocs=0;image_upload_temp_bytes=0;image_upload_scratch_bytes=17640;image_upload_scratch_grows=0;cpu_scratch_bytes=848696;cpu_scratch_grows=0;cpu_scratch_growth_bytes=0;atlas=1024x1024;expected_sdf_runs=32;expected_draw_items=65;missed120=0.000;hitch120=0.000;missed60=0.000;hitch60=0.000` |
| `web.wasm.webgpu.neon_marker.current` | `webgpu-neon-marker-current` | 3 | 12 | 36 | 0.242 | 0.267 | 0.267 | 0.267 | 0.250 | ms/frame | `draws=192;draw_items=192;draw_pipeline_binds=1;draw_bind_group_binds=0;draw_scissor_sets=1;solid_tris=6912;image_draws=0;image_mesh_draws=0;nine_slice_draws=0;glyph_quads=0;sdf_glyph_quads=0;clip_depth_peak=0;damage_rects=0;layer_draws=0;scene3d_draws=0;id_mask_draws=0;backdrop_draws=0;visual_effect_draws=0;effect_uniform_writes=0;effect_uniform_bytes=0;effect_uniform_slots=0;spinner_draws=0;camera_bg_draws=0;render_passes=1;passes=clear:0/draw:1/scene3d:0/scene3d_overlay:0/idmask:0+0+0+0/present:0;texture_copies=0;command_buffers=1;gpu_ts_passes=1;gpu_ts_total_ns=10958;gpu_ts_max_ns=10958;buffer_upload_bytes=310288;texture_upload_bytes=0;buffer_grows=0;texture_creates=0;bind_group_creates=0;pipeline_creates=0;sampler_creates=0;mesh3d_creates=0;wasm_alloc_count=236;wasm_alloc_bytes=4652;wasm_realloc_count=0;wasm_realloc_grow_bytes=0;wasm_allocating_frames=36;image_upload_temp_allocs=0;image_upload_temp_bytes=0;image_upload_scratch_bytes=17640;image_upload_scratch_grows=0;cpu_scratch_bytes=1334152;cpu_scratch_grows=0;cpu_scratch_growth_bytes=0;expected_draw_items=192;missed120=0.000;hitch120=0.000;missed60=0.000;hitch60=0.000` |
| `web.wasm.webgpu.direct_surface.current` | `webgpu-direct-surface-current` | 3 | 12 | 36 | 0.058 | 0.058 | 0.058 | 0.058 | 0.056 | ms/frame | `draws=385;draw_items=385;draw_pipeline_binds=2;draw_bind_group_binds=1;draw_scissor_sets=1;solid_tris=4;image_draws=384;image_mesh_draws=0;nine_slice_draws=0;glyph_quads=0;sdf_glyph_quads=0;clip_depth_peak=0;damage_rects=0;layer_draws=0;scene3d_draws=0;id_mask_draws=0;backdrop_draws=0;visual_effect_draws=0;effect_uniform_writes=0;effect_uniform_bytes=0;effect_uniform_slots=0;spinner_draws=0;camera_bg_draws=0;render_passes=1;passes=clear:0/draw:1/scene3d:0/scene3d_overlay:0/idmask:0+0+0+0/present:0;texture_copies=0;command_buffers=1;gpu_ts_passes=1;gpu_ts_total_ns=25541;gpu_ts_max_ns=25541;buffer_upload_bytes=58592;texture_upload_bytes=0;buffer_grows=0;texture_creates=0;bind_group_creates=0;pipeline_creates=0;sampler_creates=0;mesh3d_creates=0;wasm_alloc_count=236;wasm_alloc_bytes=4652;wasm_realloc_count=0;wasm_realloc_grow_bytes=0;wasm_allocating_frames=36;image_upload_temp_allocs=0;image_upload_temp_bytes=0;image_upload_scratch_bytes=17640;image_upload_scratch_grows=0;cpu_scratch_bytes=1334152;cpu_scratch_grows=0;cpu_scratch_growth_bytes=0;image=256x256;expected_draw_items=385;missed120=0.000;hitch120=0.000;missed60=0.000;hitch60=0.000` |

## GPU Stage Attribution

| Field | Value |
| --- | --- |
| Timestamp query | `adapter-supported` |
| Status | `timestamp-query-collected` |
| Source | `adapter.features+renderer.timestamp_writes` |
| Collected rows | `17` |
| Collected passes | `98` |
| Total ns | `4533508` |

### GPU Timestamp Stage Breakdown

| Stage | Passes | Timestamp ns |
| --- | ---: | ---: |
| `clear` | 7 | 0 |
| `draw` | 72 | 2078055 |
| `scene3d` | 4 | 41163 |
| `scene3d_overlay` | 0 | 0 |
| `id_mask_raster` | 1 | 17667 |
| `id_mask_field_seed` | 1 | 43333 |
| `id_mask_field_jump` | 9 | 1930710 |
| `id_mask_compositor` | 1 | 358791 |
| `present` | 3 | 63789 |

### GPU Timestamp Row Reconciliation

| Row | Render Passes | Timestamp Passes | Timestamp ns | Family Passes | Family Timestamp ns |
| --- | ---: | ---: | ---: | ---: | ---: |
| `web.wasm.webgpu.frame_loop` | 5 | 5 | 111251 | 5 | 111251 |
| `web.wasm.webgpu.id_mask_compositor.current` | 12 | 12 | 2350501 | 12 | 2350501 |
| `web.wasm.webgpu.glyph_atlas_upload.current_dirty` | 1 | 1 | 11125 | 1 | 11125 |
| `web.wasm.webgpu.image_upload.current_dirty` | 1 | 1 | 12333 | 1 | 12333 |
| `web.wasm.webgpu.effect_uniform.current_batched` | 51 | 51 | 1633034 | 51 | 1633034 |
| `web.wasm.webgpu.backdrop_batch.current_coalesced` | 4 | 4 | 67162 | 4 | 67162 |
| `web.wasm.webgpu.scene3d.reused_mesh` | 1 | 1 | 8499 | 1 | 8499 |
| `web.wasm.webgpu.scene3d.recreate_mesh` | 1 | 1 | 9375 | 1 | 9375 |
| `web.wasm.webgpu.scene3d.stress_reused_mesh` | 1 | 1 | 11332 | 1 | 11332 |
| `web.wasm.webgpu.scene3d.stress_recreate_mesh` | 1 | 1 | 11957 | 1 | 11957 |
| `web.wasm.webgpu.mixed_text_image_effects` | 6 | 6 | 108536 | 6 | 108536 |
| `web.wasm.webgpu.layer_damage_effects` | 9 | 9 | 106532 | 9 | 106532 |
| `web.wasm.webgpu.clean_layer.clean_reuse` | 1 | 1 | 11415 | 1 | 11415 |
| `web.wasm.webgpu.command_family_matrix` | 1 | 1 | 28708 | 1 | 28708 |
| `web.wasm.webgpu.glyph_run.current` | 1 | 1 | 15249 | 1 | 15249 |
| `web.wasm.webgpu.neon_marker.current` | 1 | 1 | 10958 | 1 | 10958 |
| `web.wasm.webgpu.direct_surface.current` | 1 | 1 | 25541 | 1 | 25541 |

## Browser Trace

| Field | Value |
| --- | --- |
| Status | `collected` |
| Artifact | `../.tasks/artifacts/webgpu-clean-layer-current-after-trace.json` |
| Capture phase | `benchmark-report` |
| Timing source | `untraced-baseline-report` |
| Events | `52650` |
| GPU-related events | `30422` |
| WebGPU/Dawn events | `923` |
| ANGLE events | `1` |
| Renderer events | `406` |
| Duration us | `751089` |
| Category count | `25` |
| Sample categories | `WebCore,__metadata,benchmark,blink,blink.resource,blink.user_timing,blink_style,cc,devtools.timeline,disabled-by-default-blink.debug.layout,disabled-by-default-devtools.timeline,disabled-by-default-devtools.timeline.frame,disabled-by-default-display.framedisplayed,disabled-by-default-gpu.service,gpu,graphics.pipeline,input.scrolling,loading,navigation,raf_investigation,rail,shutdown,startup,toplevel.flow` |
| Benchmark trace mark status | `collected` |
| Benchmark trace mark events | `52` |
| Benchmark trace mark labels | `backdrop_batch_current,clean_layer_ab,command_family_matrix,direct_surface_ab,effect_uniform_ab,frame_loop,glyph_run_current,id_mask_current,layer_effects_matrix,mixed_matrix,neon_marker_ab,scene3d_ab,upload_current` |

### Browser Trace Benchmark Intervals

| Mark | Duration us | Events | GPU events | WebGPU/Dawn events | Renderer events | Event duration us |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| `frame_loop` | 15331 | 3025 | 1736 | 79 | 22 | 169302 |
| `id_mask_current` | 36587 | 3118 | 1888 | 50 | 37 | 310335 |
| `upload_current` | 22126 | 4427 | 3051 | 91 | 33 | 187984 |
| `effect_uniform_ab` | 102176 | 3401 | 2074 | 51 | 36 | 805461 |
| `backdrop_batch_current` | 14864 | 2160 | 1467 | 47 | 11 | 109792 |
| `scene3d_ab` | 38659 | 6636 | 5141 | 172 | 36 | 243904 |
| `mixed_matrix` | 20992 | 1847 | 1332 | 46 | 9 | 146820 |
| `layer_effects_matrix` | 21517 | 1856 | 1336 | 46 | 9 | 158122 |
| `clean_layer_ab` | 13258 | 1746 | 1313 | 46 | 9 | 91821 |
| `command_family_matrix` | 12883 | 1804 | 1344 | 46 | 9 | 78277 |
| `glyph_run_current` | 13503 | 1767 | 1337 | 46 | 9 | 126123 |
| `neon_marker_ab` | 12776 | 1887 | 1381 | 46 | 9 | 65151 |
| `direct_surface_ab` | 13350 | 1750 | 1309 | 46 | 9 | 52902 |

## Benchmark Marks

| Mark | Duration ms | Trace label | WASM before bytes | WASM after bytes | WASM growth bytes | JS heap before bytes | JS heap after bytes | JS heap growth bytes | JS heap sampled | GC exposed |
| --- | ---: | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| `frame_loop` | 16.300 | `yes` | 13041664 | 13041664 | 0 | 1251706 | 1297144 | 45438 | 1 | 1 |
| `id_mask_current` | 29.400 | `yes` | 13041664 | 13041664 | 0 | 1298047 | 1318826 | 20779 | 1 | 1 |
| `upload_current` | 21.700 | `yes` | 13041664 | 13041664 | 0 | 1330556 | 1351443 | 20887 | 1 | 1 |
| `effect_uniform_ab` | 102.700 | `yes` | 13041664 | 13041664 | 0 | 1389907 | 1404752 | 14845 | 1 | 1 |
| `backdrop_batch_current` | 17.000 | `yes` | 13041664 | 13041664 | 0 | 1405062 | 1411765 | 6703 | 1 | 1 |
| `scene3d_ab` | 36.600 | `yes` | 13041664 | 13041664 | 0 | 1411964 | 1444002 | 32038 | 1 | 1 |
| `mixed_matrix` | 21.100 | `yes` | 13041664 | 13041664 | 0 | 1444154 | 1452025 | 7871 | 1 | 1 |
| `layer_effects_matrix` | 21.500 | `yes` | 13041664 | 13041664 | 0 | 1452173 | 1459228 | 7055 | 1 | 1 |
| `clean_layer_ab` | 5.000 | `yes` | 13041664 | 13041664 | 0 | 1459360 | 1473131 | 13771 | 1 | 1 |
| `command_family_matrix` | 13.600 | `yes` | 13041664 | 13041664 | 0 | 1473275 | 1482201 | 8926 | 1 | 1 |
| `glyph_run_current` | 13.300 | `yes` | 13041664 | 13041664 | 0 | 1482331 | 1488900 | 6569 | 1 | 1 |
| `neon_marker_ab` | 12.700 | `yes` | 13041664 | 13041664 | 0 | 1489038 | 1495535 | 6497 | 1 | 1 |
| `direct_surface_ab` | 13.800 | `yes` | 13041664 | 13041664 | 0 | 1495679 | 1503644 | 7965 | 1 | 1 |

## Warm Resource Churn

| Check | Rows | Buffer Grows | Texture Creates | Bind Groups | Pipelines | Samplers | Meshes | Temp Allocs | Temp Bytes | Image Scratch Grows | CPU Scratch Grows | CPU Scratch Growth Bytes |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| `web.wasm.webgpu.warm_resource_churn.current_rows` | 15 checked / 2 excluded | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |

### Warm Resource Churn Rows

| Row | Buffer Grows | Texture Creates | Bind Groups | Pipelines | Samplers | Meshes | Temp Allocs | Temp Bytes | Image Scratch Grows | CPU Scratch Grows | CPU Scratch Growth Bytes |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| `web.wasm.webgpu.frame_loop` | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| `web.wasm.webgpu.id_mask_compositor.current` | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| `web.wasm.webgpu.glyph_atlas_upload.current_dirty` | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| `web.wasm.webgpu.image_upload.current_dirty` | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| `web.wasm.webgpu.effect_uniform.current_batched` | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| `web.wasm.webgpu.backdrop_batch.current_coalesced` | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| `web.wasm.webgpu.scene3d.reused_mesh` | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| `web.wasm.webgpu.scene3d.stress_reused_mesh` | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| `web.wasm.webgpu.mixed_text_image_effects` | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| `web.wasm.webgpu.layer_damage_effects` | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| `web.wasm.webgpu.clean_layer.clean_reuse` | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| `web.wasm.webgpu.command_family_matrix` | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| `web.wasm.webgpu.glyph_run.current` | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| `web.wasm.webgpu.neon_marker.current` | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| `web.wasm.webgpu.direct_surface.current` | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |

### Warm GPU Resource Family Churn

| Row | Draw Buffers | Image Textures | Image Bind Groups | Target Textures | Target Bind Groups | Scene3D Buffers | Scene3D Bind Groups | Effect Buffers | Effect Bind Groups | ID Mask Textures | ID Mask Buffers | ID Mask Bind Groups |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| `web.wasm.webgpu.warm_resource_churn.current_rows` | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| `web.wasm.webgpu.frame_loop` | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| `web.wasm.webgpu.id_mask_compositor.current` | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| `web.wasm.webgpu.glyph_atlas_upload.current_dirty` | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| `web.wasm.webgpu.image_upload.current_dirty` | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| `web.wasm.webgpu.effect_uniform.current_batched` | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| `web.wasm.webgpu.backdrop_batch.current_coalesced` | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| `web.wasm.webgpu.scene3d.reused_mesh` | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| `web.wasm.webgpu.scene3d.stress_reused_mesh` | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| `web.wasm.webgpu.mixed_text_image_effects` | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| `web.wasm.webgpu.layer_damage_effects` | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| `web.wasm.webgpu.clean_layer.clean_reuse` | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| `web.wasm.webgpu.command_family_matrix` | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| `web.wasm.webgpu.glyph_run.current` | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| `web.wasm.webgpu.neon_marker.current` | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| `web.wasm.webgpu.direct_surface.current` | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |

### Warm Scratch Family Churn

| Row | Draw Grows | Draw Bytes | Scene3D Grows | Scene3D Bytes | Effect Grows | Effect Bytes | ID Mask Grows | ID Mask Bytes | Image Upload Grows | Image Upload Bytes | Resource Table Grows | Resource Table Bytes |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| `web.wasm.webgpu.warm_resource_churn.current_rows` | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| `web.wasm.webgpu.frame_loop` | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| `web.wasm.webgpu.id_mask_compositor.current` | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| `web.wasm.webgpu.glyph_atlas_upload.current_dirty` | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| `web.wasm.webgpu.image_upload.current_dirty` | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| `web.wasm.webgpu.effect_uniform.current_batched` | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| `web.wasm.webgpu.backdrop_batch.current_coalesced` | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| `web.wasm.webgpu.scene3d.reused_mesh` | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| `web.wasm.webgpu.scene3d.stress_reused_mesh` | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| `web.wasm.webgpu.mixed_text_image_effects` | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| `web.wasm.webgpu.layer_damage_effects` | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| `web.wasm.webgpu.clean_layer.clean_reuse` | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| `web.wasm.webgpu.command_family_matrix` | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| `web.wasm.webgpu.glyph_run.current` | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| `web.wasm.webgpu.neon_marker.current` | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| `web.wasm.webgpu.direct_surface.current` | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |

## WASM Allocation Audit

| Check | Rows | Total Allocs | Total Bytes | Reallocs | Realloc Grow Bytes | Max Allocs/Frame | Max Bytes/Frame | Max Peak Frame Bytes | Budget Allocs/Frame | Budget Bytes/Frame |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| `web.wasm.webgpu.wasm_allocation_audit.current_rows` | 15 checked / 2 excluded | 3540 | 69780 | 0 | 0 | 6.556 | 129.222 | 174 | 7 | 144 |

### WASM Allocation Invariance

| Check | Status | Reference Row | Rows | Unique Signatures | Shared Allocs | Shared Bytes | Shared Reallocs | Shared Peak Frame Bytes |
| --- | --- | --- | ---: | ---: | ---: | ---: | ---: | ---: |
| `web.wasm.webgpu.wasm_allocation_invariance.current_rows` | `shared-submit-boundary-profile` | `web.wasm.webgpu.frame_loop` | 15 | 1 | 236 | 4652 | 0 | 174 |

### WASM Allocation Rows

| Row | Frames | Allocs | Bytes | Allocs/Frame | Bytes/Frame | Reallocs | Realloc Grow Bytes | Allocating Frames | Peak Frame Bytes |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| `web.wasm.webgpu.frame_loop` | 36 | 236 | 4652 | 6.556 | 129.222 | 0 | 0 | 36 | 174 |
| `web.wasm.webgpu.id_mask_compositor.current` | 36 | 236 | 4652 | 6.556 | 129.222 | 0 | 0 | 36 | 174 |
| `web.wasm.webgpu.glyph_atlas_upload.current_dirty` | 36 | 236 | 4652 | 6.556 | 129.222 | 0 | 0 | 36 | 174 |
| `web.wasm.webgpu.image_upload.current_dirty` | 36 | 236 | 4652 | 6.556 | 129.222 | 0 | 0 | 36 | 174 |
| `web.wasm.webgpu.effect_uniform.current_batched` | 36 | 236 | 4652 | 6.556 | 129.222 | 0 | 0 | 36 | 174 |
| `web.wasm.webgpu.backdrop_batch.current_coalesced` | 36 | 236 | 4652 | 6.556 | 129.222 | 0 | 0 | 36 | 174 |
| `web.wasm.webgpu.scene3d.reused_mesh` | 36 | 236 | 4652 | 6.556 | 129.222 | 0 | 0 | 36 | 174 |
| `web.wasm.webgpu.scene3d.stress_reused_mesh` | 36 | 236 | 4652 | 6.556 | 129.222 | 0 | 0 | 36 | 174 |
| `web.wasm.webgpu.mixed_text_image_effects` | 36 | 236 | 4652 | 6.556 | 129.222 | 0 | 0 | 36 | 174 |
| `web.wasm.webgpu.layer_damage_effects` | 36 | 236 | 4652 | 6.556 | 129.222 | 0 | 0 | 36 | 174 |
| `web.wasm.webgpu.clean_layer.clean_reuse` | 36 | 236 | 4652 | 6.556 | 129.222 | 0 | 0 | 36 | 174 |
| `web.wasm.webgpu.command_family_matrix` | 36 | 236 | 4652 | 6.556 | 129.222 | 0 | 0 | 36 | 174 |
| `web.wasm.webgpu.glyph_run.current` | 36 | 236 | 4652 | 6.556 | 129.222 | 0 | 0 | 36 | 174 |
| `web.wasm.webgpu.neon_marker.current` | 36 | 236 | 4652 | 6.556 | 129.222 | 0 | 0 | 36 | 174 |
| `web.wasm.webgpu.direct_surface.current` | 36 | 236 | 4652 | 6.556 | 129.222 | 0 | 0 | 36 | 174 |

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

## Frame Loop WASM Submit Allocation Stages

| Submit Stage | Allocs | Bytes |
| --- | ---: | ---: |
| `upload` | 0 | 0 |
| `surface` | 108 | 2304 |
| `encoder` | 0 | 0 |
| `render` | 0 | 0 |
| `timestamp` | 0 | 0 |
| `scratch_stats` | 0 | 0 |
| `finish_queue` | 108 | 2088 |
| `present` | 0 | 0 |
| `timestamp_map` | 20 | 260 |

## Backend Path Coverage

| Path | Status | Comparison | Rows | Counters |
| --- | --- | --- | ---: | ---: |
| `frame_loop` | `covered` | `coverage` | 1 | 6 |
| `id_mask_compositor` | `covered` | `coverage` | 1 | 8 |
| `clean_layer_reuse` | `covered` | `current` | 1 | 8 |
| `glyph_atlas_upload` | `covered` | `coverage` | 1 | 4 |
| `image_upload` | `covered` | `coverage` | 1 | 4 |
| `effect_uniform` | `covered` | `current` | 1 | 7 |
| `backdrop_batch` | `covered` | `current` | 1 | 5 |
| `scene3d_mesh_reuse` | `covered` | `current_vs_legacy` | 2 | 5 |
| `scene3d_stress_mesh_reuse` | `covered` | `current_vs_legacy` | 2 | 5 |
| `mixed_text_image_effects` | `covered` | `current` | 1 | 15 |
| `layer_damage_effects` | `covered` | `current` | 1 | 16 |
| `command_family_matrix` | `covered` | `current` | 1 | 10 |
| `glyph_run` | `covered` | `current` | 1 | 12 |
| `neon_marker` | `covered` | `current` | 1 | 8 |
| `direct_surface` | `covered` | `current` | 1 | 10 |

## ID-Mask Summary

| Case | Current p50 ms | Current Passes | Current Upload Bytes | Vertices | Vertex Bytes |
| --- | ---: | ---: | ---: | ---: | ---: |
| `web.wasm.webgpu.id_mask_compositor.current` | 0.075 | 12 | 1120 | 9600 | 307200 |

## Upload Summary

| Case | Glyph Current p50 ms | Glyph Current Texture Bytes | Glyph Current GPU ns | Atlas Dirty WxH | Image Current p50 ms | Image Current Texture Bytes | Image Current GPU ns | Image Dirty WxH |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| `web.wasm.webgpu.upload.current_dirty` | 0.042 | 16384 | 11125 | 64x64 | 0.033 | 16384 | 12333 | 64x64 |

## Effect Uniform Summary

| Row | Current p50 ms | Current GPU ns | Current Timestamp Passes | Current Writes | Current Bytes | Current Slots | Current Backdrops | Current Texture Copies | Current Passes | Expected Backdrops |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| `web.wasm.webgpu.effect_uniform.current_batched` | 0.217 | 1633034 | 51 | 1 | 16 | 48 | 48 | 48 | 51 | 48 |

## Backdrop Batch Summary

| Row | Current p50 ms | Current Writes | Current Slots | Current Backdrops | Current Texture Copies | Current Passes | Current Timestamp Passes | Expected Backdrops |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| `web.wasm.webgpu.backdrop_batch.current` | 0.025 | 1 | 12 | 12 | 1 | 4 | 4 | 12 |

## Scene3D Summary

| Comparison | Reused p50 ms | Recreate p50 ms | Recreate / Reused | Reused Mesh Creates | Recreate Mesh Creates | Reused Buffer Grows | Recreate Buffer Grows | Reused CPU Scratch Grows | Recreate CPU Scratch Grows | Reused CPU Scratch Growth Bytes | Recreate CPU Scratch Growth Bytes | Meshes | Instances |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| `web.wasm.webgpu.scene3d.reused_mesh_vs_recreate_mesh` | 0.017 | 0.025 | 1.500 | 0 | 2 | 0 | 4 | 0 | 0 | 0 | 0 | 2 | 2 |

## Scene3D Stress Summary

| Comparison | Reused p50 ms | Recreate p50 ms | Recreate / Reused | Reused Mesh Creates | Recreate Mesh Creates | Reused Buffer Grows | Recreate Buffer Grows | Reused CPU Scratch Grows | Recreate CPU Scratch Grows | Reused CPU Scratch Growth Bytes | Recreate CPU Scratch Growth Bytes | Meshes | Instances |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| `web.wasm.webgpu.scene3d.stress_reused_mesh_vs_stress_recreate_mesh` | 0.067 | 0.083 | 1.250 | 0 | 2 | 0 | 4 | 0 | 0 | 0 | 0 | 2 | 96 |

## Mixed Scene Summary

| Case | Current p50 ms | Current Items | Pipeline Binds | Bind Groups | Scissors | Writes | Texture Copies | Passes | Glyph Quads | Image Draws | Image Tiles | Layers | Damage Rects | Backdrops | Visual Effects |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| `web.wasm.webgpu.mixed_text_image_effects.current` | 0.067 | 115 | 9 | 8 | 5 | 1 | 2 | 6 | 96 | 97 | 96 | 1 | 2 | 1 | 1 |

## Layer Effects Summary

| Case | Current p50 ms | Current Items | Pipeline Binds | Bind Groups | Scissors | Writes | Texture Copies | Passes | Glyph Quads | Image Draws | Layers | Damage Rects | Backdrops | Visual Effects | Spinners |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| `web.wasm.webgpu.layer_damage_effects.current` | 0.067 | 89 | 8 | 15 | 6 | 1 | 1 | 9 | 72 | 65 | 3 | 3 | 4 | 1 | 1 |

## Clean Layer Summary

| Row | Clean p50 ms | Clean Items | Clean Hits | Clean Misses | Clean Skipped | Clean Layer Passes | Clean Render Passes | Clean GPU ns | Glyphs | Image Tiles | Expected Layers | Expected Hits |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| `web.wasm.webgpu.clean_layer.clean_reuse` | 0.017 | 3 | 1 | 0 | 149 | 0 | 1 | 11415 | 96 | 144 | 1 | 1 |

## Command Family Summary

| Row | Current p50 ms | Current Items | Current Pipeline Binds | Current Bind Groups | Current Scissors | Image Meshes | Nine Slices | SDF Glyphs | CameraBg Draws |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| `web.wasm.webgpu.command_family_matrix.current` | 0.108 | 649 | 3 | 2 | 1 | 64 | 64 | 288 | 0 |

## Glyph Run Summary

| Row | Current p50 ms | Runs | Glyphs/Run | Items | Glyph Quads | SDF Glyphs | Pipeline Binds | Bind Groups | Scissors |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| `web.wasm.webgpu.glyph_run.current` | 0.058 | 64 | 8 | 65 | 512 | 256 | 3 | 1 | 1 |

## Neon Marker Summary

| Row | Current p50 ms | Markers | Current Items | Expected Items | Current Solid Tris | Current Pipeline Binds | Current Bind Groups | Current Scissors |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| `web.wasm.webgpu.neon_marker.current` | 0.242 | 64 | 192 | 192 | 6912 | 1 | 0 | 1 |

## Direct Surface Summary

| Row | Current p50 ms | Current Items | Current Images | Current Render Passes | Current Draw Passes | Current Clear Passes | Current Present Passes | Current GPU ns | Current Timestamp Passes | Expected Items | Expected Images |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| `web.wasm.webgpu.direct_surface.current` | 0.058 | 385 | 384 | 1 | 1 | 0 | 0 | 25541 | 1 | 385 | 384 |

## Pixel Check

| Target | Viewport | Pixdiff | Max Err | MSE | Artifact |
| --- | --- | ---: | ---: | ---: | --- |
| `app` | 320x240 | 0 | 0 | 0.000 | `../.tasks/artifacts/webgpu-clean-layer-current-after.png` |

## Notes

- BrowserRenderer selected the WebGPU backend through async renderer initialization.
- This baseline was collected from a release wasm build served through the static web host.
- Production web visual startup is WebGPU-only; unsupported browsers return NOT SUPPORTED instead of drawing through Canvas2D.
- The WebGPU ID-mask current row is captured in the default browser report; the upload legacy rows and diagnostic export were retired after same-workload A/B proof.
- The WebGPU effect-uniform A/B rows draw the same backdrop scene while comparing one batched dynamic-uniform upload against one queue write per backdrop.
- The WebGPU backdrop-batch current row draws separated consecutive backdrops through the shared scene-copy pass after the slower default per-backdrop-copy row was retired.
- The WebGPU layer/damage/effects current row draws the nested layer, damage, image, glyph, backdrop, visual-effect, and spinner workload after the slower default legacy rebind/unbatched row was retired.
- The WebGPU clean-layer current row draws the retained image/glyph/clip layer through clean cache reuse after the slower default dirty rerender row was retired.
- The WebGPU command-family current row draws the generic ImageMesh, NineSlice, and SDF glyph workload after the slower default legacy rebind row was retired, while keeping web CameraBg work unavailable.
- The WebGPU glyph-run current row draws the atlas-backed A8 and SDF GlyphRun workload after the slower default legacy rebind row was retired.
- The WebGPU direct-surface current row draws the no-effect image workload on the one-pass no-scene-present route after the slower default forced scene-present row was retired.
- The standalone draw-item coalescing and draw-state cache diagnostic exports remain non-default diagnostics; the clip-state diagnostic export was retired after repeated startup/package A/B proof.
- Pass-family counters provide browser GPU-stage attribution when direct timestamp queries are unavailable.
- Warm current-path WebGPU rows are gated against post-warmup resource creation, buffer growth, mesh creation, image-upload temp allocation, and CPU/image scratch growth.
- WASM allocation counters measure Rust allocator activity inside each post-warmup benchmark frame loop and are reported separately from renderer-owned resource churn.
- Chrome startup tracing is captured from a duplicate benchmark-report run so GPU/browser-process activity is tied to the same report workload without perturbing persisted timing rows.
- Browser User Timing marks surround every benchmark family and are persisted to prove the traced report run exercised the expected workload phases.
- Browser startup and static package byte counts are persisted so non-default diagnostic export cleanup can be A/B tested against page-init and artifact-size evidence.
