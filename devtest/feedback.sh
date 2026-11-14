#!/bin/bash
# The scenarios of the feedback round: the intro, Report a problem, the Rate row, Help and the
# banner after trouble. Started like one.sh, on one emulator:
#
#   NO_BOOT=1 KEEP=1 bash devtest/feedback.sh <avd> <port> [s70 s71 ...]    (results in devtest-out/fb_<avd>/)
#   LOOKS=1 ... feedback.sh <avd> <port> looks    pictures of every new screen: light, dark, font 1.3
#
# Debug build only (the window is driven through the `ui:` commands).

cd "$(dirname "$0")/.." || exit 1
export APK="${APK:-build/alarmfree-slint-debug.apk}"
avd=$1 port=$2
shift 2
export AVD=$avd SERIAL=emulator-$port OUT="devtest-out/fb_$avd"
. devtest/lib.sh
claim_device
if [ -z "${NO_BOOT:-}" ]; then
    reboot_and_wait || { echo "FAIL  $avd did not start"; exit 1; }
fi
said=$(adb_ install -r -g "$APK" 2>&1)
case "$said" in *Success*) ;; *) echo "STOP install: $said"; exit 1 ;; esac
: > "$RESULTS"
open_shells
dev settings put global auto_time 0
dev settings put system font_scale 1.0
dev locksettings set-disabled false > /dev/null
dev dumpsys deviceidle whitelist +"$PKG" > /dev/null
fresh

