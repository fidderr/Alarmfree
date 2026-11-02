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
    note "after dismiss: state=$(state a) next_in=$((($(next_fire_ms a) / 1000 - $(now_s)) / 60))min"
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
    if [ "$(state a)" = snoozed ] && [ "$before" = "$after" ]; then
        pass "snooze kept after reopening"
    else
        fail "state=$(state a) next ring moved by $(((${after:-0} - ${before:-0}) / 60000))min, db_enabled=$(db_enabled a)"
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
    if sound_on && [ "$(state a)" = firing ]; then
        pass "back after $(($(now_s) - t0))s, silent for ${silent} checks, UI on top: $(top_is_app && echo yes || echo no)"
    else
        fail "after 40s sound=$(sound_on && echo on || echo OFF) state=$(state a) main=$(main_pid)"
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
    start_app
    sleep 20
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
    sleep 15
    local late=no armed
    sound_on && late=yes
    armed=$(armed_at_ms a)
    if [ "$late" = yes ]; then
        pass "rang late after the restart"
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
    sleep 15
    local next
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
    dev cmd alarm set-timezone Asia/Tokyo 2> /dev/null || dev service call alarm 3 s16 Asia/Tokyo > /dev/null
    sleep 12
    after=$(armed_at_ms a)
    if [ -n "$before" ] && [ "$before" != "$after" ]; then
        pass "re-armed, moved by $(((after - before) / 60000))min"
    else
        fail "armed time unchanged after zone change ($zone to Asia/Tokyo), main=$(main_pid)"
    fi
    save_log
    dev cmd alarm set-timezone "$zone" 2> /dev/null || dev service call alarm 3 s16 "$zone" > /dev/null
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
    dev input tap 5 5
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
    if [ "$(job_status a)" = pending ] && [ "$(db_enabled a)" = false ] && ! sound_on; then
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
    sleep 3
    screenshot s25_home
    local p0 p1
    p0=$(main_pid)
    # First card, roughly a third down the screen on any phone.
    local size w h
    size=$(dev wm size | tail -1 | sed 's/.*: //')
    w=${size%x*}
    h=${size#*x}
    dev input tap $((w / 3)) $((h / 5))
    sleep 3
    screenshot s25_editor
    dev input keyevent KEYCODE_BACK
    sleep 3
    screenshot s25_after_back
    p1=$(main_pid)
    if [ "$p0" = "$p1" ] && top_is_app; then
        pass "Back stayed in the app"
    else
        fail "Back closed the app: main $p0 to ${p1:-dead}, on top: $(top_is_app && echo yes || echo no)"
    fi
    save_log
}

ALL="s01 s02 s03 s04 s05 s06 s07a s07b s07c s09 s10 s11a s11b s12 s12b s14 s15 s16 s16b s17 s19 s20 s21 s22 s23a s23b s23c s23d s24 s25 s26x5 s27"
: > "$RESULTS"
for s in ${*:-$ALL}; do
    "$s"
done
echo
echo "===== SUMMARY ($SERIAL) ====="
cat "$RESULTS"
echo "passed: $(grep -c '^PASS' "$RESULTS")  failed: $(grep -c '^FAIL' "$RESULTS")"
