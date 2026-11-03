#!/bin/bash
# Reliability scenarios for AlarmFree, run against one device over adb.
#
#   SERIAL=emulator-5554 devtest/matrix.sh            every scenario
#   SERIAL=emulator-5554 devtest/matrix.sh s02 s03    just these
#
# Each scenario starts from a clean app and judges only what can be observed
# from outside: is the sound on, what state is the alarm in, is it armed.
# Numbers match the scenario matrix in the reliability plan.

cd "$(dirname "$0")/.." || exit 1
. devtest/lib.sh
APK="${APK:-target/dx/alarmfree/debug/android/app/app/build/outputs/apk/debug/app-debug.apk}"

# A repeating alarm fires with no app process alive and the screen off.
s01() {
    scenario s01_repeating_dead
    fresh
    next_minute 45
    seed a "$(alarm_json a "$T_H" "$T_M" "$EVERY_DAY" null)"
    arm a
    dead || note "process still alive"
    screen_off
    if await_sound 40; then
        note "sound after ${SOUND_AT}s"
        sleep 8
        [ "$(state a)" = firing ] && pass "rang, sound $(ring_ms)ms after delivery, $(sound_after_t_ms)ms after its time" || fail "sound but state=$(state a)"
    else
        fail "no sound in 40s, state=$(state a) job=$(job_status a)"
    fi
    save_log
    action dismiss:a
    sleep 3
    local next_ms
    next_ms=$(next_fire_ms a)
    note "after dismiss: state=$(state a) next_in=$(((${next_ms:-0} / 1000 - $(now_s)) / 60))min"
}

# A one-time alarm fires with no app process alive.
s02() {
    scenario s02_one_time_dead
    fresh
    next_minute 45
    seed a "$(alarm_json a "$T_H" "$T_M" "$NO_DAYS" "\"$T_DATE\"")"
    arm a
    dead || note "process still alive"
    screen_off
    if await_sound 40; then
        pass "rang, sound $(ring_ms)ms after delivery"
    else
        fail "no sound in 40s, db_enabled=$(db_enabled a) state=$(state a) job=$(job_status a)"
    fi
    save_log
    action dismiss:a
    sleep 2
}

# A snooze runs out with the process dead (the Snooze button always ends it).
s03() {
    scenario s03_snooze_expires
    fresh
    next_minute 45
    seed a "$(alarm_json a "$T_H" "$T_M" "$EVERY_DAY" null)"
    arm a
    await_sound 30 || { fail "first ring never started"; save_log; return; }
    sleep 3
    action snooze:a
    sleep 3
    [ "$(state a)" = snoozed ] || note "state after snooze: $(state a)"
    sound_on && fail "still sounding after snooze"
    local until
    until=$(($(next_fire_ms a) / 1000))
    kill_all
    T=$until
    if await_sound 40; then
        pass "rang again, sound ${SOUND_AT}s after the snooze ended"
    else
        fail "no re-ring in 40s, state=$(state a) job=$(job_status a)"
    fi
    save_log
    action dismiss:a
    sleep 2
}

# A snoozed one-time alarm survives the app being opened.
s04() {
    scenario s04_one_time_snooze_kept
    fresh
    next_minute 45
    seed a "$(alarm_json a "$T_H" "$T_M" "$NO_DAYS" "\"$T_DATE\"" air 0 0 3 5)"
    arm a
    await_sound 30 || { fail "first ring never started"; save_log; return; }
    sleep 3
    action snooze:a
    sleep 3
    local before after
    before=$(next_fire_ms a)
    kill_all
    sleep 1
    start_app
    sleep 15
    after=$(next_fire_ms a)
    local card
    card=$(read_text .alarm-when)
    if [ "$(state a)" = snoozed ] && [ "$before" = "$after" ] && [[ "$card" == Snoozed* ]]; then
        pass "snooze kept after reopening, the card says '$card'"
    else
        fail "state=$(state a) next ring moved by $(((${after:-0} - ${before:-0}) / 60000))min, db_enabled=$(db_enabled a), card '$card'"
    fi
    save_log
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
    sleep 1
    start_app
    sleep 8
    if [ "$(db_enabled a)" = true ] && [ "$(state a)" = scheduled ]; then
        pass "still on and armed"
    else
        fail "db_enabled=$(db_enabled a) state=$(state a)"
    fi
    save_log
}

# Two alarms in the same minute: dismissing one leaves the other ringing.
s06() {
    scenario s06_two_alarms
    fresh
    next_minute 45
    seed a "$(alarm_json a "$T_H" "$T_M" "$EVERY_DAY" null)"
    seed b "$(alarm_json b "$T_H" "$T_M" "$EVERY_DAY" null)"
    arm a
    await_sound 30 || { fail "never started"; save_log; return; }
    sleep 6
    note "states: a=$(state a) b=$(state b)"
    local first=a second=b
    [ "$(state a)" = firing ] || { first=b; second=a; }
    action "dismiss:$first"
    sleep 5
    if [ "$(state $second)" = firing ] && sound_on; then
        pass "second alarm still ringing"
    else
        fail "after dismissing $first: $second state=$(state $second) sound=$(sound_on && echo on || echo OFF)"
    fi
    save_log
    action "dismiss:$second"
    sleep 2
}

ring_then() { # ring_then name kill_fn
    scenario "$1"
    fresh
    next_minute 45
    seed a "$(alarm_json a "$T_H" "$T_M" "$EVERY_DAY" null)"
    arm a
    await_sound 30 || { fail "never started"; save_log; return 1; }
    sleep 4
    "$2"
    local t0 i=0 silent=0
    t0=$(now_s)
    sleep 1
    while [ "$i" -lt 40 ]; do
        sound_on || silent=$((silent + 1))
        sound_on && [ -n "$(main_pid)" ] && top_is_app && break
        sleep 1
        i=$((i + 1))
    done
    if ! sound_on || [ "$(state a)" != firing ]; then
        fail "after 40s sound=$(sound_on && echo on || echo OFF) state=$(state a) main=$(main_pid)"
    elif ! top_is_app; then
        fail "the sound is back, the screen is not: main=$(main_pid)"
    else
        pass "back after $(($(now_s) - t0))s, silent for ${silent} checks"
    fi
    save_log
    action dismiss:a
    sleep 3
}
# Kills while ringing.
s07a() { ring_then s07a_kill_main_mid_ring kill_main; }
s07b() { ring_then s07b_kill_sentinel_mid_ring kill_sentinel; }
s07c() { ring_then s07c_kill_both_mid_ring kill_all; }

# Overlay denied: nothing may keep the alarm silent.
s09() {
    scenario s09_overlay_denied
    fresh
    next_minute 45
    seed a "$(alarm_json a "$T_H" "$T_M" "$EVERY_DAY" null)"
    dev appops set "$PKG" SYSTEM_ALERT_WINDOW allow
    arm a
    dev appops set "$PKG" SYSTEM_ALERT_WINDOW deny
    dead
    screen_off
    if await_sound 40; then
        pass "rang without overlay, sound ${SOUND_AT}s"
    else
        fail "no sound in 40s, state=$(state a) job=$(job_status a) main=$(main_pid)"
    fi
    save_log
    grant_all
    action dismiss:a
    sleep 2
}

# Notifications denied: the sound must still play.
s10() {
    scenario s10_notifications_denied
    fresh
    next_minute 45
    seed a "$(alarm_json a "$T_H" "$T_M" "$EVERY_DAY" null)"
    dev pm revoke "$PKG" android.permission.POST_NOTIFICATIONS
    arm a
    dead
    screen_off
    if await_sound 40; then
        pass "rang without notifications, sound ${SOUND_AT}s"
    else
        fail "no sound in 40s, state=$(state a)"
    fi
    save_log
    grant_all
    action dismiss:a
    sleep 2
}

# Reboot while ringing counts as dismissed, and must not ring on opening.
s11a() {
    scenario s11a_reboot_mid_ring
    fresh
    next_minute 45
    seed a "$(alarm_json a "$T_H" "$T_M" "$EVERY_DAY" null)"
    arm a
    await_sound 30 || { fail "never started"; save_log; return; }
    sleep 3
    reboot_and_wait || { fail "reboot hung"; return; }
    await_boot_signal || note "no boot signal in 60s"
    sleep 5
    start_app
    sleep 15
    if ! sound_on && [ "$(state a)" = scheduled ]; then
        pass "ended by the reboot, next ring armed"
    else
        fail "sound=$(sound_on && echo on || echo off) state=$(state a)"
    fi
    save_log
}

