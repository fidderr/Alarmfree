#!/bin/bash
# The scenarios for AlarmFree, run against one device over adb.
#
#   SERIAL=emulator-5554 devtest/matrix.sh            every scenario
#   SERIAL=emulator-5554 devtest/matrix.sh s03 s07    just these
#
# Each judges only what can be seen from outside: is the sound on, what state is the alarm in,
# is it armed, what is on the screen, what is saved. All of it should take under 15 minutes on
# an emulator and no scenario over a minute: the summary names the ones that do. So an emulator's
# clock moves instead of a scenario waiting for the time to pass, what can be asked is asked
# until it is true, and what can happen during the same ring does.

cd "$(dirname "$0")/.." || exit 1
. devtest/lib.sh
APK="${APK:-target/dx/alarmfree/debug/android/app/app/build/outputs/apk/debug/app-debug.apk}"

# ---- Rings with nothing running ----

# A repeating alarm rings with no app process alive and the screen off: at once, once, and with
# its fade.
s01() {
    scenario s01_repeating_dead
    fresh
    next_minute
    seed a "$(alarm_json a "$T_H" "$T_M" "$EVERY_DAY" null air 0 30)"
    arm a
    dead
    screen_off
    await_sound 30 || { fail "no sound in 30s, state=$(state a) job=$(job_status a)"; return; }
    await_state a firing 15 || { fail "sound but state=$(state a)"; return; }
    local plays
    plays=$(adb_ logcat -d -s MobileSentinel.FiringAudio:I | tr -d '\r' | grep 'playing ')
    if [ "$(echo "$plays" | grep -c 'playing ')" != 1 ] || ! echo "$plays" | grep -q 'fadeIn=30000ms'; then
        fail "playbacks: $plays"
        return
    fi
    action dismiss:a
    await_state a scheduled 5 || { fail "after dismissing: $(state a)"; return; }
    pass "sound $(ring_ms)ms after delivery, $(sound_after_t_ms)ms after its time, one playback with its fade, next ring in $((($(next_fire_ms a) / 1000 - $(now_s)) / 60)) min"
}

# A snooze runs out with the process dead (the Snooze button always ends it).
s03() {
    scenario s03_snooze_expires
    fresh
    next_minute
    seed a "$(alarm_json a "$T_H" "$T_M" "$EVERY_DAY" null)"
    arm a
    await_ring a 30 || { fail "first ring never started"; return; }
    action snooze:a
    await_state a snoozed 5 || note "state after snooze: $(state a)"
    wait_for 3 sound_off || fail "still sounding after snooze"
    local until
    until=$(($(next_fire_ms a) / 1000))
    jump_to $((until - 4))
    sleep 1.5
    kill_all
    T=$until
    if await_sound 30; then
        pass "rang again, ${SOUND_AT}s after the snooze ended"
    else
        fail "no ring again in 30s, state=$(state a) job=$(job_status a)"
    fi
}

# A snoozed one-time alarm survives the app being opened, and its card says so.
s04() {
    scenario s04_one_time_snooze_kept
    fresh
    next_minute
    seed a "$(alarm_json a "$T_H" "$T_M" "$NO_DAYS" "\"$T_DATE\"" air 0 0 3 5)"
    arm a
    await_ring a 30 || { fail "first ring never started"; return; }
    action snooze:a
    await_state a snoozed 5 || note "state after snooze: $(state a)"
    local before after card
    before=$(next_fire_ms a)
    kill_all
    wait_for 3 no_process
    start_app
    on_screen .alarm-card 10
    after=$(next_fire_ms a)
    says_snoozed() {
        card=$(read_text .alarm-when)
        [[ "$card" == Snoozed* ]]
    }
    wait_for 5 says_snoozed
    if [ "$(state a)" = snoozed ] && [ "$before" = "$after" ] && [[ "$card" == Snoozed* ]]; then
        pass "snooze kept after reopening, the card says '$card'"
    else
        fail "state=$(state a) next ring moved by $(((${after:-0} - ${before:-0}) / 60000))min, db_enabled=$(db_enabled a), card '$card'"
    fi
}

# An expired one-time alarm switched back on stays on across an app restart.
s05() {
    scenario s05_toggled_back_on
    fresh
    next_minute 3600
    # The row as the home toggle leaves it: enabled, dated to its next occurrence.
    seed a "$(alarm_json a "$T_H" "$T_M" "$NO_DAYS" "\"$T_DATE\"")"
    arm a
    dev am force-stop "$PKG"
    start_app
    on_screen .home-header 10
    sleep 0.3
    if [ "$(db_enabled a)" = true ] && [ "$(state a)" = scheduled ]; then
        pass "still on and armed"
    else
        fail "db_enabled=$(db_enabled a) state=$(state a)"
    fi
}

# A repeating and a one-time alarm in the same minute, nothing running: both ring (a one-time
# alarm used to be switched off unheard), and dismissing one leaves the other ringing.
s06() {
    scenario s06_two_alarms
    fresh
    next_minute
    seed a "$(alarm_json a "$T_H" "$T_M" "$EVERY_DAY" null)"
    seed b "$(alarm_json b "$T_H" "$T_M" "$NO_DAYS" "\"$T_DATE\"")"
    arm a
    dead
    screen_off
    await_sound 30 || { fail "no sound in 30s, a=$(state a) b=$(state b) job=$(job_status b)"; return; }
    wait_for 15 state_is a firing
    local first=a second=b
    state_is a firing || { first=b; second=a; }
    note "states: a=$(state a) b=$(state b)"
    action "dismiss:$first"
    wait_for 5 state_is "$second" firing
    sleep 1
    if [ "$(state $second)" = firing ] && sound_on; then
        pass "both rang, the second still ringing after dismissing $first"
    else
        fail "after dismissing $first: $second state=$(state $second) sound=$(sound_on && echo on || echo OFF)"
    fi
}

# Killed while ringing, all three ways during one ring: the sound and the screen come back.
back_after_kill() { # name kill_fn
    scenario "$1"
    "$2"
    local t0
    t0=$SECONDS
    sleep 0.2
    up_again() { sound_on && [ -n "$(main_pid)" ] && top_is_app; }
    if ! wait_for 30 up_again; then
        fail "after 30s sound=$(sound_on && echo on || echo OFF) state=$(state a) main=$(main_pid) on top: $(top_is_app && echo yes || echo NO)"
    elif ! await_state a firing 10; then
        fail "state=$(state a)"
    else
        pass "back after $((SECONDS - t0))s"
    fi
}
s07() {
    scenario s07_ring
    fresh
    next_minute
    seed a "$(alarm_json a "$T_H" "$T_M" "$EVERY_DAY" null)"
    arm a
    await_ring a 30 || { fail "never started"; return; }
    back_after_kill s07a_kill_main_mid_ring kill_main
    back_after_kill s07b_kill_sentinel_mid_ring kill_sentinel
    back_after_kill s07c_kill_both_mid_ring kill_all
}

# Notifications denied: the sound must still play. Denying the overlay is in s26.
s10() {
    scenario s10_notifications_denied
    fresh
    next_minute
    seed a "$(alarm_json a "$T_H" "$T_M" "$EVERY_DAY" null)"
    if ! deny_notifications; then
        skip "this device does not let the shell deny notifications"
        return
    fi
    arm a
    dead
    screen_off
    if await_sound 30; then
        pass "rang without notifications, ${SOUND_AT}s"
    else
        fail "no sound in 30s, state=$(state a)"
    fi
}

# ---- A restart ----

# All that can happen to an alarm in a restart, in one restart. It boots with an alarm in the
# middle of its ring, one snoozed, one that came due during the restart, and one long overdue.
# None of it needs the app opened.
s11() {
    scenario s11a_reboot_mid_ring
    fresh
    next_minute "$LEAD" 100
    local a_h a_m later=$(((T_H + 3) % 24))
    read -r a_h a_m <<< "$(at_hm $((T + 60)))"
    seed b "$(alarm_json b "$T_H" "$T_M" "$EVERY_DAY" null air 0 0 3 4)"
    seed a "$(alarm_json a "$a_h" "$a_m" "$EVERY_DAY" null)"
    seed c "$(alarm_json c "$later" 0 "$EVERY_DAY" null)"
    seed d "$(alarm_json d "$later" 30 "$EVERY_DAY" null)"
    arm b
    wait_file files/sentinel/armed/d.armed 5
    await_ring b 30 || { fail "b never started"; return; }
    action snooze:b
    await_state b snoozed 5 || { fail "b did not snooze: $(state b)"; return; }
    local until_b
    until_b=$(next_fire_ms b)
    T=$((T + 60))
    jump_to $((T - 3))
    await_ring a 30 || { fail "a never started"; return; }
    make_overdue c 2
    make_overdue d 30
    reboot_and_wait || { fail "reboot hung"; return; }

    scenario s12_passed_during_restart
    T=$(now_s)
    if await_sound 60; then
        pass "rang late after the restart, ${SOUND_AT}s after the boot"
    else
        fail "no late ring, armed '$(armed_at_ms c)' os_alarm=$(os_has_alarm && echo yes || echo NO), state=$(state c)"
    fi

    scenario s12b_missed_during_restart
    one_missed() { [ "$(outcomes missed)" = 1 ]; }
    wait_for 30 one_missed
    local next
    next=$(armed_at_ms d)
    if [ "$(outcomes missed)" = 1 ] && [ "$(state d)" = scheduled ] && [ "${next:-0}" -gt "$(($(now_s) * 1000))" ]; then
        pass "recorded as missed, not rung, next ring armed"
    else
        fail "missed=$(outcomes missed) state=$(state d) armed='$next'"
    fi

    await_state c firing 15
    action dismiss:c
    await_state c scheduled 5

    scenario s11a_reboot_mid_ring
    wait_for 3 sound_off
    if ! sound_on && [ "$(state a)" = scheduled ]; then
        pass "ended by the reboot, next ring armed"
    else
        fail "sound=$(sound_on && echo on || echo off) state=$(state a)"
    fi

    scenario s11b_reboot_snooze_ahead
    if [ "$(state b)" != snoozed ] || [ "$(next_fire_ms b)" != "$until_b" ]; then
        fail "state=$(state b), next ring moved from $until_b to $(next_fire_ms b)"
        return
    fi
    T=$((until_b / 1000))
    jump_to $((T - 3))
    if await_sound 30; then
        pass "the snooze rang after the reboot"
    else
        fail "no ring, state=$(state b)"
    fi
}

# Rebooted and never unlocked: the alarm rings, and the backup screen is the way out, because
# the app's own screen cannot come up yet. Sets a PIN, so never on a phone somebody uses.
s13() {
    if [ -n "${NO_LOCK:-}" ]; then
        note "s13 sets a PIN: not here"
        return
    fi
    scenario s13_before_first_unlock
    fresh
    shorten backup_holds 1
    on_emulator && set_clock "$(date +%s)"
    next_minute 3600
    seed a "$(alarm_json a "$T_H" "$T_M" "$EVERY_DAY" null)"
    arm a
    dev locksettings set-pin 1234 > /dev/null
    dead
    # Now and not during the restart's own wait: a clock change that wakes the app up meanwhile
    # would find it overdue and ring it, and the restart ends a ring as dismissed.
    stopped_state_saved
    note "before the restart: main=[$(main_pid)] sentinel=[$(sentinel_pid)] state=$(state a)"
    dead
    make_overdue a 2
    reboot_and_wait || { fail "reboot hung"; return; }
    local locked
    locked=$(dev dumpsys user | grep -c 'RUNNING_LOCKED')
    T=$(now_s)
    if [ "$locked" = 0 ]; then
        fail "the phone is not locked after the restart"
    elif ! await_sound 60; then
        fail "no sound in 60s, locked users: $locked, state=$(state a) job=$(job_status a) main=$(main_pid) $(dev dumpsys package "$PKG" | grep -m1 -oE 'stopped=[a-z]+')"
    elif ! wait_for 15 top_is_backup; then
        screenshot s13_backup_screen
        fail "rang, but the backup screen is not on top: $(dev dumpsys activity activities | grep -m1 ResumedActivity)"
    else
        screenshot s13_backup_screen
        solve_backup 1
        if wait_for 5 sound_off; then
            pass "rang ${SOUND_AT}s after the boot while locked (locked users: $locked), stopped by the backup screen"
        else
            fail "still ringing after the challenge, top: $(dev dumpsys activity activities | grep -m1 ResumedActivity)"
        fi
    fi
    dev locksettings clear --old 1234 > /dev/null
    SCREEN_WENT_OFF=1
}

