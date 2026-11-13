#!/bin/bash
# Every screen and panel, reached by touching what a person touches and scrolled through by touch,
# with a picture of each stop and a check of what is wrong there. The pictures are for a person
# to look at: only that finds what looks wrong. Sourced by matrix.sh.
#
# They end up in <run>/panels/, numbered in the order a person would see them.
#
# What is checked at a stop: text that sticks out of the window at the sides, a screen that does not
# scroll when it says there is more (the picture does not change after a swipe), a choice that does not
# take, a switch that does not switch, a panel that stays open when it should close.

PANEL_N=0
PANEL_PROBLEMS=""
panel_problem() {
    PANEL_PROBLEMS="$PANEL_PROBLEMS; $1"
    echo "$PANEL_N  $1" >> "$OUT/panels${VARIANT:-}/problems.txt"
}

picture() {
    PANEL_N=$((PANEL_N + 1))
    PANEL_LAST="$OUT/panels${VARIANT:-}/$(printf %03d "$PANEL_N")_$1.png"
    adb_ exec-out screencap -p > "$PANEL_LAST"
}

# What is wrong with what is on screen.
panel_check() {
    clipped
}

# Every stop of what is on top: a check and a picture, and the next one by touch. A swipe that
# changes nothing is the end of what scrolls.
look_through() { # name
    local name=$1 stop=0 found prev="" size w h
    size=$(window_size)
    w=${size%% *}
    size=${size#* }
    h=${size%% *}
    while [ "$stop" -lt 8 ]; do
        stop=$((stop + 1))
        found=$(panel_check)
        [ -z "$found" ] || panel_problem "$name: $found"
        picture "${name}_$stop"
        if [ -n "$prev" ] && cmp -s "$prev" "$PANEL_LAST"; then
            rm -f "$PANEL_LAST"
            PANEL_N=$((PANEL_N - 1))
            break
        fi
        prev=$PANEL_LAST
        dev input swipe $((w / 2)) $((h * 75 / 100)) $((w / 2)) $((h * 30 / 100)) 300
        sleep 0.5
    done
    # Back to the top, for what comes next.
    dev input swipe $((w / 2)) $((h * 25 / 100)) $((w / 2)) $((h * 85 / 100)) 200
    dev input swipe $((w / 2)) $((h * 25 / 100)) $((w / 2)) $((h * 85 / 100)) 200
    sleep 0.3
}

# The things on screen that are on or off: "text|x|y|state|role", one per line.
checkables() {
    ui texts | python -I -c '
import json, sys
seen = set()
for e in json.loads(sys.stdin.read()):
    if e.get("on") is None or e["t"] in seen:
        continue
    seen.add(e["t"])
    print("%s|%d|%d|%s|%s" % (e["t"], e["x"], e["y"], str(e["on"]).lower(), e["r"]))
'
}

# Every choice and switch on screen does what it does: a switch goes over and back, a choice takes.
# Then it is as it was.
try_chips() { # name
    local name=$1 line text x y was now role before=$(checkables)
    [ -n "$before" ] || return 0
    while IFS='|' read -r text x y was role; do
        [ -n "$text" ] || continue
        dev input tap "$x" "$y"
        sleep 0.2
        now=$(checkables | grep -F -m1 "$text|" | cut -d'|' -f4)
        case "$role" in
            Switch)
                [ "$now" != "$was" ] || panel_problem "$name: the switch '$text' does not switch"
                dev input tap "$x" "$y"
                sleep 0.2
                ;;
            *)
                [ "$now" = true ] || [ "$was" = true ] || panel_problem "$name: the choice '$text' does not take"
                ;;
        esac
    done <<< "$before"
    # Choices go back by taking what was on, last.
    while IFS='|' read -r text x y was role; do
        [ "$was" = true ] && [ "$role" != Switch ] && { dev input tap "$x" "$y"; sleep 0.15; }
    done <<< "$before"
    [ "$(checkables | sort)" = "$(echo "$before" | sort)" ] ||
        panel_problem "$name: the choices do not go back"
}

