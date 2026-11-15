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

# What the screen under a panel has that can be on or off: a panel's scrim leaves it in the element
# tree but a touch on it only closes the panel. Taken at every stop without a panel.
PANEL_BASELINE=""
PANEL_BASELINE_SCREEN=""
sheet_checkables() {
    local line names
    # By the text alone: what is under the panel may have moved or switched since.
    names=$(printf '%s
' "$PANEL_BASELINE" | cut -d'|' -f1)
    # The tabs under the scrim are no choice of a panel: with 3-button navigation they sit above the
    # bottom of the window, where the tour touches what it finds.
    checkables | grep -v '|Tab$' | while IFS= read -r line; do
        printf '%s
' "$names" | grep -qxF -- "${line%%|*}" || printf '%s
' "$line"
    done
}

# Everything seen on this screen so far, also what a scroll took out of the window.
baseline_update() {
    any_sheet_up && return 0
    local here
    here=$(screen_no)
    [ "$here" = "$PANEL_BASELINE_SCREEN" ] || PANEL_BASELINE=""
    PANEL_BASELINE_SCREEN=$here
    PANEL_BASELINE=$(printf '%s
%s' "$PANEL_BASELINE" "$(checkables)")
}

picture() {
    baseline_update
    PANEL_N=$((PANEL_N + 1))
    PANEL_LAST="$OUT/panels${VARIANT:-}/$(printf %03d "$PANEL_N")_$1.png"
    screen_may_be_taken || { note "no picture $1: AlarmFree is not in front"; return 0; }
    adb_ exec-out screencap -p > "$PANEL_LAST"
}

# What is wrong with what is on screen.
panel_check() {
    clipped
}

# Every stop of what is on top: a check and a picture, and the next one by touch. A swipe that
# changes nothing is the end of what scrolls.
# Where the texts are, without what they say.
layout_sig() {
    ui texts | python -I -c 'import json,sys; print(sorted(e["y"] for e in json.loads(sys.stdin.read())))' | md5sum | cut -d' ' -f1
}
look_through() { # name
    local name=$1 stop=0 moved=0 found prev="" sig first_sig="" size w h
    size=$(window_size)
    w=${size%% *}
    size=${size#* }
    h=${size%% *}
    while [ "$stop" -lt 8 ]; do
        stop=$((stop + 1))
        found=$(panel_check)
        [ -z "$found" ] || panel_problem "$name: $found"
        picture "${name}_$stop"
        # Moved or not, by where things are (a picture also differs when the clock in the status bar does).
        sig=$(layout_sig)
        if [ -n "$prev" ] && [ "$prev" = "$sig" ]; then
            rm -f "$PANEL_LAST"
            PANEL_N=$((PANEL_N - 1))
            break
        fi
        [ -z "$prev" ] || moved=1
        prev=$sig
        [ -n "$first_sig" ] || first_sig=$sig
        dev input swipe $((w / 2)) $((h * 75 / 100)) $((w / 2)) $((h * 30 / 100)) 300
        sleep 0.5
    done
    # Back to the top, for what comes next: only when it moved, and from inside what moved (a drag
    # from the scrim of a panel closes it).
    if [ "$moved" = 1 ]; then
        local k=0
        # Short pulls until it is where it was: a long one past the top would pull a panel shut.
        while [ "$k" -lt 16 ] && [ "$(layout_sig)" != "$first_sig" ]; do
            dev input swipe $((w / 2)) $((h * 45 / 100)) $((w / 2)) $((h * 70 / 100)) 250
            sleep 0.4
            k=$((k + 1))
        done
    fi
    sleep 0.3
}

# The things on screen that are on or off: "text|x|y|state|role", one per line.
checkables() { checkables_raw | tr -d '\r'; }
checkables_raw() {
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
# A panel whose rows close it on a pick (language, time format, week start, new alarms, volume ...) gets
# the opening row's place in $2 $3: it is opened again to see what the pick did, and what was on is
# taken again. Returns 3 when the panel closed itself that way, and is closed again then.
try_chips() { # name [x y of the row that opened it]
    local ox=${2:-} oy=${3:-} window
    local name=$1 line text x y was now role floor before=$(sheet_checkables | grep -v '^What is this?|')
    # What is below the window (a long list in a panel) is not touched.
    floor=$(( $(window_size | cut -d' ' -f2) - 200 ))
    # The buttons that stay at the bottom of a panel cover what is under them.
    for text in Done Start Stop; do
        y=$(where "$text" | cut -d' ' -f2)
        case "$y" in '' | *[!0-9]*) ;; *) [ "$y" -gt 1200 ] && [ $((y - 100)) -lt "$floor" ] && floor=$((y - 100)) ;; esac
    done
    [ -n "$before" ] || return 0
    while IFS='|' read -r text x y was role; do
        [ -n "$text" ] || continue
        [ "$y" -lt "$floor" ] || continue
        dev input tap "$x" "$y"
        sleep 0.2
        if ! focus_in_app; then
            # Something else came up over the app. The phone's picker is what a choice such as Photo (the
            # ring screen, no photo yet) is for: Back returns. Anything else is a problem, with the
            # window and the log kept to see what it was.
            sleep 1
            window=$(focused_window | grep -o '[A-Za-z.]*/[A-Za-z.]*' | head -1)
            case "$window" in
                *[Pp]icker* | *[Dd]ocuments* | *MediaProvider* | *[Gg]allery*)
                    note "$name: '$text' opens $window"
                    ;;
                *)
                    panel_problem "$name: after '$text' the app is not in front: $window"
                    adb_ logcat -d -t 800 -b all -v time 2> /dev/null | grep -E "AlarmFree|Sentinel|AndroidRuntime|FATAL|am_proc_died|am_kill|ActivityTaskManager" > "$OUT/panels${VARIANT:-}/lost_$(echo "${name}_$text" | tr -c 'a-zA-Z0-9
' _ | cut -c1-60).log"
                    ;;
            esac
            press_back
            wait_for 8 focus_in_app || { panel_problem "$name: not back from '$text' ($window)"; press_back; }
            sleep 0.5
            continue
        fi
        if no_sheet_up; then
            # The pick took and closed the panel by itself.
            [ "$role" != Switch ] || panel_problem "$name: the switch '$text' closes the panel"
            if [ -z "$ox" ]; then
                panel_problem "$name: the choice '$text' closes the panel, and there is no row to open it again from"
                return 3
            fi
            dev input tap "$ox" "$oy"
            sleep 0.7
            wait_for 3 any_sheet_up || { panel_problem "$name: the panel does not open again"; return 3; }
            now=$(checkables | grep -F -m1 "$text|" | cut -d'|' -f4)
            [ "$now" = true ] || [ "$was" = true ] || panel_problem "$name: the choice '$text' does not take"
            # As it was: take what was on, which closes the panel again.
            while IFS='|' read -r text x y was role; do
                [ "$was" = true ] && [ "$role" != Switch ] && { dev input tap "$x" "$y"; sleep 0.7; break; }
            done <<< "$before"
            wait_for 3 no_sheet_up || { panel_problem "$name: the choice that was on leaves the panel open"; press_back; }
            return 3
        fi
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
        [ "$was" = true ] && [ "$role" != Switch ] && [ "$y" -lt "$floor" ] && { dev input tap "$x" "$y"; sleep 0.15; }
    done <<< "$before"
    # A colour that was automatic (nothing ringed) goes back by Automatic: a swatch can not be taken off.
    if ! printf '%s
' "$before" | grep -q '|true|' && is_there Automatic; then
        for i in 1 2 3; do
            tap Automatic 0 0.6
            [ "$(sheet_checkables | grep -v '^What is this?|' | sort)" = "$(echo "$before" | sort)" ] && break
        done
    fi
    [ "$(sheet_checkables | grep -v '^What is this?|' | sort)" = "$(echo "$before" | sort)" ] ||
        panel_problem "$name: the choices do not go back"
}

