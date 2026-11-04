#!/bin/bash
# Every screen and panel, reached by clicking what a person clicks and scrolled through by touch,
# with a picture of each stop and a check of what is wrong there. The pictures are for a person
# to look at: only that finds what looks wrong. Sourced by matrix.sh.
#
# They end up in <run>/panels/, numbered in the order a person would see them.

# What scrolls on top right now.
SCROLLER="var s = document.querySelector('.sheet') || [...document.querySelectorAll('.scroll')].find(e => e.getBoundingClientRect().width > 0);"

# On one line, for the shell on the device.
page() { js "${1//$'\n'/ }"; }

# What is wrong with what is on top. At the end of what scrolls also what lies under something
# else, as in between a button that stays in place is meant to.
panel_check() { # end?
    local script="var bad = [], w = innerWidth, top = document.querySelector('.sheet') || document.querySelector('.screen') || document.body;
        var name = e => (e.textContent.trim().slice(0, 24) || e.className || e.tagName);
        top.querySelectorAll('*').forEach(e => {
            var r = e.getBoundingClientRect();
            if (r.width < 1 || r.height < 1) return;
            if ((r.right > w + 1 || r.left < -1) && !e.closest('.chip-scroll, .alarm-swipe, .quick-row, .wheel-column')) bad.push('sticks out: ' + name(e));
            if (e.children.length || !e.textContent.trim()) return;
            if (e.clientWidth && e.scrollWidth > e.clientWidth + 1 && !e.closest('.alarm-label, .chip-scroll, .row-value, .event-title, .chips:not(.chips-grow)')) bad.push('cut off: ' + name(e));
            if (!$1) return;
            var x = r.left + r.width / 2, y = r.top + r.height / 2;
            if (y < 0 || y > innerHeight || e.closest('.wheel-column')) return;
            var box = e.closest('.sheet, .scroll');
            if (box) { var b = box.getBoundingClientRect(); if (y < b.top || y > b.bottom) return; }
            var on = document.elementFromPoint(x, y);
            if (on && on !== e && !e.contains(on) && !on.contains(e) && !on.closest('svg, .reaction-countdown-num')) bad.push(name(e) + ' under ' + (on.className || on.tagName));
        });
        return [...new Set(bad)].slice(0, 4).join(', ');"
    ask_page "${script//$'\n'/ }"
}

PANEL_N=0
PANEL_PROBLEMS=""
panel_problem() {
    PANEL_PROBLEMS="$PANEL_PROBLEMS; $1"
    echo "$PANEL_N  $1" >> "$OUT/panels${VARIANT:-}/problems.txt"
}

picture() {
    PANEL_N=$((PANEL_N + 1))
    adb_ exec-out screencap -p > "$OUT/panels${VARIANT:-}/$(printf %03d "$PANEL_N")_$1.png"
}

# Every stop of what is on top: a check and a picture, and the next one by touch. It has to move
# when it says there is more.
look_through() { # name
    local name=$1 stop=0 top height x end found before after
    sleep 0.05
    read -r top height x <<< "$(page "$SCROLLER if (!s) return '0 0 0'; s.scrollTop = 0; var r = s.getBoundingClientRect(), f = devicePixelRatio;
        var left = document.querySelector('.sheet .wheel-column') ? r.width * 0.1 : r.width / 2;
        return Math.round(r.top * f) + ' ' + Math.round(r.height * f) + ' ' + Math.round((r.left + left) * f);")"
    while [ "$stop" -lt 8 ]; do
        stop=$((stop + 1))
        read -r before end <<< "$(js "$SCROLLER return !s ? '0 true' : Math.round(s.scrollTop) + ' ' + (s.scrollTop + s.clientHeight >= s.scrollHeight - 2);")"
        found=$(panel_check "$end")
        [ -z "$found" ] || panel_problem "$name: $found"
        picture "${name}_$stop"
        [ "$end" = true ] && break
        dev input swipe "$x" "$((top + height * 80 / 100))" "$x" "$((top + height * 25 / 100))" 300
        # It keeps going for a moment after the finger is off.
        sleep 0.4
        after=$(js "$SCROLLER return s ? Math.round(s.scrollTop) : 0;")
        if [ "$after" = "$before" ]; then
            panel_problem "$name: does not scroll by touch"
            js "$SCROLLER if (s) s.scrollTop += s.clientHeight * 0.7; return 'ok';" > /dev/null
            sleep 0.2
        fi
    done
}

