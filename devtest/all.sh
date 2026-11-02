#!/bin/bash
# The whole matrix on each emulator in turn, with the APK that is already built.
# Android 8's stock WebView cannot run the UI, so that one runs without a screen.
#
# Usage: bash devtest/all.sh [name]    results in devtest-out/<name>_<avd>/
cd "$(dirname "$0")/.." || exit 1
. devtest/lib.sh
APK=target/dx/alarmfree/debug/android/app/app/build/outputs/apk/debug/app-debug.apk
NAME=${1:-full}
for entry in af36: af31: af26:no_ui; do
    export AVD=${entry%%:*} NO_UI=${entry#*:} OUT="devtest-out/${NAME}_${entry%%:*}"
    echo "=== AVD $AVD"
    reboot_and_wait || { echo "FAIL  $AVD did not start"; continue; }
    adb_ install -r -g "$APK" | tail -1
    bash devtest/matrix.sh
done
