#!/bin/bash
# Helpers for the on-device scenarios in matrix.sh. Everything goes through
# adb against one explicit device, so a connected phone is never touched by
# accident. Needs a debug build, or a release with RELEASE_TEST (run-as, debug_action.txt).
#
# Nothing waits longer than it has to: what can be asked is asked until it is true, and on
# an emulator the clock moves instead of a test waiting for the time to pass.

export MSYS_NO_PATHCONV=1
ADB="${ADB:-$LOCALAPPDATA/Android/Sdk/platform-tools/adb.exe}"
SERIAL="${SERIAL:-emulator-5554}"
PKG=com.fidderr.alarmfree
ACTIVITY=dev.dioxus.main.MainActivity
OUT="${OUT:-devtest-out}"
mkdir -p "$OUT"

# Seconds from setting up an alarm until it rings: long enough to arm it and kill the app.
LEAD="${LEAD:-6}"

# One run per device. A run that was stopped can live on, as Windows does not show bash's
# children, and it then drives the device next to the new run, which looks like the app
# failing. The newest run owns the device, and an older one ends at its next step.
OWNER_FILE="${TMPDIR:-/tmp}/af_devtest_owner_$SERIAL"
claim_device() {
    RUN_ID="$$.$RANDOM$RANDOM"
    export RUN_ID
    echo "$RUN_ID" > "$OWNER_FILE"
}
still_ours() {
    local owner=""
    [ -n "${RUN_ID:-}" ] || return 0
    read -r owner < "$OWNER_FILE" 2> /dev/null
    [ "$owner" = "$RUN_ID" ]
}
# From a subshell too: `$$` is the script itself.
give_way() {
    echo "STOP  another run took over $SERIAL: this one ends" >&2
    kill "$$" 2> /dev/null
    exit 3
}

# Nothing may hang a run: an unresponsive device fails instead of waiting forever.
adb_() {
    still_ours || give_way
    timeout 120 "$ADB" -s "$SERIAL" "$@"
}
device_ok() { [ "$(timeout 20 "$ADB" -s "$SERIAL" shell echo ok | tr -d '\r')" = ok ]; }
# The app's files are device-encrypted (direct_boot in sentinel.toml), and run-as
# starts in the credential-encrypted directory.
DATA="/data/user_de/0/$PKG"

# Two shells on the device that stay open, one as adb and one as the app. Starting adb for
# every call took 110 ms, a call in an open shell takes 30, and a scenario makes hundreds. Open
# them from the script itself, not from a subshell: what a subshell opens goes with it.
open_shells() {
    { coproc DEV_SH { "$ADB" -s "$SERIAL" shell -T sh 2>&1; }; } 2> /dev/null
    exec 5>&"${DEV_SH[1]}" 6<&"${DEV_SH[0]}"
    DEV_PID=$DEV_SH_PID
    open_app_shell
}
# The one as the app. It does not start on a phone that restarted and is still locked: once
# more after the unlock, or everything that asks the app or its page goes the slow way, which
# cannot carry a script.
open_app_shell() {
    { coproc APP_SH { "$ADB" -s "$SERIAL" shell -T "run-as $PKG sh" 2>&1; }; } 2> /dev/null
    exec 7>&"${APP_SH[1]}" 8<&"${APP_SH[0]}"
    APP_PID=$APP_SH_PID
}
DEV_PID=""
APP_PID=""
shells_open() { kill -0 "$DEV_PID" 2> /dev/null && kill -0 "$APP_PID" 2> /dev/null; }

# A command in an open shell. Its output whole, and only then: a reader that stops early must
# not leave the rest in the way of the next answer. Returns what the command returned.
ask() { # pid in out command
    local pid=$1 line text="" rest
    still_ours || give_way
    printf '{ %s\n} < /dev/null 2>&1; echo "__DONE__$?"\n' "$4" >&"$2"
    while :; do
        if ! IFS= read -r -t 90 line <&"$3"; then
            kill "$pid" 2> /dev/null
            return 124
        fi
        line=${line%$'\r'}
        case "$line" in
            *__DONE__*)
                rest=${line%__DONE__*}
                [ -z "$rest" ] || text="$text$rest"$'\n'
                printf '%s' "$text"
                return "${line##*__DONE__}"
                ;;
        esac
        text="$text$line"$'\n'
    done
}

