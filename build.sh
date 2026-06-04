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
# Usage (default = published crate — the "real" flow):
#   ./build.sh                    # build debug APK only
#   ./build.sh install            # build debug + install on connected device
#   ./build.sh install bb741d88   # build debug + install on specific device
#   ./build.sh release            # build release APK (unsigned by default)
#   ./build.sh aab                # build release AAB (then run ./sign-aab.sh)
#
#   For internal testing on Play Console:
#     ./build.sh aab
#     ./sign-aab.sh
#     # then copy the file from the release/ folder
#
#   Note: "install" is only supported for debug builds. Release/AAB builds are
#         unsigned by default and will not install via adb.
#
# Usage (local workspace source):
#   ./build.sh workspace
#   ./build.sh workspace install
#   ./build.sh workspace aab
#   ./build.sh workspace release
#   (same signing flow: after aab run ./sign-aab.sh)
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

# Patch the generated Gradle for release/AAB (Play Store) builds.
# - Force a good, monotonically increasing versionCode (dx currently hardcodes low values like 1
#   even when you bump the Cargo version; Play requires unique + increasing codes).
# - Include full native debug symbols for the Rust .so (addresses the "upload debug symbols" note).
#
# Version code derivation (unless you pin an explicit one):
#   We read the [package] version from Cargo.toml and use the standard scheme:
#     versionCode = major*10000 + minor*100 + patch
#   Examples: 0.1.0 → 100, 0.1.1 → 101, 0.2.0 → 200, 1.0.0 → 10000
#   This makes every semver bump produce a new valid higher code for Play.
#
# You can also force a specific (higher) code by adding to Dioxus.toml:
#   [android]
#   version_code = 12346
#   (Use this + rebuild if you hit the "can't rollout... upgrade" error because a prior build used e.g. 12345. Set it > the max versionCode you've ever uploaded for this package.)
if [ "$MODE" = "release" ] || [ "$MODE" = "aab" ]; then
  GRADLE_FILE="target/dx/alarmfree/release/android/app/app/build.gradle.kts"
  if [ -f "$GRADLE_FILE" ]; then
    VC=""
    # Prefer explicit pin from Dioxus.toml if the user added version_code = NNN (ignore comments)
    if [ -f "Dioxus.toml" ]; then
      VC=$(grep -i 'version_code' Dioxus.toml | grep -v '^[[:space:]]*#' | head -1 | sed -E 's/.*= *([0-9]+).*/\1/' 2>/dev/null || true)
    fi
    if [ -z "$VC" ]; then
      # Derive from Cargo.toml semver (the source of truth for versionName too)
      VER=$(grep -E '^version *=' Cargo.toml | head -1 | sed -E 's/.*"([0-9]+)\.([0-9]+)\.([0-9]+)".*/\1 \2 \3/' || echo "0 1 0")
      read -r MA MI PA <<<"$VER"
      VC=$(( MA * 10000 + MI * 100 + PA ))
      [ "$VC" -lt 1 ] && VC=1
    fi

    # Replace the versionCode line that dx emitted (usually "1")
    sed -i -E "s/^( *versionCode *= *).*/\1$VC/" "$GRADLE_FILE" || true

    echo "  [play] Using versionCode=$VC (versionName will be $(grep '^version =' Cargo.toml | head -1 | sed -E 's/.*"([^"]+)".*/\1/')) for this AAB. Make sure this is higher than any previously uploaded versionCode for the app, or Play will block rollout with an 'upgrade' error."

    # Add ndk debug symbols config inside the release block (only if missing)
    if ! grep -q "debugSymbolLevel" "$GRADLE_FILE" 2>/dev/null; then
      sed -i '/getByName("release") {/a\            ndk {\n                debugSymbolLevel = "FULL"\n            }' "$GRADLE_FILE" || true
    fi

    # Force-keep all sentinel JNI entry points + manifest components.
    # Without this, R8 in release minify can strip the *Primitives / *Helper
    # classes (they are only referenced by string name from Rust JNI and by
    # the manifest injection). This caused "could not load class" for
    # permissions/overlay/scanner etc, making popups and scan not work.
    #
    # This keep rule does NOT pull in extra code. The set of modules that get
    # their Kotlin compiled is decided *before* this patch, by build_sentinel
    # reading the exact Cargo features from the .so (or --app). Only those
    # modules are include'd in settings.gradle. The keep below just protects
    # whatever was selected.
    #
    # Place sibling to this build.gradle.kts so the fileTree("**/*.pro") picks it.
    PROGUARD_KEEP_DIR="$(dirname "$GRADLE_FILE")"
    PROGUARD_KEEP="$PROGUARD_KEEP_DIR/proguard-sentinel-keep.pro"
    cat > "$PROGUARD_KEEP" << 'PRO'
# Keep sentinel classes/methods for JNI (exact names) + manifest components.
# Broad but safe: only affects the internal com.mobilesentinel package.
# IMPORTANT: This does not cause extra modules to be built.
# The list of :sentinel-xxx modules that Gradle sees comes exclusively
# from the earlier build_sentinel step, which only includes modules for
# the Cargo features that were actually enabled when the .so was compiled.
-keep class com.mobilesentinel.** { *; }
-keepclassmembers class com.mobilesentinel.** { *; }
PRO
    echo "  [play] Added proguard keep for sentinel JNI classes (permissions, overlay, scanner, ...)"

    echo "  [play] Patched Gradle for release: versionCode=$VC , debugSymbolLevel=FULL (see message above for upgrade warning)"
  fi
fi

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
  # NOTE: --release / --aab support landed in mobile-sentinel 0.1.4.
  SENTINEL_VERSION="0.1.4"
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

# For AAB (Play Store), automatically offer to sign it for internal testing
# if the signer script exists.
if [ "$MODE" = "aab" ]; then
    if [ -x "./sign-aab.sh" ]; then
        echo ""
        echo "=== Play Store AAB ready (unsigned) ==="
        echo "To produce a signed AAB ready to upload for internal testing, run:"
        echo "  ./sign-aab.sh"
        echo ""
        echo "This will create a timestamped signed file in the release/ folder"
        echo "that you can directly copy to Google Play Console → Internal testing."
        echo ""
        echo "For ALL future releases: keep using the same upload-keystore.jks"
        echo "(the script will automatically reuse it; do not create a new one)."
    else
        echo ""
        echo "AAB built. It is unsigned."
        echo "See README.md (Release / Google Play Store section) for signing instructions"
        echo "or run ./sign-aab.sh (create it if missing)."
    fi
fi

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
