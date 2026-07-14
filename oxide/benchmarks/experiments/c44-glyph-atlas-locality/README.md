# C44 — Localized glyph atlas invalidation

Status: accepted after rejecting the first generalized hit path.

C44 replaces the production single atlas revision with four hard-budgeted 1024-square pages. Each page has a stable CPU identity, an internal generation, monotonic slot generations, deterministic least-recently-used recycling, current-frame pinning, resident/occupied/fragmentation accounting, and a distinct renderer image handle. Slots are append-only within a page. Recycling clears only one unpinned page and replaces only that page's GPU handle, so retained chunks on other pages keep their resource identity.

Incremental publication uses disjoint new-slot strips and never calls the normal image-update path that invalidates prepared dependencies. Each page retains at most 16 strips; exceeding that bound advances the page generation and performs one page-local full republish. A pristine page may merge its first strips because no prior glyph slot can be sampled. First use creates one populated texture rather than eagerly creating a blank texture and immediately updating or replacing it. Metal and WebGPU expose an explicit append-only A8 upload path; normal image updates retain their existing invalidation semantics. Memory warnings release every live page through the uploader, while device-loss purge drops all CPU/GPU page mappings for clean recreation.

The accepted Metal locality comparison uses one exact release binary for both modes, 20 fixed-seed balanced fresh-process pairs, 100 measured frames and three warmups per side. The control stores both retained glyph chunks in one equal-total-byte atlas and updates one slot; the candidate stores them on two pages and recycles one page. Both modes retain two draws and 8,192 resident atlas bytes.

| Forced recycle metric | Global atlas control | Paged atlas | Change |
| --- | ---: | ---: | ---: |
| Invalidated/prepared chunks | 2 | 1 | -50% |
| Prepared cache hits | 0 | 1 | +1 |
| Atlas upload | 4 B | 4,096 B | page-local replacement tradeoff |
| Resident atlas bytes | 8,192 B | 8,192 B | unchanged |
| Draws | 2 | 2 | unchanged |
| Whole-frame p50 | 1.541583 ms | 1.539417 ms | -0.14% |
| Whole-frame p95 | 1.552208 ms | 1.550210 ms | -0.13% |
| Whole-frame p99 | 1.565501 ms | 1.565714 ms | +0.01% |
| Peak | 1.671916 ms | 1.634625 ms | -2.23% |

The paired median speedup is 0.1534% (95% CI 0.1135%..0.1796%, 19/20 wins). The gain is intentionally small because the workload contains only two tiny chunks; its purpose is to prove exact cache locality and tail neutrality under the worst recycle operation.

Two production guardrails compare the C43 parent with the multi-page candidate. The 200-new-label Metal path improved paired median 0.5994% (95% CI 0.3526%..0.7234%, 16/20 wins), while p50/p95/p99/peak moved from 2.496896/2.986006/3.155013/3.309833 ms to 2.484542/2.958170/3.117715/3.271584 ms. Both sides created one atlas, uploaded 1,048,576 bytes, encoded 95,200 geometry bytes, and issued 200 draws. The warm 1,000-label CPU path improved paired median 34.0004% (95% CI 33.4266%..34.4350%, 15/15 wins); both sides reported 1,000 layout hits, 15,000 glyph hits, and zero shaping, rasterization, eviction, or upload work.

The first generalized page-hit implementation was rejected: it performed two hash lookups for every resident glyph and regressed the warm CPU median 12.6451%, losing all 15 pairs and every percentile gate. The accepted hit path performs one lookup and carries the resolved entry directly into page/run routing. The first 20-pair locality population was also rejected without filtering: its median improved 0.1675%, but one candidate process produced a 3.1025 ms peak against 1.5952 ms control. The repeated exact population above passed every raw percentile and peak gate; both populations remain under the ignored `raw/` directory.

Exact tests prove that recycling one page preserves the other page's retained handle and prepared Metal chunk, page and byte budgets never grow, visible pages cannot be recycled mid-frame, reset advances the surviving generation and purges extra pages, immediate-mode runs are repatched after page replacement, append-only WebGPU uploads skip dependency invalidation, and memory-warning cleanup releases all GPU pages. Existing fallback, RTL, CJK, SDF/scale, IME, grapheme, and snapshot paths continue through the same paged production API. Aggregate workspace baseline promotion remains deferred to C62.