sz=$(adb_ shell wm size | tr -d '
' | grep -oE '[0-9]+x[0-9]+' | tail -1)
SW=${sz%x*}
SH=${sz#*x}
# Drag the screen up by about half its height.
scroll_up() { dev input swipe $((SW / 2)) $((SH * 70 / 100)) $((SW / 2)) $((SH * 30 / 100)) 300; }
on_screen_is() { wait_for "${2:-6}" on_screen_no "$1"; }
open_settings() {
    start_app
    wait_for 40 page_is_up
    on_screen_is 0 20 || return 1
    go_tab Settings
    on_screen_is 3
}
# A row of the settings that is below the first screenful.
settings_row() { # text
    local i=0
    while [ "$i" -lt 6 ]; do
        is_there "$1" && break
        scroll_up
        sleep 0.5
        i=$((i + 1))
    done
    tap "$1" 0 0.5
}
settings_defaults() { use_settings "${1:-\"Dark\"}" '"English"' "${2:-\"setup_done\":true,\"language_follows_phone\":false}"; }

s70() {
    scenario s70_intro
    fresh
    use_settings '"Dark"' '"English"' '"setup_done":false,"language_follows_phone":false'
    local problems=""
    start_app
    wait_for 40 page_is_up
    on_screen_is 6 20 || problems="$problems; no intro on a first start (screen $(screen_no))"
    is_there "Skip" || problems="$problems; no Skip"
    screenshot s70_intro_1
    tap "Next" 0 0.4
    is_there "Wake up properly" || problems="$problems; no second card: $(texts_of | cut -c1-200)"
    tap "Next" 0 0.4
    is_there "Get started" || problems="$problems; no last card"
    screenshot s70_intro_3
    tap "Get started" 0 0.5
    on_screen_is 4 || problems="$problems; the setup does not follow the intro (screen $(screen_no))"
    is_there "Welcome" || problems="$problems; no welcome on the setup"
    # Skip on the first card goes to the setup as well.
    dev am force-stop "$PKG"
    start_app
    wait_for 40 page_is_up
    on_screen_is 6 20 || problems="$problems; the intro is not back after the setup was left open"
    tap "Skip" 0 0.5
    on_screen_is 4 || problems="$problems; Skip did not reach the setup"
    # Done with the setup: the intro does not come again by itself, but Settings has it.
    tap "Get started" 0 0.5
    on_screen_is 0 || problems="$problems; setup did not end on the alarms (screen $(screen_no))"
    dev am force-stop "$PKG"
    start_app
    wait_for 40 page_is_up
    on_screen_is 0 20 || problems="$problems; the intro came again by itself (screen $(screen_no))"
    go_tab Settings
    settings_row "Show intro again"
    on_screen_is 6 || problems="$problems; Show intro again did not open it"
    tap "Skip" 0 0.5
    on_screen_is 3 || problems="$problems; Skip from the settings did not go back (screen $(screen_no))"
    if [ -z "$problems" ]; then
        pass "3 cards, Skip, setup follows, once, and again from the settings"
    else
        fail "${problems#; }"
    fi
}

s71() {
    scenario s71_report_a_problem
    fresh
    next_minute
    seed a "$(alarm_json a 7 0 "$EVERY_DAY" null)"
    printf "UPDATE alarms SET data = replace(data, '\"label\":\"a\"', '\"label\":\"Secret errand\"') WHERE id='a';
" | sql
    as_app "mkdir -p files/sentinel && printf '2026-11-14T07:00:01.000+01:00 ring started id=aaaaaaaa 1s after its time challenges=0\n2026-11-14T07:00:09.000+01:00 ring stopped id=aaaaaaaa how=solved challenges_solved=0\nleak Secret errand\n' > files/sentinel/diag.log"
    dev rm -f /sdcard/Download/alarmfree-report-*.txt /sdcard/Download/alarmfree-setup-*.json
    local problems=""
    open_settings || problems="$problems; no settings"
    settings_row "Report a problem or idea"
    on_screen_is 7 || problems="$problems; no report screen (screen $(screen_no))"
    is_there "What happened?" || problems="$problems; no first question"
    fields | grep -q "2026-11-14 07:00" || problems="$problems; the last alarm is not filled in: $(fields | cut -c1-300)"
    set_field 0 "It did not ring this morning" || problems="$problems; no field"
    screenshot s71_form
    tap "Idea" 0 0.4
    is_there "What would you like?" || problems="$problems; Idea does not change the question"
    tap "Problem" 0 0.4
    scroll_up
    tap "Attach my alarm setup" 0 0.3
    tap "Show what is sent" 0 0.5
    is_there "Back to the form" || problems="$problems; no preview"
    screenshot s71_preview
    tap "Back to the form" 0 0.4
    tap "Send with my email app" 0 1.5
    scroll_up
    # No email app on this image: the report goes to Downloads, and the screen says so.
    if has_there "No email app found"; then
        local saved
        saved=$(dev ls /sdcard/Download | grep -c "alarmfree-report-")
        [ "$saved" -ge 1 ] || problems="$problems; the report is not in Downloads"
        dev cat /sdcard/Download/alarmfree-report-*.txt > "$OUT/s71_report.txt"
        grep -q "It did not ring this morning" "$OUT/s71_report.txt" || problems="$problems; saved report lacks the answer"
        grep -q "ring started" "$OUT/s71_report.txt" || problems="$problems; saved report lacks the log"
        grep -q "Secret errand" "$OUT/s71_report.txt" && problems="$problems; saved report has a label"
        grep -q "Time zone:" "$OUT/s71_report.txt" || problems="$problems; saved report has no time zone"
        dev ls /sdcard/Download | grep -q "alarmfree-setup-" || problems="$problems; the alarm setup is not in Downloads"
        screenshot s71_saved
    else
        # An email app took it: the screen is left.
        on_screen_is 3 || problems="$problems; neither saved nor sent: $(texts_of | cut -c1-300)"
    fi
    if [ -z "$problems" ]; then
        pass "form, preview, no label in the report, Downloads when there is no email app"
    else
        fail "${problems#; }"
    fi
}

s72() {
    scenario s72_rate_row
    fresh
    local problems=""
    open_settings || problems="$problems; no settings"
    dev logcat -c
    settings_row "Rate AlarmFree"
    sleep 1
    # A store app opens, or the toast says it could not.
    if is_there "The Play Store could not be opened." || has_there "Play Store could not"; then
        note "no Play Store on this image: the toast says so"
    else
        case "$(focused_window)" in
            *fidderr*) problems="$problems; nothing opened and no message" ;;
            *) note "store app: $(focused_window | grep -oE '[a-z.]+/[A-Za-z.]+' | head -1)" ;;
        esac
    fi
    screenshot s72_rate
    if [ -z "$problems" ]; then
        pass "the row opens the store or says it cannot"
    else
        fail "${problems#; }"
    fi
}

