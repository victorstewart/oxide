#![allow(unexpected_cfgs)]

#[cfg(c48_baseline)]
include!("c48_bitmap_options_probe/baseline.rs");

#[cfg(not(c48_baseline))]
include!("c48_bitmap_options_probe/candidate.rs");
