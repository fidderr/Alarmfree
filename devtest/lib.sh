#!/bin/bash
# Helpers for the on-device scenarios in matrix.sh. Everything goes through
# adb against one explicit device, so a connected phone is never touched by
# accident. Needs a debug build (run-as, debug_action.txt).

export MSYS_NO_PATHCONV=1
ADB="${ADB:-$LOCALAPPDATA/Android/Sdk/platform-tools/adb.exe}"
SERIAL="${SERIAL:-emulator-5554}"
PKG=com.fidderr.alarmfree
ACTIVITY=dev.dioxus.main.MainActivity
OUT="${OUT:-devtest-out}"
mkdir -p "$OUT"

# Nothing may hang a run: an unresponsive device fails instead of waiting forever.
adb_() { timeout 120 "$ADB" -s "$SERIAL" "$@"; }
device_ok() { [ "$(timeout 20 "$ADB" -s "$SERIAL" shell echo ok | tr -d '\r')" = ok ]; }
dev() { adb_ shell "$@" | tr -d '\r'; }
# The app's files are device-encrypted (direct_boot in sentinel.toml), and run-as
# starts in the credential-encrypted directory.
DATA="/data/user_de/0/$PKG"
as_app() { adb_ shell "run-as $PKG sh -c 'cd $DATA && $*'" | tr -d '\r'; }
sql() { adb_ shell "run-as $PKG sqlite3 $DATA/files/alarmfree.db" 2>&1 | tr -d '\r'; }

main_pid() { dev pidof "$PKG"; }
sentinel_pid() { dev pidof "$PKG:sentinel"; }
kill_main() { local p; p=$(main_pid); [ -n "$p" ] && as_app "kill -9 $p"; true; }
kill_sentinel() { local p; p=$(sentinel_pid); [ -n "$p" ] && as_app "kill -9 $p"; true; }
kill_all() { kill_main; kill_sentinel; }

now_s() { dev date +%s; }
wake() { dev input keyevent KEYCODE_WAKEUP; dev wm dismiss-keyguard; }
screen_off() { dev input keyevent KEYCODE_SLEEP; }
go_home() { dev input keyevent KEYCODE_HOME; }
screenshot() { adb_ exec-out screencap -p > "$OUT/$1.png"; }

# Restart the device and wait until it is back. A device that does not come back
# fails the scenario instead of hanging the run.
#
# An emulator gets a cold start. `adb reboot` itself is unreliable on them: with no
# app running at all, the emulator can freeze for good or exit (emulator 37.2.13,
# Windows). For the app it is the same restart: every process and OS alarm is gone,
# and BOOT_COMPLETED comes. A clean shutdown is what only a real phone tests.
AVD="${AVD:-af36}"
EMULATOR="${EMULATOR:-$LOCALAPPDATA/Android/Sdk/emulator/emulator.exe}"

emulator_start() {
    powershell.exe -NoProfile -Command "Start-Process -WindowStyle Hidden -FilePath '$(cygpath -w "$EMULATOR")' -ArgumentList '-avd $AVD -no-window -no-audio -no-boot-anim -no-snapshot -gpu swiftshader_indirect -port ${SERIAL#emulator-}'"
}

emulator_stop() {
    powershell.exe -NoProfile -Command "Get-Process | Where-Object { \$_.ProcessName -match 'emulator|qemu' } | Stop-Process -Force -ErrorAction SilentlyContinue"
    sleep 3
}

reboot_and_wait() {
    case "$SERIAL" in
        emulator-*)
            adb_ shell sync
            emulator_stop
            emulator_start
            ;;
        *)
            timeout 60 "$ADB" -s "$SERIAL" reboot || { note "adb reboot did not return in 60s"; return 1; }
            ;;
    esac
    timeout 240 "$ADB" -s "$SERIAL" wait-for-device || { note "device did not come back"; return 1; }
    timeout 240 "$ADB" -s "$SERIAL" shell 'while [ -z "$(getprop sys.boot_completed)" ]; do sleep 1; done' ||
        { note "boot did not complete"; return 1; }
    sleep 3
}

# A clean app: no alarms, normal permissions, database created.
fresh() {
    dev am force-stop "$PKG"
    dev pm clear "$PKG" > /dev/null
    grant_all
    start_app
    # Opening it is what finds out the WebView is too old, as for any user.
    [ -n "$NO_UI" ] && dev am start -n "$PKG/$ACTIVITY" > /dev/null
    wait_file files/alarmfree.db 60
    sleep 3
    dev am force-stop "$PKG"
    sleep 1
    # Force-stop leaves a package "stopped": clear that by starting it once
    # more in arm(), which every scenario does before relying on an alarm.
}

grant_all() {
    dev pm grant "$PKG" android.permission.POST_NOTIFICATIONS 2> /dev/null
    dev pm grant "$PKG" android.permission.ACTIVITY_RECOGNITION 2> /dev/null
    dev pm grant "$PKG" android.permission.CAMERA 2> /dev/null
    dev appops set "$PKG" SYSTEM_ALERT_WINDOW allow
}

