#!/bin/bash
# Android's own screens (the lock screen and its PIN pad, the shade, a file picker, the launcher, Settings): what the
# suite can know about them, and how it acts there. Sourced by lib.sh.
#
# These screens are not the app's window, so the app cannot say what is on them. The device can: the focused window, the
# top activity, the user and keyguard state, the panel state of the system UI, and the tree of `uiautomator dump` (which
# works on the lock screen, the shade and the file picker: the runs of probe.sh on Android 10, 11, 12, 14 and 16). Every
# step on such a screen follows one rule (plan alarmfree-devtest-no-flake, rule 6):
#   1. await a state, read from the device, that says the screen is ready for the action (a read can be repeated: it changes
#      nothing),
#   2. act once,
#   3. confirm with a state read that the action took,
#   4. act again only when a read proves that the first action did nothing (never blind),
#   5. every wait is a poll of a named state with a deadline (SYS_DEADLINE, 30 s) and, when it passes, a failure that says which
#      state did not come and what the device showed (SYS_WHY).
#
# Scenarios read as: `sys_wait 30 "the PIN pad is up" pad_ready || problems=...; act; sys_wait 30 "the phone is unlocked" ...`.

SYS_DEADLINE="${SYS_DEADLINE:-30}"
SYS_XML=""
SYS_WHY=""
_sys_here=${BASH_SOURCE[0]%/*}
[ "$_sys_here" != "${BASH_SOURCE[0]}" ] || _sys_here=.
SYS_PY="$_sys_here/sys_nodes.py"
PIN_CODE=1234

# Milliseconds on this computer's clock (bash 5).
now_ms() {
    local t=${EPOCHREALTIME/[.,]/}
    echo $((t / 1000))
}

# The line of the window that has the keys. `2> /dev/null`: dumpsys writes "Failed to write while dumping service" to its
# stderr when grep ends the pipe early, and the device shell here joins stderr into the answer.
sys_focus() { focused_window | sed -E 's/.*mCurrentFocus=//; s/^Window\{[0-9a-f]+ u0 //; s/\}$//'; }

# One line that says what the device shows, for the message of a wait that ran out.
sys_top() { dev "dumpsys activity activities 2> /dev/null | grep -E 'topResumedActivity|ResumedActivity:' | head -1 | sed -E 's/.* u0 ([^ ]+) .*/\\1/'" | tr -d '\n'; }
sys_show() {
    echo "focus=$(sys_focus | tr -d '\n') top=$(sys_top) keyguard=$(kg_state) user=$(user_unlocked && echo unlocked || echo locked) screen=$(wakefulness)"
}

# A dialog "<something> isn't responding" in front. Of Android itself (System UI after a boot on Android 15, a Settings
# page): a named obstacle, put away with its Wait button, a NOTE that it was there, and the wait goes on. Of the app under test:
# never put away, that is a failure of the app (SYS_WHY says so, status 2).
sys_anr_check() {
    local f xy now
    now=$(now_ms)
    [ $((now - ${SYS_ANR_AT:-0})) -ge 3000 ] || return 0
    SYS_ANR_AT=$now
    f=$(sys_focus)
    case "$f" in
        *"Not Responding"*) ;;
        *) return 0 ;;
    esac
    case "$f" in
        *"Not Responding: $PKG"*)
            SYS_WHY="the app did not respond: its not-responding dialog is in front ($f)"
            return 2
            ;;
    esac
    if sys_dump && xy=$(sys_nodes find rid~aerr_wait) || { sys_dump && xy=$(sys_nodes find label=Wait); }; then
        # shellcheck disable=SC2086
        dev input tap $xy > /dev/null
        echo "NOTE  ${CURRENT:-?}  Android's own screen did not respond ($f): its dialog was put away with Wait" | tee -a "$RESULTS"
        sleep 1
    fi
    return 0
}

# Poll a named state: sys_wait SECONDS "what" command...  Returns 1 and sets SYS_WHY when the deadline passes. The check is
# run at least once, also with a deadline of 0. A not-responding dialog of Android is put away on the way (sys_anr_check).
sys_wait() {
    local secs=$1 what=$2 end
    shift 2
    end=$(($(now_ms) + secs * 1000))
    until "$@"; do
        sys_anr_check || return 1
        if [ "$(now_ms)" -ge "$end" ]; then
            SYS_WHY="$what did not come within ${secs}s ($(sys_show))"
            return 1
        fi
        sleep 0.2
    done
    SYS_WHY=""
}
# The other way round, for what must stay: the state holds at every look for SECONDS (a camera that keeps the keys).
sys_holds() {
    local secs=$1 what=$2 end
    shift 2
    end=$(($(now_ms) + secs * 1000))
    while :; do
        "$@" || { SYS_WHY="$what did not hold ($(sys_show))"; return 1; }
        [ "$(now_ms)" -lt "$end" ] || break
        sleep 0.2
    done
    SYS_WHY=""
}