# A snooze still ahead survives a reboot with no app opened.
s11b() {
    scenario s11b_reboot_snooze_ahead
    fresh
    next_minute 45
    seed a "$(alarm_json a "$T_H" "$T_M" "$EVERY_DAY" null air 0 0 3 4)"
    arm a
    await_sound 30 || { fail "never started"; save_log; return; }
    sleep 3
    action snooze:a
    sleep 3
    T=$(($(next_fire_ms a) / 1000))
    reboot_and_wait || { fail "reboot hung"; return; }
    if await_sound 40; then
        pass "snooze rang after the reboot, ${SOUND_AT}s late, no app opened"
    else
        fail "no ring, state=$(state a)"
    fi
    save_log
    action dismiss:a
    sleep 2
}

# The alarm's time passes during a restart. Under 15 minutes late must ring,
# and either way the next occurrence must be armed without opening the app.
s12() {
    scenario s12_passed_during_restart
    fresh
    next_minute 45
    seed a "$(alarm_json a "$T_H" "$T_M" "$EVERY_DAY" null)"
    arm a
    dead
    # Reboot so the time passes before the device is back.
    until_past $((T - 12))
    reboot_and_wait || { fail "reboot hung"; return; }
    local late=no armed i=0
    while [ "$i" -lt 60 ]; do
        if sound_on; then
            late=yes
            break
        fi
        sleep 1
        i=$((i + 1))
    done
    armed=$(armed_at_ms a)
    if [ "$late" = yes ]; then
        pass "rang late after the restart, $(($(now_s) - T))s after its time"
    else
        fail "no late ring, armed record='${armed}' os_alarm=$(os_has_alarm && echo yes || echo NO), state=$(state a)"
    fi
    save_log
}

# Much later than that it is a miss: nothing rings, it is on record, the next
# occurrence is armed, and all of that without opening the app.
s12b() {
    scenario s12b_missed_during_restart
    fresh
    next_minute 3600
    seed a "$(alarm_json a "$T_H" "$T_M" "$EVERY_DAY" null)"
    arm a
    dead
    make_overdue a 30
    reboot_and_wait || { fail "reboot hung"; return; }
    # The boot signal comes a while after the boot.
    local next i=0
    while [ "$i" -lt 60 ] && [ "$(outcomes missed)" != 1 ]; do
        sleep 1
        i=$((i + 1))
    done
    sleep 3
    next=$(armed_at_ms a)
    if ! sound_on && [ "$(outcomes missed)" = 1 ] && [ "${next:-0}" -gt "$(($(now_s) * 1000))" ]; then
        pass "not rung, recorded as missed, next ring armed, no app opened"
    else
        fail "sound=$(sound_on && echo ON || echo off) missed=$(outcomes missed) armed='$next' state=$(state a)"
    fi
    save_log
}

# The app comes down for an update with an alarm armed: it still rings.
s15() {
    scenario s15_app_update
    fresh
    next_minute 90
    seed a "$(alarm_json a "$T_H" "$T_M" "$EVERY_DAY" null)"
    arm a
    adb_ install -r -g "$APK" > /dev/null
    sleep 5
    if await_sound 40; then
        pass "rang after the update, $(ring_ms)ms after delivery"
    else
        fail "no sound in 40s, state=$(state a) job=$(job_status a)"
    fi
    save_log
    action dismiss:a
    sleep 2
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
    sleep 12
    after=$(armed_at_ms a)
    if [ -n "$before" ] && [ "$before" != "$after" ]; then
        pass "re-armed, moved by $(((after - before) / 60000))min"
    else
        fail "armed time unchanged after zone change ($zone to Asia/Tokyo), main=$(main_pid)"
    fi
    save_log
    set_zone "$zone"
}

# A force-stop removes the alarm, so it passes unnoticed. Opening the app less
# than 15 minutes late rings it.
s16() {
    scenario s16_force_stop
    fresh
    next_minute 45
    seed a "$(alarm_json a "$T_H" "$T_M" "$EVERY_DAY" null)"
    arm a
    dev am force-stop "$PKG"
    until_past $((T + 15))
    sound_on && note "rang despite force-stop"
    start_app
    sleep 15
    if sound_on && [ "$(state a)" = firing ]; then
        pass "rang late on opening"
    else
        fail "sound=$(sound_on && echo on || echo OFF) state=$(state a)"
    fi
    save_log
    action dismiss:a
    sleep 2
}

# Everything against it: no process, screen off, overlay denied, forced idle.
# The time from delivery to sound is what the plan promises to keep under 1 s.
s26() {
    local run=${1:-1}
    scenario "s26_cold_ring_$run"
    fresh
    next_minute 45
    seed a "$(alarm_json a "$T_H" "$T_M" "$EVERY_DAY" null)"
    arm a
    dev appops set "$PKG" SYSTEM_ALERT_WINDOW deny
    dead
    screen_off
    dev dumpsys deviceidle force-idle > /dev/null
    if await_sound 40; then
        local ms
        ms=$(sound_after_t_ms)
        if [ -n "$ms" ] && [ "$ms" -lt 1000 ]; then
            pass "sound ${ms}ms after its time, $(ring_ms)ms after delivery"
        else
            fail "sound ${ms:-?}ms after its time, $(ring_ms)ms after delivery"
        fi
    else
        fail "no sound in 40s, state=$(state a) job=$(job_status a)"
    fi
    save_log
    dev dumpsys deviceidle unforce > /dev/null
    grant_all
    wake
    action dismiss:a
    sleep 2
}
s26x5() { for r in 1 2 3 4 5; do s26 "$r"; done; }

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
    sleep 12
    screenshot s16b_notice
    local missed left
    missed=$(outcomes missed)
    left=$(unseen)
    if ! sound_on && [ "$missed" = 1 ] && [ "$left" = 0 ] && [ "$(state a)" = scheduled ]; then
        pass "recorded as missed and told, next ring armed"
    else
        fail "sound=$(sound_on && echo ON || echo off) missed=$missed unseen=$left state=$(state a)"
    fi
    save_log
}

# The screen cannot come up (phone in use, no overlay permission). The sound
# plays and the alarm still moves on: the next occurrence gets armed, with no UI.
s27() {
    scenario s27_state_machine_without_ui
    fresh
    next_minute 45
    seed a "$(alarm_json a "$T_H" "$T_M" "$EVERY_DAY" null)"
    arm a
    dev appops set "$PKG" SYSTEM_ALERT_WINDOW deny
    dead
    wake
    go_home
    await_sound 40 || { fail "no sound"; save_log; grant_all; return; }
    sleep 8
    local s next ui
    s=$(state a)
    next=$(armed_at_ms a)
    ui=$(top_is_app && echo yes || echo no)
    screenshot s27_heads_up
    if [ "$s" = firing ] && [ "${next:-0}" -gt "$(($(now_s) * 1000))" ]; then
        pass "firing and next ring armed, UI on top: $ui"
    else
        fail "state=$s armed='$next' UI on top: $ui main=$(main_pid)"
    fi
    save_log
    grant_all
    action dismiss:a
    sleep 3
}

# An unplayable custom sound must still make noise.
s17() {
    scenario s17_corrupt_sound
    fresh
    as_app "mkdir -p files/sounds/custom; echo not-audio > files/sounds/custom/bad.mp3"
    next_minute 45
    seed a "$(alarm_json a "$T_H" "$T_M" "$EVERY_DAY" null bad.mp3)"
    arm a
    dead
    if await_sound 40; then
        pass "fell back audibly, ${SOUND_AT}s"
    else
        fail "silent, state=$(state a)"
    fi
    save_log
    action dismiss:a
    sleep 2
}

# Volume keys can't turn a ringing alarm down.
s19() {
    scenario s19_volume_keys
    fresh
    next_minute 45
    seed a "$(alarm_json a "$T_H" "$T_M" "$EVERY_DAY" null)"
    arm a
    local before v0 v1 v2 i=0
    before=$(alarm_volume)
    set_alarm_volume 1
    await_sound 30 || { fail "never started"; set_alarm_volume "$before"; save_log; return; }
    sleep 4
    v0=$(alarm_volume)
    while [ "$i" -lt 8 ]; do
        dev input keyevent KEYCODE_VOLUME_DOWN
        i=$((i + 1))
    done
    sleep 3
    v1=$(alarm_volume)
    save_log
    action dismiss:a
    sleep 3
    v2=$(alarm_volume)
    set_alarm_volume "$before"
    if [ "${v0:-0}" -gt 1 ] && [ "${v1:-0}" -ge "$v0" ] && [ "$v2" = 1 ]; then
        pass "raised from 1 to $v0, held at $v1 against the keys, back to $v2 after"
    else
        fail "raised to $v0, after the keys $v1, after dismissing $v2 (should be 1)"
    fi
}

