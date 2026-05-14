# Oxide WebAssembly Browser Baseline

Date: 2026-05-14

Target: Chrome via Codex in-app browser at `http://127.0.0.1:8787/?fresh=webgpu-only-public-final`

Status: browser-baseline. This is the browser-specific WebGPU/WebAssembly baseline for the current web backend. It is not an official device parity report.

## Smoke

| Check | Result |
| --- | --- |
| Platform | `caps=40;online=true;location=not-determined;webview=ok` |
| WebGPU probe | `webgpu=device-ok` |
| Renderer backend | `webgpu` |
| Renderer | `draws=26` |

## Cases

| Case | Variant | Samples | Frames/Sample | Frames | p50 ms | p95 ms | p99 ms | Peak ms | Avg ms | Draws |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| `web.wasm.webgpu.frame_loop` | webgpu | 8 | 30 | 240 | 0.110 | 0.147 | 0.147 | 0.147 | 0.114 | 26 |

## Pixel Check

| Viewport | Alpha Samples | Non-Background Samples | Colored Samples | Artifact |
| --- | ---: | ---: | ---: | --- |
| 1280x720 | 25680 | 3872 | 273 | `oxide/host/web-app/www/oxide-wasm-browser-check.png` |

## Notes

- `BrowserRenderer` selected the WebGPU backend through async renderer initialization.
- This baseline was collected from a release wasm build.
- Production web visual startup is WebGPU-only; unsupported browsers return NOT SUPPORTED instead of drawing through Canvas2D.
- Browser permission prompts were not accepted during automation; location remained `not-determined`.
