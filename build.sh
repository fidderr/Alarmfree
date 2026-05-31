#!/bin/bash
# AlarmFree — single build command (standalone project).
# Use this when mobile-sentinel is a published crates.io dependency.
# Builds the APK: Rust .so → wire Kotlin + copy assets/icon → Gradle.
#
# Usage:
#   ./build.sh                    # build only
#   ./build.sh install            # build + install on connected device
#   ./build.sh install bb741d88   # build + install on specific device

set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_ROOT="$SCRIPT_DIR"
TARGET="aarch64-linux-android"
ADB="${LOCALAPPDATA}/Android/Sdk/platform-tools/adb.exe"

cd "$SCRIPT_DIR"

echo "=== [1/2] Building Rust .so (dx build) ==="
dx build --platform android --target "$TARGET" --package alarmfree

echo ""
echo "=== [2/2] Wiring mobile-sentinel + assets + icon + APK ==="
# build_sentinel ships as a binary in the published mobile-sentinel crate.
# Ensure the pinned version is installed (upgrades/installs as needed).
SENTINEL_VERSION="0.1.1"
if ! cargo install --list | grep -q "mobile-sentinel v${SENTINEL_VERSION}"; then
    echo "Installing build_sentinel ${SENTINEL_VERSION} from crates.io..."
    cargo install mobile-sentinel --version "$SENTINEL_VERSION" --force
fi
build_sentinel

APK="$PROJECT_ROOT/target/dx/alarmfree/debug/android/app/app/build/outputs/apk/debug/app-debug.apk"
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
