#!/bin/bash
# The baseline check, the watchdog and INFRA (devtest/baseline.sh) against a stubbed device: PASS or FAIL for each case.
# Usage: bash devtest/baseline.test.sh      (no adb, no emulator)
#
# The real lib.sh and baseline.sh run. Only the shell on the device (ask) is replaced: the two device scripts are run by a
# local bash whose getprop, pidof, dumpsys, settings, cmd and logcat answer from a table of what the "device" looks like,
# so the real text of the scripts (every grep in them) is what is tested. Commands that change something (settings put,
# wm size reset, ...) change the table.
here=$(cd "${BASH_SOURCE[0]%/*}" && pwd)
tmp=$(mktemp -d) || exit 1
trap 'rm -rf "$tmp"' EXIT
bad=0

cat > "$tmp/drive.sh" << 'E'
# Env: LIB DIR MODE, and per case SX LEFT STUCK SERIAL RESTART_FAILS.
export OUT="$DIR/out" TMPDIR="$DIR" ADB=/nonexistent/adb
export SERIAL="${SERIAL:-emulator-5554}"
mkdir -p "$OUT"
. "$LIB"
RESULTS="$OUT/results.txt"
: > "$RESULTS"
CALLS="$DIR/calls"
: > "$CALLS"

CLEAN_FOCUS='mCurrentFocus=Window{3c4d u0 com.google.android.apps.nexuslauncher/com.google.android.apps.nexuslauncher.NexusLauncherActivity}'
# What the device is like, one file per value: a change made inside a pipe (dev dumpsys ... | tr) must stay.
mkdir -p "$DIR/st"
st_set() { printf '%s' "$2" > "$DIR/st/$1"; }
st_get() { local v=""; read -r v < "$DIR/st/$1" || :; printf '%s' "$v"; }
ST_KEYS="boot bootid ss audio wake keyguard pin ime focus size dpi font idle autotime zone rot_auto rot_user night timeout stay"
st_set boot 1; st_set bootid boot-1; st_set ss 812; st_set audio 640; st_set wake Awake; st_set keyguard false; st_set pin false
st_set ime false; st_set focus 'mCurrentFocus=Window{1a2b u0 com.fidderr.alarmfree/android.app.NativeActivity}'
st_set size 1080x2400; st_set dpi 420; st_set font null; st_set idle false; st_set autotime 0; st_set zone Europe/Amsterdam
st_set rot_auto 0; st_set rot_user 0; st_set night no; st_set timeout 86400000; st_set stay 0
: > "$DIR/devlog"

# What the device prints, in the shape the system prints it (reasoned for Android 10 to 16, as in baseline.sh).
read -r -d '' FAKE_PRE << 'F'
getprop() { case "$1" in sys.boot_completed) echo "$ST_boot" ;; persist.sys.timezone) echo "$ST_zone" ;; esac; }
pidof() { case "$1" in system_server) echo "$ST_ss" ;; audioserver) echo "$ST_audio" ;; esac; }
cat() { if [ "$1" = /proc/sys/kernel/random/boot_id ]; then echo "$ST_bootid"; else command cat "$@"; fi; }
logcat() { command cat "$ST_logfile"; }
cmd() { [ "$1 $2" = "uimode night" ] && echo "Night mode: $ST_night"; }
settings() {
    [ "$1" = get ] || return 0
    case "$3" in
        font_scale) echo "$ST_font" ;; auto_time) echo "$ST_autotime" ;; accelerometer_rotation) echo "$ST_rot_auto" ;;
        user_rotation) echo "$ST_rot_user" ;; screen_off_timeout) echo "$ST_timeout" ;; stay_on_while_plugged_in) echo "$ST_stay" ;;
    esac
}
dumpsys() {
    case "$1 $2" in
        "window displays")
            printf 'WINDOW MANAGER DISPLAY CONTENTS (dumpsys window displays)\n  Display: mDisplayId=0\n'
            if [ "$ST_size $ST_dpi" = "1080x2400 420" ]; then
                printf '    init=1080x2400 420dpi cur=1080x2400 app=1080x2400 rng=1080x1000-2400x2400\n'
            else
                printf '    init=1080x2400 420dpi base=%s %sdpi cur=%s app=%s rng=1080x1000-2400x2400\n' "$ST_size" "$ST_dpi" "$ST_size" "$ST_size"
            fi
            printf '  isKeyguardShowing=%s\n  %s\n  mFocusedApp=ActivityRecord{1 u0 x/y t5}\n' "$ST_keyguard" "$ST_focus"
            ;;
        "window policy") printf 'WINDOW MANAGER POLICY STATE\n  mSafeMode=false\n  KeyguardServiceDelegate\n    showing=false\n    secure=%s\n    occluded=false\n' "$ST_pin" ;;
        "power "*) printf 'POWER MANAGER (dumpsys power)\n  mWakefulness=%s\n  mWakefulnessChanging=false\n' "$ST_wake" ;;
        "input_method "*)
            printf 'Input Method Service\n  client A mInputShown=false\n'
            [ "$ST_ime" != true ] || printf '  client B mInputShown=true\n'
            ;;
        "deviceidle "*) printf 'Idle state:\n  mForceIdle=%s\n  mState=ACTIVE\n' "$ST_idle" ;;
    esac
}
F