# A time-zone change re-arms for the new local time, no app opened.
s14() {
    scenario s14_time_zone_change
    fresh
    next_minute 7200
    seed a "$(alarm_json a "$T_H" "$T_M" "$EVERY_DAY" null)"
    arm a
    dead
    local before after zone
    before=$(armed_at_ms a)
    zone=$(dev getprop persist.sys.timezone)
    set_zone Asia/Tokyo
    moved() { [ "$(armed_at_ms a)" != "$before" ]; }
    wait_for 40 moved
    after=$(armed_at_ms a)
    if [ -n "$before" ] && [ "$before" != "$after" ]; then
        pass "re-armed, moved by $(((after - before) / 60000))min"
    else
        fail "armed time unchanged after zone change ($zone to Asia/Tokyo), main=$(main_pid)"
    fi
    set_zone "$zone"
}

# The app comes down for an update with an alarm armed: it still rings.
s15() {
    scenario s15_app_update
    fresh
    next_minute 20
    seed a "$(alarm_json a "$T_H" "$T_M" "$EVERY_DAY" null)"
    arm a
    local said
    said=$(adb_ install -r -g "$APK" 2>&1)
    case "$said" in
        *Success*) ;;
        *)
            fail "the update did not install: $said"
            return
            ;;
    esac
    if await_sound 60; then
        pass "rang after the update, ${SOUND_AT}s after its time"
    else
        fail "no sound in 60s, state=$(state a) job=$(job_status a)"
    fi
}

# A force-stop removes the alarm, so it passes unnoticed. Opening the app less
# than 15 minutes late rings it.
s16() {
    scenario s16_force_stop
    fresh
    next_minute
    seed a "$(alarm_json a "$T_H" "$T_M" "$EVERY_DAY" null)"
    arm a
    dev am force-stop "$PKG"
    jump_to $((T + 15))
    until_past $((T + 15))
    sound_on && note "rang despite force-stop"
    start_app
    ringing_a() { sound_on && state_is a firing; }
    if wait_for 15 ringing_a; then
        pass "rang late on opening"
    else
        fail "sound=$(sound_on && echo on || echo OFF) state=$(state a)"
    fi
}

# Opening the app long after a force-stop removed the alarm: a miss, explained.
s16b() {
    scenario s16b_force_stop_long_ago
    fresh
    next_minute 3600
    seed a "$(alarm_json a "$T_H" "$T_M" "$EVERY_DAY" null)"
    arm a
    dev am force-stop "$PKG"
    make_overdue a 30
    start_app
    told() { [ "$(outcomes missed)" = 1 ] && [ "$(unseen)" = 0 ] && state_is a scheduled; }
    if wait_for 12 told && ! sound_on; then
        screenshot s16b_notice
        pass "recorded as missed and told, next ring armed"
    else
        fail "sound=$(sound_on && echo ON || echo off) missed=$(outcomes missed) unseen=$(unseen) state=$(state a)"
    fi
}

# An unplayable custom sound must still make noise.
s17() {
    scenario s17_corrupt_sound
    fresh
    as_app "mkdir -p files/sounds/custom; echo not-audio > files/sounds/custom/bad.mp3"
    next_minute
    seed a "$(alarm_json a "$T_H" "$T_M" "$EVERY_DAY" null bad.mp3)"
    arm a
    dead
    if await_sound 30; then
        pass "fell back audibly, ${SOUND_AT}s"
    else
        fail "silent, state=$(state a)"
    fi
}

# ---- What a ring holds out against, all during one ring ----

# The volume keys, dark mode, turning and unfolding the phone, no touch for longer than the
# screen stays on, and an alarm without a limit past where another one would have stopped.
s19() {
    scenario s21_dark_mode_mid_ring
    fresh
    local old_timeout old_stay before p0 v0 v1 v2 problems=""
    old_timeout=$(dev settings get system screen_off_timeout)
    # An emulator stays awake for being plugged in, ringing or not.
    old_stay=$(dev settings get global stay_on_while_plugged_in)
    dev settings put global stay_on_while_plugged_in 0
    dev settings put system screen_off_timeout 10000
    before=$(alarm_volume)
    set_alarm_volume 1
    next_minute
    seed a "$(alarm_json a "$T_H" "$T_M" "$EVERY_DAY" null air 0 0 3 1 '[]' null)"
    arm a
    if ! await_ring a 30; then
        fail "never started"
        set_alarm_volume "$before"
        dev settings put system screen_off_timeout "$old_timeout"
        dev settings put global stay_on_while_plugged_in "$old_stay"
        return
    fi
    on_screen .ring-off 10
    v0=$(alarm_volume)
    adb_ shell "input keyevent KEYCODE_VOLUME_DOWN; input keyevent KEYCODE_VOLUME_DOWN; input keyevent KEYCODE_VOLUME_DOWN" > /dev/null
    sleep 1.2
    v1=$(alarm_volume)

    p0=$(main_pid)
    dev cmd uimode night yes > /dev/null
    sleep 1.5
    [ "$(main_pid)" = "$p0" ] && pass "same process" || fail "main process restarted ($p0 to $(main_pid))"
    dev cmd uimode night no > /dev/null
    sleep 0.5

    scenario s35_rotate_and_unfold_mid_ring
    local size
    size=$(dev wm size | tail -1 | sed 's/.*: //')
    dev settings put system accelerometer_rotation 0
    dev settings put system user_rotation 1
    sleep 1.5
    screenshot s35_rotated
    [ "$(main_pid)" = "$p0" ] || problems="$problems; rotating ended the process"
    sound_on || problems="$problems; silent after rotating"
    on_screen .ring-off || problems="$problems; no alarm screen after rotating"
    # The screen is set to go off after 10 s: a ring has to hold it.
    [ "$(wakefulness)" = Awake ] || problems="$problems; the screen went to sleep while ringing"
    dev settings put system user_rotation 0
    # Unfolding, as a screen size.
    dev wm size 2200x2480
    sleep 1.5
    screenshot s35_unfolded
    [ "$(main_pid)" = "$p0" ] || problems="$problems; resizing ended the process"
    sound_on || problems="$problems; silent after resizing"
    on_screen .ring-off || problems="$problems; no alarm screen after resizing"
    dev wm size "$size"
    dev settings put system accelerometer_rotation 1
    sleep 0.5
    if [ -z "$problems" ]; then
        pass "same process, still ringing ($size and 2200x2480)"
    else
        fail "${problems#; }"
    fi

    scenario s20_screen_stays_on
    until_past $((T + 13))
    local w
    w=$(wakefulness)
    [ "$w" = Awake ] && pass "screen awake $(($(now_s) - T))s in, with the screen set to go off after 10s" || fail "wakefulness=$w"
    dev settings put system screen_off_timeout "$old_timeout"
    dev settings put global stay_on_while_plugged_in "$old_stay"

    scenario s23d_limit_forever
    sound_on && pass "no limit: still ringing $(($(now_s) - T))s in" || fail "stopped, state=$(state a)"

    scenario s19_volume_keys
    action dismiss:a
    wait_for 5 sound_off
    sleep 0.3
    v2=$(alarm_volume)
    set_alarm_volume "$before"
    if [ "${v0:-0}" -gt 1 ] && [ "${v1:-0}" -ge "$v0" ] && [ "$v2" = 1 ]; then
        pass "raised from 1 to $v0, held at $v1 against the keys, back to $v2 after"
    else
        fail "raised to $v0, after the keys $v1, after dismissing $v2 (should be 1)"
    fi
}

# The UI keeps freezing. The guardian restarts it and the alarm carries on, and after two
# failures the backup screen takes over.
s22() {
    scenario s22_hung_ui
    fresh
    shorten ui_timeout_ms 4000
    shorten backup_holds 1
    next_minute
    seed a "$(alarm_json a "$T_H" "$T_M" "$EVERY_DAY" null)"
    arm a
    await_ring a 30 || { fail "never started"; return; }
    on_screen .ring-off 10
    local p0 t0
    new_main() { local p; p=$(main_pid); [ -n "$p" ] && [ "$p" != "$p0" ]; }
    p0=$(main_pid)
    t0=$SECONDS
    action hang_ui
    if wait_for 25 new_main; then
        sound_on && pass "restarted after $((SECONDS - t0))s, still ringing" || fail "restarted but silent"
    else
        fail "UI never restarted in 25s"
    fi

    scenario s22b_ui_keeps_failing
    on_screen .ring-off 10
    p0=$(main_pid)
    action hang_ui
    wait_for 30 new_main
    if wait_for 15 top_is_backup; then
        screenshot s22b_backup
        solve_backup 1
        ended() { sound_off && state_is a scheduled && [ "$(outcomes backup)" = 1 ]; }
        if wait_for 10 ended; then
            pass "backup screen took over, solving it ended the alarm"
        else
            fail "after solving: sound=$(sound_on && echo ON || echo off) state=$(state a) backup=$(outcomes backup)"
        fi
    else
        fail "backup screen not on top: $(dev dumpsys activity activities | grep -m1 ResumedActivity)"
    fi
}

# The ring limit, with a short one. No touch: it stops, and the alarm moves on.
s23a() {
    scenario s23a_limit_stops
    fresh
    next_minute
    seed a "$(alarm_json a "$T_H" "$T_M" "$EVERY_DAY" null air 0 0 3 1 '[]' 6)"
    arm a
    await_sound 30 || { fail "never started"; return; }
    if ! await_silence 40; then
        fail "still ringing after 40s"
        return
    fi
    moved_on() { state_is a scheduled && [ "$(job_status a)" = pending ]; }
    if wait_for 8 moved_on && [ "$STOPPED_AT" -ge 5 ]; then
        screenshot s23a_after_limit
        pass "stopped ${STOPPED_AT}s after its time, next ring armed"
    else
        fail "stopped at ${STOPPED_AT}s, state=$(state a) job=$(job_status a)"
    fi
}

# A touch restarts the limit. The app coming up counts as activity too, and a touch within
# 30 s of the last one is not looked at: that one is made older here instead of waiting.
s23b() {
    scenario s23b_limit_restarts_on_touch
    fresh
    next_minute
    seed a "$(alarm_json a "$T_H" "$T_M" "$EVERY_DAY" null air 0 0 3 1 '[]' 9)"
    arm a
    await_sound 30 || { fail "never started"; return; }
    on_screen .ring-off 10
    sleep 0.7
    as_app "touch -d 2020-01-01T00:00:00 files/sentinel/ringing/activity"
    # Late enough to tell: it ends at 9 seconds without this touch, and at 13 with it.
    until_past $((T + 4))
    local size
    size=$(dev wm size | tail -1 | sed 's/.*: //')
    # On the alarm's clock, where a touch does nothing else.
    dev input tap $((${size%x*} / 2)) $((${size#*x} / 5))
    until_past $((T + 11))
    if ! sound_on; then
        fail "stopped despite the touch, state=$(state a)"
    elif await_silence 20 && [ "$STOPPED_AT" -ge 12 ]; then
        pass "past its limit of 9 at 11s, stopped ${STOPPED_AT}s after its time"
    else
        fail "the limit no longer counts after the touch: stopped at ${STOPPED_AT:-never}"
    fi
}

# The screen cannot come up (phone in use, no overlay permission). The sound plays and the
# alarm moves on with no UI: the next occurrence gets armed. Then the limit stops it, and the
# app deals with that when it can.
s23c() {
    scenario s27_state_machine_without_ui
    fresh
    next_minute
    seed a "$(alarm_json a "$T_H" "$T_M" "$EVERY_DAY" null air 0 0 3 1 '[]' 8)"
    seed b "$(alarm_json b "$T_H" "$T_M" "$NO_DAYS" "\"$T_DATE\"" air 0 0 3 1 '[]' 8)"
    arm b
    wait_file files/sentinel/armed/a.armed 5
    deny_overlay
    dead
    wake
    go_home
    await_sound 30 || { fail "no sound"; return; }
    await_state a firing 12
    # The next ring is armed a moment after the state is written.
    local next
    next_ahead() { next=$(armed_at_ms a); [ "${next:-0}" -gt "$(($(now_s) * 1000))" ]; }
    wait_for 10 next_ahead
    screenshot s27_heads_up
    # A busy phone can hold the word to the app back for ten seconds, past this short ring's
    # limit: the ring is over by the time the state machine runs, which then still arms the next.
    case "$(state a)" in
        firing | scheduled)
            if [ "${next:-0}" -gt "$(($(now_s) * 1000))" ]; then
                pass "$(state a) and next ring armed, UI on top: $(top_is_app && echo yes || echo no)"
            else
                fail "state=$(state a) armed='$next' main=$(main_pid)"
            fi
            ;;
        *) fail "state=$(state a) armed='$next' main=$(main_pid)" ;;
    esac

    scenario s23c_limit_app_away
    if ! await_silence 40; then
        fail "still ringing after 40s"
        return
    fi
    grant_all
    start_app
    settled() { [ "$(job_status b)" != active ] && [ "$(db_enabled b)" = false ]; }
    if wait_for 10 settled && ! sound_on; then
        screenshot s23c_after_opening
        pass "stopped at ${STOPPED_AT}s, one-time alarm off after opening"
    else
        fail "after opening: state=$(state b) job=$(job_status b) enabled=$(db_enabled b)"
    fi
}