s73() {
    scenario s73_help_page
    fresh
    local problems=""
    open_settings || problems="$problems; no settings"
    settings_row "Help"
    on_screen_is 8 || problems="$problems; no help page (screen $(screen_no))"
    is_there "Why did my alarm not ring?" || problems="$problems; no first question: $(texts_of | cut -c1-200)"
    tap "Why did my alarm not ring?" 0 0.5
    has_there "Xiaomi" || problems="$problems; the answer does not open"
    screenshot s73_help_open
    tap "How do challenges work?" 0 0.5
    scroll_up
    has_there "Memorize and reproduce" || problems="$problems; challenges answer lacks the examples"
    scroll_up
    tap "I cannot solve a challenge" 0 0.5
    scroll_up
    has_there "hold the button 20 times" || problems="$problems; backup answer lacks the number"
    scroll_up
    tap "Reliability" 0 0.6
    on_screen_is 4 || problems="$problems; the Reliability button does not open it (screen $(screen_no))"
    dev input keyevent KEYCODE_BACK
    on_screen_is 8 || problems="$problems; back from Reliability does not return to the help (screen $(screen_no))"
    dev input keyevent KEYCODE_BACK
    on_screen_is 3 || problems="$problems; back from help does not return to settings (screen $(screen_no))"
    if [ -z "$problems" ]; then
        pass "help opens, answers expand, links work, back works"
    else
        fail "${problems#; }"
    fi
}

s74() {
    scenario s74_banner_after_trouble
    fresh
    as_app "mkdir -p files/sentinel && printf '2026-11-14T07:00:01.000+01:00 panic thread main at x.rs:1: boom\n' > files/sentinel/diag.log"
    local problems=""
    start_app
    wait_for 40 page_is_up
    on_screen_is 0 20 || problems="$problems; not on the alarms"
    on_screen "Something went wrong last time. Send a report?" 8 || problems="$problems; no banner after a panic in the log: $(texts_of | cut -c1-200)"
    screenshot s74_banner
    tap "Something went wrong last time. Send a report?" 0 0.6
    on_screen_is 7 || problems="$problems; the banner does not open the report (screen $(screen_no))"
    dev input keyevent KEYCODE_BACK
    on_screen_is 0 || problems="$problems; back does not return to the alarms"
    # Hiding it keeps it away, also after a restart.
    on_screen "Something went wrong last time. Send a report?" 3 || problems="$problems; banner gone before it was dismissed"
    { tap_in "Dismiss" 0 0.5 || tap_in "Do not show this here" 1 0.5; } || problems="$problems; no dismiss button"
    is_not_there "Something went wrong last time. Send a report?" || problems="$problems; still there after dismissing"
    dev am force-stop "$PKG"
    start_app
    wait_for 40 page_is_up
    on_screen_is 0 20
    sleep 2
    is_not_there "Something went wrong last time. Send a report?" || problems="$problems; back after a restart"
    if [ -z "$problems" ]; then
        pass "banner after a panic, opens the report, dismissed for good"
    else
        fail "${problems#; }"
    fi
}

