#!/bin/bash
# The state of the device before each scenario, and the watchdog around it. Sourced by lib.sh (the runner at the end of
# matrix.sh uses run_unit); suite.sh and ci/run.sh source it for infra_standing only.
#
# What it settles (plan alarmfree-devtest-no-flake, rules 7 and 8):
#   1. A scenario starts from a baseline that was read, not assumed: one call to the device prints what the scenario
#      before can have left (keyboard, PIN, screen size, font scale, Doze, the shade or a screen of Android in front,
#      ...). What is off is put back with the command the suite already uses, read again, and written as a NOTE (a finding
#      about the scenario before). What cannot be put back is INFRA.
#   2. After the scenario the device is read again: the system process or the audio server that was replaced, a new boot,
#      or a crash of either in the log is the device breaking, not the app: INFRA, the device is restarted and the scenario
#      runs once more. An app process that dies is never looked at here: that is the app.
#
# Result lines: `INFRA  <scenario>  [..s] <what broke>`. The one that is followed by a second run ends in `[rerun follows]`;
# an INFRA line without that ending stands (the rerun broke too, or nothing could be run). Exit code of a run
# (run_exit_code): 1 when a FAIL, else 4 when an INFRA stands, else 0.
#
# On a phone nothing is changed or restarted: it reads, and says in a NOTE what changed during the run.
#
# Android 10 to 16: every value is read from a line that the system prints in a form that was reasoned out, not tried on
# each version. A value that comes back empty is left out for that device (the first reading says which, and
# $OUT/baseline.txt keeps every reading).

declare -gA BL_VAL=() BL_WANT=() BL_WHY=()
declare -ga BL_BAD=()
BL_REF=""
BL_PREV=""
BL_INFRA=""
WD_SS="" WD_AUDIO="" WD_BOOT="" WD_WHAT="" WD_AUDIO_ONLY="" BL_BOOTED=""
RES_MARK=0
LINK_PID="" LINK_LOG="" LINK_MEANT=""

# Every line is `key=value`. The shell of the device is mksh or toybox sh: plain sh only. The dumps are read once into a
# variable and cut there, so the output is small and the call is one.
#   psize, osize, pdens, odens   the lines of `cmd window size` and `cmd window density` (what `wm size` and `wm density`
#          print): `Override ...` is there only when the size or density was changed. The displays dump is not used: Android 16
#          prints no `base=` part in it, also not for a changed size
#   pin    a lock credential is set (KeyguardServiceDelegate in the window policy dump)
#   kgpol  `showing=` in the same block: the lock screen on a version whose displays dump has no isKeyguardShowing
#          (Android 10, run 45065611089)
read -r -d '' BASELINE_SCRIPT << 'E'
w=$(dumpsys window displays)
echo "boot=$(getprop sys.boot_completed)"
echo "bootid=$(cat /proc/sys/kernel/random/boot_id)"
echo "ss=$(pidof system_server)"
echo "audio=$(pidof audioserver)"
x=$(dumpsys power 2> /dev/null | grep -m1 -o 'mWakefulness=[A-Za-z]*'); echo "wake=${x#*=}"
x=$(printf '%s\n' "$w" | grep -m1 -o 'isKeyguardShowing=[a-z]*'); echo "keyguard=${x#*=}"
p=$(dumpsys window policy 2> /dev/null)
x=$(printf '%s\n' "$p" | grep -m1 -o '[ ]showing=[a-z]*'); echo "kgpol=${x#*=}"
x=$(printf '%s\n' "$p" | grep -m1 -o 'secure=[a-z]*'); echo "pin=${x#*=}"
x=$(dumpsys input_method | grep -o 'mInputShown=[a-z]*'); case "$x" in *true*) echo "ime=true" ;; *false*) echo "ime=false" ;; *) echo "ime=" ;; esac
echo "focus=$(printf '%s\n' "$w" | grep -m1 mCurrentFocus)"
x=$(cmd window size 2> /dev/null); echo "psize=$(printf '%s\n' "$x" | grep -m1 'Physical size')"; echo "osize=$(printf '%s\n' "$x" | grep -m1 'Override size')"
x=$(cmd window density 2> /dev/null); echo "pdens=$(printf '%s\n' "$x" | grep -m1 'Physical density')"; echo "odens=$(printf '%s\n' "$x" | grep -m1 'Override density')"
echo "font=$(settings get system font_scale)"
x=$(dumpsys deviceidle 2> /dev/null | grep -m1 -o 'mForceIdle=[a-z]*'); echo "idle=${x#*=}"
echo "autotime=$(settings get global auto_time)"
echo "zone=$(getprop persist.sys.timezone)"
echo "rot_auto=$(settings get system accelerometer_rotation)"
echo "rot_user=$(settings get system user_rotation)"
x=$(cmd uimode night 2>&1 | head -1); echo "night=${x#*: }"
echo "timeout=$(settings get system screen_off_timeout)"
echo "stay=$(settings get global stay_on_while_plugged_in)"
E

# After the scenario: the processes that must not be replaced, and the log of the system.
read -r -d '' WATCH_SCRIPT << 'E'
echo "boot=$(getprop sys.boot_completed)"
echo "bootid=$(cat /proc/sys/kernel/random/boot_id)"
echo "ss=$(pidof system_server)"
echo "audio=$(pidof audioserver)"
l=$(logcat -d -b main -b system -b crash 2> /dev/null)
echo "crash=$(printf '%s\n' "$l" | grep -m1 -E 'FATAL EXCEPTION IN SYSTEM PROCESS|WATCHDOG KILLING SYSTEM PROCESS|Fatal signal.*[(]system_server[)]')"
echo "acrash=$(printf '%s\n' "$l" | grep -m1 -E 'Fatal signal.*[(]audioserver[)]')"
E

