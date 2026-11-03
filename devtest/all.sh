#!/bin/bash
# The whole matrix on each emulator in turn, with the APK that is already built (APK says
# which, for a release). The tablet and foldable only do what depends on the size.
#
# Usage: bash devtest/all.sh [name]    results in devtest-out/<name>_<avd>/
cd "$(dirname "$0")/.." || exit 1
. devtest/lib.sh
export APK="${APK:-target/dx/alarmfree/debug/android/app/app/build/outputs/apk/debug/app-debug.apk}"
NAME=${1:-full}
for AVD in af36 af31 af36tab af36fold; do
    export AVD OUT="devtest-out/${NAME}_$AVD"
    echo "=== AVD $AVD"
    reboot_and_wait || { echo "FAIL  $AVD did not start"; continue; }
    adb_ install -r -g "$APK" | tail -1
    # Without a working page nearly everything fails, and slowly: stop the run at once.
    fresh
    start_app
    for _ in 1 2 3 4 5 6; do
        on_screen .home-header && break
        sleep 5
    done
    on_screen .home-header || { echo "STOP  $AVD: the page does not render"; exit 1; }
    case "$AVD" in
        af36tab | af36fold) bash devtest/matrix.sh s01 s25 s28 s31 s34 s35 s36 s37 s49 ;;
        *) bash devtest/matrix.sh ;;
    esac
done