# Everything against it: no process, screen off, overlay denied, deep in Doze, and the second
# time also the phone's Restricted battery setting. The time from delivery to sound is what the
# plan promises to keep under 1 s. Doze needs the alarm over an hour away, so on an emulator the
# clock moves after: the app wakes up for that, and goes again. A phone's clock stays as it is.
s26() {
    local run=${1:-1} dozing=""
    scenario "s26_cold_ring_$run"
    fresh
    on_emulator && next_minute 7200 || next_minute
    seed a "$(alarm_json a "$T_H" "$T_M" "$EVERY_DAY" null)"
    arm a
    deny_overlay
    on_emulator && [ "$run" = 2 ] && dev appops set "$PKG" RUN_ANY_IN_BACKGROUND ignore
    dead
    screen_off
    if on_emulator; then
        force_idle
        jump_to $((T - LEAD))
        sleep 1.5
        dead
        dozing=", $(dev dumpsys deviceidle get deep | tr -d '\r') until then"
    fi
    if await_sound 30; then
        local ms delivery
        ms=$(sound_after_t_ms)
        delivery=$(ring_ms)
        # How late Android delivers it is not ours, and an emulator is busy: the sound is. A phone
        # that holds an alarm back is what this is for, though.
        if [ -n "$ms" ] && [ "$ms" -lt 3000 ] && [ "${delivery:-9999}" -lt 1000 ]; then
            pass "sound ${ms}ms after its time, ${delivery}ms after delivery$dozing"
        elif on_emulator && [ -n "$ms" ] && [ "${delivery:-9999}" -lt 1000 ]; then
            note "Android delivered it ${ms}ms after its time: the emulator was busy"
            pass "sound ${delivery}ms after delivery, which Android made ${ms}ms after its time$dozing"
        else
            fail "sound ${ms:-?}ms after its time, ${delivery:-?}ms after delivery"
        fi
    else
        fail "no sound in 30s, state=$(state a) job=$(job_status a)"
    fi
    dev dumpsys deviceidle unforce > /dev/null
}
s26x2() { for r in 1 2; do s26 "$r"; done; }

# ---- The challenges ----

# What sticks out of the screen, if anything.
NOTHING_CLIPPED='var w = innerWidth; var bad = [...document.querySelectorAll(".screen *")].filter(e => { var r = e.getBoundingClientRect(); return r.width > 0 && (r.right > w + 1 || r.left < -1) && !e.closest(".chip-scroll, .alarm-swipe"); }).map(e => e.className.baseVal === undefined ? e.className : "svg").slice(0, 3); return bad.join(",");'

# The sum on screen, answered with the keys.
solve_math() {
    local digit digits
    SUM=$(read_text .math-expression)
    ANSWER=$(($(echo "$SUM" | sed 's/×/*/; s/ //g')))
    digits="$ANSWER"
    while [ -n "$digits" ]; do
        digit=${digits:0:1}
        digits=${digits:1}
        js "[...document.querySelectorAll('.math-key')].find(k => k.textContent === '$digit').click(); return 'ok'" > /dev/null
    done
}

# In a challenge the sound is quiet. Doing nothing brings it back. Turning it off through the
# screens, solving a sum, and landing on history.
s28() {
    scenario s28b_idle_challenge_rings_again
    fresh
    shorten idle_secs 4
    next_minute
    seed a "$(alarm_json a "$T_H" "$T_M" "$EVERY_DAY" null air 0 0 3 1 '[["challenge.math","{\"difficulty\":\"easy\"}"]]')"
    arm a
    await_ring a 30 || { fail "never started"; return; }
    on_screen .ring-off 10
    click .ring-off 0 0.3
    wait_for 4 sound_off || { fail "still loud in the challenge"; return; }
    if wait_for 12 sound_on && on_screen .ring-off; then
        pass "ringing again after doing nothing"
    else
        fail "sound=$(sound_on && echo ON || echo off)"
        return
    fi

    scenario s28_solve_a_challenge
    shorten idle_secs 30
    screenshot s28_ringing
    local clipped
    clipped=$(ask_page "$NOTHING_CLIPPED")
    click .ring-off 0 0.3
    on_screen .math-expression
    screenshot s28_challenge
    clipped="$clipped$(js "$NOTHING_CLIPPED")"
    [ -z "$clipped" ] || { fail "clipped: $clipped"; return; }
    wait_for 4 sound_off
    solve_math
    solved() { state_is a scheduled && [ "$(outcomes solved)" = 1 ]; }
    if wait_for 8 solved && on_screen .summary 8 && ! sound_on; then
        screenshot s28_after
        pass "$SUM = $ANSWER turned it off, recorded, on history"
    else
        fail "sound=$(sound_on && echo ON || echo off) state=$(state a) solved=$(outcomes solved) sum='$sum'"
    fi
}

# The wake-up check: a little later it asks again, as loud as any alarm.
s29() {
    scenario s29_wake_up_check
    fresh
    next_minute
    ALARM_EXTRA=',"wake_check_minutes":1'
    seed a "$(alarm_json a "$T_H" "$T_M" "$EVERY_DAY" null air 0 0 0 1)"
    ALARM_EXTRA=
    arm a
    await_ring a 30 || { fail "never started"; return; }
    on_screen .ring-off 10
    click .ring-off 0 0.5
    waiting() { [ "$(db_count '"hidden":true')" = 1 ]; }
    wait_for 5 waiting || { fail "no check waiting: $(db_count '"hidden":true')"; return; }
    wait_for 3 sound_off || { fail "still ringing after turning it off"; return; }
    local id label
    id=$(printf "SELECT id FROM alarms WHERE data LIKE '%%\"hidden\":true%%';\n" | sql)
    wait_file "files/sentinel/context/$id.json" 5
    T=$(($(next_fire_ms "$id") / 1000))
    jump_to $((T - 3))
    await_sound 30 || { fail "the check never rang"; return; }
    on_screen .ring-off 10
    screenshot s29_check
    label=$(read_text .ring-label)
    click .ring-off 0 0.5
    finished() { sound_off && [ "$(db_count '"hidden":true')" = 0 ] && [ "$(outcomes solved)" = 1 ]; }
    if wait_for 8 finished && [ "$label" = "Are you up?" ]; then
        pass "rang again after ${SOUND_AT}s, and is gone after confirming"
    else
        fail "sound=$(sound_on && echo ON || echo off) hidden=$(db_count '"hidden":true') solved=$(outcomes solved) label='$label'"
    fi
}

# ---- The screens ----

# Back from the editor stays in the app.
s25() {
    scenario s25_back_navigation
    fresh
    next_minute 3600
    seed a "$(alarm_json a "$T_H" "$T_M" "$EVERY_DAY" null)"
    arm a
    on_screen .alarm-menu 10
    local p0 problems="" under
    p0=$(main_pid)
    # A sheet closes first, and the screen stays.
    click .alarm-menu
    on_screen .sheet || problems="$problems; no menu"
    press_back
    gone .sheet || problems="$problems; Back left the menu open"
    on_screen .home-header || problems="$problems; Back from the menu left Home"
    # Then the screen.
    click .alarm-main
    on_screen .time-big || problems="$problems; no editor"
    click .row-btn
    on_screen .sheet || problems="$problems; no editor sheet"
    press_back
    gone .sheet || problems="$problems; Back left the editor sheet open"
    on_screen .time-big || problems="$problems; Back from the sheet left the editor"
    press_back
    on_screen .home-header || problems="$problems; Back from the editor did not reach Home"
    # With changes it asks first.
    click .alarm-main
    on_screen .time-shift
    click .time-shift
    press_back
    on_screen .sheet || problems="$problems; no question about discarding"
    under=$(covered)
    [ -z "$under" ] || problems="$problems; in the question: $under"
    press_back
    gone .sheet || problems="$problems; Back from the question stayed"
    on_screen .time-big || problems="$problems; Back from the question left the editor"
    click .icon-btn
    on_screen .sheet-item
    click .sheet-item
    on_screen .home-header || problems="$problems; discarding did not reach Home"
    # The main screens do not stack up.
    click .nav-item 1
    click .nav-item 2
    on_screen .main-header || problems="$problems; no settings"
    press_back
    on_screen .home-header || problems="$problems; Back from settings did not reach Home"
    # And Home goes to the background, without ending anything.
    press_back
    sleep 0.3
    top_is_app && problems="$problems; Back on Home stayed on top"
    [ "$(main_pid)" = "$p0" ] || problems="$problems; main $p0 to $(main_pid)"
    start_app
    on_screen .home-header 20 || problems="$problems; not back on Home"
    if [ -z "$problems" ]; then
        pass "sheet, screen, Home, background, same process"
    else
        fail "${problems#; }"
    fi
}

# A nap in two taps, and Undo.
s30() {
    scenario s30_quick_alarm_and_undo
    fresh
    start_app
    on_screen .chip-slim 10
    click .chip-slim 0 0.5
    local made toast_text left
    made=$(db_count '"delete_after_ring":true')
    toast_text=$(read_text .toast-text)
    wait_for 3 os_has_alarm || { fail "not armed: made=$made"; return; }
    click .toast-undo 0 0.5
    left=$(db_count '"delete_after_ring":true')
    if [ "$made" = 1 ] && [ "$left" = 0 ]; then
        pass "made ($toast_text) and undone"
    else
        fail "made=$made left=$left"
    fi
}

# Pausing everything moves the next ring past the date, and Undo brings it back.
s31() {
    scenario s31_pause_all
    fresh
    next_minute 3600
    seed a "$(alarm_json a "$T_H" "$T_M" "$EVERY_DAY" null)"
    arm a
    on_screen .round-icon-btn 10
    local before during after text_paused week=$((6 * 24 * 3600 * 1000))
    before=$(armed_at_ms a)
    click .round-icon-btn 0
    on_screen .sheet-item
    click .sheet-item 1
    armed_moved() { [ "$(armed_at_ms a)" != "$1" ]; }
    wait_for 5 armed_moved "$before"
    during=$(armed_at_ms a)
    text_paused=$(read_text .alarm-when)
    click .toast-undo 0
    wait_for 5 armed_moved "$during"
    after=$(armed_at_ms a)
    if [ "$during" -ge "$((before + week))" ] && [ "$after" = "$before" ]; then
        pass "moved past the week ($text_paused), undone"
    else
        fail "armed: before $before, during $during, after $after"
    fi
}

# The test alarm can be called off, rings for real, leaves nothing behind, and its record can be saved.
s32() {
    scenario s32_test_alarm
    fresh
    start_app
    on_screen .nav-item 10
    click .nav-item 2
    on_screen .banner
    click .banner
    on_screen '.save-bar .btn-primary'
    click '.save-bar .btn-primary' 0 0.5
    test_waiting() { [ "$(db_count '"hidden":true')" = 1 ]; }
    wait_for 5 test_waiting || { fail "no test alarm"; return; }
    # It can be called off, and then nothing is armed. Set again for the ring.
    on_screen '.save-bar .btn-quiet' 3 || { fail "no way to call the test alarm off"; return; }
    click '.save-bar .btn-quiet' 0 0.5
    test_off() { [ "$(db_count '"hidden":true')" = 0 ] && [ -z "$(as_app 'ls files/sentinel/armed 2>/dev/null')" ]; }
    wait_for 5 test_off || { fail "the test alarm was not called off: hidden=$(db_count '"hidden":true') armed=$(as_app 'ls files/sentinel/armed 2>/dev/null' | tr '\n' ' ')"; return; }
    on_screen '.save-bar .btn-primary' 3 || { fail "no test button after calling one off"; return; }
    click '.save-bar .btn-primary' 0 0.5
    wait_for 5 test_waiting || { fail "no second test alarm"; return; }
    local id record=no
    id=$(printf "SELECT id FROM alarms WHERE data LIKE '%%\"hidden\":true%%';\n" | sql)
    wait_file "files/sentinel/context/$id.json" 5
    go_home
    T=$(($(next_fire_ms "$id") / 1000))
    jump_to $((T - 3))
    await_sound 30 || { fail "it never rang"; return; }
    on_screen .ring-off 10
    click .ring-off 0 0.5
    wait_for 5 sound_off
    dev rm -f /sdcard/Download/alarmfree-rings.txt
    on_screen .nav-item 8
    click .nav-item 2
    on_screen .banner
    click .banner
    on_screen .check-title
    click_row 'Save the record'
    sleep 0.5
    [ "$(dev cat /sdcard/Download/alarmfree-rings.txt | grep -c ' sound ')" -ge 1 ] && record=yes
    dev rm -f /sdcard/Download/alarmfree-rings.txt
    if ! sound_on && [ "$(db_count '"hidden":true')" = 0 ] && [ "$(outcomes solved)" = 0 ] && [ "$record" = yes ]; then
        pass "rang ${SOUND_AT}s after its time, nothing left, nothing in history, its record saved"
    elif [ "$record" != yes ]; then
        fail "no record of the ring in Downloads"
    else
        fail "sound=$(sound_on && echo ON || echo off) hidden=$(db_count '"hidden":true') solved=$(outcomes solved)"
    fi
}