# ---- Reads ----
# The lock screen: true, false, or ? when this Android version prints neither of its two places for it. Android 10 has no
# isKeyguardShowing in the displays dump; its policy dump says `showing=`.
kg_state() {
    local w
    w=$(dev "dumpsys window displays 2> /dev/null | grep -m1 -o 'isKeyguardShowing=[a-z]*'")
    w=${w#*=}
    if [ -z "$w" ]; then
        w=$(dev "dumpsys window policy 2> /dev/null | grep -m1 -o '[ ]showing=[a-z]*'")
        w=${w#*=}
    fi
    echo "${w:-?}"
}
kg_showing() { [ "$(kg_state)" = true ]; }
kg_gone() { [ "$(kg_state)" = false ]; }
screen_awake() { [ "$(wakefulness)" = Awake ]; }
screen_asleep() { [ "$(wakefulness)" != Awake ]; }
# A lock credential is set: true, false or ? (Android 10 prints no `secure=`).
pin_state() {
    local x
    # Android 9 prints secure= in that dump but never updates it (false with a PIN set, checked with `locksettings set-pin`). A
    # credential there is the file that names the synthetic password in use, which the emulator's root can see.
    if [ "$(sdk_level)" = 28 ]; then
        if [ -n "$(dev "su 0 ls /data/system_de/0/spblob/0000000000000000.handle 2> /dev/null")" ]; then echo true; else echo false; fi
        return
    fi
    x=$(dev "dumpsys window policy 2> /dev/null | grep -m1 -o 'secure=[a-z]*'")
    echo "${x#*=}"
}
pin_unset() { [ "$(pin_state)" != true ]; }
pin_is_set() { [ "$(pin_state)" != false ]; }
# The soft keyboard (Android 14 and 16 leave theirs up after the PIN).
ime_shown() { dev "dumpsys input_method 2> /dev/null | grep -c 'mInputShown=true'" | grep -q -v '^0'; }
ime_hidden() { ! ime_shown; }
# What the input method service says, for the record (ime.txt in the results).
ime_note() { printf '%s %s %s: %s
' "$(date +%T)" "${CURRENT:-?}" "$1" "$(dev "dumpsys input_method 2> /dev/null | grep -E 'mInputShown|mImeWindowVis|mShowRequested|mCurFocusedWindow=|mCurId' | tr -s ' ' | tr '
' ';'")" >> "$OUT/ime.txt"; }
unlocked_and_clear() { user_unlocked && kg_gone; }

# The tree of the screen in SYS_XML (all windows with `--windows`); 1 when this dump gave none. Dumping fails now and then on
# a busy device (an error text instead of the tree): that is a read that did not happen, so the caller asks again.
sys_dump() { # [--windows]
    SYS_XML=$(dev "timeout 20 uiautomator dump ${1:-} /data/local/tmp/af_sys.xml > /dev/null 2>&1; cat /data/local/tmp/af_sys.xml 2> /dev/null; rm -f /data/local/tmp/af_sys.xml")
    [[ $SYS_XML == *"<hierarchy"* ]]
}
# Ask the tree in SYS_XML (see sys_nodes.py for the modes and filters).
sys_nodes() { printf '%s' "$SYS_XML" | python -I "$SYS_PY" "$@"; }
# A fresh dump, then the question: success only when the dump worked and the answer holds.
sys_has() { sys_dump "${SYS_WINDOWS:-}" && [ "$(sys_nodes count "$@")" -gt 0 ] 2> /dev/null; }
# "x y" of the first node that matches, from a fresh dump; empty when the dump failed or nothing matches.
sys_where() {
    sys_dump "${SYS_WINDOWS:-}" || return 1
    sys_nodes find "$@"
}

# A screenshot, the tree and the focus of this moment, for the failure of a scenario: what the screen was.
sys_evidence() { # name
    local name=$1
    screen_may_be_taken && adb_ exec-out screencap -p > "$OUT/${name}${VARIANT:-}.png" 2> /dev/null
    {
        echo "== $(sys_show)"
        echo "-- the app says: $(UI_QUIET=1 ui ready 2>&1 | head -c 500)"
        echo "-- tree"
        if sys_dump; then sys_nodes list | head -160; else echo "(no dump)"; fi
    } > "$OUT/${name}${VARIANT:-}.screen.txt" 2>&1
}

# ---- Keys with an awaited outcome ----
# A key once, and then the state it must bring: sys_press KEYCODE "what comes" SECONDS check...  Keys are not repeated: the
# input dispatcher keeps what it was given, and a second key on a screen that has already moved on lands on the next one.
sys_press() {
    local key=$1 what=$2 secs=$3
    shift 3
    dev input keyevent "$key" > /dev/null
    sys_wait "$secs" "$what" "$@"
}
# The power button: asleep, then awake.
sys_sleep() { # [seconds]
    [ -z "${NO_LOCK:-}" ] || return 0
    SCREEN_WENT_OFF=1
    sys_press KEYCODE_SLEEP "the screen going to sleep" "${1:-$SYS_DEADLINE}" screen_asleep
}
sys_wake() { sys_press KEYCODE_WAKEUP "the screen waking up" "$SYS_DEADLINE" screen_awake; }
# Home, and the state that says the screen it left is gone.
sys_home() { # what check...
    local what=$1
    shift
    sys_press KEYCODE_HOME "$what" "$SYS_DEADLINE" "$@"
}
# The keyboard down: it hides itself after the PIN on some versions; when the read still says so after 8 s, Back is pressed once
# (it closes a keyboard that is up); when after 8 more seconds it is still up (Android 14 to 16 after the PIN: Back does not
# take it down, run 45085128218), the input method is stopped, which ends the state (the system starts it again when a field
# wants it).
sys_keyboard_down() {
    sys_wait 8 "the keyboard going down by itself" ime_hidden && return 0
    dev input keyevent KEYCODE_BACK > /dev/null
    sys_wait 8 "the keyboard going down after Back" ime_hidden && return 0
    baseline_ime_stop
    sys_wait "$SYS_DEADLINE" "the keyboard going down after the input method was stopped" ime_hidden
}

# ---- A touch on one of Android's own screens ----
# The signature of the screen: a checksum of the tree, to see that a touch changed something. Empty when the dump failed.
sys_sig() {
    sys_dump "${SYS_WINDOWS:-}" || return 1
    sys_nodes list | cksum | cut -d' ' -f1
}
# Read the screen, touch the first node that matches once, then wait for the screen to change. 0: it changed (the touch
# took). 1: nothing changed in SECONDS, which is the proof that the touch went nowhere (the caller may ask again, from a
# fresh read). 2: no node matched or the dump gave no tree: nothing was touched.
sys_tap() { # seconds filter...
    local secs=$1 xy before end now
    shift
    sys_dump "${SYS_WINDOWS:-}" || return 2
    xy=$(sys_nodes find "$@") || return 2
    before=$(sys_nodes list | cksum | cut -d' ' -f1)
    # shellcheck disable=SC2086
    dev input tap $xy > /dev/null
    end=$(($(now_ms) + secs * 1000))
    while :; do
        now=$(sys_sig) && [ "$now" != "$before" ] && return 0
        [ "$(now_ms)" -lt "$end" ] || return 1
        sleep 0.3
    done
}

# ---- Leaving a screen of Android for the app ----
# Back once from a screen of Android (Settings and its pages) until the app has the keys again. The screen must be drawn
# first (a tree with nodes of another package than the app's): a Back before that goes nowhere. A second Back only when a
# read after 15 s still shows that screen in front.
screen_of_android_drawn() {
    sys_dump || return 1
    [ "$(sys_nodes count "pkg!=$PKG" "pkg!=com.android.systemui")" -gt 2 ] 2> /dev/null
}
# A screen of Android that has closed by itself (the settings page of the app on Android 12, run 45085128218) leaves nothing to
# press Back on: the app has the keys, and a NOTE says so.
screen_drawn_or_app_back() { focus_in_app || screen_of_android_drawn; }
sys_leave() { # what
    local what=${1:-a screen of Android}
    sys_wait "$SYS_DEADLINE" "$what drawn" screen_drawn_or_app_back || return 1
    if focus_in_app; then
        echo "NOTE  ${CURRENT:-?}  $what closed by itself: the app already has the keys" | tee -a "$RESULTS"
        return 0
    fi
    dev input keyevent KEYCODE_BACK > /dev/null
    sys_wait 15 "the app getting the keys back from $what" focus_in_app && return 0
    focus_in_app && return 0
    screen_of_android_drawn || { sys_wait "$SYS_DEADLINE" "the app getting the keys back from $what" focus_in_app; return; }
    dev input keyevent KEYCODE_BACK > /dev/null
    sys_wait "$SYS_DEADLINE" "the app getting the keys back from $what (second Back, the first did nothing)" focus_in_app
}

# ---- The lock screen and its PIN pad (Android 10 to 16: the same ids on all of them) ----
# PAD_UP, PAD_FOCUS, PAD_LEN, PAD_MSG from one dump: the pad is there (`pinEntry`), the field has the focus (keys go into it;
# after a wrong PIN Android 14 takes it away for a while), how many digits it holds (one bullet each, -1 when it is not
# there), and the message over it. 1 when the dump gave no tree.
PAD_UP=0 PAD_FOCUS=0 PAD_LEN=-1 PAD_MSG=""
pad_read() {
    sys_dump || return 1
    PAD_UP=0 PAD_FOCUS=0 PAD_MSG=""
    PAD_LEN=$(sys_nodes len rid~/pinEntry)
    [ "$PAD_LEN" != -1 ] && PAD_UP=1
    [ "$(sys_nodes count rid~/pinEntry focused=true)" -gt 0 ] 2> /dev/null && PAD_FOCUS=1
    # Android 17's field is not a view that can have the focus (focusable=false, never focused): its keys still go in, through
    # the bouncer window.
    [ "$PAD_UP" = 1 ] && [ "$(sys_nodes count rid~/pinEntry focusable=true)" = 0 ] 2> /dev/null && PAD_FOCUS=1
    PAD_MSG=$(sys_nodes text rid~message_area)
    return 0
}
pad_ready() { pad_read && [ "$PAD_UP" = 1 ] && [ "$PAD_FOCUS" = 1 ]; }
pad_len_is() { pad_read && [ "$PAD_LEN" = "$1" ]; }
pad_gone() { pad_read && [ "$PAD_UP" = 0 ]; }

# The pad up and with the focus. `ask`: the bouncer is asked for (`wm dismiss-keyguard`, which has the same effect when it
# is already coming); asked again only when the last ask was 10 s ago and a read shows no pad.
pad_get() { # ask(1|0)
    local ask=${1:-1} end last=0 now
    end=$(($(now_ms) + SYS_DEADLINE * 1000))
    until pad_ready; do
        sys_anr_check || return 1
        now=$(now_ms)
        if [ "$now" -ge "$end" ]; then
            SYS_WHY="the PIN pad with the focus did not come within ${SYS_DEADLINE}s (pad up=$PAD_UP focus=$PAD_FOCUS message='$PAD_MSG'; $(sys_show))"
            return 1
        fi
        if [ "$ask" = 1 ] && [ "$PAD_UP" = 0 ] && [ $((now - last)) -ge 10000 ] && kg_showing; then
            dev wm dismiss-keyguard > /dev/null
            last=$now
        fi
        sleep 0.3
    done
    SYS_WHY=""
}
# Key n of the pad by a touch, for when the field has no focus for the keys.
pad_tap_key() { # digit|enter|delete
    local rid="key$1"
    case "$1" in enter) rid=key_enter ;; delete) rid=delete_button ;; esac
    sys_dump || return 1
    local xy
    xy=$(sys_nodes find "rid~/$rid") || return 1
    # shellcheck disable=SC2086
    dev input tap $xy > /dev/null
}
# The PIN typed once into the field and read back: the number of bullets must be the number of digits. Digits that are
# missing or too many show in that number; only a read that shows a wrong number for 10 s makes it empty the field (Delete
# as often as there are bullets) and type again.
pad_type() { # pin
    local pin=$1 want=${#pin} round=0 i
    while [ "$round" -lt 3 ]; do
        pad_get 0 || return 1
        if [ "$PAD_LEN" -gt 0 ]; then
            i=0
            while [ "$i" -lt "$PAD_LEN" ]; do
                dev input keyevent KEYCODE_DEL > /dev/null
                i=$((i + 1))
            done
            sys_wait "$SYS_DEADLINE" "the PIN field empty" pad_len_is 0 || return 1
        fi
        dev input text "$pin" > /dev/null
        if sys_wait 10 "the PIN field holding $want digits" pad_len_is "$want"; then
            SYS_WHY=""
            return 0
        fi
        round=$((round + 1))
    done
    SYS_WHY="the PIN field never held $want digits: $SYS_WHY (it holds $PAD_LEN)"
    return 1
}
# Unlock with the PIN the scenario set: the pad, the digits (read back), Enter once, and the phone unlocked with the lock
# screen gone. A wrong PIN cannot come from typing blind: the field was read before Enter, so a rejection is a finding.
# Emulator only: nothing here knows or sets the PIN of somebody's phone. `ask`: see pad_get.
unlock_pin() { # [ask(1|0)] [pin]
    local ask=${1:-1} pin=${2:-$PIN_CODE}
    on_emulator || { SYS_WHY="not an emulator"; return 1; }
    pad_get "$ask" || return 1
    pad_type "$pin" || return 1
    dev input keyevent KEYCODE_ENTER > /dev/null
    sys_wait "$SYS_DEADLINE" "the phone unlocked with the lock screen gone after Enter" unlocked_and_clear || {
        pad_read && SYS_WHY="$SYS_WHY; the pad says '$PAD_MSG', holds $PAD_LEN"
        return 1
    }
    # Android 14 and 16 leave their keyboard up over whatever comes next.
    ime_note "after the PIN"
    sys_keyboard_down || return 1
    # The shell as the app could not start while the phone was locked.
    sys_wait "$SYS_DEADLINE" "the app's shell answering after the unlock" app_shell_answers || return 1
    SYS_WHY=""
}

# The Unlock button of the SDK's unlock screen (a plain view that is no page of the app): its place from the tree.
unlock_button_xy() {
    UNLOCK_XY=$(sys_where pkg="$PKG" text=Unlock) && [ -n "$UNLOCK_XY" ] && [ "$UNLOCK_XY" != none ]
}
# Android's unlock cancelled with Back: the SDK's screen has the keys again (Back once, only when the pad is up).
sdk_unlock_cancel() {
    sys_wait "$SYS_DEADLINE" "Android's unlock being up" pad_ready || return 1
    sys_press KEYCODE_BACK "the unlock screen getting the keys back after Back" "$SYS_DEADLINE" unlock_screen_has_focus
}

# ---- The shade ----
# open | closed | ? from the system UI's own panel state (Android 10 `panelExpanded: true`, 11 `mPanelExpanded: true`,
# 12 and 14 `mPanelExpanded=true`, 16 `isPanelExpanded()=true`, all with `mExpandedVisible` on 10 to 12).
shade_state() {
    local x
    x=$(dev "dumpsys activity service com.android.systemui 2> /dev/null | grep -o -E 'anelExpanded(\\(\\))?[:=] ?(true|false)|mExpandedVisible=(true|false)'")
    case "$x" in
        *true*) case "$x" in *false*) echo '?' ;; *) echo open ;; esac ;;
        *false*) echo closed ;;
        *) echo '?' ;;
    esac
}
shade_open() {
    case "$(shade_state)" in
        open) return 0 ;;
        closed) return 1 ;;
    esac
    # Not readable on this version: the shade has the keys.
    sys_focus | grep -q -E 'NotificationShade|StatusBar' && kg_gone
}
shade_closed() {
    case "$(shade_state)" in
        closed) return 0 ;;
        open) return 1 ;;
    esac
    ! shade_open
}
# The shade open or closed, with the command that has that effect. The commands do nothing when the shade is already there,
# so asking again after a read that says it is not is safe (Android 16 ignores opening it while it is still closing).
shade_set() { # open|closed
    local want=$1 end now last=0 check=shade_open cmd=expand-notifications
    [ "$want" = closed ] && check=shade_closed cmd=collapse
    end=$(($(now_ms) + SYS_DEADLINE * 1000))
    until "$check"; do
        sys_anr_check || return 1
        now=$(now_ms)
        if [ "$now" -ge "$end" ]; then
            SYS_WHY="the shade did not get $want within ${SYS_DEADLINE}s ($(sys_show))"
            return 1
        fi
        if [ $((now - last)) -ge 5000 ]; then
            dev cmd statusbar "$cmd" > /dev/null 2>&1
            last=$now
        fi
        sleep 0.3
    done
    SYS_WHY=""
}
# The place of a button of a notification in the open shade, from a fresh tree (some Androids write it in capitals; some keep
# the shade out of the plain dump, some out of the all-windows one).
shade_button_xy() { # label
    local flag l
    for flag in "" "--windows"; do
        sys_dump "$flag" || continue
        for l in "$1" "${1^^}"; do
            SHADE_XY=$(sys_nodes find pkg=com.android.systemui label="$l") && return 0
        done
    done
    return 1
}
shade_button_gone() { # label
    local flag l
    for flag in "" "--windows"; do
        sys_dump "$flag" || return 1
        for l in "$1" "${1^^}"; do
            [ "$(sys_nodes count pkg=com.android.systemui label="$l")" = 0 ] || return 1
        done
    done
}
# A button of one of this app's notifications in the shade: the shade open (and again after a quiet 12 s, when the notification
# was folded away), the button's place read, one touch, and then what the touch must have done: the effect the scenario names
# (`check...`, a state of the app: the alarm skipped), or without one the button gone from the tree. The button gone alone says
# little (the shade can close for other reasons), so a scenario names its effect. The touch is repeated only when a read 20 s after
# it shows no effect and the button is there again.
tap_in_shade() { # label [check...]
    local label=$1 end opened now
    shift
    screen_awake || sys_wake || return 1
    kg_gone || { dev wm dismiss-keyguard > /dev/null; sys_wait "$SYS_DEADLINE" "the lock screen gone" kg_gone || return 1; }
    shade_set closed || return 1
    end=$(($(now_ms) + 90000))
    opened=0
    while [ "$(now_ms)" -lt "$end" ]; do
        sys_anr_check || return 1
        shade_set open || return 1
        [ "$opened" != 0 ] || opened=$(now_ms)
        if shade_button_xy "$label"; then
            # shellcheck disable=SC2086
            dev input tap $SHADE_XY > /dev/null
            if [ $# -gt 0 ]; then
                sys_wait 20 "the effect of the touch on $label in the shade" "$@" && return 0
            else
                sys_wait 20 "the $label button gone from the shade" shade_button_gone "$label" && return 0
            fi
            opened=0
            continue
        fi
        # No button: it was folded away, or the touch did its work and the effect is only late.
        if [ $# -gt 0 ] && "$@"; then return 0; fi
        # Android 9 opens the shade with its notifications folded after the first time (only the top one is open the first time):
        # the Expand button of a notification shows its buttons. The dump above is the one that is read.
        if SHADE_XY=$(sys_nodes find pkg=com.android.systemui rid~expand_button desc=Expand); then
            # shellcheck disable=SC2086
            dev input tap $SHADE_XY > /dev/null
            sleep 0.5
            continue
        fi
        now=$(now_ms)
        if [ $((now - opened)) -ge 12000 ]; then
            shade_set closed || return 1
            opened=0
        fi
        sleep 0.3
    done
    SYS_WHY="no effect of $label in the shade within 90s ($(sys_show))"
    return 1
}

# ---- The file picker (DocumentsUI: com.android.documentsui or com.google.android.documentsui) ----
# It opens on Recent files, which is empty: the roots drawer (the button described `Show roots`) lists Downloads; on
# Android 11 the picker of images has a folder `Download` instead.
picker_up() { sys_focus | grep -q documentsui; }
# Google's picker (Android 12 to 16 on the images with Google APIs): a touch on a file only focuses it (a text file) or opens
# its preview in another app (a sound, a picture: YouTube Music, Photos; runs 45079574283 and 45082824603), and the key Enter on the
# focused file chooses it. The picker of Android 10 and 11 takes the touch.
picker_is_google() { sys_focus | grep -q 'com.google.android.documentsui'; }
# What the picker shows, for the message of a failure.
picker_summary() {
    sys_dump && sys_nodes list | grep -E 'documentsui' | grep -E '\|(title|header_title)\||Show roots|dir_list|message' | cut -d'|' -f3-5 | head -12 | tr '\n' ';'
}
# Choose the file whose label has `name` in it. One action per read; the next action comes from the next read. A touch is
# repeated only when the tree did not change in 10 s after it. Android 11 ignores a touch on a file: after that proof it is
# chosen with the keys (the focus moved onto the row, then Enter).
picker_keys_select() { # name
    local name=$1 box fb i=0 keys=(KEYCODE_DPAD_DOWN KEYCODE_DPAD_RIGHT)
    sys_dump || return 1
    box=$(sys_nodes box "label~$name") || return 1
    while [ "$i" -lt 14 ]; do
        # Where the focus is, before the next key (a grid of two files in a row is reached by Right, a list by Down).
        if sys_dump && fb=$(sys_nodes focusbox); then
            # shellcheck disable=SC2086
            set -- $box $fb
            if [ "$#" = 8 ] && [ "$5" -le "$1" ] && [ "$6" -le "$2" ] && [ "$7" -ge "$3" ] && [ "$8" -ge "$4" ]; then
                dev input keyevent KEYCODE_ENTER > /dev/null
                return 0
            fi
        fi
        dev input keyevent "${keys[$((i % 2))]}" > /dev/null
        i=$((i + 1))
    done
    return 1
}
# What picker_pick did and saw, one line per step (picker_trace.txt in the results).
picker_trace() { printf '%s %s %s
' "$(date +%T.%N | cut -c1-12)" "${CURRENT:-?}" "$*" >> "$OUT/picker_trace.txt"; }
# The state for half a second after a touch, for the record: who has the keys, and a picture.
picker_watch() { # label rounds
    local i=0
    while [ "$i" -lt "$2" ]; do
        picker_trace "$1 +$i: $(sys_focus | tr -d '
')"
        # Nothing more to record once the app's own window has the keys again, and the toast with Undo that the picker's
        # result brings lives 6 s: 8 rounds of 0.5 s ate most of it (s48 on Android 17).
        focused_window | grep -q "$PKG/android.app.NativeActivity" && break
        i=$((i + 1))
        sleep 0.5
    done
    screen_may_be_taken && adb_ exec-out screencap -p > "$OUT/picker_$1.png" 2> /dev/null
}
# A screen of another app opened from the picker's task (the preview of an audio file or a photo): it is not the app and not
# the picker.
picker_preview_up() { sys_focus | grep -v -e documentsui -e "$PKG" -e '^$' -e '^null' | grep -q .; }
# The picker is gone: the app has the keys again. After a detour through the preview of the file the picker's task can leave the
# launcher in front (Android 13, run 45082824603): the app is then started again, and a NOTE says so.
# The crash of the picker's own process (run 45743443306, Android 11: a StaleDataException in DocumentsUI, 4 s after it came up,
# while the dump walked its list): the exception line, and a non-zero status when the picker did not crash. The log is cleared
# at the start of a scenario.
picker_crash() {
    local line
    line=$(dev "logcat -d -b crash,main -s AndroidRuntime:E 2> /dev/null" | tr -d '\r' | grep -A1 'Process: com\.[a-z.]*documentsui' | tail -1 | cut -c1-200)
    [ -n "$line" ] && printf '%s' "$line" | sed 's/^.*AndroidRuntime[^:]*: //'
}
picker_back_in_app() {
    sys_wait 15 "the app getting the keys back from the picker" focus_in_app && return 0
    picker_up && return 1
    echo "NOTE  ${CURRENT:-?}  the picker closed and the app was not in front ($(sys_focus | tr -d '
')): the app was started again" | tee -a "$RESULTS"
    start_app
    sys_wait "$SYS_DEADLINE" "the app in front after it was started again" focus_in_app
}
# One touch on the file. A preview that comes up is closed with Back (once), which brings the picker again.
picker_touch_file() { # name xy
    picker_trace "touch on $1 at $2"
    # shellcheck disable=SC2086
    dev input tap $2 > /dev/null
    picker_watch after_touch 8
    sys_wait 2 "the picker closing after the touch on $1" focus_in_app && return 0
    if picker_preview_up; then
        picker_trace "a preview is up: $(sys_focus | tr -d '
'): Back once"
        dev input keyevent KEYCODE_BACK > /dev/null
        sys_wait "$SYS_DEADLINE" "the picker coming back after Back from the preview" picker_up || return 1
        picker_watch after_back 2
    fi
    return 1
}
picker_keys_file() { # name
    picker_trace "keys for $1"
    picker_keys_select "$1"
    picker_watch after_keys 8
    sys_wait 15 "the picker closing after the keys on $1" focus_in_app
}
# Choose the file. The order of the ways depends on the picker: Google's (Android 12 to 16) only previews a file on a touch, so
# the keys come first, then a touch (which focuses the file, and on Android 13 is what makes the keys work), then the keys again;
# the picker of Android 10 and 11 takes the touch, then the keys. Each way is tried once, and only after the one before left the
# file in the list.
picker_pick() { # name
    local name=$1 end xy step=0 sig crash
    sys_wait "$SYS_DEADLINE" "the file picker being up" picker_up || return 1
    picker_trace "== pick $name: $(sys_show)"
    end=$(($(now_ms) + ${PICKER_DEADLINE:-150} * 1000))
    while :; do
        sys_anr_check || return 1
        if ! picker_up; then
            crash=$(picker_crash) && {
                SYS_WHY="the system's file picker crashed on its own ($crash): the app was given no file"
                return 1
            }
            picker_back_in_app
            return
        fi
        if [ "$(now_ms)" -ge "$end" ]; then
            SYS_WHY="the picker did not deliver $name within ${PICKER_DEADLINE:-150}s (step $step; shows: $(picker_summary); $(sys_show))"
            return 1
        fi
        sys_dump || { sleep 0.5; continue; }
        sig=$(sys_nodes list | cksum | cut -d' ' -f1)
        if xy=$(sys_nodes find "label~$name" pkg~documentsui); then
            local way
            if picker_is_google; then
                case $step in 0) way=keys ;; 1) way=touch ;; 2) way=keys ;; *) way=none ;; esac
            else
                case $step in 0) way=touch ;; 1) way=keys ;; *) way=none ;; esac
            fi
            step=$((step + 1))
            case $way in
                touch) picker_touch_file "$name" "$xy" && return 0 ;;
                keys) picker_keys_file "$name" && return 0 ;;
                *)
                    SYS_WHY="$name is listed but neither the touch nor the keys chose it (shows: $(picker_summary); $(sys_show))"
                    return 1
                    ;;
            esac
        elif xy=$(sys_nodes find label=Downloads pkg~documentsui); then
            picker_trace "touch on Downloads at $xy"
            # shellcheck disable=SC2086
            dev input tap $xy > /dev/null
            sys_wait 10 "the picker showing another listing after Downloads" picker_changed "$sig"
        elif xy=$(sys_nodes find label=Download pkg~documentsui); then
            picker_trace "touch on the Download folder at $xy"
            # shellcheck disable=SC2086
            dev input tap $xy > /dev/null
            sys_wait 10 "the picker showing another listing after the Download folder" picker_changed "$sig"
        elif xy=$(sys_nodes find desc="Show roots" pkg~documentsui); then
            picker_trace "touch on Show roots at $xy"
            # shellcheck disable=SC2086
            dev input tap $xy > /dev/null
            sys_wait 10 "the roots drawer after Show roots" picker_changed "$sig"
        else
            sleep 0.5
        fi
    done
}
picker_changed() { # signature before
    local now
    now=$(sys_sig) && [ "$now" != "$1" ]
}

