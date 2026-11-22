#!/bin/bash
# Android's own screens (the lock screen and its PIN pad, the shade, a file picker, the launcher, Settings): what the
# suite can know about them, and how it acts there. Sourced by lib.sh.
#
# These screens are not the app's window, so the app cannot say what is on them. The device can: the focused window, the
# top activity, the user and keyguard state, and the tree of `uiautomator dump`. Every step on such a screen follows one
# rule (plan alarmfree-devtest-no-flake, rule 6):
#   1. await a state, read from the device, that says the screen is ready for the action (a read can be repeated: it changes
#      nothing),
#   2. act once,
#   3. confirm with a state read that the action took,
#   4. act again only when a read proves that the first action did nothing (never blind),
#   5. every wait is a poll of a named state with a deadline (SYS_DEADLINE, 30 s) and, when it passes, a failure that says which
#      state did not come and what the device showed (SYS_WHY).
#
# Scenarios read as: `sys_wait 30 "the PIN pad is up" pin_pad_up || problems=...; act; sys_wait 30 "the phone is unlocked" ...`.

SYS_DEADLINE="${SYS_DEADLINE:-30}"
SYS_XML=""
SYS_WHY=""
_sys_here=${BASH_SOURCE[0]%/*}
[ "$_sys_here" != "${BASH_SOURCE[0]}" ] || _sys_here=.
SYS_PY="$_sys_here/sys_nodes.py"

# Milliseconds on this computer's clock (bash 5).
now_ms() {
    local t=${EPOCHREALTIME/[.,]/}
    echo $((t / 1000))
}

# One line that says what the device shows, for the message of a wait that ran out.
sys_show() {
    local f
    f=$(focused_window | sed -E 's/.*mCurrentFocus=//; s/^Window\{[0-9a-f]+ u0 //; s/\}$//')
    echo "focus=${f:-none} top=$(top_activity) keyguard=$(kg_state) user=$(user_unlocked && echo unlocked || echo locked) screen=$(wakefulness)"
}

# Poll a named state: sys_wait SECONDS "what" command...  Returns 1 and sets SYS_WHY when the deadline passes. The check is
# run at least once, also with a deadline of 0.
sys_wait() {
    local secs=$1 what=$2 end
    shift 2
    end=$(($(now_ms) + secs * 1000))
    until "$@"; do
        if [ "$(now_ms)" -ge "$end" ]; then
            SYS_WHY="$what did not come within ${secs}s ($(sys_show))"
            return 1
        fi
        sleep 0.2
    done
    SYS_WHY=""
}

# ---- Reads ----
# The lock screen: true, false, or ? when this Android version prints neither of its two places for it. Android 10 has no
# isKeyguardShowing in the displays dump; its policy dump says `showing=`.
kg_state() {
    local w
    w=$(dev "dumpsys window displays | grep -m1 -o 'isKeyguardShowing=[a-z]*'")
    w=${w#*=}
    if [ -z "$w" ]; then
        w=$(dev "dumpsys window policy | grep -m1 -o '[ ]showing=[a-z]*'")
        w=${w#*=}
    fi
    echo "${w:-?}"
}
kg_showing() { [ "$(kg_state)" = true ]; }
kg_gone() { [ "$(kg_state)" = false ]; }
# The device's own place for the credential: the phone is locked with a PIN or not.
screen_awake() { [ "$(wakefulness)" = Awake ]; }
screen_asleep() { [ "$(wakefulness)" != Awake ]; }

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
        echo "-- windows (focus lines)"
        dev "dumpsys window displays | grep -E 'mCurrentFocus|mFocusedApp|isKeyguardShowing'" | head -5
        echo "-- tree"
        if sys_dump; then sys_nodes list | head -70; else echo "(no dump)"; fi
    } > "$OUT/${name}${VARIANT:-}.screen.txt" 2>&1
}
