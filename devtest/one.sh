#!/bin/bash
# A few scenarios on one emulator, started like all.sh does.
# Usage: bash devtest/one.sh <avd> <port> [scenario...]    (results in devtest-out/one_<avd>.log)
cd "$(dirname "$0")/.." || exit 1
export APK="${APK:-build/alarmfree-slint-debug.apk}"
avd=$1 port=$2
shift 2
export AVD=$avd SERIAL=emulator-$port OUT="devtest-out/one_$avd"
[ "$avd" = af31 ] && export EMULATOR_GPU=swiftshader_indirect
. devtest/lib.sh
claim_device
if [ -z "${NO_BOOT:-}" ]; then
    reboot_and_wait || { echo "FAIL  $avd did not start"; exit 1; }
fi
said=$(adb_ install -r -g "$APK" 2>&1)
case "$said" in *Success*) ;; *) echo "STOP install: $said"; exit 1 ;; esac
fresh
export CLEAN_COPY
start_app
wait_for 60 page_is_up && wait_for 30 on_screen_no 0 || { echo "STOP  window does not come up"; exit 1; }
bash devtest/matrix.sh "$@"
[ -z "${KEEP:-}" ] && emulator_stop
