# AGENTS.md — Repo rules (Rust)

## Activation
This is a Rust workspace. Load exactly this profile:
- Read `~/.codex/rules/rust/GPT5_RULES.md` and follow it verbatim.
- Do not load any other language profile.

## Local overrides (optional, narrow)
- Use only when a task explicitly opts into a subfolder‑specific behavior.
- Document any local override at the top of the PR description.

## Formatting
- Source of truth: the repo‑root `rustfmt.toml`. Always use this file to format Rust sources.
- Always read repo‑level `AGENTS.md` and `rustfmt.toml` before doing any work, even when operating only in a subfolder.
- Apply formatting before proposing patches. Use: `cargo fmt --all`.
- If a subfolder contains another `rustfmt.toml`, prefer the repo‑root rules unless a task explicitly opts into the subfolder override.
- Use the `rustfmt` version pinned by the repo (e.g., via `rust-toolchain.toml` or CI). If your local version differs, do not reformat. Open an issue to align versions.
- When `rustfmt.toml` changes, perform a repo‑wide reformat in a single standalone commit titled `style: reformat for new rustfmt.toml`.
- Style is enforced by `rustfmt`. It targets 3‑space indentation and Allman braces per this repo’s policy. If `rustfmt` cannot encode a choice, use the Style canon below after formatting.

### Style canon (spacing where `rustfmt` is silent)
When `rustfmt` does not enforce a choice, match the spacing shown here.

```rust
// Canonical spacing example.

use std::{fmt::Display, ops::Add};

pub struct Canon<T>
{
   data: Vec<T>
}

impl<T> Canon<T>
where
   T: Display + Clone + From<i32> + Copy + Add<Output = T>
{
   pub fn add(a: i32, b: i32) -> i32
   {
      a + b
   }

   pub fn demo(&mut self, x: i32)
   {
      let r: &Vec<T> = &self.data;
      let first: T = *r.get(0).unwrap_or(&T::from(0));

      self.data.push(T::from(x + 1));

      if x > 0
      {
         for i in 0..x
         {
            self.log(i);
         }
      }

      let y: i32 = if x > 1 { x + (first as i32) } else { 0 };

      println!("y={}", y);
   }

   fn log(&self, i: i32)
   {
      println!("{}", i);
   }
}

pub enum E
{
   A,
   B(i32)
}

pub fn match_demo(e: E) -> i32
{
   match e
   {
      E::A => 0,
      E::B(v) => v * v
   }
}

pub fn map_demo<I, F>(iter: I, f: F) -> Vec<i32>
where
   I: IntoIterator<Item = i32>,
   F: Fn(i32) -> i32
{
   iter.into_iter().map(|v| f(v) + 1).collect()
}