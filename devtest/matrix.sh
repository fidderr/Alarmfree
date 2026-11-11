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
    if ! notifications_off && ! deny_notifications; then
        skip "turn this app's notifications off by hand for this one: the shell may not here"
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

# ---- Restarted and not unlocked yet ----
#
# The app's own screen cannot start until the first unlock: the alarm rings all the same, and a
# screen of the SDK asks for that unlock. These set a PIN, so never on a phone somebody uses.

# An alarm that rings on a phone that restarted and stayed locked, behind the unlock screen.
# `$1`: seconds the ring is quiet after the Unlock button, `$2`: its ring limit in seconds.
# `OVERLAY=deny` takes away the permission that lets the app start a screen from the background.
locked_ring() {
    fresh
    [ "${OVERLAY:-}" = deny ] && deny_overlay
    shorten unlock_quiet_secs "${1:-60}"
    on_emulator && set_clock "$(date +%s)"
    next_minute 3600
    seed a "$(alarm_json a "$T_H" "$T_M" "$EVERY_DAY" null air 0 0 3 1 '[]' "${2:-1800}")"
    arm a
    dev locksettings set-pin 1234 > /dev/null
    dead
    # Now and not during the restart's own wait: a clock change that wakes the app up meanwhile
    # would find it overdue and ring it, and the restart ends a ring as dismissed.
    stopped_state_saved
    dead
    make_overdue a 2
    reboot_and_wait || { fail "reboot hung"; return 1; }
    T=$(now_s)
    if [ "$(dev dumpsys user | grep -c 'RUNNING_LOCKED')" = 0 ]; then
        fail "the phone is not locked after the restart"
        return 1
    fi
    await_sound 60 || { fail "no sound in 60s, main=$(main_pid) $(dev dumpsys package "$PKG" | grep -m1 -oE 'stopped=[a-z]+')"; return 1; }
    wait_for 15 top_is_unlock || { fail "rang, but the unlock screen is not on top: $(top_activity)"; return 1; }
}
# After a scenario that set the PIN, whatever became of it.
no_pin() {
    dev locksettings clear --old 1234 > /dev/null
    OVERLAY=
    SCREEN_WENT_OFF=1
}
no_lock_here() {
    [ -n "${NO_LOCK:-}" ] || return 1
    note "$1 sets a PIN: not here"
}
# Whether the SDK gave up on the app's screen for this ring. As root: the app's own shell does
# not start while the phone is locked.
ui_given_up() { [ "$(dev "su 0 sh -c 'test -e $DATA/files/sentinel/ringing/ui_failed && echo y'")" = y ]; }

# The whole way: it rings behind the unlock screen, nothing there ends it, its button brings
# Android's own unlock, and with the PIN in, the alarm's own screen is there to turn it off on.
# On the way, what must not happen: the app's core dying while locked must not count as its
# screen failing, and a ring that went quiet for the unlock must come back by itself.
s13() {
    no_lock_here s13 && return
    scenario s13_locked_restart_unlock
    # Without the permission to start a screen from the background: the way through the button
    # must not need it.
    OVERLAY=deny locked_ring 5 || { no_pin; return; }
    local problems="" rang=$SOUND_AT
    # A phone's cleaner can take a process with nothing on screen. The rounds in which that
    # could count against the app's screen pass while the quiet below runs out.
    kill_main
    tap_text Unlock || problems="$problems; no Unlock button on the screen"
    wait_for 4 sound_off || problems="$problems; still sounding after Unlock"
    # Left alone, the phone stays locked: the quiet runs out.
    wait_for 12 sound_on || problems="$problems; the sound did not come back when the quiet ran out"
    user_unlocked && problems="$problems; the phone unlocked by itself"
    ui_given_up && problems="$problems; the core dying while locked counted as a failing screen"
    # Android's unlock is still up from the first time: away with it, and ask again.
    press_back
    wait_for 5 top_is_unlock || problems="$problems; after cancelling the unlock: $(top_activity)"
    screenshot s13_unlock_screen
    sound_on || problems="$problems; silent after cancelling the unlock"
    tap_text Unlock || problems="$problems; no Unlock button the second time"
    wait_for 4 sound_off || problems="$problems; still sounding after the second Unlock"

    type_test_pin || problems="$problems; the PIN did not unlock the phone"
    if ! on_screen .ring-off 20; then
        screenshot s13_after_unlock
        fail "the alarm's own screen did not come up after the unlock: $(top_activity)${problems}"
        no_pin
        return
    fi
    screenshot s13_alarm_screen
    top_is_main || problems="$problems; the alarm's screen is not on top: $(top_activity)"
    wait_for 5 sound_on || problems="$problems; silent on the alarm's own screen"
    state_is a firing || problems="$problems; state $(state a) on the alarm's own screen"
    click .ring-off 0 0.5
    finished() { sound_off && state_is a scheduled && [ "$(outcomes solved)" = 1 ]; }
    wait_for 8 finished || problems="$problems; after turning it off: sound=$(sound_on && echo ON || echo off) state=$(state a) solved=$(outcomes solved)"
    [ "$(outcomes backup)" = 0 ] || problems="$problems; history says backup screen"
    if [ -z "$problems" ]; then
        pass "rang ${rang}s after the boot behind the unlock screen, quiet for the unlock and back, own screen after the PIN, turned off there"
    else
        fail "${problems#; }"
    fi
    no_pin
}

# Not the button but the lock screen itself: the alarm's own screen still comes up by itself,
# and before that the ring comes back after its service is killed in the quiet.
s13b() {
    no_lock_here s13b && return
    scenario s13b_locked_restart_lock_screen
    locked_ring 60 || { no_pin; return; }
    local problems=""
    tap_text Unlock || problems="$problems; no Unlock button on the screen"
    wait_for 4 sound_off || problems="$problems; still sounding after Unlock"
    kill_sentinel
    wait_for 15 sound_on || problems="$problems; no sound within 15s of the ring service being killed in the quiet"
    wait_for 10 top_is_unlock || problems="$problems; the unlock screen did not come back after that: $(top_activity)"
    # Android's unlock can still be up from the button, and it takes Home for itself.
    press_back
    go_home
    left() { ! top_is_unlock; }
    wait_for 3 left || problems="$problems; Home did not leave the unlock screen"
    sound_on || problems="$problems; silent after Home"
    dev wm dismiss-keyguard
    type_test_pin || problems="$problems; the PIN did not unlock the phone"
    if ! on_screen .ring-off 20; then
        screenshot s13b_after_unlock
        fail "the alarm's own screen did not come up after unlocking on the lock screen: $(top_activity)${problems}"
        no_pin
        return
    fi
    sound_on || problems="$problems; silent on the alarm's own screen"
    click .ring-off 0 0.5
    finished() { sound_off && state_is a scheduled && [ "$(outcomes solved)" = 1 ]; }
    wait_for 8 finished || problems="$problems; after turning it off: sound=$(sound_on && echo ON || echo off) state=$(state a) solved=$(outcomes solved)"
    if [ -z "$problems" ]; then
        pass "the ring came back after its service died in the quiet, and unlocking on the lock screen brought the alarm's own screen"
    else
        fail "${problems#; }"
    fi
    no_pin
}