# The ? on top, if there is one: it opens the explanation in place, and closes it again.
panel_help() { # name
    local name=$1 before after sig0
    is_there "What is this?" || return 0
    sig0=$(layout_sig)
    before=$(texts_of)
    tap "What is this?" 0 0.3
    after=$(texts_of)
    if [ "$before" != "$after" ]; then
        look_through "${name}_help"
    else
        panel_problem "$name: the ? opens nothing"
    fi
    tap "What is this?" 0 0.3
    # By where things are: a countdown on the screen behind changes by itself.
    [ "$(layout_sig)" = "$sig0" ] || panel_problem "$name: the ? does not close"
}

any_sheet_up() { sheet_open || panel_up || editor_sheet_open || [ "$(sn theme_sheet)" = true ]; }
no_sheet_up() { ! any_sheet_up; }

# A row of a screen that scrolls: where it is, wherever the screen was left.
tap_row() { # text
    has_there "$1" || { sheet_scroll_up; has_there "$1" || sheet_scroll; }
    has_there "$1" || sheet_scroll
    # A row in the tree can sit behind the Save or Done button that stays at the bottom, or the
    # navigation bar: a touch there is theirs. Scroll until the row is clear of them.
    local i y b floor
    floor=$(( $(window_size | cut -d' ' -f2) - 200 ))
    for b in Save Done; do
        y=$(where "$b" | cut -d' ' -f2)
        case "$y" in '' | *[!0-9]*) ;; *) [ "$y" -gt 1200 ] && [ $((y - 120)) -lt "$floor" ] && floor=$((y - 120)) ;; esac
    done
    for i in 1 2 3; do
        y=$(where_has "$1" | cut -d' ' -f2)
        case "$y" in '' | *[!0-9]*) ;; *) [ "$y" -lt "$floor" ] && break ;; esac
        sheet_scroll
    done
    # What stands on this screen now (the labelled swatches of the theme screen under a panel) is no choice of the panel.
    baseline_update
    tap_has "$1" 0 0.5
}