# NO_UI: for a WebView too old for the UI. Rust only renders once the page's JavaScript
# runs, so opening the app would set up nothing there.
start_app() {
    if [ -n "$NO_UI" ]; then
        headless
    else
        dev am start -n "$PKG/$ACTIVITY" > /dev/null
    fi
}

# Start the app's core without a screen, as a reboot or a ring does. Its receiver only
# takes that from the system, or from root on an emulator.
headless() {
    case "$(adb_ root 2>&1)" in
        *restarting*) sleep 3 ;;
    esac
    dev am broadcast -f 32 -n "$PKG/com.mobilesentinel.SentinelRearmReceiver" -a com.mobilesentinel.RING_STARTED > /dev/null
}

wait_file() {
    local f=$1 t=${2:-30} i=0
    while [ "$i" -lt $((t * 2)) ]; do
        [ "$(as_app "test -e $f && echo y")" = y ] && return 0
        sleep 0.5
        i=$((i + 1))
    done
    return 1
}

# The time `lead` seconds from now, rounded up to a whole minute. Sets T
# (unix s), T_H, T_M, T_DATE. Alarms have minute resolution.
next_minute() {
    local lead=${1:-45} n off local_s
    n=$(now_s)
    T=$(((n + lead + 59) / 60 * 60))
    # The device's own `date -d @T` is UTC on Android 8's toybox.
    off=$(dev date +%z | tr -d '\r')
    local_s=$((T + ${off:0:1}(10#${off:1:2} * 3600 + 10#${off:3:2} * 60)))
    T_H=$(date -u -d "@$local_s" +%-H)
    T_M=$(date -u -d "@$local_s" +%-M)
    T_DATE=$(date -u -d "@$local_s" +%Y-%m-%d)
}

# alarm_json id hour minute days date [sound] [sound_mode] [fade] [snooze_n] [snooze_min] [challenges] [ring_limit_secs]
alarm_json() {
    local id=$1 h=$2 m=$3 days=$4 date=$5 sound=${6:-air} mode=${7:-0} fade=${8:-0}
    local sn=${9:-3} si=${10:-1} ch=${11:-[]} limit=${12:-1800}
    printf '{"id":"%s","time_hour":%s,"time_minute":%s,"label":"%s","days":%s,"date":%s,"skip_until_ms":null,"fade_in_secs":%s,"ring_limit_secs":%s,"order":0,"enabled":true,"sound":"%s","sound_mode":%s,"challenges":%s,"snooze_count":%s,"snooze_interval":%s}' \
        "$id" "$h" "$m" "$id" "$days" "$date" "$fade" "$limit" "$sound" "$mode" "$ch" "$sn" "$si"
}
EVERY_DAY='[true,true,true,true,true,true,true]'
NO_DAYS='[false,false,false,false,false,false,false]'

seed() { # seed id json (app must be stopped)
    printf "INSERT OR REPLACE INTO alarms (id, data) VALUES ('%s', '%s');\n" "$1" "$2" | sql
}

db_enabled() { # true / false / missing
    local v
    v=$(printf "SELECT data FROM alarms WHERE id='%s';\n" "$1" | sql)
    [ -z "$v" ] && { echo missing; return; }
    echo "$v" | grep -q '"enabled":true' && echo true || echo false
}

# Start the app so it arms the seeded alarms, then wait until `id` is armed.
arm() {
    start_app
    wait_file "files/sentinel/context/$1.json" 45
    sleep 3
}

# Leave no app process running, the way the system leaves it overnight.
dead() {
    go_home
    sleep 1
    kill_all
    sleep 1
    [ -z "$(main_pid)$(sentinel_pid)" ]
}

state() { # idle / scheduled / firing / snoozed / dismissed / none
    local s
    s=$(as_app "grep \"\\\"state\\\": \\\"\" files/sentinel/context/$1.json 2>/dev/null" | sed -E 's/.*"state": "([a-z]+)".*/\1/')
    echo "${s:-none}"
}
next_fire_ms() { as_app "grep next_fire_unix_ms files/sentinel/context/$1.json 2>/dev/null" | sed -E 's/[^0-9]//g'; }
job_status() { # active / pending / none
    if [ "$(as_app "test -e files/sentinel/jobs/$1.active && echo y")" = y ]; then
        echo active
    elif [ "$(as_app "test -e files/sentinel/jobs/$1.json && echo y")" = y ]; then
        echo pending
    else
        echo none
    fi
}
action() { as_app "echo $1 > files/debug_action.txt"; }

# Pretend the alarm was due `$2` minutes ago, with the app stopped.
make_overdue() {
    local due=$(($(now_s) * 1000 - $2 * 60000))
    as_app "sed -i -E \"s/(next_fire_unix_ms\\\": )[0-9]+/\\1$due/\" files/sentinel/context/$1.json"
    as_app "sed -i -E \"1s/.*/$due/\" files/sentinel/armed/$1.armed"
}

outcomes() { # how many history rows have this outcome
    printf "SELECT count(*) FROM alarm_events WHERE outcome='%s';\n" "$1" | sql
}
unseen() { # how many history rows the user has not been told about
    printf "SELECT count(*) FROM alarm_events WHERE seen=0;\n" | sql
}

# Is the app's alarm sound playing right now?
sound_on() {
    local uid
    uid=$(dev cmd package list packages -U "$PKG" | sed -E 's/.*uid:([0-9]+).*/\1/')
    dev dumpsys audio | grep -E "u/pid:$uid/[0-9]+ .*state:started.*(USAGE_ALARM|usage=4 )" > /dev/null
}

# Milliseconds from the alarm being delivered until the sound plays, from the
# log (needs the log since the alarm). Empty when either line is missing.
ring_ms() {
    adb_ logcat -d -v epoch 2> /dev/null | tr -d '\r' | awk '
        /MobileSentinel.AlarmRx/ && /onReceive: instance=/ && !/#watchdog/ && !rx { rx = $1 }
        /MobileSentinel.FiringAudio/ && /playing / && !play { play = $1 }
        END { if (rx && play) printf "%d", (play - rx) * 1000 }'
}

# Milliseconds from the alarm's time T until the sound plays.
sound_after_t_ms() {
    adb_ logcat -d -v epoch 2> /dev/null | tr -d '\r' | awk -v t="$T" '
        /MobileSentinel.FiringAudio/ && /playing / && !play { play = $1 }
        END { if (play) printf "%d", (play - t) * 1000 }'
}

# Wait for the sound to stop; sets STOPPED_AT to seconds past T. Fails after `limit` seconds past T.
await_silence() {
    local limit=$1 n
    while :; do
        n=$(now_s)
        [ "$n" -gt $((T + limit)) ] && return 1
        if ! sound_on; then
            STOPPED_AT=$((n - T))
            return 0
        fi
        sleep 1
    done
}

# Seconds from T until the sound starts, or empty after `limit` seconds past T.
await_sound() {
    local limit=${1:-30} n
    while :; do
        n=$(now_s)
        [ "$n" -gt $((T + limit)) ] && return 1
        if sound_on; then
            SOUND_AT=$((n - T))
            return 0
        fi
        sleep 0.3
    done
}

until_past() { # wait until device time is past unix second $1
    while [ "$(now_s)" -lt "$1" ]; do sleep 1; done
}

top_is_backup() { dev dumpsys activity activities | grep -E "topResumedActivity|ResumedActivity:" | head -1 | grep -q SentinelBackupActivity; }

# Do the backup screen's challenge: `n` long presses where its button is.
solve_backup() {
    local n=$1 size w h i=0
    size=$(dev wm size | tail -1 | sed 's/.*: //')
    w=${size%x*}
    h=${size#*x}
    while [ "$i" -lt "$n" ]; do
        dev input swipe $((w / 2)) $((h * 2 / 3)) $((w / 2)) $((h * 2 / 3)) 3300
        sleep 0.3
        i=$((i + 1))
    done
}

top_is_app() { dev dumpsys activity activities | grep -E "topResumedActivity|ResumedActivity:" | head -1 | grep -q "$PKG"; }
armed_at_ms() { as_app "head -n 1 files/sentinel/armed/$1.armed 2>/dev/null"; }
os_has_alarm() { dev dumpsys alarm | grep -q "$PKG.*ALARM_DISPATCH\|ALARM_DISPATCH.*$PKG\|walarm\*:com.mobilesentinel.ALARM_DISPATCH"; }
alarm_volume() { dev cmd media_session volume --stream 4 --get 2> /dev/null | grep -oE 'volume is [0-9]+' | grep -oE '[0-9]+'; }
# Android 16 only listens to the first of these, older versions only to the second.
set_alarm_volume() {
    dev cmd audio set-volume 4 "$1" > /dev/null 2>&1
    dev cmd media_session volume --stream 4 --set "$1" > /dev/null 2>&1
}

# Wait until Android has told the app about the boot: on a software-rendered
# emulator that is well after sys.boot_completed. Opening the app before it can get
# the activity relaunched, and any activity destroy ends the main process.
await_boot_signal() {
    local i=0
    while [ "$i" -lt 60 ]; do
        [ -n "$(sentinel_pid)" ] && return 0
        sleep 1
        i=$((i + 1))
    done
    return 1
}

# ---- results ----
RESULTS="$OUT/results.txt"
CURRENT=""
scenario() {
    CURRENT=$1
    echo
    echo "=== $CURRENT"
    if ! device_ok; then
        echo "device not responding, stopping the run"
        echo "STOP  $CURRENT  device not responding" >> "$RESULTS"
        exit 2
    fi
    adb_ logcat -c
}
pass() { echo "PASS  $CURRENT  $1" | tee -a "$RESULTS"; }
fail() { echo "FAIL  $CURRENT  $1" | tee -a "$RESULTS"; }
note() { echo "      $1"; }
save_log() { adb_ logcat -d -v time 2> /dev/null | grep -E "AlarmFree|MobileSentinel|Sentinel|AlarmKit|AlarmClass|alarmfree|ActivityTaskManager.*(fidderr|Background)|BackgroundActivity|AndroidRuntime|FATAL" > "$OUT/$CURRENT.log"; }