# The ? on top, if there is one: it opens the explanation in place, and closes it again.
panel_help() { # name
    local name=$1 before after
    is_there "What is this?" || return 0
    before=$(texts_of)
    tap "What is this?" 0 0.3
    after=$(texts_of)
    if [ "$before" != "$after" ]; then
        look_through "${name}_help"
    else
        panel_problem "$name: the ? opens nothing"
    fi
    tap "What is this?" 0 0.3
    [ "$(texts_of)" = "$before" ] || panel_problem "$name: the ? does not close"
}

any_sheet_up() { sheet_open || panel_up || editor_sheet_open; }
no_sheet_up() { ! any_sheet_up; }

# A panel that opened: all of it, and what closes it closes it and nothing else: Done from a row,
# Back from anywhere else.
panel_open() { # name
    local name=$1
    if ! wait_for 4 any_sheet_up; then
        panel_problem "$name: no panel"
        return 1
    fi
    look_through "$name"
    panel_help "$name"
    try_chips "$name"
    if is_there Done; then
        tap Done 0 0.3
        wait_for 3 no_sheet_up || {
            panel_problem "$name: Done leaves it open"
            press_back
        }
        return 0
    fi
    press_back
    wait_for 3 no_sheet_up || {
        panel_problem "$name: Back leaves it open"
        press_back
    }
}

# A week of history, with what can go wrong in it too.
panel_history_rows() {
    local now i day
    now=$(now_s)
    {
        for i in 0 1 2 3 4 6 8 9; do
            day=$(date -u -d "@$((now - i * 86400))" +%Y-%m-%dT07:00:00+00:00)
            printf "INSERT INTO alarm_events (id, alarm_id, alarm_label, fire_timestamp, first_interaction_timestamp, dismiss_timestamp, snooze_count, snooze_intervals, challenge_solves, total_dismissal_time_ms, outcome, seen) VALUES ('e%s', 'a', 'Work', '%s', '%s', '%s', %s, '%s', '[{\"challenge_type\":\"challenge.math\",\"difficulty\":\"medium\",\"time_ms\":%s000}]', %s, 'solved', 1);\n" \
                "$i" "$day" "$day" "$day" "$((i % 3))" "$([ $((i % 3)) = 0 ] && echo '[]' || echo '[5]')" "$((20 + i * 3))" "$(((40 + i * 25) * 1000))"
        done
        day=$(date -u -d "@$((now - 5 * 86400))" +%Y-%m-%dT07:00:00+00:00)
        printf "INSERT INTO alarm_events (id, alarm_id, alarm_label, fire_timestamp, dismiss_timestamp, total_dismissal_time_ms, outcome, seen) VALUES ('m1', 'a', 'Work', '%s', '%s', 0, 'missed', 1);\n" "$day" "$day"
        day=$(date -u -d "@$((now - 7 * 86400))" +%Y-%m-%dT07:00:00+00:00)
        printf "INSERT INTO alarm_events (id, alarm_id, alarm_label, fire_timestamp, dismiss_timestamp, total_dismissal_time_ms, outcome, seen) VALUES ('l1', 'a', 'Gym', '%s', '%s', 0, 'limit', 1);\n" "$day" "$day"
    } | sql
}