# ---- What the SDK's unlock screen does (a plain view of the SDK, in the app's package) ----
# How many times the SDK logged that it quieted the ring for Android's unlock (the log is cleared at the start of a scenario).
quiet_count() { dev "logcat -d -s MobileSentinel.Service:I 2> /dev/null | grep -c 'quiet for the unlock'" | tail -1; }
unlock_asked() { [ "$(quiet_count)" -gt "${UNLOCK_QUIET_BEFORE:-0}" ] 2> /dev/null || ! unlock_screen_has_focus; }
quiet_or_silent() { [ "$(quiet_count)" -gt "${UNLOCK_QUIET_BEFORE:-0}" ] 2> /dev/null || sound_off; }
# The Unlock button of the unlock screen, touched once: its place from the tree. What shows that the touch took is the SDK's
# own line "quiet for the unlock" or the unlock screen having lost the keys to Android's unlock. When after 15 s neither has
# happened and the unlock screen is still in front, the touch is proven lost and the button is touched once more. Then the
# ring must be quiet (the log line, or no sound at a look).
sdk_unlock_press() {
    local round=1 wait=15
    sys_wait "$SYS_DEADLINE" "the unlock screen having the keys" unlock_screen_has_focus || return 1
    UNLOCK_QUIET_BEFORE=$(quiet_count)
    while :; do
        sys_wait "$SYS_DEADLINE" "the Unlock button in the tree of the unlock screen" unlock_button_xy || return 1
        # shellcheck disable=SC2086
        dev input tap $UNLOCK_XY > /dev/null
        sys_wait "$wait" "Android's unlock being asked for after the touch on Unlock" unlock_asked && break
        [ "$round" -lt 2 ] && unlock_screen_has_focus || return 1
        round=$((round + 1))
        wait=$SYS_DEADLINE
    done
    sys_wait 10 "the ring going quiet for the unlock (no 'quiet for the unlock' in the log, sound on)" quiet_or_silent
}
# Home from the unlock screen: what shows it left is the screen not being on top, or the system pausing it as the user
# leaving. The ring's own notification can bring the unlock screen back within milliseconds (Android 11 does, now and then:
# the full screen intent comes with the lock screen).
unlock_left_count() { dev "logcat -d -b events 2> /dev/null | grep -c 'wm_pause_activity.*SentinelUnlockActivity.*userLeaving=true'" | tail -1; }
unlock_left_since() { ! top_is_unlock || [ "$(unlock_left_count)" -gt "$1" ] 2> /dev/null; }
# Whether Home takes the unlock screen away depends on the Android version (Android 16 over a phone that is locked and has no
# Android unlock up: the key is taken and nothing moves, the log shows no pause of the screen in 40 s), so what the scenario
# gets is an answer: 0 when the screen was left, 1 when nothing moved within SECONDS (a NOTE says so, and it is no failure of
# the app: what the app has to do afterwards is checked by the scenario).
home_from_unlock() { # [seconds]
    local before secs=${1:-10}
    before=$(unlock_left_count)
    dev input keyevent KEYCODE_HOME > /dev/null
    sys_wait "$secs" "Home taking the screen from the unlock screen" unlock_left_since "$before" && return 0
    echo "NOTE  ${CURRENT:-?}  Home did not move the unlock screen within ${secs}s on this Android version ($SYS_WHY)" | tee -a "$RESULTS"
    return 1
}