fd_set() { case " ${STUCK:-} " in *" $1 "*) ;; *) st_set "$1" "$2" ;; esac; }
fake_script() { # the script of the device, run by a local bash that answers as the device would
    local k
    (
        for k in $ST_KEYS; do export "ST_$k=$(st_get "$k")"; done
        export ST_logfile="$DIR/devlog"
        bash -c "$FAKE_PRE"$'\n'"$1"
    )
}
fake_command() {
    local c=$1
    case "$c" in
        *isKeyguardShowing=false*) [ "$(st_get keyguard)" = false ]; return ;;
        "locksettings clear --old 1234"*) fd_set pin false ;;
        "input keyevent KEYCODE_WAKEUP"*) fd_set wake Awake ;;
        "wm dismiss-keyguard"*) fd_set keyguard false ;;
        "input keyevent KEYCODE_BACK"*) fd_set ime false ;;
        "input keyevent KEYCODE_HOME"*)
            # SLOW_HOME=n: a change of focus that shows only after the n-th press.
            echo h >> "$DIR/homes"
            [ "$(count "$DIR/homes")" -lt "${SLOW_HOME:-1}" ] || fd_set focus "$CLEAN_FOCUS"
            ;;
        "wm size reset"*) fd_set size 1080x2400 ;;
        "wm density reset"*) fd_set dpi 420 ;;
        "settings put system font_scale"*) fd_set font "${c##* }" ;;
        "settings put global auto_time"*) fd_set autotime "${c##* }" ;;
        "settings put system accelerometer_rotation"*) fd_set rot_auto "${c##* }" ;;
        "settings put system user_rotation"*) fd_set rot_user "${c##* }" ;;
        "settings put system screen_off_timeout"*) fd_set timeout "${c##* }" ;;
        "settings put global stay_on_while_plugged_in"*) fd_set stay "${c##* }" ;;
        "dumpsys deviceidle unforce"*) fd_set idle false ;;
        "am force-stop com.fidderr.alarmfree"* | "am force-stop com.android.settings"*) fd_set focus "$CLEAN_FOCUS" ;;
        "su 0 id -u"*) [ -n "${NOROOT:-}" ] || echo 0 ;;
        "cmd alarm set-timezone"*) fd_set zone "${c##* }" ;;
        "cmd uimode night"*) fd_set night "${c##* }" ;;
    esac
    return 0
}
ask() { # pid in out command
    printf '%s\n' "${4%%$'\n'*}" >> "$CALLS"
    case "$4" in
        "$BASELINE_SCRIPT" | "$WATCH_SCRIPT") fake_script "$4" ;;
        *) fake_command "$4" ;;
    esac
}
adb_() {
    [ "$1" = shell ] || return 0
    shift
    case "$*" in
        "$WATCH_SCRIPT") fake_script "$*" ;;
        *) fake_command "$*" ;;
    esac
}
shells_open() { return 0; }
write_log() { :; }
DEV_PID=$$
reboot_and_wait() {
    echo r >> "$DIR/reboots"
    [ -z "${RESTART_FAILS:-}" ] || return 1
    st_set bootid boot-2
    st_set ss 4000
    st_set audio 4100
    watchdog_announce_restart
    watchdog_rebase
}
count() { local n=0 l; [ -f "$1" ] && while IFS= read -r l; do n=$((n + 1)); done < "$1"; echo "$n"; }