# What the log says after a real ring and a snooze, and that it never holds a label or a note.
s75() {
    scenario s75_log_events
    fresh
    next_minute
    seed a "$(alarm_json a "$T_H" "$T_M" "$EVERY_DAY" null)"
    arm a
    local problems="" log
    if await_ring a 40 && on_screen "Turn off" 15; then
        tap "Turn off" 0 0.5
        wait_for 8 sound_off || problems="$problems; still ringing"
    else
        problems="$problems; did not ring: state=$(state a)"
    fi
    sleep 2
    go_tab Alarms
    tap "+5" 0 0.8
    sleep 1
    log=$(as_app "cat files/sentinel/diag.log files/sentinel/diag.1.log 2>/dev/null")
    echo "$log" > "$OUT/s75_diag.log"
    for needle in " start " " permissions " " alarm added " " schedule " " rearm " " ring started " " ring stopped "; do
        echo "$log" | grep -q -- "$needle" || problems="$problems; no '$needle' line"
    done
    echo "$log" | grep -q "after its time" || problems="$problems; no lateness"
    if [ -z "$problems" ]; then
        pass "start, permissions, schedule, rearm, ring started with lateness, ring stopped"
    else
        fail "${problems#; }"
    fi
}

# Pictures of every new screen in dark and light, at font scale 1.0 and 1.3, for a person to look at.
looks() {
    scenario looks
    local theme scale tag
    for theme in Dark Light; do
        for scale in 1.0 1.3; do
            tag="${theme}_${scale}"
            fresh
            use_settings "\"$theme\"" '"English"' '"setup_done":false,"language_follows_phone":false'
            as_app "mkdir -p files/sentinel && printf '2026-11-14T07:00:01.000+01:00 ring started id=aaaaaaaa 1s after its time challenges=0
2026-11-14T09:30:00.000+01:00 panic thread main at x.rs:1: boom
' > files/sentinel/diag.log"
            dev settings put system font_scale "$scale"
            start_app
            wait_for 40 page_is_up
            sleep 4
            screenshot "look_intro1_$tag"
            tap "Next" 0 0.5
            tap "Next" 0 0.5
            screenshot "look_intro3_$tag"
            tap "Skip" 0 0.6
            tap "Get started" 0 0.8
            on_screen "Something went wrong last time. Send a report?" 6
            screenshot "look_home_banner_$tag"
            tap "Something went wrong last time. Send a report?" 0 0.8
            set_field 0 "The alarm did not ring this morning and I woke up late"
            screenshot "look_report_$tag"
            tap "Show what is sent" 0 0.8
            screenshot "look_preview_$tag"
            tap "Back to the form" 0 0.5
            dev input keyevent KEYCODE_BACK
            sleep 0.5
            go_tab Settings
            scroll_up
            sleep 0.6
            screenshot "look_settings_$tag"
            settings_row "Help"
            sleep 0.5
            tap "Why did my alarm not ring?" 0 0.6
            screenshot "look_help_$tag"
        done
    done
    dev settings put system font_scale 1.0
    pass "pictures in $OUT/look_*.png"
}

# The existing screens at font scale 1.3, for a person to look at.
looks13() {
    scenario looks13
    fresh
    ALARM_EXTRA=',"note":"Take the laptop charger"'
    seed a "$(alarm_json a 7 0 '[true,true,true,true,true,false,false]' null air 0 30 3 5 '[["challenge.math","{\"difficulty\":\"medium\"}"],["challenge.memory","{}"]]')"
    ALARM_EXTRA=
    dev settings put system font_scale 1.3
    start_app
    wait_for 40 page_is_up
    sleep 4
    screenshot look13_home
    press open a 0.8
    sleep 1
    screenshot look13_editor
    scroll_up
    screenshot look13_editor2
    dev input keyevent KEYCODE_BACK
    sleep 0.5
    tap "Discard" 0 0.5
    go_tab History
    sleep 0.8
    screenshot look13_history
    go_tab Settings
    sleep 0.8
    screenshot look13_settings
    tap "Reliability" 0 0.8
    screenshot look13_reliability
    dev input keyevent KEYCODE_BACK
    go_tab Settings
    tap "Theme" 0 0.8
    screenshot look13_theme
    dev settings put system font_scale 1.0
    pass "pictures in $OUT/look13_*.png"
}

LIST="${*:-s70 s71 s72 s73 s74 s75}"
for s in $LIST; do
    "$s"
done
echo
echo "===== SUMMARY ($SERIAL) ====="
cat "$RESULTS"
dev settings put system font_scale 1.0
[ -z "${KEEP:-}" ] && emulator_stop
exit 0