# The same without the permission to start a screen from the background. Android then allows no
# more than a notification once the phone is in use: the alarm rings on, and is there to turn
# off as soon as the app is opened. Only by name, not in a whole run: what the SDK does here is
# what s13b tests, the rest is Android's, and each of these takes a restart.
s13c() {
    no_lock_here s13c && return
    scenario s13c_locked_restart_no_overlay
    OVERLAY=deny locked_ring 60 || { no_pin; return; }
    local problems="" how="own screen only once the app was opened, as Android allows without the overlay"
    go_home
    sleep 1
    dev wm dismiss-keyguard
    type_test_pin || problems="$problems; the PIN did not unlock the phone"
    wait_for 8 top_is_main && how="own screen up by itself"
    sound_on || problems="$problems; silent after the unlock"
    state_is a firing || problems="$problems; state $(state a) after the unlock"
    top_is_backup && problems="$problems; the backup screen came up"
    start_app
    if ! on_screen .ring-off 20; then
        fail "the alarm's own screen is not there when the app is opened: $(top_activity)${problems}"
        no_pin
        return
    fi
    click .ring-off 0 0.5
    finished() { sound_off && state_is a scheduled && [ "$(outcomes solved)" = 1 ]; }
    wait_for 8 finished || problems="$problems; after turning it off: sound=$(sound_on && echo ON || echo off) state=$(state a) solved=$(outcomes solved)"
    if [ -z "$problems" ]; then
        pass "rang on after the unlock, $how, turned off there"
    else
        fail "${problems#; }"
    fi
    no_pin
}

# Nobody comes: the ring ends at its limit with the phone still locked, the unlock screen goes
# with it, and the history tells after the unlock.
s13d() {
    no_lock_here s13d && return
    scenario s13d_locked_restart_limit
    locked_ring 60 8 || { no_pin; return; }
    local problems=""
    await_silence 60 || problems="$problems; still ringing 60s after the boot with a limit of 8s"
    gone() { ! top_is_unlock; }
    wait_for 6 gone || problems="$problems; the unlock screen stayed after the ring ended"
    user_unlocked && problems="$problems; the phone unlocked by itself"
    dev wm dismiss-keyguard
    type_test_pin || problems="$problems; the PIN did not unlock the phone"
    recorded() { [ "$(outcomes limit)" = 1 ] && state_is a scheduled; }
    wait_for 15 recorded || problems="$problems; after the unlock: limit=$(outcomes limit) state=$(state a)"
    sound_off || problems="$problems; sounding after the unlock"
    if [ -z "$problems" ]; then
        pass "stopped at its limit ${STOPPED_AT}s after the boot while locked, and the history says so"
    else
        fail "${problems#; }"
    fi
    no_pin
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
    local p0 t0 armed_pid try=0
    while :; do
        fresh
        shorten ui_timeout_ms 4000
        shorten backup_holds 1
        next_minute
        seed a "$(alarm_json a "$T_H" "$T_M" "$EVERY_DAY" null)"
        arm a
        armed_pid=$(main_pid)
        await_ring a 30 || { fail "never started"; return; }
        on_screen .ring-off 10
        # With a limit this short, a busy machine can run it out while the ring screen still
        # loads. The guardian restarts it before anything here froze, and the freeze below then
        # counts as its second failure, which rightly goes to the backup screen.
        [ "$(main_pid)" = "$armed_pid" ] && break
        try=$((try + 1))
        [ "$try" -lt 3 ] || { fail "the ring screen took longer than its limit, three times"; return; }
    done
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
    next_minute 320
    ALARM_EXTRA=',"notify_before_minutes":5'
    seed a "$(alarm_json a "$T_H" "$T_M" "$EVERY_DAY" null)"
    ALARM_EXTRA=
    arm a
    jump_to $((T - 303))
    # This app's own notice, not its channel: that stays listed once one has been shown. On a phone
    # its time has to come first, too.
    if wait_for 180 reminded; then
        local left=$((T - $(now_s)))
        if [ "$left" -ge 240 ] && [ "$left" -le 305 ]; then
            pass "shown ${left}s before the ring"
        else
            fail "shown ${left}s before the ring, it should be 300"
        fi
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
texts_of() { ask_page "return [...document.querySelectorAll('$1')].map(e => e.textContent).join(' ')"; }
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
    [ "$(js "return document.querySelectorAll('.swatch').length")" = 10 ] || problems="$problems; no swatches"
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
    # Any warning that can be hidden: the ones that keep an alarm from ringing cannot. Without one, a
    # warning is made where the shell may. A phone's shell often may not touch the overlay, but does
    # let the battery exemption go, which the caller of this then puts back.
    start_app
    if ! on_screen '.banner-row .icon-btn' 8; then
        dev am force-stop "$PKG"
        if ! deny_overlay; then
            skip "needs a warning that can be hidden: this app without its battery exemption has one"
            return
        fi
        start_app
        on_screen '.banner-row .icon-btn' 10
    fi
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
        *SoundLofi*) ;;
        *) problems="$problems; sound not taken over: $rows" ;;
    esac
    case "$rows" in
        *"9 min, 2"*) ;;
        *) problems="$problems; snooze not taken over: $rows" ;;
    esac
    case "$rows" in
        *Keep\ out* | *"Label"a*) problems="$problems; its name came along: $rows" ;;
    esac
    open_row Sound
    case "$(js "return [...document.querySelectorAll('.sheet .chip.selected')].map(e => e.textContent).join('|')")" in
        *"1 min"*) ;;
        *) problems="$problems; fade not taken over" ;;
    esac
    if [ -z "$problems" ]; then
        pass "sound, fade and snooze taken over, its name not"
    else
        fail "${problems#; }"
    fi
}