# The quiet notification before an alarm. A loose OS alarm, which Android may deliver late:
# what counts is that it comes before the ring.
s33() {
    scenario s33_notification_before_alarm
    fresh
    use_settings '"Dark"' '"English"' '"setup_done":true,"language_follows_phone":false,"notify_before_minutes":5'
    next_minute 320
    seed a "$(alarm_json a "$T_H" "$T_M" "$EVERY_DAY" null)"
    arm a
    jump_to $((T - 303))
    reminded() { dev dumpsys notification --noredact | grep -q 'sentinel_reminders'; }
    # Not an exact alarm, on purpose. On a phone its time has to come first, too.
    if wait_for 180 reminded; then
        pass "shown $((T - $(now_s)))s before the ring"
    else
        fail "not shown, $((T - $(now_s)))s before the ring: $(dev dumpsys alarm | grep -m1 -A3 'reminder:a' | tr -s ' ' | cut -c1-160)"
    fi
}

# Nothing sticks out of the screen in the longest languages, with large text.
s34() {
    scenario s34_nothing_clipped
    local check=$NOTHING_CLIPPED problems="" locale result locales="Dutch Spanish French German"
    # On a big screen only the longest words, each language is a restart.
    [ -z "${VARIANT:-}" ] || locales=German
    fresh
    next_minute 3600
    seed a "$(alarm_json a "$T_H" "$T_M" "$EVERY_DAY" null)"
    dev settings put system font_scale 1.3
    for locale in $locales; do
        use_settings '"Dark"' "\"$locale\"" '"setup_done":true,"language_follows_phone":false'
        start_app
        on_screen .alarm-main 10
        result=$(ask_page "$check")
        [ -z "$result" ] || problems="$problems $locale/home: $result;"
        click .alarm-main
        on_screen .time-big
        result=$(ask_page "$check")
        [ -z "$result" ] || problems="$problems $locale/editor: $result;"
        press_back
        on_screen .nav-item
        click .nav-item 1
        result=$(ask_page "$check")
        [ -z "$result" ] || problems="$problems $locale/history: $result;"
        click .nav-item 2
        result=$(ask_page "$check")
        [ -z "$result" ] || problems="$problems $locale/settings: $result;"
        click .banner
        result=$(ask_page "$check")
        [ -z "$result" ] || problems="$problems $locale/reliability: $result;"
        dev am force-stop "$PKG"
    done
    dev settings put system font_scale 1.0
    if [ -z "$problems" ]; then
        pass "$locales with large text"
    else
        fail "$problems"
    fi
}

# Where the middle of an element is on the screen, in device pixels. The page counts from
# the top of the WebView, which can start below the status bar.
WEBVIEW_TOP=""
know_webview_top() {
    [ -z "$WEBVIEW_TOP" ] || return 0
    dev uiautomator dump /sdcard/af_ui.xml > /dev/null 2>&1
    WEBVIEW_TOP=$(dev cat /sdcard/af_ui.xml | tr '>' '\n' | grep -m1 'android.webkit.WebView' | sed -E 's/.*bounds="\[[0-9]+,([0-9]+)\].*/\1/')
}
screen_point() { # selector [index]
    js "var r = document.querySelectorAll('$1')[${2:-0}].getBoundingClientRect(); return Math.round((r.left + r.width / 2) * devicePixelRatio) + ' ' + (Math.round((r.top + r.height / 2) * devicePixelRatio) + ${WEBVIEW_TOP:-0});"
}
# Hold an element, then move it down by `$3` device pixels, as a finger does.
hold_and_drag() { # selector index pixels
    local x y step
    read -r x y <<< "$(screen_point "$1" "$2")"
    # Numbers or nothing: a failure here must fail the scenario, not end the run.
    case "$x$y$3" in
        '' | *[!0-9-]*)
            echo "nothing to drag: $x $y $3"
            return 1
            ;;
    esac
    dev input motionevent DOWN "$x" "$y"
    sleep 0.6
    for step in 2 4; do
        dev input motionevent MOVE "$x" $((y + $3 * step / 4))
    done
    sleep 0.2
    dev input motionevent UP "$x" $((y + $3))
    sleep 0.4
}
# The distance between two elements of a kind, in device pixels.
gap_between() { # selector from to
    js "var c = document.querySelectorAll('$1'); return Math.round((c[$3].getBoundingClientRect().top - c[$2].getBoundingClientRect().top) * devicePixelRatio);"
}
texts_of() { js "return [...document.querySelectorAll('$1')].map(e => e.textContent).join(' ')"; }
click_row() { # the row of a list with this title
    js "var e = [...document.querySelectorAll('.row-btn')].find(e => e.textContent.includes('$1')); if (!e) return 'no row $1'; e.click(); return 'ok'" > /dev/null
    sleep 0.25
}

# Hold an alarm on the list and move it; the same for a challenge, by its handle.
s36() {
    scenario s36_hold_and_drag
    fresh
    local two='[["challenge.math","{\"difficulty\":\"easy\"}"],["challenge.memory","{\"difficulty\":\"easy\"}"]]'
    seed a "$(alarm_json a 7 0 "$EVERY_DAY" null air 0 0 3 1 "$two")"
    seed b "$(alarm_json b 8 0 "$EVERY_DAY" null)"
    seed c "$(alarm_json c 9 0 "$EVERY_DAY" null)"
    arm a
    on_screen .alarm-main 10
    know_webview_top
    local before after again gap problems=""
    before=$(texts_of .alarm-label)
    gap=$(gap_between .alarm-card 0 2)
    case "$gap" in
        '' | *[!0-9]*)
            fail "no two cards to measure: $gap"
            return
            ;;
    esac
    hold_and_drag .alarm-main 0 $((gap + 30))
    after=$(texts_of .alarm-label)
    dev am force-stop "$PKG"
    start_app
    on_screen .alarm-main 10
    again=$(texts_of .alarm-label)
    [ "$before" = "a b c" ] || problems="$problems; started as '$before'"
    [ "$after" = "b c a" ] || problems="$problems; after dragging '$after'"
    [ "$again" = "$after" ] || problems="$problems; after restarting '$again'"
    # In the editor. The first card is now b, a is the last.
    click .alarm-main 2
    on_screen .row-btn
    click_row Challenges
    on_screen .challenge-row
    local rows_before rows_after
    rows_before=$(texts_of .challenge-row)
    gap=$(gap_between .challenge-row 0 1)
    hold_and_drag .drag-handle 0 $((gap + 20))
    rows_after=$(texts_of .challenge-row)
    [ -n "$rows_before" ] && [ "$rows_before" != "$rows_after" ] || problems="$problems; challenges stayed: '$rows_after'"
    if [ -z "$problems" ]; then
        pass "alarms $before -> $after, kept after a restart, challenges too"
    else
        fail "$problems"
    fi
}

# Swiping a card aside shows skip and delete; both have an undo.
s37() {
    scenario s37_swipe_a_card
    fresh
    next_minute 3600
    seed a "$(alarm_json a "$T_H" "$T_M" "$EVERY_DAY" null)"
    arm a
    on_screen .alarm-card 10
    know_webview_top
    swipe_card() {
        local x y width
        read -r x y <<< "$(screen_point .alarm-card)"
        width=$(js "return Math.round(document.querySelector('.alarm-card').getBoundingClientRect().width * devicePixelRatio)")
        case "$x$y$width" in
            '' | *[!0-9-]*)
                echo "nothing to swipe: $x $y $width"
                return 1
                ;;
        esac
        dev input swipe $((x + width * 35 / 100)) "$y" $((x - width * 35 / 100)) "$y" 300
        sleep 0.6
    }
    local problems=""
    swipe_card
    screenshot s37_swiped
    [ "$(js "return document.querySelector('.alarm-card').scrollLeft > 0 ? 'y' : 'n'")" = y ] || problems="$problems; did not move"
    [ "$(js "return document.querySelector('.swipe-action.danger').getBoundingClientRect().right <= innerWidth ? 'y' : 'n'")" = y ] || problems="$problems; delete not in view"
    click .swipe-action 0 0.6
    [ "$(db_count '"skip_until_ms":null')" = 0 ] || problems="$problems; not skipped"
    [ "$(js "return document.querySelector('.alarm-card').scrollLeft === 0 ? 'y' : 'n'")" = y ] || problems="$problems; stayed aside"
    click .toast-undo 0 0.5
    [ "$(db_count '"skip_until_ms":null')" = 1 ] || problems="$problems; skip not undone"
    swipe_card
    click .swipe-action.danger 0 0.5
    [ "$(db_count '"id":"a"')" = 0 ] || problems="$problems; not deleted"
    gone .alarm-card || problems="$problems; card still there"
    click .toast-undo 0 0.5
    [ "$(db_count '"id":"a"')" = 1 ] || problems="$problems; delete not undone"
    on_screen .alarm-card || problems="$problems; card not back"
    wait_for 3 os_has_alarm || problems="$problems; not armed after undo"
    # A swipe does not get in the way of an ordinary tap.
    click .alarm-main
    on_screen .time-big || problems="$problems; no editor"
    if [ -z "$problems" ]; then
        pass "skip and delete by swiping, both undone"
    else
        fail "$problems"
    fi
}

# The accent colour, and a name for a sound of one's own.
s38() {
    scenario s38_accent_and_sound_name
    fresh
    as_app "mkdir -p files/sounds/custom && cp files/sounds/default/air.ogg files/sounds/custom/mytune.ogg"
    seed a "$(alarm_json a 7 0 "$EVERY_DAY" null)"
    arm a
    on_screen .nav-item 10
    local problems="" accent
    click .nav-item 2
    on_screen .row-btn
    click_row Theme
    on_screen .swatch
    screenshot s38_swatches
    [ "$(js "return document.querySelectorAll('.swatch').length")" = 5 ] || problems="$problems; no swatches"
    click .swatch 2
    accent="getComputedStyle(document.querySelector('.app-root')).getPropertyValue('--accent').trim()"
    [ "$(js "return $accent")" = "#b9a0ff" ] || problems="$problems; colour is $(js "return $accent")"
    dev am force-stop "$PKG"
    start_app
    on_screen .alarm-main 10
    [ "$(js "return $accent")" = "#b9a0ff" ] || problems="$problems; not kept: $(js "return $accent")"
    # Its own name.
    click .alarm-main
    on_screen .row-btn
    click_row Sound
    on_screen .radio-text
    local names
    names=$(texts_of .radio-text)
    case "$names" in
        *mytune*) ;;
        *) problems="$problems; own sound not listed: $names" ;;
    esac
    click '.radio-row .icon-btn' 0
    on_screen '.radio-row .field'
    js "var i = document.querySelector('.radio-row .field'); Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value').set.call(i, 'Morning'); i.dispatchEvent(new Event('input', { bubbles: true })); return 'ok'" > /dev/null
    sleep 0.3
    click '.radio-row .icon-btn' 0 0.4
    names=$(texts_of .radio-text)
    case "$names" in
        *Morning*) ;;
        *) problems="$problems; name not shown: $names" ;;
    esac
    printf "SELECT prefs FROM settings;\n" | sql | grep -q '"mytune.ogg":"Morning"' || problems="$problems; name not saved"
    if [ -z "$problems" ]; then
        pass "accent purple kept, sound renamed"
    else
        fail "$problems"
    fi
}

# An alarm can vibrate first and go to full brightness.
s39() {
    scenario s39_vibrate_first_and_brightness
    fresh
    next_minute
    ALARM_EXTRA=',"sound_delay_secs":5,"max_brightness":true'
    seed a "$(alarm_json a "$T_H" "$T_M" "$EVERY_DAY" null air 0 0 0 1)"
    ALARM_EXTRA=
    arm a
    ring_started() { adb_ logcat -d 2> /dev/null | grep -q 'ringing a:'; }
    until_past "$T"
    wait_for 30 ring_started || { fail "never started"; return; }
    local started problems="" heard=""
    started=$SECONDS
    sound_on && problems="$problems; sound at once"
    wait_for 12 sound_on && heard=$((SECONDS - started))
    [ -n "$heard" ] && [ "$heard" -ge 3 ] && [ "$heard" -le 9 ] || problems="$problems; sound after '${heard}'s"
    bright() { dev dumpsys window windows | grep -q 'sbrt=1.0'; }
    wait_for 8 bright || problems="$problems; not at full brightness"
    on_screen .ring-off 10
    click .ring-off 0 0.5
    wait_for 4 sound_off || problems="$problems; still ringing"
    not_bright() { ! bright; }
    wait_for 4 not_bright || problems="$problems; brightness stayed"
    if [ -z "$problems" ]; then
        pass "sound ${heard}s after the vibration, full brightness until over"
    else
        fail "$problems"
    fi
}