# Alarms of every kind, with a long name, and all the challenges there are.
panel_alarms() {
    local weekdays='[true,true,true,true,true,false,false]' weekend='[false,false,false,false,false,true,true]' later every
    later=$(date -u -d "@$(($(now_s) + 3 * 86400))" +%Y-%m-%d)
    every='[["challenge.math","{\"difficulty\":\"medium\"}"],["challenge.typing","{\"difficulty\":\"easy\"}"],["challenge.memory","{\"difficulty\":\"easy\"}"],["challenge.reaction","{\"count\":10,\"time_ms\":3000,\"rounds\":1}"],["challenge.hold","{\"count\":1,\"hold_ms\":3000}"],["challenge.steps","{\"count\":20}"],["challenge.shake","{\"count\":5}"],["challenge.scan","{\"value\":\"12345\"}"]]'
    ALARM_EXTRA=',"note":"Take the laptop charger"'
    seed work "$(alarm_json work 7 0 "$weekdays" null air 0 60 3 5 "$every")"
    ALARM_EXTRA=
    seed long "$(alarm_json long 9 30 "$weekend" null happy 0 0 0 5 '[]' 3600)"
    seed date "$(alarm_json date 14 15 "$NO_DAYS" "\"$later\"" lofi 2 0 3 5 '[["challenge.hold","{\"count\":1,\"hold_ms\":3000}"]]')"
    seed gym "$(alarm_json gym 18 45 "$EVERY_DAY" null funk 1 0 1 5 '[["challenge.scan","{}"]]')"
    {
        printf "UPDATE alarms SET data = replace(data, '\"enabled\":true', '\"enabled\":false') WHERE id='gym';\n"
        printf "UPDATE alarms SET data = replace(data, '\"label\":\"long\"', '\"label\":\"A very long label that keeps going on and on\"') WHERE id='long';\n"
        printf "UPDATE alarms SET data = replace(data, '\"label\":\"work\"', '\"label\":\"Work\"') WHERE id='work';\n"
        printf "UPDATE alarms SET data = replace(data, '\"label\":\"date\"', '\"label\":\"Dentist\"') WHERE id='date';\n"
        printf "UPDATE alarms SET data = replace(data, '\"label\":\"gym\"', '\"label\":\"Gym\"') WHERE id='gym';\n"
    } | sql
}

# How it looks on this device: theme, language, time format, text size.
panel_look() {
    local look
    case "$AVD" in
        af31) look="Light German H12 1.3 Blue" ;;
        phone) look="Dark Dutch H24 1.0 Blue" ;;
        *) look="Dark English H24 1.0 Blue" ;;
    esac
    [ -z "${VARIANT:-}" ] || look="Black English H12 1.0 Blue"
    # TOUR_LOOK="Black Dutch H24 1.0 Amber": how someone has it, instead of the device's own.
    look=${TOUR_LOOK:-$look}
    read -r PANEL_THEME PANEL_LANGUAGE PANEL_HOURS PANEL_FONT PANEL_ACCENT <<< "$look"
}
# Without animations: there is nothing to wait for, and no picture of something half way.
panel_settings() { use_settings "\"$PANEL_THEME\"" "\"$PANEL_LANGUAGE\"" "\"setup_done\":$1,\"language_follows_phone\":false,\"less_motion\":true,\"time_format\":\"$PANEL_HOURS\",\"accent\":\"$PANEL_ACCENT\""; }

# Each row of the screen on top: one that opens a panel or a screen is looked through and left,
# one with a switch goes over and back. The row for a widget is for s40: it asks the launcher.
# The rows are the buttons of the screen, which a person reads as "title value".
panel_rows() { # name screen-number
    local name=$1 here=$2 line text x y role was now before_n
    local rows
    rows=$(ui texts | python -I -c '
import json, sys
seen = set()
for e in json.loads(sys.stdin.read()):
    if e["r"] != "Button" or e["t"] in seen or e["w"] < 300:
        continue
    seen.add(e["t"])
    print("%s|%d|%d" % (e["t"], e["x"], e["y"]))
')
    while IFS='|' read -r text x y; do
        [ -n "$text" ] || continue
        case "$text" in
            *idget* | "Create new alarm" | Alarms | History | Settings) continue ;;
        esac
        dev input tap "$x" "$y"
        sleep 0.4
        if any_sheet_up; then
            panel_open "${name}_$(echo "$text" | tr -c 'a-zA-Z0-9\n' _ | cut -c1-24)"
        elif ! on_screen_no "$here"; then
            look_through "${name}_$(echo "$text" | tr -c 'a-zA-Z0-9\n' _ | cut -c1-24)"
            press_back
            wait_for 4 on_screen_no "$here" || panel_problem "$name: not back from $text"
        fi
    done <<< "$rows"
}