# A scenario: one result line, and what it does to the device.
sx() {
    CURRENT=sx
    SCENARIO_START=$SECONDS
    case "${SX:-ok}" in
        ok) pass "fine" ;;
        fail) fail "the app is wrong" ;;
        break_once)
            if [ ! -e "$DIR/once" ]; then
                : > "$DIR/once"
                st_set ss 999
                fail "something odd"
            else
                pass "fine"
            fi
            ;;
        break_always)
            st_set ss $(($(st_get ss) + 1))
            pass "fine"
            ;;
        app_dies)
            printf '%s\n' 'E AndroidRuntime: FATAL EXCEPTION: main' 'E AndroidRuntime: Process: com.fidderr.alarmfree, PID: 4321' \
                'F libc    : Fatal signal 11 (SIGSEGV), code 1 (SEGV_MAPERR), fault addr 0x0 in tid 4321 (alarmfree), pid 4321 (com.fidderr.alarmfree)' >> "$DIR/devlog"
            fail "the app crashed"
            ;;
        system_crash)
            printf '%s\n' 'E AndroidRuntime: *** FATAL EXCEPTION IN SYSTEM PROCESS: android.fg' >> "$DIR/devlog"
            pass "fine"
            ;;
        audio_crash)
            printf '%s\n' 'F libc    : Fatal signal 6 (SIGABRT), code -1 (SI_QUEUE) in tid 640 (audioserver), pid 640 (audioserver)' >> "$DIR/devlog"
            pass "fine"
            ;;
        audio_replaced)
            st_set audio 641
            pass "fine"
            ;;
        new_boot)
            st_set bootid boot-9
            pass "fine"
            ;;
        reboot_meant)
            reboot_and_wait
            pass "fine"
            ;;
        reboot_then_break)
            reboot_and_wait
            st_set ss 7777
            pass "fine"
            ;;
        vars)
            if [ ! -e "$DIR/once" ]; then
                : > "$DIR/once"
                S13D_DONE=1 S23B_TRY=2 S26_AGAIN=1 OVERLAY=deny ALARM_EXTRA=',"x":1' VARIANT=@big RETRY_CARRY=old
                st_set ss 999
                pass "first"
            else
                pass "second:${S13D_DONE:-unset}:${S23B_TRY:-unset}:${S26_AGAIN:-unset}:${OVERLAY:-}:${ALARM_EXTRA:-}:${VARIANT:-}:${RETRY_CARRY:-}"
            fi
            ;;
    esac
}

eval "${PRE:-:}"
case "$MODE" in
    clean)
        baseline_unit s01
        echo $? > "$DIR/rc"
        echo "${BL_VAL[sizeov]-} ${BL_VAL[dpiov]-}" > "$DIR/disp"
        ;;
    selftest)
        # The scenarios that test the runner, with the scenario helpers stubbed.
        scenario() { CURRENT=$1; SCENARIO_START=$SECONDS; }
        fresh() { :; }
        hide_keyboard() { echo hide_keyboard >> "$CALLS"; }
        . "$SELFTEST"
        case "$WHICH" in
            cleanup)
                SELFTEST_LEFT=${LEFT_FLAG-}
                selftest_cleanup
                echo "$SELFTEST_LEFT" > "$DIR/left"
                ;;
            *) "$WHICH" ;;
        esac
        ;;
    leftover)
        # The first unit fixes what is normal on this device, then one thing is left behind.
        baseline_unit s01
        BL_PREV=s01
        st_set "${LEFT%%=*}" "${LEFT#*=}"
        baseline_unit s02
        echo $? > "$DIR/rc"
        st_get "${LEFT%%=*}" > "$DIR/after"
        echo "$BL_INFRA" > "$DIR/infra"
        ;;
    watch)
        baseline_unit s01
        [ -z "${ANNOUNCE:-}" ] || watchdog_announce_restart
        eval "$CHANGE"
        watchdog_end
        echo "$WD_WHAT" > "$DIR/what"
        ;;
    run)
        run_unit sx
        echo $? > "$DIR/rc"
        ;;
    runs)
        run_unit sx
        SX=ok
        run_unit sx
        ;;
esac
E

run_case() { # name mode [VAR=value...]
    local name=$1 mode=$2
    shift 2
    mkdir -p "$tmp/$name"
    env LIB="$here/lib.sh" SELFTEST="$here/selftest.sh" DIR="$tmp/$name" MODE="$mode" "$@" bash "$tmp/drive.sh" > "$tmp/$name/stdout" 2>&1
    echo $? > "$tmp/$name/exit"
}
verdict() { # name ok
    if [ "$2" = 1 ]; then
        echo "PASS  $1"
    else
        echo "FAIL  $1"
        bad=1
    fi
}
lines() { local n=0 l; [ -f "$1" ] && while IFS= read -r l || [ -n "$l" ]; do n=$((n + 1)); done < "$1"; echo "$n"; }
has() { grep -q -- "$2" "$1" 2> /dev/null; }
# A file's single line, without a program.
first() { local l=""; [ -f "$1" ] && read -r l < "$1"; echo "$l"; }
res() { echo "$tmp/$1/out/results.txt"; }