# What goes through the phone's own file picker, in the build that R8 went over: a sound of
# one's own in, and alarms out to a file and back. Emulator only: the picker differs per
# phone, and this looks for its words.
ui_tap() { # the middle of the first element with this in its description
    local bounds x1 y1 x2 y2
    # UI_WINDOWS=--windows looks in every window: Android can go on calling the home screen the
    # active one with the shade open over it.
    dev uiautomator dump ${UI_WINDOWS:-} /sdcard/af_ui.xml > /dev/null 2>&1
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
    done_sheet
    open_row 'Ring limit'
    click_text '.sheet .chip' '60 min'
    done_sheet
    open_row 'Are you up?'
    click_text '.sheet .chip' '3 min'
    done_sheet
    open_row 'Notice before'
    click_text '.sheet .chip' '10 min'
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
        '"snooze_count":4' 'challenge.math' 'challenge.steps' '"notify_before_minutes":10' \
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
    for shown in Morning Funk Steady '6 min, 4' Math '60 min' 'After 3 min' '10 min'; do
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
    click_row Theme
    on_screen .theme-tile
    click_text '.theme-tile-name' Light 0.3
    press_back
    on_screen .row-btn
    pick 'Time format' .sheet-item '12 hours'
    pick 'Week starts on' .sheet-item Sunday
    open_row 'Minimum volume'
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
    for field in '"time_format":"H12"' '"week_start":"Sunday"' \
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
        pass "theme, language, 12 hours, week, volume and switches all stick"
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


# Is something vibrating right now? Under that heading it says null when nothing is, on every
# Android. Filtered on the device, in one go: a pipe with a `dev` on both sides has two readers on
# the one open shell, and hangs.
vibrating_now() {
    dev "dumpsys vibrator_manager | grep -A1 'CurrentVibration:' | grep -v 'CurrentVibration:' | grep -vE '^ *null *\$' | grep -q ."
}
not_vibrating() { ! vibrating_now; }
start_stop_says() { js "var e = document.querySelector('.start-stop'); return e ? e.getAttribute('aria-pressed') : 'none'"; }

# Trying a sound and a vibration: a tap only picks a sound, Start plays it, and Stop, closing and
# time all end it. A tap on a pattern vibrates, for just as long.
s58() {
    scenario s58_trying_sound_and_vibration
    fresh
    seed a "$(alarm_json a 7 0 "$EVERY_DAY" null)"
    arm a
    on_screen .alarm-main 10
    local problems="" started took
    click .alarm-main
    on_screen .row-btn
    open_row Sound
    click_text '.sheet .radio-text' Funk 0.6
    sound_on && problems="$problems; a tap on a sound plays it"
    [ "$(start_stop_says)" = false ] || problems="$problems; says it runs: $(start_stop_says)"
    click .start-stop 0
    wait_for 4 sound_on || problems="$problems; Start is silent"
    [ "$(start_stop_says)" = true ] || problems="$problems; Start does not turn to Stop"
    screenshot s58_playing
    # While it plays, another sound takes over.
    click_text '.sheet .radio-text' Happy 0.6
    sound_on || problems="$problems; another sound stopped it"
    [ "$(start_stop_says)" = true ] || problems="$problems; another sound ended it"
    click .start-stop 0
    wait_for 4 sound_off || problems="$problems; Stop does not stop it"
    # By itself after a while, also with a sound shorter than that.
    click .start-stop 0
    wait_for 4 sound_on
    started=$SECONDS
    wait_for 22 sound_off || problems="$problems; does not end by itself"
    took=$((SECONDS - started))
    [ "$took" -ge 12 ] && [ "$took" -le 19 ] || problems="$problems; lasted ${took}s"
    [ "$(start_stop_says)" = false ] || problems="$problems; still says Stop after it ended"
    # Closing the panel ends it, by Done and by Back.
    click .start-stop 0
    wait_for 4 sound_on
    done_sheet
    wait_for 4 sound_off || problems="$problems; Done does not stop it"
    open_row Sound
    click .start-stop 0
    wait_for 4 sound_on
    press_back
    gone .sheet
    wait_for 4 sound_off || problems="$problems; Back does not stop it"

    open_row Vibration
    click_text '.sheet .chip' Heartbeat 0.6
    vibrating_now || problems="$problems; a tap on a pattern does not vibrate"
    [ "$(start_stop_says)" = true ] || problems="$problems; no way to stop the vibration"
    screenshot s58_vibrating
    started=$SECONDS
    sleep 8
    vibrating_now || problems="$problems; stopped after one beat"
    wait_for 12 not_vibrating || problems="$problems; vibration does not end by itself"
    took=$((SECONDS - started))
    [ "$took" -ge 12 ] && [ "$took" -le 19 ] || problems="$problems; vibrated ${took}s"
    click_text '.sheet .chip' Pulse 0.6
    vibrating_now || problems="$problems; the next pattern does not vibrate"
    click .start-stop 0
    wait_for 3 not_vibrating || problems="$problems; Stop does not stop the vibration"
    click_text '.sheet .chip' Steady 0.6
    click_text '.sheet .chip' Off 0.6
    wait_for 3 not_vibrating || problems="$problems; Off does not stop the vibration"
    [ "$(start_stop_says)" = none ] || problems="$problems; a button to try nothing"
    click_text '.sheet .chip' Pulse 0.6
    done_sheet
    wait_for 3 not_vibrating || problems="$problems; closing does not stop the vibration"
    # Turning the touch feedback off has nothing to do with this.
    press_back
    on_screen '.sheet, .alarm-card' 4
    is_there .sheet && click '.sheet-item.danger' 0 0.3
    on_screen .nav-item 5
    click .nav-item 2
    on_screen .row-btn
    click_row 'Touch feedback'
    click .nav-item 0
    on_screen .alarm-main
    click .alarm-main
    on_screen .row-btn
    open_row Vibration
    click_text '.sheet .chip' Steady 0.6
    vibrating_now || problems="$problems; touch feedback off silences the preview"
    done_sheet
    if [ -z "$problems" ]; then
        pass "a tap is silent, Start and Stop work, 15 s, closing ends it, vibration too (${took}s)"
    else
        fail "${problems#; }"
    fi
}

# An alarm going off during a preview: its sound and vibration are its own, and stay.
s59() {
    scenario s59_alarm_during_a_preview
    fresh
    next_minute 45
    seed a "$(alarm_json a "$T_H" "$T_M" "$EVERY_DAY" null air 0 0 3 1)"
    arm a
    on_screen .alarm-main 10
    local problems=""
    click .alarm-main
    on_screen .row-btn
    open_row Sound
    click .start-stop 0
    wait_for 4 sound_on || problems="$problems; no preview"
    done_sheet
    open_row Vibration
    click_text '.sheet .chip' Pulse 0.3
    if await_ring a 70 && on_screen .ring-off 15; then
        # Past when the preview would have stopped things.
        sleep 16
        sound_on || problems="$problems; the ring went silent"
        vibrating_now || problems="$problems; the ring stopped vibrating"
        state_is a firing || problems="$problems; state $(state a)"
        click .ring-off 0 0.3
        wait_for 5 sound_off || problems="$problems; still ringing after turning it off"
    else
        problems="$problems; did not ring: state=$(state a)"
    fi
    if [ -z "$problems" ]; then
        pass "the ring keeps its sound and vibration after the preview is gone"
    else
        fail "${problems#; }"
    fi
}

# The notice before an alarm: from there the next ring can be skipped with the app dead, and that
# can be taken back. The ring after that one stays.
s60() {
    if ! on_emulator; then
        note "s60 needs an emulator's clock"
        return
    fi
    scenario s60_skip_from_the_notice
    fresh
    next_minute 320
    ALARM_EXTRA=',"notify_before_minutes":5'
    seed a "$(alarm_json a "$T_H" "$T_M" "$EVERY_DAY" null)"
    ALARM_EXTRA=
    arm a
    local problems="" armed
    armed=$(armed_at_ms a)
    dev input keyevent KEYCODE_HOME
    jump_to $((T - 303))
    wait_for 180 reminded || { fail "no notice with a Skip"; return; }
    kill_all
    wait_for 3 no_process
    # Each time again: Android 16 ignores opening the shade while it is still closing. Some
    # Androids write a button in capitals. A locked screen shows a notice without its buttons, and
    # an emulator can have gone to sleep after what ran before this. Waking closes the shade, so
    # only once.
    tap_in_shade() {
        local i=0
        wake
        while [ "$i" -lt 8 ]; do
            dev cmd statusbar expand-notifications
            # Only once it has finished opening is the shade what counts as on screen.
            sleep 1.5
            UI_WINDOWS=--windows ui_tap "text=\"$1\"" && return 0
            UI_WINDOWS=--windows ui_tap "text=\"${1^^}\"" && return 0
            i=$((i + 1))
        done
        return 1
    }
    tap_in_shade Skip || problems="$problems; no Skip to tap"
    dev cmd statusbar collapse
    skipped() { printf "SELECT data FROM alarms WHERE id='a';\n" | sql | grep -q "\"skip_until_ms\":$((T * 1000))"; }
    wait_for 15 skipped || problems="$problems; not skipped: $(printf "SELECT data FROM alarms WHERE id='a';\n" | sql | grep -o '"skip_until_ms":[^,]*')"
    moved() { [ "$(armed_at_ms a)" = "$((armed + 86400000))" ]; }
    wait_for 8 moved || problems="$problems; armed for $(armed_at_ms a), was $armed"
    # Taken back.
    wait_for 8 done_notice || problems="$problems; nothing to take it back with"
    tap_in_shade Undo || problems="$problems; no Undo to tap"
    dev cmd statusbar collapse
    not_skipped() { printf "SELECT data FROM alarms WHERE id='a';\n" | sql | grep -q '"skip_until_ms":null'; }
    wait_for 15 not_skipped || problems="$problems; undo did nothing"
    back_armed() { [ "$(armed_at_ms a)" = "$armed" ]; }
    wait_for 8 back_armed || problems="$problems; undo left it armed for $(armed_at_ms a)"
    wait_for 8 reminded || problems="$problems; the notice does not come back after undo"
    # Skipped for good this time, from a list that is open: the list says so.
    start_app
    on_screen .alarm-main 10
    tap_in_shade Skip || problems="$problems; no second Skip to tap"
    dev cmd statusbar collapse
    wait_for 15 skipped || problems="$problems; not skipped the second time"
    says_it() { has_text=$(read_text .alarm-when); case "$has_text" in Skips*) return 0 ;; esac; return 1; }
    wait_for 8 says_it || problems="$problems; the list says '$has_text'"
    # No ring at its time.
    dev am force-stop "$PKG"
    jump_to $((T - 5))
    start_app
    on_screen .alarm-main 10
    sleep 15
    sound_on && problems="$problems; rang although skipped"
    state_is a firing && problems="$problems; firing although skipped"
    # And the next one does ring.
    T=$((T + 86400))
    jump_to $((T - 20))
    if ! await_ring a 90; then
        problems="$problems; the ring after the skipped one did not come: state=$(state a)"
    fi
    if [ -z "$problems" ]; then
        pass "skipped with the app dead, taken back, skipped again, that ring silent, the next one rings"
    else
        fail "${problems#; }"
    fi
}