# The home-screen widget: placed from the settings, says the next ring, follows changes.
# Leaves a widget behind, so never on a phone somebody uses.
s40() {
    if ! on_emulator; then
        note "s40 leaves a widget on the launcher: not here"
        return
    fi
    scenario s40_home_widget
    fresh
    next_minute 3600
    seed a "$(alarm_json a "$T_H" "$T_M" "$EVERY_DAY" null)"
    arm a
    on_screen .nav-item 10
    ui_texts() {
        dev uiautomator dump /sdcard/af_ui.xml > /dev/null 2>&1
        dev cat /sdcard/af_ui.xml | tr '>' '\n' | grep -oE ' text="[^"]+"' | sed -E 's/ text="(.*)"/\1/'
    }
    click .nav-item 2
    on_screen .row-btn || { fail "no settings"; return; }
    click_row "Add widget"
    # The launcher asks the user first.
    local add x1 y1 x2 y2 problems="" minute texts
    the_add_button() {
        dev uiautomator dump /sdcard/af_ui.xml > /dev/null 2>&1
        add=$(dev cat /sdcard/af_ui.xml | tr '>' '\n' | grep -iE ' text="(add|add to home screen|add automatically)"' | grep -m1 'clickable="true"' | sed -E 's/.*bounds="\[([0-9]+),([0-9]+)\]\[([0-9]+),([0-9]+)\].*/\1 \2 \3 \4/')
        [ -n "$add" ]
    }
    if ! wait_for 8 the_add_button; then
        fail "the launcher did not ask: $(ui_texts | tr '\n' '|')"
        return
    fi
    read -r x1 y1 x2 y2 <<< "$add"
    dev input tap $(((x1 + x2) / 2)) $(((y1 + y2) / 2))
    sleep 1
    dev dumpsys appwidget | grep -q 'SentinelHomeWidget' || problems="$problems; not registered"
    go_home
    sleep 1
    minute=$(printf ':%02d' "$T_M")
    texts=$(ui_texts | tr '\n' '|')
    case "$texts" in
        *"$minute"*) ;;
        *) problems="$problems; no time on it: $texts" ;;
    esac
    # Follows a change, without the widget being asked.
    start_app
    on_screen .nav-item 8
    click .nav-item 0
    on_screen .alarm-switch
    click .alarm-switch 0 1
    go_home
    sleep 1
    texts=$(ui_texts | tr '\n' '|')
    case "$texts" in
        *"No alarm set"*) ;;
        *) problems="$problems; still old: $texts" ;;
    esac
    remove_widgets
    if [ -z "$problems" ]; then
        pass "placed, shows the next ring, follows a change"
    else
        fail "$problems"
    fi
}

# The text of the open panel that lies under something else, at the end of a long panel.
covered() {
    js 'var s = document.querySelector(".sheet"); if (s) s.scrollTop = s.scrollHeight; return "ok"' > /dev/null
    sleep 0.1
    js 'var s = document.querySelector(".sheet"), bad = []; if (!s) return ""; var box = s.getBoundingClientRect(); s.querySelectorAll("*").forEach(e => { if (e.children.length || !e.textContent.trim()) return; var r = e.getBoundingClientRect(), x = r.left + r.width / 2, y = r.top + r.height / 2; if (r.width < 2 || r.height < 2 || y < box.top || y > box.bottom) return; var on = document.elementFromPoint(x, y); if (on && on !== e && !e.contains(on) && !on.contains(e)) bad.push(e.textContent.trim().slice(0, 24) + " under " + (on.className || on.tagName)); }); return bad.slice(0, 3).join(", ");'
}

# The camera scanner really opens, also in a release: what only a layout refers to is the
# first thing R8 and the resource shrinker take.
s42() {
    scenario s42_scanner_opens
    fresh
    seed a "$(alarm_json a 7 0 "$EVERY_DAY" null air 0 0 3 1 '[["challenge.scan","{}"]]')"
    arm a
    on_screen .alarm-main 10
    click .alarm-main
    on_screen .row-btn
    click_row Challenges
    on_screen '.challenge-row .link-btn'
    click '.challenge-row .link-btn' 0 0.4
    on_screen .run-screen || { fail "no challenge screen"; return; }
    adb_ logcat -c
    click .scan-button 0 0.3
    # It has the keys, or Back goes nowhere. And the page behind it stays what it was, so what
    # says it came back is the button being one again, ready for the next scan.
    scanner_on_top() { focused_window | grep -q SentinelScannerActivity; }
    local crashed
    if wait_for 8 scanner_on_top; then
        crashed=$(adb_ logcat -d 2> /dev/null | grep -E 'FATAL EXCEPTION|ClassNotFoundException|NoClassDefFoundError|Resources\$NotFoundException|InflateException' | head -3)
        if [ -n "$crashed" ]; then
            fail "opened, but: $crashed"
            return
        fi
        press_back
        if ! on_screen button.scan-button 8; then
            fail "did not come back from the scanner: $(focused_window)"
        else
            # Right away again: the first one must not end this one.
            click .scan-button 0 0.3
            if ! wait_for 8 scanner_on_top; then
                fail "the second time it does not open: $(read_text .scan-button)"
            elif ! is_there 'div.scan-button.scanning'; then
                fail "the second scan ended while the camera is up: $(read_text .scan-button)"
            else
                press_back
                if on_screen button.scan-button 8; then
                    pass "opened the camera twice in a row and came back"
                else
                    fail "did not come back the second time"
                fi
            fi
        fi
    else
        crashed=$(adb_ logcat -d 2> /dev/null | grep -E 'FATAL EXCEPTION|ClassNotFoundException|NoClassDefFoundError|Resources\$NotFoundException|InflateException' | head -3)
        fail "no scanner on top: $(dev dumpsys activity activities | grep -m1 ResumedActivity); $crashed"
    fi
}

# One row of history, `$2` days ago.
history_row() { # id days_ago
    local at
    at=$(date -u -d "@$(($(now_s) - $2 * 86400))" +%Y-%m-%dT07:00:00+00:00)
    printf "INSERT INTO alarm_events (id, alarm_id, alarm_label, fire_timestamp, dismiss_timestamp, total_dismissal_time_ms, outcome, seen) VALUES ('%s', 'a', 'a', '%s', '%s', 0, 'solved', 1);\n" "$1" "$at" "$at" | sql
}
history_count() { printf "SELECT count(*) FROM alarm_events;\n" | sql; }

# A period of the history and all of it each go only after asking again.
s43() {
    scenario s43_delete_history
    fresh
    history_row old 200
    history_row new 1
    start_app
    on_screen .nav-item 10
    local problems=""
    click .nav-item 1
    on_screen .icon-btn
    # A period asks first: Back is no, and the answer takes only that period.
    click .icon-btn 0
    on_screen .sheet || problems="$problems; no sheet"
    click .sheet-item 0 0.4
    is_there '.sheet-item.danger' || problems="$problems; older than 30 days went without asking"
    press_back
    gone .sheet || problems="$problems; Back left the period question open"
    [ "$(history_count)" = 2 ] || problems="$problems; Back on the period question deleted"
    click .icon-btn 0
    on_screen .sheet-item
    click .sheet-item 0 0.4
    click '.sheet-item.danger' 0 0.4
    [ "$(history_count)" = 1 ] || problems="$problems; older than 30 days left $(history_count)"
    on_screen .toast-text || problems="$problems; nothing said about it"
    click .icon-btn 0
    on_screen .sheet-item
    click .sheet-item 2 0.4
    [ "$(history_count)" = 1 ] || problems="$problems; everything went without asking again"
    # What the delete before it said is still up: it may not cover this question's buttons.
    is_there .toast || problems="$problems; the message before this went too soon to tell"
    local under
    under=$(covered)
    [ -z "$under" ] || problems="$problems; a message covers: $under"
    screenshot s43_asking
    press_back
    gone .sheet || problems="$problems; Back left the question open"
    [ "$(history_count)" = 1 ] || problems="$problems; Back deleted it"
    click .icon-btn 0
    on_screen .sheet-item
    click .sheet-item 2 0.3
    click '.sheet-item.danger' 0 0.4
    [ "$(history_count)" = 0 ] || problems="$problems; left $(history_count) after confirming"
    if [ -z "$problems" ]; then
        pass "a period and everything, each after asking, and Back is no"
    else
        fail "${problems#; }"
    fi
}

# Two shake challenges in a row: the second starts its sensor after the first stopped its
# own. Only on an emulator, which can be told what its accelerometer feels.
s44() {
    if ! on_emulator; then
        note "s44 needs an emulator"
        return
    fi
    scenario s44_two_shakes_in_a_row
    fresh
    next_minute
    seed a "$(alarm_json a "$T_H" "$T_M" "$EVERY_DAY" null air 0 0 3 1 '[["challenge.shake","{\"count\":2}"],["challenge.shake","{\"count\":2}"]]')"
    arm a
    await_ring a 30 || { fail "never started"; return; }
    shake() {
        local i=0
        while [ "$i" -lt 3 ]; do
            adb_ emu sensor set acceleration 40:40:40 > /dev/null
            sleep 0.35
            adb_ emu sensor set acceleration 0:0:9.8 > /dev/null
            sleep 0.35
            i=$((i + 1))
        done
    }
    on_screen .ring-off 10
    adb_ logcat -c
    click .ring-off 0 0.5
    on_screen .progress-ring
    local rounds=0 starts
    while [ "$rounds" -lt 5 ] && state_is a firing; do
        shake
        sleep 1
        rounds=$((rounds + 1))
    done
    await_state a scheduled 5
    starts=$(adb_ logcat -d 2> /dev/null | grep -c 'Accelerometer started')
    if [ "$(state a)" = scheduled ] && [ "$(outcomes solved)" = 1 ] && [ "$starts" -ge 2 ] && ! sound_on; then
        pass "both counted, the sensor started $starts times, in $rounds rounds of shaking"
    else
        fail "state=$(state a) solved=$(outcomes solved) sensor started $starts times, on screen '$(read_text .progress-ring-text)'"
    fi
}

# A warning that is not a problem for this user can be hidden, and stays on its own screen.
s45() {
    scenario s45_hide_a_warning
    fresh
    if ! deny_overlay; then
        skip "needs a warning to hide"
        return
    fi
    start_app
    on_screen .banner-text 10
    local problems="" first
    first=$(read_text .banner-text)
    [ -n "$first" ] || problems="$problems; nothing to hide"
    click '.banner-row .icon-btn' 0 0.4
    [ "$(read_text .banner-text)" != "$first" ] || problems="$problems; still there after hiding"
    dev am force-stop "$PKG"
    start_app
    on_screen .home-header 10
    [ "$(read_text .banner-text)" != "$first" ] || problems="$problems; back after a restart"
    printf "SELECT prefs FROM settings;\n" | sql | grep -q 'hidden_warnings":\["' || problems="$problems; not saved"
    if [ -z "$problems" ]; then
        pass "hid '$first', and it stays hidden"
    else
        fail "${problems#; }"
    fi
}

# A 12-hour clock everywhere: the list, the editor, the wheels, and what gets saved.
s46() {
    scenario s46_twelve_hours
    fresh
    use_settings '"Dark"' '"English"' '"setup_done":true,"language_follows_phone":false,"time_format":"H12"'
    seed a "$(alarm_json a 14 5 "$EVERY_DAY" null)"
    arm a
    on_screen .alarm-time 10
    local problems="" card big wheel
    card=$(read_text .alarm-time)
    [ "$card" = "2:05 PM" ] || problems="$problems; card says '$card'"
    click .alarm-main
    on_screen .time-big
    big=$(read_text .time-big)
    [ "$big" = "2:05 PM" ] || problems="$problems; editor says '$big'"
    click .time-big
    on_screen .wheel-cell-center
    wheel=$(js "return [...document.querySelectorAll('.wheel-cell-center')].map(e => e.textContent).join(' ')")
    case "$wheel" in
        *2*PM*5* | *2*PM*05*) ;;
        *) problems="$problems; wheels say '$wheel'" ;;
    esac
    if [ -z "$problems" ]; then
        pass "the list, the editor and the wheels say $card"
    else
        fail "${problems#; }"
    fi
}

