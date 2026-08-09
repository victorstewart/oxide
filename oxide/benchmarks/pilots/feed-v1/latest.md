# feed-v1 physical-device evidence

- Status: `blocked`
- Decision: `blocked`
- Canonical JSON: `latest.json` (SHA-256 `9b7e891824308d10a99fb5eda38ba41ea99e1bdb276ee64685bd672e95216f68`)
- Fixture: `a1de9b4a914734fe21d21e9b6f8a9b61970f7e22e0fa4ef0103031e399881473` (717745 bytes)
- Device: `missing or inadmissible`
- Visual gate: `feed-v1:ssim8-luma>=0.96:tile48-rgb-mae<=18:rgba8:surface1170x2532:v1`
- Runs: 6 total, 0 primary
- Repository: `refs/heads/agent/oxide-final-clean-candidate` at commit `0e85e6bbd39b64e7079cc55a5e5237015e1aaae2` (tree `b11eb2c4732c0a8b198d65503d30b65c94ea96e3`)

## Frozen policy

- Callback quantiles: `one-based-nearest-rank` with `ceil(q * n), one-based`.
- Treatment aggregates: `median-of-nine-cluster-quantiles` over 9 clusters of 2 directions.
- Exact median interval: ranks 2-8 of 9, target 95.000%, achieved 96.093750%.
- Classification: slower above +5.0% at the lower bound; faster below +0.0% at the upper bound with lower aggregate p50/p95; non-inferior at or below +5.0% at the upper bound.
- Guardrails: missed ratio <= comparator + 0.5 percentage points and <= 2.0% absolute; hitch <= comparator + 2.0 ms/s and <= 10.0 ms/s absolute.

## Blockers

- cleanup/runtime/cap proof failed
- controller runtime proof population is missing, duplicated, or mislabeled
- primary travel equivalence for oxide forward has 0 pairs, expected 9
- primary travel equivalence for oxide reverse has 0 pairs, expected 9
- primary travel equivalence for uikit-optimized forward has 0 pairs, expected 9
- primary travel equivalence for uikit-optimized reverse has 0 pairs, expected 9
- read lock-state evidence /private/tmp/oxide-feed-v1-0e85e6b-arm64/raw/lock-before-primary.json: No such file or directory (os error 2)
- run population is 6, expected 60
- run tuple population is missing, duplicated, or out of scope
- runner blocked at smoke-admission: the frozen smoke reducer gate blocked
- session 0 pair 0 treatment order is not balanced
- session 0 pair 1 treatment order is not balanced
- session 0 pair 2 treatment order is not balanced
- session 1 pair 0 treatment order is not balanced
- session 1 pair 1 treatment order is not balanced
- session 1 pair 2 treatment order is not balanced
- session 2 pair 0 treatment order is not balanced
- session 2 pair 1 treatment order is not balanced
- session 2 pair 2 treatment order is not balanced
- visual gate failed for oxide bottom admission: SSIM 0.875600, worst tile MAE 38.421875
- visual gate failed for oxide bottom repeat: SSIM 0.875600, worst tile MAE 38.421875
- visual gate failed for oxide top admission: SSIM 0.874027, worst tile MAE 38.421875
- visual gate failed for oxide top repeat: SSIM 0.874027, worst tile MAE 38.421875

## Visual admission

| State | Treatment | Capture | SSIM | Worst 48x48 RGB MAE | Full-surface RGB MAE | Pass |
|---|---|---|---:|---:|---:|---:|
| top | uikit-idiomatic | admission | 1.000000 | 0.000 | 0.000 | yes |
| top | uikit-idiomatic | repeat | 1.000000 | 0.000 | 0.000 | yes |
| top | uikit-optimized | admission | 1.000000 | 0.000 | 0.000 | yes |
| top | uikit-optimized | repeat | 1.000000 | 0.000 | 0.000 | yes |
| top | oxide | admission | 0.874027 | 38.422 | 6.217 | no |
| top | oxide | repeat | 0.874027 | 38.422 | 6.217 | no |
| bottom | uikit-idiomatic | admission | 1.000000 | 0.000 | 0.000 | yes |
| bottom | uikit-idiomatic | repeat | 1.000000 | 0.000 | 0.000 | yes |
| bottom | uikit-optimized | admission | 1.000000 | 0.000 | 0.000 | yes |
| bottom | uikit-optimized | repeat | 1.000000 | 0.000 | 0.000 | yes |
| bottom | oxide | admission | 0.875600 | 38.422 | 6.033 | no |
| bottom | oxide | repeat | 0.875600 | 38.422 | 6.033 | no |

## Evidence and limitations

- Visual-gate specification SHA-256: `097ad822430981f6b6a144e493df0c51f47cf89479ba3d85cf206f77b97c180e`
- Visual-gate source SHA-256: `3caf4cf6fd30cd78c674933811fc7e648d81c8d851ec77ac822e32c941b7c3db`
- Evidence manifest SHA-256: `0cd05ba067c3e09c174bd95cf761ff65fbd0a0b086bfcdd800cf05f177fa5ecf`
- Raw evidence inventory: 75 sorted files with byte counts and SHA-256 values in canonical JSON.
- Primary callback rows: 0 with raw sample arrays in canonical JSON only.
- Retained reducer input: 6923192 bytes

## Cleanup

- Cleanup proof admitted: `false`
- Runner observations: test succeeded `false`, apps uninstalled `true`, controller uninstalled `true`, controller process absent `true`, source preserved `true`, build removed `true`, result bundle removed `false`.
- Missing metrics: presented-frame pacing, visible-frame pacing, input-to-visible latency, main-thread CPU, process CPU, resident memory, symmetric direct GPU time, direct energy

These figures are display-link callback pacing only. They make no presented-frame, visible-frame, or photon-latency claim.
