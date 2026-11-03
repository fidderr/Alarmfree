#!/bin/bash
# The whole suite on the newest and the oldest Android at the same time, then what depends on
# the size on a tablet and a foldable, with the APK that is already built (APK says which, for a
# release).
#
# Usage: bash devtest/all.sh [name]    results in devtest-out/<name>_<avd>/ and <name>_<avd>.log
cd "$(dirname "$0")/.." || exit 1
export APK="${APK:-target/dx/alarmfree/debug/android/app/app/build/outputs/apk/debug/app-debug.apk}"
NAME=${1:-full}
SIZES="s01 s19 s25 s28 s31 s34 s36 s37 s49"

run_on() { # avd port scenarios...
    local avd=$1 port=$2
    shift 2
    (
        export AVD=$avd SERIAL=emulator-$port OUT="devtest-out/${NAME}_$avd"
        . devtest/lib.sh
        reboot_and_wait || { echo "FAIL  $avd did not start"; exit 1; }
        adb_ install -r -g "$APK" | tail -1
        # Without a working page nearly everything fails, and slowly: stop the run at once.
        fresh
        start_app
        on_screen .home-header 30 || { echo "STOP  $avd: the page does not render"; exit 1; }
        bash devtest/matrix.sh "$@"
        emulator_stop
    ) > "devtest-out/${NAME}_$avd.log" 2>&1
    echo "=== $avd"
    sed -n '/===== SUMMARY/,$p' "devtest-out/${NAME}_$avd.log" | grep -E '^(FAIL|STOP|passed:|over a minute)'
}

START=$SECONDS
run_on af36 5554 &
run_on af31 5556 &
wait
run_on af36tab 5554 $SIZES &
run_on af36fold 5556 $SIZES &
wait
echo "all of it in $(((SECONDS - START) / 60)) min $(((SECONDS - START) % 60)) s"
