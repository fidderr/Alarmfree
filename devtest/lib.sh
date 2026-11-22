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
ACTIVITY=android.app.NativeActivity
OUT="${OUT:-devtest-out}"
mkdir -p "$OUT"
# Sabotage mode (CHAOS=1): touches dropped and steps delayed on purpose. Nothing happens without it.
_here=${BASH_SOURCE[0]%/*}
[ "$_here" != "${BASH_SOURCE[0]}" ] || _here=.
# shellcheck disable=SC1091
. "$_here/chaos.sh"
# The state of the device before each scenario, and the watchdog around it (run_unit in the runner of matrix.sh).
# shellcheck disable=SC1091
. "$_here/baseline.sh"

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
        if ! IFS= read -r -t "${ASK_TIMEOUT:-90}" line <&"$3"; then
            echo "NOTE  no answer from the device shell to: ${4:0:160}" >&2
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
# On a real phone an `input` command goes only to AlarmFree, its keyboard or its permission dialogs: when anything else
# (the launcher, the shade, a picker) has the focus the command is refused and logged, so a touch never lands on the
# owner's own screens. Not on an emulator.
GUARD_LOG="${GUARD_LOG:-/tmp/af_guard.log}"
guard_input() {
    local f="" k=0
    while [ -z "$f" ] && [ $k -lt 6 ]; do
        f=$(timeout 8 "$ADB" -s "$SERIAL" shell dumpsys window 2> /dev/null | grep mCurrentFocus | grep -v '=null' | head -1)
        [ -n "$f" ] || sleep 0.2
        k=$((k + 1))
    done
    if echo "$f" | grep -Eq 'com\.fidderr\.alarmfree|nputMethod|inputmethod|permissioncontroller|packageinstaller|RequestIgnoreBatteryOptimizations'; then return 0; fi
    echo "$(date +%T) REFUSED [$f] $*" >> "$GUARD_LOG"
    return 1
}
dev() {
    [ -z "$CHAOS_ON" ] || chaos_touch "$@" || return 0
    case "$SERIAL:$*" in emulator-*) ;; *"input "*) guard_input "$@" || return 1 ;; esac
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
#
# Robustness: both the kill and the "is it gone yet?" poll use their own short, timeout-bounded
# adb shells, never the long-lived DEV coprocess. A loaded emulator can stall that coprocess for
# up to ASK_TIMEOUT (90 s); when that happened mid-kill the old code dropped the signal and then
# misread the stalled shell as "process still alive", failing the scenario inside a 6 s budget it
# could not even reach ("could not kill process N"). A fresh shell per attempt cannot be poisoned
# that way, and the kill is re-sent every poll so a single dropped signal is simply resent.
kill_pids() {
    local budget=$((SECONDS + ${KILL_BUDGET:-25})) p
    for p in $*; do
        [ -n "$p" ] || continue
        while :; do
            kill_pid_once "$p"
            pid_gone "$p" && break
            [ "$SECONDS" -ge "$budget" ] && { fail "could not kill process $p"; return 1; }
            sleep 0.3
        done
    done
    true
}
# One kill signal through a fresh, timeout-bounded shell: root on an emulator, run-as on a phone.
# Idempotent, so it is safe to re-send while polling (harmless once the process is already gone).
kill_pid_once() {
    if on_emulator; then
        timeout 12 "$ADB" -s "$SERIAL" shell "su 0 kill -9 $1" > /dev/null 2>&1
    else
        timeout 12 "$ADB" -s "$SERIAL" shell "run-as $PKG kill -9 $1" > /dev/null 2>&1
    fi
    return 0
}
# True once PID is gone. A fresh short shell prints a definite token either way; an empty or
# garbled answer (device momentarily unresponsive) counts as "not gone" so we keep trying rather
# than declare a live process dead.
pid_gone() {
    local out
    out=$(timeout 10 "$ADB" -s "$SERIAL" shell "[ -e /proc/$1 ] && echo __LIVE__ || echo __GONE__" 2> /dev/null | tr -d '\r')
    case "$out" in
        *__GONE__*) return 0 ;;
        *) return 1 ;;
    esac
}
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
# On a phone a picture is taken only while AlarmFree has the focus: never the owner's file picker,
# notification shade or another app.
screen_may_be_taken() { on_emulator || focus_in_app; }
screenshot() {
    screen_may_be_taken || { note "no picture $1: AlarmFree is not in front"; return 0; }
    adb_ exec-out screencap -p > "$OUT/$1${VARIANT:-}.png"
}

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
AVD="${AVD:-afg16}"
EMULATOR="${EMULATOR:-$LOCALAPPDATA/Android/Sdk/emulator/emulator.exe}"

[ "$(uname -s)" = Linux ] && . "$(dirname "${BASH_SOURCE[0]}")/linux.sh"

emulator_start() {
    still_ours || return 0
    [ "$(uname -s)" = Linux ] && { linux_emulator_start; return; }
    powershell.exe -NoProfile -Command "Start-Process -WindowStyle Hidden -FilePath '$(cygpath -w "$EMULATOR")' -ArgumentList '-avd $AVD -no-window -no-audio -no-boot-anim -no-snapshot -gpu ${EMULATOR_GPU:-host} -port ${SERIAL#emulator-}'"
}

# This emulator, and not another one running next to it.
emulator_stop() {
    still_ours || return 0
    [ "$(uname -s)" = Linux ] && { linux_emulator_stop; return; }
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
# Android resizes a new window about a second after its first frame, and a key or touch in that moment is lost: wait
# until 3 s have passed since the app was started, and the layout holds still.
window_past_late_resize() {
    local since=$((SECONDS - ${STARTED_AT:-0}))
    [ "$since" -ge 3 ] || sleep $((3 - since))
    UI_QUIET=1 await_settled 5
}
saved_as_running() {
    # Android 12 and up write the file as binary XML (abx2xml reads it); Android 10 and 11 write plain XML.
    dev "{ su 0 abx2xml /data/system/users/0/package-restrictions.xml - 2> /dev/null || su 0 cat /data/system/users/0/package-restrictions.xml; } | tr '>' '\n' | grep 'name=\"$PKG\"'" | grep -v -q 'stopped="true"'
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
    # A restart that is meant is no finding of the watchdog.
    watchdog_announce_restart
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
    # Asked again when the connection drops: adbd restarts early in a boot ("error: closed").
    local boot_deadline=$((SECONDS + 240))
    until [ "$(timeout 20 "$ADB" -s "$SERIAL" shell getprop sys.boot_completed 2> /dev/null | tr -d '\r')" = 1 ]; do
        [ "$SECONDS" -lt "$boot_deadline" ] || { note "boot did not complete"; return 1; }
        sleep 1
    done
    # A tap from the page is no activity to Android: left to itself, an emulator's screen went
    # dark ten minutes into a run, and every picture after that was black. A phone keeps its own.
    case "$SERIAL" in
        emulator-*)
            note "screen_off_timeout was $(timeout 20 "$ADB" -s "$SERIAL" shell settings get system screen_off_timeout | tr -d '
')"
            timeout 20 "$ADB" -s "$SERIAL" shell settings put system screen_off_timeout 86400000
            # It boots to its lock screen. The app shows over it, so a run can go a long way
            # before a system page, which does not, stays hidden behind it.
            timeout 20 "$ADB" -s "$SERIAL" shell wm dismiss-keyguard
            # A user has no accessibility service: the suite runs without one (A11Y=on keeps what the emulator has,
            # for a run with a screen reader's tree being built).
            if [ "${A11Y:-off}" = off ]; then
                timeout 20 "$ADB" -s "$SERIAL" shell settings put secure enabled_accessibility_services null
                timeout 20 "$ADB" -s "$SERIAL" shell settings put secure accessibility_enabled 0
            fi
            ;;
    esac
    # The new processes are the ones the watchdog holds the rest of the scenario to.
    watchdog_rebase
}

# ---- A clean app ----
# No alarms, normal permissions, an empty database. The first time the real way, with the app
# making its database; after that from a copy of what it made, in a second instead of ten.
CLEAN_COPY="${CLEAN_COPY:-}"
PERMISSIONS_TOUCHED=""
fresh() {
    shells_open || open_shells
    dev am force-stop "$PKG"
    # A soft keyboard that the last scenario's field left up outlives the app on Android 12, and
    # covers the lower half of the next one.
    on_emulator && hide_keyboard
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
on_home_or_setup() { page_is_up; }

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

# Android 13 tells even a stopped app of a clock change (TIME_SET), and the process that starts for it can read
# the database before a row is seeded: the app started afterwards keeps what that process read (nothing armed,
# "state=none"). So an app that is up while rows were seeded is stopped before it starts.
start_app() {
    if [ -n "${SEEDED:-}" ]; then
        SEEDED=""
        no_process || dev am force-stop "$PKG"
    fi
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
    SEEDED=1
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
    # On a starved emulator Android can kill the starting app ("start timeout") before it arms
    # anything: start it again rather than wait for a ring that was never set.
    local try
    for try in 1 2 3; do
        start_app
        wait_file "files/sentinel/armed/$1.armed" 30 && break
        note "alarm $1 not armed 30s after start $try (main=$(main_pid))"
    done
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

# ---- The window (Slint) ----
# The app answers commands written to files/debug_action.txt (src/debug_hooks.rs, src/ui/debug*.rs;
# a debug build made by ./build.sh). One program on the device writes the command and waits for the
# answer: under a tenth of a second.
# A text for inside single quotes in a shell: each quote becomes quote, backslash, quote, quote. Done
# in a loop: how bash reads a quote in the replacement of ${var//x/y} differs between versions (5.3
# made three quotes of it, which left the quote open and the shell on the device waiting for the
# rest: "no answer from the device shell" after 90 s, on any text with an apostrophe).
sq_escape() {
    local rest=$1 out="" q="'"
    while [[ $rest == *"$q"* ]]; do
        out+="${rest%%"$q"*}'\\''"
        rest=${rest#*"$q"}
    done
    printf '%s' "$out$rest"
}
ui() { # command: its answer, or "no answer from the app"
    local out
    [ -z "$CHAOS_ON" ] || chaos_step
    out=$(as_app "cd files && rm -f debug_script_result.txt && print -r -- 'ui:$(sq_escape "$1")' > debug_action.tmp && mv debug_action.tmp debug_action.txt && i=0; while [ ! -e debug_script_result.txt ] && [ \$i -lt ${UI_WAIT:-300} ]; do sleep 0.01; i=\$((i + 1)); done; cat debug_script_result.txt 2> /dev/null")
    if [ -n "$out" ]; then
        echo "$out"
        return 0
    fi
    echo "no answer from the app"
    return 1
}

# ---- Readiness signals ----
# The app tells when it is ready (src/ui/ready.rs), so a scenario waits for that and not for a
# guess. `ui:await` runs inside the app: it answers when every condition holds and the layout has
# stood still for a moment (no panel sliding, nothing fading), or when the time is up.
# Conditions: key=value or key!=value of `ui ready` (screen, home_sheet, panel, editor_sheet,
# theme_sheet, ringing, toast, first_frame), key>N, text=, textc=, gone=. UI_QUIET=1: no message.
await_ms() { # milliseconds condition...
    local ms=$1 out
    shift
    out=$(UI_WAIT=$((ms / 10 + 400)) ui "await:$ms:$(IFS=';'; echo "$*")")
    case "$out" in OK*) return 0 ;; esac
    [ -n "${UI_QUIET:-}" ] || echo "WAIT  $CURRENT  gave up after ${ms}ms waiting for [$*]: $out" >&2
    return 1
}
await_ui() { local s=$1; shift; await_ms $((s * 1000)) "$@"; }
await_settled() { await_ui "${1:-3}"; }
await_first_frame() { await_ui "${1:-30}" 'first_frame=1'; }
await_screen() { await_ui "${2:-6}" "screen=$1"; }
await_sheet_open() { await_ui "${1:-6}" 'home_sheet!=None'; }
await_sheet_closed() { await_ui "${1:-6}" 'home_sheet=None'; }
await_panel_open() { await_ui "${1:-6}" 'panel=true'; }
await_panel_closed() { await_ui "${1:-6}" 'panel=false'; }
await_text() { await_ui "${2:-6}" "text=$1"; }
await_text_has() { await_ui "${2:-6}" "textc=$1"; }
await_gone() { await_ui "${2:-6}" "gone=$1"; }
ui_frames() { ui ready | sed -nE 's/.*frames=([0-9]+).*/\1/p'; }
# After a touch or key: the app has drawn something new (a touch that changes nothing draws
# nothing: then the short wait ends quietly) and the layout stands still.
# SETTLED_AT: the second in which the window was last seen at rest, so the next touch need not
# ask again.
SETTLED_AT=""
after_input() {
    await_ms 5000 '~moved'
    SETTLED_AT=$SECONDS
}
# Type into text field n and check what the field holds, again (up to 3 times) when it does not
# hold the text: the keys go through the window's own events (settext), and the check is the
# element channel's reading of the field, not a hope.
field_value() { ui inputs | python -I -c '
import json, sys
rows = json.loads(sys.stdin.read())
n = int(sys.argv[1])
print(rows[n]["t"] if n < len(rows) else "")
' "$1" 2> /dev/null; }

# Type text with the phone's own keyboard (`adb input text` drops characters on some Androids:
# "aMoring"). The text goes in short pieces, then the field is read back through the element
# channel; when it does not hold the text it is emptied and typed again in shorter pieces.
type_real() { # field text
    local n=$1 text=$2 try=0 chunk=4 i got dels="67 67 67 67 67 67 67 67 67 67 67 67 67 67 67 67 67 67 67 67"
    while [ "$try" -lt 3 ]; do
        i=0
        while [ "$i" -lt "${#text}" ]; do
            dev input text "${text:i:chunk}"
            i=$((i + chunk))
        done
        UI_QUIET=1 await_settled 3
        got=$(field_value "$n")
        case "${got,,}" in *"${text,,}"*) return 0 ;; esac
        note "typed [$text] but the field holds [$got]: emptied, typed again in pieces of $((chunk > 1 ? chunk / 2 : 1))"
        dev input keyevent KEYCODE_MOVE_END
        # shellcheck disable=SC2086
        dev input keyevent $dels
        dev input keyevent $dels
        chunk=$((chunk > 1 ? chunk / 2 : 1))
        try=$((try + 1))
    done
    echo "the field does not take [$text]: it holds [$(field_value "$n")]" >&2
    return 1
}

# A property of the window from the snapshot: numbers, booleans and strings (not the lists).
sn() {
    ui snapshot | sed -nE 's/.*"'"$1"'":("([^"]*)"|([^,}]*)).*/\2\3/p' | head -1
}
# Which screen: 0 alarms, 1 editor, 2 history, 3 settings, 4 reliability, 5 theme.
screen_no() { ui screen | sed -nE 's/^screen=([0-9]+).*/\1/p'; }
on_screen_no() { [ "$(screen_no)" = "$1" ]; }
# A button of the window by its callback name, as a tap would (see src/ui/debug.rs).
press() { # callback [argument]
    local said
    said=$(ui "press:$1:${2:-}")
    [ "$said" = OK ] || echo "press $1: $said" >&2
    # The callback has run when the app says OK; what it moved is still settling.
    UI_QUIET=1 await_settled 3
    SETTLED_AT=$SECONDS
    [ "$said" = OK ]
}
# The middle of the first text on screen that says this, in device pixels ("x y"), or "none".
where() { ui "find${2:+@$2}:$1"; }
where_has() { ui "findc${2:+@$2}:$1"; }
is_there() {
    case "$(where "$1" "${2:-}")" in
        [0-9]*) return 0 ;;
    esac
    return 1
}
is_not_there() { [ "$(where "$1")" = none ]; }
has_there() {
    case "$(where_has "$1")" in
        [0-9]*) return 0 ;;
    esac
    return 1
}
page_is_up() { [ "$(ui window)" != "no answer from the app" ]; }
# Right now, and until it shows or is gone: a screen takes a moment to change. A window that is not
# up yet does not count, once per scenario: a first start can take most of a minute when Android is
# busy with something else, which the results then say.
PAGE_WAITED=""
on_screen() { # text [seconds]
    UI_QUIET=1 await_text "$1" "${2:-6}" && return 0
    [ -z "$PAGE_WAITED" ] || return 1
    PAGE_WAITED=1
    page_is_up && return 1
    local t0=$SECONDS
    UI_QUIET=1 await_first_frame 60 || return 1
    echo "NOTE  $CURRENT  the window was not up for another $((SECONDS - t0))s" | tee -a "$RESULTS"
    UI_QUIET=1 await_text "$1" "${2:-6}"
}
on_screen_has() { wait_for "${2:-6}" has_there "$1"; }
gone() { UI_QUIET=1 await_gone "$1" "${2:-6}"; }
# Every text on screen, one per line, top to bottom.
all_texts() { ui texts | grep -oE '"t":"([^"\]|\.)*"' | sed -E 's/^"t":"//; s/"$//; s/\\"/"/g'; }
texts_of() { all_texts | tr '\n' '|' | sed 's/|$//'; }
# A touch where the text is: the middle of the first text that says this (the n-th, counting
# from 0), as a finger does it. With `c` in front of the pause, the text only has to have this in it.
# Where the text is once it holds still: a panel slides in, and a touch on the way would miss.
still_where() { # command text [which]
    [ "$SETTLED_AT" = "$SECONDS" ] || UI_QUIET=1 await_settled 2
    "$1" "$2" "${3:-}"
}
# A touch through the app: the app finds the element and delivers the press through its own window
# events, and answers after it ran. Nothing is sent from outside, so no touch is lost on the way and no
# position can be out of date. It fails when the app says the element is not there. A real touch is
# `finger`: for where the touch itself is the subject, and for a text field that has to bring up the
# keyboard (only a real touch does).
tap_through() { # command, what to say when it fails
    local said
    [ "$SETTLED_AT" = "$SECONDS" ] || UI_QUIET=1 await_settled 2
    said=$(ui "$1")
    UI_QUIET=1 await_settled 3
    SETTLED_AT=$SECONDS
    [ "$said" = OK ] && return 0
    echo "$2 (the app said: $said)" >&2
    return 1
}
tap() { tap_through "tap${2:+@$2}:$1" "nothing says $1"; } # text [which of them] [seconds, not read]
# The time of the editor, which a touch there opens the time sheet.
tap_time() { tap_has "Time:" 0 "${1:-0.6}"; }
tap_has() { tap_through "tapc${2:+@$2}:$1" "nothing has $1"; } # text [which] [seconds, not read]
tap_in() { tap "$@"; }
# A touch at a place, in device pixels.
tap_xy() { tap_through "tapxy:$1 $2" "no touch at $1 $2"; } # x y
# The real touch, from outside the app: read where the text is, once it holds still, and press there with a
# finger. Only for what the touch itself is the subject of, and for a text field that brings up the keyboard.
finger() { # text [which of them] [seconds to wait]
    local xy
    xy=$(still_where where "$1" "${2:-}")
    case "$xy" in
        [0-9]*) ;;
        *)
            echo "nothing says $1" >&2
            return 1
            ;;
    esac
    # shellcheck disable=SC2086
    dev input tap $xy
    after_input
}
finger_has() { # text [which] [seconds]
    local xy
    xy=$(still_where where_has "$1" "${2:-}")
    case "$xy" in
        [0-9]*) ;;
        *)
            echo "nothing has $1" >&2
            return 1
            ;;
    esac
    # shellcheck disable=SC2086
    dev input tap $xy
    after_input
}
click_text() { tap "$@"; }
# The n-th text field on screen takes this text.
set_field() { type_field "$@"; }
# The same through the keys, which runs the field's own handler for what is typed.
type_field() { # n text
    local said try=0
    # The keyboard moves the sheet: tap a field only once the layout stands still, or the tap
    # lands where the field was (the second text went into the first field).
    await_settled 6 || return 1
    while [ "$try" -lt 3 ]; do
        said=$(ui "settext:$1:$2")
        [ "$said" = OK ] || { echo "no field $1" >&2; return 1; }
        await_settled 3
        # What the field says now, from the element channel: not what was sent.
        [ "$(field_value "$1")" = "$2" ] && return 0
        try=$((try + 1))
        note "field $1 holds [$(field_value "$1")], not [$2]: typed again"
    done
    echo "field $1 does not take [$2]: it holds [$(field_value "$1")]" >&2
    return 1
}
fields() { ui inputs; }
# Slider n (from the top) to this fraction (0 to 1) of its track, by a touch there (through the app).
set_slider() { # n fraction
    local row x y w scale
    row=$(ui boxes:AppSlider | grep -oE '\{[^{}]*\}' | sed -n "$(($1 + 1))p")
    [ -n "$row" ] || { echo "no slider $1" >&2; return 1; }
    x=$(echo "$row" | sed -E 's/.*"x":([0-9]+).*/\1/')
    y=$(echo "$row" | sed -E 's/.*"y":([0-9]+).*/\1/')
    w=$(echo "$row" | sed -E 's/.*"w":([0-9]+).*/\1/')
    scale=$(window_size | cut -d' ' -f3)
    tap_xy "$(awk -v x="$x" -v w="$w" -v s="$scale" -v f="$2" 'BEGIN { printf "%d", x - w / 2 + 12 * s + (w - 24 * s) * f }')" "$y"
}
# The number of a choice in a list of radio rows (from the top), by its text.
radio_index() {
    ui texts | python -I -c '
import json, sys
want = sys.argv[1]
rows = [e for e in json.loads(sys.stdin.read()) if e["r"] == "RadioButton"]
print(next((i for i, e in enumerate(rows) if e["t"] == want), -1))
' "$1"
}
# Is a choice, chip or switch with this label on (checked or selected)?
is_on() { ui texts | grep -oE '\{[^{}]*"t":"'"$1"'"[^{}]*\}' | grep -q '"on":true'; }
has_role() { ui texts | grep -oE '\{[^{}]*"t":"'"$1"'"[^{}]*\}' | grep -q "\"r\":\"$2\""; }
# Device pixels of the window: "width height scale".
window_size() { ui window; }