# As adb. What gives pages of output goes the old way: reading it line by line is slow. Filter
# it on the device to have it both ways, like sound_on.
dev() {
    case "$1" in
        dumpsys | cat | uiautomator) adb_ shell "$@" | tr -d '\r' ;;
        *)
            if kill -0 "$DEV_PID" 2> /dev/null; then
                ask "$DEV_PID" 5 6 "$*"
            else
                adb_ shell "$@" | tr -d '\r'
                return "${PIPESTATUS[0]}"
            fi
            ;;
    esac
}
# As the app, in its files.
as_app() {
    if kill -0 "$APP_PID" 2> /dev/null; then
        ask "$APP_PID" 7 8 "( cd $DATA && $* )"
    else
        adb_ shell "run-as $PKG sh -c 'cd $DATA && $*'" | tr -d '\r'
        return "${PIPESTATUS[0]}"
    fi
}
# A phone has no sqlite3: PHONE_SQLITE names a tool pushed to it for this. It has to run
# from the app's own files, and clearing the app takes it along.
sql() {
    local tool=sqlite3 query
    if [ -n "${PHONE_SQLITE:-}" ]; then
        tool=$DATA/af_sqlite3
        as_app "test -x $tool || { cp $PHONE_SQLITE $tool; chmod 700 $tool; }" > /dev/null
    fi
    query=$(cat)
    as_app "print -r -- '${query//\'/\'\\\'\'}' | $tool $DATA/files/alarmfree.db"
}

# Run a check again until it holds, for at most `$1` seconds.
wait_for() {
    local limit=$((SECONDS + $1))
    shift
    until "$@"; do
        [ "$SECONDS" -ge "$limit" ] && return 1
        sleep 0.1
    done
}

main_pid() { dev pidof "$PKG"; }
sentinel_pid() { dev pidof "$PKG:sentinel"; }
# Android 12 does not let run-as kill the app, and says so only in the log. An emulator can do
# it as root.
kill_pids() {
    if on_emulator; then
        dev "su 0 kill -9 $*"
    else
        as_app "kill -9 $*"
    fi
    # Nothing but the log says when this is refused. A starved emulator can take seconds to
    # take a process down.
    local p
    for p in $*; do
        wait_for 6 pid_gone "$p" || fail "could not kill process $p"
    done
    true
}
pid_gone() { dev "[ ! -e /proc/$1 ]"; }
kill_main() { local p; p=$(main_pid); [ -n "$p" ] && kill_pids "$p"; true; }
kill_sentinel() { local p; p=$(sentinel_pid); [ -n "$p" ] && kill_pids "$p"; true; }
# In one go: with a moment in between, what survives can bring back what did not.
kill_all() { kill_pids "$(main_pid) $(sentinel_pid)"; }
no_process() { [ -z "$(main_pid)$(sentinel_pid)" ]; }
sentinel_up() { [ -n "$(sentinel_pid)" ]; }

now_s() { dev date +%s; }
# Awake, Asleep or Dozing. A picture of an asleep screen is black and says nothing.
wakefulness() { dev dumpsys power | grep -m1 -E 'mWakefulness=' | sed 's/.*=//'; }
wake() {
    dev input keyevent KEYCODE_WAKEUP
    dev wm dismiss-keyguard
    # Right after waking up that can go unheard, and the lock screen then takes every key.
    on_emulator && wait_for 3 no_keyguard
    true
}
no_keyguard() {
    dev "dumpsys window displays | grep -q 'isKeyguardShowing=false'" > /dev/null && return 0
    dev wm dismiss-keyguard
    return 1
}
# NO_LOCK: a phone with a PIN cannot be unlocked by a script, and nothing here sets or asks
# for one. It stays awake then, and being locked is tested by hand.
SCREEN_WENT_OFF=""
screen_off() {
    [ -n "${NO_LOCK:-}" ] && return 0
    dev input keyevent KEYCODE_SLEEP
    SCREEN_WENT_OFF=1
}
go_home() { dev input keyevent KEYCODE_HOME; }
screenshot() { adb_ exec-out screencap -p > "$OUT/$1${VARIANT:-}.png"; }

