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
# Phones are arm64. An emulator needs another ABI:
#   TARGET=x86_64-linux-android ./build.sh install emulator-5556
#
# The SDK is the mobile-sentinel worktree beside this folder (see Cargo.toml).
set -euo pipefail

cd "$(dirname "$0")"
SDK_HOME="${ANDROID_HOME:-$LOCALAPPDATA/Android/Sdk}"
export PATH="$PATH:$HOME/.cargo/bin:$SDK_HOME/platform-tools"
export ANDROID_HOME="$SDK_HOME"
# Slint's prebuilt Skia needs NDK r30.
export ANDROID_NDK_ROOT="$SDK_HOME/ndk/30.0.16248370" ANDROID_NDK_HOME="$SDK_HOME/ndk/30.0.16248370"
export JAVA_HOME="${JAVA_HOME:-C:/Program Files/Microsoft/jdk-17.0.21.21-hotspot}"
NDK_BIN="$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/windows-x86_64/bin"
# What C code in dependencies (SQLite) is compiled with.
for triple in aarch64-linux-android x86_64-linux-android; do
  var="${triple//-/_}"
  export "CC_$var=$NDK_BIN/${triple}31-clang.cmd" "CXX_$var=$NDK_BIN/${triple}31-clang++.cmd" "AR_$var=$NDK_BIN/llvm-ar.exe"
done
TARGET="${TARGET:-aarch64-linux-android}"
SENTINEL_MANIFEST="${SENTINEL_MANIFEST:-../vp-work/ms-native/Cargo.toml}"

MODE=debug
PROFILE_DIR=debug
CARGO_FLAGS=()
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
cargo run --quiet --manifest-path "$SENTINEL_MANIFEST" --bin build_sentinel -- \
  ${SENTINEL_FLAGS[@]+"${SENTINEL_FLAGS[@]}"}

if [ "$MODE" = debug ]; then
  OUT_REL=target/sentinel-android/app/build/outputs/apk/debug/app-debug.apk
elif [ "$MODE" = aab ]; then
  OUT_REL=target/sentinel-android/app/build/outputs/bundle/release/app-release.aab
else
  OUT_REL=target/sentinel-android/app/build/outputs/apk/release/app-release.apk
  [ -f "$OUT_REL" ] || OUT_REL=target/sentinel-android/app/build/outputs/apk/release/app-release-unsigned.apk
fi
mkdir -p build
EXT=apk; [ "$MODE" = aab ] && EXT=aab
cp "$OUT_REL" "build/alarmfree-slint-$MODE.$EXT"
echo "built build/alarmfree-slint-$MODE.$EXT ($(du -h "build/alarmfree-slint-$MODE.$EXT" | cut -f1))"

if $INSTALL && [ "$MODE" != aab ]; then
  echo "=== [3/3] Installing ==="
  if [ -n "$SERIAL" ]; then
    adb -s "$SERIAL" install -r "build/alarmfree-slint-$MODE.apk"
  else
    adb install -r "build/alarmfree-slint-$MODE.apk"
  fi
fi