# The screen stays on while ringing, without any touch.
s20() {
    scenario s20_screen_stays_on
    fresh
    local old
    old=$(dev settings get system screen_off_timeout)
    dev settings put system screen_off_timeout 15000
    next_minute 45
    seed a "$(alarm_json a "$T_H" "$T_M" "$EVERY_DAY" null)"
    arm a
    await_sound 30 || { fail "never started"; dev settings put system screen_off_timeout "$old"; save_log; return; }
    sleep 30
    local w
    w=$(dev dumpsys power | grep -m1 -E 'mWakefulness=' | sed 's/.*=//')
    [ "$w" = Awake ] && pass "screen awake after 30s without touch" || fail "wakefulness=$w"
    dev settings put system screen_off_timeout "$old"
    save_log
    action dismiss:a
    sleep 2
}

# A dark-mode switch mid-ring must not end the process.
s21() {
    scenario s21_dark_mode_mid_ring
    fresh
    next_minute 45
    seed a "$(alarm_json a "$T_H" "$T_M" "$EVERY_DAY" null)"
    arm a
    await_sound 30 || { fail "never started"; save_log; return; }
    sleep 5
    local p0 p1
    p0=$(main_pid)
    dev cmd uimode night yes > /dev/null
    sleep 6
    p1=$(main_pid)
    dev cmd uimode night no > /dev/null
    [ "$p0" = "$p1" ] && pass "same process" || fail "main process restarted ($p0 to $p1)"
    save_log
    sleep 3
    action dismiss:a
    sleep 2
}

# Rebooted and never unlocked: the alarm rings, and the backup screen is the way
# out, because the app's own screen cannot come up yet.
s13() {
    scenario s13_before_first_unlock
    fresh
    next_minute 180
    seed a "$(alarm_json a "$T_H" "$T_M" "$EVERY_DAY" null)"
    arm a
    dev locksettings set-pin 1234 > /dev/null
    dead
    reboot_and_wait || { fail "reboot hung"; return; }
    local locked
    locked=$(dev dumpsys user | grep -c 'RUNNING_LOCKED')
    if await_sound 60; then
        sleep 4
        screenshot s13_backup_screen
        if top_is_backup; then
            solve_backup 20
            sleep 3
            if sound_on; then
                fail "still ringing after the challenge, top: $(dev dumpsys activity activities | grep -m1 ResumedActivity)"
            else
                pass "rang ${SOUND_AT}s late while locked (locked users: $locked), stopped by the backup screen"
            fi
        else
            fail "rang, but the backup screen is not on top: $(dev dumpsys activity activities | grep -m1 ResumedActivity)"
        fi
    else
        fail "no sound in 60s, locked users: $locked, state=$(state a) job=$(job_status a) main=$(main_pid) $(dev dumpsys package "$PKG" | grep -m1 -oE 'stopped=[a-z]+')"
    fi
    save_log
    dev locksettings clear --old 1234 > /dev/null
}

# The UI keeps freezing: after two failures the backup screen takes over.
s22b() {
    scenario s22b_ui_keeps_failing
    fresh
    next_minute 45
    seed a "$(alarm_json a "$T_H" "$T_M" "$EVERY_DAY" null)"
    arm a
    await_sound 30 || { fail "never started"; save_log; return; }
    sleep 6
    local tries=0 i
    while [ "$tries" -lt 2 ]; do
        local p0 p1
        p0=$(main_pid)
        action hang_ui
        i=0
        while [ "$i" -lt 70 ]; do
            p1=$(main_pid)
            [ -n "$p1" ] && [ "$p1" != "$p0" ] && break
            sleep 1
            i=$((i + 1))
        done
        note "frozen UI $((tries + 1)) replaced after ${i}s"
        sleep 8
        tries=$((tries + 1))
    done
    sleep 3
    screenshot s22b_backup
    if top_is_backup; then
        solve_backup 20
        sleep 8
        if ! sound_on && [ "$(state a)" = scheduled ] && [ "$(outcomes backup)" = 1 ]; then
            pass "backup screen took over, solving it ended the alarm"
        else
            fail "after solving: sound=$(sound_on && echo ON || echo off) state=$(state a) backup=$(outcomes backup)"
        fi
    else
        fail "backup screen not on top: $(dev dumpsys activity activities | grep -m1 ResumedActivity)"
    fi
    save_log
    action dismiss:a
    sleep 2
}

# A frozen UI gets restarted by the guardian; the alarm carries on.
s22() {
    scenario s22_hung_ui
    fresh
    next_minute 45
    seed a "$(alarm_json a "$T_H" "$T_M" "$EVERY_DAY" null)"
    arm a
    await_sound 30 || { fail "never started"; save_log; return; }
    sleep 6
    local p0 p1 i=0
    p0=$(main_pid)
    action hang_ui
    while [ "$i" -lt 45 ]; do
        p1=$(main_pid)
        [ -n "$p1" ] && [ "$p1" != "$p0" ] && break
        sleep 1
        i=$((i + 1))
    done
    if [ "$p0" != "$p1" ] && [ -n "$p1" ]; then
        sleep 8
        sound_on && pass "restarted after ${i}s, still ringing" || fail "restarted but silent"
    else
        fail "UI never restarted in 45s"
    fi
    save_log
    action dismiss:a
    sleep 2
}

# Fade-in survives a cold start: the first playback keeps its fade.
s24() {
    scenario s24_fade_in_cold_start
    fresh
    next_minute 45
    seed a "$(alarm_json a "$T_H" "$T_M" "$EVERY_DAY" null air 0 30)"
    arm a
    dead
    await_sound 40 || { fail "no sound"; save_log; return; }
    sleep 6
    local plays
    plays=$(adb_ logcat -d -s MobileSentinel.FiringAudio:I | tr -d '\r' | grep 'playing ')
    note "$plays"
    local n
    n=$(echo "$plays" | grep -c 'playing ')
    if [ "$n" = 1 ] && echo "$plays" | grep -q 'fadeIn=30000ms'; then
        pass "one playback with fade"
    else
        fail "$n playbacks, fade kept: $(echo "$plays" | tail -1 | grep -c 'fadeIn=30000ms')"
    fi
    save_log
    action dismiss:a
    sleep 2
}

# The ring limit, with a short one. No touch: it stops, and the alarm moves on.
s23a() {
    scenario s23a_limit_stops
    fresh
    next_minute 45
    seed a "$(alarm_json a "$T_H" "$T_M" "$EVERY_DAY" null air 0 0 3 1 '[]' 40)"
    arm a
    await_sound 30 || { fail "never started"; save_log; return; }
    if await_silence 70; then
        sleep 6
        screenshot s23a_after_limit
        local s j
        s=$(state a)
        j=$(job_status a)
        if [ "$STOPPED_AT" -ge 38 ] && [ "$s" = scheduled ] && [ "$j" = pending ]; then
            pass "stopped ${STOPPED_AT}s after its time, next ring armed"
        else
            fail "stopped at ${STOPPED_AT}s, state=$s job=$j"
        fi
    else
        fail "still ringing after 70s"
    fi
    save_log
}