# ---- The clock ----
on_emulator() { case "$SERIAL" in emulator-*) return 0 ;; *) return 1 ;; esac; }
set_clock() { dev cmd alarm set-time $(($1 * 1000)) > /dev/null; }

# As if the time had passed, on an emulator. A phone's clock is left alone: there the time has
# to pass by itself, which what comes next waits for. A running app gets told, and a killed one
# wakes up for it, so a scenario that wants it dead kills it after this. An emulator's restart
# puts its clock back on the true time, which a ring before the restart has to lie before.
jump_to() {
    on_emulator && set_clock "$1"
    true
}

# The moment `lead` seconds from now, rounded up to a whole minute, as alarms have minute
# resolution. Sets T (unix s), T_H, T_M, T_DATE. An emulator's clock is put so that the whole
# minute is that near, and stays within half a minute of the true time, or of `$2` seconds
# before it. Only with the app stopped, as `fresh` leaves it: a stopped app gets no signal of
# the change.
next_minute() {
    local lead=${1:-$LEAD} n off local_s try=0 host
    while :; do
        if on_emulator && [ "$lead" -lt 600 ]; then
            host=$(($(date +%s) - ${2:-0}))
            set_clock $((host + ((-lead - 1 - host) % 60 + 90) % 60 - 30))
        fi
        n=$(now_s)
        T=$(((n + lead + 59) / 60 * 60))
        # A slow device can miss the minute it was aiming at.
        on_emulator && [ "$lead" -lt 600 ] && [ $((T - n)) -gt $((lead + 5)) ] && [ "$try" -lt 3 ] || break
        try=$((try + 1))
    done
    off=$(dev date +%z | tr -d '\r')
    # A device that does not answer must not end the run on a number that is not there.
    case "$off" in [+-][0-9][0-9][0-9][0-9]) ;; *) off=+0000 ;; esac
    ZONE_S=$((${off:0:1}(10#${off:1:2} * 3600 + 10#${off:3:2} * 60)))
    local_s=$((T + ZONE_S))
    T_H=$(date -u -d "@$local_s" +%-H)
    T_M=$(date -u -d "@$local_s" +%-M)
    T_DATE=$(date -u -d "@$local_s" +%Y-%m-%d)
    # For `arm`: the alarm seeded next is for this moment.
    ARM_AT=$T
}
ARM_AT=""

# The device's own date `$1` days from now, as the app writes it: around midnight it is not the
# host's, and not UTC's.
local_date_in() {
    local off
    off=$(dev date +%z | tr -d '\r')
    case "$off" in [+-][0-9][0-9][0-9][0-9]) ;; *) off=+0000 ;; esac
    date -u -d "@$(($(now_s) + $1 * 86400 + ${off:0:1}(10#${off:1:2} * 3600 + 10#${off:3:2} * 60)))" +%Y-%m-%d
}
# Hour and minute of a moment, on the device's clock. After next_minute.
at_hm() { date -u -d "@$(($1 + ZONE_S))" "+%-H %-M"; }

until_past() { # wait until device time is past unix second $1
    while [ "$(now_s)" -lt "$1" ]; do sleep 0.2; done
}

# ---- The device itself ----
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
    still_ours || return 0
    powershell.exe -NoProfile -Command "Start-Process -WindowStyle Hidden -FilePath '$(cygpath -w "$EMULATOR")' -ArgumentList '-avd $AVD -no-window -no-audio -no-boot-anim -no-snapshot -gpu ${EMULATOR_GPU:-host} -port ${SERIAL#emulator-}'"
}

# This emulator, and not another one running next to it.
emulator_stop() {
    still_ours || return 0
    powershell.exe -NoProfile -Command "Get-CimInstance Win32_Process | Where-Object { \$_.Name -match 'emulator|qemu' -and \$_.CommandLine -match '-port ${SERIAL#emulator-}' } | ForEach-Object { Stop-Process -Id \$_.ProcessId -Force -ErrorAction SilentlyContinue }"
    sleep 1
}

