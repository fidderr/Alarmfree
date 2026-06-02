#!/bin/bash
# AlarmFree — single build command (standalone project).
# Use this when mobile-sentinel is a published crates.io dependency.
# Builds the APK (or AAB for Play): Rust .so → wire Kotlin + copy assets/icon → Gradle.
#
# Default behavior uses the published `mobile-sentinel` crate from crates.io.
# This exercises the exact same flow that consumers of the crate will use
# when they depend on it in their own Cargo.toml (the "real" validated path).
#
# For local development inside this workspace (when you want to test changes
# to the mobile-sentinel source at the same time), prefix with the `workspace`
# flag:
#
#   ./build.sh workspace ...     # use local sibling mobile-sentinel/ source instead of crates.io
#
# Usage (default = published crate):
#   ./build.sh                    # build debug APK only
#   ./build.sh install            # build debug + install on connected device
#   ./build.sh install bb741d88   # build debug + install on specific device
#   ./build.sh release            # build release APK (unsigned by default)
#   ./build.sh aab                # build release AAB (recommended for Google Play)
#
#   Note: "install" is only supported for debug builds. Release/AAB builds are
#         unsigned by default and will not install via adb.
#
# Usage (local workspace source):
#   ./build.sh workspace
#   ./build.sh workspace install
#   ./build.sh workspace aab
#   ./build.sh workspace release
#   (install after release/aab is not supported for the same reason as above)
#
# Package name is controlled by Dioxus.toml under [bundle] identifier (or [android] identifier).
# Current: com.fidderr.alarmfree

set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_ROOT="$SCRIPT_DIR"
TARGET="aarch64-linux-android"
ADB="${LOCALAPPDATA}/Android/Sdk/platform-tools/adb.exe"

cd "$SCRIPT_DIR"

# Workspace flag: if present as the first argument, use the local
# mobile-sentinel/ sibling via `cargo run` instead of the published crate.
# This replaces the old separate build-workspace.sh script.
USE_WORKSPACE=false
if [ "${1:-}" = "workspace" ] || [ "${1:-}" = "--workspace" ]; then
  USE_WORKSPACE=true
  shift
fi

# Determine mode from (remaining) first argument. Supported: (nothing)=debug, release, aab.
# "install" as first arg implies debug + install (backward compat).
MODE="debug"
if [ "${1:-}" = "release" ]; then
    MODE="release"
elif [ "${1:-}" = "aab" ]; then
    MODE="aab"
fi

DX_RELEASE=""
SENTINEL_FLAGS=""
ARTIFACT_NAME="APK"
OUT_REL="apk/debug/app-debug.apk"

if [ "$MODE" = "release" ]; then
    DX_RELEASE="--release"
    SENTINEL_FLAGS="--release"
    ARTIFACT_NAME="release APK"
    OUT_REL="apk/release/app-release-unsigned.apk"
elif [ "$MODE" = "aab" ]; then
    DX_RELEASE="--release"
    SENTINEL_FLAGS="--release --aab"
    ARTIFACT_NAME="AAB"
    OUT_REL="bundle/release/app-release.aab"
fi

echo "=== [1/2] Building Rust .so (dx build) ==="
dx build --platform android --target "$TARGET" --package alarmfree $DX_RELEASE

echo ""
echo "=== [2/2] Wiring mobile-sentinel + assets + icon + $ARTIFACT_NAME ==="

if $USE_WORKSPACE; then
  # Local development mode: run directly from the sibling source tree.
  # No crates.io install.
  WORKSPACE_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
  if [ ! -f "$WORKSPACE_ROOT/mobile-sentinel/Cargo.toml" ]; then
    echo "ERROR: 'workspace' flag given, but no mobile-sentinel/ directory found as a sibling."
    echo "       This mode is only intended when developing inside the combined workspace"
    echo "       (alarmfree/ next to mobile-sentinel/)."
    exit 1
  fi
  echo "Using LOCAL workspace mobile-sentinel (via cargo run --bin build_sentinel)"
  # Always pass --app so the bin reliably finds the dx output under this layout.
  # shellcheck disable=SC2086
  cargo run --manifest-path "$WORKSPACE_ROOT/mobile-sentinel/Cargo.toml" \
            --bin build_sentinel -- --app alarmfree $SENTINEL_FLAGS
else
  # Default: the real published-crate flow that end users of mobile-sentinel will experience.
  # build_sentinel ships as a binary in the published mobile-sentinel crate.
  # Ensure a sufficiently recent version is installed for release/aab support.
  # NOTE: --release / --aab support landed in mobile-sentinel 0.1.3.
  SENTINEL_VERSION="0.1.3"
  if ! cargo install --list | grep -q "mobile-sentinel v${SENTINEL_VERSION}"; then
      echo "Installing build_sentinel ${SENTINEL_VERSION} from crates.io (required for release/aab)..."
      if ! cargo install mobile-sentinel --version "$SENTINEL_VERSION" --force; then
          echo "WARNING: Could not install exact v${SENTINEL_VERSION} from crates.io yet."
          echo "         Publish the updated mobile-sentinel crate first, or pre-install with:"
          echo "         cargo install --path ../mobile-sentinel --force"
          echo "         Then re-run this script. Proceeding with whatever build_sentinel is in PATH..."
      fi
  fi

  # shellcheck disable=SC2086
  build_sentinel $SENTINEL_FLAGS
fi

if [ "$MODE" = "debug" ]; then
    OUT="$PROJECT_ROOT/target/dx/alarmfree/debug/android/app/app/build/outputs/apk/debug/app-debug.apk"
else
    OUT="$PROJECT_ROOT/target/dx/alarmfree/release/android/app/app/build/outputs/$OUT_REL"
fi

# Note: the build_sentinel binary (whether invoked via cargo run in workspace mode
# or the installed binary) already prints the final artifact path. We avoid printing
# it a second time here to prevent duplicate "ready" messages.

# Install step is only supported for debug builds.
# Release and AAB artifacts are unsigned by default and will fail to install.
if [ "${1:-}" = "install" ] || [ "${2:-}" = "install" ]; then
    if [ "$MODE" != "debug" ]; then
        echo ""
        echo "Install was requested, but this is a $MODE build."
        echo "Release/AAB artifacts are unsigned by default (INSTALL_PARSE_FAILED_NO_CERTIFICATES)."
        echo "Sign the APK/AAB first (or build a debug version) if you want to sideload via adb."
        echo "You can still run adb manually against a signed artifact if you have one."
    else
        DEVICE=""
        if [ "${1:-}" = "install" ]; then
            DEVICE="${2:-}"
        else
            DEVICE="${3:-}"
        fi
        if [ -n "$DEVICE" ]; then
            echo "=== Installing on $DEVICE ==="
            "$ADB" -s "$DEVICE" install -r "$OUT"
        else
            echo "=== Installing on default device ==="
            "$ADB" install -r "$OUT"
        fi
    fi
fi