# A touch restarts the limit.
s23b() {
    scenario s23b_limit_restarts_on_touch
    fresh
    next_minute 45
    seed a "$(alarm_json a "$T_H" "$T_M" "$EVERY_DAY" null air 0 0 3 1 '[]' 50)"
    arm a
    await_sound 30 || { fail "never started"; save_log; return; }
    until_past $((T + 40))
    local size
    size=$(dev wm size | tail -1 | sed 's/.*: //')
    # On the alarm's clock, where a touch does nothing else.
    dev input tap $((${size%x*} / 2)) $((${size#*x} / 5))
    until_past $((T + 60))
    if ! sound_on; then
        fail "stopped despite the touch, state=$(state a)"
        save_log
        return
    fi
    if await_silence 110; then
        [ "$STOPPED_AT" -ge 85 ] && pass "ran on after the touch, stopped at ${STOPPED_AT}s" || fail "stopped too early, ${STOPPED_AT}s"
    else
        fail "still ringing after 110s"
    fi
    save_log
    action dismiss:a
    sleep 2
}

# With the app away: the ring still stops, and the app deals with it when it can.
s23c() {
    scenario s23c_limit_app_away
    fresh
    next_minute 45
    seed a "$(alarm_json a "$T_H" "$T_M" "$NO_DAYS" "\"$T_DATE\"" air 0 0 3 1 '[]' 40)"
    arm a
    dev appops set "$PKG" SYSTEM_ALERT_WINDOW deny
    dead
    wake
    go_home
    await_sound 30 || { fail "never started"; save_log; return; }
    if ! await_silence 70; then
        fail "still ringing after 70s"
        save_log
        return
    fi
    grant_all
    sleep 1
    start_app
    sleep 8
    if [ "$(job_status a)" != active ] && [ "$(db_enabled a)" = false ] && ! sound_on; then
        pass "stopped at ${STOPPED_AT}s, one-time alarm off after opening"
    else
        fail "after opening: state=$(state a) job=$(job_status a) enabled=$(db_enabled a)"
    fi
    screenshot s23c_after_opening
    save_log
}

# Forever means forever, past any limit the others would have hit.
s23d() {
    scenario s23d_limit_forever
    fresh
    next_minute 45
    seed a "$(alarm_json a "$T_H" "$T_M" "$EVERY_DAY" null air 0 0 3 1 '[]' null)"
    arm a
    await_sound 30 || { fail "never started"; save_log; return; }
    until_past $((T + 70))
    sound_on && pass "still ringing after 70s" || fail "stopped, state=$(state a)"
    save_log
    action dismiss:a
    sleep 2
}

# Back from the editor stays in the app.
s25() {
    scenario s25_back_navigation
    fresh
    next_minute 3600
    seed a "$(alarm_json a "$T_H" "$T_M" "$EVERY_DAY" null)"
    arm a
    sleep 2
    local p0 problems=""
    p0=$(main_pid)
    # A sheet closes first, and the screen stays.
    click .alarm-menu
    on_screen .sheet || problems="$problems; no menu"
    press_back
    on_screen .sheet && problems="$problems; Back left the menu open"
    on_screen .home-header || problems="$problems; Back from the menu left Home"
    # Then the screen.
    click .alarm-main
    on_screen .time-big || problems="$problems; no editor"
    click .row-btn
    on_screen .sheet || problems="$problems; no editor sheet"
    press_back
    on_screen .sheet && problems="$problems; Back left the editor sheet open"
    on_screen .time-big || problems="$problems; Back from the sheet left the editor"
    press_back
    on_screen .home-header || problems="$problems; Back from the editor did not reach Home"
    # With changes it asks first.
    click .alarm-main
    click .time-shift
    press_back
    on_screen .sheet || problems="$problems; no question about discarding"
    local under
    under=$(covered)
    [ -z "$under" ] || problems="$problems; in the question: $under"
    press_back
    on_screen .time-big || problems="$problems; Back from the question left the editor"
    click .icon-btn
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
    top_is_app && problems="$problems; Back on Home stayed on top"
    [ "$(main_pid)" = "$p0" ] || problems="$problems; main $p0 to $(main_pid)"
    start_app
    sleep 2
    on_screen .home-header || problems="$problems; not back on Home"
    screenshot s25_end
    if [ -z "$problems" ]; then
        pass "sheet, screen, Home, background, same process"
    else
        fail "${problems#; }"
    fi
    save_log
}

# What sticks out of the screen, if anything.
NOTHING_CLIPPED='var w = innerWidth; var bad = [...document.querySelectorAll(".screen *")].filter(e => { var r = e.getBoundingClientRect(); return r.width > 0 && (r.right > w + 1 || r.left < -1) && !e.closest(".chip-scroll, .alarm-swipe"); }).map(e => e.className.baseVal === undefined ? e.className : "svg").slice(0, 3); return bad.join(",");'

# Turning it off through the screens, solving a sum, and landing on history.
s28() {
    scenario s28_solve_a_challenge
    fresh
    next_minute 45
    seed a "$(alarm_json a "$T_H" "$T_M" "$EVERY_DAY" null air 0 0 3 1 '[["challenge.math","{\"difficulty\":\"easy\"}"]]')"
    arm a
    await_sound 90 || { fail "never started"; save_log; return; }
    sleep 4
    screenshot s28_ringing
    local clipped
    clipped=$(js "$NOTHING_CLIPPED")
    click .ring-off 0 1.5
    screenshot s28_challenge
    clipped="$clipped$(js "$NOTHING_CLIPPED")"
    [ -z "$clipped" ] || { fail "clipped: $clipped"; save_log; action dismiss:a; return; }
    local sum answer quiet=no
    sound_on || quiet=yes
    sum=$(read_text .math-expression)
    answer=$(($(echo "$sum" | sed 's/×/*/; s/ //g')))
    local digit digits="$answer"
    while [ -n "$digits" ]; do
        digit=${digits:0:1}
        digits=${digits:1}
        js "[...document.querySelectorAll('.math-key')].find(k => k.textContent === '$digit').click(); return 'ok'" > /dev/null
    done
    sleep 4
    screenshot s28_after
    if [ "$quiet" = yes ] && ! sound_on && [ "$(state a)" = scheduled ] && [ "$(outcomes solved)" = 1 ] && on_screen .summary; then
        pass "$sum = $answer turned it off, recorded, on history"
    else
        fail "quiet during the challenge: $quiet, sound=$(sound_on && echo ON || echo off) state=$(state a) solved=$(outcomes solved) sum='$sum'"
    fi
    save_log
    action dismiss:a
}

# Doing nothing in a challenge: it goes back to ringing.
s28b() {
    scenario s28b_idle_challenge_rings_again
    fresh
    next_minute 45
    seed a "$(alarm_json a "$T_H" "$T_M" "$EVERY_DAY" null air 0 0 3 1 '[["challenge.math","{\"difficulty\":\"easy\"}"]]')"
    arm a
    await_sound 90 || { fail "never started"; save_log; return; }
    sleep 4
    click .ring-off 0 2
    sound_on && { fail "still loud in the challenge"; save_log; action dismiss:a; return; }
    local i=0
    while [ "$i" -lt 75 ] && ! sound_on; do
        sleep 1
        i=$((i + 1))
    done
    if sound_on && on_screen .ring-off; then
        pass "ringing again after ${i}s"
    else
        fail "after ${i}s: sound=$(sound_on && echo ON || echo off)"
    fi
    save_log
    action dismiss:a
}

# The wake-up check: a little later it asks again, as loud as any alarm.
s29() {
    scenario s29_wake_up_check
    fresh
    next_minute 45
    ALARM_EXTRA=',"wake_check_minutes":1' seed a "$(ALARM_EXTRA=',"wake_check_minutes":1' alarm_json a "$T_H" "$T_M" "$EVERY_DAY" null air 0 0 0 1)"
    arm a
    await_sound 90 || { fail "never started"; save_log; return; }
    sleep 4
    click .ring-off 0 2
    local hidden
    hidden=$(db_count '"hidden":true')
    [ "$hidden" = 1 ] || { fail "no check waiting: $hidden"; save_log; return; }
    sound_on && { fail "still ringing after turning it off"; save_log; return; }
    T=$(now_s)
    await_sound 130 || { fail "the check never rang"; save_log; return; }
    sleep 4
    screenshot s29_check
    local label
    label=$(read_text .ring-label)
    click .ring-off 0 3
    if ! sound_on && [ "$(db_count '"hidden":true')" = 0 ] && [ "$(outcomes solved)" = 1 ] && [ "$label" = "Are you up?" ]; then
        pass "rang again after ${SOUND_AT}s, and is gone after confirming"
    else
        fail "sound=$(sound_on && echo ON || echo off) hidden=$(db_count '"hidden":true') solved=$(outcomes solved) label='$label'"
    fi
    save_log
}

# A nap in two taps, and Undo.
s30() {
    scenario s30_quick_alarm_and_undo
    fresh
    start_app
    sleep 5
    click .chip-slim 0 1
    local made toast_text
    made=$(db_count '"delete_after_ring":true')
    toast_text=$(read_text .toast-text)
    os_has_alarm || { fail "not armed: made=$made"; save_log; return; }
    click .toast-undo 0 1.5
    local left
    left=$(db_count '"delete_after_ring":true')
    if [ "$made" = 1 ] && [ "$left" = 0 ]; then
        pass "made ($toast_text) and undone"
    else
        fail "made=$made left=$left"
    fi
    save_log
}

# Pausing everything moves the next ring past the date, and Undo brings it back.
s31() {
    scenario s31_pause_all
    fresh
    next_minute 3600
    seed a "$(alarm_json a "$T_H" "$T_M" "$EVERY_DAY" null)"
    arm a
    sleep 2
    local before during after
    before=$(armed_at_ms a)
    click .round-icon-btn 0
    click .sheet-item 1 2
    during=$(armed_at_ms a)
    local text_paused
    text_paused=$(read_text .alarm-when)
    click .toast-undo 0 2
    after=$(armed_at_ms a)
    local week=$((6 * 24 * 3600 * 1000))
    if [ "$during" -ge "$((before + week))" ] && [ "$after" = "$before" ]; then
        pass "moved past the week ($text_paused), undone"
    else
        fail "armed: before $before, during $during, after $after"
    fi
    save_log
}

# The test alarm rings for real, and leaves nothing behind.
s32() {
    scenario s32_test_alarm
    fresh
    start_app
    sleep 5
    click .nav-item 2
    click .banner
    screenshot s32_reliability
    click '.save-bar .btn-primary' 0 2
    [ "$(db_count '"hidden":true')" = 1 ] || { fail "no test alarm"; save_log; return; }
    go_home
    T=$(now_s)
    await_sound 150 || { fail "it never rang"; save_log; return; }
    sleep 5
    click .ring-off 0 3
    if ! sound_on && [ "$(db_count '"hidden":true')" = 0 ] && [ "$(outcomes solved)" = 0 ]; then
        pass "rang ${SOUND_AT}s later, nothing left, nothing in history"
    else
        fail "sound=$(sound_on && echo ON || echo off) hidden=$(db_count '"hidden":true') solved=$(outcomes solved)"
    fi
    save_log
}

