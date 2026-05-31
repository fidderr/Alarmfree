#!/bin/bash
# AlarmFree — workspace build command.
# Use this when mobile-sentinel is a local workspace member at ../../crates/mobile-sentinel.
# Builds the APK: Rust .so → wire Kotlin + copy assets/icon → Gradle.
#
# Usage:
#   ./build-workspace.sh                    # build only
#   ./build-workspace.sh install            # build + install on connected device
#   ./build-workspace.sh install bb741d88   # build + install on specific device

set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
WORKSPACE_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
TARGET="aarch64-linux-android"
ADB="${LOCALAPPDATA}/Android/Sdk/platform-tools/adb.exe"

cd "$SCRIPT_DIR"

echo "=== [1/2] Building Rust .so (dx build) ==="
dx build --platform android --target "$TARGET" --package alarmfree

echo ""
echo "=== [2/2] Wiring mobile-sentinel + assets + icon + APK ==="
cd "$WORKSPACE_ROOT"
cargo run -p mobile-sentinel --bin build_sentinel -- --app alarmfree

APK="$WORKSPACE_ROOT/target/dx/alarmfree/debug/android/app/app/build/outputs/apk/debug/app-debug.apk"
echo ""
echo "=== APK ready: $APK ==="

# Install if requested
if [ "${1:-}" = "install" ]; then
    DEVICE="${2:-}"
    if [ -n "$DEVICE" ]; then
        echo "=== Installing on $DEVICE ==="
        "$ADB" -s "$DEVICE" install -r "$APK"
    else
        echo "=== Installing on default device ==="
        "$ADB" install -r "$APK"
    fi
fi