# From an alarm's menu: new alarms start like this one, without its name.
s47() {
    scenario s47_use_for_new_alarms
    fresh
    ALARM_EXTRA=',"note":"Keep out"'
    seed a "$(alarm_json a 7 0 "$EVERY_DAY" null lofi 0 60 2 9)"
    ALARM_EXTRA=
    arm a
    on_screen .alarm-menu 10
    local problems="" rows
    click .alarm-menu
    on_screen .sheet-item
    js "[...document.querySelectorAll('.sheet-item')].find(e => e.textContent === 'Use for new alarms').click(); return 'ok'" > /dev/null
    on_screen .toast-text || problems="$problems; nothing said about it"
    click .fab
    on_screen .row-btn
    rows=$(js "return [...document.querySelectorAll('.row-btn')].map(e => e.textContent).join('|')")
    case "$rows" in
        *Lofi*fade*) ;;
        *) problems="$problems; sound not taken over: $rows" ;;
    esac
    case "$rows" in
        *"9 min, 2"*) ;;
        *) problems="$problems; snooze not taken over: $rows" ;;
    esac
    case "$rows" in
        *Keep\ out* | *"Label"a*) problems="$problems; its name came along: $rows" ;;
    esac
    if [ -z "$problems" ]; then
        pass "sound and snooze taken over, its name not"
    else
        fail "${problems#; }"
    fi
}

# What goes through the phone's own file picker, in the build that R8 went over: a sound of
# one's own in, and alarms out to a file and back. Emulator only: the picker differs per
# phone, and this looks for its words.
ui_tap() { # the middle of the first element with this in its description
    local bounds x1 y1 x2 y2
    dev uiautomator dump /sdcard/af_ui.xml > /dev/null 2>&1
    bounds=$(dev cat /sdcard/af_ui.xml | tr '>' '\n' | grep -m1 "$1" | sed -E 's/.*bounds="\[([0-9]+),([0-9]+)\]\[([0-9]+),([0-9]+)\].*/\1 \2 \3 \4/')
    [ -n "$bounds" ] || return 1
    read -r x1 y1 x2 y2 <<< "$bounds"
    dev input tap $(((x1 + x2) / 2)) $(((y1 + y2) / 2))
}
picker_tap() { # file name
    local i=0
    while [ "$i" -lt 8 ]; do
        ui_tap "text=\"$1\"" && return 0
        # It opens on its recent files. Over to Downloads.
        if ui_tap 'content-desc="Show roots"'; then
            sleep 1
            ui_tap 'text="Downloads"'
        fi
        sleep 1.5
        i=$((i + 1))
    done
    return 1
}
s48() {
    if ! on_emulator; then
        note "s48 needs an emulator"
        return
    fi
    scenario s48_files_in_and_out
    fresh
    dev rm -f /sdcard/Download/mytune.ogg '/sdcard/Download/alarmfree-*'
    adb_ push assets/sounds/default/air.ogg /sdcard/Download/mytune.ogg > /dev/null
    dev content call --uri content://media --method scan_volume --arg external_primary > /dev/null 2>&1
    seed a "$(alarm_json a 7 0 "$EVERY_DAY" null)"
    arm a
    on_screen .alarm-main 10
    local problems="" names file
    # A sound.
    click .alarm-main
    on_screen .row-btn
    click_row Sound
    on_screen '.sheet .btn-quiet'
    click '.sheet .btn-quiet' 0 1
    if picker_tap mytune.ogg; then
        on_screen .radio-text 20
        own_sound_there() {
            names=$(texts_of .radio-text)
            case "$names" in
                *mytune*) return 0 ;;
            esac
            return 1
        }
        wait_for 8 own_sound_there || problems="$problems; own sound not listed: $names"
        [ -n "$(as_app "ls files/sounds/custom")" ] || problems="$problems; not copied"
    else
        problems="$problems; the picker never showed the file"
        press_back
    fi
    dev am force-stop "$PKG"
    start_app
    on_screen .nav-item 10
    # Alarms to a file: only the one alarm in it, under today's name.
    click .nav-item 2
    on_screen .row-btn
    click_row "Back up"
    on_screen .sheet-item
    click .sheet-item 0 0.8
    file=$(dev ls /sdcard/Download/ | grep -m1 '^alarmfree-')
    [ -n "$file" ] || problems="$problems; no file in Downloads"
    [ "$(dev cat "/sdcard/Download/$file" | grep -c '"time_hour"')" -ge 1 ] || problems="$problems; no alarm in the file"
    # And back from it, as a second one.
    click_row "Back up"
    on_screen .sheet-item
    click .sheet-item 1 1
    if picker_tap "$file"; then
        on_screen .toast-undo 20
        [ "$(alarm_count)" = 2 ] || problems="$problems; $(alarm_count) alarms after restoring"
        click .toast-undo 0 0.5
        [ "$(alarm_count)" = 1 ] || problems="$problems; undo left $(alarm_count) alarms"
    else
        problems="$problems; the picker never showed the backup"
        press_back
    fi
    dev rm -f /sdcard/Download/mytune.ogg "/sdcard/Download/$file"
    if [ -z "$problems" ]; then
        pass "own sound in, alarms out to $file and back"
    else
        fail "${problems#; }"
    fi
}

# The keyboard stays off what is being typed in, in a panel and in the typing challenge, and
# what is typed arrives.
s49() {
    scenario s49_keyboard
    fresh
    seed a "$(alarm_json a 7 0 "$EVERY_DAY" null air 0 0 3 1 '[["challenge.typing","{\"difficulty\":\"easy\"}"]]')"
    arm a
    on_screen .alarm-main 10
    know_webview_top
    local problems="" x y before free
    # How much of the page is left above whatever covers it.
    local left='return Math.round(innerHeight - parseFloat(getComputedStyle(document.querySelector(".app-root")).getPropertyValue("--safe-bottom")))'
    before=$(js "$left")
    click .alarm-main
    on_screen .row-btn
    click_row Label
    on_screen '.sheet .field'
    read -r x y <<< "$(screen_point '.sheet .field')"
    dev input tap "$x" "$y"
    made_room() {
        free=$(js "$left")
        [ "$free" -lt $((before - 100)) ] 2> /dev/null
    }
    wait_for 6 made_room || problems="$problems; the page did not make room: $before, then $free"
    screenshot s49_label
    [ "$(js "return document.querySelector('.sheet .field').getBoundingClientRect().bottom <= $free && document.querySelector('.sheet .sheet-done').getBoundingClientRect().bottom <= $free ? 'y' : 'n'")" = y ] ||
        problems="$problems; the field or Done is under the keyboard"
    dev input text Morning
    sleep 0.6
    # A letter can get lost on an emulator this slow, with keys this close together.
    case "$(js "return document.querySelector('.sheet .field').value")" in
        aM*g) ;;
        *) problems="$problems; typed text did not arrive: $(js "return document.querySelector('.sheet .field').value")" ;;
    esac
    press_back
    press_back
    room_back() {
        free=$(js "$left")
        [ "$free" = "$before" ]
    }
    wait_for 5 room_back || problems="$problems; no room given back: $before, now $free"
    # The challenge. It has to show the passage and the box together.
    press_back
    on_screen '.sheet-item.danger'
    click '.sheet-item.danger' 0 0.3
    on_screen .home-header || problems="$problems; not back on Home"
    click .alarm-menu
    on_screen .sheet-item
    click .sheet-item 0 0.5
    on_screen .typing-input-area 6
    read -r x y <<< "$(screen_point .typing-input-area)"
    dev input tap "$x" "$y"
    wait_for 6 made_room
    screenshot s49_typing
    [ "$(js "var a = document.querySelector('.typing-input-area').getBoundingClientRect(), p = document.querySelector('.typing-passage').getBoundingClientRect(); return a.bottom <= $free && p.top >= 0 && p.height >= 60 ? 'y' : 'n'")" = y ] ||
        problems="$problems; passage and box do not fit above the keyboard"
    dev input text "$(js "return document.querySelector('.typing-passage').textContent.replace(/\\s+/g, '%s')")"
    gone .typing-input-area 8 || problems="$problems; typing it all did not finish: '$(js "return document.querySelector('.typing-input-area').value")'"
    if [ -z "$problems" ]; then
        pass "room made and given back, text arrives, challenge done by typing"
    else
        fail "${problems#; }"
    fi
}

click_text() { # selector text [seconds]: the one of these that says exactly this
    js "var e = [...document.querySelectorAll('$1')].find(e => e.textContent.trim() === '$2'); if (!e) return 'nothing says $2'; e.click(); return 'ok'" > /dev/null
    sleep "${3:-0.25}"
}
set_field() { # selector which value [event]: as typing or picking does
    js "var i = document.querySelectorAll('$1')[$2]; Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value').set.call(i, '$3'); i.dispatchEvent(new Event('${4:-input}', { bubbles: true })); return 'ok'" > /dev/null
    sleep 0.15
}
done_sheet() {
    click '.sheet .sheet-done' 0 0.2
    gone .sheet
}
# Open a row, and wait for its panel.
open_row() {
    click_row "$1"
    on_screen .sheet
}
# Does the text have this in it? Otherwise it goes on the list of problems.
has() { # text what
    case "$1" in
        *"$2"*) ;;
        *) problems="$problems; no $2" ;;
    esac
}

# Everything of an alarm, set through the screens: what is saved, and what it says after.
s50() {
    scenario s50_editor_saves_it_all
    fresh
    start_app
    on_screen .fab 10
    local problems="" row rows
    click .fab
    on_screen .time-shift
    # Either side of the time: -1, -5, then +1, +5.
    click .time-shift 3
    click .time-shift 3
    click_text .chip-small Weekend
    open_row Label
    set_field '.sheet .field' 0 Morning
    set_field '.sheet .field' 1 'Take pills'
    done_sheet
    open_row Sound
    click_text '.sheet .radio-text' Funk
    click_text '.sheet .chip' '2 min'
    set_field '.sheet .slider' 0 75
    done_sheet
    open_row Vibration
    click_text '.sheet .chip' Steady
    click_text '.sheet .chip' '15 s'
    done_sheet
    open_row Snooze
    click '.sheet .round-btn' 1
    click '.sheet .round-btn' 3
    done_sheet
    open_row Challenges
    # Math and Steps, each through Add and its own sheet.
    for option in 0 5; do
        click '.sheet .btn-quiet' 0 0.3
        on_screen .add-option || problems="$problems; no way to add a challenge"
        click .add-option "$option" 0.3
        on_screen '.sheet .sheet-done'
        click '.sheet .sheet-done' 0 0.3
        on_screen .challenge-row || problems="$problems; not back on the list after adding"
    done
    click_text '.sheet .chip' Random
    done_sheet
    open_row 'Ring limit'
    click_text '.sheet .chip' '60 min'
    done_sheet
    open_row 'Wake-up check'
    click_text '.sheet .chip' '3 min'
    done_sheet
    click_row 'Full brightness'
    screenshot s50_set
    click .save-btn 0 0.3
    on_screen .toast-text || problems="$problems; Save said nothing"
    on_screen .home-header || problems="$problems; Save did not go back"
    row=$(printf 'SELECT data FROM alarms;\n' | sql)
    for field in '"time_hour":7' '"time_minute":10' '"label":"Morning"' '"note":"Take pills"' \
        '"days":[false,false,false,false,false,true,true]' '"sound":"funk' '"fade_in_secs":120' \
        '"volume_percent":75' '"vibration":1' '"sound_delay_secs":15' '"snooze_interval":6' \
        '"snooze_count":4' 'challenge.math' 'challenge.steps' '"random_order":true' \
        '"ring_limit_secs":3600' '"wake_check_minutes":3' '"max_brightness":true'; do
        has "$row" "$field"
    done
    local id
    id=$(printf 'SELECT id FROM alarms;\n' | sql)
    await_state "$id" scheduled 5 || problems="$problems; not armed: $(state "$id")"
    # And what it says when opened again.
    click .alarm-main
    on_screen .row-btn
    rows=$(js "return [...document.querySelectorAll('.row-btn')].map(e => e.textContent).join('|')")
    for shown in Morning Funk '6 min, 4' Math '60 min' '3 min'; do
        has "$rows" "$shown"
    done
    has "$(read_text .time-big)" ':10'
    # Off the list from here too, after asking.
    click '.screen-header .icon-btn' 1
    on_screen .sheet || problems="$problems; delete does not ask"
    click '.sheet-item.danger' 0 0.5
    [ "$(alarm_count)" = 0 ] || problems="$problems; not deleted"
    on_screen .home-header || problems="$problems; delete did not go back"
    if [ -z "$problems" ]; then
        pass "every setting saved, shown again, armed, and deleted"
    else
        fail "${problems#; }"
    fi
}