# The quiet notification before an alarm.
s33() {
    scenario s33_notification_before_alarm
    fresh
    use_settings '"Dark"' '"English"' '"setup_done":true,"language_follows_phone":false,"notify_before_minutes":5'
    next_minute 340
    seed a "$(alarm_json a "$T_H" "$T_M" "$EVERY_DAY" null)"
    arm a
    # A loose OS alarm, which Android may deliver late: what counts is that it comes
    # before the ring.
    local before
    while [ "$(now_s)" -lt $((T - 15)) ]; do
        dev dumpsys notification --noredact | grep -q 'sentinel_reminders' && break
        sleep 2
    done
    before=$((T - $(now_s)))
    if dev dumpsys notification --noredact | grep -q 'sentinel_reminders'; then
        pass "shown ${before}s before the ring"
    else
        fail "not shown, ${before}s before the ring: $(dev dumpsys alarm | grep -m1 -A3 'reminder:a' | tr -s ' ' | cut -c1-160)"
    fi
    save_log
}

# Turning the phone and unfolding it mid-ring: it rings on, and the screen fits.
s35() {
    scenario s35_rotate_and_unfold_mid_ring
    fresh
    next_minute 45
    seed a "$(alarm_json a "$T_H" "$T_M" "$EVERY_DAY" null)"
    arm a
    await_sound 90 || { fail "never started"; save_log; return; }
    sleep 5
    local p0 problems=""
    p0=$(main_pid)
    dev settings put system accelerometer_rotation 0
    dev settings put system user_rotation 1
    sleep 4
    screenshot s35_rotated
    [ "$(main_pid)" = "$p0" ] || problems="$problems; rotating ended the process"
    sound_on || problems="$problems; silent after rotating"
    on_screen .ring-off || problems="$problems; no alarm screen after rotating"
    dev settings put system user_rotation 0
    # Unfolding, as a screen size.
    local size
    size=$(dev wm size | tail -1 | sed 's/.*: //')
    dev wm size 2200x2480
    sleep 4
    screenshot s35_unfolded
    [ "$(main_pid)" = "$p0" ] || problems="$problems; resizing ended the process"
    sound_on || problems="$problems; silent after resizing"
    on_screen .ring-off || problems="$problems; no alarm screen after resizing"
    dev wm size reset
    dev settings put system accelerometer_rotation 1
    sleep 2
    if [ -z "$problems" ]; then
        pass "same process, still ringing ($size and 2200x2480)"
    else
        fail "${problems#; }"
    fi
    save_log
    action dismiss:a
    sleep 2
}

# Nothing sticks out of the screen in any language, also with large text.
s34() {
    scenario s34_nothing_clipped
    local check=$NOTHING_CLIPPED
    local problems="" scale locale screen result
    for scale in 1.0 1.3; do
        dev settings put system font_scale "$scale"
        for locale in English Dutch Spanish French German; do
            fresh
            use_settings '"Dark"' "\"$locale\"" '"setup_done":true,"language_follows_phone":false'
            next_minute 3600
            seed a "$(alarm_json a "$T_H" "$T_M" "$EVERY_DAY" null)"
            arm a
            sleep 2
            result=$(js "$check")
            [ -z "$result" ] || problems="$problems $locale/$scale/home: $result;"
            click .alarm-main
            result=$(js "$check")
            [ -z "$result" ] || problems="$problems $locale/$scale/editor: $result;"
            [ "$scale" = 1.3 ] && screenshot "s34_editor_$locale"
            press_back
            click .nav-item 1
            result=$(js "$check")
            [ -z "$result" ] || problems="$problems $locale/$scale/history: $result;"
            click .nav-item 2
            result=$(js "$check")
            [ -z "$result" ] || problems="$problems $locale/$scale/settings: $result;"
            [ "$scale" = 1.3 ] && screenshot "s34_settings_$locale"
            click .banner
            result=$(js "$check")
            [ -z "$result" ] || problems="$problems $locale/$scale/reliability: $result;"
        done
    done
    dev settings put system font_scale 1.0
    if [ -z "$problems" ]; then
        pass "5 languages, normal and large text"
    else
        fail "$problems"
    fi
    save_log
}

# Where the middle of an element is on the screen, in device pixels. The page counts from
# the top of the WebView, which can start below the status bar.
webview_top() {
    dev uiautomator dump /sdcard/af_ui.xml > /dev/null 2>&1
    dev cat /sdcard/af_ui.xml | tr '>' '\n' | grep -m1 'android.webkit.WebView' | sed -E 's/.*bounds="\[[0-9]+,([0-9]+)\].*/\1/'
}
screen_point() { # selector [index]
    local top
    top=$(webview_top)
    js "var r = document.querySelectorAll('$1')[${2:-0}].getBoundingClientRect(); return Math.round((r.left + r.width / 2) * devicePixelRatio) + ' ' + (Math.round((r.top + r.height / 2) * devicePixelRatio) + ${top:-0});"
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
    sleep 0.9
    for step in 1 2 3 4; do
        dev input motionevent MOVE "$x" $((y + $3 * step / 4))
    done
    sleep 0.4
    dev input motionevent UP "$x" $((y + $3))
    sleep 1
}
# The distance between two elements of a kind, in device pixels.
gap_between() { # selector from to
    js "var c = document.querySelectorAll('$1'); return Math.round((c[$3].getBoundingClientRect().top - c[$2].getBoundingClientRect().top) * devicePixelRatio);"
}
texts_of() { js "return [...document.querySelectorAll('$1')].map(e => e.textContent).join(' ')"; }
click_row() { # the row of a list with this title
    js "var e = [...document.querySelectorAll('.row-btn')].find(e => e.textContent.includes('$1')); if (!e) return 'no row $1'; e.click(); return 'ok'" > /dev/null
    sleep 0.8
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
    sleep 2
    local before after again gap
    before=$(texts_of .alarm-label)
    gap=$(gap_between .alarm-card 0 2)
    hold_and_drag .alarm-main 0 $((gap + 30))
    screenshot s36_dragged
    after=$(texts_of .alarm-label)
    dev am force-stop "$PKG"
    sleep 1
    start_app
    sleep 5
    again=$(texts_of .alarm-label)
    local problems=""
    [ "$before" = "a b c" ] || problems="$problems; started as '$before'"
    [ "$after" = "b c a" ] || problems="$problems; after dragging '$after'"
    [ "$again" = "$after" ] || problems="$problems; after restarting '$again'"
    # In the editor. The first card is now b, a is the last.
    click .alarm-main 2
    click_row Challenges
    local rows_before rows_after
    rows_before=$(texts_of .challenge-row)
    gap=$(gap_between .challenge-row 0 1)
    hold_and_drag .drag-handle 0 $((gap + 20))
    screenshot s36_dragged_challenge
    rows_after=$(texts_of .challenge-row)
    [ -n "$rows_before" ] && [ "$rows_before" != "$rows_after" ] || problems="$problems; challenges stayed: '$rows_after'"
    if [ -z "$problems" ]; then
        pass "alarms $before -> $after, kept after a restart, challenges too"
    else
        fail "$problems"
    fi
    save_log
}

# Swiping a card aside shows skip and delete; both have an undo.
s37() {
    scenario s37_swipe_a_card
    fresh
    next_minute 3600
    seed a "$(alarm_json a "$T_H" "$T_M" "$EVERY_DAY" null)"
    arm a
    sleep 2
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
        sleep 1.2
    }
    local problems=""
    swipe_card
    screenshot s37_swiped
    [ "$(js "return document.querySelector('.alarm-card').scrollLeft > 0 ? 'y' : 'n'")" = y ] || problems="$problems; did not move"
    [ "$(js "return document.querySelector('.swipe-action.danger').getBoundingClientRect().right <= innerWidth ? 'y' : 'n'")" = y ] || problems="$problems; delete not in view"
    click .swipe-action 0 1.5
    [ "$(db_count '"skip_until_ms":null')" = 0 ] || problems="$problems; not skipped"
    [ "$(js "return document.querySelector('.alarm-card').scrollLeft === 0 ? 'y' : 'n'")" = y ] || problems="$problems; stayed aside"
    click .toast-undo 0 1.5
    [ "$(db_count '"skip_until_ms":null')" = 1 ] || problems="$problems; skip not undone"
    swipe_card
    click .swipe-action.danger 0 1.5
    [ "$(db_count '"id":"a"')" = 0 ] || problems="$problems; not deleted"
    on_screen .alarm-card && problems="$problems; card still there"
    click .toast-undo 0 1.5
    [ "$(db_count '"id":"a"')" = 1 ] || problems="$problems; delete not undone"
    on_screen .alarm-card || problems="$problems; card not back"
    os_has_alarm || problems="$problems; not armed after undo"
    # A swipe does not get in the way of an ordinary tap.
    click .alarm-main
    on_screen .time-big || problems="$problems; no editor"
    if [ -z "$problems" ]; then
        pass "skip and delete by swiping, both undone"
    else
        fail "$problems"
    fi
    save_log
}