# 1. A clean reading passes with no note, and costs one call to the device.
run_case clean clean
[ "$(first "$tmp/clean/rc")" = 0 ] && [ "$(lines "$(res clean)")" = 0 ] && [ "$(lines "$tmp/clean/calls")" = 1 ] &&
    verdict "a clean reading passes, writes no line and costs one call to the device" 1 ||
    verdict "a clean reading passes, writes no line and costs one call to the device (calls $(lines "$tmp/clean/calls"), lines $(lines "$(res clean)"))" 0
has "$tmp/clean/out/baseline.txt" "boot=1 bootid=boot-1 ss=812" && verdict "the reading is kept in baseline.txt" 1 || verdict "the reading is kept in baseline.txt" 0
# The system prints `base=` only when the size or density was changed: no `base=` is no override.
[ "$(first "$tmp/clean/disp")" = "none none" ] && verdict "a display line without base= reads as no override" 1 || verdict "a display line without base= reads as no override ($(first "$tmp/clean/disp"))" 0

# 2. Each kind of leftover is put back, read again, and said in a NOTE.
leftover_case() { # name key=value expected-text key-after
    local name=$1 want=$4
    run_case "$name" leftover LEFT="$2"
    local rc after note
    rc=$(first "$tmp/$name/rc")
    after=$(first "$tmp/$name/after")
    has "$(res "$name")" "^NOTE  s02  the scenario before (s01) left: $3 (put back)\$" && note=1 || note=0
    if [ "$rc" = 0 ] && [ "$note" = 1 ] && [ "$after" = "$want" ] && [ "$(lines "$(res "$name")")" = 1 ]; then
        verdict "left behind and put back, with a NOTE: $3" 1
    else
        verdict "left behind and put back, with a NOTE: $3 (rc $rc, note $note, after '$after', results: $(cat "$(res "$name")" 2> /dev/null))" 0
    fi
}
leftover_case font font=1.3 "font scale 1.3" 1.0
leftover_case size size=900x2000 "screen size 900x2000" 1080x2400
leftover_case dpi dpi=560 "screen density 560" 420
leftover_case ime ime=true "the keyboard is up" false
leftover_case pin pin=true "a lock PIN or pattern is set" false
leftover_case idle idle=true "Doze forced" false
leftover_case autotime autotime=1 "automatic time is on" 0
leftover_case zone zone=Asia/Tokyo "time zone Asia/Tokyo (was Europe/Amsterdam)" Europe/Amsterdam
leftover_case rotauto rot_auto=1 "auto-rotate 1 (was 0)" 0
leftover_case rotuser rot_user=1 "rotation 1 (was 0)" 0
leftover_case night night=yes "night mode yes (was no)" no
leftover_case timeout timeout=10000 "screen timeout 10000 (was 86400000)" 86400000
leftover_case stay stay=3 "stay awake while plugged in 3 (was 0)" 0
leftover_case wake wake=Asleep "the screen was Asleep" Awake
leftover_case keyguard keyguard=true "the lock screen is showing" false
leftover_case shade "focus=mCurrentFocus=Window{9 u0 NotificationShade}" "the focus is on NotificationShade" "mCurrentFocus=Window{3c4d u0 com.google.android.apps.nexuslauncher/com.google.android.apps.nexuslauncher.NexusLauncherActivity}"
leftover_case settings "focus=mCurrentFocus=Window{9 u0 com.android.settings/com.android.settings.Settings}" "the focus is on com.android.settings/com.android.settings.Settings" "mCurrentFocus=Window{3c4d u0 com.google.android.apps.nexuslauncher/com.google.android.apps.nexuslauncher.NexusLauncherActivity}"

# The dialog of an app that does not respond goes with the app (a FAIL of the scenario before, not a broken device).
leftover_case anr "focus=mCurrentFocus=Window{9 u0 Application Not Responding: com.fidderr.alarmfree}" "the focus is on a not-responding dialog of com.fidderr.alarmfree" "mCurrentFocus=Window{3c4d u0 com.google.android.apps.nexuslauncher/com.google.android.apps.nexuslauncher.NexusLauncherActivity}"
# A change of focus that shows only after the fourth press of Home on a busy runner is still put back, not INFRA.
run_case slowhome leftover LEFT="focus=mCurrentFocus=Window{9 u0 NotificationShade}" SLOW_HOME=4
[ "$(first "$tmp/slowhome/rc")" = 0 ] && has "$(res slowhome)" '^NOTE  s02  the scenario before (s01) left: the focus is on NotificationShade (put back)$' && verdict "a repair that shows only on the fourth try is still put back (no INFRA)" 1 || verdict "a repair that shows only on the fourth try is still put back (rc $(first "$tmp/slowhome/rc"), $(cat "$(res slowhome)" 2> /dev/null))" 0