# A checkout with Windows line ends must not send a carriage return to the device.
BASELINE_SCRIPT=${BASELINE_SCRIPT//$'\r'/}
WATCH_SCRIPT=${WATCH_SCRIPT//$'\r'/}

BL_KEYS="boot pin wake keyguard ime focus sizeov dpiov font idle autotime zone rot_auto rot_user night timeout stay"

# `key=value` lines into the array named by $2 (no program: a here-string and the builtins).
baseline_parse() { # text
    local line
    BL_VAL=()
    while IFS= read -r line; do
        case "$line" in [a-z]*=*) BL_VAL[${line%%=*}]=${line#*=} ;; esac
    done <<< "$1"
    # What the shell cannot compare as it is printed.
    local f=${BL_VAL[focus]-} n=${BL_VAL[night]-} fs=${BL_VAL[font]-}
    # A line `Override size: 900x1600` is there only when changed; no Physical line means the command gave nothing.
    BL_VAL[sizeov]="?" BL_VAL[dpiov]="?"
    if [ -n "${BL_VAL[psize]-}" ]; then BL_VAL[sizeov]=none; [ -z "${BL_VAL[osize]-}" ] || BL_VAL[sizeov]=${BL_VAL[osize]##*: }; fi
    if [ -n "${BL_VAL[pdens]-}" ]; then BL_VAL[dpiov]=none; [ -z "${BL_VAL[odens]-}" ] || BL_VAL[dpiov]=${BL_VAL[odens]##*: }; fi
    # The window that has the keys: the app, a launcher, nothing (a moment of change) or something of Android.
    # `mCurrentFocus=Window{4a1b2c u0 com.android.settings/com.android.settings.Settings}`: focusname is the last word.
    case "$f" in
        "") BL_VAL[focusc]="" ;;
        *"Not Responding"* | *Recents*) BL_VAL[focusc]=bad ;;
        *"${PKG:-com.fidderr.alarmfree}/"* | *"=null"* | *[Ll]auncher*) BL_VAL[focusc]=ok ;;
        *) BL_VAL[focusc]=bad ;;
    esac
    f=${f%\}}
    BL_VAL[focusname]=${f##* }
    # The words of `cmd uimode night`, `custom_bedtime` (what Android 16 says at its first start) and `custom_schedule` among them; an error text is not.
    case "$n" in yes | no | auto | custom_*) ;; *) BL_VAL[night]="" ;; esac
    # Android 10 has no isKeyguardShowing in the displays dump: the policy dump says it.
    if [ -z "${BL_VAL[keyguard]-}" ] && [ -n "${BL_VAL[kgpol]-}" ]; then BL_VAL[keyguard]=${BL_VAL[kgpol]} BL_VAL[kgsrc]=policy; fi
    case "$fs" in null | 1 | 1.0 | 1.00) BL_VAL[font]=1.0 ;; esac
}

# The app's own word for the keyboard (`keyboard=up|down` of `ui ready`: what the window's insets say), asked only while the app
# is in front. The input method service lags a change (run 45085128218: mInputShown=false and mImeWindowVis=0 right after the PIN,
# true and 3 a little later; s49 started with the insets saying up and the baseline having read false), so the keyboard is up
# when either of the two says so.
baseline_app_keyboard() { UI_QUIET=1 ui ready 2> /dev/null | sed -n 's/.*keyboard=\([a-z]*\).*/\1/p' | head -1; }
baseline_app_in_front() { case "${BL_VAL[focus]-}" in *"${PKG:-com.fidderr.alarmfree}/"*) return 0 ;; esac; return 1; }

# One call to the device. Returns 1 when it did not answer.
baseline_read() {
    local out
    out=$(dev "$BASELINE_SCRIPT" 2> /dev/null)
    BL_RAW=$out
    baseline_parse "$out"
    if [ "${BL_VAL[ime]-}" = false ] && on_emulator && baseline_app_in_front && [ "$(baseline_app_keyboard)" = up ]; then
        BL_VAL[ime]=true
    fi
    [ -n "${BL_VAL[boot]+x}" ]
}

# A screen timeout of 86400000 (reboot_only) or more (the emulator writes 2147483647 when it has booted) is `never`.
baseline_tmo() { # value
    case "$1" in
        '' | *[!0-9]*) echo "$1" ;;
        *) if [ ${#1} -gt 9 ] || [ "$1" -ge 86400000 ]; then echo never; else echo "$1"; fi ;;
    esac
}

# What is wanted. A phone has no fixed wishes: what it showed at the first reading is what it should show.
baseline_want_init() {
    local k
    BL_WANT=()
    if on_emulator; then
        BL_WANT[boot]=1 BL_WANT[wake]=Awake BL_WANT[keyguard]=false BL_WANT[ime]=false BL_WANT[focus]=ok
        BL_WANT[sizeov]=none BL_WANT[dpiov]=none BL_WANT[font]=1.0 BL_WANT[idle]=false BL_WANT[autotime]=0
        # A device that does not show `false` at the start has no readable PIN here: the value is left out.
        [ "${BL_VAL[pin]-}" != false ] || BL_WANT[pin]=false
        # The same for the lock screen where only the policy dump says it.
        [ "${BL_VAL[kgsrc]-}" != policy ] || [ "${BL_VAL[keyguard]-}" = false ] || BL_WANT[keyguard]=
    else
        for k in ime sizeov dpiov font idle; do BL_WANT[$k]=${BL_VAL[$k]-}; done
    fi
    for k in zone rot_auto rot_user night timeout stay; do BL_WANT[$k]=${BL_VAL[$k]-}; done
    # The night mode of a fresh Android 16 emulator is `custom_bedtime` and turns to `no` by itself: that is the wish.
    case "${BL_WANT[night]}" in custom_*) on_emulator && BL_WANT[night]=no ;; esac
}