# Every setting through its own screen, and all of it still there after a restart.
s51() {
    scenario s51_settings_stick
    fresh
    seed a "$(alarm_json a 14 5 "$EVERY_DAY" null)"
    arm a
    on_screen .nav-item 10
    local problems="" row columns
    click .nav-item 2
    on_screen .row-btn
    pick() { # row, then the choice of what it opens
        open_row "$1"
        click_text "$2" "$3" 0.3
        is_there .sheet && press_back
        gone .sheet
    }
    pick Theme .sheet-item Light
    pick 'Time format' .sheet-item '12 hours'
    pick 'Week starts on' .sheet-item Sunday
    open_row 'Before an alarm'
    click_text '.sheet .chip' '10 min'
    done_sheet
    open_row 'Minimum alarm volume'
    click_text '.sheet .chip' '75%'
    done_sheet
    click_row 'Touch feedback'
    click_row 'Less motion'
    click_row Confetti
    pick Language .sheet-item Nederlands
    sleep 0.2
    dev am force-stop "$PKG"
    start_app
    on_screen .alarm-time 10
    screenshot s51_after
    columns=$(printf 'SELECT theme, locale, min_volume_percent FROM settings;\n' | sql)
    row=$(printf 'SELECT prefs FROM settings;\n' | sql)
    has "$columns" Light
    has "$columns" Dutch
    has "$columns" 75
    for field in '"time_format":"H12"' '"week_start":"Sunday"' '"notify_before_minutes":10' \
        '"touch_feedback":false' '"less_motion":true' '"confetti":false' \
        '"language_follows_phone":false'; do
        has "$row" "$field"
    done
    is_there .theme-light || problems="$problems; not light"
    is_there .less-motion || problems="$problems; still moving"
    has "$(read_text .alarm-time)" '2:05 PM'
    has "$(texts_of .nav-item)" Wekkers
    # A week that starts on Sunday: Z for zondag first.
    has "$(js "return document.querySelector('.alarm-day').textContent")" Z
    if [ -z "$problems" ]; then
        pass "theme, language, 12 hours, week, notification, volume and switches all stick"
    else
        fail "${problems#; }"
    fi
}

# The alarm menu and what else is on the list, all the way through.
s52() {
    scenario s52_alarm_menu
    fresh
    # a rings later than b, but comes first in its own order.
    local later
    later=$(date -u -d "@$(($(now_s) + 4 * 86400))" +%Y-%m-%d)
    seed a "$(alarm_json a 9 0 "$EVERY_DAY" null)"
    seed b "$(alarm_json b 7 0 "$EVERY_DAY" null)"
    printf "UPDATE alarms SET data = replace(data, '\"order\":0', '\"order\":1') WHERE id='a'; UPDATE alarms SET data = replace(data, '\"order\":0', '\"order\":2') WHERE id='b';\n" | sql
    history_row one 0
    history_row two 0
    arm a
    on_screen .alarm-label 10
    local problems="" row
    menu_item() { # what to pick from the first alarm's menu
        click .alarm-menu 0
        on_screen .sheet-item
        click_text .sheet-item "$1" "${2:-0.5}"
    }
    data_of_a() { printf "SELECT data FROM alarms WHERE id='a';\n" | sql; }
    [ "$(texts_of .alarm-label)" = "a b" ] || problems="$problems; own order: $(texts_of .alarm-label)"
    click .round-icon-btn 1
    on_screen .sheet-item
    click_text .sheet-item 'By time' 0.4
    [ "$(texts_of .alarm-label)" = "b a" ] || problems="$problems; by time: $(texts_of .alarm-label)"
    click .round-icon-btn 1
    on_screen .sheet-item
    click_text .sheet-item 'Your own order' 0.4
    [ "$(texts_of .alarm-label)" = "a b" ] || problems="$problems; back to own: $(texts_of .alarm-label)"
    # Pause one until a date, and end it.
    menu_item 'Pause until a date' 0.3
    on_screen '.sheet .field'
    set_field '.sheet .field' 0 "$later" change
    done_sheet
    sleep 0.3
    row=$(data_of_a)
    case "$row" in
        *'"pause_until_ms":null'*) problems="$problems; not paused" ;;
    esac
    has "$(read_text .alarm-when)" Paused
    [ "$(printf "SELECT data FROM alarms WHERE id='b';\n" | sql | grep -c '"pause_until_ms":[0-9]')" = 0 ] || problems="$problems; paused the other one too"
    local paused_at
    paused_at=$(armed_at_ms a)
    [ "${paused_at:-0}" -gt $((($(now_s) + 3 * 86400) * 1000)) ] || problems="$problems; armed for $paused_at, inside the pause"
    menu_item 'End the pause'
    has "$(data_of_a)" '"pause_until_ms":null'
    # Skip the next ring from the menu, and take it back.
    menu_item 'Skip the next ring'
    case "$(data_of_a)" in
        *'"skip_until_ms":null'*) problems="$problems; not skipped" ;;
    esac
    menu_item 'Undo the skip'
    has "$(data_of_a)" '"skip_until_ms":null'
    # Duplicate: an editor with the copy, that saves as one more.
    menu_item Duplicate 0.3
    on_screen .row-btn
    has "$(js "return [...document.querySelectorAll('.row-btn')].map(e => e.textContent).join('|')")" 'a (copy)'
    click .save-btn 0 0.5
    [ "$(alarm_count)" = 3 ] || problems="$problems; $(alarm_count) alarms after duplicating"
    on_screen .alarm-switch
    # The switch.
    local switch_of_a="[...document.querySelectorAll('.alarm-face')].find(f => f.querySelector('.alarm-label').textContent === 'a').querySelector('.alarm-switch').click(); return 'ok'"
    js "$switch_of_a" > /dev/null
    sleep 0.5
    [ "$(db_enabled a)" = false ] || problems="$problems; the switch left it $(db_enabled a)"
    [ "$(state a)" != scheduled ] || problems="$problems; still armed when off"
    js "$switch_of_a" > /dev/null
    sleep 0.5
    [ "$(db_enabled a)" = true ] && await_state a scheduled 3 || problems="$problems; not back on: $(db_enabled a) $(state a)"
    # One row of the history.
    click .nav-item 1
    on_screen .event-main
    local before
    before=$(history_count)
    click .event-main 0
    on_screen '.event-details .link-btn'
    click_text '.event-details .link-btn' 'Delete this one' 0.5
    on_screen '.sheet .sheet-item.danger' || problems="$problems; deleting one row does not ask"
    click '.sheet-item.danger' 0 0.5
    [ "$(history_count)" = $((before - 1)) ] || problems="$problems; history went from $before to $(history_count)"
    if [ -z "$problems" ]; then
        pass "sort, pause, skip, duplicate, switch and one row of history after asking"
    else
        fail "${problems#; }"
    fi
}

# What is left of the screens: templates, a typed time, the wheels, one date, a challenge's
# settings, deleting a sound, the history's own days, the reliability screen's buttons.
s53() {
    scenario s53_the_rest_of_the_screens
    fresh
    as_app "mkdir -p files/sounds/custom && cp files/sounds/default/air.ogg files/sounds/custom/mytune.ogg"
    history_row near 3
    history_row far 30
    start_app
    on_screen .empty 10
    local problems="" row
    is_there .empty || problems="$problems; no empty state"
    click .empty-template 0 0.2
    on_screen .toast-undo || problems="$problems; a template says nothing"
    [ "$(alarm_count)" = 1 ] || problems="$problems; a template makes no alarm"
    click .alarm-main
    on_screen .time-big || problems="$problems; the alarm from a template opens no editor"
    # Typed, then a tap on the wheel: the row under the middle is one more.
    click .time-big
    on_screen '.sheet .field'
    set_field '.sheet .field' 0 835
    done_sheet
    has "$(read_text .time-big)" ':35'
    click .time-big
    on_screen .wheel-column
    js "document.querySelectorAll('.wheel-column')[1].querySelectorAll('.wheel-cell-near')[1].click(); return 'ok'" > /dev/null
    sleep 0.2
    done_sheet
    has "$(read_text .time-big)" ':36'
    # One date.
    click_text .chip-small 'One date'
    on_screen '.sheet .chip'
    click_text '.sheet .chip' Tomorrow
    done_sheet
    # A challenge with a setting, and one taken away again.
    open_row Challenges
    click '.sheet .btn-quiet' 0
    on_screen '.sheet .add-name'
    click_text '.sheet .add-name' Math 0.4
    is_there .challenge-list || { is_there .sheet && press_back; }
    on_screen .challenge-open
    click .challenge-open 0
    on_screen '.sheet .chip'
    click_text '.sheet .chip' Hard
    click '.sheet .sheet-done' 0 0.3
    on_screen .challenge-list
    click '.sheet .btn-quiet' 0
    on_screen '.sheet .add-name'
    click_text '.sheet .add-name' Shake 0.4
    is_there .challenge-list || { is_there .sheet && press_back; }
    on_screen .challenge-list
    [ "$(js "return document.querySelectorAll('.challenge-row').length")" = 2 ] || problems="$problems; added: $(texts_of .challenge-name)"
    click '.challenge-row .icon-btn' 1 0.3
    [ "$(js "return document.querySelectorAll('.challenge-row').length")" = 1 ] || problems="$problems; not taken away: $(texts_of .challenge-name)"
    done_sheet
    # A sound of one's own: asks first, then it is gone.
    open_row Sound
    has "$(texts_of .radio-text)" mytune
    click '.radio-row .icon-btn' 1 0.3
    click '.radio-row .link-btn.danger' 0 0.4
    case "$(texts_of .radio-text)" in
        *mytune*) problems="$problems; sound still listed" ;;
    esac
    [ -z "$(as_app "ls files/sounds/custom")" ] || problems="$problems; file still there"
    done_sheet
    click .save-btn 0 0.6
    row=$(printf 'SELECT data FROM alarms;\n' | sql)
    has "$row" '"time_hour":8'
    has "$row" '"time_minute":36'
    has "$row" "\"date\":\"$(local_date_in 1)"
    has "$row" '"days":[false,false,false,false,false,false,false]'
    has "$row" 'hard'
    case "$row" in
        *challenge.shake*) problems="$problems; the taken away challenge was saved" ;;
    esac
    # The history over your own number of days: only the one within them.
    click .nav-item 1
    on_screen .chip-pill
    click_text .chip-pill 'Your own days'
    on_screen '.custom-days .field'
    set_field '.custom-days .field' 0 7 change
    sleep 0.3
    [ "$(js "return document.querySelectorAll('.event-main').length")" = 1 ] || problems="$problems; $(js "return document.querySelectorAll('.event-main').length") rows in the last 7 days"
    # The reliability screen's buttons leave for the phone's own screens, and come back.
    click .nav-item 2
    on_screen .banner
    click .banner
    on_screen .check-title
    local i=0 n
    n=$(js "return document.querySelectorAll('.btn-small').length")
    # Another app has the keys. A system screen can take a while to start, and Back goes nowhere
    # until it has.
    away() { focused_window | grep -v null | grep -qv "$PKG/"; }
    # And a second Back in the middle of the change closes the screen it comes back to.
    back_in_app() {
        press_back
        wait_for 4 focus_in_app
    }
    # A moment between two of the phone's screens: asked for the same one again at once, a slow
    # device's Settings can freeze, and that is its own trouble.
    while [ "$i" -lt "${n:-0}" ] 2> /dev/null; do
        click .btn-small "$i" 0.3
        wait_for 15 away || problems="$problems; fix button $i stays in the app"
        wait_for 12 back_in_app || problems="$problems; Back from fix button $i leaves the keys with:$(focused_window)"
        on_screen .check-title 20 || problems="$problems; not back from fix button $i"
        sleep 1
        i=$((i + 1))
    done
    click_row "This app in the phone"
    wait_for 15 away || problems="$problems; the app's settings stay in the app"
    wait_for 12 back_in_app || problems="$problems; Back from the app's settings leaves the keys with:$(focused_window)"
    if [ -z "$problems" ]; then
        pass "templates, time, wheels, date, challenge settings, deleting a sound, own days, $n fix buttons"
    else
        fail "${problems#; }"
    fi
}

# Every scenario. On a phone not what needs a restart with somebody there, what changes the
# phone's settings, or what only an emulator can do: name those to run them anyway.
# Over the lock screen, with a PIN: it rings, its screen comes up on top, and ending the alarm
# leaves the phone locked.
# Last of all: Android 12 goes on asking for a PIN that is gone, until it restarts. The next one to
# ask the lock screen to go away gets that prompt, which is no fault of the app's.
s54() {
    if ! on_emulator; then
        note "s54 sets a PIN: not here"
        return
    fi
    scenario s54_over_the_lock_screen
    fresh
    dev locksettings set-pin 1234 > /dev/null
    next_minute 10
    seed a "$(alarm_json a "$T_H" "$T_M" "$EVERY_DAY" null air 0 0 3 1 '[["challenge.scan","{}"]]')"
    arm a
    dead
    screen_off
    locked() { dev "dumpsys trust | grep -q 'deviceLocked=1'" > /dev/null; }
    local problems=""
    wait_for 6 locked || problems="$problems; not locked before the ring"
    if ! await_ring a 30; then
        problems="$problems; no ring: state=$(state a)"
    else
        wait_for 15 focus_in_app || problems="$problems; its screen is not on top: $(focused_window)"
        on_screen .ring-off 15 || problems="$problems; no alarm screen"
        screenshot s54_over_the_lock_screen
        locked || problems="$problems; ringing unlocked the phone"
        click .ring-off
        on_screen button.scan-button 8 || problems="$problems; no scan button"
        # The power button, by accident: what comes back is the challenge, not the PIN.
        dev input keyevent KEYCODE_SLEEP
        sleep 1
        dev input keyevent KEYCODE_WAKEUP
        challenge_is_up() { focus_in_app && is_there button.scan-button; }
        wait_for 8 challenge_is_up || problems="$problems; no challenge after the power button: $(focused_window)"
        click .scan-button 0 0.3
        camera_has_keys() { focused_window | grep -q SentinelScannerActivity; }
        wait_for 8 camera_has_keys || problems="$problems; no camera over the lock screen: $(focused_window)"
        sleep 1
        camera_has_keys || problems="$problems; the camera lost the keys to: $(focused_window)"
        screenshot s54_camera_over_the_lock_screen
        locked || problems="$problems; the camera unlocked the phone"
        as_app "echo a-code > /data/user/0/$PKG/files/sentinel_scan_result.txt"
        wait_for 10 sound_off || problems="$problems; still ringing after the scan"
        await_state a scheduled 5 || problems="$problems; state=$(state a) after the scan"
        wait_for 3 camera_has_keys && press_back
        sleep 1
        locked || problems="$problems; ending the alarm unlocked the phone"
    fi
    dev locksettings clear --old 1234 > /dev/null
    wake
    if [ -z "$problems" ]; then
        pass "rang ${SOUND_AT}s after its time, on top of the lock screen, with the camera too, and the phone stayed locked"
    else
        fail "${problems#; }"
    fi
}