# The app or a launcher in front, or nothing yet, are all a baseline.
for f in 'mCurrentFocus=Window{1 u0 com.fidderr.alarmfree/android.app.NativeActivity}' 'mCurrentFocus=Window{2 u0 com.android.launcher3/com.android.launcher3.uioverrides.QuickstepLauncher}' 'mCurrentFocus=null'; do
    run_case "focus_ok" leftover LEFT="focus=$f"
    [ "$(first "$tmp/focus_ok/rc")" = 0 ] && [ "$(lines "$(res focus_ok)")" = 0 ] && verdict "focus on ${f#*u0 } is no leftover" 1 || verdict "focus on ${f#*u0 } is no leftover" 0
done

# 3. A leftover that stays is INFRA, and the one that was put back still gets its NOTE.
run_case stuck leftover LEFT=font=1.3 STUCK=font
infra=$(first "$tmp/stuck/infra")
[ "$(first "$tmp/stuck/rc")" = 1 ] && case "$infra" in *"font scale 1.3 and cannot be put back") true ;; *) false ;; esac && verdict "a leftover that stays gives INFRA, the scenario does not run" 1 || verdict "a leftover that stays gives INFRA ($infra)" 0

# 4. The watchdog.
run_case w_ss watch "CHANGE=st_set ss 999"
what=$(first "$tmp/w_ss/what")
case "$what" in "system_server restarted (pid 812, now 999)") verdict "a changed system_server pid is INFRA" 1 ;; *) verdict "a changed system_server pid is INFRA ($what)" 0 ;; esac
run_case w_ss_ann watch "CHANGE=st_set ss 999" ANNOUNCE=1
[ -z "$(first "$tmp/w_ss_ann/what")" ] && verdict "the same change after the announced restart is no finding" 1 || verdict "the same change after the announced restart is no finding ($(first "$tmp/w_ss_ann/what"))" 0
run_case w_gone watch "CHANGE=st_set ss ''"
case "$(first "$tmp/w_gone/what")" in "system_server restarted (pid 812, now none)") verdict "a system_server that is gone is INFRA" 1 ;; *) verdict "a system_server that is gone is INFRA ($(first "$tmp/w_gone/what"))" 0 ;; esac
run_case w_audio watch "CHANGE=st_set audio 641"
case "$(first "$tmp/w_audio/what")" in "the audio server restarted (pid 640, now 641)") verdict "a replaced audio server is INFRA" 1 ;; *) verdict "a replaced audio server is INFRA ($(first "$tmp/w_audio/what"))" 0 ;; esac
run_case w_boot watch "CHANGE=st_set bootid boot-9"
case "$(first "$tmp/w_boot/what")" in "the device restarted (a new boot)") verdict "a new boot id is INFRA" 1 ;; *) verdict "a new boot id is INFRA ($(first "$tmp/w_boot/what"))" 0 ;; esac
run_case w_none watch "CHANGE=:"
[ -z "$(first "$tmp/w_none/what")" ] && [ "$(lines "$tmp/w_none/calls")" = 2 ] && verdict "an unchanged device is whole (one call to read it before, one after)" 1 || verdict "an unchanged device is whole" 0
# The device that does not answer prints nothing: the key boot is missing.
run_case w_noans watch 'CHANGE=fake_script() { :; }'
case "$(first "$tmp/w_noans/what")" in "the device did not answer after the scenario") verdict "a device that does not answer after the scenario is INFRA" 1 ;; *) verdict "a device that does not answer after the scenario is INFRA ($(first "$tmp/w_noans/what"))" 0 ;; esac

# 5. The runner: a dead app is never INFRA; a device that broke is, once, with a rerun.
run_case r_ok run SX=ok
[ "$(lines "$(res r_ok)")" = 1 ] && has "$(res r_ok)" '^PASS  sx  \[[0-9]*s\] fine$' && [ ! -e "$tmp/r_ok/reboots" ] && verdict "a normal run writes the same single line as before (no NOTE, no extra line, no restart)" 1 || verdict "a normal run writes the same single line as before ($(cat "$(res r_ok)"))" 0
run_case r_app run SX=app_dies
[ "$(lines "$(res r_app)")" = 1 ] && has "$(res r_app)" '^FAIL  sx  ' && [ ! -e "$tmp/r_app/reboots" ] && verdict "an app that dies (crash lines of the app in the log) stays a FAIL and is no INFRA" 1 || verdict "an app that dies stays a FAIL ($(cat "$(res r_app)"))" 0
run_case r_fail run SX=fail
has "$(res r_fail)" '^FAIL  sx  ' && [ ! -e "$tmp/r_fail/reboots" ] && verdict "a failing scenario on a whole device stays a FAIL" 1 || verdict "a failing scenario on a whole device stays a FAIL" 0
run_case r_once run SX=break_once
rf=$(res r_once)
if has "$rf" '^INFRA  sx  \[[0-9]*s\] system_server restarted (pid 812, now 999); the scenario said: FAIL something odd \[rerun follows\]$' && has "$rf" '^PASS  sx  ' && ! has "$rf" '^FAIL' && [ "$(lines "$tmp/r_once/reboots")" = 1 ] && [ "$(lines "$rf")" = 2 ]; then
    verdict "a device that broke gives INFRA, one restart, one rerun that counts; both lines stay" 1