# The challenges panel of the editor on screen: each challenge's settings, adding one, and each as
# it is when it has to be done.
panel_challenges() {
    local added
    tap "Add a challenge" 0 0.4
    look_through challenge_add
    panel_help challenge_add
    press_back
    wait_for 3 editor_sheet_open || panel_problem "not back from adding a challenge"
    tap Try L 0.4
    if wait_for 5 run_up; then
        sleep 0.3
        focus_in_app || { press_back; sleep 0.5; }
        found=$(panel_check)
        [ -z "$found" ] || panel_problem "a challenge: $found"
        picture challenge_run
        if is_there "Can't do this one? Hold instead"; then
            tap "Can't do this one? Hold instead" 0 0.3
            look_through challenge_other_way
            press_back
        fi
        press_back
        wait_for 4 editor_sheet_open || panel_problem "not back from trying a challenge"
    else
        panel_problem "a challenge cannot be tried"
    fi
}

# The theme screen: what each theme looks like, the wheel, and a theme of one's own.
panel_theme_stops() {
    local name
    for name in "Same as the phone" Dark Light "Pure black" Midnight Forest Plum Sand Mist; do
        tap "$name" 0 0.3
        is_on "$name" || panel_problem "theme $name does not take"
        picture "theme_$(echo "$name" | tr ' ' _)"
    done
    tap Dark 0 0.3
    tap "Choose your own colour" 0 0.4
    panel_open theme_accent_wheel
    tap "Your own theme" 0 0.4
    look_through theme_own
    for name in Background Cards Text; do
        tap "$name" L 0.4
        panel_open "theme_own_$name"
    done
    tap Dark 0 0.3
}

