# iOS Host App

This directory will contain the Xcode host application and `main.m` per the spec. The Rust static library crate lives at `oxide/host/ios-app/oxide-host-ios`.

The iOS host supports arm64 only. Device builds use `aarch64-apple-ios` and
Apple-silicon Simulator builds use `aarch64-apple-ios-sim`; the generated Xcode
project pins every Simulator configuration to `arm64`, including generic
Release destinations. Intel-Mac Simulator slices are intentionally unsupported.
