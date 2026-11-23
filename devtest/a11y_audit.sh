#!/bin/bash
# Walks the main screens and lists what an accessibility service cannot name (a11y_audit.py).
# A11Y=1 SERIAL=emulator-5558 bash devtest/matrix.sh a11y_audit    (a debug build; needs the tree)
audit_screen() { # name
    a11y_dump
    printf '%s' "$A11Y_XML" | python -I devtest/a11y_audit.py "$1"
    a11y_ask texts | tr '\n' '|' | cut -c1-300 | sed "s/^/  [$1] texts: /"
}
# A fresh start of the window, so that a screen does not depend on how the one before was left.
audit_restart() {
    dev am force-stop "$PKG"
    start_app
    on_home 20 || echo "no home"
}
audit_rows() { # screen rows...
    local screen=$1 row
    shift
    for row in "$@"; do
        audit_restart
        case "$screen" in
            settings) go_tab Settings ;;
            editor) tap_has "$(row_time), " L 0.8 ;;
        esac
        scroll_to "$row" > /dev/null 2>&1
        if tap_has "$row" 0 0.8; then
            audit_screen "${screen}_$row"
        else
            echo "${screen}_$row: row not found"
        fi
    done
}
a11y_audit() {
    scenario a11y_audit
    fresh
    seed a "$(alarm_json a 7 0 "$EVERY_DAY" null)"
    arm a
    on_home 10
    audit_screen home
    tap_has "More actions" 0 0.6
    audit_screen home_more
    audit_restart
    go_tab History
    audit_screen history
    audit_restart
    go_tab Settings
    audit_screen settings
    audit_restart
    tap_has "$(row_time), " L 0.8
    on_editor
    audit_screen editor
    tap_time 0.8
    audit_screen time_sheet
    audit_rows editor Label Sound Vibration Snooze Challenges "Alarm screen" "Ring limit" "Are you up?"
    audit_rows settings Theme Language Reliability
    pass "audit done"
}

# Every screen's labels as the framework sees them: a uiautomator dump per screen, kept in $OUT/labels and checked
# by label_check.py (root named AlarmFree, no unnamed or duplicate interactive nodes). Findings are printed, not failed.
label_dump_screen() { # name
    mkdir -p "$OUT/labels"
    # The tree of a window is built when something asks for it: the first dump can be empty.
    local i
    for i in 1 2 3 4 5; do
        dev uiautomator dump /sdcard/ui.xml > /dev/null
        adb_ exec-out cat /sdcard/ui.xml > "$OUT/labels/$1.xml"
        # The dump is one line: grep -c counts matching lines (0 or 1), not occurrences. Count them with -o.
        [ "$(grep -o 'com.fidderr.alarmfree' "$OUT/labels/$1.xml" | wc -l)" -gt 6 ] && break
        sleep 1
    done
    python -I devtest/label_check.py "$1" < "$OUT/labels/$1.xml"
}
label_dump() {
    scenario label_dump
    fresh
    seed a "$(alarm_json a 7 0 "$EVERY_DAY" null)"
    arm a
    on_home 10
    label_dump_screen home
    audit_restart
    go_tab History
    label_dump_screen history
    press h-delete-history "" 0.5
    wait_for 3 panel_up
    label_dump_screen history_delete_sheet
    press_back
    audit_restart
    go_tab Settings
    label_dump_screen settings
    tap_has Theme 0 0.8
    sleep 0.5
    label_dump_screen theme
    tap_row "Your own theme"
    sleep 0.8
    tap_row Text
    sleep 0.8
    label_dump_screen colour_swatches
    press_back
    audit_restart
    tap_has "$(row_time), " L 0.8
    on_editor
    label_dump_screen editor
    scroll_to Weekdays > /dev/null 2>&1
    label_dump_screen editor_days
    scroll_to Sound > /dev/null 2>&1
    tap_has Sound 0 0.8
    label_dump_screen sound_sheet
    pass "label dumps in $OUT/labels"
}