# BL_BAD: the keys that are not as wanted, BL_WHY[key]: what it was.
baseline_diffs() {
    local k got want
    BL_BAD=()
    BL_WHY=()
    for k in $BL_KEYS; do
        want=${BL_WANT[$k]-}
        [ -n "$want" ] || continue
        got=${BL_VAL[$k]-}
        [ "$k" != focus ] || got=${BL_VAL[focusc]-}
        if [ "$k" = timeout ]; then
            [ "$(baseline_tmo "$got")" != "$(baseline_tmo "$want")" ] || continue
        fi
        # Not readable on this device (or this once): not a finding. Boot is the one that must be there.
        if [ -z "$got" ] || [ "$got" = "?" ]; then
            [ "$k" = boot ] || continue
            BL_BAD+=(boot)
            BL_WHY[boot]="the device did not finish booting"
            continue
        fi
        [ "$got" != "$want" ] || continue
        BL_BAD+=("$k")
        case "$k" in
            boot) BL_WHY[$k]="the device did not finish booting" ;;
            pin) BL_WHY[$k]="a lock PIN or pattern is set" ;;
            wake) BL_WHY[$k]="the screen was $got" ;;
            keyguard) BL_WHY[$k]="the lock screen is showing" ;;
            ime) BL_WHY[$k]="the keyboard is up" ;;
            focus)
                case "${BL_VAL[focus]-}" in
                    *"Not Responding"*) BL_WHY[$k]="the focus is on a not-responding dialog of ${BL_VAL[focusname]:-an app}" ;;
                    *) BL_WHY[$k]="the focus is on ${BL_VAL[focusname]:-another screen of Android}" ;;
                esac
                ;;
            sizeov) BL_WHY[$k]="screen size $got" ;;
            dpiov) BL_WHY[$k]="screen density $got" ;;
            font) BL_WHY[$k]="font scale $got" ;;
            idle) BL_WHY[$k]="Doze forced" ;;
            autotime) BL_WHY[$k]="automatic time is on" ;;
            zone) BL_WHY[$k]="time zone $got (was $want)" ;;
            rot_auto) BL_WHY[$k]="auto-rotate $got (was $want)" ;;
            rot_user) BL_WHY[$k]="rotation $got (was $want)" ;;
            night) BL_WHY[$k]="night mode $got (was $want)" ;;
            timeout) BL_WHY[$k]="screen timeout $got (was $want)" ;;
            stay) BL_WHY[$k]="stay awake while plugged in $got (was $want)" ;;
        esac
    done
}

