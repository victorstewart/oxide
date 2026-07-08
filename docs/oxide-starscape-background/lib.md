# oxide-starscape-background::lib

## Intention And Purpose

`oxide-starscape-background` provides a reusable renderer-agnostic starscape background. It exists so apps can share the same Rust-driven base fill, atmosphere wash, dust, and star rendering across native and web backends instead of duplicating scene-specific background code.

## Relation To The Rest Of The Code

Callers construct `StarscapeBackgroundConfig`, then ask `StarscapeBackground::draw` to emit generic `oxide_renderer_api::RenderEncoder` commands. Backends such as Metal and web lower those generic solid and rounded-rect commands into platform drawing.

Call graph:

- App scene config -> `StarscapeBackgroundConfig`
- App frame draw -> `StarscapeBackground::draw`
- `StarscapeBackground::draw` -> base fill -> atmosphere strips/lobes -> dust -> stars
- `RenderEncoder` backend -> Metal/Web/native draw commands

## Entry Points

- `oxide_starscape_background::StarscapeBackgroundConfig::new(base_color, stars, atmosphere) -> Self`: bundles base, star, and optional atmosphere configuration.
- `oxide_starscape_background::StarscapeStarConfig::nametag(pink) -> Self`: returns the reusable Nametag star palette defaults.
- `oxide_starscape_background::StarscapeAtmosphereConfig::nametag_top_simple(pink, evening) -> Self`: returns the subtle top vertical wash preset.
- `oxide_starscape_background::StarscapeAtmosphereConfig::nametag_top_complex(pink, evening) -> Self`: returns the subtle top wash plus broad low-alpha lobes.
- `oxide_starscape_background::StarscapeBackground::new(seed) -> Self`: creates deterministic star and dust placement.
- `oxide_starscape_background::StarscapeBackground::draw(encoder, rect, config)`: renders base, atmosphere, dust, and stars in order.
- `oxide_starscape_background::color_from_srgb_u8(r, g, b, a) -> Color`: converts app-supplied sRGB byte colors into renderer color floats.
- `oxide_starscape_background::generated_background_seed(load_timestamp, salt) -> u64`: derives deterministic per-load background seeds.

## Logic Narrative

`StarscapeBackground::new` precomputes normalized star and dust positions from a deterministic RNG. During draw, the crate fills the base rectangle first, draws the optional atmosphere before particles, then draws dust and stars over it so stars remain crisp.

The atmosphere uses horizontal strips because the current shared `RenderEncoder::draw_solid` path accepts one color per draw. Strip rectangles are exactly contiguous: each row's lower edge matches the next row's upper edge. This avoids alpha-amplifying overlap lines while preserving backend portability. The origin edge holds the caller-supplied pink hue before it eases toward evening, so brand colors do not immediately muddy over a black base. `ComplexSoftMesh` starts from the same vertical wash and adds a few very broad, low-alpha ellipse fans partially offscreen to break perfect linearity without changing the effect into an illustration.

## Preconditions And Postconditions

The draw rectangle must have positive width and height; otherwise drawing is skipped. Atmosphere drawing is skipped when `max_alpha <= 0.0` or `coverage_fraction <= 0.0`. After a successful draw call, the renderer receives base fill commands before atmosphere commands, and star/dust rounded-rect commands after atmosphere commands.

## Edge Cases And Failure Modes

Rows are clamped to the crate's supported range. Coverage is clamped to the viewport height. Very low-alpha trailing strips are omitted because they are visually transparent. Top and bottom origins use mirrored y coordinates so a bottom atmosphere can reproduce the same gradient geometry inverted vertically.

## Concurrency And Memory Behavior

`StarscapeBackground` owns immutable `Vec` storage for stars and dust after construction. The draw path borrows this storage and emits renderer commands through the caller-owned `RenderEncoder`. Atmosphere strip generation currently allocates one small `Vec` per draw, bounded by the maximum row count.

## Performance Notes

Draw complexity is `O(rows + dust + visible_stars)`. The complex atmosphere adds three bounded ellipse fans. No image assets, CSS layers, or backend-specific shader paths are required. The strip-contiguity invariant avoids overdraw at every row boundary, which is both visually cleaner and cheaper than overlapping rows. Dust particles are deliberately tiny and low-alpha so they read as sparse texture instead of visible round haze marks over the atmosphere.

## Feature Flags And Cfgs

The crate currently exposes one portable path without feature-flagged behavior.

## Testing And Benchmarks

`oxide/crates/starscape-background/tests/atmosphere.rs` verifies alpha falloff, top/bottom mirroring, disabled-atmosphere invariance for star/dust draws, exact Nametag color inputs, pink-hue retention at the origin edge, tiny dust bounds, and non-overlapping atmosphere strip boundaries.

## Examples

```rust
let pink = color_from_srgb_u8(255, 57, 117, 1.0);
let evening = color_from_srgb_u8(35, 37, 44, 1.0);
let background = StarscapeBackground::new(generated_background_seed(load_timestamp, "foundation"));
let config = StarscapeBackgroundConfig::new(
   Color::rgba(0.0, 0.0, 0.0, 1.0),
   StarscapeStarConfig::nametag(pink),
   Some(StarscapeAtmosphereConfig::nametag_top_complex(pink, evening)),
);
background.draw(encoder, rect, &config);
```

## Changelog

- Added the reusable starscape background crate with atmosphere presets and Nametag integration support.
- Adjusted atmosphere strip geometry so rows are contiguous rather than overlapping, preventing visible alpha bands in Canvas/Web and native backends.
- Retuned the Nametag atmosphere presets to keep the supplied pink hue at the origin edge and reduced dust size/alpha so the atmosphere does not reveal circular haze artifacts.