else
    verdict "a device that broke gives INFRA, one restart, one rerun that counts ($(cat "$rf"))" 0
fi
run_case r_twice run SX=break_always
rf=$(res r_twice)
if has "$rf" 'rerun follows\]$' && has "$rf" '^INFRA  sx  .*scenario said: PASS fine$' && ! has "$rf" '^PASS' && [ "$(lines "$tmp/r_twice/reboots")" = 1 ]; then
    verdict "broken again on the rerun: INFRA stands, only one restart" 1
else
    verdict "broken again on the rerun: INFRA stands ($(cat "$rf"))" 0
fi
run_case r_syscrash run SX=system_crash
has "$(res r_syscrash)" 'crash in the system log: .*FATAL EXCEPTION IN SYSTEM PROCESS' && verdict "a crash of the system process in the log is INFRA" 1 || verdict "a crash of the system process in the log is INFRA ($(cat "$(res r_syscrash)"))" 0
run_case r_audiocrash run SX=audio_crash
has "$(res r_audiocrash)" 'crash in the system log: .*(audioserver)' && verdict "a crash of the audio server in the log is INFRA" 1 || verdict "a crash of the audio server in the log is INFRA ($(cat "$(res r_audiocrash)"))" 0
run_case r_audio run SX=audio_replaced
has "$(res r_audio)" 'the audio server restarted (pid 640, now 641)' && verdict "a replaced audio server is INFRA in a run" 1 || verdict "a replaced audio server is INFRA in a run" 0
run_case r_boot run SX=new_boot
has "$(res r_boot)" 'the device restarted (a new boot)' && verdict "a new boot is INFRA in a run" 1 || verdict "a new boot is INFRA in a run" 0
run_case r_meant run SX=reboot_meant
[ "$(lines "$(res r_meant)")" = 1 ] && has "$(res r_meant)" '^PASS  sx  ' && verdict "a restart the scenario announced is no INFRA" 1 || verdict "a restart the scenario announced is no INFRA ($(cat "$(res r_meant)"))" 0
run_case r_rebase run SX=reboot_then_break
has "$(res r_rebase)" '^INFRA  sx  .*system_server restarted (pid 4000, now 7777)' && verdict "after an announced restart the new processes are watched: a break later in the same scenario is INFRA" 1 || verdict "after an announced restart the new processes are watched ($(cat "$(res r_rebase)"))" 0
run_case r_vars run SX=vars
has "$(res r_vars)" '^PASS  sx  .*second:unset:unset:unset::::$' && verdict "the second run starts without what the first one left in bash variables" 1 || verdict "the second run starts without what the first one left in bash variables ($(cat "$(res r_vars)"))" 0
run_case r_base run SX=ok PRE="st_set font 1.3" STUCK=font
rf=$(res r_base)
if has "$rf" '^INFRA  sx  \[[0-9]*s\] the device is not at its baseline: font scale 1.3 and cannot be put back \[rerun follows\]$' && has "$rf" 'baseline: font scale 1.3 and cannot be put back$' && ! has "$rf" '^PASS' && [ "$(lines "$tmp/r_base/reboots")" = 1 ]; then
    verdict "a baseline that cannot be reached: INFRA, the scenario does not run, one restart, INFRA stands" 1
else
    verdict "a baseline that cannot be reached: INFRA, the scenario does not run ($(cat "$rf"))" 0
fi
run_case r_restartfails run SX=break_once RESTART_FAILS=1
[ "$(first "$tmp/r_restartfails/exit")" = 2 ] && has "$(res r_restartfails)" '^STOP  sx  the device did not come back after the restart' && verdict "a device that does not come back ends the run with STOP and the reason" 1 || verdict "a device that does not come back ends the run with STOP ($(cat "$(res r_restartfails)"), exit $(first "$tmp/r_restartfails/exit"))" 0