# Colours of one's own: on the wheel, in a code, and never unreadable. They stay, also on the ring.
s61() {
    scenario s61_colours_of_ones_own
    fresh
    next_minute 3000
    seed a "$(alarm_json a 7 0 "$EVERY_DAY" null)"
    arm a
    on_screen .nav-item 10
    local problems="" token
    # Asked again when a page that has just come up is too busy to answer.
    token() { ask_page "return getComputedStyle(document.querySelector('.app-root')).getPropertyValue('--$1').trim()"; }
    click .nav-item 2
    on_screen .row-btn
    click_row Theme
    on_screen .theme-tile
    [ "$(js "return document.querySelectorAll('.theme-tile').length")" = 9 ] || problems="$problems; not all themes"
    [ "$(js "return document.querySelectorAll('.swatch').length")" = 10 ] || problems="$problems; not all accents"
    screenshot s61_theme
    # A theme that comes with the app, and an accent for it.
    click_text '.theme-tile-name' Sand 0.3
    [ "$(token bg)" = "#f3ede2" ] || problems="$problems; Sand is $(token bg)"
    is_there .theme-light || problems="$problems; Sand is not a light theme"
    # On the wheel: a finger, as a person does it.
    click .swatch-wheel 0
    on_screen .color-wheel
    local before x y
    before=$(token accent)
    read -r x y <<< "$(page "var r = document.querySelector('.color-wheel').getBoundingClientRect(), f = devicePixelRatio; return Math.round((r.left + r.width * 0.15) * f) + ' ' + Math.round((r.top + r.height * 0.5) * f);")"
    dev input swipe "$x" "$y" "$((x + 20))" "$((y + 20))" 300
    sleep 0.4
    [ "$(token accent)" != "$before" ] || problems="$problems; the wheel does nothing"
    screenshot s61_wheel
    # In a code.
    set_field '.sheet .color-hex' 0 '#c06020'
    [ "$(token accent)" = "#c06020" ] || problems="$problems; the code gives $(token accent)"
    done_sheet
    # A theme of one's own, from what is on screen.
    click '.theme-own .row-btn' 0 0.3
    [ "$(token bg)" = "#f3ede2" ] || problems="$problems; does not start from the theme on screen"
    screenshot s61_own
    # Rows: the own theme itself, then its three colours.
    click '.row-btn' 1 0.3
    on_screen .color-wheel
    set_field '.sheet .color-hex' 0 '#102030'
    [ "$(token bg)" = "#102030" ] || problems="$problems; background is $(token bg)"
    is_there .theme-dark || problems="$problems; a dark background is not a dark theme"
    [ "$(token text)" != "$(token bg)" ] || problems="$problems; text like the background"
    done_sheet
    # Unreadable on purpose.
    click '.row-btn' 3 0.3
    on_screen .color-wheel
    set_field '.sheet .color-hex' 0 '#102030'
    [ "$(token text)" != "#102030" ] || problems="$problems; text as dark as the page can be chosen"
    is_there .color-adjusted || problems="$problems; does not say it changed the colour"
    screenshot s61_adjusted
    click_text '.sheet .chip' Automatic 0.3
    is_there .color-adjusted && problems="$problems; still says it changed the colour"
    done_sheet
    # It stays, after a restart.
    dev am force-stop "$PKG"
    start_app
    on_screen .alarm-main 10
    [ "$(token bg)" = "#102030" ] || problems="$problems; background not kept: $(token bg)"
    [ "$(token accent)" = "#c06020" ] || problems="$problems; accent not kept, or changed: $(token accent)"
    # What a saved Colorblind theme comes up as: not an empty screen.
    dev am force-stop "$PKG"
    use_settings '"Colorblind"' '"English"' '"setup_done":true,"language_follows_phone":false'
    start_app
    on_screen .alarm-main 10 || problems="$problems; a saved Colorblind theme stops the app"
    [ -n "$(token bg)" ] || problems="$problems; no colours for a saved Colorblind theme"
    if [ -z "$problems" ]; then
        pass "wheel, code and own theme work, stay after a restart, and text stays readable"
    else
        fail "${problems#; }"
    fi
}