press_back() {
    dev input keyevent KEYCODE_BACK
    after_input
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
            # Silent for good: the player that stalls hands over to the next sound, with a gap.
            sleep 1.5
            if sound_off; then
                STOPPED_AT=$((n - T))
                return 0
            fi
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
    local try
    for try in 1 2 3; do
        box=$(dev "uiautomator dump /data/local/tmp/af_ui.xml > /dev/null 2>&1; tr '>' '\\n' < /data/local/tmp/af_ui.xml | grep 'text=\"$1\"' | sed -E 's/.*bounds=\"\\[([0-9]+),([0-9]+)\\]\\[([0-9]+),([0-9]+)\\]\".*/\\1 \\2 \\3 \\4/' | head -1")
        [[ "$box" =~ ^[0-9]+\ [0-9]+\ [0-9]+\ [0-9]+$ ]] && break
        retry_used "tap_text $1: the screen dump failed"
        sleep 1
    done
    # A dump that failed leaves an error text (sh: can't open ...) where the numbers go; in $(( )) that ends the whole run.
    [[ "$box" =~ ^[0-9]+\ [0-9]+\ [0-9]+\ [0-9]+$ ]] || return 1
    # shellcheck disable=SC2086
    set -- $box
    TAPPED_AT="$((($1 + $3) / 2)) $((($2 + $4) / 2))"
    # shellcheck disable=SC2086
    dev input tap $TAPPED_AT
}
# Where tap_text last tapped, without looking the screen over again, which takes seconds.
TAPPED_AT=""
tap_again() {
    [ -n "$TAPPED_AT" ] || return 1
    # shellcheck disable=SC2086
    dev input tap $TAPPED_AT
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
    # Android 16 leaves its keyboard up over whatever comes next after the PIN.
    hide_keyboard
    # The shell as the app could not start while the phone was locked.
    wait_for 10 app_shell_answers || return 1
    # The alarm's own screen starts a moment after the PIN; a scenario that looks at it right away finds no window.
    wait_for 20 page_is_up || true
}
# Android shows its unlock in the window of the notification shade.
unlock_has_focus() { focused_window | grep -q -E 'NotificationShade|Bouncer'; }
# The alarm's own unlock screen, once Android's unlock has given the focus back: a tap before that goes nowhere.
unlock_screen_has_focus() { focused_window | grep -q SentinelUnlockActivity; }
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
# Android 10 has the old `media volume` command, Android 12 `cmd media_session volume`.
alarm_volume() {
    local v
    v=$({ dev cmd media_session volume --stream 4 --get 2> /dev/null
      dev media volume --stream 4 --get 2> /dev/null; } |
        grep -oE 'volume is [0-9]+' | grep -oE '[0-9]+' | sed -n 1p)
    # Android 11 has neither command printing it: the audio service's own report does.
    [ -n "$v" ] || v=$(dev dumpsys audio | sed -n '/- STREAM_ALARM:/,/streamVolume/p' | grep -oE 'streamVolume:[0-9]+' | grep -oE '[0-9]+' | sed -n 1p)
    echo "$v"
}
# Android 16 only listens to the first of these, Android 12 to the second, Android 10 to the third.
set_alarm_volume() {
    dev cmd audio set-volume 4 "$1" > /dev/null 2>&1
    dev cmd media_session volume --stream 4 --set "$1" > /dev/null 2>&1
    dev media volume --stream 4 --set "$1" > /dev/null 2>&1
    # Android 14 refuses those for the alarm stream (AppOps: "com.android.server.media" is no package of the shell's
    # uid): the audio service's own call, as the shell's package, works (setStreamVolume is transaction 12 there).
    if [ "$(alarm_volume)" != "$1" ] && [ "$(dev getprop ro.build.version.sdk | tr -d '
')" = 34 ]; then
        dev service call audio 12 i32 4 i32 "$1" i32 0 s16 com.android.shell > /dev/null 2>&1
    fi
}

# What the app saved in Downloads has a row in MediaStore. Removing only the file leaves it, and
# Android 10 then saves the next one of that name as "name (1).txt".
rm_download() {
    local name
    for name in "$@"; do
        dev rm -f "/sdcard/Download/$name"
        dev "content delete --uri content://media/external/downloads --where \"_display_name like '${name//\*/%}'\"" > /dev/null 2>&1
    done
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
    RETRY_NOTE=$RETRY_CARRY RETRY_CARRY=""
    SCENARIO_START=$SECONDS
    echo
    echo "=== $CURRENT"
    still_ours || give_way
    if ! device_ok; then
        echo "device not responding, stopping the run"
        echo "STOP  $CURRENT  device not responding" >> "$RESULTS"
        exit 2
    fi
    chaos_hog_ensure
    # A dark screen fails everything after it, and says nothing about the app.
    if on_emulator && shells_open && [ "$(wakefulness)" != Awake ]; then
        note "the screen was off between scenarios: woken"
        wake
    fi
    # A phone's own app that stopped answering leaves its dialog over everything, and no touch gets through.
    if on_emulator && shells_open && focused_window | grep -q "Not Responding"; then
        note "a system app did not respond: its dialog is closed"
        dev am force-stop com.android.settings
        dev input keyevent KEYCODE_BACK
    fi
    adb_ logcat -c
}
took() { echo "$((SECONDS - SCENARIO_START))s"; }
pass() { echo "PASS  $CURRENT  [$(took)] $1${RETRY_NOTE:+  (ASKED AGAIN: $RETRY_NOTE)}" | tee -a "$RESULTS"; }
# A scenario that asks again (a lost touch, a machine that used the limit up) says so in its result line, and a retry on a
# machine that was not starved is a finding: the host's load against its cores is read at that moment (a runner; a PC says
# "load unknown"). One line per use in retries.txt of the results, which the CI table lists.
RETRY_NOTE="" RETRY_CARRY=""
retry_used() { # what was asked again, and why
    local load cores verdict=unknown
    load=$(cut -d' ' -f1 /proc/loadavg 2> /dev/null)
    cores=$(nproc 2> /dev/null)
    if [ -n "$load" ] && [ -n "$cores" ]; then
        if awk -v l="$load" -v c="$cores" 'BEGIN { exit !(l >= c) }'; then verdict=starved; else verdict=NOT-STARVED; fi
    fi
    RETRY_NOTE="${RETRY_NOTE:+$RETRY_NOTE, }$1 [host load ${load:-?}/${cores:-?}: $verdict]"
    echo "$CURRENT | $1 | host load ${load:-?} of ${cores:-?} cores: $verdict" >> "$OUT/retries.txt"
    note "ASKED AGAIN: $1 (host load ${load:-?}/${cores:-?}: $verdict)"
}
# The log of what went wrong, right when it did. It takes a moment, so only then.
skip() {
    echo "SKIP  $CURRENT  [$(took)] $1" | tee -a "$RESULTS"
}
fail() {
    echo "FAIL  $CURRENT  [$(took)] $1${RETRY_NOTE:+  (ASKED AGAIN: $RETRY_NOTE)}" | tee -a "$RESULTS"
    write_log
}
note() { echo "      $1"; }
write_log() { adb_ logcat -d -t 30000 -b all -v time 2> /dev/null | grep -E "AlarmFree|MobileSentinel|Sentinel|AlarmKit|AlarmClass|alarmfree|ActivityTaskManager.*(fidderr|Background)|BackgroundActivity|AndroidRuntime|FATAL|am_anr|ANR in|WATCHDOG|Watchdog|system_server|audioserver|AudioFlinger|AudioService|Fatal signal" > "$OUT/$CURRENT.log"; anr_traces; app_story; }
# What the app itself says happened to its rings: its diagnostic log (ring started / stopped and how) and the outcomes it
# wrote to the history (times and outcomes only, no labels). Decides whether a ring that ended by itself was dismissed,
# missed, ended by the ring limit or by a restart.
app_story() {
    {
        echo "--- files/sentinel/diag.log"
        as_app "cat files/sentinel/diag.1.log files/sentinel/diag.log 2>/dev/null | tail -n 60"
        echo "--- alarm_events (alarm_id fire dismiss outcome)"
        printf 'SELECT alarm_id, fire_timestamp, dismiss_timestamp, outcome FROM alarm_events ORDER BY fire_timestamp DESC LIMIT 10;
' | sql
    } 2> /dev/null > "$OUT/$CURRENT.story.txt"
}
# What the system wrote about a not-responding app (stacks of its threads), kept with the log of a failed scenario.
anr_traces() {
    timeout 60 "$ADB" -s "$SERIAL" shell "su 0 sh -c 'tail -n +1 /data/anr/* 2>/dev/null'" 2> /dev/null | head -c 400000 > "$OUT/$CURRENT.anr.txt"
    [ -s "$OUT/$CURRENT.anr.txt" ] || rm -f "$OUT/$CURRENT.anr.txt"
    # Is the audio server alive, and has anything crashed (Android 11 on a runner: the audio policy service did not answer)?
    timeout 60 "$ADB" -s "$SERIAL" shell "getprop init.svc.audioserver; ps -A | grep -i -E 'audioserver|media'; logcat -d -b crash | tail -40; logcat -d -b all -s audioserver:* AudioFlinger:* AudioPolicyService:* | tail -40" 2> /dev/null > "$OUT/$CURRENT.audio.txt"
}

# One row of history, `$2` days ago.
history_row() { # id days_ago
    local at now at_s
    now=$(now_s)
    # 07:00 UTC of that day, but never in the future (today before 07:00 UTC): a period lists only what has happened.
    at_s=$(date -u -d "$(date -u -d "@$((now - $2 * 86400))" +%Y-%m-%d)T07:00:00Z" +%s)
    [ "$at_s" -le $((now - 600)) ] || at_s=$((now - 600))
    at=$(date -u -d "@$at_s" +%Y-%m-%dT%H:%M:%S+00:00)
    printf "INSERT INTO alarm_events (id, alarm_id, alarm_label, fire_timestamp, dismiss_timestamp, total_dismissal_time_ms, outcome, seen) VALUES ('%s', 'a', 'a', '%s', '%s', 0, 'solved', 1);\n" "$1" "$at" "$at" | sql
}
history_count() { printf "SELECT count(*) FROM alarm_events;\n" | sql; }

# The bottom bar: Alarms, History or Settings.
go_tab() { # Alarms / History / Settings; a touch in the first seconds of the window can be lost: again
    local want i=0
    case "$1" in
        Alarms | Wekkers) want=0 ;; # Wekkers: the Dutch name of the tab, as s51 asks for it with the language on Dutch
        History) want=2 ;;
        *) want=3 ;;
    esac
    while [ "$i" -lt 3 ]; do
        [ "$i" = 0 ] || retry_used "go_tab $1: the first touch did not change the screen"
        tap "$1" L 0.3
        wait_for 2 on_screen_no "$want" && return 0
        i=$((i + 1))
    done
    return 1
}