baseline_put() { # system|global key value
    if [ "$3" = null ]; then dev settings delete "$1" "$2" > /dev/null 2>&1; else dev settings put "$1" "$2" "$3" > /dev/null 2>&1; fi
}
# The lock screen: the shade or the status bar in front while the keyguard says showing, or says nothing (Android 10 without
# a reading), is the lock screen; with the keyguard not showing it is the shade.
baseline_is_lock() {
    case "${BL_VAL[focusname]-}" in NotificationShade | StatusBar) ;; *) return 1 ;; esac
    case "${BL_VAL[keyguard]-}" in true | "") return 0 ;; esac
    return 1
}
# Wake and dismiss (what `wake` does). Run 45065611089: that alone did not take in four places. A screen of the app over the
# lock screen (s06 leaves its ring there) holds the dismiss back, so it is stopped first (every scenario starts with the same
# force-stop); from the second round the lock screen is also swiped away, as a hand would.
baseline_unlock() { # round
    local pk=${PKG:-com.fidderr.alarmfree} size w h
    case "${BL_VAL[focus]-}" in
        *"$pk/"*) [ "${BL_VAL[keyguard]-}" != true ] || dev am force-stop "$pk" > /dev/null 2>&1 ;;
    esac
    SCREEN_WENT_OFF=""
    # The lock screen is put away by lock_away (system.sh): the dismiss, the old PIN on a pad that still asks for it (Android 10
    # to 12 after `locksettings clear`), sleep and wake, the Menu key, a swipe; each step only while a read shows the lock screen.
    if [ "$(kg_state)" = true ]; then
        lock_away > /dev/null 2>&1
        return 0
    fi
    wake
    [ "$1" -ge 2 ] || return 0
    dev input keyevent KEYCODE_MENU > /dev/null 2>&1
    size=${BL_VAL[osize]:-${BL_VAL[psize]-}}
    size=${size##* }
    w=${size%x*}
    h=${size#*x}
    case "$w$h" in '' | *[!0-9]*) ;; *) dev input swipe $((w / 2)) $((h * 4 / 5)) $((w / 2)) $((h / 5)) 300 > /dev/null 2>&1 ;; esac
    wake
}
# The input method of the phone stopped: its state of "shown" goes with it.
baseline_ime_stop() {
    local ime
    ime=$(dev settings get secure default_input_method 2> /dev/null)
    ime=${ime%%/*}
    case "$ime" in *.*) dev am force-stop "$ime" > /dev/null 2>&1 ;; esac
}
# Each with the command the suite already has for it. Emulator only.
baseline_repair() { # round
    local k woke=""
    for k in "${BL_BAD[@]}"; do
        case "$k" in
            pin)
                dev locksettings clear --old 1234 > /dev/null 2>&1
                OVERLAY=
                SCREEN_WENT_OFF=1
                ;;
            wake | keyguard) [ -n "$woke" ] || { baseline_unlock "$1"; woke=1; } ;;
            ime)
                dev input keyevent KEYCODE_BACK > /dev/null 2>&1
                # Android 14 to 16 keep the keyboard shown after the PIN and Back does not take it down: the input method is
                # stopped from the second round (the system starts it again when a field wants it), which ends that state.
                [ "$1" -lt 2 ] || baseline_ime_stop
                ;;
            focus)
                if baseline_is_lock && [ -z "$woke" ]; then
                    baseline_unlock "$1"
                    woke=1
                fi
                dev cmd statusbar collapse > /dev/null 2>&1
                case "${BL_VAL[focus]-}" in
                    *"Not Responding"*)
                        case "${BL_VAL[focusname]-}" in
                            com.android.systemui | android | "") ;;
                            *.*) dev am force-stop "${BL_VAL[focusname]}" > /dev/null 2>&1 ;;
                        esac
                        ;;
                esac
                case "${BL_VAL[focusname]-}" in
                    *FallbackHome*) ;;
                    *settings*) dev am force-stop com.android.settings ;;
                esac
                dev input keyevent KEYCODE_HOME > /dev/null 2>&1
                ;;
            sizeov) dev wm size reset > /dev/null 2>&1 ;;
            dpiov) dev wm density reset > /dev/null 2>&1 ;;
            font) dev settings put system font_scale 1.0 > /dev/null 2>&1 ;;
            idle) dev dumpsys deviceidle unforce > /dev/null 2>&1 ;;
            autotime) dev settings put global auto_time 0 > /dev/null 2>&1 ;;
            zone) set_zone "${BL_WANT[zone]}" ;;
            rot_auto) baseline_put system accelerometer_rotation "${BL_WANT[rot_auto]}" ;;
            rot_user) baseline_put system user_rotation "${BL_WANT[rot_user]}" ;;
            night) dev cmd uimode night "${BL_WANT[night]}" > /dev/null 2>&1 ;;
            timeout) baseline_put system screen_off_timeout "${BL_WANT[timeout]}" ;;
            stay) baseline_put global stay_on_while_plugged_in "${BL_WANT[stay]}" ;;
        esac
    done
}

# The keyboard of a reading that the second look does not confirm (Android 11 keeps `mInputShown=true` a while) is no finding,
# and no Back key goes to whatever is in front.
baseline_drop_stale_ime() {
    local k shown
    local -a keep=()
    case " ${BL_BAD[*]-} " in *" ime "*) ;; *) return 0 ;; esac
    on_emulator || return 0
    shown=$(dev dumpsys input_method 2> /dev/null)
    case "$shown" in '' | *mInputShown=true*) return 0 ;; esac
    # The app says it is up: it is.
    baseline_app_in_front && [ "$(baseline_app_keyboard)" = up ] && return 0
    for k in "${BL_BAD[@]}"; do [ "$k" = ime ] || keep+=("$k"); done
    BL_BAD=(${keep[@]+"${keep[@]}"})
}

# What the system says about the keyboard at the moment a reading finds it up, kept before it is put back: who asked for it
# (explicit or implicit), which window has the focus, touch mode, and the log lines of the input method of the scenario before.
baseline_ime_evidence() { # unit
    on_emulator || return 0
    {
        echo "=== $1 $(date +%T)"
        dev dumpsys input_method 2> /dev/null | grep -E 'mShowRequested|mShowExplicitlyRequested|mInputShown|mCurFocusedWindow|mCurId|mCurClient|mImeWindowVis' | cut -c1-260
        dev dumpsys window 2> /dev/null | grep -E 'mCurrentFocus|mInTouchMode' | head -4
        echo "--- log"
        adb_ logcat -d -b all -v time 2> /dev/null | tr -d '
' | grep -E 'ImeTracker|InputMethodManager|showSoftInput|hideSoftInput|IME_INSETS|InputMethodService|startInput|WINDOW_FOCUS_GAIN' | tail -40 | cut -c1-260
    } >> "$OUT/ime_evidence.txt" 2> /dev/null
}

baseline_note() { # unit text
    echo "NOTE  $1  $2"
    echo "NOTE  $1  $2" >> "$RESULTS"
}

# The check before a scenario `$1` (the name the runner got). Returns 1 with BL_INFRA set when the baseline cannot be
# reached; the scenario is then not run.
baseline_unit() {
    local s=$1 round k left="" lockfirst="" prev=${BL_PREV:+" ($BL_PREV)"} was_off=${SCREEN_WENT_OFF-}
    local -a first=()
    local -A why=()
    BL_INFRA=""
    shells_open || open_shells
    if ! baseline_read; then
        on_emulator || return 0
        BL_INFRA="the device did not answer"
        return 1
    fi
    if [ -z "$BL_REF" ]; then
        BL_REF=1
        baseline_want_init
        for k in $BL_KEYS; do [ -n "${BL_VAL[$k]-}" ] || left="$left $k"; done
        [ -z "$left" ] || echo "BASELINE  not readable on this device:$left"
        left=""
    fi
    printf '%s %s\n' "$s" "${BL_RAW//$'\n'/ }" >> "$OUT/baseline.txt"
    baseline_diffs
    baseline_drop_stale_ime
    if [ ${#BL_BAD[@]} -eq 0 ]; then
        baseline_watch_from_read
        return 0
    fi
    first=("${BL_BAD[@]}")
    case " ${first[*]} " in *" ime "*) baseline_ime_evidence "$s" ;; esac
    # The lock screen in front of the focus is one finding, not two.
    case " ${first[*]} " in *" keyguard "*) ! baseline_is_lock || lockfirst=1 ;; esac
    for k in "${first[@]}"; do why[$k]=${BL_WHY[$k]}; done
    if ! on_emulator; then
        for k in "${first[@]}"; do
            baseline_note "$s" "changed during the run$prev: ${why[$k]} (a phone: not put back)"
            BL_WANT[$k]=${BL_VAL[$k]-}
        done
        baseline_watch_from_read
        return 0
    fi
    for round in 1 2 3 4; do
        baseline_repair "$round"
        # A change of focus, the keyboard and the shade take a moment to show in the dumps: read at once, a round later
        # puts back what is already on its way and the baseline would look stuck on a busy runner.
        sleep 1
        baseline_read || { BL_INFRA="the device stopped answering while the baseline was put back"; return 1; }
        baseline_diffs
        baseline_drop_stale_ime
        [ ${#BL_BAD[@]} -gt 0 ] || break
    done
    for k in "${first[@]}"; do
        case " ${BL_BAD[*]} " in
            *" $k "*) ;;
            *)
                # After a restart nothing was left by a scenario: the system reports assumed values until it has answered.
                [ -z "${BL_RESTARTED-}" ] || continue
                # No scenario ran before the first one of a run.
                [ -n "$BL_PREV" ] || continue
                [ "$k" != focus ] || [ -z "$lockfirst" ] || continue
                # The screen that a scenario switched off on purpose, and the lock screen of a restart the scenario meant, are no finding.
                case "$k" in wake | keyguard | focus) { [ -z "$was_off" ] && [ -z "${BL_BOOTED-}" ]; } || continue ;; esac
                baseline_note "$s" "the scenario before$prev left: ${why[$k]} (put back)"
                ;;
        esac
    done
    if [ ${#BL_BAD[@]} -gt 0 ]; then
        # What is off in the last reading, not in the first.
        for k in "${BL_BAD[@]}"; do left="$left; ${BL_WHY[$k]}"; done
        BL_INFRA="${left#; } and cannot be put back"
        return 1
    fi
    baseline_watch_from_read
}
baseline_watch_from_read() {
    WD_SS=${BL_VAL[ss]-} WD_AUDIO=${BL_VAL[audio]-} WD_BOOT=${BL_VAL[bootid]-}
}

# A restart that the scenario means (s11, s13 and the like): reboot_only calls the first when it starts, so that the new
# boot is not a finding, and the second when the device is back, so that what breaks later in the same scenario is.
# BL_BOOTED: a restart boots to the lock screen, which the next baseline reading may still find: no finding about the scenario.
watchdog_announce_restart() { WD_SS="" WD_AUDIO="" WD_BOOT="" BL_BOOTED=1 LINK_MEANT=1; }
watchdog_rebase() {
    local out line
    WD_SS="" WD_AUDIO="" WD_BOOT=""
    out=$(adb_ shell "$WATCH_SCRIPT" 2> /dev/null)
    out=${out//$'\r'/}
    while IFS= read -r line; do
        case "$line" in
            ss=*) WD_SS=${line#*=} ;;
            audio=*) WD_AUDIO=${line#*=} ;;
            bootid=*) WD_BOOT=${line#*=} ;;
        esac
    done <<< "$out"
}

# After the scenario, pass or fail. WD_WHAT is empty when the device is whole.
watchdog_end() {
    local out line reason="" areason=""
    local -A W=()
    local try
    WD_WHAT=""
    WD_AUDIO_ONLY=""
    for try in 1 2; do
        out=$(dev "$WATCH_SCRIPT" 2> /dev/null)
        while IFS= read -r line; do
            case "$line" in [a-z]*=*) W[${line%%=*}]=${line#*=} ;; esac
        done <<< "$out"
        [ -z "${W[boot]+x}" ] || break
        # No answer: the connection to the device may be down (adb offline for a while) and the device whole. Wait for the
        # connection once and ask again; a device that is really gone gives no answer the second time either.
        [ "$try" = 1 ] && on_emulator && link_restore || break
    done
    if [ -z "${W[boot]+x}" ]; then
        reason="the device did not answer after the scenario"
    else
        [ -z "$WD_BOOT" ] || [ "$WD_BOOT" = "${W[bootid]-}" ] || reason="the device restarted (a new boot)"
        if [ -n "$WD_SS" ] && [ "$WD_SS" != "${W[ss]-}" ]; then
            reason="${reason:+$reason; }system_server restarted (pid $WD_SS, now ${W[ss]:-none})"
        fi
        if [ -n "$WD_AUDIO" ] && [ "$WD_AUDIO" != "${W[audio]-}" ]; then
            areason="the audio server restarted (pid $WD_AUDIO, now ${W[audio]:-none})"
        fi
        [ -z "${W[acrash]-}" ] || areason="${areason:+$areason; }crash in the system log: ${W[acrash]:0:140}"
        [ -z "${W[crash]-}" ] || reason="${reason:+$reason; }crash in the system log: ${W[crash]:0:140}"
    fi
    # Only the audio server: a unit that passed keeps its PASS (run_unit decides).
    [ -z "$areason" ] || { [ -n "$reason" ] || WD_AUDIO_ONLY=1; reason="${reason:+$reason; }$areason"; }
    [ -n "$reason" ] || return 0
    if on_emulator; then
        WD_WHAT=$reason
    else
        baseline_note "${CURRENT:-?}" "the device changed during the scenario ($reason); a phone is not restarted"
    fi
}

# The connection to the device (adb's own state for it) while a unit runs. On a loaded GitHub machine the emulator can drop off
# adb for half a minute and come back with the guest untouched (run 45191082131, afg15, s25@1600x2560 and s49@2208x1840:
# "adb: device offline", "waiting for device", "device 'emulator-5554' not found"). What the scenario then says is about the
# missing connection, not about the app. A poller writes every moment the state is not `device` into $LINK_LOG, and adb_
# (lib.sh) adds the lines adb itself printed about it. Only on a GitHub machine (Linux) or with LINK_WATCH set.
link_state() { timeout 5 "$ADB" -s "$SERIAL" get-state 2>&1 | tr -d '\r'; }
link_watch_wanted() { [ -n "${LINK_WATCH:-}" ] || { on_emulator && [ "$(uname -s)" = Linux ]; }; }
link_watch_start() { # unit
    LINK_MEANT=""
    link_watch_wanted || return 0
    LINK_LOG="$OUT/$1.link.txt"
    : > "$LINK_LOG"
    (
        local s
        while :; do
            s=$(link_state)
            [ "$s" = device ] || printf '%s adb state: %s\n' "$SECONDS" "${s:0:100}" >> "$LINK_LOG"
            sleep "${LINK_POLL:-2}"
        done
    ) > /dev/null 2>&1 &
    LINK_PID=$!
}
link_watch_stop() {
    [ -z "$LINK_PID" ] || { kill "$LINK_PID" 2> /dev/null; wait "$LINK_PID" 2> /dev/null; }
    LINK_PID=""
}
# What adb printed on its own stderr, passed on: the lines about the connection also go into $LINK_LOG.
link_stderr() {
    local l
    while IFS= read -r l; do
        printf '%s\n' "$l" >&2
        case "$l" in
            *"device offline"* | *"waiting for device"* | *"device '"*"' not found"*)
                [ -z "${LINK_LOG:-}" ] || printf '%s adb said: %s\n' "$SECONDS" "${l:0:100}" >> "$LINK_LOG"
                ;;
        esac
    done
}
# True, with a short description on stdout, when the connection dropped while the unit ran and no restart was meant.
link_dropped() {
    local l n=0 first=""
    [ -z "$LINK_MEANT" ] && [ -n "$LINK_LOG" ] && [ -s "$LINK_LOG" ] || return 1
    while IFS= read -r l; do
        [ -n "$first" ] || first=${l#* }
        n=$((n + 1))
    done < "$LINK_LOG"
    printf '%s (%s lines in %s)\n' "${first:0:100}" "$n" "${LINK_LOG##*/}"
}
# Wait until the device is connected again, and open the shells that went with the old connection.
link_wait() { timeout 150 "$ADB" -s "$SERIAL" wait-for-device > /dev/null 2>&1; }
link_restore() {
    link_wait || return 1
    shells_open || open_shells
}