# The accent colour, and a name for a sound of one's own.
s38() {
    scenario s38_accent_and_sound_name
    fresh
    as_app "mkdir -p files/sounds/custom && cp files/sounds/default/air.ogg files/sounds/custom/mytune.ogg"
    seed a "$(alarm_json a 7 0 "$EVERY_DAY" null)"
    arm a
    sleep 2
    local problems="" accent
    click .nav-item 2
    click_row Theme
    screenshot s38_swatches
    [ "$(js "return document.querySelectorAll('.swatch').length")" = 5 ] || problems="$problems; no swatches"
    click .swatch 2
    accent="getComputedStyle(document.querySelector('.app-root')).getPropertyValue('--accent').trim()"
    [ "$(js "return $accent")" = "#b9a0ff" ] || problems="$problems; colour is $(js "return $accent")"
    dev am force-stop "$PKG"
    sleep 1
    start_app
    sleep 5
    [ "$(js "return $accent")" = "#b9a0ff" ] || problems="$problems; not kept: $(js "return $accent")"
    screenshot s38_purple
    # Its own name.
    click .alarm-main
    click_row Sound
    local names
    names=$(texts_of .radio-text)
    case "$names" in
        *mytune*) ;;
        *) problems="$problems; own sound not listed: $names" ;;
    esac
    click '.radio-row .icon-btn' 0
    js "var i = document.querySelector('.radio-row .field'); Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value').set.call(i, 'Morning'); i.dispatchEvent(new Event('input', { bubbles: true })); return 'ok'" > /dev/null
    sleep 0.5
    screenshot s38_renaming
    click '.radio-row .icon-btn' 0 1
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
    save_log
}

# An alarm can vibrate first and go to full brightness.
s39() {
    scenario s39_vibrate_first_and_brightness
    fresh
    next_minute 45
    local extra=',"sound_delay_secs":15,"max_brightness":true'
    ALARM_EXTRA=$extra seed a "$(ALARM_EXTRA=$extra alarm_json a "$T_H" "$T_M" "$EVERY_DAY" null air 0 0 0 1)"
    arm a
    wait_ring_started() {
        local i=0
        while [ "$i" -lt 240 ]; do
            adb_ logcat -d 2> /dev/null | grep -q 'ringing a:' && return 0
            sleep 0.5
            i=$((i + 1))
        done
        return 1
    }
    wait_ring_started || { fail "never started"; save_log; return; }
    local started problems="" i=0 heard=""
    started=$(now_s)
    sound_on && problems="$problems; sound at once"
    sleep 8
    sound_on && problems="$problems; sound after 8s"
    while [ "$i" -lt 40 ]; do
        if sound_on; then
            heard=$(($(now_s) - started))
            break
        fi
        sleep 0.5
        i=$((i + 1))
    done
    [ -n "$heard" ] && [ "$heard" -ge 12 ] && [ "$heard" -le 20 ] || problems="$problems; sound after '${heard}'s"
    local bright=no
    dev dumpsys window | grep -q 'sbrt=1.0' && bright=yes
    [ "$bright" = yes ] || problems="$problems; not at full brightness"
    click .ring-off 0 3
    sound_on && problems="$problems; still ringing"
    dev dumpsys window | grep -q 'sbrt=1.0' && problems="$problems; brightness stayed"
    if [ -z "$problems" ]; then
        pass "sound ${heard}s after the vibration, full brightness until over"
    else
        fail "$problems"
    fi
    save_log
    action dismiss:a
}

# The home-screen widget: placed from the settings, says the next ring, follows changes.
s40() {
    scenario s40_home_widget
    fresh
    next_minute 3600
    seed a "$(alarm_json a "$T_H" "$T_M" "$EVERY_DAY" null)"
    arm a
    sleep 2
    ui_texts() {
        dev uiautomator dump /sdcard/af_ui.xml > /dev/null 2>&1
        dev cat /sdcard/af_ui.xml | tr '>' '\n' | grep -oE ' text="[^"]+"' | sed -E 's/ text="(.*)"/\1/'
    }
    click .nav-item 2
    on_screen .row-btn || { fail "no settings"; save_log; return; }
    click_row "Add widget"
    sleep 3
    screenshot s40_asked
    # The launcher asks the user first.
    dev uiautomator dump /sdcard/af_ui.xml > /dev/null 2>&1
    local add
    add=$(dev cat /sdcard/af_ui.xml | tr '>' '\n' | grep -iE ' text="(add|add to home screen|add automatically)"' | grep -m1 'clickable="true"' | sed -E 's/.*bounds="\[([0-9]+),([0-9]+)\]\[([0-9]+),([0-9]+)\].*/\1 \2 \3 \4/')
    if [ -z "$add" ]; then
        fail "the launcher did not ask: $(ui_texts | tr '\n' '|')"
        save_log
        return
    fi
    local x1 y1 x2 y2
    read -r x1 y1 x2 y2 <<< "$add"
    dev input tap $(((x1 + x2) / 2)) $(((y1 + y2) / 2))
    sleep 3
    local problems=""
    dev dumpsys appwidget | grep -q 'SentinelHomeWidget' || problems="$problems; not registered"
    go_home
    sleep 3
    screenshot s40_on_home
    local minute texts
    minute=$(printf ':%02d' "$T_M")
    texts=$(ui_texts | tr '\n' '|')
    case "$texts" in
        *"$minute"*) ;;
        *) problems="$problems; no time on it: $texts" ;;
    esac
    # Follows a change, without the widget being asked.
    start_app
    sleep 3
    click .nav-item 0
    click .alarm-switch 0 2
    go_home
    sleep 3
    texts=$(ui_texts | tr '\n' '|')
    case "$texts" in
        *"No alarm set"*) ;;
        *) problems="$problems; still old: $texts" ;;
    esac
    screenshot s40_none
    if [ -z "$problems" ]; then
        pass "placed, shows the next ring, follows a change"
    else
        fail "$problems"
    fi
    save_log
}

# The text of the open panel that lies under something else, at the end of a long panel.
covered() {
    js 'var s = document.querySelector(".sheet"); if (s) s.scrollTop = s.scrollHeight; return "ok"' > /dev/null
    sleep 0.3
    js 'var s = document.querySelector(".sheet"), bad = []; if (!s) return ""; var box = s.getBoundingClientRect(); s.querySelectorAll("*").forEach(e => { if (e.children.length || !e.textContent.trim()) return; var r = e.getBoundingClientRect(), x = r.left + r.width / 2, y = r.top + r.height / 2; if (r.width < 2 || r.height < 2 || y < box.top || y > box.bottom) return; var on = document.elementFromPoint(x, y); if (on && on !== e && !e.contains(on) && !on.contains(e)) bad.push(e.textContent.trim().slice(0, 24) + " under " + (on.className || on.tagName)); }); return bad.slice(0, 3).join(", ");'
}

# Every row and button does something and can be left again, and every kind of challenge
# can be tried. Not how well: that they are there, since a control nothing uses breaks
# without anything else noticing.
sweep_rows() { # where, for the messages
    local where=$1 i=0 n kind now title found
    n=$(js "return document.querySelectorAll('.row-btn').length")
    case "$n" in
        '' | *[!0-9]*)
            echo "$where: no rows ($n)"
            return
            ;;
    esac
    while [ "$i" -lt "$n" ]; do
        kind=$(js "var e = document.querySelectorAll('.row-btn')[$i]; return e.getAttribute('role') === 'switch' ? e.getAttribute('aria-checked') : 'row'")
        title=$(js "var e = document.querySelectorAll('.row-btn')[$i].querySelector('.row-title'); return e ? e.textContent : ''")
        if [ "$kind" = row ]; then
            click .row-btn "$i" 1.5
            # A sheet, a screen of its own, or something of the system's on top.
            if ! on_screen .sheet && ! on_screen .screen-header; then
                dev dumpsys window | grep -m1 mCurrentFocus | grep -q "$PKG" && echo "$where: '$title' does nothing"
            fi
            found=$(covered)
            [ -z "$found" ] || echo "$where: in '$title': $found"
            press_back
        else
            click .row-btn "$i" 0.6
            now=$(js "return document.querySelectorAll('.row-btn')[$i].getAttribute('aria-checked')")
            [ "$now" != "$kind" ] || echo "$where: switch '$title' stays $now"
            click .row-btn "$i" 0.6
        fi
        now=$(js "return document.querySelectorAll('.row-btn').length")
        [ "$now" = "$n" ] || {
            echo "$where: after '$title', $now rows instead of $n"
            return
        }
        i=$((i + 1))
    done
}