# The language sheet: every language would change what the rest of the tour reads. One other one
# takes, and English (the tour's own) comes back.
try_language() { # name [x y of the row that opened it]
    tap_row "Nederlands"
    sleep 0.8
    is_there Instellingen || panel_problem "$1: Nederlands does not take"
    # A language closes the sheet. Open it again, now in Dutch, and take English from it.
    wait_for 3 no_sheet_up || { panel_problem "$1: choosing a language leaves the sheet open"; press_back; }
    if [ -n "${2:-}" ]; then
        dev input tap "$2" "$3"
    else
        tap_has Taal 0 0.3
    fi
    sleep 0.7
    wait_for 4 any_sheet_up || { panel_problem "$1: the sheet does not open again"; return; }
    # The last "English": the first one can be the row behind the sheet, which says the language it has.
    tap English L 0.5
    sleep 0.8
    is_there Settings || panel_problem "$1: English does not come back"
    wait_for 3 no_sheet_up || { panel_problem "$1: choosing English leaves the sheet open"; press_back; }
}

# A panel that opened: all of it, and what closes it closes it and nothing else: Done from a row,
# Back from anywhere else.
panel_open() { # name [x y of the row that opened it, for panels that close themselves]
    local name=$1
    if ! wait_for 4 any_sheet_up; then
        panel_problem "$name: no panel"
        return 1
    fi
    look_through "$name"
    panel_help "$name"
    case "$name" in
        *Language*)
            try_language "$name" "${2:-}" "${3:-}"
            sleep 0.6
            return 0
            ;;
        *) try_chips "$name" "${2:-}" "${3:-}" || { sleep 0.6; return 0; } ;;
    esac
    # A field the tour touched has the keyboard up, which covers Done and takes the first Back.
    hide_keyboard
    if is_there Done; then
        tap Done 0 0.3
        wait_for 3 no_sheet_up || {
            panel_problem "$name: Done leaves it open"
            press_back
        }
        sleep 0.6
        return 0
    fi
    press_back
    wait_for 3 no_sheet_up || {
        panel_problem "$name: Back leaves it open"
        press_back
    }
    # It slides out before it lets touches through.
    sleep 0.6
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
        afg12) look="Light English H12 1.3 Blue" ;; # the tour finds things by their English text (s34 covers the long languages)
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
    baseline_update
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
            panel_open "${name}_$(echo "$text" | tr -c 'a-zA-Z0-9\n' _ | cut -c1-24)" "$x" "$y"
        elif ! on_screen_no "$here"; then
            # What this screen has was learnt before; the screen opened has its own, and must not replace it.
            local saved=$PANEL_BASELINE saved_screen=$PANEL_BASELINE_SCREEN
            look_through "${name}_$(echo "$text" | tr -c 'a-zA-Z0-9\n' _ | cut -c1-24)"
            press_back
            wait_for 4 on_screen_no "$here" || panel_problem "$name: not back from $text"
            PANEL_BASELINE=$saved
            PANEL_BASELINE_SCREEN=$saved_screen
        fi
    done <<< "$rows"
}