# 6. A phone is read and noted, nothing is changed or restarted.
run_case phone leftover LEFT=font=1.3 SERIAL=R58M12345A
if [ "$(first "$tmp/phone/rc")" = 0 ] && [ "$(first "$tmp/phone/after")" = 1.3 ] && [ "$(lines "$tmp/phone/calls")" = 2 ] && has "$(res phone)" '^NOTE  s02  changed during the run (s01): font scale 1.3 (a phone: not put back)$'; then
    verdict "a phone: the read and a NOTE, no repair" 1
else
    verdict "a phone: the read and a NOTE, no repair (calls $(lines "$tmp/phone/calls"), $(cat "$(res phone)"))" 0
fi
run_case phone_w watch "CHANGE=st_set ss 999" SERIAL=R58M12345A
[ -z "$(first "$tmp/phone_w/what")" ] && verdict "a phone whose system process changed is not INFRA and not restarted" 1 || verdict "a phone whose system process changed is not INFRA" 0

# 7. The exit code of a run.
printf 'PASS  a  [1s] x\nFAIL  b  [1s] y\nINFRA  c  [1s] z\n' > "$tmp/e1"
printf 'PASS  a  [1s] x\nINFRA  c  [1s] z\n' > "$tmp/e2"
printf 'INFRA  c  [1s] z [rerun follows]\nPASS  c  [1s] fine\n' > "$tmp/e3"
printf 'PASS  a  [1s] x\n' > "$tmp/e4"
printf 'INFRA  c  [1s] z [rerun follows]\nINFRA  c  [1s] z\n' > "$tmp/e5"
(
    . "$here/baseline.sh"
    run_exit_code "$tmp/e1"; r1=$?
    run_exit_code "$tmp/e2"; r2=$?
    run_exit_code "$tmp/e3"; r3=$?
    run_exit_code "$tmp/e4"; r4=$?
    run_exit_code "$tmp/e5"; r5=$?
    run_exit_code "$tmp/nothing"; r6=$?
    echo "$r1 $r2 $r3 $r4 $r5 $r6" > "$tmp/codes"
)
[ "$(first "$tmp/codes")" = "1 4 0 0 4 0" ] && verdict "exit code: 1 with a FAIL, 4 with a standing INFRA, 0 after an INFRA whose rerun passed" 1 || verdict "exit code: 1 with a FAIL, 4 with a standing INFRA, 0 after an INFRA whose rerun passed ($(first "$tmp/codes"))" 0

# 8. The device scripts are plain sh (syntax check with sh -n) and the pieces are wired in.
(
    . "$here/baseline.sh"
    printf '%s\n' "$BASELINE_SCRIPT" > "$tmp/baseline_script.sh"
    printf '%s\n' "$WATCH_SCRIPT" > "$tmp/watch_script.sh"
)
if sh -n "$tmp/baseline_script.sh" && sh -n "$tmp/watch_script.sh" && [ "$(lines "$tmp/baseline_script.sh")" -gt 15 ]; then
    verdict "both device scripts pass sh -n" 1
else
    verdict "both device scripts pass sh -n" 0
fi
# On a phone dev refuses a command that has "input " in it unless AlarmFree is in front: the scripts must not.
case "$(cat "$tmp/baseline_script.sh" "$tmp/watch_script.sh")" in *"input "*) verdict "the device scripts hold no word that a phone's input guard would catch" 0 ;; *) verdict "the device scripts hold no word that a phone's input guard would catch" 1 ;; esac
wired=0
while IFS= read -r l; do
    l=${l%$'\r'}
    case "$l" in
        'reboot_only() {') seen=1 ;;
        *watchdog_announce_restart*) [ "${seen:-}" = 1 ] && wired=$((wired + 1)); seen=0 ;;
    esac
done < "$here/lib.sh"
runner=0
while IFS= read -r l; do case "$l" in *'run_unit "$s"'*) runner=1 ;; esac; done < "$here/matrix.sh"
[ "$wired" = 1 ] && [ "$runner" = 1 ] && verdict "reboot_only announces its restart, and the runner of matrix.sh goes through run_unit" 1 || verdict "reboot_only announces its restart ($wired), the runner uses run_unit ($runner)" 0
# The pieces that the crash scan, the start of a run and the callers rely on.
in_func() { # file function-head text-in-it
    local l inside=0 found=0
    while IFS= read -r l || [ -n "$l" ]; do
        l=${l%$'\r'}
        case "$l" in
            "$2") inside=1 ;;
            "}") inside=0 ;;
            *"$3"*) [ "$inside" = 1 ] && found=1 ;;
        esac
    done < "$1"
    [ "$found" = 1 ]
}
in_func "$here/lib.sh" 'reboot_only() {' watchdog_rebase && verdict "reboot_only watches the new processes when the device is back (watchdog_rebase)" 1 || verdict "reboot_only calls watchdog_rebase" 0
in_func "$here/lib.sh" 'scenario() {' 'logcat -c' && verdict "scenario() clears the log: the crash scan of the watchdog sees only what came after the scenario began" 1 || verdict "scenario() clears the log (the crash scan depends on it)" 0
# At the start of a run the PIN of a killed run is cleared before the first reading, or the reading would adopt it.
pin_line=0 first_unit=0 n=0 started=0
while IFS= read -r l || [ -n "$l" ]; do
    n=$((n + 1))
    case "$l" in
        *'RUN_START=$SECONDS'*) started=1 ;;
        *'locksettings clear --old 1234'*) [ "$started" = 1 ] && [ "$pin_line" = 0 ] && pin_line=$n ;;
        *'run_unit "$s"'*) first_unit=$n ;;
    esac
