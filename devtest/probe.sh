#!/bin/bash
# Probes of Android's own screens on each version: they change nothing in the suite, they write what the device says into
# $OUT/probe_*.txt, by name only (SCENARIOS="t05_probe_lock t06_probe_shade t07_probe_picker"). Sourced by matrix.sh.

PROBE=""
pr() { printf '%s\n' "$*" >> "$PROBE"; }
pr_t() { pr "[$(date +%T.%N | cut -c1-12)] $*"; }
pr_head() { # label
    pr ""
    pr_t "=== $1"
}
pr_state() {
    pr "  focus : $(focused_window)"
    pr "  top   : $(top_activity)"
    pr "  kg    : $(kg_state)   unlocked: $(user_unlocked && echo yes || echo no)   wake: $(wakefulness)   $(dev "dumpsys trust | grep -m1 -o 'deviceLocked=[0-9a-z]*'")"
}
pr_policy() {
    pr "  policy: $(dev "dumpsys window policy | grep -i -E 'showing|occluded|bouncer' | head -8" | tr '\n' ' ' | tr -s ' ')"
}
pr_nodes() { # label [--windows]
    local n=0
    pr "  tree ($1):"
    while [ "$n" -lt 4 ]; do
        if sys_dump "${2:-}"; then
            sys_nodes list | head -90 | sed 's/^/    /' >> "$PROBE"
            return 0
        fi
        n=$((n + 1))
        pr "    (dump $n gave no tree: $(printf '%.100s' "$SYS_XML"))"
        sleep 1
    done
}
# One line per half second, for a number of rounds.
pr_timeline() { # rounds label
    local i=0
    while [ "$i" -lt "$1" ]; do
        pr_t "$2 +$i: focus=$(focused_window | sed -E 's/.*mCurrentFocus=//') kg=$(kg_state) unlocked=$(user_unlocked && echo y || echo n) wake=$(wakefulness)"
        sleep 0.5
        i=$((i + 1))
    done
}
pr_pin() { # label: what the PIN pad holds and says
    local t
    sys_dump || { pr_t "$1: no dump"; return; }
    t=$(sys_nodes list | grep -i -E 'pin|password|entry|wrong|incorrect|attempt' | tr '\n' ';')
    pr_t "$1: ${t:-nothing like a PIN field}"
}

t05_probe_lock() {
    scenario t05_probe_lock
    on_emulator || { skip "probe: emulator only"; return; }
    PROBE=$OUT/probe_lock.txt
    : > "$PROBE"
    fresh
    pr "sdk $(dev getprop ro.build.version.sdk) release $(dev getprop ro.build.version.release) model $(dev getprop ro.product.model)"
    dev locksettings set-pin 1234 > /dev/null
    pr_head "asleep"
    dev input keyevent KEYCODE_SLEEP
    pr_timeline 6 asleep
    pr_head "wake"
    dev input keyevent KEYCODE_WAKEUP
    pr_timeline 8 wake
    pr_state
    pr_policy
    pr_nodes "lock screen, plain"
    pr_nodes "lock screen, windows" --windows
    pr_head "wm dismiss-keyguard"
    dev wm dismiss-keyguard
    pr_timeline 12 dismiss
    pr_state
    pr_policy
    pr_nodes "bouncer, plain"
    pr_nodes "bouncer, windows" --windows
    pr_head "bouncer systemui lines"
    pr "$(dev "dumpsys activity service com.android.systemui 2>/dev/null | grep -i -E 'bouncer' | head -12")"
    pr_head "digits one by one (keyevents)"
    pr_pin "before the first digit"
    local d
    for d in 1 2 3 4; do
        dev input keyevent KEYCODE_$d
        sleep 0.6
        pr_pin "after $d"
    done
    dev input keyevent 66
    pr_timeline 10 enter
    pr_state
    pr_head "second round: input text, with a wrong PIN first"
    dev input keyevent KEYCODE_SLEEP
    pr_timeline 4 asleep2
    dev input keyevent KEYCODE_WAKEUP
    pr_timeline 4 wake2
    dev wm dismiss-keyguard
    pr_timeline 6 dismiss2
    dev input text 1239
    dev input keyevent 66
    pr_timeline 4 wrong
    pr_pin "after a wrong PIN"
    pr_nodes "after a wrong PIN"
    dev input text 1234
    pr_pin "after text 1234"
    dev input keyevent 66
    pr_timeline 10 enter2
    pr_state
    pr_head "third round: locksettings verify"
    dev input keyevent KEYCODE_SLEEP
    pr_timeline 4 asleep3
    dev input keyevent KEYCODE_WAKEUP
    pr_timeline 4 wake3
    pr "verify said: $(dev locksettings verify --old 1234 2>&1 | head -3 | tr '\n' ' ')"
    pr_timeline 6 verify
    pr_state
    pr_nodes "after verify"
    pr_head "cmd statusbar collapse on the bouncer"
    dev wm dismiss-keyguard
    pr_timeline 4 dismiss3
    pr "collapse said: $(dev cmd statusbar collapse 2>&1 | head -2 | tr '\n' ' ')"
    pr_timeline 4 collapse
    pr_state
    pr_head "put the PIN away: clear, then lock_away (system.sh), with what the screen shows before"
    dev input keyevent KEYCODE_SLEEP
    pr_timeline 3 asleep4
    dev input keyevent KEYCODE_WAKEUP
    pr_timeline 3 wake4
    pr "clear said: $(dev locksettings clear --old 1234 2>&1 | head -2 | tr '
' ' ')"
    pr_timeline 3 cleared
    pr_state
    dev wm dismiss-keyguard
    pr_timeline 3 dismiss5
    pr_nodes "after the clear and a dismiss"
    if lock_away; then pr "lock_away: gone"; else pr "lock_away: STILL THERE: $SYS_WHY"; fi
    pr_state
    cat "$OUT/lock_away.txt" >> "$PROBE" 2> /dev/null
    pass "probed the lock screen: see probe_lock.txt"
}