# The challenges panel of the editor on screen: each challenge's settings, adding one, and each as
# it is when it has to be done.
panel_challenges() {
    local added
    # Under the eight challenges of this alarm: the list scrolls, and with a navigation bar the
    # button is not in the window until it has (nothing says it), or lies under Done.
    tap_clear "Add a challenge" "" 300
    look_through challenge_add
    panel_help challenge_add
    press_back
    wait_for 3 editor_sheet_open || panel_problem "not back from adding a challenge"
    sleep 0.8
    tap "Try all" 0 0.4
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
    # A phone takes a moment to repaint in the new colours, and a touch before that is lost.
    theme_tap() {
        local i
        for i in 1 2 3; do
            tap "$1" 0 1.0
            wait_for 2 is_on "$1" && break
        done
        sleep 0.4
    }
    for name in "Same as the phone" Dark Light "Pure black" Midnight Forest Plum Sand Mist; do
        theme_tap "$name"
        is_on "$name" || panel_problem "theme $name does not take"
        picture "theme_$(echo "$name" | tr ' ' _)"
    done
    theme_tap Dark
    tap_row "Choose your own colour"
    panel_open theme_accent_wheel
    tap_row "Your own theme"
    look_through theme_own
    for name in Background Cards Text; do
        tap_row "$name"
        panel_open "theme_own_$name"
    done
    is_there Dark || { sheet_scroll_up; sheet_scroll_up; }
    theme_tap Dark
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
    # The intro of three cards comes first; Skip goes on to the setup.
    wait_for 15 on_screen_no 6 || panel_problem "no intro before the first-run screen"
    look_through intro
    tap Skip 0 0.6
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
    local i n j v
    n=$(ui snapshot | grep -o '"id":"' | wc -l)
    i=0
    while [ "$i" -lt "${n:-0}" ]; do
        j=$i
        # The cards below the screen: scroll the list to the end, then count from the last one.
        if [ "$(where "More actions" "$i")" = none ]; then
            sheet_scroll
            v=$(ui texts | python -I -c 'import json,sys; print(sum(1 for e in json.loads(sys.stdin.read()) if e["t"] == "More actions" and e["r"] == "Button"))' | tr -d '
')
            j=$((i - n + ${v:-0}))
        fi
        tap "More actions" "$j" 0.4
        panel_open "home_menu$i"
        i=$((i + 1))
    done

    sheet_scroll_up
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
    tap_time 0.4
    panel_open editor_time
    tap_trash
    panel_open editor_delete
    # Each row of the editor.
    for name in Label Sound Vibration Snooze 'Alarm screen' 'Ring limit' 'Are you up?' 'Notice before'; do
        tap_row "$name"
        panel_open "editor_$(echo "$name" | tr -c 'a-zA-Z0-9\n' _)"
    done
    # Trying them is what the panels of sound and vibration are about: what it looks like then.
    tap_row Sound
    if is_there 'Play preview'; then
        tap 'Play preview' 0 0.3
        picture editor_sound_playing
        tap 'Stop preview' 0 0.3
        press_back
        wait_for 3 editor_sheet_closed
    else
        panel_problem "no way to try a sound"
    fi
    tap_row Vibration
    if is_there Steady; then
        tap Steady 0 0.3
        is_there 'Stop preview' || panel_problem "a tap on a pattern shows no way to stop it"
        picture editor_vibration_running
        press_back
        wait_for 3 editor_sheet_closed
    else
        panel_problem "no vibration panel"
    fi
    # Every look of the ring screen as big as it will be. A moment first: what moves has to get going.
    tap_row 'Alarm screen'
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
            sleep 0.3
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
    # The panel before this one is still sliding out: a touch now is lost.
    wait_for 4 no_sheet_up
    sleep 0.8
    tap_row Challenges
    wait_for 3 editor_sheet_open || { sleep 0.8; tap_row Challenges; }
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
    tap_row "Older than 30 days"
    if wait_for 3 is_there Delete; then
        look_through history_delete_period
        press_back
        wait_for 3 panel_down || panel_problem "history_delete_period: Back leaves it open"
    else
        panel_problem "deleting a period does not ask"
    fi
    tap_has "All stats" 0 0.4
    look_through history_all_stats
    tap_has "Fewer stats" 0 0.3
    prev_chip="This week"
    for name in "This week" "Last week" "This month" "Last month" "This year" "Last year" "Your own days"; do
        # The row of periods scrolls sideways: what is out of the window comes in by a swipe.
        k=0
        while ! has_there "$name" && [ "$k" -lt 3 ]; do
            y=$(where "$prev_chip" | cut -d' ' -f2)
            case "$y" in '' | *[!0-9]*) y=$(where "This month" | cut -d' ' -f2) ;; esac
            case "$y" in '' | *[!0-9]*) y=341 ;; esac
            dev input swipe 900 "$y" 100 "$y" 300
            sleep 0.6
            k=$((k + 1))
        done
        prev_chip=$name
        tap "$name" 0 0.3
        # The chips of the row scroll sideways: at the end of the row one of them is cut off at the
        # edge by design, with fewer than three others in the window (a narrower font than the phone's).
        found=$(panel_check | tr ',' '\n' | sed 's/^ *//' | grep -vxE 'This week|Last week|This month|Last month|This year|Last year|Your own days' | paste -sd, -)
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
    tap_row Reliability
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
