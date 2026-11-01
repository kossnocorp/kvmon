#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/../../.."
cargo build --release -p kvmon-macos
app="target/release/kvmon.app"
mkdir -p "$app/Contents/MacOS"
cp target/release/kvmon-macos "$app/Contents/MacOS/kvmon"
cp pkgs/macos/Info.plist "$app/Contents/Info.plist"
codesign --force --sign - "$app"
echo "Built $app"