# Every choice in the panel on top does what a choice does. Then it is back as it was, also for
# chips of which more than one can be on: first what has to go off, then what was on.
try_chips() { # name
    local name=$1 n i one two was round sel='.sheet button.chip[aria-pressed], .sheet button.swatch[aria-pressed]'
    n=$(js "return document.querySelectorAll('$sel').length;")
    [ "${n:-0}" -gt 0 ] 2> /dev/null || return 0
    was=$(js "return [...document.querySelectorAll('$sel')].map(e => e.getAttribute('aria-pressed')).join(' ');")
    i=0
    while [ "$i" -lt "$n" ]; do
        one=$(js "var c = document.querySelectorAll('$sel')[$i]; if (!c) return 'gone'; var v = c.getAttribute('aria-pressed'); c.click(); return v;")
        sleep 0.05
        two=$(js "var c = document.querySelectorAll('$sel')[$i]; return c ? c.getAttribute('aria-pressed') : 'gone';")
        [ "$one" = true ] || [ "$two" = true ] || panel_problem "$name: choice $i does not take: $one then $two"
        i=$((i + 1))
    done
    for round in false true; do
        js "var was = '$was'.split(' '); [...document.querySelectorAll('$sel')].forEach((e, i) => { if (was[i] === '$round' && e.getAttribute('aria-pressed') !== was[i]) e.click(); }); return 'ok';" > /dev/null
        sleep 0.05
    done
    # A value none of the choices has cannot go back: a phone run keeps its volume that low.
    case " $was " in
        *" true "*)
            [ "$(js "return [...document.querySelectorAll('$sel')].map(e => e.getAttribute('aria-pressed')).join(' ');")" = "$was" ] ||
                panel_problem "$name: the choices do not go back"
            ;;
    esac
}

# Every switch in the panel on top goes over and back.
try_switches() { # name
    local name=$1 n i one two
    n=$(js "return document.querySelectorAll('.sheet [role=switch]').length;")
    i=0
    while [ "$i" -lt "${n:-0}" ] 2> /dev/null; do
        one=$(js "var c = document.querySelectorAll('.sheet [role=switch]')[$i]; var v = c.getAttribute('aria-checked'); c.click(); return v;")
        sleep 0.05
        two=$(js "var c = document.querySelectorAll('.sheet [role=switch]')[$i]; var v = c.getAttribute('aria-checked'); c.click(); return v;")
        sleep 0.05
        [ "$one" != "$two" ] || panel_problem "$name: switch $i does not switch"
        i=$((i + 1))
    done
}

# A list to choose from, on top and opened by a row: another one is taken, closes the panel and
# shows in the row. Then back as it was.
try_items() { # name row chosen
    local name=$1 row=$2 chosen=$3 other was now
    read_row() { js "var r = document.querySelectorAll('.row-btn')[$row]; return r ? r.textContent : '';"; }
    other=$((chosen == 0 ? 1 : 0))
    was=$(read_row)
    click '.sheet .sheet-item' "$other" 0.05
    gone .sheet 3 || { panel_problem "$name: stays open after a choice"; press_back; gone .sheet 3; }
    now=$(read_row)
    [ "$now" != "$was" ] || panel_problem "$name: the row says the same after another choice"
    click .row-btn "$row" 0.05
    on_screen .sheet 4 || return 0
    click '.sheet .sheet-item' "$chosen" 0.05
    gone .sheet 3 || press_back
    now=$(read_row)
    [ "$now" = "$was" ] || panel_problem "$name: does not go back to what it said: $now"
}