reboot_and_wait() {
    reboot_only || return 1
    open_shells
}
# Android writes "stopped" and "no longer stopped" to disk up to 10 s after the fact, measured.
# A phone saves it when shutting down, but a restart here kills the emulator: with "stopped" on
# disk the app counts as force-stopped after the boot, and gets no boot signal. For an app that
# has started since: asked from the saved file itself, which an emulator's root can read, and
# which often never said "stopped" at all.
saved_as_running() {
    dev "su 0 abx2xml /data/system/users/0/package-restrictions.xml - | tr '>' '\n' | grep 'name=\"$PKG\"'" | grep -v -q 'stopped="true"'
}
stopped_state_saved() {
    # No device, as before an emulator's first start: nothing to save.
    [ "$(timeout 5 "$ADB" -s "$SERIAL" get-state 2> /dev/null | tr -d '\r')" = device ] || return 0
    local from=$SECONDS
    if wait_for 14 saved_as_running; then
        [ $((SECONDS - from)) -lt 2 ] || note "waited $((SECONDS - from))s for Android to save that the app is not stopped"
        return 0
    fi
    local since=$((SECONDS - ${STARTED_AT:-0}))
    [ "$since" -ge 12 ] || sleep $((12 - since))
}
reboot_only() {
    case "$SERIAL" in
        emulator-*)
            stopped_state_saved
            adb_ shell sync
            emulator_stop
            emulator_start
            ;;
        *)
            timeout 60 "$ADB" -s "$SERIAL" reboot || { note "adb reboot did not return in 60s"; return 1; }
            ;;
    esac
    timeout 240 "$ADB" -s "$SERIAL" wait-for-device || { note "device did not come back"; return 1; }
    timeout 240 "$ADB" -s "$SERIAL" shell 'while [ -z "$(getprop sys.boot_completed)" ]; do sleep 0.5; done' ||
        { note "boot did not complete"; return 1; }
    # A tap from the page is no activity to Android: left to itself, an emulator's screen went
    # dark ten minutes into a run, and every picture after that was black. A phone keeps its own.
    case "$SERIAL" in
        emulator-*)
            note "screen_off_timeout was $(timeout 20 "$ADB" -s "$SERIAL" shell settings get system screen_off_timeout | tr -d '')"
            timeout 20 "$ADB" -s "$SERIAL" shell settings put system screen_off_timeout 86400000
            # It boots to its lock screen. The app shows over it, so a run can go a long way
            # before a system page, which does not, stays hidden behind it.
            timeout 20 "$ADB" -s "$SERIAL" shell wm dismiss-keyguard
            ;;
    esac
}

# ---- A clean app ----
# No alarms, normal permissions, an empty database. The first time the real way, with the app
# making its database; after that from a copy of what it made, in a second instead of ten.
CLEAN_COPY="${CLEAN_COPY:-}"
PERMISSIONS_TOUCHED=""
fresh() {
    shells_open || open_shells
    dev am force-stop "$PKG"
    [ -z "$SCREEN_WENT_OFF" ] || { wake; SCREEN_WENT_OFF=""; }
    [ -z "$PERMISSIONS_TOUCHED" ] || grant_all
    if [ -z "$CLEAN_COPY" ]; then
        # ColorOS does not let the shell clear an app: then only what the app itself owns.
        case "$(dev pm clear "$PKG" 2>&1)" in
            *Success*) ;;
            *) as_app "rm -rf ./*" ;;
        esac
        grant_all
        start_app
        wait_file files/alarmfree.db 60
        wait_for 20 on_home_or_setup
        dev am force-stop "$PKG"
        printf 'PRAGMA wal_checkpoint(TRUNCATE);\n' | sql > /dev/null
        as_app "cp files/alarmfree.db clean.db"
        CLEAN_COPY=1
    fi
    as_app 'cd files && for f in *; do [ "$f" = sounds ] || rm -rf "$f"; done; rm -rf sounds/custom ../shared_prefs; cp ../clean.db alarmfree.db'
    use_settings '"Dark"' '"English"' '"setup_done":true,"language_follows_phone":false'
}
on_home_or_setup() { is_there '.home-header, .setup-header'; }

# The app's settings: theme, language, and more preferences as JSON fields. Past the
# first-run screen and in one language, unless a scenario wants otherwise.
# MIN_VOLUME: not to have a phone ring out loud for an hour.
use_settings() {
    printf "INSERT OR REPLACE INTO settings (id, theme, locale, min_volume_percent, prefs) VALUES (1, '%s', '%s', ${MIN_VOLUME:-50}, '{%s}');\n" "$1" "$2" "$3" | sql
}