# The ring screen of one's own: chosen in the editor, seen in a preview that does nothing, and there
# on the real ring with every button where it was. With less motion it stands still, a photo comes
# in small, and a photo that is gone is the plain screen.
s62() {
    scenario s62_ring_screen_of_ones_own
    fresh
    next_minute 50
    seed a "$(alarm_json a "$T_H" "$T_M" "$EVERY_DAY" null)"
    arm a
    on_screen .alarm-main 10
    local problems=""
    ring_colour() { js "return getComputedStyle(document.querySelector('.ring-backdrop')).getPropertyValue('--ring-1').trim()"; }
    parts_moving() { js "return document.querySelector('.ring-backdrop').getAnimations({subtree: true}).filter(a => a.playState === 'running').length"; }
    # Not under anything: a touch there reaches what is there to touch.
    on_top() { [ "$(js "var e = document.querySelector('$1'), r = e.getBoundingClientRect(), o = document.elementFromPoint(r.left + r.width / 2, r.top + r.height / 2); return e.contains(o) ? 'y' : 'n'")" = y ]; }
    open_look() {
        click .alarm-main
        on_screen .row-btn
        open_row 'Alarm screen'
    }
    open_look
    [ "$(js "return document.querySelectorAll('.look-tile').length")" = 8 ] || problems="$problems; not all backgrounds"
    screenshot s62_panel
    click_text '.look-tile-name' Waves 0.3
    click .look-row 0 0.3
    on_screen .color-wheel
    set_field '.sheet .color-hex' 0 '#c06020'
    click '.sheet .sheet-done' 0 0.3
    on_screen .look-grid
    click_text '.sheet .chip' Mono 0.2
    # A preview: as it will look, without a sound, and it closes by a tap and by Back.
    click .look-preview-link 0 0.3
    on_screen .ring-preview || problems="$problems; no preview"
    is_there '.ring-preview .backdrop-waves' || problems="$problems; the preview has not got the background"
    is_there '.ring-preview .clock-mono' || problems="$problems; the preview has not got the clock"
    [ "$(ring_colour)" = "#c06020" ] || problems="$problems; the preview's colour is $(ring_colour)"
    [ "$(parts_moving)" -gt 0 ] 2> /dev/null || problems="$problems; nothing moves in the preview"
    sound_on && problems="$problems; the preview makes a sound"
    screenshot s62_preview
    click .ring-preview 0 0.3
    on_screen .look-grid || problems="$problems; a tap does not close the preview"
    click .look-preview-link 0 0.3
    on_screen .ring-preview
    press_back
    on_screen .look-grid || problems="$problems; Back does more than close the preview"
    done_sheet
    click .save-btn 0 0.3
    # The real thing.
    if await_ring a 70 && on_screen .ring-off 15; then
        is_there .backdrop-waves || problems="$problems; the ring has not got the background"
        is_there .clock-mono || problems="$problems; the ring has not got the clock"
        [ "$(ring_colour)" = "#c06020" ] || problems="$problems; the ring's colour is $(ring_colour)"
        [ "$(parts_moving)" -gt 0 ] 2> /dev/null || problems="$problems; nothing moves on the ring"
        on_top .ring-off || problems="$problems; Turn off is under something"
        on_top .ring-snooze || problems="$problems; Snooze is under something"
        sound_on || problems="$problems; the ring is silent"
        screenshot s62_ring
        click .ring-off 0 0.3
        wait_for 5 sound_off || problems="$problems; still ringing after turning it off"
    else
        problems="$problems; did not ring: state=$(state a)"
    fi
    # With less motion it stands still.
    dev am force-stop "$PKG"
    use_settings '"Dark"' '"English"' '"setup_done":true,"language_follows_phone":false,"less_motion":true'
    start_app
    on_screen .alarm-main 10
    open_look
    click .look-preview-link 0 0.5
    on_screen .ring-preview
    [ "$(parts_moving)" = 0 ] || problems="$problems; $(parts_moving) things move with less motion on"
    screenshot s62_still
    press_back
    # A photo that is gone: the plain screen, nothing else changes.
    dev am force-stop "$PKG"
    fresh
    ALARM_EXTRA=',"ring_look":{"backdrop":"Photo","photo":"gone.jpg"}'
    seed b "$(alarm_json b 7 0 "$EVERY_DAY" null)"
    ALARM_EXTRA=
    start_app
    on_screen .alarm-main 10
    open_look
    click .look-preview-link 0 0.3
    on_screen .ring-preview || problems="$problems; no preview for a photo that is gone"
    is_there .ring-on-photo && problems="$problems; dressed for a photo that is gone"
    is_there .ring-off || problems="$problems; a photo that is gone takes the screen with it"
    press_back
    on_screen .look-grid
    # A photo of one's own, through the phone's picker: it differs per phone, and this looks for its
    # words. Larger than a screen needs: it comes in small.
    if on_emulator && [ -f "$OUT/s62_ring.png" ]; then
        # A ring can leave the screen off and locked, and a picker does not show over that.
        wake
        dev rm -f /sdcard/Download/af_photo.png
        adb_ push "$OUT/s62_ring.png" /sdcard/Download/af_photo.png > /dev/null
        dev content call --uri content://media --method scan_volume --arg external_primary > /dev/null 2>&1
        # It still has that gone photo: another one, from the link.
        click .look-other-photo 0 1
        photo_tap() {
            local i=0
            while [ "$i" -lt 8 ]; do
                ui_tap 'content-desc="af_photo.png' && return 0
                ui_tap 'text="af_photo.png"' && return 0
                if ui_tap 'content-desc="Show roots"'; then
                    sleep 1
                    ui_tap 'text="Downloads"'
                fi
                sleep 1.5
                i=$((i + 1))
            done
            return 1
        }
        if photo_tap; then
            chosen() { [ -n "$(as_app "ls files/ring_photos 2> /dev/null")" ]; }
            wait_for 20 chosen || problems="$problems; the photo was not kept"
            sleep 0.5
            click .look-preview-link 0 0.5
            on_screen .ring-on-photo 5 || problems="$problems; the photo does not show"
            case "$(js "return document.querySelector('.ring-photo').src.slice(0, 15)")" in
                data:image/jpeg) ;;
                *) problems="$problems; not the small copy" ;;
            esac
            [ "$(js "return Math.max(document.querySelector('.ring-photo').naturalWidth, document.querySelector('.ring-photo').naturalHeight)")" = 1600 ] ||
                problems="$problems; a photo of the wrong size"
            on_top .ring-off || problems="$problems; Turn off is under the photo"
            screenshot s62_photo
            press_back
        else
            problems="$problems; the picker did not offer the photo"
            screenshot s62_picker
            dev cat /sdcard/af_ui.xml | tr '>' '
