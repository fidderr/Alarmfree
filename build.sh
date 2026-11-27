#!/bin/bash
# AlarmFree Slint: the copy of AlarmFree whose UI is Slint, with no WebView.
# Cargo builds libalarmfree.so, then mobile-sentinel's build_sentinel (generate = true in
# sentinel.toml) writes the whole Gradle project into target/sentinel-android/, wires the SDK's
# Kotlin, manifest parts and assets into it and Gradle builds the APK. The app keeps no android/ folder.
# A release build runs R8 and the Android Gradle plugin strips the library; the unstripped copy
# (for crash symbols) is build/libmain.so.unstripped.
#
# Usage:
#   ./build.sh                      debug APK
#   ./build.sh install [serial]     debug APK, installed on the device
#   ./build.sh release [install]    release APK (signed with the debug key, so it installs)
#   ./build.sh fast [install]       release APK from the fast cargo profile (no LTO, incremental):
#                                   a changed file builds in seconds, the .so is bigger. Not for shipping.
#   ./build.sh aab                  release AAB for Play (unsigned; then ./sign-aab.sh)
#
# A test copy beside the real app (debug only; a release, fast or aab build refuses it):
#   APP_ID_SUFFIX=.dev ./build.sh install
# has the id com.fidderr.alarmfree.dev and the launcher label "AlarmFree test", so it installs next to
# the Play app without touching it, and writes build/alarmfree-slint-debug-dev.apk. Everything that has
# to be unique per app follows the id (provider authorities, the receiver permission). Without the
# switch the build is exactly as before. devtest/phone.sh runs the suite on that id.
#
# Phones are arm64. An emulator needs another ABI:
#   TARGET=x86_64-linux-android ./build.sh install emulator-5556
#
# The SDK is the mobile-sentinel worktree beside this folder (see Cargo.toml).
set -euo pipefail

cd "$(dirname "$0")"
# The copy of sentinel.toml that a test build (APP_ID_SUFFIX, below) leaves when it is killed: put back before anything reads it.
TOML_COPY=build/sentinel.toml.original
[ ! -f "$TOML_COPY" ] || mv -f "$TOML_COPY" sentinel.toml
SDK_HOME="${ANDROID_HOME:-${LOCALAPPDATA:-}/Android/Sdk}"
export PATH="$PATH:$HOME/.cargo/bin:$SDK_HOME/platform-tools"
export ANDROID_HOME="$SDK_HOME"
# Slint's prebuilt Skia needs NDK r30.
export ANDROID_NDK_ROOT="$SDK_HOME/ndk/30.0.16248370" ANDROID_NDK_HOME="$SDK_HOME/ndk/30.0.16248370"
export JAVA_HOME="${JAVA_HOME:-C:/Program Files/Microsoft/jdk-17.0.21.21-hotspot}"
# Windows is the owner's PC; Linux is the CI runner (.github/workflows/emulators.yml).
if [ "$(uname -s)" = Linux ]; then HOST_TAG=linux-x86_64 CMD_EXT="" EXE_EXT=""; else HOST_TAG=windows-x86_64 CMD_EXT=.cmd EXE_EXT=.exe; fi
NDK_BIN="$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/$HOST_TAG/bin"
# The oldest Android the app runs on, from sentinel.toml: what the native code is linked and compiled for.
MIN_SDK=$(sed -n 's/^min_sdk *= *\([0-9][0-9]*\).*/\1/p' sentinel.toml)
[ -n "$MIN_SDK" ] || { echo "ERROR: sentinel.toml has no min_sdk"; exit 1; }
# What C code in dependencies (SQLite) is compiled with, and what links the library.
for triple in aarch64-linux-android x86_64-linux-android; do
  var="${triple//-/_}"
  export "CC_$var=$NDK_BIN/${triple}${MIN_SDK}-clang$CMD_EXT" "CXX_$var=$NDK_BIN/${triple}${MIN_SDK}-clang++$CMD_EXT" "AR_$var=$NDK_BIN/llvm-ar$EXE_EXT"
  export "CARGO_TARGET_$(echo "$var" | tr a-z A-Z)_LINKER=$NDK_BIN/${triple}${MIN_SDK}-clang$CMD_EXT"
done
TARGET="${TARGET:-aarch64-linux-android}"
SENTINEL_MANIFEST="${SENTINEL_MANIFEST:-../mobile-sentinel/Cargo.toml}"

APP_ID_SUFFIX="${APP_ID_SUFFIX:-}"
APP_ID_LABEL="AlarmFree test"
[[ -z $APP_ID_SUFFIX || $APP_ID_SUFFIX =~ ^\.[a-z][a-z0-9_]*$ ]] || { echo "ERROR: APP_ID_SUFFIX is a dot and a lower-case word, for example .dev"; exit 1; }

