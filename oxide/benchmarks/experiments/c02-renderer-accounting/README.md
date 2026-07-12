# C02 renderer accounting evidence

This directory is the stable local evidence root for C02. Large browser reports, paired-process artifacts, packages, and environment records live in the ignored `raw/` child so evidence acquisition does not change the candidate tree being measured.

Accounting contract:

- Metal reports logical bytes and API-exposed allocated bytes. Texture and buffer identities use separate deduplication sets, and each resource has one category owner.
- WebGPU reports logical bytes and keeps `gpu_allocated_bytes_available=false` plus `gpu_allocated_total_bytes=0`; wgpu does not expose driver allocation sizes.
- Resident image/layer/mesh/ID-mask table walks are sampled every 60 frames. Ordinary frames copy the latest snapshot in constant time, and explicit benchmark controls can disable the complete accounting snapshot path or only the resident scan.
- All byte and work accumulation saturates instead of wrapping.

Required evidence:

- `browser-smoke.json`: live Chrome/WebGPU rows containing the new report fields and reconciled logical resource totals.
- `paired-overhead/`: fresh-process parent/candidate distributions for unchanged C01 workloads, including the instrumentation overhead control.
- `environment.json`: exact source identities, hardware, OS, GPU, toolchain, browser, power/thermal state, build flags, and commands.

The first reduced browser diagnostic was rejected because two ID-mask measured frames were insufficient for the asynchronous timestamp-settle contract; it is retained only in the C02 work log, not as accepted evidence. The accepted rerun uses the established eight-sample timestamp cardinality.

Official workspace, web, and physical-device baseline promotion remains deferred to C62.