t06_probe_shade() {
    scenario t06_probe_shade
    on_emulator || { skip "probe: emulator only"; return; }
    PROBE=$OUT/probe_shade.txt
    : > "$PROBE"
    fresh
    pr "sdk $(dev getprop ro.build.version.sdk)"
    dev cmd notification post -t "Probe title" probetag "Probe text" > /dev/null 2>&1
    sleep 1
    pr_head "posted a notification: $(dev "dumpsys notification | grep -c probetag") lines"
    pr_state
    pr_head "expand-notifications"
    pr "said: $(dev cmd statusbar expand-notifications 2>&1 | head -2 | tr '\n' ' ')"
    pr_timeline 10 expand
    pr_state
    pr "  systemui: $(dev "dumpsys activity service com.android.systemui 2>/dev/null | grep -i -E 'panelExpanded|mExpandedVisible|expandedFraction|panel_expanded|isExpanded' | head -8" | tr '\n' ' ' | tr -s ' ')"
    pr "  statusbar: $(dev "dumpsys statusbar 2>/dev/null | grep -i -E 'expanded|panel|disable' | head -8" | tr '\n' ' ' | tr -s ' ')"
    pr_nodes "shade, plain"
    pr_nodes "shade, windows" --windows
    pr_head "collapse"
    pr "said: $(dev cmd statusbar collapse 2>&1 | head -2 | tr '\n' ' ')"
    pr_timeline 10 collapse
    pr_state
    pr "  systemui: $(dev "dumpsys activity service com.android.systemui 2>/dev/null | grep -i -E 'panelExpanded|mExpandedVisible|expandedFraction|panel_expanded|isExpanded' | head -8" | tr '\n' ' ' | tr -s ' ')"
    pr_head "heads-up lines of systemui at rest"
    pr "$(dev "dumpsys activity service com.android.systemui 2>/dev/null | grep -i -E 'headsup|heads-up|HeadsUp' | head -14")"
    pr_head "expand twice in a row"
    dev cmd statusbar expand-notifications > /dev/null 2>&1
    dev cmd statusbar expand-notifications > /dev/null 2>&1
    pr_timeline 6 twice
    dev cmd statusbar collapse > /dev/null 2>&1
    dev cmd statusbar collapse > /dev/null 2>&1
    pr_timeline 6 collapse2
    pr_state
    dev cmd notification cancel probetag > /dev/null 2>&1
    pass "probed the shade: see probe_shade.txt"
}

t07_probe_picker() {
    scenario t07_probe_picker
    on_emulator || { skip "probe: emulator only"; return; }
    PROBE=$OUT/probe_picker.txt
    : > "$PROBE"
    fresh
    pr "sdk $(dev getprop ro.build.version.sdk)"
    dev rm -f /sdcard/Download/mytune.ogg
    rm_download 'alarmfree-*'
    adb_ push assets/sounds/default/air.ogg /sdcard/Download/mytune.ogg > /dev/null
    dev content call --uri content://media --method scan_volume --arg external_primary > /dev/null 2>&1
    seed a "$(alarm_json a 7 0 "$EVERY_DAY" null)"
    arm a
    on_card 10
    press open a 0.8
    on_editor
    tap_has Sound 0 0.6
    scroll_until "Add your own sound"
    on_screen "Add your own sound" 4
    pr_head "tap Add your own sound"
    tap_in "Add your own sound" 0 1
    pr_timeline 8 opening
    pr_state
    pr_nodes "picker, first look"
    pr_nodes "picker, windows" --windows
    pr_head "tap the file by position"
    local xy
    xy=$(sys_where text=mytune.ogg) || xy=$(sys_where desc~mytune)
    pr "file at: ${xy:-not found}"
    if [ -n "$xy" ] && [ "$xy" != none ]; then
        # shellcheck disable=SC2086
        dev input tap $xy
        pr_timeline 12 tapped
        pr_state
        pr_nodes "after the tap"
    fi
    dev am force-stop "$PKG"
    pr_head "after force-stop: $(own_sound=$(as_app 'ls files/sounds/custom' 2>/dev/null); echo "custom: $own_sound")"
    pr_state
    start_app
    on_home 10
    pr_head "Save to a file"
    press go 3
    await_ui 30 "screen=3"
    tap_scrolling "Back up"
    on_screen "Save to a file" 6
    tap_in "Save to a file" 0 1
    local i=0
    while [ "$i" -lt 40 ]; do
        pr_t "save +$i: files=$(dev ls /sdcard/Download/ | tr '\n' ' ') screen=$(screen_no) focus=$(focused_window | sed -E 's/.*mCurrentFocus=//')"
        i=$((i + 1))
        sleep 0.5
    done
    pr_nodes "after Save to a file"
    rm_download 'alarmfree-*'
    dev rm -f /sdcard/Download/mytune.ogg
    pass "probed the picker: see probe_picker.txt"
}