done < "$here/matrix.sh"
[ "$pin_line" != 0 ] && [ "$first_unit" != 0 ] && [ "$pin_line" -lt "$first_unit" ] && verdict "the runner clears a PIN left by a killed run before the first scenario" 1 || verdict "the runner clears a PIN left by a killed run before the first scenario ($pin_line, $first_unit)" 0
# phone.sh must not call a run with a FAIL (1) or an INFRA (4) in its results "stopped".
has "$here/phone.sh" '0 | 1 | 4) ;;' && verdict "phone.sh takes exit code 1 and 4 of a run as a run that ended" 1 || verdict "phone.sh takes exit code 1 and 4 of a run as a run that ended" 0

# 9. The scenarios that test the runner (selftest.sh), against the stub.
run_case t03_noroot selftest WHICH=t03_device_breaks NOROOT=1 RUN_ID=run-1
has "$(res t03_noroot)" '^SKIP  t03_device_breaks  .*no root' && ! has "$(res t03_noroot)" '^FAIL' && [ ! -e "$tmp/t03_noroot/out/t03_device_breaks.marker" ] && verdict "t03 on an image without root says so and skips (no FAIL, no marker)" 1 || verdict "t03 on an image without root says so and skips ($(cat "$(res t03_noroot)" 2> /dev/null))" 0
run_case t03_same selftest WHICH=t03_device_breaks RUN_ID=run-1 PRE='echo run-1 > "$OUT/t03_device_breaks.marker"'
has "$(res t03_same)" '^PASS  t03_device_breaks  .*second run' && verdict "t03 does not kill again when the marker is of this run" 1 || verdict "t03 does not kill again when the marker is of this run ($(cat "$(res t03_same)" 2> /dev/null))" 0
run_case t03_old selftest WHICH=t03_device_breaks RUN_ID=run-2 PRE='echo run-1 > "$OUT/t03_device_breaks.marker"'
has "$(res t03_old)" '^FAIL  t03_device_breaks  .*no system_server to kill' && ! has "$(res t03_old)" 'second run' && verdict "a marker of an earlier run does not make t03 skip the kill" 1 || verdict "a marker of an earlier run does not make t03 skip the kill ($(cat "$(res t03_old)" 2> /dev/null))" 0
run_case t01_phone selftest WHICH=t01_leftovers SERIAL=R58M12345A
has "$(res t01_phone)" '^SKIP  t01_leftovers' && ! has "$tmp/t01_phone/calls" 'font_scale' && ! has "$tmp/t01_phone/calls" 'wm size' && verdict "t01 on a phone changes nothing" 1 || verdict "t01 on a phone changes nothing ($(cat "$tmp/t01_phone/calls" 2> /dev/null))" 0
run_case t03_phone selftest WHICH=t03_device_breaks SERIAL=R58M12345A RUN_ID=run-1
has "$(res t03_phone)" '^SKIP  t03_device_breaks' && [ ! -e "$tmp/t03_phone/out/t03_device_breaks.marker" ] && verdict "t03 on a phone kills nothing" 1 || verdict "t03 on a phone kills nothing" 0
run_case t01_last selftest WHICH=cleanup LEFT_FLAG=1
has "$tmp/t01_last/calls" 'wm size reset' && has "$tmp/t01_last/calls" 'font_scale 1.0' && [ -z "$(first "$tmp/t01_last/left")" ] && verdict "what t01 left is put back at the end of the run when t01 was the last" 1 || verdict "what t01 left is put back at the end of the run when t01 was the last" 0
run_case t01_not_last selftest WHICH=cleanup
[ "$(lines "$tmp/t01_not_last/calls")" = 0 ] && verdict "nothing is touched at the end of a run when t01 did not run" 1 || verdict "nothing is touched at the end of a run when t01 did not run" 0

exit "$bad"
