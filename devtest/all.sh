#!/bin/bash
# The whole suite on the newest and the oldest Android at the same time, with the APK that is
# already built (APK says which, for a release). Two emulators: that is what 16 GB has room for.
#
# Usage: bash devtest/all.sh [name]    results in devtest-out/<name>_<avd>/ and <name>_<avd>.log
cd "$(dirname "$0")/.." || exit 1
export APK="${APK:-target/dx/alarmfree/debug/android/app/app/build/outputs/apk/debug/app-debug.apk}"
NAME=${1:-full}
# What does not depend on Android itself, on the oldest only: its WebView is the hardest on a
# page. What depends on the size, on the newest only: from Android 16 on an app has no say in
# how it turns on a big screen. Also the restart with a PIN, which works the same on all of them
# and takes a minute.
# A build that fails leaves the last APK where it was: this would then test something older.
built=$(stat -c %Y "$APK") || exit 1
for repo in . ../mobile-sentinel; do
    if [ "$built" -lt "$(git -C "$repo" log -1 --format=%ct)" ]; then
        echo "$APK is older than the last commit in $repo: build first"
        exit 1
    fi
done

OLDEST_ONLY="s29 s30 s31 s32 s34 s38 s43 s44 s45 s46 s47 s50 s51 s52"
NEWEST_ONLY="s13 s13b s13d big_screens"

run_on() { # avd port scenarios...
    local avd=$1 port=$2
    shift 2
    (
        export AVD=$avd SERIAL=emulator-$port OUT="devtest-out/${NAME}_$avd"
        . devtest/lib.sh
        claim_device
        reboot_and_wait || { echo "FAIL  $avd did not start"; exit 1; }
        # A failed install would go on to test what is already on it.
        said=$(adb_ install -r -g "$APK" 2>&1)
        case "$said" in
            *Success*) ;;
            *)
                echo "STOP  $avd: the install failed: $said"
                exit 1
                ;;
        esac
        # Without a working page nearly everything fails, and slowly: stop the run at once.
        fresh
        export CLEAN_COPY
        start_app
        on_screen .home-header 30 || { echo "STOP  $avd: the page does not render"; exit 1; }
        bash devtest/matrix.sh "$@"
        emulator_stop
    ) > "devtest-out/${NAME}_$avd.log" 2>&1
    echo "=== $avd"
    sed -n '/===== SUMMARY/,$p' "devtest-out/${NAME}_$avd.log" | grep -E '^(FAIL|STOP|passed:|over a minute)'
}

START=$SECONDS
# The big screens first: next to the other emulator's tour of every panel, late in its run, Android
# 16 starved to the point of its own system UI freezing.
# The restarts with a PIN last: Android 12 is done by then, and they stay under a minute.
FIRST=big_screens LAST="s13d s13b s13" SKIP=$OLDEST_ONLY run_on af36 5554 &
# Android 12's graphics on the host's GPU crashed the emulator now and then, in the middle of a run.
EMULATOR_GPU=swiftshader_indirect SKIP=$NEWEST_ONLY run_on af31 5556 &
wait
echo "all of it in $(((SECONDS - START) / 60)) min $(((SECONDS - START) % 60)) s"