# In one go: each of these starts a program on the device.
grant_all() {
    adb_ shell "pm grant $PKG android.permission.POST_NOTIFICATIONS; pm grant $PKG android.permission.ACTIVITY_RECOGNITION; pm grant $PKG android.permission.CAMERA; appops set $PKG SYSTEM_ALERT_WINDOW allow; appops set $PKG POST_NOTIFICATION allow; appops set $PKG RUN_ANY_IN_BACKGROUND allow" > /dev/null 2>&1
    PERMISSIONS_TOUCHED=""
}
# A widget of ours on the launcher can bring the app up right after a force-stop. A clock change
# then has it read the alarms before a scenario seeded one, and opening it reads them no more.
remove_widgets() {
    # It clears the launcher: never on someone's phone.
    on_emulator || return 0
    local host
    for host in $(dev dumpsys appwidget | grep -B1 "provider=.*$PKG/" | sed -nE 's/.*host=.*pkg:([^}]+)}.*/\1/p' | sort -u); do
        dev pm clear "$host" > /dev/null
    done
}

# A phone can refuse these to the shell. Then what comes next is not about what was asked for, and
# that has to show.
deny_overlay() {
    PERMISSIONS_TOUCHED=1
    dev appops set "$PKG" SYSTEM_ALERT_WINDOW deny > /dev/null && return 0
    echo "NOTE  $CURRENT  this device does not let the overlay be denied, it stays allowed" | tee -a "$RESULTS"
    return 1
}
# Turned off by a person, in the phone's settings: a phone's shell may not do it itself.
notifications_off() {
    dev dumpsys package "$PKG" | grep -q 'POST_NOTIFICATIONS: granted=false' && return 0
    dev dumpsys notification | grep -q "AppSettings: $PKG .*importance=NONE"
}
deny_notifications() {
    PERMISSIONS_TOUCHED=1
    local denied=""
    dev pm revoke "$PKG" android.permission.POST_NOTIFICATIONS > /dev/null && denied=1
    # Android 12 has no such permission, and this is what its switch comes down to.
    dev appops set "$PKG" POST_NOTIFICATION ignore > /dev/null && denied=1
    [ -n "$denied" ]
}
# Doze, as deep as it goes. Android refuses with an alarm less than an hour away, and only what
# the command prints says so.
force_idle() {
    local said
    said=$(dev dumpsys deviceidle force-idle)
    case "$said" in
        *"Now forced"*) ;;
        *) fail "the device does not go into Doze: $said" ;;
    esac
}

start_app() {
    dev am start -n "$PKG/$ACTIVITY" > /dev/null
    STARTED_AT=$SECONDS
}

wait_file() {
    local f=$1 t=${2:-30}
    [ "$(as_app "i=0; while [ ! -e $f ] && [ \$i -lt $((t * 20)) ]; do sleep 0.05; i=\$((i + 1)); done; test -e $f && echo y")" = y ]
}

# A number instead of a wait that takes too long for a test: see platform::shortened.
shorten() { as_app "echo $2 > files/debug_$1.txt"; }