' | grep -oE '(text|content-desc)="[^"]+"' | cut -c1-60 | head -24
            press_back
        fi
    fi
    if [ -z "$problems" ]; then
        pass "chosen, previewed, on the ring with its buttons free, still with less motion, and a lost photo is plain"
    else
        fail "${problems#; }"
    fi
}

# A ring that starts while the phone is in use: once the alarm's own screen is up, no banner of its
# notification covers it. Some phones keep the loud one's banner up for its whole time, over the date.
# Only this app's own words are looked for, and the screen's dump is gone again at once.
s63() {
    scenario s63_no_banner_over_the_ring
    fresh
    next_minute
    seed a "$(alarm_json a "$T_H" "$T_M" "$EVERY_DAY" null)"
    arm a
    dev input keyevent KEYCODE_HOME
    local problems="" seen="" t0
    banner_up() {
        dev uiautomator dump --windows /sdcard/af_ui.xml > /dev/null 2>&1
        local found=1
        dev cat /sdcard/af_ui.xml | grep -q 'text="Tap to open"' && found=0
        dev rm -f /sdcard/af_ui.xml
        return $found
    }
    if await_ring a 40 && on_screen .ring-off 15; then
        t0=$SECONDS
        sleep 1
        banner_up && seen="$seen $((SECONDS - t0))s"
        sleep 1
        banner_up && seen="$seen $((SECONDS - t0))s"
        screenshot s63_ring
        [ -z "$seen" ] || problems="$problems; its banner is over the alarm's screen at$seen after it came up"
        own_notifications | grep -q 'id=900[12]' || problems="$problems; no notification while it rings"
        click .ring-off 0 0.3
        wait_for 8 sound_off || problems="$problems; still ringing after turning it off"
    else
        problems="$problems; did not ring: state=$(state a)"
    fi
    if [ -z "$problems" ]; then
        pass "no banner over the alarm's screen, the notification stays in the list"
    else
        fail "${problems#; }"
    fi
}

