#!/bin/bash
# The scan challenge's time limit on the Slint build: during a real ring the camera closes itself
# after the challenge's limit, the challenge is back and still quiet, and with no new tap on Scan
# the alarm rings again. Then Scan once more and a code (written where the scanner leaves it)
# turns the alarm off. Prints PASS or FAIL per step. Debug build, emulator (the clock is moved).
#
#   SERIAL=emulator-5554 bash devtest/slint_scan.sh

export MSYS_NO_PATHCONV=1
ADB="${ADB:-$LOCALAPPDATA/Android/Sdk/platform-tools/adb.exe}"
SERIAL="${SERIAL:-emulator-5554}"
PKG=com.fidderr.alarmfree.slint
FILES="/data/user_de/0/$PKG/files"
FAILS=0
mkdir -p devtest-out

dev() { timeout 60 "$ADB" -s "$SERIAL" shell "$@" | tr -d '\r'; }
as_app() { dev "run-as $PKG sh -c '$1'"; }

ui() {
    as_app "cd $FILES && rm -f debug_script_result.txt && echo ui:$1 > debug_action.tmp && mv debug_action.tmp debug_action.txt && i=0; while [ ! -e debug_script_result.txt ] && [ \$i -lt 500 ]; do sleep 0.02; i=\$((i + 1)); done; cat debug_script_result.txt 2> /dev/null"
}

# Wait up to $2 seconds until the command $1 (a shell function) succeeds.
until_true() {
    local i=0
    while [ "$i" -lt "${2:-20}" ]; do
        "$1" && return 0
        sleep 1
        i=$((i + 1))
    done
    return 1
}

check() { # name function [seconds]
    if until_true "$2" "${3:-20}"; then
        echo "PASS  $1"
    else
        echo "FAIL  $1 ($(ui snapshot | grep -o '"ringing":[a-z]*\|"run_active":[a-z]*\|"scan_state":[0-9]*\|"scanner_open":[a-z]*' | tr '\n' ' ')$(focused))"
        FAILS=$((FAILS + 1))
    fi
}

focused() { dev dumpsys window | grep -m1 mCurrentFocus | sed 's/.*{[^ ]* [^ ]* //; s/}//'; }
uid() { dev cmd package list packages -U "$PKG" | sed -E 's/.*uid:([0-9]+).*/\1/'; }
# The alarm holds audio focus while it sounds; pausing gives it back. (The player list of
# `dumpsys audio` keeps a paused player as started.)
sound_on() { dev logcat -d -s MediaFocusControl | grep "uid/pid $UID_APP/" | tail -1 | grep -q requestAudioFocus; }
sound_off() { ! sound_on; }
snap_has() { ui snapshot | grep -q -- "$1"; }
ringing() { snap_has '"ringing":true'; }
in_challenge() { snap_has '"run_active":true'; }
scanner_up() { focused | grep -q SentinelScannerActivity; }
scanner_gone() { ! scanner_up && snap_has '"scan_state":0'; }
turned_off() { snap_has '"ringing":false' && sound_off; }

dev pm clear "$PKG" > /dev/null
dev logcat -c
dev pm grant "$PKG" android.permission.CAMERA 2> /dev/null
UID_APP=$(uid)
dev monkey -p "$PKG" 1 > /dev/null 2>&1
db_made() { as_app "test -e $FILES/alarmfree.db" > /dev/null 2>&1 && as_app "test -e $FILES/alarmfree.db && echo y" | grep -q y; }
until_true db_made 40 || { echo "FAIL  the app made no database"; exit 1; }
sleep 1
dev am force-stop "$PKG"

# One alarm two minutes ahead, every day, with a scan challenge (any code).
now=$(dev date +%s)
at=$((now + 120))
h=$(dev date -d "@$at" +%H | sed 's/^0//'); m=$(dev date -d "@$at" +%M | sed 's/^0//')
json=$(printf '{"id":"a","time_hour":%s,"time_minute":%s,"label":"a","days":[true,true,true,true,true,true,true],"date":null,"skip_until_ms":null,"fade_in_secs":0,"ring_limit_secs":1800,"order":0,"enabled":true,"sound":"air","sound_mode":0,"challenges":[["challenge.scan","{}"]],"snooze_count":3,"snooze_interval":1}' "${h:-0}" "${m:-0}")
printf "INSERT OR REPLACE INTO alarms (id, data) VALUES ('a', '%s');
" "$json" > devtest-out/slint_scan_seed.sql
"$ADB" -s "$SERIAL" push devtest-out/slint_scan_seed.sql /data/local/tmp/slint_scan_seed.sql > /dev/null
dev chmod 644 /data/local/tmp/slint_scan_seed.sql
as_app "sqlite3 $FILES/alarmfree.db < /data/local/tmp/slint_scan_seed.sql"
seeded() { as_app "echo \"select count(*) from alarms;\" | sqlite3 $FILES/alarmfree.db" | grep -q "^1$"; }
seeded || { echo "FAIL  the alarm could not be seeded"; exit 1; }
# Short limits for the test: the camera 8 s, the challenge 8 s.
as_app "cd $FILES && echo 8 > debug_scan_camera_secs.txt && echo 8 > debug_idle_secs.txt"
dev monkey -p "$PKG" 1 > /dev/null 2>&1
sleep 4
target=$(( (at / 60) * 60 ))
dev cmd alarm set-time $(((target - 5) * 1000)) > /dev/null
check "the alarm rings" ringing 40
ringing || { echo "$FAILS step(s) FAILED (no ring, nothing else to check)"; exit 1; }
ui press:ring-turn-off > /dev/null
check "the scan challenge is up" in_challenge 10
in_challenge || { echo "$FAILS step(s) FAILED (no challenge)"; exit 1; }

ui press:scan > /dev/null
check "the camera opens" scanner_up 10
scanner_up || { echo "$FAILS step(s) FAILED (no camera)"; exit 1; }
check "quiet under the camera" sound_off 5
check "the camera closes itself after its limit" scanner_gone 16
check "back on the challenge, still quiet" in_challenge 3
sound_off || { echo "FAIL  loud right after the camera closed"; FAILS=$((FAILS + 1)); }
check "no tap on Scan: the alarm rings again" sound_on 16

# Back to the challenge, Scan, and a code turns it off.
until_true in_challenge 2 || { ui press:ring-turn-off > /dev/null; sleep 1; }
check "the challenge again" in_challenge 10
ui press:scan > /dev/null
check "the camera opens again" scanner_up 10
# Where the scanner leaves its result: the app context's files, credential- or device-encrypted.
as_app "echo a-code > /data/user/0/$PKG/files/sentinel_scan_result.txt; echo a-code > $FILES/sentinel_scan_result.txt"
check "the code turns the alarm off" turned_off 15

dev pm clear "$PKG" > /dev/null
if [ "$FAILS" -eq 0 ]; then
    echo "ALL PASS"
else
    echo "$FAILS step(s) FAILED"
    exit 1
fi
