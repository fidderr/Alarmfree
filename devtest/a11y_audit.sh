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
            editor) tap_has "$(row_time)" L 0.8 ;;
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
    tap_has "$(row_time)" L 0.8
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
        [ "$(grep -c 'com.fidderr.alarmfree' "$OUT/labels/$1.xml")" -gt 6 ] && break
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
    tap_has "$(row_time)" L 0.8
    on_editor
    label_dump_screen editor
    scroll_to Weekdays > /dev/null 2>&1
    label_dump_screen editor_days
    scroll_to Sound > /dev/null 2>&1
    tap_has Sound 0 0.8
    label_dump_screen sound_sheet
    pass "label dumps in $OUT/labels"
}