# A panel that opened: all of it, and what closes it closes it and nothing else: Done from a row,
# Back from anywhere else and for a list. The list of challenges has its own part. The names used before fill in a field, and are no choice between them.
panel_open() { # name [row]
    local name=$1 row=${2:-} chosen
    if ! on_screen .sheet 4; then
        panel_problem "$name: no panel"
        return 1
    fi
    look_through "$name"
    if ! is_there '.challenge-list, .sheet input[type=text]'; then
        try_chips "$name"
        try_switches "$name"
    fi
    if [ -n "$row" ] && is_there .sheet-done; then
        click .sheet-done 0 0.05
        gone .sheet 3 || {
            panel_problem "$name: Done leaves it open"
            press_back
        }
        return 0
    fi
    chosen=$(js "return [...document.querySelectorAll('.sheet .sheet-item')].findIndex(e => e.classList.contains('selected'));")
    press_back
    gone .sheet 3 || {
        panel_problem "$name: Back leaves it open"
        press_back
    }
    [ -n "$row" ] && [ "${chosen:--1}" -ge 0 ] 2> /dev/null || return 0
    click .row-btn "$row" 0.05
    on_screen .sheet 4 || { panel_problem "$name: does not open again"; return 1; }
    try_items "$name" "$row" "$chosen"
}

# Every row of the list on screen. One that opens a panel or a screen is looked through and left,
# one with a switch goes over and back. The row for a widget is for s40: it asks the launcher.
panel_rows() { # name
    local name=$1 n i kind title one two before
    n=$(js "return document.querySelectorAll('.row-btn').length;")
    i=0
    while [ "$i" -lt "${n:-0}" ] 2> /dev/null; do
        IFS='|' read -r kind title before <<< "$(page "var e = document.querySelectorAll('.row-btn')[$i], t = e.querySelector('.row-title');
            return (e.getAttribute('role') === 'switch' ? 'switch' : 'row') + '|' + (t ? t.textContent : '').toLowerCase().replace(/[^a-z0-9]+/g, '_').slice(0, 24) + '|' + location.pathname + ' ' + document.querySelectorAll('.row-btn').length;")"
        case "$title" in
            *widget*) ;;
            *)
                if [ "$kind" = switch ]; then
                    one=$(js "var e = document.querySelectorAll('.row-btn')[$i]; var v = e.getAttribute('aria-checked'); e.click(); return v;")
                    sleep 0.05
                    two=$(js "var e = document.querySelectorAll('.row-btn')[$i]; var v = e.getAttribute('aria-checked'); e.click(); return v;")
                    sleep 0.05
                    [ "$one" != "$two" ] || panel_problem "$name: the switch $title does not switch"
                else
                    click .row-btn "$i" 0.1
                    if wait_for 1 is_there .sheet; then
                        panel_open "${name}_$title" "$i"
                    elif [ "$(js "return location.pathname + ' ' + document.querySelectorAll('.row-btn').length;")" != "$before" ]; then
                        look_through "${name}_$title"
                        press_back
                        on_screen .row-btn 4 || panel_problem "$name: not back from $title"
                    else
                        panel_problem "$name: the row $title does nothing"
                    fi
                fi
                ;;
        esac
        i=$((i + 1))
    done
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
        af31) look="Light German H12 1.3" ;;
        phone) look="Dark Dutch H24 1.0" ;;
        *) look="Dark English H24 1.0" ;;
    esac
    [ -z "${VARIANT:-}" ] || look="Black English H12 1.0"
    read -r PANEL_THEME PANEL_LANGUAGE PANEL_HOURS PANEL_FONT <<< "$look"
}
# Without animations: there is nothing to wait for, and no picture of something half way.
panel_settings() { use_settings "\"$PANEL_THEME\"" "\"$PANEL_LANGUAGE\"" "\"setup_done\":$1,\"language_follows_phone\":false,\"less_motion\":true,\"time_format\":\"$PANEL_HOURS\""; }