s57() {
    scenario s57_every_panel
    panel_look
    PANEL_N=0
    PANEL_PROBLEMS=""
    mkdir -p "$OUT/panels${VARIANT:-}"
    rm -f "$OUT/panels${VARIANT:-}"/*
    local found name old_font
    old_font=$(dev settings get system font_scale)
    # A phone keeps the size of text it has. An emulator gets the size of that phone, the narrowest
    # there is much of, unless this is about another size.
    if on_emulator; then
        dev settings put system font_scale "$PANEL_FONT"
        if [ -z "${VARIANT:-}" ]; then
            dev wm size 1080x2376
            dev wm density 480
        fi
    fi

    # The first start.
    fresh
    panel_settings false
    start_app
    wait_for 15 on_screen_no 4 || panel_problem "no first-run screen"
    look_through setup
    dev am force-stop "$PKG"

    # Nothing yet.
    fresh
    panel_settings true
    start_app
    on_screen "No alarms yet" 15 || panel_problem "no empty list"
    look_through home_empty
    go_tab History
    wait_for 4 on_screen_no 2
    look_through history_empty
    dev am force-stop "$PKG"

    # The list, and everything from it.
    fresh
    panel_settings true
    panel_alarms
    panel_history_rows
    start_app
    on_card 15 || panel_problem "no list"
    # What the phone says about itself takes a moment to show.
    sleep 1
    look_through home
    tap "Pause all alarms until a date" 0 0.4
    panel_open home_pause
    tap "Pause all alarms until a date" 0 0.4
    if wait_for 4 sheet_open; then
        tap "Until a date you pick" 0 0.4
        look_through home_pause_date
        press_back
        wait_for 3 sheet_closed || press_back
    fi
    tap "Sort alarms" 0 0.4
    panel_open home_sort
    local i n
    n=$(ui snapshot | grep -o '"id":"' | wc -l)
    i=0
    while [ "$i" -lt "${n:-0}" ]; do
        tap "More actions" "$i" 0.4
        panel_open "home_menu$i"
        i=$((i + 1))
    done

    # A quick alarm, and what it says.
    tap "+5" 0 0.4
    [ "$(sn toast_visible)" = true ] || panel_problem "a quick alarm says nothing"
    picture home_quick_alarm
    tap Undo 0 0.3

    # The editor of a new alarm.
    open_new_editor || panel_problem "no editor for a new alarm"
    look_through editor_new
    tap "One date" 0 0.4
    panel_open editor_date
    press_back
    wait_for 4 editor_sheet_open && {
        look_through editor_discard
        tap Discard 0 0.3
    }
    on_home 5 || panel_problem "not back on the list from a new alarm"

    # The editor of an alarm with everything on it.
    press open work 0.8
    on_editor 5 || panel_problem "no editor"
    look_through editor
    # The small steps either side of the time: a minute later, and back.
    local before after
    before=$(sn ed_time_text)
    tap "+1" 0 0.2
    after=$(sn ed_time_text)
    tap "−1" 0 0.2
    [ "$before" != "$after" ] && [ "$(sn ed_time_text)" = "$before" ] ||
        panel_problem "the minute buttons do not move the time a minute and back"
    tap ":" 0 0.4
    panel_open editor_time
    tap_trash
    panel_open editor_delete
    # Each row of the editor.
    for name in Label Sound Vibration Snooze 'Alarm screen' 'Ring limit' 'Are you up?' 'Notice before'; do
        tap_has "$name" 0 0.4
        panel_open "editor_$(echo "$name" | tr -c 'a-zA-Z0-9\n' _)"
    done
    # Trying them is what the panels of sound and vibration are about: what it looks like then.
    tap_has Sound 0 0.5
    if is_there Start; then
        tap Start 0 0.3
        picture editor_sound_playing
        tap Stop 0 0.3
        press_back
        wait_for 3 editor_sheet_closed
    else
        panel_problem "no way to try a sound"
    fi
    tap_has Vibration 0 0.5
    if is_there Steady; then
        tap Steady 0 0.3
        is_there Stop || panel_problem "a tap on a pattern shows no way to stop it"
        picture editor_vibration_running
        press_back
        wait_for 3 editor_sheet_closed
    else
        panel_problem "no vibration panel"
    fi
    # Every look of the ring screen as big as it will be. A moment first: what moves has to get going.
    tap_has 'Alarm screen' 0 0.5
    if wait_for 3 editor_sheet_open; then
        look_preview() { # picture
            tap Preview L 0.5
            if wait_for 3 preview_up; then
                sleep 1.5
                found=$(panel_check)
                [ -z "$found" ] || panel_problem "$1: $found"
                picture "$1"
                tap "Preview. Tap to close." 0 0.4
                wait_for 3 preview_down || panel_problem "$1: not back from the preview"
            else
                panel_problem "$1: no preview"
            fi
        }
        for name in Aurora Sunrise Waves Stars Rings Bubbles; do
            tap_in "$name" 0 0.3
            look_preview "ring_look_$name"
        done
        tap_has Colour 0 0.5
        if on_screen "Colour code" 3 || on_screen_has code 2; then
            tap_in Coral 0 0.3
            picture ring_look_colour
            tap Done 0 0.3
            look_preview ring_look_own_colour
        else
            panel_problem "no colour for the ring screen"
        fi
        for name in Thin Bold Classic Mono; do
            tap_in "$name" 0 0.3
            look_preview "ring_clock_$name"
        done
        tap_in Plain 0 0.3
        tap_in Clean 0 0.3
        press_back
        wait_for 3 editor_sheet_closed
    else
        panel_problem "no panel for the ring screen"
    fi
    tap_has Challenges 0 0.5
    if wait_for 3 editor_sheet_open; then
        is_there "Try all" || panel_problem "no way to try them all"
        panel_challenges
        press_back
        wait_for 3 editor_sheet_closed || panel_problem "Back leaves the challenges open"
    else
        panel_problem "no challenges panel"
    fi
    press_back
    wait_for 4 editor_sheet_open && {
        look_through editor_discard_changes
        tap Discard 0 0.3
    }
    on_home 5 || panel_problem "not back on the list from the editor"

    # History.
    go_tab History
    wait_for 5 on_screen_no 2
    look_through history
    press h-delete-history "" 0.5
    panel_open history_delete
    # A period asks first too, and Back is no.
    press h-delete-history "" 0.5
    wait_for 3 panel_up
    tap "Older than 30 days" 0 0.4
    if is_there Delete; then
        look_through history_delete_period
        press_back
        wait_for 3 panel_down || panel_problem "history_delete_period: Back leaves it open"
    else
        panel_problem "deleting a period does not ask"
    fi
    tap_has "All stats" 0 0.4
    look_through history_all_stats
    tap_has "Fewer stats" 0 0.3
    for name in "This week" "Last week" "This month" "Last month" "This year" "Last year" "Your own days"; do
        tap "$name" 0 0.3
        found=$(panel_check)
        [ -z "$found" ] || panel_problem "history filter $name: $found"
        picture "history_filter_$(echo "$name" | tr ' ' _)"
    done

    # Settings, and what is behind them.
    go_tab Settings
    wait_for 5 on_screen_no 3
    look_through settings
    panel_rows settings 3

    # Colours, on a screen of their own. Every theme, and what is behind choosing your own.
    tap_has Theme 0 0.5
    if wait_for 5 on_screen_no 5; then
        look_through theme
        panel_theme_stops
        press_back
        wait_for 5 on_screen_no 3 || panel_problem "not back from the theme screen"
    else
        panel_problem "no theme screen"
    fi
    tap Reliability 0 0.5
    if wait_for 5 on_screen_no 4; then
        look_through reliability
        panel_help reliability
        press_back
        wait_for 5 on_screen_no 3 || panel_problem "not back from reliability"
    else
        panel_problem "no reliability screen"
    fi
    dev am force-stop "$PKG"

    # The alarm itself, what its challenge looks like, and what comes after.
    fresh
    panel_settings true
    panel_history_rows
    next_minute
    ALARM_EXTRA=',"note":"Take the laptop charger"'
    seed a "$(alarm_json a "$T_H" "$T_M" "$EVERY_DAY" null air 0 0 3 1 '[["challenge.math","{\"difficulty\":\"easy\"}"]]')"
    ALARM_EXTRA=
    arm a
    if await_ring a 70 && on_screen "Turn off" 15; then
        # As soon as it is there: a banner over it would be gone after some seconds.
        sleep 0.5
        found=$(panel_check)
        [ -z "$found" ] || panel_problem "ring: $found"
        picture ring
        tap "Turn off" 0 0.3
        wait_for 5 run_up || panel_problem "no challenge on the alarm"
        found=$(panel_check)
        [ -z "$found" ] || panel_problem "ring challenge: $found"
        picture ring_challenge
        solve_math
        solved() { state_is a scheduled; }
        wait_for 8 solved || panel_problem "solving it did not end it: state=$(state a)"
        sleep 1
        picture ring_after
    else
        panel_problem "the alarm did not ring: state=$(state a)"
    fi
    dev am force-stop "$PKG"
    if on_emulator; then
        dev settings put system font_scale "$old_font"
        if [ -z "${VARIANT:-}" ]; then
            dev wm size reset
            dev wm density reset
        fi
    fi

    if [ -z "$PANEL_PROBLEMS" ]; then
        pass "$PANEL_N pictures in $PANEL_THEME $PANEL_LANGUAGE, nothing cut off, covered or stuck"
    else
        fail "${PANEL_PROBLEMS#; }"
    fi
}
