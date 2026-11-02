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

adb_() { "$ADB" -s "$SERIAL" "$@"; }
dev() { adb_ shell "$@" | tr -d '\r'; }
as_app() { adb_ shell "run-as $PKG sh -c '$*'" | tr -d '\r'; }

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

boot_wait() {
    adb_ wait-for-device
    adb_ shell 'while [ -z "$(getprop sys.boot_completed)" ]; do sleep 1; done'
    sleep 3
}

# A clean app: no alarms, normal permissions, database created.
fresh() {
    dev am force-stop "$PKG"
    dev pm clear "$PKG" > /dev/null
    grant_all
    start_app
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

start_app() { dev am start -n "$PKG/$ACTIVITY" > /dev/null; }

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
    local lead=${1:-45} n
    n=$(now_s)
    T=$(((n + lead + 59) / 60 * 60))
    T_H=$(dev date -d "@$T" +%-H)
    T_M=$(dev date -d "@$T" +%-M)
    T_DATE=$(dev date -d "@$T" +%Y-%m-%d)
}

# alarm_json id hour minute days date [sound] [sound_mode] [fade] [snooze_n] [snooze_min] [challenges]
alarm_json() {
    local id=$1 h=$2 m=$3 days=$4 date=$5 sound=${6:-air} mode=${7:-0} fade=${8:-0}
    local sn=${9:-3} si=${10:-1} ch=${11:-[]}
    printf '{"id":"%s","time_hour":%s,"time_minute":%s,"label":"%s","days":%s,"date":%s,"skip_until_ms":null,"fade_in_secs":%s,"order":0,"enabled":true,"sound":"%s","sound_mode":%s,"challenges":%s,"snooze_count":%s,"snooze_interval":%s}' \
        "$id" "$h" "$m" "$id" "$days" "$date" "$fade" "$sound" "$mode" "$ch" "$sn" "$si"
}
EVERY_DAY='[true,true,true,true,true,true,true]'
NO_DAYS='[false,false,false,false,false,false,false]'

seed() { # seed id json (app must be stopped)
    printf "INSERT OR REPLACE INTO alarms (id, data) VALUES ('%s', '%s');\n" "$1" "$2" |
        adb_ shell "run-as $PKG sqlite3 files/alarmfree.db"
}

db_enabled() { # true / false / missing
    local v
    v=$(printf "SELECT data FROM alarms WHERE id='%s';\n" "$1" |
        adb_ shell "run-as $PKG sqlite3 files/alarmfree.db" | tr -d '\r')
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

# Is the app's alarm sound playing right now?
sound_on() {
    local uid
    uid=$(dev cmd package list packages -U "$PKG" | sed -E 's/.*uid:([0-9]+).*/\1/')
    dev dumpsys audio | grep -E "u/pid:$uid/[0-9]+ .*state:started.*USAGE_ALARM" > /dev/null
}

# Milliseconds from the alarm being delivered until the sound plays, from the
# log (needs the log since the alarm). Empty when either line is missing.
ring_ms() {
    adb_ logcat -d -v epoch 2> /dev/null | tr -d '\r' | awk '
        /MobileSentinel.AlarmRx/ && /onReceive: instance=/ && !/#watchdog/ && !rx { rx = $1 }
        /MobileSentinel.FiringAudio/ && /playing / && !play { play = $1 }
        END { if (rx && play) printf "%d", (play - rx) * 1000 }'
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

top_is_app() { dev dumpsys activity activities | grep -E "topResumedActivity|ResumedActivity:" | head -1 | grep -q "$PKG"; }
armed_at_ms() { as_app "head -n 1 files/sentinel/armed/$1.armed 2>/dev/null"; }
os_has_alarm() { dev dumpsys alarm | grep -q "$PKG.*ALARM_DISPATCH\|ALARM_DISPATCH.*$PKG\|walarm\*:com.mobilesentinel.ALARM_DISPATCH"; }
alarm_volume() { dev cmd media_session volume --stream 4 --get 2> /dev/null | grep -oE 'volume is [0-9]+' | grep -oE '[0-9]+'; }

# ---- results ----
RESULTS="$OUT/results.txt"
CURRENT=""
scenario() {
    CURRENT=$1
    echo
    echo "=== $CURRENT"
    adb_ logcat -c
}
pass() { echo "PASS  $CURRENT  $1" | tee -a "$RESULTS"; }
fail() { echo "FAIL  $CURRENT  $1" | tee -a "$RESULTS"; }
note() { echo "      $1"; }
save_log() { adb_ logcat -d -v time 2> /dev/null | grep -E "AlarmFree|MobileSentinel|Sentinel|AlarmKit|AlarmClass|alarmfree|ActivityTaskManager.*(fidderr|Background)|BackgroundActivity|AndroidRuntime|FATAL" > "$OUT/$CURRENT.log"; }