# alarm_json id hour minute days date [sound] [sound_mode] [fade] [snooze_n] [snooze_min] [challenges] [ring_limit_secs]
# ALARM_EXTRA adds JSON fields to it.
alarm_json() {
    local id=$1 h=$2 m=$3 days=$4 date=$5 sound=${6:-air} mode=${7:-0} fade=${8:-0}
    local sn=${9:-3} si=${10:-1} ch=${11:-[]} limit=${12:-1800}
    printf '{"id":"%s","time_hour":%s,"time_minute":%s,"label":"%s","days":%s,"date":%s,"skip_until_ms":null,"fade_in_secs":%s,"ring_limit_secs":%s,"order":0,"enabled":true,"sound":"%s","sound_mode":%s,"challenges":%s,"snooze_count":%s,"snooze_interval":%s%s}' \
        "$id" "$h" "$m" "$id" "$days" "$date" "$fade" "$limit" "$sound" "$mode" "$ch" "$sn" "$si" "${ALARM_EXTRA:-}"
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
    wait_file "files/sentinel/armed/$1.armed" 30
    # Whatever else was seeded comes right after.
    sleep 0.3
    # A slow start (Android optimising the app after an install, a starved emulator) can take
    # the whole lead: the minute is past by then, and the alarm armed for tomorrow. The clock
    # goes back under it, and the app, told of the change, arms it for today again.
    # Armed for the same minute tomorrow is the sign of it; anything else is another alarm.
    if on_emulator && [ -n "$ARM_AT" ] && [ "$(armed_at_ms "$1")" = "$((ARM_AT * 1000 + 86400000))" ]; then
        note "armed late, for tomorrow instead of $((ARM_AT * 1000)): the clock goes back"
        set_clock $((ARM_AT - LEAD))
        armed_for() { [ "$(armed_at_ms "$1")" = "$((ARM_AT * 1000))" ]; }
        wait_for 15 armed_for "$1" || note "still armed for $(armed_at_ms "$1")"
    fi
    ARM_AT=""
}

# Leave no app process running, the way the system leaves it overnight.
dead() {
    go_home
    kill_all
    wait_for 2 no_process && return 0
    # What a clock change left in the queue can bring a process back.
    kill_all
    wait_for 2 no_process && return 0
    # Then whatever comes next says nothing about an app that is gone.
    fail "the app survived being killed: main=$(main_pid) sentinel=$(sentinel_pid)"
    return 1
}

state() { # idle / scheduled / firing / snoozed / dismissed / none
    local s
    s=$(as_app "grep \"\\\"state\\\": \\\"\" files/sentinel/context/$1.json 2>/dev/null" | sed -E 's/.*"state": "([a-z]+)".*/\1/')
    echo "${s:-none}"
}
state_is() { [ "$(state "$1")" = "$2" ]; }
await_state() { wait_for "${3:-10}" state_is "$1" "$2"; } # id state seconds
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
# In one go: the app deletes the file once it has seen it.
action() { as_app "echo $1 > files/debug_action.tmp && mv files/debug_action.tmp files/debug_action.txt"; }

# Run JavaScript in the page and print what it returns. It is how the scenarios press buttons
# and read the screen on any phone size. One program on the device, that also waits for the
# answer: under a tenth of a second, against a second when each step was its own.
js() {
    local out
    out=$(as_app "cd files && rm -f debug_script_result.txt && print -r -- 'js:${1//\'/\'\\\'\'}' > debug_action.tmp && mv debug_action.tmp debug_action.txt && i=0; while [ ! -e debug_script_result.txt ] && [ \$i -lt 300 ]; do sleep 0.01; i=\$((i + 1)); done; cat debug_script_result.txt 2> /dev/null")
    case "$out" in
        =*)
            echo "${out#=}"
            return 0
            ;;
    esac
    # Never empty: that reads as an answer.
    echo "no answer from the page"
    return 1
}
# click selector [which of them] [seconds to wait]
click() {
    js "var e = document.querySelectorAll('$1')[${2:-0}]; if (!e) return 'nothing like $1'; e.click(); return 'ok'" > /dev/null
    sleep "${3:-0.1}"
}
# For what only looks, so it can be asked again when the page is too busy to answer.
ask_page() {
    local out try
    for try in 1 2 3; do
        out=$(js "$1")
        case "$out" in
            *"no answer"*) sleep 0.5 ;;
            *) break ;;
        esac
    done
    echo "$out"
}
# Right now, and until it shows or is gone: a screen takes a moment to change. A page that is not
# up yet does not count, once per scenario: a first load can take most of a minute when Android
# is busy with something else, which the results then say.
is_there() { [ "$(js "return document.querySelector('$1') ? 'y' : 'n'")" = y ]; }
is_not_there() { [ "$(js "return document.querySelector('$1') ? 'y' : 'n'")" = n ]; }
page_is_up() { [ "$(js "return 'y'")" = y ]; }
PAGE_WAITED=""
on_screen() {
    wait_for "${2:-6}" is_there "$1" && return 0
    [ -z "$PAGE_WAITED" ] || return 1
    PAGE_WAITED=1
    page_is_up && return 1
    local t0=$SECONDS
    wait_for 60 page_is_up || return 1
    echo "NOTE  $CURRENT  the page was not up for another $((SECONDS - t0))s" | tee -a "$RESULTS"
    wait_for "${2:-6}" is_there "$1"
}
gone() { wait_for "${2:-6}" is_not_there "$1"; }
read_text() { ask_page "var e = document.querySelector('$1'); return e ? e.textContent : ''"; }
press_back() {
    dev input keyevent KEYCODE_BACK
    sleep 0.2
}
db_count() { printf "SELECT count(*) FROM alarms WHERE data LIKE '%%%s%%';\n" "$1" | sql; }
alarm_count() { printf 'SELECT count(*) FROM alarms;\n' | sql; }