# Scroll the panel on top (or the screen) up by about a screen, as a finger does: 1100 px, from
# 1800 px as before, but from no lower than 80% of the window. On a window of 1920 px 1800 is in the
# gesture area at the bottom, where an upward swipe is the Home gesture and takes the app away; and
# a shorter swipe than 1100 px leaves rows of the editor below the window.
swipe_ys() { # scroll | back -> x y x y in device pixels
    local size w h from to
    size=$(window_size)
    w=$(echo "$size" | cut -d' ' -f1)
    h=$(echo "$size" | cut -d' ' -f2)
    case "$w$h" in *[!0-9]* | "") w=1080 h=1920 ;; esac
    from=$((h * 80 / 100))
    [ "$from" -le 1800 ] || from=1800
    to=$((from - 1100))
    [ "$to" -ge $((h * 4 / 100)) ] || to=$((h * 4 / 100))
    if [ "$1" = back ]; then
        echo "$((w / 2)) $to $((w / 2)) $from"
    else
        echo "$((w / 2)) $from $((w / 2)) $to"
    fi
}
sheet_scroll() {
    # shellcheck disable=SC2046
    dev input swipe $(swipe_ys scroll) 400
    after_input
}
# A tap on what the text names, once it is clear of the bottom of the window: on a window with a
# navigation bar (3-button navigation) or a Done button that stays at the bottom of a panel, what is
# there takes the touch and the control does nothing. With "has" the text only has to be in it.
# The margin (default 100 px) is how far above the bottom edge it has to be.
tap_clear() { # text [has] [margin]
    local i y h find=where tapper=tap margin=${3:-100}
    [ "${2:-}" != has ] || { find=where_has; tapper=tap_has; }
    h=$(window_size | cut -d' ' -f2)
    case "$h" in '' | *[!0-9]*) h=1920 ;; esac
    for i in 1 2 3; do
        y=$($find "$1" | cut -d' ' -f2)
        case "$y" in '' | *[!0-9]*) ;; *) [ "$y" -lt $((h - margin)) ] && break ;; esac
        sheet_scroll
    done
    $tapper "$1" 0 0.5
}
# set_field for a field that may lie under the panel's pinned Done button (3-button navigation and a
# panel that is longer than the window): the panel is scrolled up first, from the gutter at its left
# edge, where no slider or field takes the swipe.
set_field_clear() { # n text
    local i f d h x1 y1 x2 y2
    h=$(window_size | cut -d' ' -f2)
    case "$h" in '' | *[!0-9]*) h=1920 ;; esac
    for i in 1 2 3; do
        f=$(ui inputs | grep -oE '"y":[0-9]+' | sed -n "$(($1 + 1))p" | tr -dc 0-9)
        d=$(where Done | cut -d' ' -f2)
        case "$d" in '' | *[!0-9]*) d=$((h - 200)) ;; esac
        case "$f" in '' | *[!0-9]*) break ;; esac
        [ "$f" -lt $((d - 80)) ] && break
        read -r x1 y1 x2 y2 <<< "$(swipe_ys scroll)"
        dev input swipe 26 "$y1" 26 "$y2" 400
        after_input
    done
    set_field "$1" "$2"
}
# A short scroll: about a quarter of the window, where sheet_scroll's fling may take a screen and a half.
sheet_nudge() {
    local h
    h=$(window_size | cut -d' ' -f2)
    case "$h" in '' | *[!0-9]*) h=1920 ;; esac
    dev input swipe 540 $((h * 70 / 100)) 540 $((h * 45 / 100)) 700
    after_input
}
# A row of a long list into the middle of the window, by short swipes (a fling carries a long one
# past it, by a different distance each time): there it is neither under the title nor under the
# bottom bar. With "has" the text only has to be in it.
centre_row() { # text [has]
    local i y h w find=where
    [ "${2:-}" != has ] || find=where_has
    h=$(window_size | cut -d' ' -f2)
    case "$h" in '' | *[!0-9]*) h=1920 ;; esac
    w=540
    for i in 1 2 3 4 5 6 7 8; do
        y=$($find "$1" | cut -d' ' -f2)
        case "$y" in
            '' | *[!0-9]*)
                # Not in the window: below it first, then above.
                if [ "$i" -le 4 ]; then
                    dev input swipe "$w" $((h * 70 / 100)) "$w" $((h * 45 / 100)) 700
                else
                    dev input swipe "$w" $((h * 45 / 100)) "$w" $((h * 70 / 100)) 700
                fi
                ;;
            *)
                if [ "$y" -gt $((h * 75 / 100)) ]; then
                    dev input swipe "$w" $((h * 70 / 100)) "$w" $((h * 50 / 100)) 700
                elif [ "$y" -lt $((h * 20 / 100)) ]; then
                    dev input swipe "$w" $((h * 45 / 100)) "$w" $((h * 65 / 100)) 700
                else
                    return 0
                fi
                ;;
        esac
        after_input
    done
    return 1
}
# Scroll the panel until this exact text (the n-th, or L for the last) is on screen.
scroll_to() { # text [which]
    local i
    for i in 1 2 3 4; do
        is_there "$1" "${2:-}" && return 0
        sheet_scroll
    done
    return 1
}
# Tap what the text names, scrolling the panel up first when it is not on screen yet.
# Up to four screens down: the settings list is longer than one (Help and feedback sits above Data).
tap_scrolling() { # text
    local i
    for i in 1 2 3 4; do
        wait_for 2 has_there "$1" && break
        sheet_scroll
    done
    tap_has "$1" 0 0.3
}
# A phone's keyboard stays up over a sheet's Done button: Back puts it away (an emulator has none).
# Asked twice: the input method's own dump can say it is hidden while it still covers the window (the Done button
# under it then takes a tap as a key: "#c0y6020", seen on Android 15), and the app's insets say how far it reaches.
keyboard_up() {
    dev dumpsys input_method | grep -q 'mInputShown=true' && return 0
    local px
    px=$(sn insets_bottom_px | cut -d. -f1)
    case "$px" in '' | *[!0-9]*) return 1 ;; esac
    [ "$px" -gt 400 ]
}
keyboard_down() { ! keyboard_up; }
hide_keyboard() {
    local try
    for try in 1 2 3; do
        keyboard_up || return 0
        dev input keyevent KEYCODE_BACK
        after_input
        wait_for 2 keyboard_down && return 0
    done
}
sheet_scroll_up() {
    # shellcheck disable=SC2046
    dev input swipe $(swipe_ys back) 400
    after_input
}