# ---- Putting the lock screen away once the PIN is cleared (scenario ends, baseline) ----
# Android 10 to 12 keep the lock screen up after `locksettings clear` (the probe on msd29: `wm dismiss-keyguard`, the Menu key,
# `locksettings set-disabled true` and a swipe up all left it showing, `deviceLocked=0`). It still asks for the PIN that is gone,
# so the PIN pad is what has to be dealt with. Each step only when a read still shows the lock screen; LOCK_AWAY_LOG (in the
# results) says which step worked on which version.
lock_away_note() { printf '%s %s %s: %s
' "$(date +%T)" "${CURRENT:-?}" "$1" "$(sys_show)" >> "$OUT/lock_away.txt"; }
lock_away() {
    local step=${LOCK_AWAY_STEP:-8}
    [ "$(kg_state)" != true ] && return 0
    lock_away_note "start"
    screen_awake || sys_wake || return 1
    dev wm dismiss-keyguard > /dev/null
    sys_wait "$step" "the lock screen gone after wm dismiss-keyguard" kg_gone && { lock_away_note "gone by wm dismiss-keyguard"; return 0; }
    # Asleep and awake again, so that the lock screen is made anew (this took it away on Android 10 and 11); then asked once more.
    sys_sleep > /dev/null
    sys_wake > /dev/null
    dev wm dismiss-keyguard > /dev/null
    sys_wait "$step" "the lock screen gone after sleep, wake and wm dismiss-keyguard" kg_gone && { lock_away_note "gone by sleep and wake"; return 0; }
    dev input keyevent KEYCODE_MENU > /dev/null
    sys_wait "$step" "the lock screen gone after the Menu key" kg_gone && { lock_away_note "gone by Menu"; return 0; }
    local size w h
    size=$(dev wm size | sed -n 's/^Physical size: //p' | head -1)
    w=${size%x*}
    h=${size#*x}
    dev input swipe $((w / 2)) $((h * 4 / 5)) $((w / 2)) $((h / 5)) 300 > /dev/null
    sys_wait "$step" "the lock screen gone after a swipe up" kg_gone && { lock_away_note "gone by a swipe"; return 0; }
    # The lock screen may still ask for the old PIN: the pad is up (or comes up now), the old PIN takes it away.
    if sys_wait "$step" "the PIN pad after wm dismiss-keyguard" pad_ready && unlock_pin 0 && sys_wait "$step" "the lock screen gone after the old PIN" kg_gone; then
        lock_away_note "gone by the old PIN on the pad"
        return 0
    fi
    lock_away_note "STILL THERE: $SYS_WHY"
    sys_evidence lock_away_still_there
    return 1
}

# ---- The launcher (home screen, and the dialog that asks to add a widget) ----
launcher_up() { sys_focus | grep -q -i launcher; }
# Every text on the launcher's screen, one per line (the widget's text included).
launcher_texts() { sys_dump && sys_nodes list | cut -d'|' -f4 | grep -v '^$'; }
launcher_has() { launcher_texts | grep -q -F -- "$1"; }
# An instance of the widget is on the launcher (the provider is listed by `dumpsys appwidget` always; a placed widget is a
# `provider=...` line under its host, the same lines remove_widgets reads).
widget_registered() { dev dumpsys appwidget | grep -q "provider=.*$PKG/.*SentinelHomeWidget"; }
# The button of the launcher's question "add this widget?" (what it is called differs per launcher and per version: "Add to Home
# screen" on Android 12 to 16 with a capital H, "Add to home screen" on 17, where the dump has the button as a TextView that is not
# clickable: the match is by the label, and `find` takes a clickable node first when there is one).
the_add_button() {
    sys_dump || return 1
    ADD_XY=$(sys_nodes find 'label^^(add|add to home screen|add automatically)$')
}
add_question_gone() { sys_dump && ! sys_nodes find 'label^^(add|add to home screen|add automatically)$' > /dev/null; }
widget_placed() { widget_registered && add_question_gone; }
# The widget added once the question is up: the button touched once, and the widget placed (an instance under the launcher in
# `dumpsys appwidget`, and the question gone). When after 15 s a read still shows the question, the touch is proven lost and the
# button is touched once more.
launcher_add_widget() {
    sys_wait "$SYS_DEADLINE" "the launcher asking to add the widget" the_add_button || return 1
    # shellcheck disable=SC2086
    dev input tap $ADD_XY > /dev/null
    sys_wait 15 "the widget being placed" widget_placed && return 0
    the_add_button || { sys_wait "$SYS_DEADLINE" "the widget being placed" widget_placed; return; }
    # shellcheck disable=SC2086
    dev input tap $ADD_XY > /dev/null
    sys_wait "$SYS_DEADLINE" "the widget being placed (second touch, the first did nothing)" widget_placed
}

# ---- Time, and a banner over the ring screen ----
elapsed_since() { [ $(($(now_ms) - $1)) -ge "$2" ]; }
# A tree that was really read: a dump that failed is asked for again (up to 8 times) and never counts as "nothing there".
sys_tree_read() { # [--windows]
    local n=0
    until sys_dump "$@"; do
        n=$((n + 1))
        [ "$n" -lt 8 ] || return 1
        sleep 0.5
    done
}
# 0: the banner of the app's own notification ("Tap to open") is on screen, 1: it is not, 2: no tree could be read.
heads_up_banner_there() {
    sys_tree_read --windows || sys_tree_read || return 2
    [ "$(sys_nodes count text="Tap to open")" -gt 0 ]
}

# ---- The SDK's backup screen (a plain view): its challenge is one long press (the scenario sets `backup_holds` to 1) ----
backup_screen_has_focus() { focused_window | grep -q SentinelBackupActivity; }
backup_press_once() {
    local size w h
    size=$(dev wm size | tail -1 | sed 's/.*: //')
    w=${size%x*}
    h=${size#*x}
    dev input swipe $((w / 2)) $((h * 2 / 3)) $((w / 2)) $((h * 2 / 3)) 3300 > /dev/null
}
# The hold once, with the screen's keys read first, and the state it must bring (`check...`) awaited. A second hold only when a
# read after 10 s still shows the backup screen in front and the sound on: the first hold is then proven to have counted for nothing.
backup_hold() { # check...
    sys_wait "$SYS_DEADLINE" "the backup screen having the keys" backup_screen_has_focus || return 1
    backup_press_once
    sys_wait 10 "the state after the hold" "$@" && return 0
    backup_screen_has_focus && sound_on || return 1
    backup_press_once
    sys_wait "$SYS_DEADLINE" "the state after the second hold (the first counted for nothing)" "$@"
}