# The challenge sheet of the editor on screen: each challenge's settings, adding one, and each as
# it is when it has to be done.
panel_challenges() {
    local n i found
    n=$(js "return document.querySelectorAll('.challenge-open').length;")
    i=0
    while [ "$i" -lt "${n:-0}" ] 2> /dev/null; do
        click .challenge-open "$i" 0.05
        on_screen .sheet-done 3
        look_through "challenge_settings$i"
        try_chips "challenge_settings$i"
        press_back
        on_screen .challenge-row 4 || panel_problem "not back from the settings of challenge $i"
        i=$((i + 1))
    done
    click '.sheet .btn-quiet' 0 0.05
    if on_screen .add-option 4; then
        [ "$(js "return document.querySelectorAll('.add-option').length;")" = 8 ] || panel_problem "not all challenges can be added"
        look_through challenge_add
        press_back
    else
        panel_problem "no way to add a challenge"
    fi
    on_screen .challenge-row 4 || panel_problem "not back from adding a challenge"
    i=0
    while [ "$i" -lt "${n:-0}" ] 2> /dev/null; do
        click '.challenge-row .link-btn' "$i" 0.05
        if on_screen .run-screen 5; then
            sleep 0.2
            # A permission it asks for, on a phone that has not given it.
            focus_in_app || { press_back; sleep 0.5; }
            found=$(panel_check true)
            [ -z "$found" ] || panel_problem "challenge $i: $found"
            picture "challenge_run$i"
            if is_there .run-fallback; then
                click .run-fallback 0 0.05
                if on_screen .sheet 3; then
                    [ "$i" -gt 0 ] || look_through challenge_other_way
                    click .sheet-done 0 0.05
                    gone .sheet 3 || panel_problem "challenge $i: Cancel leaves it open"
                else
                    panel_problem "challenge $i: the other way does nothing"
                fi
            fi
            click '.run-top .link-btn' 0 0.05
        else
            panel_problem "challenge $i cannot be tried"
        fi
        on_screen .challenge-row 5 || panel_problem "not back from trying challenge $i"
        i=$((i + 1))
    done
}

