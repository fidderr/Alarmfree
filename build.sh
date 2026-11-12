#!/bin/bash
# AlarmFree Slint: the copy of AlarmFree whose UI is Slint, with no WebView.
# Cargo builds libalarmfree.so, then mobile-sentinel's build_sentinel wires the SDK's Kotlin,
# manifest parts and assets into the Gradle project in android/ and Gradle builds the APK.
#
# Usage:
#   ./build.sh                      debug APK
#   ./build.sh install [serial]     debug APK, installed on the device
#   ./build.sh release [install]    release APK (signed with the debug key, so it installs)
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
CARGO_FLAGS=()
SENTINEL_FLAGS=()
if [ "${1:-}" = "release" ]; then
  MODE=release
  CARGO_FLAGS=(--release)
  SENTINEL_FLAGS=(--release)
  shift
fi
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

# The Gradle project takes the native library from build/jniLibs, the Java source from
# android/src/java, and the manifest and theme from app/src/main.
rm -rf build/jniLibs
mkdir -p "build/jniLibs/$ABI" android/app/src/main/res/values
cp "target/$TARGET/$MODE/libalarmfree.so" "build/jniLibs/$ABI/libalarmfree.so"
cp android/src/AndroidManifest.template.xml android/app/src/main/AndroidManifest.xml
cp android/src/res/values/styles.xml android/app/src/main/res/values/styles.xml

# build_sentinel writes this machine's SDK paths into these two files; put them back after.
mkdir -p build
cp android/settings.gradle build/settings.gradle.orig
cp android/app/build.gradle.kts build/app.build.gradle.kts.orig
restore() {
  cp build/settings.gradle.orig android/settings.gradle
  cp build/app.build.gradle.kts.orig android/app/build.gradle.kts
}
trap restore EXIT

echo "=== [2/3] Wiring mobile-sentinel + assets + icon, and Gradle ==="
cargo run --quiet --manifest-path "$SENTINEL_MANIFEST" --bin build_sentinel -- \
  --project android ${SENTINEL_FLAGS[@]+"${SENTINEL_FLAGS[@]}"}

if [ "$MODE" = debug ]; then
  OUT_REL=android/app/build/outputs/apk/debug/app-debug.apk
else
  OUT_REL=android/app/build/outputs/apk/release/app-release.apk
  [ -f "$OUT_REL" ] || OUT_REL=android/app/build/outputs/apk/release/app-release-unsigned.apk
fi
mkdir -p build
cp "$OUT_REL" "build/alarmfree-slint-$MODE.apk"
echo "built build/alarmfree-slint-$MODE.apk ($(du -h "build/alarmfree-slint-$MODE.apk" | cut -f1))"

if $INSTALL; then
  echo "=== [3/3] Installing ==="
  if [ -n "$SERIAL" ]; then
    adb -s "$SERIAL" install -r "build/alarmfree-slint-$MODE.apk"
  else
    adb install -r "build/alarmfree-slint-$MODE.apk"
  fi
fi