# The promise of this app: with a challenge on it, an alarm has no way out but solving it. Whatever
# is tried, on its ring screen and in the middle of the challenge when it is quiet, it is back on top
# and not over, and nothing is recorded as solved. Only then the challenge turns it off.
s56() {
    scenario s56a_no_way_out_of_the_ring_screen
    fresh
    next_minute
    seed a "$(alarm_json a "$T_H" "$T_M" "$EVERY_DAY" null air 0 0 3 1 '[["challenge.math","{\"difficulty\":\"easy\"}"]]')"
    arm a
    await_ring a 30 || { fail "never started"; return; }
    on_screen .ring-off 10
    local problems="" next
    next=$(next_fire_ms a)
    # Back on top, still on, nothing solved. Android 12 removes the task it hid from recents, which
    # brings the alarm back as a new screen: that counts too.
    back_on_it() { focus_in_app && is_there '.ring-off, .run-screen'; }
    held() { # what was tried
        if ! wait_for 20 back_on_it; then
            problems="$problems; $1: not back on top: $(focused_window)"
            screenshot "${CURRENT}_not_back"
            dev dumpsys window displays | grep -E 'mCurrentFocus|isKeyguard|mDreamingLockscreen|mShowingDream' | sed 's/^/    /' >> "$RESULTS"
        fi
        state_is a firing || problems="$problems; $1: state=$(state a)"
        [ "$(outcomes solved)" = 0 ] || problems="$problems; $1: recorded as solved"
    }
    try_key() { dev input keyevent "$1"; sleep 1.5; }
    try_key KEYCODE_BACK
    held "Back"
    sound_on || problems="$problems; Back: silent"
    try_key KEYCODE_HOME
    held "Home"
    wait_for 10 sound_on || problems="$problems; Home: silent"
    try_key KEYCODE_RECENT_APPS
    held "Recents"
    wait_for 10 sound_on || problems="$problems; Recents: silent"
    if [ -z "$problems" ]; then
        pass "Back, Home and Recents all end up back on it, still ringing"
    else
        fail "${problems#; }"
    fi

    # Into the challenge, where it is quiet: this is where one could go back to sleep.
    into_challenge() {
        on_screen '.ring-off, .run-screen' 15
        is_there .ring-off && click .ring-off 0 0.3
        on_screen .math-expression 10 || problems="$problems; no challenge after $1"
        wait_for 5 sound_off || problems="$problems; still loud in the challenge after $1"
    }
    scenario s56b_no_way_out_of_the_challenge
    problems=""
    into_challenge "the ring screen"
    try_key KEYCODE_BACK
    held "Back"
    into_challenge "Back"
    try_key KEYCODE_HOME
    held "Home"
    into_challenge "Home"
    try_key KEYCODE_RECENT_APPS
    held "Recents"
    if [ -z "$problems" ]; then
        pass "Back, Home and Recents in the challenge all end up back on it"
    else
        fail "${problems#; }"
    fi

    # Killed in the challenge, when it was quiet: it has to come back loud.
    scenario s56c_killed_in_the_challenge
    problems=""
    local p0
    into_challenge "Recents"
    p0=$(main_pid)
    kill_main
    held "killing the app"
    [ "$(main_pid)" != "$p0" ] || problems="$problems; killing the app: same process"
    wait_for 15 sound_on || problems="$problems; killing the app: silent"
    into_challenge "killing the app"
    kill_sentinel
    held "killing the ring"
    wait_for 15 sentinel_up || problems="$problems; killing the ring: it did not come back"
    into_challenge "killing the ring"
    kill_all
    sleep 0.3
    held "killing both"
    wait_for 15 sound_on || problems="$problems; killing both: silent"
    if [ -z "$problems" ]; then
        pass "killed three ways in the challenge, back on it each time and loud"
    else
        fail "${problems#; }"
    fi

    # Turning off the screen in the challenge and going back to sleep.
    scenario s56d_screen_off_in_the_challenge
    problems=""
    shorten idle_secs 4
    # A new challenge reads that: leave this one by doing nothing.
    into_challenge "killing both"
    wait_for 70 sound_on || problems="$problems; doing nothing in the challenge stays silent"
    into_challenge "doing nothing"
    screen_off
    wait_for 5 sound_off || problems="$problems; loud just after the screen went off"
    if wait_for 15 sound_on; then
        [ -z "${SCREEN_WENT_OFF:-}" ] || wake
        held "the screen off"
    else
        problems="$problems; silent with the screen off: back to sleep"
        [ -z "${SCREEN_WENT_OFF:-}" ] || wake
    fi
    shorten idle_secs 60
    if [ -z "$problems" ]; then
        pass "with the screen off it rings again after doing nothing"
    else
        fail "${problems#; }"
    fi

    # And then the one way out.
    scenario s56e_only_solving_ends_it
    problems=""
    into_challenge "the screen off"
    [ "$(next_fire_ms a)" = "$next" ] || note "next ring moved during the ring"
    solve_math
    solved() { state_is a scheduled && [ "$(outcomes solved)" = 1 ]; }
    if wait_for 8 solved && wait_for 5 sound_off; then
        pass "solving it turned it off, and that is recorded"
    else
        fail "${problems#; }; sound=$(sound_on && echo ON || echo off) state=$(state a) solved=$(outcomes solved)"
    fi
}

# A scan challenge during a real ring, with the lock on. The camera stays up past when a frozen
# screen would be restarted. It is quiet, but not for ever: that would be a way out. Closing it is
# quiet again, Home does not get away from it, and what it scans turns the alarm off.
s55() {
    scenario s55_scan_during_a_ring
    fresh
    # Shorter than the camera stays up, and longer than a cold start takes: Android 12 takes the
    # task away on Home and the app comes back from nothing, which on a busy emulator is 6 s.
    shorten ui_timeout_ms 10000
    shorten scan_quiet_secs 8
    next_minute
    seed a "$(alarm_json a "$T_H" "$T_M" "$EVERY_DAY" null air 0 0 3 1 '[["challenge.scan","{}"]]')"
    arm a
    await_ring a 30 || { fail "never started"; return; }
    on_screen .ring-off 10
    click .ring-off 0 0.3
    if ! on_screen button.scan-button 10; then
        fail "no scan button: $(read_text .ring-screen)"
        return
    fi
    local p0 problems=""
    p0=$(main_pid)
    scanner_on_top() { focused_window | grep -q SentinelScannerActivity; }
    click .scan-button 0 0.3
    if ! wait_for 8 scanner_on_top; then
        fail "the camera does not open: $(focused_window)"
        return
    fi
    wait_for 3 sound_off || problems="$problems; loud with the camera just opened"
    wait_for 14 sound_on || problems="$problems; silent under the camera for good, a way out"
    sleep 1
    scanner_on_top || problems="$problems; pushed off the screen: $(focused_window)"
    [ "$(main_pid)" = "$p0" ] || problems="$problems; the app was restarted under the camera"
    press_back
    on_screen button.scan-button 8 || problems="$problems; not back from the camera"
    wait_for 5 sound_off || problems="$problems; still loud with the camera closed"
    click .scan-button 0 0.3
    wait_for 8 scanner_on_top || problems="$problems; the camera does not open again"
    # Past the moment the lock stands aside for an activity of its own to come up.
    sleep 2.5
    go_home
    wait_for 8 focus_in_app || problems="$problems; Home got away from the camera: $(focused_window)"
    # Android 12 removes the task it hid from recents, so the alarm comes back as a new screen.
    on_screen 'button.scan-button, .ring-off' 10
    is_there .ring-off && click .ring-off 0 0.3
    on_screen button.scan-button 8 || problems="$problems; no scan button after Home"
    click .scan-button 0 0.3
    wait_for 8 scanner_on_top || problems="$problems; the camera does not open after Home"
    # What the scanner does with a code, without a code to show an emulator's camera.
    as_app "echo a-code > /data/user/0/$PKG/files/sentinel_scan_result.txt"
    ended() { state_is a scheduled && sound_off; }
    wait_for 8 ended || problems="$problems; the code did not turn it off: state=$(state a) sound=$(sound_on && echo on || echo off)"
    wait_for 5 scanner_on_top && press_back
    if [ -z "$problems" ]; then
        pass "quiet, then loud again under the camera, no way out by Home, and a code turns it off"
    else
        fail "${problems#; }"
    fi
}

# What depends on the size. Another screen on this Android is all the app can tell of a tablet
# or an open foldable, and it is what a new AVD would cost a minute to start for.
with_screen() { # size density scenarios...
    local size=$1 density=$2 s
    shift 2
    dev wm size "$size"
    dev wm density "$density"
    VARIANT="@$size"
    for s in "$@"; do
        "$s"
    done
    VARIANT=""
    dev wm size reset
    dev wm density reset
}
big_screens() {
    with_screen 1600x2560 320 s19 s25 s28 s34 s36
    with_screen 2208x1840 420 s34 s49
}

. "$(dirname "$0")/panels.sh"

ALL="s01 s03 s04 s05 s06 s07 s10 s11 s13 s14 s15 s16 s16b s17 s19 s22 s23a s23b s23c s25 s26x2 s28 s29 s30 s31 s32 s33 s34 s36 s37 s38 s39 s40 s42 s43 s44 s45 s46 s47 s48 s49 s50 s51 s52 s53 s55 s56 s57 big_screens s54"
ON_A_PHONE="s01 s03 s04 s05 s06 s07 s10 s15 s16 s16b s17 s22 s23a s23b s23c s25 s26x2 s28 s29 s30 s31 s32 s33 s36 s37 s38 s39 s42 s43 s45 s46 s47 s49 s50 s51 s52 s53 s55 s56 s57"

: > "$RESULTS"
RUN_START=$SECONDS
open_shells
if on_emulator; then
    dev settings put global auto_time 0
    # What an earlier run that did not get to its end left.
    dev wm size reset
    dev wm density reset
    dev locksettings set-disabled false > /dev/null
    # The one time Android 12 explains full screen, over the scanner, it takes the keys.
    dev settings put secure immersive_mode_confirmations confirmed
    remove_widgets
fi
APP_UID=$(dev cmd package list packages -U "$PKG" | sed -E 's/.*uid:([0-9]+).*/\1/')
if on_emulator; then
    DEFAULT_LIST=$ALL
else
    DEFAULT_LIST=$ON_A_PHONE
fi
# FIRST names scenarios to run before the rest: the heavy ones, while another emulator next to
# this one is still on its light ones.
RUN_LIST="${FIRST:-}"
for s in ${*:-$DEFAULT_LIST}; do
    case " ${SKIP:-} ${FIRST:-} " in
        *" $s "*) continue ;;
    esac
    RUN_LIST="$RUN_LIST $s"
done
for s in $RUN_LIST; do
    "$s"
done
dev am force-stop "$PKG"
echo
echo "===== SUMMARY ($SERIAL) ====="
cat "$RESULTS"
echo "passed: $(grep -c '^PASS' "$RESULTS")  failed: $(grep -c '^FAIL' "$RESULTS")  skipped: $(grep -c '^SKIP' "$RESULTS")  in $(((SECONDS - RUN_START) / 60)) min $(((SECONDS - RUN_START) % 60)) s"
slow=$(sed -nE 's/^[A-Z]+ +([^ ]+) +\[([0-9]+)s\].*/\2 \1/p' "$RESULTS" | awk '$1 > 60 { printf "%s (%ss) ", $2, $1 }')
[ -z "$slow" ] || echo "over a minute: $slow"