# INFRA instead of the result lines the scenario wrote since RES_MARK (its PASS, FAIL and SKIP lines; a NOTE stays).
infra_record() { # unit started what follow
    local s=$1 t=$(($SECONDS - $2)) what=$3 follow=$4 i n=0 line
    local -a all out
    mapfile -t all < "$RESULTS"
    out=("${all[@]:0:RES_MARK}")
    for ((i = RES_MARK; i < ${#all[@]}; i++)); do
        line=${all[i]}
        if [[ $line =~ ^(PASS|FAIL|SKIP)\ \ ([^\ ]+)\ +(\[[0-9]+s\])\ (.*)$ ]]; then
            out+=("INFRA  ${BASH_REMATCH[2]}  ${BASH_REMATCH[3]} $what; the scenario said: ${BASH_REMATCH[1]} ${BASH_REMATCH[4]:0:140}$follow")
            n=$((n + 1))
        else
            out+=("$line")
        fi
    done
    [ "$n" -gt 0 ] || out+=("INFRA  $s  [${t}s] $what$follow")
    printf '%s\n' "${out[@]}" > "$RESULTS"
    for ((i = RES_MARK; i < ${#out[@]}; i++)); do
        case "${out[i]}" in "INFRA "*) echo "${out[i]}" ;; esac
    done
}

# What the log of the device said, before a restart takes it away.
infra_evidence() { # unit
    timeout 30 "$ADB" -s "$SERIAL" logcat -d -b main -b system -b crash -t 3000 -v time > "$OUT/$1.infra.log" 2>&1
}

# What a scenario keeps in bash variables for later (retry counters, the "s13d is done" mark, extra alarm fields, a screen
# size name). The second run of a scenario starts without them, as if the first had not been.
unit_forget() {
    unset S13D_DONE S23B_TRY S26_AGAIN S28_AGAIN
    ALARM_EXTRA="" OVERLAY="" VARIANT="" RETRY_NOTE="" RETRY_CARRY=""
}

# True when the scenario wrote a FAIL line since RES_MARK.
unit_wrote_fail() {
    local l i
    local -a now
    mapfile -t now < "$RESULTS"
    for ((i = RES_MARK; i < ${#now[@]}; i++)); do
        l=${now[i]}
        case "$l" in "FAIL "*) return 0 ;; esac
    done
    return 1
}

# A fault of the device image that is known: what the system log says when it happens, per unit. s53 on the Android 11 image of
# the CI (msd30): the settings screen of the fix button for "Stay allowed when unused" is launched and system_server crashes
# in the system's own code (`FATAL EXCEPTION IN SYSTEM PROCESS`, a SecurityException when it binds the permission controller's
# RoleControllerService as itself, run 45169682262 and the run before). The app is not in that stack. A unit that broke the
# device like this the first time runs again on a restarted device and says so in a NOTE; the second break stands as INFRA.
KNOWN_FAULT_s53='Unable to find app for caller .* when binding service .*RoleControllerService'
# Prints the log line when the log of the device holds the signature of this unit's known fault (every buffer: the crash text of
# the system process was not in the main, system and crash buffers that the watchdog reads, run 45169682262).
known_fault() { # unit
    local var="KNOWN_FAULT_${1%%@*}" sig line
    sig=${!var-}
    [ -n "$sig" ] || return 1
    line=$(dev "logcat -d -b all 2> /dev/null | grep -m1 -E '$sig'" 2> /dev/null | tr -d '\r')
    # The log that a failed scenario keeps (its own 30000 lines of every buffer, written when it failed) and the evidence file.
    [ -n "$line" ] || line=$(grep -a -m1 -h -E "$sig" "$OUT/${CURRENT:-$1}.log" "$OUT/$1.infra.log" 2> /dev/null | tr -d '\r')
    [ -n "$line" ] || return 1
    printf '%s\n' "${line:0:200}"
}

# The results the unit wrote since RES_MARK become one NOTE (what the scenario said stays in it); other lines stay.
known_record() { # unit what [lead]
    local s=$1 what=$2 i said="" line
    local lead=${3:-"the first run met a known fault of this device image ($2); the device is restarted and the scenario runs again"}
    local -a all out
    mapfile -t all < "$RESULTS"
    out=("${all[@]:0:RES_MARK}")
    for ((i = RES_MARK; i < ${#all[@]}; i++)); do
        line=${all[i]}
        if [[ $line =~ ^(PASS|FAIL|SKIP)\ \ ([^\ ]+)\ +(\[[0-9]+s\])\ (.*)$ ]]; then
            [ -n "$said" ] || said="${BASH_REMATCH[1]} ${BASH_REMATCH[4]:0:140}"
        else
            out+=("$line")
        fi
    done
    out+=("NOTE  $s  $lead; the first run said: ${said:-nothing}")
    printf '%s\n' "${out[@]}" > "$RESULTS"
    echo "${out[${#out[@]} - 1]}"
}

# FLAKY: a scenario that FAILs runs once more on the same device (no restart: a second failure in the same state is the stronger
# signal, and a restart would hide what the first one left). The first run's PASS, FAIL and SKIP lines are held back while it
# runs; when the scenario is settled, a FAIL that the rerun passed is one FLAKY line with both results, and a FAIL that the rerun
# failed again stays a FAIL (the first result in its text). What the first failure kept (<scenario>.* files: picture, tree, log,
# story) is copied to first_<file> before the rerun can write over it. INFRA keeps its own path. FLAKY_RERUN=0 turns it off.
flaky_wanted() { [ "${FLAKY_RERUN-1}" != 0 ] && on_emulator; }
FLAKY_FIRST=()
FLAKY_RE='^(PASS|FAIL|SKIP)\ \ ([^\ ]+)\ +(\[[0-9]+s\])\ (.*)$'
flaky_stash() { # unit
    local s=$1 line i name f said="" n=0
    local -a all keep=()
    mapfile -t all < "$RESULTS"
    FLAKY_FIRST=()
    for ((i = 0; i < ${#all[@]}; i++)); do
        line=${all[i]}
        if [ "$i" -ge "$RES_MARK" ] && [[ $line =~ $FLAKY_RE ]]; then
            FLAKY_FIRST+=("$line")
            if [ "${BASH_REMATCH[1]}" = FAIL ]; then
                name=${BASH_REMATCH[2]}
                for f in "$OUT/$name".*; do [ -e "$f" ] && cp -f "$f" "$OUT/first_${f##*/}" 2> /dev/null; done
                said="${said:+$said; }$name: ${BASH_REMATCH[4]:0:100}"
                n=$((n + 1))
            fi
        else
            keep+=("$line")
        fi
    done
    : > "$RESULTS"
    [ ${#keep[@]} -eq 0 ] || printf '%s
' "${keep[@]}" >> "$RESULTS"
    FLAKY_MARK=${#keep[@]}
    echo "RERUN  $s  failed ($said): runs once more on the same device (the first run's files are first_*)"
}
flaky_settle() { # unit
    local line i name st
    local -a all out=()
    local -A first_st=() first_line=() seen=()
    [ ${#FLAKY_FIRST[@]} -gt 0 ] || return 0
    for line in "${FLAKY_FIRST[@]}"; do
        [[ $line =~ $FLAKY_RE ]] || continue
        first_st[${BASH_REMATCH[2]}]=${BASH_REMATCH[1]}
        first_line[${BASH_REMATCH[2]}]="${BASH_REMATCH[1]} ${BASH_REMATCH[4]:0:140}"
    done
    mapfile -t all < "$RESULTS"
    out=("${all[@]:0:FLAKY_MARK}")
    for ((i = FLAKY_MARK; i < ${#all[@]}; i++)); do
        line=${all[i]}
        if [[ $line =~ $FLAKY_RE ]]; then
            name=${BASH_REMATCH[2]} st=${BASH_REMATCH[1]}
            seen[$name]=1
            if [ "${first_st[$name]-}" = FAIL ]; then
                if [ "$st" = PASS ]; then
                    line="FLAKY  $name  ${BASH_REMATCH[3]} failed the first run (${first_line[$name]#FAIL }) and passed the second (${BASH_REMATCH[4]:0:100})"
                elif [ "$st" = FAIL ]; then
                    line="FAIL  $name  ${BASH_REMATCH[3]} ${BASH_REMATCH[4]} (failed twice; first run: ${first_line[$name]#FAIL })"
                fi
            fi
        fi
        out+=("$line")
    done
    # A scenario of the first run that the second one wrote nothing for (the rerun was INFRA, or ended early) keeps its first line.
    for line in "${FLAKY_FIRST[@]}"; do
        [[ $line =~ $FLAKY_RE ]] || continue
        [ -n "${seen[${BASH_REMATCH[2]}]-}" ] || out+=("$line")
    done
    : > "$RESULTS"
    printf '%s
' "${out[@]}" >> "$RESULTS"
    for line in "${out[@]:FLAKY_MARK}"; do
        case "$line" in "FLAKY "* | "FAIL "*"(failed twice;"*) echo "$line" ;; esac
    done
    FLAKY_FIRST=()
}

# One scenario of the runner: the baseline, the scenario, the watchdog, and the one rerun on a restarted device.
# A unit that runs a function of another name (one scenario at one screen size) passes it as the second word.
# The seconds each unit took (with its baseline, its rerun and its restart): unit_times.txt of the results, one `name seconds` line per
# call. devtest/ci/split.py --measure reads it to balance the two parts of a full run.
run_unit() { # name [function]
    local ut0=$SECONDS rc
    run_unit_once "$@"
    rc=$?
    echo "$1 $((SECONDS - ut0))" >> "$OUT/unit_times.txt"
    return $rc
}
run_unit_once() { # name [function]
    local s=$1 fn=${2:-$1} attempt=1 what t0 follow bl_ok known link linkfault
    while :; do
        t0=$SECONDS
        what=""
        mapfile -t RES_ALL < "$RESULTS"
        RES_MARK=${#RES_ALL[@]}
        baseline_unit "$s" && bl_ok=1 || bl_ok=""
        BL_RESTARTED="" BL_BOOTED=""
        link="" linkfault=""
        if [ -n "$bl_ok" ]; then
            link_watch_start "$s"
            "$fn"
            link_watch_stop
            watchdog_end
            what=$WD_WHAT
            # An audio server that restarted while the scenario passed is the device's, not a verdict on the app.
            if [ -n "$what" ] && [ -n "$WD_AUDIO_ONLY" ] && ! unit_wrote_fail; then
                baseline_note "$s" "the scenario passed although $what"
                what=""
            fi
        else
            what="the device is not at its baseline: $BL_INFRA"
        fi
        # The connection to the device dropped while the scenario ran (and the device is whole): what a failed scenario then says is
        # about the missing connection. A passed one keeps its PASS.
        if [ -z "$what" ] && link=$(link_dropped); then
            if unit_wrote_fail; then
                what="the connection to the device dropped during the scenario (adb: $link)"
                linkfault=1
            else
                baseline_note "$s" "the connection to the device dropped while the scenario ran and it passed (adb: $link)"
            fi
        fi
        if [ -z "$what" ]; then
            if [ "$attempt" = 1 ] && [ ${#FLAKY_FIRST[@]} -eq 0 ] && flaky_wanted && unit_wrote_fail; then
                flaky_stash "$s"
                BL_PREV=$s
                unit_forget
                continue
            fi
            flaky_settle "$s"
            BL_PREV=$s
            return 0
        fi
        follow=""
        [ "$attempt" != 1 ] || follow=" [rerun follows]"
        infra_evidence "$s"
        evidence_snapshot infra "$s" "$what"
        if [ "$attempt" = 1 ] && [ -n "$bl_ok" ] && known=$(known_fault "$s"); then
            known_record "$s" "$known"
        elif [ "$attempt" = 1 ] && [ -n "$linkfault" ]; then
            known_record "$s" "$link" "the first run lost the connection to the device (adb: $link); the connection is waited for and the scenario runs again"
        else
            infra_record "$s" "$t0" "$what" "$follow"
            BL_PREV=$s
            [ "$attempt" = 1 ] || { flaky_settle "$s"; return 0; }
            echo "INFRA  $s  the device is restarted and the scenario runs once more"
        fi
        BL_PREV=$s
        if [ -n "$linkfault" ] && link_restore; then
            :
        elif ! reboot_and_wait; then
            echo "STOP  $s  the device did not come back after the restart ($what)" | tee -a "$RESULTS"
            exit 2
        fi
        attempt=2
        unit_forget
        BL_RESTARTED=1
    done
}

# True when a results file holds an INFRA that no second run followed.
infra_standing() { # results file
    local l
    [ -r "$1" ] || return 1
    while IFS= read -r l || [ -n "$l" ]; do
        case "$l" in
            "INFRA "*)
                case "$l" in *"[rerun follows]") ;; *) return 0 ;; esac
                ;;
        esac
    done < "$1"
    return 1
}
# 1 when any FAIL, else 4 when an INFRA stands, else 0.
run_exit_code() { # results file
    local l
    if [ -r "$1" ]; then
        while IFS= read -r l || [ -n "$l" ]; do
            case "$l" in "FAIL "*) return 1 ;; esac
        done < "$1"
    fi
    infra_standing "$1" && return 4
    return 0
}