s57() {
    scenario s57_every_panel
    panel_look
    PANEL_N=0
    PANEL_PROBLEMS=""
    mkdir -p "$OUT/panels${VARIANT:-}"
    rm -f "$OUT/panels${VARIANT:-}"/*
    local i n old_font found
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
    on_screen .setup-header 15 || panel_problem "no first-run screen"
    look_through setup
    dev am force-stop "$PKG"

    # Nothing yet.
    fresh
    panel_settings true
    start_app
    on_screen .empty 15 || panel_problem "no empty list"
    look_through home_empty
    click .nav-item 1
    on_screen .screen-title-large
    look_through history_empty
    dev am force-stop "$PKG"

    # The list, and everything from it.
    fresh
    panel_settings true
    panel_alarms
    panel_history_rows
    start_app
    on_screen .alarm-card 15 || panel_problem "no list"
    # What the phone says about itself takes a moment to show.
    sleep 1
    look_through home
    click .round-icon-btn 0 0.05
    panel_open home_pause
    click .round-icon-btn 0 0.05
    if on_screen .sheet 4; then
        js "var i = document.querySelectorAll('.sheet .sheet-item'); i[i.length - 1].click(); return 'ok';" > /dev/null
        sleep 0.05
        look_through home_pause_date
        press_back
        gone .sheet 3 || press_back
    fi
    click .round-icon-btn 1 0.05
    panel_open home_sort
    n=$(js "return document.querySelectorAll('.alarm-menu').length;")
    i=0
    while [ "$i" -lt "${n:-0}" ] 2> /dev/null; do
        click .alarm-menu "$i" 0.05
        panel_open "home_menu$i"
        i=$((i + 1))
    done

    # A quick alarm, and what it says.
    click .chip-slim 0 0.05
    on_screen .toast-text 4 || panel_problem "a quick alarm says nothing"
    [ "$(js "var e = document.querySelector('.toast-undo'); return !!e && e.scrollWidth <= e.clientWidth + 1;")" = true ] ||
        panel_problem "the button on a message is cut off"
    picture home_quick_alarm
    click .toast-undo 0 0.1

    # The editor of a new alarm.
    click .fab 0 0.05
    on_screen .time-big 5 || panel_problem "no editor for a new alarm"
    look_through editor_new
    js "var b = document.querySelectorAll('.repeat-presets button'); b[b.length - 1].click(); return 'ok';" > /dev/null
    panel_open editor_date
    press_back
    on_screen '.sheet, .alarm-card' 4
    if is_there .sheet; then
        look_through editor_discard
        click '.sheet-item.danger' 0 0.05
    fi
    on_screen .alarm-card 5 || panel_problem "not back on the list from a new alarm"

    # The editor of an alarm with everything on it.
    n=$(js "return [...document.querySelectorAll('.alarm-label')].findIndex(e => e.textContent === 'Work');")
    click .alarm-main "$n" 0.05
    on_screen .time-big 5 || panel_problem "no editor"
    look_through editor
    # The small steps either side of the time: a minute later, and back.
    TB_BEFORE=$(js "return document.querySelector('.time-big').textContent;")
    click .time-shift 2 0.05
    TB_AFTER=$(js "return document.querySelector('.time-big').textContent;")
    click .time-shift 0 0.05
    [ "$TB_BEFORE" != "$TB_AFTER" ] && [ "$(js "return document.querySelector('.time-big').textContent;")" = "$TB_BEFORE" ] ||
        panel_problem "the minute buttons do not move the time a minute and back"
    click .time-big 0 0.05
    panel_open editor_time
    click '.screen-header .icon-btn' 1 0.05
    panel_open editor_delete
    panel_rows editor
    # The fifth row.
    click .row-btn 4 0.05
    if on_screen .challenge-row 4; then
        # The list, Add, Try all and the order: nothing else to choose from.
        [ "$(js "return document.querySelectorAll('.sheet .chip').length;")" = 2 ] ||
            panel_problem "the challenges sheet has more to choose than the order"
        panel_challenges
        press_back
        gone .sheet 3 || panel_problem "Back leaves the challenges open"
    else
        panel_problem "the fifth row is not the challenges"
    fi
    press_back
    on_screen '.sheet, .alarm-card' 4
    if is_there .sheet; then
        look_through editor_discard_changes
        click '.sheet-item.danger' 0 0.05
    fi
    on_screen .alarm-card 5 || panel_problem "not back on the list from the editor"

    # History.
    click .nav-item 1
    on_screen .screen-title-large 5
    look_through history
    click .icon-btn 0 0.05
    panel_open history_delete
    # A period asks first too, and Back is no.
    click .icon-btn 0 0.05
    on_screen .sheet 3
    click '.sheet .sheet-item' 0 0.05
    if on_screen '.sheet .sheet-item.danger' 3; then
        look_through history_delete_period
        press_back
        gone .sheet 3 || panel_problem "history_delete_period: Back leaves it open"
    else
        panel_problem "deleting a period does not ask"
    fi
    click .summary-toggle 0 0.05
    look_through history_all_stats
    click .summary-toggle 0 0.05
    click .event-main 0 0.05
    look_through history_event
    # Deleting one entry asks first; Back is no, and the answer takes exactly one away.
    HIST_N=$(js "return document.querySelectorAll('.event-main').length;")
    click '.event-details .link-btn' 0 0.05
    if on_screen '.sheet .sheet-item.danger' 3; then
        look_through history_delete_one
        press_back
        gone .sheet 3 || panel_problem "history_delete_one: Back leaves it open"
        [ "$(js "return document.querySelectorAll('.event-main').length;")" = "$HIST_N" ] ||
            panel_problem "Back on the question deleted the entry"
        click '.event-details .link-btn' 0 0.05
        on_screen '.sheet .sheet-item.danger' 3
        click '.sheet-item.danger' 0 0.1
        [ "$(js "return document.querySelectorAll('.event-main').length;")" = "$((HIST_N - 1))" ] ||
            panel_problem "deleting one entry did not take exactly one away"
    else
        panel_problem "deleting one entry does not ask"
    fi
    n=$(js "return document.querySelectorAll('.chip-pill').length;")
    i=0
    while [ "$i" -lt "${n:-0}" ] 2> /dev/null; do
        click .chip-pill "$i" 0.1
        [ "$(js "return document.querySelectorAll('.chip-pill')[$i].getAttribute('aria-pressed');")" != false ] ||
            panel_problem "history filter $i does not take"
        found=$(panel_check false)
        [ -z "$found" ] || panel_problem "history filter $i: $found"
        picture "history_filter$i"
        i=$((i + 1))
    done

    # Settings, and what is behind them.
    click .nav-item 2
    on_screen .row-btn 5
    look_through settings
    panel_rows settings
    click .banner 0 0.05
    if on_screen .check-title 5; then
        look_through reliability
        press_back
        on_screen .row-btn 5 || panel_problem "not back from reliability"
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
    if await_ring a 70 && on_screen .ring-off 15; then
        # As soon as it is there: a banner over it would be gone after some seconds.
        sleep 0.5
        found=$(panel_check true)
        [ -z "$found" ] || panel_problem "ring: $found"
        picture ring
        click .ring-off 0 0.05
        on_screen .math-expression 5 || panel_problem "no challenge on the alarm"
        found=$(panel_check true)
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
