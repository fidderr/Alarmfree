#!/bin/bash
# The suite on a real phone: APK=<arm64 apk> SERIAL=<phone> PHONE_SQLITE=<tool> bash devtest/phone.sh <name> [scenarios...]
# Never reboots it, never sets a PIN, no widget: not reboot_and_wait as devtest/all.sh does, because
# a phone with a PIN would then stand at its lock screen. The alarm volume goes to 1 for the run and
# comes back after, also when the run is stopped. Run it with a test build and no alarms of one's own
# on it: `fresh` throws the app's data away.
cd "$(dirname "$0")/.." || exit 1
NAME=$1
shift
: "${APK:?APK names the build for the phone}" "${SERIAL:?SERIAL names the phone}" "${PHONE_SQLITE:?PHONE_SQLITE names the sqlite tool on it}"
export APK SERIAL PHONE_SQLITE AVD=phone OUT="devtest-out/$NAME" NO_LOCK=1 MIN_VOLUME=1
mkdir -p "$OUT"
(
    . devtest/lib.sh
    alarm_volume() { dev cmd media_session volume --stream 4 --get 2> /dev/null | grep -oE 'volume is [0-9]+' | grep -oE '[0-9]+' | sed -n 1p; }
    OLD_VOLUME=$(alarm_volume)
    OLD_TIMEOUT=$(timeout 20 "$ADB" -s "$SERIAL" shell settings get system screen_off_timeout | tr -d '\r')
    OLD_STAY=$(timeout 20 "$ADB" -s "$SERIAL" shell settings get global stay_on_while_plugged_in | tr -d '\r')
    echo "volume was ${OLD_VOLUME:-?}, timeout $OLD_TIMEOUT, stay $OLD_STAY"
    [ -n "$OLD_VOLUME" ] || OLD_VOLUME=16
    restore() {
        timeout 20 "$ADB" -s "$SERIAL" shell cmd audio set-volume 4 "$OLD_VOLUME" > /dev/null 2>&1
        timeout 20 "$ADB" -s "$SERIAL" shell cmd media_session volume --stream 4 --set "$OLD_VOLUME" > /dev/null 2>&1
        timeout 20 "$ADB" -s "$SERIAL" shell settings put system screen_off_timeout "$OLD_TIMEOUT"
        timeout 20 "$ADB" -s "$SERIAL" shell settings put global stay_on_while_plugged_in "$OLD_STAY"
        echo "put back: volume $OLD_VOLUME, timeout $OLD_TIMEOUT, stay $OLD_STAY"
    }
    trap restore EXIT
    if [ -z "${NO_INSTALL:-}" ]; then
        said=$(adb_ install -r -g "$APK" 2>&1)
        case "$said" in
            *Success*) echo "installed" ;;
            *)
                echo "STOP  the install failed: $said"
                exit 1
                ;;
        esac
    fi
    dev_shell() { timeout 20 "$ADB" -s "$SERIAL" shell "$@"; }
    dev_shell settings put global stay_on_while_plugged_in 3
    dev_shell settings put system screen_off_timeout 1800000
    dev_shell input keyevent KEYCODE_WAKEUP
    set_alarm_volume 1
    open_shells
    fresh
    export CLEAN_COPY
    start_app
    on_screen .home-header 40 || { echo "STOP  the page does not render"; exit 1; }
    timeout "${LIMIT:-3000}" bash devtest/matrix.sh "$@" || echo "STOP  the run ended with $?"
) > "devtest-out/$NAME.log" 2>&1
sed -n '/===== SUMMARY/,$p' "devtest-out/$NAME.log" | grep -E '^(FAIL|STOP|SKIP|NOTE|passed:|over a minute)' | cut -c1-300 | sort -u
grep -E '^(volume was|installed|STOP|put back)' "devtest-out/$NAME.log" | cut -c1-200
tail -1 "devtest-out/$NAME.log" | cut -c1-200