# Pretend the alarm was due `$2` minutes ago.
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
APP_UID=""
sound_on() {
    local uid=$APP_UID
    [ -n "$uid" ] || uid=$(dev cmd package list packages -U "$PKG" | sed -E 's/.*uid:([0-9]+).*/\1/')
    dev "dumpsys audio | grep -E 'u/pid:$uid/[0-9]+ .*state:started.*(USAGE_ALARM|usage=4 )' > /dev/null"
}
sound_off() { ! sound_on; }

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
        if sound_off; then
            STOPPED_AT=$((n - T))
            return 0
        fi
        sleep 0.1
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
        sleep 0.1
    done
}

# The sound is on and the app has taken it up. The app hears of the ring by a broadcast, which
# a starved emulator can hold for a while: the sound does not wait for it.
await_ring() { # id [seconds past T]
    await_sound "${2:-30}" && await_state "$1" firing 30
}

top_is_backup() { dev "dumpsys activity activities | grep -E 'topResumedActivity|ResumedActivity:' | head -1 | grep -q SentinelBackupActivity" > /dev/null; }
# The screen that asks for the unlock, on a phone that restarted and was not unlocked yet.
top_is_unlock() { dev "dumpsys activity activities | grep -E 'topResumedActivity|ResumedActivity:' | head -1 | grep -q SentinelUnlockActivity" > /dev/null; }
# The app's own screen, and not one of the SDK's, which are in the same package.
top_is_main() { dev "dumpsys activity activities | grep -E 'topResumedActivity|ResumedActivity:' | head -1 | grep -q '$PKG/$ACTIVITY'" > /dev/null; }
# What is on top, for a message.
top_activity() { dev "dumpsys activity activities | grep -E 'topResumedActivity|ResumedActivity:' | head -1 | sed -E 's/.* u0 ([^ ]+) .*/\\1/'"; }
user_unlocked() { dev "dumpsys user | grep -q RUNNING_UNLOCKED" > /dev/null; }

# Tap the plain Android view that shows this text. For a screen that is no page: nothing to ask
# there, and no script runs in it.
tap_text() {
    local box
    box=$(dev "uiautomator dump /data/local/tmp/af_ui.xml > /dev/null 2>&1; tr '>' '\\n' < /data/local/tmp/af_ui.xml | grep 'text=\"$1\"' | sed -E 's/.*bounds=\"\\[([0-9]+),([0-9]+)\\]\\[([0-9]+),([0-9]+)\\]\".*/\\1 \\2 \\3 \\4/' | head -1")
    [ -n "$box" ] || return 1
    # shellcheck disable=SC2086
    set -- $box
    dev input tap $((($1 + $3) / 2)) $((($2 + $4) / 2))
}

# The PIN a scenario set itself, typed into Android's own unlock. Only on an emulator: nothing
# here knows, sets or asks for the PIN of somebody's phone.
type_test_pin() {
    on_emulator || return 1
    # Keys typed before Android's unlock has the focus go nowhere, and it has the focus a
    # moment before its keypad is there to take them.
    wait_for 5 unlock_has_focus || note "Android's unlock does not have the focus: $(focused_window)"
    sleep 1
    dev input text 1234
    dev input keyevent 66
    if ! wait_for 5 user_unlocked; then
        note "the PIN did not go in the first time, focus: $(focused_window)"
        dev input text 1234
        dev input keyevent 66
        if ! wait_for 8 user_unlocked; then
            note "still locked. $(dev "dumpsys window | grep -i -m8 -E 'bouncer|keyguardshowing|occluded|mCurrentFocus'" | tr '\n' ' ')"
            return 1
        fi
    fi
    # The shell as the app could not start while the phone was locked.
    wait_for 10 app_shell_answers
}
# Android shows its unlock in the window of the notification shade.
unlock_has_focus() { focused_window | grep -q -E 'NotificationShade|Bouncer'; }
app_shell_answers() {
    kill -0 "$APP_PID" 2> /dev/null || open_app_shell
    sleep 0.3
    kill -0 "$APP_PID" 2> /dev/null && [ "$(as_app 'echo ok' 2> /dev/null)" = ok ]
}