s41() {
    scenario s41_every_control
    fresh
    seed a "$(alarm_json a 7 0 "$EVERY_DAY" null)"
    arm a
    sleep 2
    local problems="" i n found
    # Home.
    click .alarm-menu
    on_screen .sheet || problems="$problems; no alarm menu"
    found=$(covered)
    [ -z "$found" ] || problems="$problems; in the alarm menu: $found"
    press_back
    for i in 0 1; do
        click .round-icon-btn "$i"
        on_screen .sheet || problems="$problems; home button $i does nothing"
        found=$(covered)
        [ -z "$found" ] || problems="$problems; in home sheet $i: $found"
        press_back
    done
    for i in 0 1 2; do
        click .round-icon-btn 1
        click .sheet-item "$i"
        on_screen .sheet && press_back
        on_screen .alarm-card || problems="$problems; sorting $i lost the list"
    done
    # History, with or without anything in it.
    click .nav-item 1
    n=$(js "return document.querySelectorAll('.chip-pill').length")
    [ "${n:-0}" -ge 2 ] 2> /dev/null || problems="$problems; no history filters: $n"
    i=0
    while [ "$i" -lt "${n:-0}" ] 2> /dev/null; do
        click .chip-pill "$i" 0.4
        i=$((i + 1))
    done
    on_screen .screen-title-large || problems="$problems; a history filter broke the screen"
    # Settings and the editor.
    click .nav-item 2
    found=$(sweep_rows settings)
    [ -z "$found" ] || problems="$problems; $found"
    click .nav-item 0
    click .alarm-main
    found=$(sweep_rows editor)
    [ -z "$found" ] || problems="$problems; $found"
    # Every kind of challenge, added and tried, from a list that starts empty.
    click_row Challenges
    local name opened=""
    i=0
    while [ "$i" -lt 8 ]; do
        click '.sheet .btn-quiet' 0 0.6
        name=$(js "var e = document.querySelectorAll('.add-name')[$i]; return e ? e.textContent : ''")
        click .add-option "$i" 1
        press_back
        on_screen .challenge-list || click_row Challenges
        n=$(js "return document.querySelectorAll('.challenge-row').length")
        click '.challenge-row .link-btn' $((n - 1)) 1.5
        dev dumpsys window | grep -m1 mCurrentFocus | grep -q "$PKG" || press_back
        if on_screen .run-screen; then
            opened="$opened $name"
            click '.run-top .link-btn' 0 1
        else
            problems="$problems; $name cannot be tried"
            press_back
        fi
        on_screen .challenge-list || click_row Challenges
        i=$((i + 1))
    done
    screenshot s41_challenges
    if [ -z "$problems" ]; then
        pass "all rows and sheets, tried:$opened"
    else
        fail "$problems"
    fi
    save_log
}

# The camera scanner really opens, also in a release: what only a layout refers to is the
# first thing R8 and the resource shrinker take.
s42() {
    scenario s42_scanner_opens
    fresh
    seed a "$(alarm_json a 7 0 "$EVERY_DAY" null air 0 0 3 1 '[["challenge.scan","{}"]]')"
    arm a
    sleep 2
    click .alarm-main
    click_row Challenges
    click '.challenge-row .link-btn' 0 1.5
    on_screen .run-screen || { fail "no challenge screen"; save_log; return; }
    adb_ logcat -c
    click .scan-button 0 4
    screenshot s42_scanner
    local top crashed
    top=$(dev dumpsys activity activities | grep -m1 ResumedActivity)
    crashed=$(adb_ logcat -d 2> /dev/null | grep -E 'FATAL EXCEPTION|ClassNotFoundException|NoClassDefFoundError|Resources\$NotFoundException|InflateException' | head -3)
    case "$top" in
        *SentinelScannerActivity*)
            if [ -z "$crashed" ]; then
                press_back
                sleep 1
                if on_screen .run-screen; then
                    pass "opened the camera and came back"
                else
                    fail "did not come back from the scanner"
                fi
            else
                fail "opened, but: $crashed"
            fi
            ;;
        *) fail "no scanner on top: $top; $crashed" ;;
    esac
    save_log
}

# One row of history, `$2` days ago.
history_row() { # id days_ago
    local at
    at=$(date -u -d "@$(($(now_s) - $2 * 86400))" +%Y-%m-%dT07:00:00+00:00)
    printf "INSERT INTO alarm_events (id, alarm_id, alarm_label, fire_timestamp, dismiss_timestamp, total_dismissal_time_ms, outcome, seen) VALUES ('%s', 'a', 'a', '%s', '%s', 0, 'solved', 1);\n" "$1" "$at" "$at" | sql
}
history_count() { printf "SELECT count(*) FROM alarm_events;\n" | sql; }

# A period of the history goes at once, all of it only after asking again.
s43() {
    scenario s43_delete_history
    fresh
    history_row old 200
    history_row new 1
    start_app
    sleep 5
    local problems=""
    click .nav-item 1
    click .icon-btn 0
    on_screen .sheet || problems="$problems; no sheet"
    click .sheet-item 0 1.5
    [ "$(history_count)" = 1 ] || problems="$problems; older than 30 days left $(history_count)"
    on_screen .toast-text || problems="$problems; nothing said about it"
    click .icon-btn 0
    click .sheet-item 2
    [ "$(history_count)" = 1 ] || problems="$problems; everything went without asking again"
    screenshot s43_asking
    press_back
    on_screen .sheet && problems="$problems; Back left the question open"
    [ "$(history_count)" = 1 ] || problems="$problems; Back deleted it"
    click .icon-btn 0
    click .sheet-item 2
    click .sheet-item 0 1.5
    [ "$(history_count)" = 0 ] || problems="$problems; left $(history_count) after confirming"
    if [ -z "$problems" ]; then
        pass "by period at once, everything after asking again"
    else
        fail "${problems#; }"
    fi
    save_log
}

# Two shake challenges in a row: the second starts its sensor after the first stopped its
# own. Only on an emulator, which can be told what its accelerometer feels.
s44() {
    scenario s44_two_shakes_in_a_row
    case "$SERIAL" in
        emulator-*) ;;
        *)
            note "needs an emulator"
            return
            ;;
    esac
    fresh
    next_minute 45
    seed a "$(alarm_json a "$T_H" "$T_M" "$EVERY_DAY" null air 0 0 3 1 '[["challenge.shake","{\"count\":3}"],["challenge.shake","{\"count\":3}"]]')"
    arm a
    await_sound 90 || { fail "never started"; save_log; return; }
    sleep 4
    shake() {
        local i=0
        while [ "$i" -lt 5 ]; do
            adb_ emu sensor set acceleration 40:40:40 > /dev/null
            sleep 0.45
            adb_ emu sensor set acceleration 0:0:9.8 > /dev/null
            sleep 0.45
            i=$((i + 1))
        done
    }
    click .ring-off 0 2
    local first second
    first=$(read_text .progress-ring-text)
    shake
    sleep 1.5
    second=$(read_text .progress-ring-text)
    screenshot s44_second
    on_screen .progress-ring || { fail "no second challenge: first said '$first'"; save_log; action dismiss:a; return; }
    shake
    sleep 3
    if [ "$(state a)" = scheduled ] && [ "$(outcomes solved)" = 1 ] && ! sound_on; then
        pass "both counted: '$first', then '$second'"
    else
        fail "after the second: state=$(state a) solved=$(outcomes solved) on screen '$(read_text .progress-ring-text)'"
    fi
    save_log
    action dismiss:a
}

# A warning that is not a problem for this user can be hidden, and stays on its own screen.
s45() {
    scenario s45_hide_a_warning
    fresh
    dev appops set "$PKG" SYSTEM_ALERT_WINDOW deny
    start_app
    sleep 5
    local problems="" first
    first=$(read_text .banner-text)
    [ -n "$first" ] || problems="$problems; nothing to hide"
    click '.banner-row .icon-btn' 0 1
    [ "$(read_text .banner-text)" != "$first" ] || problems="$problems; still there after hiding"
    dev am force-stop "$PKG"
    sleep 1
    start_app
    sleep 5
    [ "$(read_text .banner-text)" != "$first" ] || problems="$problems; back after a restart"
    printf "SELECT prefs FROM settings;\n" | sql | grep -q 'hidden_warnings":\["' || problems="$problems; not saved"
    grant_all
    if [ -z "$problems" ]; then
        pass "hid '$first', and it stays hidden"
    else
        fail "${problems#; }"
    fi
    save_log
}