# The notice before an alarm on a real phone, in real time, with the app really swiped away from
# recents: an emulator cannot show what the phone's maker does to that. Round one: closed before the
# notice comes, Skip on the pop-up, Undo, and it must ring. Round two: closed with the notice up, Skip
# by way of the chevron in the shade, and it must not ring. The shade holds other apps' messages and
# the picker the user's own files: only this app's own lines are read, and there are no pictures of
# either. PHOTO names a large JPEG for s62p, taken sideways: 4000x3000 with EXIF orientation 6.
swipe_away() {
    local x y
    dev input keyevent KEYCODE_RECENT_APPS
    sleep 1.5
    read -r x y <<< "$(dev wm size | grep -oE '[0-9]+x[0-9]+' | tail -1 | tr x ' ')"
    dev input swipe $((x / 2)) $((y / 2)) $((x / 2)) $((y / 12)) 220
    sleep 1.2
    dev input keyevent KEYCODE_HOME
    sleep 0.6
}
own_nodes() {
    dev uiautomator dump --windows /sdcard/af_ui.xml > /dev/null 2>&1
    dev cat /sdcard/af_ui.xml | tr '>' '\n' | grep -E 'text="(Alarm at [^"]*|Skip|SKIP|Undo|UNDO)"'
    dev rm -f /sdcard/af_ui.xml
}
tap_node() { # lines, what
    local bounds x1 y1 x2 y2
    bounds=$(printf '%s\n' "$1" | grep -m1 -E "$2" | sed -E 's/.*bounds="\[([0-9]+),([0-9]+)\]\[([0-9]+),([0-9]+)\].*/\1 \2 \3 \4/')
    [ -n "$bounds" ] || return 1
    read -r x1 y1 x2 y2 <<< "$bounds"
    dev input tap $(((x1 + x2) / 2)) $(((y1 + y2) / 2))
}
# In the shade: the button, after the chevron if it is folded away.
shade_tap() { # Skip or Undo
    local i=0 nodes t0=$SECONDS y
    while [ "$i" -lt 3 ]; do
        dev cmd statusbar expand-notifications
        sleep 1.2
        nodes=$(own_nodes)
        tap_node "$nodes" "text=\"($1|${1^^})\"" && { echo "$1 in the shade after $((SECONDS - t0))s"; return 0; }
        y=$(printf '%s\n' "$nodes" | grep -m1 'text="Alarm at' | sed -E 's/.*bounds="\[[0-9]+,([0-9]+)\]\[[0-9]+,([0-9]+)\].*/\1 \2/')
        if [ -n "$y" ]; then
            set -- "$1" $y
            dev input tap 965 $((($2 + $3) / 2))
            sleep 1
            nodes=$(own_nodes)
            tap_node "$nodes" "text=\"($1|${1^^})\"" && { echo "$1 under the chevron after $((SECONDS - t0))s"; return 0; }
        fi
        i=$((i + 1))
    done
    echo "$1 not found: $(printf '%s\n' "$nodes" | grep -oE 'text="[^"]*"' | tr '\n' ' ')"
    return 1
}
sql_alarm() { printf "SELECT data FROM alarms WHERE id='a';\n" | sql; }
own_notifications() { dev dumpsys notification --noredact | awk '/NotificationRecord\(/ { ours = /pkg=com\.fidderr\.alarmfree/ } ours'; }
reminded() { own_notifications | grep -q '"Skip"'; }
done_notice() { own_notifications | grep -q '"Undo"'; }
skipped() { sql_alarm | grep -q "\"skip_until_ms\":$((T * 1000))"; }
not_skipped() { sql_alarm | grep -q '"skip_until_ms":null'; }
start_round() {
    fresh
    # A notice two minutes before, which the panel does not offer but the app reads the same.
    next_minute 165
    ALARM_EXTRA=',"notify_before_minutes":2'
    seed a "$(alarm_json a "$T_H" "$T_M" "$EVERY_DAY" null)"
    ALARM_EXTRA=
    arm a
    ARMED=$(armed_at_ms a)
    echo "ring at $T_H:$T_M, $((T - $(now_s))) s from now"
    dev input keyevent KEYCODE_HOME
    sleep 0.6
}
s60p_a() {
    scenario s60p_closed_first_popup_skip_undo_rings
    local problems="" early t0
    start_round
    swipe_away
    wait_for 6 no_process || problems="$problems; still running after the swipe: main=$(main_pid) sentinel=$(sentinel_pid)"
    echo "swiped away, running: main=[$(main_pid)] sentinel=[$(sentinel_pid)]"
    wait_for 120 reminded || { fail "no notice with the app closed; $problems"; return; }
    early=$((T - $(now_s)))
    echo "notice came $early s before the ring, with the app closed"
    # It takes a moment to slide in, and a tap on it before that is a tap on the notification.
    sleep 1.5
    screenshot s60p_a_popup
    # On the pop-up itself, as a person does it, where it is right now: with or without the status
    # bar it sits at another height. In the time it lasts, about 6 s.
    local shown
    shown=$(own_nodes)
    tap_node "$shown" 'text="(Skip|SKIP)"' || problems="$problems; no Skip on the pop-up"
    t0=$SECONDS
    wait_for 20 skipped || problems="$problems; a tap on the pop-up did not skip: $(sql_alarm | grep -o '"skip_until_ms":[^,]*')"
    echo "skipped $((SECONDS - t0))s after the tap"
    echo "the phone's log says: $(dev logcat -d -b all -t 4000 | grep -oE 'notification_(action_)?clicked.{0,30}alarmfree' | tail -1 | cut -c1-40)"
    moved() { [ "$(armed_at_ms a)" = "$((ARMED + 86400000))" ]; }
    wait_for 10 moved || problems="$problems; armed for $(armed_at_ms a), was $ARMED"
    wait_for 8 done_notice || problems="$problems; nothing to take it back with"
    screenshot s60p_a_done_popup
    sleep 7
    # Back through the shade.
    shade_tap Undo || problems="$problems; no Undo to tap"
    dev cmd statusbar collapse
    wait_for 20 not_skipped || problems="$problems; undo did nothing"
    back_armed() { [ "$(armed_at_ms a)" = "$ARMED" ]; }
    wait_for 10 back_armed || problems="$problems; undo left it armed for $(armed_at_ms a)"
    wait_for 10 reminded || problems="$problems; the notice does not come back after undo"
    echo "taken back, $((T - $(now_s))) s to go, running: main=[$(main_pid)] sentinel=[$(sentinel_pid)]"
    # And it rings: a mis-tap cannot make you oversleep.
    if await_ring a 40 && on_screen .ring-off 20; then
        echo "rang $(($(now_s) - T)) s after its time"
        wait_for 8 sound_on || problems="$problems; the ring is silent"
        screenshot s60p_a_ring
        click .ring-off 0 0.3
        wait_for 8 sound_off || problems="$problems; still ringing after turning it off"
    else
        problems="$problems; did not ring after undo: state=$(state a) now=$(now_s) T=$T"
    fi
    if [ -z "$problems" ]; then
        pass "closed first: notice $early s before, skipped on the pop-up, taken back, and it rang"
    else
        fail "${problems#; }"
    fi
}
s60p_b() {
    scenario s60p_closed_with_notice_up_stays_silent
    local problems="" early t0
    start_round
    wait_for 120 reminded || { fail "no notice"; return; }
    early=$((T - $(now_s)))
    echo "notice came $early s before the ring"
    sleep 6.5
    swipe_away
    wait_for 6 no_process || problems="$problems; still running after the swipe: main=$(main_pid) sentinel=$(sentinel_pid)"
    echo "after the swipe: running main=[$(main_pid)] sentinel=[$(sentinel_pid)], Skip in our notice: $(own_notifications | grep -c '"Skip"')"
    wait_for 8 reminded || problems="$problems; the swipe took the notice away"
    shade_tap Skip || problems="$problems; no Skip to tap"
    dev cmd statusbar collapse
    t0=$SECONDS
    wait_for 20 skipped || problems="$problems; not skipped: $(sql_alarm | grep -o '"skip_until_ms":[^,]*')"
    echo "skipped $((SECONDS - t0))s after the tap, $((T - $(now_s))) s to go"
    moved() { [ "$(armed_at_ms a)" = "$((ARMED + 86400000))" ]; }
    wait_for 10 moved || problems="$problems; armed for $(armed_at_ms a), was $ARMED"
    wait_for 8 done_notice || problems="$problems; nothing to take it back with"
    # No ring at its time, also with the app dead.
    local left=$((T - $(now_s) + 25))
    [ "$left" -gt 0 ] && sleep "$left"
    sound_on && problems="$problems; rang although skipped"
    state_is a firing && problems="$problems; firing although skipped"
    done_notice && problems="$problems; still offering Undo after the ring's time"
    [ "$(armed_at_ms a)" = "$((ARMED + 86400000))" ] || problems="$problems; the next ring is at $(armed_at_ms a), expected $((ARMED + 86400000))"
    start_app
    on_screen .alarm-main 20
    echo "the list says: $(read_text .alarm-when)"
    screenshot s60p_b_home
    if [ -z "$problems" ]; then
        pass "closed with it up: notice $early s before, skipped by the chevron, silent at its time, next one armed"
    else
        fail "${problems#; }"
    fi
}
# A photo as a camera makes one, through the phone's own picker. Nothing of the picker goes in the
# log or in a picture: it shows the user's own files.
s62p() {
    if on_emulator || [ -z "${PHOTO:-}" ]; then
        note "s62p needs a phone and PHOTO"
        return
    fi
    scenario s62p_photo_from_the_phone
    fresh
    seed a "$(alarm_json a 7 0 "$EVERY_DAY" null)"
    arm a
    on_screen .alarm-main 15
    local problems="" i=0 picked="" box
    dev rm -f /sdcard/Download/af_photo.jpg
    adb_ push "$PHOTO" /sdcard/Download/af_photo.jpg > /dev/null
    dev content call --uri content://media --method scan_volume --arg external_primary > /dev/null 2>&1
    click .alarm-main
    on_screen .row-btn
    open_row 'Alarm screen'
    click_text '.look-tile-name' Photo 2.5
    echo "picker: $(focused_window | grep -oE '[a-z.]+/[A-Za-z.]+' | head -1)"
    while [ "$i" -lt 6 ]; do
        dev uiautomator dump /sdcard/af_ui.xml > /dev/null 2>&1
        box=$(dev cat /sdcard/af_ui.xml | tr '>' '\n' | grep -m1 -E '(text|content-desc)="af_photo' | sed -E 's/.*bounds="\[([0-9]+),([0-9]+)\]\[([0-9]+),([0-9]+)\].*/\1 \2 \3 \4/')
        dev rm -f /sdcard/af_ui.xml
        if [ -n "$box" ]; then
            set -- $box
            dev input tap $((($1 + $3) / 2)) $((($2 + $4) / 2))
            picked=1
            break
        fi
        sleep 1.5
        i=$((i + 1))
    done
    if [ -n "$picked" ]; then
        kept() { [ -n "$(as_app "ls files/ring_photos 2> /dev/null")" ]; }
        wait_for 25 kept || problems="$problems; the photo was not kept"
        on_screen .look-grid 15 || problems="$problems; not back on the panel"
        sleep 1
        echo "kept: $(as_app "ls -l files/ring_photos 2> /dev/null | tail -1" | awk '{print $5}') bytes"
        click .look-preview-link 0 0.8
        on_screen .ring-on-photo 8 || problems="$problems; the photo does not show"
        size=$(js "var p = document.querySelector('.ring-photo'); return p ? p.naturalWidth + 'x' + p.naturalHeight : 'none'")
        echo "shown at $size"
        [ "$size" = 1200x1600 ] || problems="$problems; the photo came in as $size, not upright at 1200x1600"
        screenshot s62p_photo
        press_back
        on_screen .look-grid 5
        done_sheet
    else
        problems="$problems; the picker did not show the photo"
        press_back
        sleep 1
    fi
    dev rm -f /sdcard/Download/af_photo.jpg
    dev content call --uri content://media --method scan_volume --arg external_primary > /dev/null 2>&1
    if [ -z "$problems" ]; then
        pass "a 12 MP photo taken sideways comes in small and upright, and shows behind the time"
    else
        fail "${problems#; }"
    fi
}

. "$(dirname "$0")/panels.sh"

ALL="s01 s03 s04 s05 s06 s07 s10 s11 s13 s13b s13d s14 s15 s16 s16b s17 s19 s22 s23a s23b s23c s25 s26x2 s28 s29 s30 s31 s32 s33 s34 s36 s37 s38 s39 s40 s42 s43 s44 s45 s46 s47 s48 s49 s50 s51 s52 s53 s55 s56 s57 s58 s59 s60 s61 s62 s63 big_screens s54"
ON_A_PHONE="s01 s03 s04 s05 s06 s07 s10 s15 s16 s16b s17 s22 s23a s23b s23c s25 s26x2 s28 s29 s30 s31 s32 s33 s36 s37 s38 s39 s42 s43 s45 s46 s47 s49 s50 s51 s52 s53 s55 s56 s57 s58 s59 s61 s62 s63 s60p_a s60p_b s62p"

: > "$RESULTS"
RUN_START=$SECONDS
# Started by itself, and not from a run that claimed the device already.
[ -n "${RUN_ID:-}" ] || claim_device
open_shells
if on_emulator; then
    dev settings put global auto_time 0
    # What an earlier run that did not get to its end left.
    dev wm size reset
    dev wm density reset
    dev locksettings set-disabled false > /dev/null
    # The one time Android 12 explains full screen, over the scanner, it takes the keys.
    dev settings put secure immersive_mode_confirmations confirmed
    # A clean app has no warning on its list: on an emulator that was just made it would have
    # one about the battery, which comes a moment after the list and moves it.
    dev dumpsys deviceidle whitelist +"$PKG" > /dev/null
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