MODE=debug
PROFILE_DIR=debug
# The debug build carries what devtest/ drives it with (cargo feature devtest); no other build does.
CARGO_FLAGS=(--features devtest)
SENTINEL_FLAGS=()
case "${1:-}" in
  release)
    MODE=release; PROFILE_DIR=release
    CARGO_FLAGS=(--release); SENTINEL_FLAGS=(--release)
    shift ;;
  fast)
    MODE=fast; PROFILE_DIR=release-fast
    CARGO_FLAGS=(--profile release-fast); SENTINEL_FLAGS=(--release)
    shift ;;
  aab)
    MODE=aab; PROFILE_DIR=release
    CARGO_FLAGS=(--release); SENTINEL_FLAGS=(--release --aab)
    shift ;;
esac
if [ -n "$APP_ID_SUFFIX" ] && [ "$MODE" != debug ]; then
  echo "ERROR: APP_ID_SUFFIX is for the debug build only: the $MODE build is the app that goes to Play"
  exit 1
fi
OUT_NAME="alarmfree-slint-$MODE${APP_ID_SUFFIX:+-${APP_ID_SUFFIX#.}}"
INSTALL=false
SERIAL=""
if [ "${1:-}" = "install" ]; then
  INSTALL=true
  SERIAL="${2:-}"
fi

case "$TARGET" in
  aarch64-linux-android) ABI=arm64-v8a ;;
  x86_64-linux-android) ABI=x86_64 ;;
  *) echo "ERROR: TARGET must be aarch64-linux-android or x86_64-linux-android"; exit 1 ;;
esac

echo "=== [1/3] Building libalarmfree.so ($TARGET, $MODE) ==="
cargo build --lib --target "$TARGET" ${CARGO_FLAGS[@]+"${CARGO_FLAGS[@]}"}

# Named main: the manifest's NativeActivity and the SDK's Kotlin load the library by that name
# (SentinelBridge), also for what runs without the UI (a ring, a restart after a boot).
rm -rf build/jniLibs
mkdir -p "build/jniLibs/$ABI"
cp "target/$TARGET/$PROFILE_DIR/libalarmfree.so" "build/jniLibs/$ABI/libmain.so"
cp "target/$TARGET/$PROFILE_DIR/libalarmfree.so" build/libmain.so.unstripped

echo "=== [2/3] Generating the Android project, wiring mobile-sentinel + assets + icon, and Gradle ==="
# The SDK reads the id and the label from sentinel.toml in this folder: for a test copy it reads a changed one, and the
# trap puts the real one back (also after an abort; a copy left by a run that was killed is put back first).
mkdir -p build
if [ -n "$APP_ID_SUFFIX" ]; then
  cp -p sentinel.toml "$TOML_COPY"
  trap 'mv -f "$TOML_COPY" sentinel.toml' EXIT
  sed -i -e "/^application_id *=/s/\"[[:space:]]*\$/$APP_ID_SUFFIX\"/" -e "/^label *=/s/\".*\"/\"$APP_ID_LABEL\"/" sentinel.toml
  grep -q "^application_id = \".*$APP_ID_SUFFIX\"" sentinel.toml || { echo "ERROR: sentinel.toml has no application_id to give the suffix"; exit 1; }
  echo "Test copy: $(sed -n 's/^application_id *= *"\(.*\)".*/\1/p' sentinel.toml), label \"$APP_ID_LABEL\""
fi
# Newer SDKs keep the build tool behind the `build` feature.
SDK_FEATURES=()
grep -q '^build = ' "$SENTINEL_MANIFEST" && SDK_FEATURES=(--features build)
cargo run --quiet --manifest-path "$SENTINEL_MANIFEST" ${SDK_FEATURES[@]+"${SDK_FEATURES[@]}"} --bin build_sentinel -- \
  ${SENTINEL_FLAGS[@]+"${SENTINEL_FLAGS[@]}"}

if [ "$MODE" = debug ]; then
  OUT_REL=target/sentinel-android/app/build/outputs/apk/debug/app-debug.apk
elif [ "$MODE" = aab ]; then
  OUT_REL=target/sentinel-android/app/build/outputs/bundle/release/app-release.aab
else
  OUT_REL=target/sentinel-android/app/build/outputs/apk/release/app-release.apk
  [ -f "$OUT_REL" ] || OUT_REL=target/sentinel-android/app/build/outputs/apk/release/app-release-unsigned.apk
fi
EXT=apk; [ "$MODE" = aab ] && EXT=aab
cp "$OUT_REL" "build/$OUT_NAME.$EXT"
echo "built build/$OUT_NAME.$EXT ($(du -h "build/$OUT_NAME.$EXT" | cut -f1))"

if $INSTALL && [ "$MODE" != aab ]; then
  echo "=== [3/3] Installing ==="
  if [ -n "$SERIAL" ]; then
    adb -s "$SERIAL" install -r "build/$OUT_NAME.apk"
  else
    adb install -r "build/$OUT_NAME.apk"
  fi
fi