# A 12-hour clock everywhere: the list, the editor, the wheels, and what gets saved.
s46() {
    scenario s46_twelve_hours
    fresh
    use_settings '"Dark"' '"English"' '"setup_done":true,"language_follows_phone":false,"time_format":"H12"'
    seed a "$(alarm_json a 14 5 "$EVERY_DAY" null)"
    arm a
    sleep 2
    local problems="" card big wheel
    card=$(read_text .alarm-time)
    [ "$card" = "2:05 PM" ] || problems="$problems; card says '$card'"
    click .alarm-main
    big=$(read_text .time-big)
    [ "$big" = "2:05 PM" ] || problems="$problems; editor says '$big'"
    click .time-big
    wheel=$(js "return [...document.querySelectorAll('.wheel-cell-center')].map(e => e.textContent).join(' ')")
    screenshot s46_wheels
    case "$wheel" in
        *2*PM*5* | *2*PM*05*) ;;
        *) problems="$problems; wheels say '$wheel'" ;;
    esac
    if [ -z "$problems" ]; then
        pass "the list, the editor and the wheels say $card"
    else
        fail "${problems#; }"
    fi
    save_log
}

# From an alarm's menu: new alarms start like this one, without its name.
s47() {
    scenario s47_use_for_new_alarms
    fresh
    local extra=',"note":"Keep out"'
    ALARM_EXTRA=$extra seed a "$(ALARM_EXTRA=$extra alarm_json a 7 0 "$EVERY_DAY" null lofi 0 60 2 9)"
    arm a
    sleep 2
    local problems=""
    click .alarm-menu
    js "[...document.querySelectorAll('.sheet-item')].find(e => e.textContent === 'Use for new alarms').click(); return 'ok'" > /dev/null
    sleep 1
    on_screen .toast-text || problems="$problems; nothing said about it"
    click .fab
    local rows
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
    save_log
}

# What goes through the phone's own file picker, in the build that R8 went over: a sound of
# one's own in, and alarms out to a file and back. Emulator only: the picker differs per
# phone, and this looks for its words.
picker_tap() { # file name
    local bounds i=0
    while [ "$i" -lt 12 ]; do
        dev uiautomator dump /sdcard/af_ui.xml > /dev/null 2>&1
        bounds=$(dev cat /sdcard/af_ui.xml | tr '>' '\n' | grep -m1 "text=\"$1\"" | sed -E 's/.*bounds="\[([0-9]+),([0-9]+)\]\[([0-9]+),([0-9]+)\].*/\1 \2 \3 \4/')
        if [ -n "$bounds" ]; then
            local x1 y1 x2 y2
            read -r x1 y1 x2 y2 <<< "$bounds"
            dev input tap $(((x1 + x2) / 2)) $(((y1 + y2) / 2))
            return 0
        fi
        sleep 1.5
        i=$((i + 1))
    done
    return 1
}
s48() {
    scenario s48_files_in_and_out
    case "$SERIAL" in
        emulator-*) ;;
        *)
            note "needs an emulator"
            return
            ;;
    esac
    fresh
    dev rm -f /sdcard/Download/mytune.ogg '/sdcard/Download/alarmfree-*'
    adb_ push assets/sounds/default/air.ogg /sdcard/Download/mytune.ogg > /dev/null
    dev am broadcast -a android.intent.action.MEDIA_SCANNER_SCAN_FILE -d file:///sdcard/Download/mytune.ogg > /dev/null
    seed a "$(alarm_json a 7 0 "$EVERY_DAY" null)"
    arm a
    sleep 2
    local problems=""
    # A sound.
    click .alarm-main
    click_row Sound
    click '.sheet .btn-quiet' 0 3
    screenshot s48_picker
    if picker_tap mytune.ogg; then
        sleep 4
        local names
        names=$(texts_of .radio-text)
        case "$names" in
            *mytune*) ;;
            *) problems="$problems; own sound not listed: $names" ;;
        esac
        as_app "ls files/sounds/custom" | grep -q mytune || problems="$problems; not copied"
    else
        problems="$problems; the picker never showed the file"
        press_back
    fi
    dev am force-stop "$PKG"
    sleep 1
    start_app
    sleep 5
    # Alarms to a file: only the one alarm in it, under today's name.
    click .nav-item 2
    click_row "Back up"
    click .sheet-item 0 2
    local file
    file=$(dev ls /sdcard/Download/ | grep -m1 '^alarmfree-')
    [ -n "$file" ] || problems="$problems; no file in Downloads"
    [ "$(dev cat "/sdcard/Download/$file" | grep -c '"time_hour"')" -ge 1 ] || problems="$problems; no alarm in the file"
    # And back from it, as a second one.
    click_row "Back up"
    click .sheet-item 1 3
    if picker_tap "$file"; then
        sleep 4
        [ "$(printf 'SELECT count(*) FROM alarms;\n' | sql)" = 2 ] || problems="$problems; $(printf 'SELECT count(*) FROM alarms;\n' | sql) alarms after restoring"
        on_screen .toast-undo || problems="$problems; no undo after restoring"
    else
        problems="$problems; the picker never showed the backup"
        press_back
    fi
    screenshot s48_restored
    dev rm -f /sdcard/Download/mytune.ogg "/sdcard/Download/$file"
    if [ -z "$problems" ]; then
        pass "own sound in, alarms out to $file and back"
    else
        fail "${problems#; }"
    fi
    save_log
}

# The keyboard stays off what is being typed in, in a panel and in the typing challenge, and
# what is typed arrives.
s49() {
    scenario s49_keyboard
    fresh
    seed a "$(alarm_json a 7 0 "$EVERY_DAY" null air 0 0 3 1 '[["challenge.typing","{\"difficulty\":\"easy\"}"]]')"
    arm a
    sleep 2
    local problems="" x y before free
    # How much of the page is left above whatever covers it.
    local left='return Math.round(innerHeight - parseFloat(getComputedStyle(document.querySelector(".app-root")).getPropertyValue("--safe-bottom")))'
    before=$(js "$left")
    click .alarm-main
    click_row Label
    read -r x y <<< "$(screen_point '.sheet .field')"
    dev input tap "$x" "$y"
    sleep 2.5
    screenshot s49_label
    free=$(js "$left")
    [ "$free" -lt $((before - 100)) ] 2> /dev/null || problems="$problems; the page did not make room: $before, then $free"
    [ "$(js "return document.querySelector('.sheet .field').getBoundingClientRect().bottom <= $free && document.querySelector('.sheet .sheet-done').getBoundingClientRect().bottom <= $free ? 'y' : 'n'")" = y ] ||
        problems="$problems; the field or Done is under the keyboard"
    dev input text Morning
    sleep 1.5
    [ "$(js "return document.querySelector('.sheet .field').value")" = Morning ] || problems="$problems; typed text did not arrive: $(js "return document.querySelector('.sheet .field').value")"
    press_back
    press_back
    sleep 1
    free=$(js "$left")
    [ "$free" = "$before" ] || problems="$problems; no room given back: $before, now $free"
    # The challenge. It has to show the passage and the box together.
    press_back
    click '.sheet-item.danger' 0 1
    on_screen .home-header || problems="$problems; not back on Home"
    click .alarm-menu
    click .sheet-item 0 2
    read -r x y <<< "$(screen_point .typing-input-area)"
    dev input tap "$x" "$y"
    sleep 2.5
    screenshot s49_typing
    free=$(js "$left")
    [ "$(js "var a = document.querySelector('.typing-input-area').getBoundingClientRect(), p = document.querySelector('.typing-passage').getBoundingClientRect(); return a.bottom <= $free && p.top >= 0 && p.height >= 60 ? 'y' : 'n'")" = y ] ||
        problems="$problems; passage and box do not fit above the keyboard"
    local passage
    passage=$(js "return document.querySelector('.typing-passage').textContent")
    dev input text "$(echo "$passage" | sed 's/ /%s/g')"
    sleep 4
    screenshot s49_typed
    on_screen .typing-input-area && problems="$problems; typing it all did not finish: '$(js "return document.querySelector('.typing-input-area').value")'"
    if [ -z "$problems" ]; then
        pass "room made and given back, text arrives, challenge done by typing"
    else
        fail "${problems#; }"
    fi
    save_log
}

ALL="s01 s02 s03 s04 s05 s06 s07a s07b s07c s09 s10 s11a s11b s12 s12b s13 s14 s15 s16 s16b s17 s19 s20 s21 s22 s22b s23a s23b s23c s23d s24 s25 s26x5 s27 s28 s28b s29 s30 s31 s32 s33 s34 s35 s36 s37 s38 s39 s40 s41 s42 s43 s44 s45 s46 s47 s48 s49"
: > "$RESULTS"
for s in ${*:-$ALL}; do
    "$s"
done
echo
echo "===== SUMMARY ($SERIAL) ====="
cat "$RESULTS"
echo "passed: $(grep -c '^PASS' "$RESULTS")  failed: $(grep -c '^FAIL' "$RESULTS")"