# What a screen reader gets on every screen, in every full run: the tree of the window (`uiautomator dump`, what TalkBack
# reads) checked by label_check.py. A control without a name, or a window named app_process64, fails the scenario;
# two controls with one name are a NOTE (a list can mean that on purpose).
names_here() { # screen
    local f="$OUT/labels/$1.xml" out line
    mkdir -p "$OUT/labels"
    # The tree of a window is built when something asks for it: the first dump can be empty.
    names_dumped() {
        dev uiautomator dump /sdcard/ui.xml > /dev/null 2>&1 # lint-ok: the tree a screen reader gets is the subject of this scenario
        adb_ exec-out cat /sdcard/ui.xml > "$f" 2> /dev/null
        # The dump is one line: grep -c counts matching lines (0 or 1), not occurrences. Count them with -o.
        [ "$(grep -o 'com.fidderr.alarmfree' "$f" | wc -l)" -gt 6 ]
    }
    if ! wait_for 15 names_dumped; then
        NAMES_PROBLEMS="$NAMES_PROBLEMS; $1: no tree of the window"
        return 0
    fi
    NAMES_SCREENS=$((NAMES_SCREENS + 1))
    out=$(python -I devtest/label_check.py "$1" < "$f")
    while IFS= read -r line; do
        case "$line" in
            *": UNNAMED "* | *": ROOT "*) NAMES_PROBLEMS="$NAMES_PROBLEMS; $line" ;;
            *": DUPLICATE "*) note "$line" ;;
        esac
    done <<< "$out"
}
# The editor of an alarm, from a fresh start of the window.
names_editor() { # alarm
    audit_restart
    press open "$1" 0.8
    on_editor 10 || NAMES_PROBLEMS="$NAMES_PROBLEMS; no editor for $1"
}
s70() {
    scenario s70_every_control_has_a_name
    fresh
    seed a "$(alarm_json a 7 0 "$EVERY_DAY" null air 0 0 3 1 '[["challenge.typing","{\"difficulty\":\"easy\"}"]]')"
    seed b "$(alarm_json b 8 0 "$EVERY_DAY" null air 0 0 3 1 '[["challenge.scan","{}"]]')"
    arm a
    NAMES_PROBLEMS="" NAMES_SCREENS=0
    local row k y
    audit_restart
    names_here home
    tap "More actions" 0 0.5
    on_screen "Try its challenges" 4 || NAMES_PROBLEMS="$NAMES_PROBLEMS; no menu on Home"
    names_here home_menu
    audit_restart
    open_reliability || NAMES_PROBLEMS="$NAMES_PROBLEMS; no reliability screen"
    names_here reliability
    # History: the list, the delete sheet, and the field for the days of one's own period.
    audit_restart
    go_tab History || NAMES_PROBLEMS="$NAMES_PROBLEMS; no History"
    names_here history
    press h-delete-history "" 0.5
    wait_for 3 panel_up || NAMES_PROBLEMS="$NAMES_PROBLEMS; no delete sheet"
    names_here history_delete_sheet
    press_back
    wait_for 3 panel_down
    k=0
    while ! has_there "Your own days" && [ "$k" -lt 3 ]; do
        y=$(where "This week" | cut -d' ' -f2)
        case "$y" in '' | *[!0-9]*) y=341 ;; esac
        finger_swipe 900 "$y" 100 "$y" 300
        wait_for 2 has_there "Your own days"
        k=$((k + 1))
    done
    tap "Your own days" 0 0.3
    wait_for 5 has_there days || NAMES_PROBLEMS="$NAMES_PROBLEMS; no field for the days of one's own period"
    names_here history_own_days
    # Settings and what is behind them.
    audit_restart
    go_tab Settings || NAMES_PROBLEMS="$NAMES_PROBLEMS; no Settings"
    names_here settings
    tap_has Theme 0 0.8
    wait_for 5 on_screen_no 5 || NAMES_PROBLEMS="$NAMES_PROBLEMS; no theme screen"
    names_here theme
    tap_row "Your own theme"
    tap_row Text
    names_here colour_swatches
    audit_restart
    go_tab Settings
    tap_has Language 0 0.8
    names_here language
    # The editor, each of its rows, the time, the look of the ring screen.
    names_editor a
    names_here editor
    scroll_to Weekdays > /dev/null 2>&1
    names_here editor_days
    tap_time 0.8
    names_here time_sheet
    for row in Label Sound Vibration Snooze "Ring limit" "Are you up?" "Alarm screen" Challenges; do
        names_editor a
        open_row "$row"
        hide_keyboard
        names_here "editor_$(echo "$row" | tr -c 'a-zA-Z0-9\n' _)"
    done
    # The ring screen as it will be.
    names_editor a
    open_row "Alarm screen"
    tap Preview L 0.5
    wait_for 3 preview_up || NAMES_PROBLEMS="$NAMES_PROBLEMS; no preview of the ring screen"
    names_here ring_screen_preview
    # A challenge as it is done: the typing box.
    names_editor a
    open_row Challenges
    tap "Try all" 0 0.4
    wait_for 5 run_up || NAMES_PROBLEMS="$NAMES_PROBLEMS; the typing challenge cannot be tried"
    names_here typing_challenge
    # The settings of a challenge: the code that a scan has to read.
    names_editor b
    open_row Challenges
    tap_has Scan 0 0.6
    wait_for 5 has_there "Which code" || NAMES_PROBLEMS="$NAMES_PROBLEMS; no settings of the scan challenge"
    names_here scan_challenge_settings
    dev am force-stop "$PKG"
    if [ -z "$NAMES_PROBLEMS" ]; then
        pass "$NAMES_SCREENS screens read as a screen reader gets them: every control has a name"
    else
        fail "${NAMES_PROBLEMS#; }"
    fi
}