# Do the backup screen's challenge: `n` long presses where its button is.
solve_backup() {
    local n=$1 size w h i=0
    size=$(dev wm size | tail -1 | sed 's/.*: //')
    w=${size%x*}
    h=${size#*x}
    while [ "$i" -lt "$n" ]; do
        dev input swipe $((w / 2)) $((h * 2 / 3)) $((w / 2)) $((h * 2 / 3)) 3300
        sleep 0.2
        i=$((i + 1))
    done
}

top_is_app() { dev "dumpsys activity activities | grep -E 'topResumedActivity|ResumedActivity:' | head -1 | grep -q $PKG" > /dev/null; }
# The window that has the keys. `dumpsys window` opens with the report of the last ANR, which has
# a line like it from back then: only the displays part says what is so now.
focused_window() { dev "dumpsys window displays | grep -m1 mCurrentFocus"; }
focus_in_app() { focused_window | grep -q "$PKG/"; }
armed_at_ms() { as_app "head -n 1 files/sentinel/armed/$1.armed 2>/dev/null"; }
os_has_alarm() { dev "dumpsys alarm | grep -qE '$PKG.*ALARM_DISPATCH|ALARM_DISPATCH.*$PKG|walarm\\*:com.mobilesentinel.ALARM_DISPATCH'" > /dev/null; }
alarm_volume() {
    dev cmd media_session volume --stream 4 --get 2> /dev/null |
        grep -oE 'volume is [0-9]+' | grep -oE '[0-9]+' | sed -n 1p
}
# Android 16 only listens to the first of these, older ones to the second.
set_alarm_volume() {
    dev cmd audio set-volume 4 "$1" > /dev/null 2>&1
    dev cmd media_session volume --stream 4 --set "$1" > /dev/null 2>&1
}

set_zone() { dev cmd alarm set-timezone "$1" > /dev/null 2>&1; }

# ---- results ----
RESULTS="$OUT/results.txt"
CURRENT=""
SCENARIO_START=$SECONDS
scenario() {
    PAGE_WAITED=""
    ARM_AT=""
    CURRENT=$1${VARIANT:-}
    SCENARIO_START=$SECONDS
    echo
    echo "=== $CURRENT"
    still_ours || give_way
    if ! device_ok; then
        echo "device not responding, stopping the run"
        echo "STOP  $CURRENT  device not responding" >> "$RESULTS"
        exit 2
    fi
    # A dark screen fails everything after it, and says nothing about the app.
    if on_emulator && shells_open && [ "$(wakefulness)" != Awake ]; then
        note "the screen was off between scenarios: woken"
        wake
    fi
    adb_ logcat -c
}
took() { echo "$((SECONDS - SCENARIO_START))s"; }
pass() { echo "PASS  $CURRENT  [$(took)] $1" | tee -a "$RESULTS"; }
# The log of what went wrong, right when it did. It takes a moment, so only then.
skip() {
    echo "SKIP  $CURRENT  [$(took)] $1" | tee -a "$RESULTS"
}
fail() {
    echo "FAIL  $CURRENT  [$(took)] $1" | tee -a "$RESULTS"
    write_log
}
note() { echo "      $1"; }
write_log() { adb_ logcat -d -b all -v time 2> /dev/null | grep -E "AlarmFree|MobileSentinel|Sentinel|AlarmKit|AlarmClass|alarmfree|ActivityTaskManager.*(fidderr|Background)|BackgroundActivity|AndroidRuntime|FATAL|am_anr|ANR in" > "$OUT/$CURRENT.log"; }
