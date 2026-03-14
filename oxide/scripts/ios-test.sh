#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
PROJECT="$ROOT/host/ios-app/App/OxideHost.xcodeproj"
export PATH="$HOME/.cargo/bin:$PATH"
SCHEME="OxideHost"
DESTINATION=${DESTINATION:-"platform=iOS Simulator,name=iPhone 16,OS=18.6"}

pushd "$ROOT" >/dev/null
TOOLCHAIN="${TOOLCHAIN:-1.86.0}"
cargo +"${TOOLCHAIN}" run --package xtask -- ios prepare
cargo +"${TOOLCHAIN}" build --package oxide-host-ios --release --target aarch64-apple-ios-sim
cargo +"${TOOLCHAIN}" build --package oxide-host-ios --release --target aarch64-apple-ios
popd >/dev/null

xcodebuild -project "$PROJECT" -scheme "$SCHEME" -destination "$DESTINATION" test
