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
# A11Y=1: find and touch things through the accessibility tree (devtest/a11y.sh)
[ "${A11Y:-0}" = 1 ] && . devtest/a11y.sh
APK="${APK:-build/alarmfree-slint-debug.apk}"

# What the window shows, from its snapshot (lib.sh: ui, sn, on_screen, tap, ...). The screens are
# 0 alarms, 1 editor, 2 history, 3 settings, 4 reliability, 5 theme.
on_home() { await_ui "${1:-6}" "screen=0"; }
has_rows() { ui snapshot | grep -q '"rows":\[{'; }
on_card() { wait_for "${1:-6}" has_rows; }
# What the first card of the list says it does next, and its time.
row_when() { ui snapshot | grep -oE '"when":"[^"]*"' | head -1 | sed -E 's/^"when":"//; s/"$//'; }
row_time() { ui snapshot | grep -oE '"time":"[^"]*"' | head -1 | sed -E 's/^"time":"//; s/"$//'; }
row_labels() { ui snapshot | python -I -c 'import json,sys; print(" ".join(r["label"] for r in json.loads(sys.stdin.read()).get("rows", [])))'; }

# A scenario that only looks at the window and what it saves now runs headless, in `cargo test`
# (src/ui/flow_tests.rs): it stays a name here so that lists and results still show it.
moved_to_cargo() { # scenario name, test name
    scenario "$1"
    skip "moved to cargo test: ui::flow_tests::$2 (docs/test-moves.md)"
}

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
    on_card 10
    after=$(next_fire_ms a)
    says_snoozed() {
        card=$(row_when)
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
    on_home 10
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
    # When the window takes the second one up only after the first was dismissed (2 s later on a starved
    # emulator), the ring service has nothing to hand the sound to, and the second is silent until the
    # service's watchdog brings it back, 6 s on. Silent for good still fails.
    local silent_from=$SECONDS
    if [ "$(state $second)" = firing ] && wait_for 10 sound_on; then
        [ $((SECONDS - silent_from)) -lt 2 ] || note "s06: $second was silent for $((SECONDS - silent_from))s after dismissing $first, then rang"
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

# The ring's watchdog (an exact alarm a few seconds ahead) brings a ring back after a kill. A ring it has
# resumed must have the next watchdog armed at once: a second end of the process in the first moments after
# the resume then finds one. Without it only Android's own restart of the service is left, whose delay grows
# with every kill (1 s, then 4 s, 16 s, 64 s). A script from the host cannot hit that moment, so the debug build
# has a hook: with the file sentinel/debug_die_after_watchdog_resume its alarm process ends itself once, right
# after it asked for the resume, before its service exists. Kills from outside run Android's delay up until the watchdog is the first
# to come (it fires 6 s after the last tick), the hook ends that resumed ring, and the ring has to be back within
# the watchdog's own time. If the watchdog never came, there was no such moment: SKIP, never a PASS by luck.
s07d() {
    scenario s07d_end_again_just_after_the_watchdog_resumed_the_ring
    on_emulator || { skip "the kills need a root shell: emulators only"; return; }
    fresh
    next_minute
    seed a "$(alarm_json a "$T_H" "$T_M" "$EVERY_DAY" null)"
    arm a
    await_ring a 30 || { fail "never started: $RING_WHY"; return; }
    local hook=files/sentinel/debug_die_after_watchdog_resume
    as_app "mkdir -p files/sentinel && touch $hook"
    hook_taken() { [ "$(as_app "test -e $hook && echo there || echo gone")" = gone ]; }
    local kills=0 at=""
    while [ "$kills" -lt 5 ]; do
        kill_sentinel
        kills=$((kills + 1))
        # 8 s: the watchdog comes 6 s after the last tick, if the service is not back before it.
        if wait_for 8 hook_taken; then
            at=$(now_s)
            break
        fi
    done
    if [ -z "$at" ]; then
        as_app "rm -f $hook"
        skip "the watchdog did not resume the ring in $kills kills, so there was no moment to hit (sound=$(sound_on && echo on || echo OFF), sentinel=$(sentinel_pid))"
        return
    fi
    # The hook ends the process at once: the sound goes off, and only then does a sound mean the ring is back.
    if ! wait_for 5 sound_off; then
        fail "the hook took its file after $kills kills, but the ring process did not end (sound still on)"
        return
    fi
    up_again() { sound_on && [ -n "$(main_pid)" ] && top_is_app; }
    # The watchdog's time: 6 s ahead, and the ring started again. Android's own restart after this end is 16 s or more.
    until up_again || [ "$(now_s)" -gt $((at + 12)) ]; do sleep 0.3; done # lint-ok: polling with its own bound
    if up_again; then
        pass "back $(($(now_s) - at))s after the process ended just after the watchdog resumed the ring (kills from outside: $kills)"
    else
        fail "not back 12s after the process ended just after the watchdog resumed the ring: sound=$(sound_on && echo on || echo OFF) state=$(state a) main=$(main_pid) sentinel=$(sentinel_pid)"
    fi
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
    # The outcome is written first and the next ring armed a moment later (seen on a busy Android 14: armed='' when
    # read at once): wait until it is armed for a time to come.
    next_armed() { [ "$(armed_at_ms d)" -gt "$(($(now_s) * 1000))" ] 2> /dev/null; }
    wait_for 20 next_armed
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
    if [ -n "${SECOND_ALARM:-}" ]; then
        # Two rings in one restart: a rings at the boot (it was due 2 minutes ago and comes again tomorrow), b is due an hour
        # on and rings when the clock is moved there, the phone still locked. B_T is its time.
        local a_h a_m
        read -r a_h a_m <<< "$(at_hm $((T - 7200)))"
        seed a "$(alarm_json a "$a_h" "$a_m" "$EVERY_DAY" null air 0 0 3 1 '[]' "${2:-1800}")"
        seed b "$(alarm_json b "$T_H" "$T_M" "$EVERY_DAY" null air 0 0 3 1)"
        B_T=$T
        arm a
        wait_file files/sentinel/armed/b.armed 5
    else
        seed a "$(alarm_json a "$T_H" "$T_M" "$EVERY_DAY" null air 0 0 3 1 '[]' "${2:-1800}")"
        arm a
    fi
    dev locksettings set-pin 1234 > /dev/null
    sys_wait "$SYS_DEADLINE" "the PIN being set" pin_is_set || { fail "$SYS_WHY"; return 1; }
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
    sys_wait "$SYS_DEADLINE" "the unlock screen on top" top_is_unlock || { fail "rang, but $SYS_WHY"; return 1; }
}
# After a scenario that set the PIN, whatever became of it: the PIN is gone (read), and so is the lock screen that Android
# 10 to 12 keep up after it (a note when it will not go: the next scenario's baseline puts it away).
no_pin() {
    dev locksettings clear --old 1234 > /dev/null
    OVERLAY=
    SCREEN_WENT_OFF=1
    sys_wait "$SYS_DEADLINE" "the PIN being cleared" pin_unset || note "$SYS_WHY"
    lock_away || note "the PIN is cleared but the lock screen stays: $SYS_WHY"
    # The keyboard of the PIN pad can come up a moment after the lock screen went: wait for it a little, then put it down.
    sys_wait 3 "the keyboard coming up" ime_shown > /dev/null
    sys_keyboard_down || note "$SYS_WHY"
    ime_note "after the PIN was put away"
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
# Every step on a screen of Android (the unlock screen, Android's unlock and its PIN pad) waits for a state read from the
# device, acts once and confirms (devtest/system.sh).
s13() {
    no_lock_here s13 && return
    scenario s13_locked_restart_unlock
    # Without the permission to start a screen from the background: the way through the button
    # must not need it.
    OVERLAY=deny locked_ring 4 || { no_pin; return; }
    local problems="" rang=$SOUND_AT
    # A phone's cleaner can take a process with nothing on screen. The rounds in which that
    # could count against the app's screen pass while the quiet below runs out.
    kill_main
    sdk_unlock_press || problems="$problems; $SYS_WHY"
    # Left alone, the phone stays locked: the quiet runs out.
    sys_wait "$SYS_DEADLINE" "the sound coming back when the quiet ran out" sound_on || problems="$problems; $SYS_WHY"
    user_unlocked && problems="$problems; the phone unlocked by itself"
    # Android's unlock is still up from the first time: away with it, and ask again.
    sdk_unlock_cancel || problems="$problems; $SYS_WHY"
    sys_wait "$SYS_DEADLINE" "the unlock screen on top after cancelling the unlock" top_is_unlock || problems="$problems; $SYS_WHY"
    screenshot s13_unlock_screen
    sys_wait "$SYS_DEADLINE" "the sound after cancelling the unlock" sound_on || problems="$problems; silent after cancelling the unlock: $SYS_WHY"
    sdk_unlock_press || problems="$problems; the second Unlock: $SYS_WHY"
    ui_given_up && problems="$problems; the core dying while locked counted as a failing screen"

    unlock_pin 0 || problems="$problems; the PIN did not unlock the phone: $SYS_WHY"
    sys_wait "$SYS_DEADLINE" "the app's window answering after the unlock" page_is_up || note "$SYS_WHY"
    if ! on_screen "Turn off" 30; then
        screenshot s13_after_unlock
        fail "the alarm's own screen did not come up after the unlock: $(top_activity)${problems}"
        no_pin
        return
    fi
    # The keyboard of the PIN pad comes up again with the app's window (Android 14 and 16) and covers the buttons.
    sys_keyboard_down || problems="$problems; $SYS_WHY"
    top_is_main || problems="$problems; the alarm's screen is not on top: $(top_activity)"
    sys_wait "$SYS_DEADLINE" "the sound on the alarm's own screen" sound_on || problems="$problems; silent on the alarm's own screen"
    state_is a firing || problems="$problems; state $(state a) on the alarm's own screen"
    finished() { sound_off && state_is a scheduled && [ "$(outcomes solved)" = 1 ]; }
    # One tap through the app (it waits for the button and the window to stand still), then the system deadline.
    tap "Turn off" 0 0.5
    wait_for "$SYS_DEADLINE" finished || problems="$problems; after turning it off: sound=$(sound_on && echo ON || echo off) state=$(state a) solved=$(outcomes solved)"
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
# In the same restart, first: nobody comes, the ring ends at its limit with the phone still locked, the unlock
# screen goes with it, and the history tells after the unlock (s13d_locked_restart_limit, its verdict comes
# at the end, when the unlock has made the history readable). Two rings, one restart instead of two.
s13b() {
    no_lock_here s13b && return
    scenario s13d_locked_restart_limit
    SECOND_ALARM=1 locked_ring 60 8 || { no_pin; return; }
    local problems="" limit_problems="" rang=$SOUND_AT
    await_silence 60 || limit_problems="$limit_problems; still ringing 60s after the boot with a limit of 8s"
    local stopped=$STOPPED_AT
    unlock_gone() { ! top_is_unlock; }
    sys_wait "$SYS_DEADLINE" "the unlock screen going away after the ring ended" unlock_gone || limit_problems="$limit_problems; the unlock screen stayed after the ring ended"
    user_unlocked && limit_problems="$limit_problems; the phone unlocked by itself"
    local d_took
    d_took=$(took)
    S13D_DONE=1

    scenario s13b_locked_restart_lock_screen
    T=$B_T
    set_clock $((B_T - 3))
    await_sound 30 || { fail "b did not ring in the locked phone after the clock moved: main=$(main_pid) os_alarm=$(os_has_alarm && echo yes || echo NO)"; no_pin; return; }
    sys_wait "$SYS_DEADLINE" "the unlock screen on top" top_is_unlock || { fail "rang, but $SYS_WHY"; no_pin; return; }
    sdk_unlock_press || problems="$problems; $SYS_WHY"
    kill_sentinel
    sys_wait "$SYS_DEADLINE" "the sound within the time of the ring service being killed in the quiet" sound_on || problems="$problems; no sound after the ring service was killed in the quiet: $SYS_WHY"
    sys_wait "$SYS_DEADLINE" "the unlock screen coming back after that" top_is_unlock || problems="$problems; $SYS_WHY"
    # Android's unlock can still be up from the button, and it takes Home for itself: away with it first (only when a read
    # shows its pad), then Home.
    if pad_read && [ "$PAD_UP" = 1 ]; then
        sdk_unlock_cancel || problems="$problems; $SYS_WHY"
    fi
    home_from_unlock || :
    sys_wait "$SYS_DEADLINE" "the sound after Home" sound_on || problems="$problems; silent after Home"
    unlock_pin 1 || problems="$problems; the PIN did not unlock the phone: $SYS_WHY"
    sys_wait "$SYS_DEADLINE" "the app's window answering after the unlock" page_is_up || note "$SYS_WHY"
    if ! on_screen "Turn off" 30; then
        screenshot s13b_after_unlock
        fail "the alarm's own screen did not come up after unlocking on the lock screen: $(top_activity)${problems}"
        no_pin
        return
    fi
    sys_keyboard_down || problems="$problems; $SYS_WHY"
    sys_wait "$SYS_DEADLINE" "the sound on the alarm's own screen" sound_on || problems="$problems; silent on the alarm's own screen"
    finished() { sound_off && state_is b scheduled && [ "$(outcomes solved)" = 1 ]; }
    # As in s13: one tap through the app, then the system deadline.
    tap "Turn off" 0 0.5
    wait_for "$SYS_DEADLINE" finished || problems="$problems; after turning it off: sound=$(sound_on && echo ON || echo off) state=$(state b) solved=$(outcomes solved)"
    if [ -z "$problems" ]; then
        pass "the ring came back after its service died in the quiet, and unlocking on the lock screen brought the alarm's own screen"
    else
        fail "${problems#; }"
    fi

    # Only now can the history be read.
    CURRENT=s13d_locked_restart_limit
    recorded() { [ "$(outcomes limit)" = 1 ] && state_is a scheduled; }
    wait_for 30 recorded || limit_problems="$limit_problems; after the unlock: limit=$(outcomes limit) state=$(state a)"
    if [ -z "$limit_problems" ]; then
        echo "PASS  $CURRENT  [$d_took] stopped at its limit ${stopped}s after the boot while locked (rang ${rang}s after it), and the history says so" | tee -a "$RESULTS"
    else
        echo "FAIL  $CURRENT  [$d_took] ${limit_problems#; }" | tee -a "$RESULTS"
        write_log
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
    home_from_unlock || :
    unlock_pin 1 || problems="$problems; the PIN did not unlock the phone: $SYS_WHY"
    wait_for 8 top_is_main && how="own screen up by itself"
    sys_wait "$SYS_DEADLINE" "the sound after the unlock" sound_on || problems="$problems; silent after the unlock"
    state_is a firing || problems="$problems; state $(state a) after the unlock"
    top_is_backup && problems="$problems; the backup screen came up"
    start_app
    if ! on_screen "Turn off" 30; then
        fail "the alarm's own screen is not there when the app is opened: $(top_activity)${problems}"
        no_pin
        return
    fi
    sys_keyboard_down || problems="$problems; $SYS_WHY"
    press ring-turn-off
    finished() { sound_off && state_is a scheduled && [ "$(outcomes solved)" = 1 ]; }
    wait_for "$SYS_DEADLINE" finished || problems="$problems; after turning it off: sound=$(sound_on && echo ON || echo off) state=$(state a) solved=$(outcomes solved)"
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
    # s13b did it in its restart; by name alone it runs on its own.
    [ -z "${S13D_DONE:-}" ] || return 0
    scenario s13d_locked_restart_limit
    locked_ring 60 8 || { no_pin; return; }
    local problems=""
    await_silence 60 || problems="$problems; still ringing 60s after the boot with a limit of 8s"
    unlock_gone() { ! top_is_unlock; }
    sys_wait "$SYS_DEADLINE" "the unlock screen going away after the ring ended" unlock_gone || problems="$problems; the unlock screen stayed after the ring ended"
    user_unlocked && problems="$problems; the phone unlocked by itself"
    unlock_pin 1 || problems="$problems; the PIN did not unlock the phone: $SYS_WHY"
    recorded() { [ "$(outcomes limit)" = 1 ] && state_is a scheduled; }
    wait_for 30 recorded || problems="$problems; after the unlock: limit=$(outcomes limit) state=$(state a)"
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
    on_screen "Turn off" 10
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
    on_screen "Turn off" || problems="$problems; no alarm screen after rotating"
    # The screen is set to go off after 10 s: a ring has to hold it.
    [ "$(wakefulness)" = Awake ] || problems="$problems; the screen went to sleep while ringing"
    dev settings put system user_rotation 0
    # Unfolding, as a screen size.
    dev wm size 2200x2480
    sleep 1.5
    screenshot s35_unfolded
    [ "$(main_pid)" = "$p0" ] || problems="$problems; resizing ended the process"
    sound_on || problems="$problems; silent after resizing"
    on_screen "Turn off" || problems="$problems; no alarm screen after resizing"
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
    # The sound is off before the service shuts down and puts the volume back (run 45145328118, Android 13: 0.6 s later).
    volume_is_back() { [ "$(alarm_volume)" = 1 ]; }
    wait_for 5 volume_is_back
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
        on_screen "Turn off" 10
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
    on_screen "Turn off" 10
    p0=$(main_pid)
    action hang_ui
    wait_for 30 new_main
    if sys_wait "$SYS_DEADLINE" "the backup screen on top" top_is_backup; then
        screenshot s22b_backup
        ended() { sound_off && state_is a scheduled && [ "$(outcomes backup)" = 1 ]; }
        if backup_hold ended; then
            pass "backup screen took over, solving it ended the alarm"
        else
            fail "after solving: sound=$(sound_on && echo ON || echo off) state=$(state a) backup=$(outcomes backup): $SYS_WHY"
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
    on_screen "Turn off" 10
    sleep 0.7
    as_app "touch -d 2020-01-01T00:00:00 files/sentinel/ringing/activity"
    # A busy machine can spend most of the 9 s on getting the ring screen up, and the ring then ends before the touch is made:
    # that is the machine, so it is asked again (twice at most); the test itself stays as it is.
    if [ "$(now_s)" -gt $((T + 3)) ] && [ "${S23B_TRY:-0}" -lt 2 ]; then
        retry_used "s23b: the ring screen took $(($(now_s) - T))s of the 9s limit"
        RETRY_CARRY=$RETRY_NOTE
        S23B_TRY=$((${S23B_TRY:-0} + 1))
        s23b
        return
    fi
    # Late enough to tell: it ends at 9 seconds without this touch, and at 13 with it.
    until_past $((T + 4))
    local size
    size=$(dev wm size | tail -1 | sed 's/.*: //')
    # On the alarm's clock, where a touch does nothing else.
    # The limit of the ring is 9 s: the touch is waited for 1.5 s, not 8.
    FINGER_ACK_MS=1500 finger_xy $((${size%x*} / 2)) $((${size#*x} / 5))
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
        elif on_emulator && [ "${S26_AGAIN:-0}" -lt 2 ] && [ -n "$ms" ] && [ "${delivery:-0}" -ge 1000 ]; then
            # The cold start of the window starves the sound of its CPU on an emulator with 2 cores and
            # no GPU (seen: 3 to 7 s from the broadcast to the sound, 0.1 s on a phone and on a quiet
            # emulator). A real fault repeats: it counts only when it does it three times running.
            # What the guest was doing while the sound waited (the open question of real-questions #2: CPU or one slow step).
            {
                echo "=== s26 run $run, ask $((${S26_AGAIN:-0} + 1)): sound ${delivery}ms after delivery, ${ms}ms after its time"
                dev "top -H -b -n 1 2> /dev/null | head -25; ps -A 2> /dev/null | grep -E 'dex2oat|odrefresh|compile' | head; dumpsys cpuinfo 2> /dev/null | head -12; dumpsys jobscheduler 2> /dev/null | grep -iE 'dexopt|idle' | head -6" | tr -d '\r'
                echo "--- the app's timing marks"
                adb_ logcat -d -b all 2> /dev/null | tr -d '\r' | grep -E 'MobileSentinel.(FiringAudio|Ring|Timing)|JOB_HEADS_UP|timing' | tail -30 | cut -c1-200
            } >> "$OUT/s26_slow.txt" 2> /dev/null
            retry_used "s26: the sound came ${delivery}ms after delivery"
            RETRY_CARRY=$RETRY_NOTE
            dev dumpsys deviceidle unforce > /dev/null
            S26_AGAIN=$((${S26_AGAIN:-0} + 1)) s26 "$run"
            return
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

# What sticks out of the window at the sides (the middle of a text out of its width), if anything.
clipped() {
    local r
    # A window that has just come up lays itself out more than once (Android 12 and 13 on a big screen: every text
    # "outside" for a moment): look when no frame is being drawn.
    UI_QUIET=1 await_settled 6
    r=$(ui overflow)
    [ "$r" = none ] || echo "$r"
}

# The sum on screen, answered with the keys.
solve_math() {
    local digit digits
    SUM=$(all_texts | grep -m1 -E '^[0-9]+ ?[+×−x*-] ?[0-9]+')
    ANSWER=$(($(echo "$SUM" | sed 's/×/*/; s/−/-/; s/ //g')))
    digits="$ANSWER"
    while [ -n "$digits" ]; do
        digit=${digits:0:1}
        digits=${digits:1}
        # The key is the text of one single digit, lower on the screen than the sum.
        tap_in "$digit" L 0.1
    done
    # The sum is solved by its last digit (src/challenges/math.rs): the challenge closes by itself, and OK only says that what is
    # typed is not the answer. There is nothing to confirm, and no OK to tap on a screen that is already leaving.
}

# In a challenge the sound is quiet. Doing nothing brings it back. Turning it off through the
# screens, solving a sum, and landing on history.
s28() {
    scenario s28b_idle_challenge_rings_again
    fresh
    shorten idle_secs 4
    # A longer lead than the usual 6 s: on a busy emulator the seeding and arming took the whole of it
    # now and then, and the alarm never rang in the 30 s that followed.
    next_minute 14
    seed a "$(alarm_json a "$T_H" "$T_M" "$EVERY_DAY" null air 0 0 3 1 '[["challenge.math","{\"difficulty\":\"easy\"}"]]')"
    arm a
    if ! await_ring a 30; then
        # Seen on the Android 12 emulator now and then: the system stalls for ~13 s around the minute
        # (the window's first frame takes that long), and the alarm armed for it never arrives, with
        # nothing in the log. A second start is the same test; it fails when that happens twice.
        if on_emulator && [ -z "${S28_AGAIN:-}" ]; then
            retry_used "s28: the alarm armed for $T did not ring (state=$(state a))"
            RETRY_CARRY=$RETRY_NOTE
            S28_AGAIN=1 s28
            unset S28_AGAIN
            return
        fi
        fail "never started: T=$T now=$(now_s) state=$(state a) job=$(job_status a) armed=$(armed_at_ms a)"
        return
    fi
    on_screen "Turn off" 10
    tap "Turn off" 0 0.3
    wait_for 4 sound_off || { fail "still loud in the challenge"; return; }
    if wait_for 12 sound_on && on_screen "Turn off"; then
        pass "ringing again after doing nothing"
    else
        fail "sound=$(sound_on && echo ON || echo off)"
        return
    fi

    scenario s28_solve_a_challenge
    shorten idle_secs 30
    screenshot s28_ringing
    local wide
    wide=$(clipped)
    tap "Turn off" 0 0.3
    on_screen "Solve the math problem"
    screenshot s28_challenge
    wide="$wide$(clipped)"
    [ -z "$wide" ] || { fail "clipped: $wide"; return; }
    wait_for 4 sound_off
    solve_math
    solved() { state_is a scheduled && [ "$(outcomes solved)" = 1 ]; }
    if wait_for 8 solved && await_ui 8 "screen=2" && ! sound_on; then
        screenshot s28_after
        pass "$SUM = $ANSWER turned it off, recorded, on history"
    else
        fail "sound=$(sound_on && echo ON || echo off) state=$(state a) solved=$(outcomes solved) sum='$SUM' screen=$(screen_no)"
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
    on_screen "Turn off" 10
    tap "Turn off" 0 0.5
    waiting() { [ "$(db_count '"hidden":true')" = 1 ]; }
    wait_for 5 waiting || { fail "no check waiting: $(db_count '"hidden":true')"; return; }
    wait_for 3 sound_off || { fail "still ringing after turning it off"; return; }
    local id label
    id=$(printf "SELECT id FROM alarms WHERE data LIKE '%%\"hidden\":true%%';\n" | sql)
    wait_file "files/sentinel/context/$id.json" 5
    T=$(($(next_fire_ms "$id") / 1000))
    jump_to $((T - 3))
    await_sound 30 || { fail "the check never rang"; return; }
    on_screen "Turn off" 10
    screenshot s29_check
    label=$(sn ring_label)
    tap "Turn off" 0 0.5
    finished() { sound_off && [ "$(db_count '"hidden":true')" = 0 ] && [ "$(outcomes solved)" = 1 ]; }
    if wait_for 8 finished && [ "$label" = "Are you up?" ]; then
        pass "rang again after ${SOUND_AT}s, and is gone after confirming"
    else
        fail "sound=$(sound_on && echo ON || echo off) hidden=$(db_count '"hidden":true') solved=$(outcomes solved) label='$label'"
    fi
}

# ---- The screens ----

# A panel is open when the snapshot names one.
sheet_open() { [ "$(sn home_sheet)" != None ] && [ -n "$(sn home_sheet)" ]; }
sheet_closed() { [ "$(sn home_sheet)" = None ]; }
preview_up() { [ "$(sn ed_preview)" = true ]; }
preview_down() { [ "$(sn ed_preview)" = false ]; }
editor_sheet_open() { [ "$(sn ed_sheet)" != None ] && [ -n "$(sn ed_sheet)" ]; }
editor_sheet_closed() { [ "$(sn ed_sheet)" = None ]; }
on_editor() { await_ui "${1:-6}" "screen=1"; }

# Back from the editor stays in the app.
s25() {
    scenario s25_back_navigation
    fresh
    next_minute 3600
    seed a "$(alarm_json a "$T_H" "$T_M" "$EVERY_DAY" null)"
    arm a
    on_card 30 || { fail "the alarm list did not come up in 30 s"; return; }
    local p0 problems=""
    p0=$(main_pid)
    # Sheets, editor, question and the main screens are checked in cargo test
    # (ui::flow_tests::s25_back_navigation). What needs the phone: Back on Home sends the app to
    # the background, without ending anything.
    local seen
    seen="screen=$(sn screen) sheet=$(sn home_sheet) panel=$(sn panel_open) ringing=$(sn ringing) preview=$(sn ed_preview)"
    window_past_late_resize
    press_back
    wait_for 5 not_on_top || problems="$problems; Back on Home stayed on top ($seen; now $(sn screen) $(sn home_sheet) $(sn panel_open))"
    [ "$(main_pid)" = "$p0" ] || problems="$problems; main $p0 to $(main_pid)"
    start_app
    on_home 20 || problems="$problems; not back on Home"
    if [ -z "$problems" ]; then
        pass "Back on Home goes to the background, same process"
    else
        fail "${problems#; }"
    fi
}
not_on_top() { ! top_is_app; }

# A nap in two taps, and Undo.
s30() {
    scenario s30_quick_alarm_and_undo
    fresh
    start_app
    on_home 20
    tap "+5" 0 0.5
    local made toast_text left
    made=$(db_count '"delete_after_ring":true')
    toast_text=$(sn toast_text)
    wait_for 3 os_has_alarm || { fail "not armed: made=$made"; return; }
    tap Undo 0 0.5
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
    on_card 10
    local before during after text_paused week=$((6 * 24 * 3600 * 1000))
    before=$(armed_at_ms a)
    tap "Pause all alarms until a date" 0 0.4
    on_screen "For a week"
    tap "For a week" 0 0.4
    armed_moved() { [ "$(armed_at_ms a)" != "$1" ]; }
    wait_for 5 armed_moved "$before"
    during=$(armed_at_ms a)
    text_paused=$(row_when)
    tap Undo 0 0.4
    wait_for 5 armed_moved "$during"
    after=$(armed_at_ms a)
    if [ "$during" -ge "$((before + week))" ] && [ "$after" = "$before" ]; then
        pass "moved past the week ($text_paused), undone"
    else
        fail "armed: before $before, during $during, after $after"
    fi
}

# The trash can in the editor's header: a picture with no name, at the right end of the header.
tap_trash() {
    local w s y
    read -r w _ s <<< "$(window_size)"
    # On the title's line: its height depends on the status bar.
    y=$(where "$(sn ed_title)" | cut -d' ' -f2)
    case "$y" in '' | *[!0-9]*) y=$(awk -v s="$s" 'BEGIN { printf "%d", 56 * s }') ;; esac
    tap_xy "$(awk -v w="$w" -v s="$s" 'BEGIN { printf "%d", w - 22 * s }')" "$y"
    sleep 0.5
}

# The new-alarm button, until the editor is there (it takes touches only once it has settled).
open_new_editor() {
    tap "Create new alarm" 0 0.8
    on_editor 2 && return 0
    step_failed "open_new_editor: no editor came up after the tap: screen $(screen_no)"
}

# The bottom bar and the reliability screen.
open_reliability() {
    go_tab Settings || return 1
    await_ui 5 "screen=3" || return 1
    tap Reliability 0 0.3
    await_ui 5 "screen=4"
}

# The test alarm can be called off, rings for real, leaves nothing behind, and its record can be saved.
s32() {
    scenario s32_test_alarm
    fresh
    start_app
    on_home 20
    open_reliability || { fail "no reliability screen"; return; }
    tap "Test alarm" L 0.5
    test_waiting() { [ "$(db_count '"hidden":true')" = 1 ]; }
    wait_for 5 test_waiting || { fail "no test alarm"; return; }
    # It can be called off, and then nothing is armed. Set again for the ring.
    on_screen "Cancel the test alarm" 3 || { fail "no way to call the test alarm off"; return; }
    tap "Cancel the test alarm" 0 0.5
    test_off() { [ "$(db_count '"hidden":true')" = 0 ] && [ -z "$(as_app 'ls files/sentinel/armed 2>/dev/null')" ]; }
    wait_for 5 test_off || { fail "the test alarm was not called off: hidden=$(db_count '"hidden":true') armed=$(as_app 'ls files/sentinel/armed 2>/dev/null' | tr '\n' ' ')"; return; }
    on_screen "Test alarm" 3 || { fail "no test button after calling one off"; return; }
    tap "Test alarm" L 0.5
    wait_for 5 test_waiting || { fail "no second test alarm"; return; }
    local id record=no
    id=$(printf "SELECT id FROM alarms WHERE data LIKE '%%\"hidden\":true%%';\n" | sql)
    wait_file "files/sentinel/context/$id.json" 5
    go_home
    T=$(($(next_fire_ms "$id") / 1000))
    jump_to $((T - 3))
    await_sound 30 || { fail "it never rang"; return; }
    on_screen "Turn off" 10
    # By the window's own events: a finger's tap in the moment Android resizes the new window is lost, and what counts here is the record.
    tap_in "Turn off" 0 0.5
    wait_for 5 sound_off
    rm_download alarmfree-rings.txt
    start_app
    wait_for 20 page_is_up
    # The window is up before its bottom bar has painted, on a loaded emulator.
    # The window comes back on the screen it was left on: the one the ring was started from.
    on_screen_no 4 || { on_screen Settings 15 || screenshot s32_no_settings; open_reliability; }
    # The page follows the ring that ended while it was open: its button is "Test alarm" again, not "Cancel the test alarm",
    # and the record can be saved. Without that the tap below met a page that changed under it.
    on_screen "Test alarm" 10 || { fail "the reliability screen still waits for the test alarm after it rang: $(texts_of | cut -c1-200)"; return; }
    # The row is below the page's window until the screen is scrolled (the tree lists a row only once part of it is in the window,
    # and then its middle can still be behind the Test alarm button): the tap waits until the whole row is above the bar.
    tap_above_bar "Save the record of how fast alarms rang" "Test alarm" "Cancel the test alarm"
    # The file is written a moment after the tap: wait for what is in it, not for half a second.
    has_record() { [ "$(dev cat /sdcard/Download/alarmfree-rings.txt 2> /dev/null | grep -c ' sound ')" -ge 1 ]; }
    wait_for 10 has_record && record=yes
    rm_download alarmfree-rings.txt
    if ! sound_on && [ "$(db_count '"hidden":true')" = 0 ] && [ "$(outcomes solved)" = 0 ] && [ "$record" = yes ]; then
        pass "rang ${SOUND_AT}s after its time, nothing left, nothing in history, its record saved"
    elif [ "$record" != yes ]; then
        fail "no record of the ring in Downloads (test alarms now: $(db_count '"hidden":true'))"
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
    local problems="" locale result locales="Dutch Spanish French German Finnish Turkish"
    # On a big screen only the longest words, each language is a restart.
    [ -z "${VARIANT:-}" ] || locales=German
    fresh
    next_minute 3600
    seed a "$(alarm_json a "$T_H" "$T_M" "$EVERY_DAY" null)"
    dev settings put system font_scale 1.3
    for locale in $locales; do
        use_settings '"Dark"' "\"$locale\"" '"setup_done":true,"language_follows_phone":false'
        start_app
        on_card 10
        result=$(clipped)
        [ -z "$result" ] || problems="$problems $locale/home: $result;"
        press open a 0.6
        await_ui 5 "screen=1"
        result=$(clipped)
        [ -z "$result" ] || problems="$problems $locale/editor: $result;"
        press back
        await_ui 5 "screen=0"
        press go 1 0.5
        result=$(clipped)
        # The period chips are a row that scrolls: what sticks out has to come into the window by a swipe.
        if [ -n "$result" ]; then
            finger_swipe 900 341 100 341 300
            sleep 0.8
            after=$(clipped)
            # The longest words need more than one swipe to come in (a swipe moves the row about 800 px and the row is longer): look
            # when the row has come to rest, and swipe again while the same chip is still out, three more times at most (a row that
            # cannot be scrolled in stays out through all of it). Runs 45207471615 and 45218660851: this was an ask-again that fired
            # on machines that were not starved, in Finnish, and the second swipe always brought the chip in.
            for _ in 1 2 3; do
                for _ in 1 2 3 4 5 6; do
                    case "$after" in *"$result"*) sleep 0.8; after=$(clipped) ;; *) break ;; esac
                done
                case "$after" in *"$result"*) finger_swipe 900 341 100 341 300 ;; *) break ;; esac
            done
            case "$after" in *"$result"*) after=$(clipped) ;; esac
            case "$after" in
                *"$result"*) problems="$problems $locale/history: $result stays out after scrolling;" ;;
            esac
            result=""
        fi
        [ -z "$result" ] || problems="$problems $locale/history: $result;"
        press go 2 0.5
        result=$(clipped)
        [ -z "$result" ] || problems="$problems $locale/settings: $result;"
        press st-row 9 0.6
        result=$(clipped)
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

# Hold at a place, then move down by `$3` device pixels, as a finger does.
hold_and_drag() { # x y pixels
    local step
    case "$1$2$3" in
        '' | *[!0-9-]*)
            echo "nothing to drag: $1 $2 $3"
            return 1
            ;;
    esac
    dev input motionevent DOWN "$1" "$2"
    sleep 0.6
    for step in 1 2 3 4; do
        dev input motionevent MOVE "$1" $(($2 + $3 * step / 4))
        sleep 0.05
    done
    sleep 0.2
    dev input motionevent UP "$1" $(($2 + $3))
    sleep 0.5
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
    on_card 10
    # Android resizes the window once more about a second after the first frame; a hold that starts in that moment is lost.
    sleep 2.5
    local before after again gap problems="" ax ay cx cy
    before=$(row_labels)
    read -r ax ay <<< "$(where a)"
    read -r cx cy <<< "$(where c)"
    case "$ay$cy" in
        '' | *[!0-9]*)
            fail "no two cards to measure: $ay $cy"
            return
            ;;
    esac
    gap=$((cy - ay))
    hold_and_drag "$ax" "$ay" $((gap + 30))
    after=$(row_labels)
    dev am force-stop "$PKG"
    start_app
    on_card 10
    again=$(row_labels)
    [ "$before" = "a b c" ] || problems="$problems; started as '$before'"
    [ "$after" = "b c a" ] || problems="$problems; after dragging '$after'"
    [ "$again" = "$after" ] || problems="$problems; after restarting '$again'"
    # In the editor. The first card is now b, a is the last.
    press open a 0.8
    on_editor
    open_row Challenges
    on_screen Math 4
    local rows_before rows_after hx hy hy2
    rows_before=$(all_texts | grep -E '^(Math|Memory)$' | tr '\n' ' ')
    # The grip is a bare strip at the left of each row, left of its number.
    hx=$(awk -v s="$(window_size | cut -d' ' -f3)" 'BEGIN { printf "%d", 38 * s }')
    read -r _ hy <<< "$(where Math)"
    read -r _ hy2 <<< "$(where Memory)"
    if [ -n "$hy2" ] 2> /dev/null; then
        hold_and_drag "$hx" "$hy" $((hy2 - hy + 20))
    else
        problems="$problems; no handles to drag by"
    fi
    rows_after=$(all_texts | grep -E '^(Math|Memory)$' | tr '\n' ' ')
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
    on_card 10
    sleep 0.5
    swipe_card() {
        local x y size w
        read -r x y <<< "$(where a)"
        size=$(window_size)
        w=${size%% *}
        case "$x$y$w" in
            '' | *[!0-9-]*)
                echo "nothing to swipe: $x $y $w"
                return 1
                ;;
        esac
        finger_swipe $((w * 70 / 100)) "$y" $((w * 15 / 100)) "$y" 300
        sleep 0.7
    }
    local problems=""
    swipe_card
    # The first touch in the first seconds of a window can be lost: once more.
    is_there "Skip the next ring" || { retry_used "s37: the first swipe moved nothing"; swipe_card; }
    screenshot s37_swiped
    is_there "Skip the next ring" || problems="$problems; did not move: $(texts_of)"
    is_there Delete || problems="$problems; delete not in view"
    tap "Skip the next ring" 0 0.6
    [ "$(db_count '"skip_until_ms":null')" = 0 ] || problems="$problems; not skipped"
    is_there "Skip the next ring" && problems="$problems; stayed aside"
    tap Undo 0 0.5
    [ "$(db_count '"skip_until_ms":null')" = 1 ] || problems="$problems; skip not undone"
    swipe_card
    tap Delete 0 0.5
    [ "$(db_count '"id":"a"')" = 0 ] || problems="$problems; not deleted"
    await_ui 4 'home_sheet=None'
    has_rows && problems="$problems; card still there"
    tap Undo 0 0.5
    [ "$(db_count '"id":"a"')" = 1 ] || problems="$problems; delete not undone"
    on_card 3 || problems="$problems; card not back"
    wait_for 3 os_has_alarm || problems="$problems; not armed after undo"
    # A swipe does not get in the way of an ordinary tap.
    tap_has "$(row_time), " L 0.6
    on_editor 3 || problems="$problems; no editor"
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
    on_home 10
    local problems="" accent
    go_tab Settings || { fail "no Settings tab: $(ui screen)"; return; }
    await_ui 5 "screen=3"
    tap_has Theme 0 0.5
    await_ui 5 "screen=5" || problems="$problems; no theme screen"
    screenshot s38_swatches
    # The second row of swatches sits below the window on a screen with a navigation bar.
    scroll_to "Choose your own colour" || problems="$problems; no swatches: $(texts_of)"
    # The swatches are pictures with no name: the third one is purple.
    press accent 2 0.5
    [ "$(sn pal_accent)" = "#b9a0ff" ] || problems="$problems; colour is $(sn pal_accent)"
    dev am force-stop "$PKG"
    start_app
    on_card 10
    [ "$(sn pal_accent)" = "#b9a0ff" ] || problems="$problems; not kept: $(sn pal_accent)"
    # Its own name.
    press open a 0.8
    on_editor
    tap_has Sound 0 0.6
    scroll_until mytune
    local names
    names=$(texts_of)
    case "$names" in
        *mytune*) ;;
        *) problems="$problems; own sound not listed: $names" ;;
    esac
    press sound-rename "$(sound_row mytune)" 0.6
    screenshot s38_rename_before
    set_field 0 Morning || problems="$problems; no field to rename in"
    screenshot s38_rename_typed
    press sound-rename-done "" 0.6
    screenshot s38_rename_done
    # The sheet is short while the keyboard goes down and has its height back when Android has given the inset back (run 45191082131,
    # Android 10: the list was read 0.6 s after Done, with the rows below Thriller not laid out yet): wait for the keyboard, then for the row.
    await_ui 10 'keyboard=down' || problems="$problems; the keyboard is still up after the rename: $(ui ready | grep -oE 'keyboard=[a-z]+')"
    wait_for 10 has_there Morning || scroll_until Morning
    names=$(texts_of)
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
    on_screen "Turn off" 10
    tap "Turn off" 0 0.5
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
    on_home 10
    press go 2
    await_ui 30 "screen=3" || { fail "no settings"; return; }
    # The row "Add widget" (row 3 of the settings), pressed through its own callback: nothing to scroll to or to touch.
    press st-row 3
    # The launcher asks the user first.
    local problems="" minute
    launcher_add_widget || { fail "the widget was not added: $SYS_WHY; the launcher shows: $(launcher_texts | tr '\n' '|'); appwidget says: $(dev dumpsys appwidget | grep -m4 'provider=' | tr -s ' ' | tr '\n' ';')"; return; }
    sys_home "the launcher in front" launcher_up || problems="$problems; $SYS_WHY"
    minute=$(printf ':%02d' "$T_M")
    sys_wait 60 "the widget showing the time $minute" launcher_has "$minute" || problems="$problems; no time on it: $(launcher_texts | tr '\n' '|')"
    # Follows a change, without the widget being asked.
    start_app
    on_home 8
    press go 0
    on_card 5
    tap_in "On or off: a" 0 1
    sys_home "the launcher in front" launcher_up || problems="$problems; $SYS_WHY"
    sys_wait 60 "the widget saying No alarm set" launcher_has "No alarm set" || problems="$problems; still old: $(launcher_texts | tr '\n' '|')"
    remove_widgets
    if [ -z "$problems" ]; then
        pass "placed, shows the next ring, follows a change"
    else
        fail "${problems#; }"
    fi
}

# The camera scanner really opens, also in a release: what only a layout refers to is the
# first thing R8 and the resource shrinker take.
s42() {
    scenario s42_scanner_opens
    fresh
    seed a "$(alarm_json a 7 0 "$EVERY_DAY" null air 0 0 3 1 '[["challenge.scan","{}"]]')"
    arm a
    on_card 10
    press open a 0.8
    on_editor
    tap_has Challenges 0 0.6
    on_screen Try 5 || { fail "no challenge list: $(texts_of)"; return; }
    tap Try 0 0.5
    wait_for 5 run_up || { fail "no challenge screen"; return; }
    adb_ logcat -c
    tap SCAN 0 0.3
    # It has the keys, or Back goes nowhere. And the window behind it stays what it was, so what
    # says it came back is the button being one again, ready for the next scan.
    scanner_on_top() { focused_window | grep -q SentinelScannerActivity; }
    scan_idle() { [ "$(sn scan_state)" = 0 ] && is_there SCAN; }
    local crashed
    if wait_for 30 scanner_on_top; then
        crashed=$(adb_ logcat -d 2> /dev/null | grep -E 'FATAL EXCEPTION|ClassNotFoundException|NoClassDefFoundError|Resources\$NotFoundException|InflateException' | head -3)
        if [ -n "$crashed" ]; then
            fail "opened, but: $crashed"
            return
        fi
        if ! sys_press KEYCODE_BACK "the scan button back after Back from the scanner" 30 scan_idle; then
            fail "did not come back from the scanner: $(focused_window)"
        else
            # Right away again: the first one must not end this one.
            tap SCAN 0 0.3
            if ! wait_for 30 scanner_on_top; then
                fail "the second time it does not open: scan_state=$(sn scan_state)"
            elif [ "$(sn scan_state)" = 0 ]; then
                fail "the second scan ended while the camera is up"
            else
                if sys_press KEYCODE_BACK "the scan button back after the second Back" 30 scan_idle; then
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


run_up() { [ "$(sn run_active)" = true ]; }
ring_or_run() { is_there "Turn off" || run_up; }
scan_or_ring() { is_there SCAN || is_there "Turn off"; }
panel_up() { [ "$(sn panel_open)" = true ]; }
panel_down() { [ "$(sn panel_open)" = false ]; }

# A period of the history and all of it each go only after asking again.
s43() {
    moved_to_cargo s43_delete_history s43_delete_history
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
    # Closed loop: the acceleration is held until the app has counted a shake (the sensor listener of a slow emulator gets few
    # samples: run 45191082131, Android 11, the first challenge took 16 s of open-loop pulses and the second got none), then the
    # rest value goes in for longer than the detector's own cooldown (300 ms). The count after each pulse is kept for the failure.
    local trail="" missed=0
    shake_moved() { [ "$(sn shakes)" != "$1" ]; }
    shake() {
        local c0 c1
        c0=$(sn shakes)
        adb_ emu sensor set acceleration 40:40:40 > /dev/null
        wait_for 3 shake_moved "$c0" || missed=$((missed + 1))
        c1=$(sn shakes)
        trail="$trail $c0>$c1"
        adb_ emu sensor set acceleration 0:0:9.8 > /dev/null
        sleep 0.45 # lint-ok: the detector's cooldown (300 ms) has to pass before the next shake counts
    }
    on_screen "Turn off" 10
    # The log since this scenario began holds the app's start. A native method the SDK registers with a wrong
    # signature is a warning there at every launch (the accelerometer one was, until SDK 0.7.1).
    local badjni
    badjni=$(adb_ logcat -d -b all 2> /dev/null | tr -d '\r' | grep -m1 'Failed to register native method com.mobilesentinel')
    adb_ logcat -c
    tap "Turn off" 0 0.5
    wait_for 5 run_up
    local rounds=0 starts deadline=$((SECONDS + 90))
    while [ "$SECONDS" -lt "$deadline" ] && state_is a firing; do
        shake
        rounds=$((rounds + 1))
    done
    await_state a scheduled 5
    starts=$(adb_ logcat -d 2> /dev/null | grep -c 'Accelerometer started')
    # Android 15 and 16 say it when the sensor is first used, which is after the first look.
    [ -n "$badjni" ] || badjni=$(adb_ logcat -d -b all 2> /dev/null | tr -d '\r' | grep -m1 'Failed to register native method com.mobilesentinel')
    if [ -n "$badjni" ]; then
        fail "the log of the app's start holds: ${badjni#*: }"
    elif [ "$(state a)" = scheduled ] && [ "$(outcomes solved)" = 1 ] && [ "$starts" -ge 2 ] && ! sound_on; then
        pass "both counted, the sensor started $starts times, in $rounds shakes (counts:$trail), no native method failed to register"
    else
        fail "state=$(state a) solved=$(outcomes solved) sensor started $starts times, $rounds shakes in 90 s ($missed not counted in 3 s; counts:$trail), on screen '$(texts_of)'"
    fi
}

# The squat challenge: shaking the phone does not count, squats do. An emulator can be told what its
# accelerometer feels but not move, so a squat of 3 s and 0.6 m is set as the vertical acceleration it
# makes, every 0.3 s or so, from the clock (the shell and adb take as long as they take). The detector
# misses an occasional one set this coarsely, so it goes on until the alarm is off.
s66() {
    if ! on_emulator; then
        note "s66 needs an emulator"
        return
    fi
    scenario s66_squats
    fresh
    next_minute
    seed a "$(alarm_json a "$T_H" "$T_M" "$EVERY_DAY" null air 0 0 3 1 '[["challenge.squat","{\"count\":2}"]]')"
    arm a
    await_ring a 30 || { fail "never started"; return; }
    shake() {
        local i=0
        while [ "$i" -lt 4 ]; do
            adb_ emu sensor set acceleration 40:40:40 > /dev/null
            sleep 0.35
            adb_ emu sensor set acceleration 0:0:9.8 > /dev/null
            sleep 0.35
            i=$((i + 1))
        done
    }
    squat() {
        local start now value
        start=$(date +%s%N)
        while :; do
            now=$(date +%s%N)
            value=$(awk -v n="$now" -v s="$start" 'BEGIN {
                t = (n - s) / 1e9
                if (t > 3.0) { print "end"; exit }
                w = 2 * 3.14159265 / 3.0
                printf "%.2f", 9.8 - 0.3 * w * w * cos(w * t)
            }')
            [ "$value" = end ] && break
            adb_ emu sensor set acceleration "0:0:$value" > /dev/null
            sleep 0.2
        done
        adb_ emu sensor set acceleration 0:0:9.8 > /dev/null
        sleep 1.2
    }
    on_screen "Turn off" 10
    adb_ logcat -c
    tap "Turn off" 0 0.5
    wait_for 5 run_up
    local problems=""
    is_there "Hold your phone steady and squat!" || problems="$problems; no squat instruction on '$(texts_of)'"
    screenshot s66_squat_screen
    shake
    sleep 1
    is_there "0 / 2" || problems="$problems; shaking counted: '$(texts_of)'"
    local rounds=0
    while [ "$rounds" -lt 8 ] && state_is a firing; do
        squat
        rounds=$((rounds + 1))
    done
    await_state a scheduled 5
    local starts
    starts=$(adb_ logcat -d 2> /dev/null | grep -c 'Accelerometer started')
    if [ "$(state a)" != scheduled ] || [ "$(outcomes solved)" != 1 ] || sound_on; then
        problems="$problems; state=$(state a) solved=$(outcomes solved) after $rounds squats, on screen '$(texts_of)'"
    fi
    if [ -z "$problems" ]; then
        pass "shaking did not count, 2 squats did (in $rounds tries), the sensor started $starts times"
    else
        fail "${problems#; }"
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
    if ! on_screen "Do not show this here" 8; then
        dev am force-stop "$PKG"
        if ! deny_overlay; then
            skip "needs a warning that can be hidden: this app without its battery exemption has one"
            return
        fi
        start_app
        on_screen "Do not show this here" 10
    fi
    local problems="" first
    first=$(sn banner)
    [ -n "$first" ] || problems="$problems; nothing to hide"
    tap "Do not show this here" 0 0.5
    [ "$(sn banner)" != "$first" ] || problems="$problems; still there after hiding"
    dev am force-stop "$PKG"
    start_app
    on_home 10
    [ "$(sn banner)" != "$first" ] || problems="$problems; back after a restart"
    printf "SELECT prefs FROM settings;\n" | sql | grep -q 'hidden_warnings":\["' || problems="$problems; not saved"
    if [ -z "$problems" ]; then
        pass "hid '$first', and it stays hidden"
    else
        fail "${problems#; }"
    fi
}

# A 12-hour clock everywhere: the list, the editor, the wheels, and what gets saved.
s46() {
    moved_to_cargo s46_twelve_hours s46_twelve_hours
}

# From an alarm's menu: new alarms start like this one, without its name.
s47() {
    moved_to_cargo s47_use_for_new_alarms s47_use_for_new_alarms
}

s48() {
    if ! on_emulator; then
        note "s48 needs an emulator"
        return
    fi
    scenario s48_files_in_and_out
    fresh
    dev rm -f /sdcard/Download/mytune.ogg
    rm_download 'alarmfree-*'
    adb_ push assets/sounds/default/air.ogg /sdcard/Download/mytune.ogg > /dev/null
    dev content call --uri content://media --method scan_volume --arg external_primary > /dev/null 2>&1
    seed a "$(alarm_json a 7 0 "$EVERY_DAY" null)"
    arm a
    on_card 10
    local problems="" names file
    # A sound.
    press open a 0.8
    on_editor
    on_screen_has Sound 30 || problems="$problems; no Sound row in the editor"
    open_row Sound
    scroll_until "Add your own sound"
    tap_scrolling "Add your own sound"
    if picker_pick mytune.ogg; then
        own_sound_there() {
            names=$(texts_of)
            case "$names" in
                *mytune*) return 0 ;;
            esac
            return 1
        }
        sound_copied() { [ -n "$(as_app "ls files/sounds/custom")" ]; }
        wait_for 30 sound_copied || problems="$problems; not copied"
        if ! wait_for 10 own_sound_there; then
            # The app can come back from the picker as a new start (Android 13: the launcher had the keys): the sound is then in
            # the list of a sheet that is opened again.
            if [ "$(screen_no)" != 1 ]; then
                on_home 10
                press open a 0.8
            fi
            on_editor
            open_row Sound
            # A sheet that is opened again starts at the top: the own sound is below the bundled ones, off the window (run 45282045399,
            # afg13: the list read Funk to Samba and the footer, no sound below it).
            has_there mytune || note "s48: the sheet that was opened again did not show the own sound before it was scrolled"
            scroll_until mytune
            wait_for 30 own_sound_there || problems="$problems; own sound not listed: $names (files/sounds/custom: $(as_app "ls files/sounds/custom 2> /dev/null" | tr '\n' ' '); imports: $(as_app "ls files/sentinel_imports 2> /dev/null" | tr '\n' ' '); picker log: $(adb_ logcat -d -s MobileSentinel.FilePicker:I 2> /dev/null | tr -d '\r' | tail -3 | cut -c1-120 | tr '\n' '|'))"
        fi
    else
        problems="$problems; the picker never showed the file: $SYS_WHY"
        sys_press KEYCODE_BACK "the app getting the keys back from the picker" "$SYS_DEADLINE" focus_in_app || note "$SYS_WHY"
    fi
    dev am force-stop "$PKG"
    start_app
    on_home 10
    # Alarms to a file: only the one alarm in it, under today's name.
    press go 2
    await_ui 30 "screen=3" || problems="$problems; the settings did not open"
    press st-row 6
    on_screen "Save to a file" 30 || problems="$problems; no Save to a file"
    tap_has "Save to a file"
    saved_file() {
        file=$(dev ls /sdcard/Download/ | grep -m1 '^alarmfree-')
        [ -n "$file" ] && [ "$(dev cat "/sdcard/Download/$file" | grep -c '"time_hour"')" -ge 1 ]
    }
    wait_for 30 saved_file || problems="$problems; no file with an alarm in Downloads (Downloads holds: $(dev ls /sdcard/Download/ | tr '\n' ' '))"
    # And back from it, as a second one.
    # The picker lists what the media index knows: the file just written is not in it yet.
    dev content call --uri content://media --method scan_volume --arg external_primary > /dev/null 2>&1
    # The toast that says where it went sits over the Back up row.
    saved_toast_gone() { ! has_there "Saved to"; }
    wait_for 30 saved_toast_gone
    await_ui 30 "screen=3" || problems="$problems; the settings are not up for the second picker"
    press st-row 6
    on_screen "Add alarms from a file" 30 || problems="$problems; no Add alarms from a file"
    tap_has "Add alarms from a file"
    if [ -n "$file" ] && picker_pick "$file"; then
        two_alarms() { [ "$(alarm_count)" = 2 ]; }
        if on_screen Undo 30; then
            two_alarms || problems="$problems; $(alarm_count) alarms after restoring"
            tap_in Undo 0 0.5
            [ "$(alarm_count)" = 1 ] || problems="$problems; undo left $(alarm_count) alarms"
        else
            # Android 13 does not always bring the app back when the picker is done (run 45388385272: the window stayed
            # stopped for 35 s): the alarms are added without it, and there was nobody to show the Undo to.
            wait_for 20 two_alarms || problems="$problems; $(alarm_count) alarms after restoring, and no Undo on screen"
            echo "NOTE  $CURRENT  no Undo to tap: the app was not in front when the picker returned; the alarms were added anyway" | tee -a "$RESULTS"
        fi
    else
        problems="$problems; the picker never showed the backup: ${file:-no file name}: $SYS_WHY"
        sys_press KEYCODE_BACK "the app getting the keys back from the picker" "$SYS_DEADLINE" focus_in_app || note "$SYS_WHY"
    fi
    dev rm -f /sdcard/Download/mytune.ogg
    [ -z "$file" ] || rm_download "$file"
    if [ -z "$problems" ]; then
        pass "own sound in, alarms out to $file and back"
    else
        fail "${problems#; }"
    fi
}

# A backup with the own sounds in it: the settings offer both files, the size is told, the sound comes back on a phone
# that has none (here: after it was deleted), and a file without the sounds says which ones are missing.
s48b() {
    if ! on_emulator; then
        note "s48b needs an emulator"
        return
    fi
    scenario s48b_backup_with_sounds
    fresh
    rm_download 'alarmfree-*'
    as_app "mkdir -p files/sounds/custom && cp files/sounds/default/air.ogg files/sounds/custom/mytune.ogg"
    seed a "$(alarm_json a 7 0 "$EVERY_DAY" null mytune.ogg)"
    start_app
    on_card 20
    local problems="" file
    save_with() { # the row to tap
        [ "$(screen_no)" = 3 ] || press go 2
        await_ui 30 "screen=3" || { problems="$problems; the settings did not open"; return 1; }
        press st-row 6
        on_screen "Save alarms only" 30 || { problems="$problems; no choice of files: $(texts_of)"; return 1; }
        has_there "MB)" || problems="$problems; the size of the larger file is not told: $(texts_of)"
        tap_has "$1"
        saved_file() { file=$(dev ls /sdcard/Download/ | grep -m1 '^alarmfree-'); [ -n "$file" ] && [ "$(dev cat "/sdcard/Download/$file" | grep -c '"file"')" -ge 1 ]; }
        wait_for 30 saved_file || { problems="$problems; no file with the sound's name in Downloads (holds: $(dev ls /sdcard/Download/ | tr '
' ' '))"; return 1; }
    }
    # A phone that has neither the alarm nor the sound.
    wipe() {
        dev am force-stop "$PKG"
        as_app "rm -f files/sounds/custom/*"
        printf 'DELETE FROM alarms;
' | sql > /dev/null
        start_app
        on_home 10
    }
    restore_from() {
        dev content call --uri content://media --method scan_volume --arg external_primary > /dev/null 2>&1
        saved_toast_gone() { ! has_there "Saved to"; }
        wait_for 30 saved_toast_gone
        [ "$(screen_no)" = 3 ] || press go 2
        await_ui 30 "screen=3" || { problems="$problems; the settings are not up for the picker"; return 1; }
        press st-row 6
        on_screen "Add alarms from a file" 30 || { problems="$problems; no Add alarms from a file"; return 1; }
        tap_has "Add alarms from a file"
        picker_pick "$file" || { problems="$problems; the picker never showed the backup: $SYS_WHY"; sys_press KEYCODE_BACK "the app getting the keys back from the picker" "$SYS_DEADLINE" focus_in_app || note "$SYS_WHY"; return 1; }
        one_alarm() { [ "$(alarm_count)" = 1 ]; }
        wait_for 30 one_alarm || { problems="$problems; $(alarm_count) alarms after restoring"; return 1; }
    }
    # With the sounds: the file holds the sound, and it is on the phone again, under the same name, with the alarm on it.
    if save_with "Save alarms with my sounds"; then
        [ "$(dev cat "/sdcard/Download/$file" | grep -c '"data"')" -ge 1 ] || problems="$problems; no sound data in the larger file"
        wipe
        [ -z "$(as_app "ls files/sounds/custom")" ] || problems="$problems; the sound was not gone before the restore"
        if restore_from; then
            [ "$(as_app "ls files/sounds/custom")" = mytune.ogg ] || problems="$problems; the sound is not back: $(as_app "ls files/sounds/custom" | tr '
' ' ')"
            [ "$(db_count mytune.ogg)" = 1 ] || problems="$problems; the alarm does not use the sound"
            as_app "cmp files/sounds/custom/mytune.ogg files/sounds/default/air.ogg" > /dev/null 2>&1 || problems="$problems; the sound is not the one that went in"
        fi
        rm_download "$file"
    fi
    # Without them: the alarm comes back on the standard sound, and the message names the missing one.
    if [ -z "$problems" ]; then
        if save_with "Save alarms only"; then
            [ "$(dev cat "/sdcard/Download/$file" | grep -c '"data"')" = 0 ] || problems="$problems; the small file holds sound data"
            wipe
            if restore_from; then
                [ -z "$(as_app "ls files/sounds/custom")" ] || problems="$problems; a sound came from a file without sounds"
                [ "$(db_count mytune.ogg)" = 0 ] || problems="$problems; the alarm still points to a sound that is not there"
                if on_screen_has "standard sound" 10; then
                    has_there mytune || problems="$problems; the message does not name the sound: $(texts_of)"
                else
                    echo "NOTE  $CURRENT  no message to read: the app was not in front when the picker returned" | tee -a "$RESULTS"
                fi
            fi
            rm_download "$file"
        fi
    fi
    if [ -z "$problems" ]; then
        pass "backup with and without own sounds, the sound back, the missing one named"
    else
        fail "${problems#; }"
    fi
}

# The part of the window the keyboard and the bars leave, in device pixels from the top.
free_px() {
    local size
    size=$(window_size)
    echo $(($(echo "$size" | cut -d' ' -f2) - $(sn insets_bottom_px | cut -d. -f1)))
}
field_xy() { ui inputs | grep -oE '"x":[0-9]+,"y":[0-9]+' | head -1 | sed -E 's/"x":([0-9]+),"y":([0-9]+)/\1 \2/'; }
text_y() { where "$1" | cut -d' ' -f2; }
# The keyboard stays off what is being typed in, in a panel and in the typing challenge, and
# what is typed arrives.
s49() {
    scenario s49_keyboard
    fresh
    seed a "$(alarm_json a 7 0 "$EVERY_DAY" null air 0 0 3 1 '[["challenge.typing","{\"difficulty\":\"easy\"}"]]')"
    arm a
    on_card 10
    local problems="" x y before free typed
    # The room the window has is measured with the keyboard down (Android can bring one back that the baseline did not see).
    hide_keyboard
    before=$(free_px)
    # The window is resized once more a moment after it came up (seen on Android 15 at 2208x1840: a smaller reading at
    # first): the room it has is what two readings a second apart agree on.
    steady() { local a; a=$(free_px); sleep 1; [ "$(free_px)" = "$a" ] && before=$a; }
    wait_for 10 steady
    press open a 0.8
    on_editor
    tap_has Label 0 0.6 || { fail "no Label row on the editor, the app shows: $(ui ready | grep -oE 'keyboard=[a-z]+ win=[0-9x]+')"; hide_keyboard; return; }
    await_ui 3 'editor_sheet!=None' || problems="$problems; no label panel"
    read -r x y <<< "$(field_xy)"
    finger_xy "$x" "$y"
    made_room() {
        free=$(free_px)
        [ "$free" -lt $((before - 100)) ] 2> /dev/null
    }
    wait_for 6 made_room || problems="$problems; the window did not make room: $before, then $free"
    screenshot s49_label
    read -r x y <<< "$(field_xy)"
    [ "${y:-9999}" -le "$free" ] && [ "$(text_y Done)" -le "$free" ] 2> /dev/null ||
        problems="$problems; the field or Done is under the keyboard ($y, $(text_y Done), free $free)"
    type_real 0 Morning || problems="$problems; typed text did not arrive: $(ui inputs)"
    press_back
    press_back
    room_back() {
        free=$(free_px)
        [ "$free" = "$before" ]
    }
    wait_for 5 room_back || problems="$problems; no room given back: $before, now $free"
    # The challenge. It has to show the passage and the box together. (The first Back closed the
    # keyboard, the second the panel, a third asks about the change.)
    press_back
    await_ui 4 'editor_sheet!=None'
    tap Discard 0 0.5
    on_home 4 || problems="$problems; not back on Home"
    tap "More actions" 0 0.5
    on_screen "Try its challenges" 4
    tap "Try its challenges" 0 0.8
    wait_for 5 run_up || problems="$problems; no typing challenge"
    read -r x y <<< "$(field_xy)"
    finger_xy "$x" "$y"
    wait_for 6 made_room
    screenshot s49_typing
    read -r x y <<< "$(field_xy)"
    [ "${y:-9999}" -le "$free" ] || problems="$problems; the box is under the keyboard ($y, free $free)"
    # The keyboard shows the first word of it arriving (a phone's own typing drops what comes after
    # a space on this emulator), the rest goes in as keys of the window.
    type_real 0 "$(sn type_passage | cut -d' ' -f1)" || problems="$problems; the first word did not arrive: $(ui inputs)"
    type_field 0 "$(sn type_passage)"
    finished() { [ "$(ui inputs)" = "[]" ]; }
    wait_for 10 finished || problems="$problems; typing it all did not finish: '$(ui inputs)'"
    # Not left up for the scenario after this one.
    hide_keyboard
    if [ -z "$problems" ]; then
        pass "room made and given back, text arrives, challenge done by typing"
    else
        fail "${problems#; }"
    fi
}

# One tap on Done, and the sheet has to be gone: a second tap on Done before it is would land on the editor under it. The
# tries it used to have were never used in the full runs 45105395298 and 45114508112 (retries.txt, all seven versions).
done_sheet() {
    # A panel takes touches once it holds still (the first Done after a sheet within it is lost on a busy machine).
    await_settled 4
    tap_in Done L 0.3
    await_ui 8 'editor_sheet=None' && return 0
    screenshot done_sheet_still_up
    step_failed "done_sheet: the sheet is still up after Done: $(sn ed_sheet)"
}
# Open a row of the editor, and wait for its panel.
# A touch that lands while the sheet before it is still closing is dropped, and everything after it
# then goes to the editor instead of the sheet: look whether the sheet came up, and touch again if not.
open_row() {
    # Where the row is, if the screen was left scrolled either way (s50 once found no "Label" on Android 12).
    # A keyboard left up by something before (the one of the last typing, which Android brings back) covers the rows.
    has_there "$1" || { hide_keyboard; sheet_scroll_up; has_there "$1" || sheet_scroll; }
    scroll_clear "$1"
    tap_has "$1" 0 0.4
    await_ui 4 'editor_sheet!=None' && return 0
    step_failed "open_row $1: no panel came up after the tap: $(sn ed_sheet)"
}
# The number of an own sound's row in the sound sheet: the bundled ones first, then the own ones by
# name. Counted from the files, because a scrolled list shows only part of its rows.
sound_row() { # name (without .ogg)
    local own
    own=$(as_app "ls files/sounds/custom" | sort | grep -n -x "$1.ogg" | cut -d: -f1)
    echo $(($(as_app "ls files/sounds/default" | sed 's/\.[^.]*$//' | sort -u | wc -l) + own - 1))
}
# A list longer than the window (a phone's sheet): scroll until this text is on screen.
scroll_until() { # text
    local i
    for i in 1 2 3 4; do
        wait_for 2 has_there "$1" && return 0
        sheet_scroll
    done
    return 1
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
    on_home 20
    local problems="" row rows
    open_new_editor || { fail "no editor: $(texts_of)"; return; }
    # Either side of the time: -1, -5, then +1, +5.
    tap "+5" 0 0.2
    tap "+5" 0 0.2
    tap Weekend 0 0.3
    open_row Label
    type_field 0 Morning || problems="$problems; label field: $(field_value 0)"
    type_field 1 'Take pills' || problems="$problems; note field: $(field_value 1)"
    # On a slow emulator the keyboard comes up late and stays over the lower half of everything after this
    # (the whole editor then failed at once, 156 s, with the keyboard in the picture).
    on_emulator && hide_keyboard
    done_sheet
    open_row Sound
    tap_in Funk
    sheet_scroll
    tap_in "2 min"
    set_slider 0 0.75
    done_sheet
    open_row Vibration
    tap_in Steady
    tap_in "15 s"
    done_sheet
    open_row Snooze
    tap_scrolling "More How often"
    tap_scrolling "More How long"
    done_sheet
    open_row Challenges
    # Math and Steps, each through Add and its own sheet.
    for option in Math Steps; do
        tap "Add a challenge" 0 0.4
        on_screen "$option" 4 || problems="$problems; no way to add $option"
        tap_in "$option" L 0.4
        # A new challenge can come with a sheet of its own for its settings.
        # Math has its settings sheet (Difficulty), Steps has one too (How many steps): the Done of the Challenges sheet would
        # otherwise be the first tap on Done that only closes that one (seen as a retry on every Android version).
        { is_there Difficulty || is_there "How many steps"; } && done_sheet_inner
    done
    done_sheet
    sheet_scroll
    open_row 'Ring limit'
    tap_scrolling "60 min" || note "no 60 min: sheet=$(sn ed_sheet) screen=$(screen_no) $(texts_of | tail -c 400)"
    done_sheet
    open_row 'Are you up?'
    tap_scrolling "3 min"
    done_sheet
    open_row 'Reminder'
    tap_scrolling "10 min"
    done_sheet
    has_there "Full brightness" || sheet_scroll
    scroll_clear "Full brightness"
    tap "Full brightness" 0 0.3
    screenshot s50_set
    tap Save 0 0.5
    [ "$(sn toast_visible)" = true ] || problems="$problems; Save said nothing"
    on_home 4 || problems="$problems; Save did not go back"
    row=$(printf 'SELECT data FROM alarms;\n' | sql)
    for field in '"time_hour":7' '"time_minute":10' '"label":"Morning"' '"note":"Take pills"' \
        '"days":[false,false,false,false,false,true,true]' '"sound":"funk' '"fade_in_secs":120' \
        '"volume_percent":75' '"vibration":1' '"sound_delay_secs":15' '"snooze_interval":6' \
        '"snooze_count":4' 'challenge.math' 'challenge.steps' '"reminder":{"on":true,"minutes":10' \
        '"ring_limit_secs":3600' '"wake_check_minutes":3' '"max_brightness":true'; do
        has "$row" "$field"
    done
    local id
    id=$(printf 'SELECT id FROM alarms;\n' | sql)
    await_state "$id" scheduled 5 || problems="$problems; not armed: $(state "$id")"
    # And what it says when opened again.
    press open "$id" 0.8
    on_editor
    rows=$(texts_of)
    # In steps, as a longer swipe can carry the rows in between out of the window unseen.
    local k
    for k in 1 2 3 4; do
        sheet_nudge
        rows="$rows|$(texts_of)"
    done
    for shown in Morning Funk Steady '6 min, 4' Math '60 min' 'After 3 min' '10 min'; do
        has "$rows" "$shown"
    done
    has "$(sn ed_time_text)" ':10'
    # Off the list from here too, after asking.
    sheet_scroll_up
    tap_trash
    await_ui 3 'editor_sheet!=None' || problems="$problems; delete does not ask"
    tap Delete 0 0.5
    [ "$(alarm_count)" = 0 ] || problems="$problems; not deleted"
    on_home 4 || problems="$problems; delete did not go back"
    if [ -z "$problems" ]; then
        pass "every setting saved, shown again, armed, and deleted"
    else
        fail "${problems#; }"
    fi
}
done_sheet_inner() { tap Done 0 0.3; }

# Every setting through its own screen, and all of it still there after a restart.
s51() {
    scenario s51_settings_stick
    fresh
    seed a "$(alarm_json a 14 5 "$EVERY_DAY" null)"
    arm a
    on_home 10
    local problems="" row columns
    go_tab Settings || { fail "no Settings tab: $(ui screen)"; return; }
    await_ui 5 "screen=3"
    pick() { # row, then the choice of what it opens
        tap_has "$1" 0 0.5 || note "no row $1 on: $(texts_of | cut -c1-300)"
        await_ui 3 'panel=true'
        tap_in "$2" "${3:-}" 0.3
        # A choice closes its panel; one that does not is closed by Back.
        await_ui 2 'panel=false' || press_back
        await_ui 3 'panel=false'
    }
    tap_has Theme 0 0.5
    await_ui 5 "screen=5"
    tap Light 0 0.4
    press_back
    await_ui 5 "screen=3"
    pick 'Time format' '12 hours'
    pick 'Week starts on' Sunday
    tap_has 'Minimum volume' 0 0.5
    await_ui 3 'panel=true'
    tap_in "75%" 0 0.3
    tap Done 0 0.3
    await_ui 3 'panel=false'
    # The last rows sit under the bottom bar until the screen is scrolled.
    centre_row "Touch feedback"
    tap "Touch feedback" 0 0.3
    centre_row "Less motion"
    tap "Less motion" 0 0.3
    centre_row Confetti
    tap Confetti 0 0.3
    centre_row Language has
    pick Language Nederlands
    sleep 0.2
    dev am force-stop "$PKG"
    start_app
    on_card 10
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
    [ "$(sn pal_light)" = true ] || problems="$problems; not light"
    [ "$(sn less_motion)" = true ] || problems="$problems; still moving"
    has "$(row_time)" '2:05 PM'
    has "$(texts_of)" Wekkers
    # A week that starts on Sunday: zondag first, in the day buttons of the editor.
    go_tab Wekkers || { fail "no Wekkers tab: $(ui screen)"; return; }
    press open a 0.8
    on_editor
    has "$(texts_of)" 'Zondag|Maandag|Dinsdag|Woensdag|Donderdag|Vrijdag|Zaterdag'
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
    on_card 10
    local problems="" row
    menu_item() { # what to pick from the first alarm's menu
        tap "More actions" 0 0.5
        on_screen "$1" 4
        tap "$1" 0 "${2:-0.5}"
    }
    data_of_a() { printf "SELECT data FROM alarms WHERE id='a';\n" | sql; }
    [ "$(row_labels)" = "a b" ] || problems="$problems; own order: $(row_labels)"
    tap "Sort alarms" 0 0.5
    tap "By time" 0 0.4
    [ "$(row_labels)" = "b a" ] || problems="$problems; by time: $(row_labels)"
    tap "Sort alarms" 0 0.5
    tap "Your own order" 0 0.4
    [ "$(row_labels)" = "a b" ] || problems="$problems; back to own: $(row_labels)"
    # Pause one until a date, and end it.
    menu_item 'Pause until a date' 0.4
    local day
    day=$(date -u -d "$later" +%-d)
    [ "$(date -u -d "$later" +%-m)" = "$(date +%-m)" ] || press cal-next "" 0.3
    tap "$day" L 0.4
    tap Pause L 0.5
    sleep 0.3
    row=$(data_of_a)
    case "$row" in
        *'"pause_until_ms":null'*) problems="$problems; not paused" ;;
    esac
    has "$(row_when)" Paused
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
    menu_item Duplicate 0.6
    on_editor
    has "$(texts_of)" 'a (copy)'
    tap Save 0 0.6
    [ "$(alarm_count)" = 3 ] || problems="$problems; $(alarm_count) alarms after duplicating"
    on_home 4
    # The switch.
    tap "On or off: a" 0 0.5
    [ "$(db_enabled a)" = false ] || problems="$problems; the switch left it $(db_enabled a)"
    [ "$(state a)" != scheduled ] || problems="$problems; still armed when off"
    tap "On or off: a" 0 0.5
    [ "$(db_enabled a)" = true ] && await_state a scheduled 3 || problems="$problems; not back on: $(db_enabled a) $(state a)"
    # One row of the history.
    go_tab History || { fail "no History tab: $(ui screen)"; return; }
    await_ui 5 "screen=2"
    local before xy
    before=$(history_count)
    # The first row of the list (its text ends with the alarm's label) opens its details.
    xy=$(ui texts | python -I -c '
import json, sys
rows = [e for e in json.loads(sys.stdin.read()) if e["r"] == "Text" and e["t"].endswith(" a") and e["y"] > 900]
print("%d %d" % (rows[0]["x"], rows[0]["y"]) if rows else "")
')
    # shellcheck disable=SC2086
    [ -z "$xy" ] || tap_xy $xy
    sleep 0.5
    tap "Delete this one" 0 0.5
    await_ui 3 'panel=true' || problems="$problems; deleting one row does not ask"
    tap Delete 0 0.5
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
    seed a "$(alarm_json a 7 0 "$EVERY_DAY" null)"
    start_app
    on_card 20
    local problems="" row
    # Templates, the typed time, the wheels, one date and a challenge's settings are checked in
    # cargo test (ui::flow_tests::s53_the_rest_of_the_screens). On the phone: a sound of one's
    # own is a file that goes, and the reliability screen's buttons leave for the phone's own
    # screens and come back.
    press open a
    on_editor
    open_row Sound
    scroll_until mytune
    has "$(texts_of)" mytune
    press sound-delete "$(sound_row mytune)"
    tap Delete L
    case "$(texts_of)" in
        *mytune*) problems="$problems; sound still listed" ;;
    esac
    [ -z "$(as_app "ls files/sounds/custom")" ] || problems="$problems; file still there"
    done_sheet
    # Without changes to the alarm, Back leaves at once; with them it asks first.
    press_back
    if [ "$(screen_no)" = 1 ]; then
        await_ui 4 'editor_sheet!=None'
        tap Discard
    fi
    on_home 5
    open_reliability || problems="$problems; no reliability screen"
    local i=0 n
    n=$(all_texts | grep -c -E '^Fix .')
    # Another app has the keys. A screen of Android can take a while to start: sys_leave waits until its tree is drawn,
    # presses Back once and waits for the keys to come back (a second Back only when a read after 15 s still shows it). A
    # second Back in the middle of the change would close the screen the app comes back to.
    away() { focused_window | grep -v null | grep -qv "$PKG/"; }
    while [ "$i" -lt "${n:-0}" ] 2> /dev/null; do
        tap_has "Fix " "$i"
        sys_wait "$SYS_DEADLINE" "fix button $i leaving the app" away || problems="$problems; fix button $i stays in the app: $SYS_WHY"
        sys_leave "the screen of fix button $i" || problems="$problems; Back from fix button $i: $SYS_WHY"
        await_ui 30 "screen=4" || problems="$problems; not back from fix button $i"
        i=$((i + 1))
    done
    scroll_to "This app in the phone's settings" || problems="$problems; no button for the app's settings"
    tap_in "This app in the phone's settings"
    sys_wait "$SYS_DEADLINE" "the app's settings leaving the app" away || problems="$problems; the app's settings stay in the app: $SYS_WHY"
    sys_leave "the app's settings" || problems="$problems; Back from the app's settings: $SYS_WHY"
    if [ -z "$problems" ]; then
        pass "deleting a sound, $n fix buttons"
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
    sys_wait "$SYS_DEADLINE" "the PIN being set" pin_is_set || { fail "$SYS_WHY"; return; }
    next_minute 10
    seed a "$(alarm_json a "$T_H" "$T_M" "$EVERY_DAY" null air 0 0 3 1 '[["challenge.scan","{}"]]')"
    arm a
    dead
    sys_sleep || note "$SYS_WHY"
    locked() { dev "dumpsys trust | grep -q 'deviceLocked=1'" > /dev/null; }
    local problems=""
    sys_wait "$SYS_DEADLINE" "the phone being locked before the ring" locked || problems="$problems; not locked before the ring: $SYS_WHY"
    if ! await_ring a 30; then
        problems="$problems; no ring: state=$(state a)"
    else
        sys_wait "$SYS_DEADLINE" "its screen being on top" focus_in_app || problems="$problems; $SYS_WHY"
        on_screen "Turn off" 30 || problems="$problems; no alarm screen"
        screenshot s54_over_the_lock_screen
        locked || problems="$problems; ringing unlocked the phone"
        # Over the lock screen the touch is not the subject (the keys are, below): the challenge comes through the button's
        # own callback, once, and the screen says so.
        press ring-turn-off
        on_screen SCAN 30 || problems="$problems; no scan button"
        # The power button, by accident: what comes back is the challenge, not the PIN.
        # The SDK can wake the screen again within a moment (Android 13): then the screen was never seen asleep, and what has to hold
        # is the same, the challenge and not the PIN.
        sys_sleep 8 || note "the screen was not seen asleep: $SYS_WHY"
        sys_wake || problems="$problems; $SYS_WHY"
        challenge_is_up() { focus_in_app && is_there SCAN; }
        sys_wait "$SYS_DEADLINE" "the challenge after the power button" challenge_is_up || problems="$problems; no challenge after the power button: $SYS_WHY"
        tap SCAN
        camera_has_keys() { focused_window | grep -q SentinelScannerActivity; }
        if sys_wait "$SYS_DEADLINE" "the camera screen having the keys" camera_has_keys; then
            sys_holds 2 "the camera keeping the keys" camera_has_keys || problems="$problems; the camera lost the keys: $SYS_WHY"
        else
            problems="$problems; no camera over the lock screen: $SYS_WHY"
        fi
        screenshot s54_camera_over_the_lock_screen
        locked || problems="$problems; the camera unlocked the phone"
        as_app "echo a-code > /data/user/0/$PKG/files/sentinel_scan_result.txt"
        sys_wait "$SYS_DEADLINE" "the ring being quiet after the scan" sound_off || problems="$problems; still ringing after the scan"
        await_state a scheduled 30 || problems="$problems; state=$(state a) after the scan"
        camera_gone() { ! camera_has_keys; }
        if ! sys_wait 10 "the camera screen closing after the scan" camera_gone; then
            sys_press KEYCODE_BACK "the camera screen leaving after Back" "$SYS_DEADLINE" camera_gone || problems="$problems; $SYS_WHY"
        fi
        sys_holds 2 "the phone staying locked after the alarm ended" locked || problems="$problems; ending the alarm unlocked the phone"
    fi
    no_pin
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
    on_screen "Turn off" 10
    local problems="" next
    next=$(next_fire_ms a)
    # Back on top, still on, nothing solved. Android 12 removes the task it hid from recents, which
    # brings the alarm back as a new screen: that counts too.
    back_on_it() { focus_in_app && { is_there "Turn off" || [ "$(sn run_active)" = true ]; }; }
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
        wait_for 15 ring_or_run
        # Through the window's events: a finger's tap on a window that has just come up (after the kills here) can be
        # lost, and this step is only the way into the challenge.
        is_there "Turn off" && tap_in "Turn off" 0 0.3
        wait_for 10 run_up || problems="$problems; no challenge after $1"
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
# screen would be restarted. It is quiet, but not for ever: that would be a way out. Here the
# camera closes itself after the challenge's limit (shortened to 8 s), the challenge is quiet again,
# and with no new tap on Scan the alarm rings again. Home does not get away from it, and what it
# scans turns the alarm off.
s55() {
    scenario s55_scan_during_a_ring
    fresh
    # Shorter than the camera stays up, and longer than a cold start takes: Android 12 takes the
    # task away on Home and the app comes back from nothing, which on a busy emulator is 6 s.
    shorten ui_timeout_ms 10000
    shorten scan_camera_secs 8
    shorten idle_secs 8
    next_minute
    seed a "$(alarm_json a "$T_H" "$T_M" "$EVERY_DAY" null air 0 0 3 1 '[["challenge.scan","{}"]]')"
    arm a
    await_ring a 30 || { fail "never started"; return; }
    on_screen "Turn off" 10
    tap "Turn off" 0 0.3
    if ! on_screen SCAN 10; then
        fail "no scan button: $(texts_of)"
        return
    fi
    local p0 problems=""
    p0=$(main_pid)
    scanner_on_top() { focused_window | grep -q SentinelScannerActivity; }
    tap SCAN 0 0.3
    if ! wait_for 8 scanner_on_top; then
        fail "the camera does not open: $(focused_window)"
        return
    fi
    wait_for 8 sound_off || problems="$problems; loud with the camera just opened"
    sleep 1
    scanner_on_top || problems="$problems; pushed off the screen: $(focused_window)"
    [ "$(main_pid)" = "$p0" ] || problems="$problems; the app was restarted under the camera"
    scanner_gone() { ! scanner_on_top && [ "$(sn scan_state)" = 0 ]; }
    wait_for 16 scanner_gone || problems="$problems; the camera did not close itself after its limit"
    wait_for 8 sound_off || problems="$problems; loud with the camera closed"
    # No new tap on Scan: it is no way out, and rings again.
    wait_for 16 sound_on || problems="$problems; silent for good after the camera, a way out"
    wait_for 10 scan_or_ring
    is_there "Turn off" && tap "Turn off" 0 0.3
    on_screen SCAN 8 || problems="$problems; no scan button again"
    tap SCAN 0 0.3
    wait_for 8 scanner_on_top || problems="$problems; the camera does not open again"
    # Past the moment the lock stands aside for an activity of its own to come up.
    sleep 2.5
    go_home
    wait_for 8 focus_in_app || problems="$problems; Home got away from the camera: $(focused_window)"
    # Android 12 removes the task it hid from recents, so the alarm comes back as a new screen.
    wait_for 10 scan_or_ring
    is_there "Turn off" && tap "Turn off" 0 0.3
    on_screen SCAN 8 || problems="$problems; no scan button after Home"
    # A try, like the taps in the loop below: SCAN can be gone again when the challenge screen went back to the ring (its idle
    # time) between the look and the tap (run 45145328118, Android 10), and the loop says whether the camera opened.
    TAP_MAY_FAIL=1 tap SCAN 0 0.3
    # The process lives through Home on Android 14 and later: a tap while the camera that Home closed is
    # still reporting back is ignored (it is still scanning for that long). A person taps again.
    # The challenge screen goes back to the ring after its idle time (8 s here): on a busy emulator that has happened
    # by now, and a person would press Turn off and Scan again.
    # (The ring screen's Turn off is listed behind the challenge screen too: it is pressed only without a Scan on screen.)
    # The tap on SCAN waits for it in the app (8 s) and is a try: the loop below says whether the camera opened. SCAN can be gone
    # for good when the challenge screen went back to the ring (its idle time) since the last look, and then the next round
    # starts from the ring's Turn off.
    again() { is_there SCAN || { is_there "Turn off" && tap "Turn off" 0 0.3; }; TAP_MAY_FAIL=1 TAP_WAIT=8 tap SCAN 0 0.3; }
    # Android 10 to 13 keep the camera that Home paused until its limit: a person taps a few times.
    for _ in 1 2 3; do
        wait_for 3 scanner_on_top && break
        sleep 1
        again
    done
    wait_for 8 scanner_on_top || {
        problems="$problems; the camera does not open after Home"
        note "after Home: focus=$(focused_window | cut -c1-90) scan_state=$(sn scan_state) screen=$(screen_no) texts=$(texts_of | head -c 160)"
        screenshot s55_after_home
    }
    # What the scanner does with a code, without a code to show an emulator's camera.
    as_app "echo a-code > /data/user/0/$PKG/files/sentinel_scan_result.txt; echo a-code > $DATA/files/sentinel_scan_result.txt"
    ended() { state_is a scheduled && sound_off; }
    wait_for 8 ended || problems="$problems; the code did not turn it off: state=$(state a) sound=$(sound_on && echo on || echo off)"
    wait_for 5 scanner_on_top && press_back
    if [ -z "$problems" ]; then
        pass "quiet under the camera, it closes itself and the alarm rings again, no way out by Home, and a code turns it off"
    else
        fail "${problems#; }"
    fi
}

# What depends on the size. Another screen on this Android is all the app can tell of a tablet
# or an open foldable, and it is what a new AVD would cost a minute to start for.
# Each scenario at a size is a unit of its own (baseline before, watchdog after, the one rerun): a restart of the device is
# then INFRA for the one scenario it happened in, not for every result line of the whole group.
with_screen() { # size density scenarios...
    local s
    SCREEN_SIZE=$1 SCREEN_DENSITY=$2
    shift 2
    for s in "$@"; do
        SCREEN_SCENARIO=$s
        ensure_booted "$s@$SCREEN_SIZE"
        run_unit "$s@$SCREEN_SIZE" at_screen_size
    done
}
at_screen_size() {
    dev wm size "$SCREEN_SIZE"
    dev wm density "$SCREEN_DENSITY"
    VARIANT="@$SCREEN_SIZE"
    "$SCREEN_SCENARIO"
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
    # Android 10 and 11 call the service `vibrator` and put it on one line: mCurrentVibration=null.
    dev "{ dumpsys vibrator_manager | grep -A1 'CurrentVibration:' | grep -v 'CurrentVibration:' | grep -vE '^ *null *\$'; dumpsys vibrator | grep 'mCurrentVibration=' | grep -v 'mCurrentVibration=null'; } 2> /dev/null | grep -q ."
}
not_vibrating() { ! vibrating_now; }
# What the try button of the open panel says: true while it runs (it says Stop), false (Start), or none.
start_stop_says() {
    if is_there 'Stop preview'; then echo true; elif is_there 'Play preview'; then echo false; else echo none; fi
}
# The button that is there now: Stop while it plays, Play when it does not.
start_stop() {
    if is_there 'Stop preview'; then tap 'Stop preview' 0 0.3; else tap 'Play preview' 0 0.3; fi
}

# Trying a sound and a vibration: a tap picks a sound and plays it for a few seconds, Start plays it
# for longer, and Stop, closing and time all end it. A tap on a pattern vibrates, for just as long.
s58() {
    scenario s58_trying_sound_and_vibration
    fresh
    # A tap plays 2 s and a try 4 s here, not 5 s and 15 s: the app takes the numbers from the
    # files (`preview_ms`, `sound_tap_ms`), and the real ones are checked by a unit test.
    shorten sound_tap_ms 2000
    shorten preview_ms 4000
    seed a "$(alarm_json a 7 0 "$EVERY_DAY" null)"
    arm a
    on_card 10
    local problems="" started took
    press open a
    on_editor
    open_row Sound
    tap_in Funk
    wait_for 4 sound_on || problems="$problems; a tap on a sound is silent"
    [ "$(start_stop_says)" = true ] || problems="$problems; a tap plays but does not say Stop: $(start_stop_says)"
    screenshot s58_playing
    # While it plays, another sound takes over, and the same one starts over.
    tap_in Happy
    sound_on || problems="$problems; another sound stopped it"
    [ "$(start_stop_says)" = true ] || problems="$problems; another sound ended it"
    tap_in Happy
    sound_on || problems="$problems; the same sound again stopped it"
    start_stop
    wait_for 4 sound_off || problems="$problems; Stop does not stop it"
    [ "$(start_stop_says)" = false ] || problems="$problems; says it runs after Stop: $(start_stop_says)"
    # A tap plays a moment and ends by itself.
    tap_in Happy
    started=$SECONDS
    wait_for 4 sound_on || problems="$problems; a tap after Stop is silent"
    wait_for 8 sound_off || problems="$problems; a tap does not end by itself"
    took=$((SECONDS - started))
    [ "$took" -ge 1 ] && [ "$took" -le 5 ] || problems="$problems; a tap played ${took}s"
    # Start plays it as well.
    start_stop
    wait_for 4 sound_on || problems="$problems; Start is silent"
    [ "$(start_stop_says)" = true ] || problems="$problems; Start does not turn to Stop"
    start_stop
    wait_for 4 sound_off
    # By itself after a while, also with a sound shorter than that.
    start_stop
    wait_for 4 sound_on
    started=$SECONDS
    wait_for 10 sound_off || problems="$problems; does not end by itself"
    took=$((SECONDS - started))
    [ "$took" -ge 3 ] && [ "$took" -le 8 ] || problems="$problems; lasted ${took}s"
    await_ui 3 'ed_preview=false' || problems="$problems; still says Stop after it ended"
    [ "$(start_stop_says)" = false ] || problems="$problems; still says Stop after it ended"
    # Closing the panel ends it, by Done and by Back.
    start_stop
    wait_for 4 sound_on
    done_sheet
    wait_for 4 sound_off || problems="$problems; Done does not stop it"
    open_row Sound
    start_stop
    wait_for 4 sound_on
    press_back
    await_ui 3 'editor_sheet=None'
    wait_for 4 sound_off || problems="$problems; Back does not stop it"

    open_row Vibration
    tap_in Heartbeat
    vibrating_now || problems="$problems; a tap on a pattern does not vibrate"
    [ "$(start_stop_says)" = true ] || problems="$problems; no way to stop the vibration"
    screenshot s58_vibrating
    started=$SECONDS
    sleep 2
    vibrating_now || problems="$problems; stopped after one beat"
    wait_for 8 not_vibrating || problems="$problems; vibration does not end by itself"
    took=$((SECONDS - started))
    [ "$took" -ge 3 ] && [ "$took" -le 8 ] || problems="$problems; vibrated ${took}s"
    tap_in Pulse
    vibrating_now || problems="$problems; the next pattern does not vibrate"
    start_stop
    wait_for 3 not_vibrating || problems="$problems; Stop does not stop the vibration"
    tap_in Steady
    tap_in Off
    wait_for 3 not_vibrating || problems="$problems; Off does not stop the vibration"
    [ "$(start_stop_says)" = none ] || problems="$problems; a button to try nothing"
    tap_in Pulse
    done_sheet
    wait_for 3 not_vibrating || problems="$problems; closing does not stop the vibration"
    # Turning the touch feedback off has nothing to do with this.
    press_back
    await_ui 4 'editor_sheet!=None'
    tap Discard
    on_home 5
    go_tab Settings || { fail "no Settings tab: $(ui screen)"; return; }
    centre_row "Touch feedback"
    tap "Touch feedback"
    go_tab Alarms || { fail "no Alarms tab: $(ui screen)"; return; }
    on_card 5
    press open a
    on_editor
    open_row Vibration
    tap_in Steady
    vibrating_now || problems="$problems; touch feedback off silences the preview"
    done_sheet
    if [ -z "$problems" ]; then
        pass "a tap plays, Start and Stop work, it ends by itself, closing ends it, vibration too (${took}s)"
    else
        fail "${problems#; }"
    fi
}

# An alarm going off during a preview: its sound and vibration are its own, and stay.
s59() {
    scenario s59_alarm_during_a_preview
    fresh
    # The ring comes while a try of 6 s is still playing: the clock moves to just before it.
    shorten preview_ms 6000
    next_minute 320
    seed a "$(alarm_json a "$T_H" "$T_M" "$EVERY_DAY" null air 0 0 3 1)"
    arm a
    on_card 10
    local problems=""
    press open a
    on_editor
    open_row Sound
    start_stop
    wait_for 4 sound_on || problems="$problems; no preview"
    done_sheet
    open_row Vibration
    tap_in Pulse
    jump_to $((T - 3))
    if await_ring a 40 && on_screen "Turn off" 15; then
        # Past when the preview stops itself: that must not take the ring's sound with it.
        await_ui 12 'ed_preview=false' || problems="$problems; the preview never ended"
        sleep 1
        sound_on || problems="$problems; the ring went silent"
        vibrating_now || problems="$problems; the ring stopped vibrating"
        state_is a firing || problems="$problems; state $(state a)"
        tap "Turn off"
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
    sys_home "the launcher in front" launcher_up || problems="$problems; $SYS_WHY"
    jump_to $((T - 303))
    wait_for 180 reminded || { fail "no notice with a Skip"; return; }
    kill_all
    wait_for 30 no_process
    tap_in_shade Skip skipped || problems="$problems; no Skip to tap: $SYS_WHY"
    shade_set closed || problems="$problems; $SYS_WHY"
    skipped() { printf "SELECT data FROM alarms WHERE id='a';\n" | sql | grep -q "\"skip_until_ms\":$((T * 1000))"; }
    wait_for 15 skipped || problems="$problems; not skipped: $(printf "SELECT data FROM alarms WHERE id='a';\n" | sql | grep -o '"skip_until_ms":[^,]*')"
    moved() { [ "$(armed_at_ms a)" = "$((armed + 86400000))" ]; }
    wait_for 8 moved || problems="$problems; armed for $(armed_at_ms a), was $armed"
    # Taken back.
    wait_for 8 done_notice || problems="$problems; nothing to take it back with"
    tap_in_shade Undo not_skipped || problems="$problems; no Undo to tap: $SYS_WHY"
    shade_set closed || problems="$problems; $SYS_WHY"
    not_skipped() { printf "SELECT data FROM alarms WHERE id='a';\n" | sql | grep -q '"skip_until_ms":null'; }
    wait_for 15 not_skipped || problems="$problems; undo did nothing"
    back_armed() { [ "$(armed_at_ms a)" = "$armed" ]; }
    wait_for 8 back_armed || problems="$problems; undo left it armed for $(armed_at_ms a)"
    wait_for 8 reminded || problems="$problems; the notice does not come back after undo"
    # Skipped for good this time, from a list that is open: the list says so.
    start_app
    on_card 10
    tap_in_shade Skip skipped || problems="$problems; no second Skip to tap: $SYS_WHY"
    shade_set closed || problems="$problems; $SYS_WHY"
    wait_for 15 skipped || problems="$problems; not skipped the second time"
    says_it() { has_text=$(row_when); case "$has_text" in Skips*) return 0 ;; esac; return 1; }
    wait_for 8 says_it || problems="$problems; the list says '$has_text'"
    # No ring at its time.
    dev am force-stop "$PKG"
    jump_to $((T - 3))
    start_app
    on_card 10
    # Past its time: the clock has to get there.
    until_past $((T + 5))
    sound_on && problems="$problems; rang although skipped"
    state_is a firing && problems="$problems; firing although skipped"
    # And the next one does ring.
    T=$((T + 86400))
    jump_to $((T - 4))
    if ! await_ring a 40; then
        problems="$problems; the ring after the skipped one did not come: state=$(state a)"
    fi
    if [ -z "$problems" ]; then
        pass "skipped with the app dead, taken back, skipped again, that ring silent, the next one rings"
    else
        fail "${problems#; }"
    fi
}

# The reminder before an alarm with the user's own time and text: 8 hours before, silent, and 15 minutes
# before, with the notification sound. Each comes when the clock reaches its moment (not before), says the
# text, sits on its own channel (the quiet one, the one with sound), and Skip still skips the ring.
s67() {
    if ! on_emulator; then
        note "s67 needs an emulator's clock"
        return
    fi
    scenario s67_reminder_with_own_time_and_text
    local problems="" quiet=sentinel_reminders_popup loud=sentinel_reminders_sound
    # An 8 hour reminder, silent.
    fresh
    next_minute 40000
    ALARM_EXTRA=',"reminder":{"on":true,"minutes":480,"text":"Go to bed, your alarm goes off in 8 hours","sound":false}'
    seed a "$(alarm_json a "$T_H" "$T_M" "$EVERY_DAY" null)"
    ALARM_EXTRA=
    arm a
    sys_home "the launcher in front" launcher_up || problems="$problems; $SYS_WHY"
    reminded && problems="$problems; a reminder 8 hours ahead is already up"
    jump_to $((T - 8 * 3600 - 60))
    not_reminded() { ! reminded; }
    sys_holds 3 "no reminder a minute before its moment" not_reminded || problems="$problems; the 8 hour reminder came a minute early"
    wait_for 150 reminded || { fail "no 8 hour reminder with a Skip"; return; }
    [ "$(now_s)" -ge $((T - 8 * 3600)) ] || problems="$problems; the 8 hour reminder came before its moment"
    own_notifications | grep -q "Go to bed, your alarm goes off in 8 hours" || problems="$problems; the 8 hour reminder does not say the text"
    own_notifications | grep -q "channel=$quiet" || problems="$problems; the silent reminder is not on the quiet channel"
    own_notifications | grep -q "channel=$loud" && problems="$problems; the silent reminder is on the sound channel"
    shade_set open || problems="$problems; $SYS_WHY"
    adb_ exec-out screencap -p > "$OUT/s67_reminder_shade.png"
    tap_in_shade Skip skipped || problems="$problems; no Skip to tap: $SYS_WHY"
    shade_set closed || problems="$problems; $SYS_WHY"
    wait_for 15 skipped || problems="$problems; Skip on the 8 hour reminder did not skip the ring"
    # A 15 minute reminder, with sound.
    fresh
    next_minute 1200
    ALARM_EXTRA=',"reminder":{"on":true,"minutes":15,"text":"Dentist in 15 minutes","sound":true}'
    seed a "$(alarm_json a "$T_H" "$T_M" "$EVERY_DAY" null)"
    ALARM_EXTRA=
    arm a
    sys_home "the launcher in front" launcher_up || problems="$problems; $SYS_WHY"
    reminded && problems="$problems; a reminder 20 minutes ahead is already up"
    jump_to $((T - 15 * 60 - 45))
    sys_holds 3 "no reminder 45 seconds before the 15 minute moment" not_reminded || problems="$problems; the 15 minute reminder came early"
    wait_for 120 reminded || { fail "no 15 minute reminder with a Skip"; return; }
    [ "$(now_s)" -ge $((T - 15 * 60)) ] || problems="$problems; the 15 minute reminder came before its moment"
    own_notifications | grep -q "Dentist in 15 minutes" || problems="$problems; the 15 minute reminder does not say the text"
    own_notifications | grep -q "channel=$loud" || problems="$problems; the reminder with sound is not on the sound channel"
    own_notifications | grep -q "channel=$quiet" && problems="$problems; the reminder with sound is on the quiet channel"
    # What Android keeps per channel: the quiet one has no sound, the other has the notification sound.
    local channels
    channels=$(dev dumpsys notification --noredact)
    printf '%s\n' "$channels" | grep -A6 "mId='$loud'" | grep -q 'mSound=content:' || problems="$problems; the sound channel has no sound"
    printf '%s\n' "$channels" | grep -A6 "mId='$quiet'" | grep -q 'mSound=null' || problems="$problems; the quiet channel has a sound"
    if [ -z "$problems" ]; then
        pass "an 8 hour reminder (silent) and a 15 minute one (with sound) came on time, said their text, used their own channel, and Skip skipped"
    else
        fail "${problems#; }"
    fi
}

# The edit lock: 50 minutes before the ring the alarm is free, 25 minutes before (the clock moved) the card shows
# the lock and the switch says no with the sheet, the alarm's own challenge opens it, then it can be switched
# off, and 6 minutes later (the clock moved again) it is locked once more.
s68() {
    if ! on_emulator; then
        note "s68 needs an emulator's clock"
        return
    fi
    scenario s68_edit_lock
    local problems=""
    fresh
    next_minute 3000
    ALARM_EXTRA=',"lock_minutes":30'
    seed a "$(alarm_json a "$T_H" "$T_M" "$EVERY_DAY" null air 0 0 3 1 '[["challenge.math","{\"difficulty\":\"easy\"}"]]')"
    ALARM_EXTRA=
    arm a
    start_app
    on_card 10
    keyboard_down || problems="$problems; the keyboard is up over the home screen at the start"
    locked_row() { ui snapshot | grep -q '"locked":true'; }
    is_enabled() { printf 'SELECT data FROM alarms;\n' | sql | grep -q '"enabled":true'; }
    locked_row && problems="$problems; locked 50 minutes before the ring"
    jump_to $((T - 25 * 60))
    wait_for 20 locked_row || problems="$problems; no lock on the card 25 minutes before the ring"
    screenshot s68_locked_card
    tap_has "On or off" 0 0.5
    on_screen_has "Locked until it rings at" 5 || problems="$problems; the switch gave no sheet: $(texts_of)"
    screenshot s68_locked_sheet
    is_enabled || problems="$problems; the switch turned a locked alarm off"
    tap "Do its challenges" 0 0.5
    on_screen "Solve the math problem" 5 || problems="$problems; no challenge to unlock with: $(texts_of)"
    solve_math
    on_screen_has "Open for 5 minutes" 8 || problems="$problems; nothing said about the unlock: $(texts_of)"
    locked_row && problems="$problems; still locked after its challenge"
    tap_has "On or off" 0 0.5
    wait_for 5 is_enabled && problems="$problems; an open alarm could not be switched off"
    tap_has "On or off" 0 0.5
    wait_for 5 is_enabled || problems="$problems; not on again"
    jump_to $((T - 25 * 60 + 6 * 60))
    sleep 1
    tap_has "On or off" 0 0.5
    on_screen_has "Locked until it rings at" 5 || problems="$problems; no sheet 6 minutes after the unlock: $(texts_of)"
    is_enabled || problems="$problems; it was switched off 6 minutes after the unlock"
    if [ -z "$problems" ]; then
        pass "free 50 minutes before, locked 25 minutes before (card and sheet), its challenge opened it, locked again after 5 minutes"
    else
        fail "${problems#; }"
    fi
}

# The reminder before an alarm that is already locked when it comes has no Skip button (the lock could be got round
# from the notice); with the lock shorter than the reminder, the same reminder has its Skip.
s68b() {
    if ! on_emulator; then
        note "s68b needs an emulator's clock"
        return
    fi
    scenario s68b_no_skip_on_a_locked_reminder
    local problems="" lock
    noticed() { own_notifications | grep -q 'channel=sentinel_reminders_popup'; }
    for lock in 30 10; do
        fresh
        next_minute 1500
        ALARM_EXTRA=",\"reminder\":{\"on\":true,\"minutes\":15,\"text\":\"\",\"sound\":false},\"lock_minutes\":$lock"
        seed a "$(alarm_json a "$T_H" "$T_M" "$EVERY_DAY" null)"
        ALARM_EXTRA=
        arm a
        sys_home "the launcher in front" launcher_up || problems="$problems; $SYS_WHY"
        jump_to $((T - 15 * 60 - 3))
        wait_for 60 noticed || { fail "no reminder with a lock of $lock minutes"; return; }
        if [ "$lock" = 30 ]; then
            reminded && problems="$problems; the reminder of a locked alarm has a Skip"
        else
            reminded || problems="$problems; the reminder of an alarm that is not locked has no Skip"
        fi
    done
    if [ -z "$problems" ]; then
        pass "no Skip on the reminder when the lock is on by then, a Skip when it is not"
    else
        fail "${problems#; }"
    fi
}

# The panels with rows of chips (ring limit, edit lock, fade in, are you up, reminder) in every language: no chip
# with its text cut short, nothing out of the window. A picture of each in <run>/chips/ for a person to look over.
chips_in_languages() { # scenario name, then pairs of locale code and name
    local scen=$1 problems="" code locale key title found
    shift
    scenario "$scen"
    mkdir -p "$OUT/chips"
    fresh
    next_minute 3600
    seed a "$(alarm_json a "$T_H" "$T_M" "$EVERY_DAY" null)"
    while [ $# -gt 0 ]; do
        code=$1 locale=$2
        shift 2
        use_settings '"Dark"' "\"$locale\"" '"setup_done":true,"language_follows_phone":false'
        start_app
        wait_for 40 page_is_up
        on_card 20 || { problems="$problems; $locale: no card"; dev am force-stop "$PKG"; continue; }
        # Android resizes the new window about a second after its first frame, and a touch in that moment is lost.
        window_past_late_resize
        press open a 0.6
        on_editor 5 || { problems="$problems; $locale: no editor"; continue; }
        # Android brings back the keyboard of the last typing for the app's window: it would cover the rows.
        hide_keyboard
        for key in config.sound config.ring_limit config.edit_lock config.wake_check config.notice; do
            title=$(python -I -c 'import json,sys; print(json.load(open(sys.argv[1],encoding="utf-8"))[sys.argv[2]])' "src/i18n/$code.json" "$key")
            # A row says its title and value together; the title alone is also there. By part of a text
            # "Son" would be the first line of the card ("Sonnera demain") in French.
            pre="screen=$(screen_no) at=$(where_has "$title" "$([ "$key" = config.notice ] && echo L)" | cut -c1-30) save=$(where Save | cut -c1-30)"
            if [ "$key" = config.sound ]; then
                tap "$title" 0 0.5
            elif [ "$key" = config.notice ]; then
                # In French the snooze and the reminder are both "Rappel": the reminder is the lower row.
                tap_row "$title" L
            else
                tap_row "$title"
            fi
            if ! wait_for 8 any_sheet_up; then
                screenshot "chips/${code}_${key}_NO_PANEL"
                note "s69 $locale/$key: no panel; before the tap: $pre; row=$(where_has "$title" | cut -c1-40) win=$(window_size) screen=$(screen_no) ed_sheet=$(sn ed_sheet) focus=$(focused_window | cut -c1-90)"
                problems="$problems; $locale/$key: no panel"
                continue
            fi
            sleep 0.5
            found=$(clipped)
            screenshot "chips/${code}_$key"
            # The sound panel has the fade-in chips below the volume.
            if [ "$key" = config.sound ]; then
                sheet_scroll
                found="$found$(clipped)"
                screenshot "chips/${code}_${key}_fade"
            fi
            [ -z "$found" ] || problems="$problems; $locale/$key: $found"
            press_back
            wait_for 8 no_sheet_up || problems="$problems; $locale/$key: Back leaves it open"
            on_editor 6 || note "s69 $locale/$key: after Back the editor is gone: screen=$(screen_no) focus=$(focused_window | cut -c1-90)"
        done
        dev am force-stop "$PKG"
    done
    if [ -z "$problems" ]; then
        pass "no chip cut short and nothing out of the window on five panels"
    else
        fail "${problems#; }"
    fi
}
s69a() { chips_in_languages s69a_chips_in_languages_1 en English nl Dutch es Spanish; }
s69b() { chips_in_languages s69b_chips_in_languages_2 fr French de German it Italian; }
s69c() { chips_in_languages s69c_chips_in_languages_3 pt Portuguese sv Swedish da Danish; }
s69d() { chips_in_languages s69d_chips_in_languages_4 nb Norwegian fi Finnish tr Turkish id Indonesian; }

# Colours of one's own: on the wheel, in a code, and never unreadable. They stay, also on the ring.
s61() {
    scenario s61_colours_of_ones_own
    fresh
    next_minute 3000
    seed a "$(alarm_json a 7 0 "$EVERY_DAY" null)"
    arm a
    on_home 10
    ui trace_clear > /dev/null
    local problems="" token_n
    go_tab Settings || { fail "no Settings tab: $(ui screen)"; return; }
    await_ui 5 "screen=3"
    tap_has Theme 0 0.5
    await_ui 5 "screen=5" || problems="$problems; no theme screen"
    [ "$(all_texts | grep -c -E '^(Same as the phone|Dark|Light|Pure black|Midnight|Forest|Plum|Sand|Mist)$')" -ge 9 ] || problems="$problems; not all themes: $(texts_of)"
    # The second row of swatches sits below the window on a screen with a navigation bar.
    scroll_to "Choose your own colour" || problems="$problems; not all accents"
    screenshot s61_theme
    sheet_scroll_up
    # A theme that comes with the app, and an accent for it.
    tap Sand 0 0.4
    [ "$(sn pal_bg)" = "#f3ede2" ] || problems="$problems; Sand is $(sn pal_bg)"
    [ "$(sn pal_light)" = true ] || problems="$problems; Sand is not a light theme"
    # On the wheel: a finger, as a person does it.
    tap_clear "Choose your own colour"
    on_screen "Colour code" 4 || on_screen_has "code" 3
    local before x y w h scale
    before=$(sn pal_accent)
    local boxes
    boxes=$(ui boxes:ColourControls)
    x=$(echo "$boxes" | grep -oE '"x":[0-9]+' | head -1 | tr -dc 0-9)
    y=$(echo "$boxes" | grep -oE '"y":[0-9]+' | head -1 | tr -dc 0-9)
    h=$(echo "$boxes" | grep -oE '"h":[0-9]+' | head -1 | tr -dc 0-9)
    scale=$(window_size | cut -d' ' -f3)
    # The wheel is 200 points across at the top of the controls: a point left of its middle.
    y=$((y - h / 2 + $(awk -v s="$scale" 'BEGIN { printf "%d", 108 * s }')))
    x=$((x - $(awk -v s="$scale" 'BEGIN { printf "%d", 70 * s }')))
    # A panel takes touches once it holds still.
    await_settled
    finger_swipe "$x" "$y" "$((x + 20))" "$((y + 20))" 300
    [ "$(sn pal_accent)" != "$before" ] || problems="$problems; the wheel does nothing"
    screenshot s61_wheel
    # In a code.
    set_field 0 '#c06020'
    [ "$(sn pal_accent)" = "#c06020" ] || problems="$problems; the code gives $(sn pal_accent)"
    done_sheet_theme() {
        hide_keyboard
        tap Done && await_ui 8 'theme_sheet=false' || {
            problems="$problems; the theme sheet is still up after Done: $(ui ready | grep -oE 'theme_sheet=[a-z]+')"
            note "after Done: $(ui texts | grep -oE '\{[^{}]*"t":"Done"[^{}]*\}' | tr '\n' ' ') $(ui ready)"
            local trace
            trace=$(ui trace)
            [ "${#trace}" -le 4500 ] || trace=${trace: -4500}
            note "trace (ms since start, newest last): $trace"
            screenshot s61_done_still_up
        }
    }
    done_sheet_theme
    # A theme of one's own, from what is on screen.
    tap "Your own theme" 0 0.5
    [ "$(sn pal_bg)" = "#f3ede2" ] || problems="$problems; does not start from the theme on screen"
    screenshot s61_own
    tap Background 0 0.5
    await_settled
    set_field 0 '#102030'
    [ "$(sn pal_bg)" = "#102030" ] || problems="$problems; background is $(sn pal_bg)"
    [ "$(sn pal_light)" = false ] || problems="$problems; a dark background is not a dark theme"
    [ "$(sn pal_text)" != "$(sn pal_bg)" ] || problems="$problems; text like the background"
    done_sheet_theme
    # Unreadable on purpose.
    scroll_to Text L
    tap Text L 0.5
    await_settled
    set_field 0 '#102030'
    [ "$(sn pal_text)" != "#102030" ] || problems="$problems; text as dark as the page can be chosen"
    on_screen "Changed a little so everything stays readable." 3 || problems="$problems; does not say it changed the colour"
    screenshot s61_adjusted
    tap Automatic 0 0.4
    is_there "Changed a little so everything stays readable." && problems="$problems; still says it changed the colour"
    done_sheet_theme
    # It stays, after a restart.
    dev am force-stop "$PKG"
    start_app
    on_card 10
    [ "$(sn pal_bg)" = "#102030" ] || problems="$problems; background not kept: $(sn pal_bg)"
    [ "$(sn pal_accent)" = "#c06020" ] || problems="$problems; accent not kept, or changed: $(sn pal_accent)"
    # What a saved Colorblind theme comes up as: not an empty screen.
    dev am force-stop "$PKG"
    use_settings '"Colorblind"' '"English"' '"setup_done":true,"language_follows_phone":false'
    start_app
    on_card 10 || problems="$problems; a saved Colorblind theme stops the app"
    [ -n "$(sn pal_bg)" ] || problems="$problems; no colours for a saved Colorblind theme"
    if [ -z "$problems" ]; then
        pass "wheel, code and own theme work, stay after a restart, and text stays readable"
    else
        fail "${problems#; }"
    fi
}

# Does the picture of the screen change in the next moment? Backgrounds move, and stand still with
# less motion.
screen_moves() {
    # Below the status bar: on a phone its icons change by themselves (the Wi-Fi activity arrows).
    local top
    top=$(dev dumpsys window | grep -oE 'type=statusBars frame=\[0,0\]\[[0-9]+,[0-9]+\]' | head -1 | grep -oE '[0-9]+\]$' | tr -d ']')
    case "$top" in '' | *[!0-9]*) top=150 ;; esac
    adb_ exec-out screencap -p > "$OUT/_move_a.png"
    sleep 0.7
    adb_ exec-out screencap -p > "$OUT/_move_b.png"
    python -I -c '
import sys
from PIL import Image, ImageChops
a, b = (Image.open(p).convert("RGB") for p in sys.argv[1:3])
top = int(sys.argv[3])
box = (0, top, a.width, a.height)
sys.exit(0 if ImageChops.difference(a.crop(box), b.crop(box)).getbbox() else 1)
' "$OUT/_move_a.png" "$OUT/_move_b.png" "$top"
}
# The size of the picture a ring look keeps (the newest file in ring_photos), as "WxH".
kept_photo_size() {
    local name
    name=$(as_app "ls -t files/ring_photos 2> /dev/null | head -1")
    [ -n "$name" ] || return 1
    adb_ exec-out "run-as $PKG cat $DATA/files/ring_photos/$name" > "$OUT/_kept_photo.jpg"
    file "$OUT/_kept_photo.jpg" | grep -oE '[0-9]{3,5} ?x ?[0-9]{3,5}' | tail -1 | tr -d ' '
}

# The ring screen of one's own: chosen in the editor, seen in a preview that does nothing, and there
# on the real ring with every button where it was. With less motion it stands still, a photo comes
# in small, and a photo that is gone is the plain screen.
s62() {
    scenario s62_ring_screen_of_ones_own
    fresh
    next_minute 320
    seed a "$(alarm_json a "$T_H" "$T_M" "$EVERY_DAY" null)"
    arm a
    on_card 10
    local problems="" id=a
    open_look() {
        press open "$id" 0.8
        on_editor
        open_row 'Alarm screen'
    }
    preview_up() { [ "$(sn ed_preview)" = true ]; }
    preview_down() { [ "$(sn ed_preview)" = false ]; }
    open_look
    [ "$(all_texts | grep -c -E '^(Plain|Aurora|Sunrise|Waves|Stars|Rings|Bubbles|Photo)$')" -ge 8 ] || problems="$problems; not all backgrounds: $(texts_of)"
    screenshot s62_panel
    tap_in Waves 0 0.3
    tap_clear Colour has 300
    set_field_clear 0 '#c06020'
    hide_keyboard
    tap Done 0 0.4
    scroll_to Mono
    tap_in Mono 0 0.2
    # A preview: as it will look, without a sound, and it closes by a tap and by Back.
    tap Preview L 0.6
    await_ui 3 'ed_preview=true' || problems="$problems; no preview"
    [ "$(sn look_kind)" = 3 ] || problems="$problems; the preview has not got the background: $(sn look_kind)"
    [ "$(sn look_clock)" = 4 ] || problems="$problems; the preview has not got the clock: $(sn look_clock)"
    [ "$(sn look_c1)" = "#c06020" ] || problems="$problems; the preview's colour is $(sn look_c1)"
    screen_moves || problems="$problems; nothing moves in the preview"
    sound_on && problems="$problems; the preview makes a sound"
    screenshot s62_preview
    tap "Preview. Tap to close." 0 0.4
    await_ui 3 'ed_preview=false' || problems="$problems; a tap does not close the preview"
    tap Preview L 0.5
    await_ui 3 'ed_preview=true'
    press_back
    await_ui 3 'ed_preview=false'
    editor_sheet_open || problems="$problems; Back does more than close the preview"
    done_sheet
    tap Save
    # The real thing: the clock goes to just before it.
    jump_to $((T - 5))
    if await_ring a 40 && on_screen "Turn off" 15; then
        [ "$(sn look_kind)" = 3 ] || problems="$problems; the ring has not got the background"
        [ "$(sn look_clock)" = 4 ] || problems="$problems; the ring has not got the clock"
        [ "$(sn look_c1)" = "#c06020" ] || problems="$problems; the ring's colour is $(sn look_c1)"
        screen_moves || problems="$problems; nothing moves on the ring"
        is_there Snooze || has_there Snooze || problems="$problems; Snooze is not there"
        sound_on || problems="$problems; the ring is silent"
        screenshot s62_ring
        tap "Turn off" 0 0.3
        wait_for 5 sound_off || problems="$problems; still ringing after turning it off"
    else
        problems="$problems; did not ring: state=$(state a)"
    fi
    # With less motion it stands still.
    dev am force-stop "$PKG"
    use_settings '"Dark"' '"English"' '"setup_done":true,"language_follows_phone":false,"less_motion":true'
    start_app
    on_card 10
    open_look
    tap Preview L 0.6
    await_ui 3 'ed_preview=true'
    screen_moves && problems="$problems; things move with less motion on"
    screenshot s62_still
    press_back
    # A photo that is gone: the plain screen, nothing else changes.
    dev am force-stop "$PKG"
    fresh
    ALARM_EXTRA=',"ring_look":{"backdrop":"Photo","photo":"gone.jpg"}'
    seed b "$(alarm_json b 7 0 "$EVERY_DAY" null)"
    ALARM_EXTRA=
    start_app
    on_card 10
    id=b
    open_look
    tap Preview L 0.6
    await_ui 3 'ed_preview=true' || problems="$problems; no preview for a photo that is gone"
    [ "$(sn look_photo)" = false ] || problems="$problems; dressed for a photo that is gone"
    is_there "Turn off" || has_there "to turn it off" || problems="$problems; a photo that is gone takes the screen with it"
    press_back
    await_ui 3 'ed_preview=false'
    # A photo of one's own, through the phone's picker: it differs per phone, and this looks for its
    # words. Larger than a screen needs: it comes in small.
    if on_emulator && [ -f "$OUT/s62_ring.png" ]; then
        # A ring can leave the screen off and locked, and a picker does not show over that.
        wake
        sys_wait "$SYS_DEADLINE" "the lock screen gone" kg_gone || note "$SYS_WHY"
        dev rm -f /sdcard/Download/af_photo.png
        adb_ push "$OUT/s62_ring.png" /sdcard/Download/af_photo.png > /dev/null
        dev content call --uri content://media --method scan_volume --arg external_primary > /dev/null 2>&1
        # It still has that gone photo: another one, from the link.
        tap_in "Choose another photo" 0 1
        if picker_pick af_photo.png; then
            chosen() { [ -n "$(as_app "ls files/ring_photos 2> /dev/null")" ]; }
            wait_for 40 chosen || problems="$problems; the photo was not kept"
            await_settled
            tap Preview L
            await_ui 5 'ed_preview=true'
            [ "$(sn look_photo)" = true ] || problems="$problems; the photo does not show"
            local dims
            dims=$(kept_photo_size)
            case "$dims" in
                1600x* | *x1600) ;;
                *) problems="$problems; a photo of the wrong size: '$dims'" ;;
            esac
            screenshot s62_photo
            press_back
        else
            problems="$problems; the picker did not offer the photo: $SYS_WHY"
            sys_press KEYCODE_BACK "the app getting the keys back from the picker" "$SYS_DEADLINE" focus_in_app || note "$SYS_WHY"
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
    sys_home "the launcher in front" launcher_up || note "$SYS_WHY"
    local problems="" seen="" t0 at
    # The banner is read from a tree that was really read: a dump that failed says nothing (it used to count as "no banner").
    # The ring has to be taken up by the app within 5 s of its sound (await_ring says which part was missing).
    if ! await_ring a 40 5; then
        problems="$problems; $RING_WHY"
    elif ! on_screen "Turn off" 15; then
        problems="$problems; the app took the ring up, but its screen did not come up in 15s (on top: $(top_activity))"
    else
        t0=$(now_ms)
        for at in 1000 2000; do
            sys_wait 10 "the moment ${at}ms after the screen came up" elapsed_since "$t0" "$at"
            heads_up_banner_there
            case $? in
                0) seen="$seen $(($(now_ms) - t0))ms" ;;
                2) problems="$problems; the screen could not be read at ${at}ms" ;;
            esac
        done
        screenshot s63_ring
        keyboard_down || problems="$problems; the keyboard is up over the alarm's screen"
        [ -z "$seen" ] || problems="$problems; its banner is over the alarm's screen at$seen after it came up"
        own_notifications | grep -q 'id=900[12]' || problems="$problems; no notification while it rings"
        press ring-turn-off
        wait_for 30 sound_off || problems="$problems; still ringing after turning it off"
    fi
    if [ -z "$problems" ]; then
        pass "no banner over the alarm's screen, the notification stays in the list"
    else
        fail "${problems#; }"
    fi
}

# The main process is cached, not yet frozen, at the ring: Android 14 and 15 hold a broadcast back for such an app
# (a receiver registered at run time), 16 delivers it. The app must take the ring up within 5 s of the sound, and its
# screen must be on top. Where the phone does not get the app into that state (older versions, or it was frozen already),
# the result is SKIP with the state it had: never a PASS for a state the test did not reach.
s63b() {
    scenario s63b_ring_with_the_app_cached
    fresh
    next_minute 26
    seed a "$(alarm_json a "$T_H" "$T_M" "$EVERY_DAY" null)"
    arm a
    until_past $((T - 12))
    adb_ logcat -c -b all
    go_home
    probe_other_apps deep4
    until_past $((T - 1))
    local at_ring frozen=no
    at_ring=$(probe_main_state)
    adb_ logcat -d -b events 2> /dev/null | tr -d '\r' | grep -E 'am_freeze' | grep -q 'com\.fidderr\.alarmfree\]' && frozen=yes
    dev am force-stop com.android.settings
    dev am force-stop com.google.android.contacts
    dev am force-stop com.android.chrome
    dev am force-stop com.google.android.apps.messaging
    case "$at_ring" in
        *cch*) ;;
        *) skip "the main process is not cached at the ring: $at_ring"; return ;;
    esac
    [ "$frozen" = no ] || { skip "the main process was frozen before the ring: $at_ring"; return; }
    local problems=""
    if ! await_ring a 40 5; then
        problems="$problems; $RING_WHY"
    elif ! on_screen "Turn off" 5; then
        problems="$problems; the app took the ring up, but its screen is not on top 5s later (on top: $(top_activity))"
    else
        keyboard_down || problems="$problems; the keyboard is up over the alarm's screen"
        tap_turn_off || problems="$problems; $TURN_OFF_WHY"
    fi
    if [ -z "$problems" ]; then
        pass "taken up within 5s of the sound with the main process $at_ring, its screen on top"
    else
        fail "${problems#; } (main process at the ring: $at_ring)"
    fi
}

# The guardian does not trust its one heads-up: the debug build is told (a file) to ignore it, as if Android held it back,
# and the guardian has to bring the ring screen up itself, about 2 s after the heads-up it sent.
s63c() {
    scenario s63c_ring_with_the_heads_up_held_back
    fresh
    next_minute
    seed a "$(alarm_json a "$T_H" "$T_M" "$EVERY_DAY" null)"
    arm a
    as_app "mkdir -p files/sentinel && touch files/sentinel/debug_hold_heads_up"
    go_home
    local problems=""
    if ! await_sound 40; then
        problems="$problems; did not ring: no sound within 40s past T"
    elif ! wait_for 6 top_is_main; then
        problems="$problems; sound at +${SOUND_AT}s, but the app's screen did not come up in 6s (on top: $(top_activity), state=$(state a))"
    elif ! await_state a firing 3; then
        problems="$problems; the screen came up, but the app's state did not become firing: state=$(state a), on top: $(top_activity)"
    elif ! on_screen "Turn off" 15; then
        problems="$problems; the state is firing, but the ring screen did not draw 'Turn off' in 15s: on top: $(top_activity)"
    fi
    local said
    said=$(as_app "grep -c 'not taken up' files/sentinel/diag.log 2> /dev/null")
    [ "${said:-0}" -ge 1 ] 2> /dev/null || problems="$problems; the guardian did not say it started the screen itself"
    as_app "rm -f files/sentinel/debug_hold_heads_up"
    if [ -z "$problems" ]; then
        pass "the heads-up was held back, the guardian brought the ring screen up itself (state=$(state a))"
    else
        fail "${problems#; }"
    fi
}

# A large display size and a large text, as a tester had it: the card of an alarm, the quick row,
# the days and the list of settings in the editor all stay inside the window and inside their card.
s65() {
    scenario s65_large_size_fits
    fresh
    seed a "$(alarm_json a 7 0 "$EVERY_DAY" null)"
    arm a
    local problems="" old_font said look
    # What sticks out, asked until the window has taken its new size (a start in between is blank).
    fits() {
        local n=0
        said=$(ui "$1")
        while [ "$said" != none ] && [ "$n" -lt 8 ]; do
            await_settled 5
            said=$(ui "$1")
            n=$((n + 1))
        done
    }
    old_font=$(dev settings get system font_scale)
    # An unset font scale reads as the word null: writing that back leaves 1.3 in place.
    case "$old_font" in null | "") old_font=1.0 ;; esac
    dev wm size 1080x2376
    # One look on the phone (a real window changing its density); both looks and every row are
    # checked in cargo test (ui::flow_tests::s65_large_size_fits).
    for look in "560 1.3"; do
        dev wm density "${look% *}"
        dev settings put system font_scale "${look#* }"
        start_app
        on_card 15 || problems="$problems; no list at $look"
        fits fit:AlarmCard
        [ "$said" = none ] || problems="$problems; home at $look: $said"
        screenshot "s65_home_${look% *}_${look#* }"
        press open a 0.8
        on_editor || problems="$problems; no editor at $look"
        fits fit:RowList
        [ "$said" = none ] || problems="$problems; editor at $look: $said"
        screenshot "s65_editor_${look% *}_${look#* }"
        dev am force-stop "$PKG"
    done
    dev settings put system font_scale "${old_font:-1.0}"
    dev wm size reset
    dev wm density reset
    if [ -z "$problems" ]; then
        pass "card, quick row, days and settings fit at large sizes"
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
    if await_ring a 40 && on_screen "Turn off" 20; then
        echo "rang $(($(now_s) - T)) s after its time"
        wait_for 8 sound_on || problems="$problems; the ring is silent"
        screenshot s60p_a_ring
        tap "Turn off" 0 0.3
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
    on_card 20
    echo "the list says: $(row_when)"
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
    on_card 15
    local problems="" i=0 picked="" box
    dev rm -f /sdcard/Download/af_photo.jpg
    adb_ push "$PHOTO" /sdcard/Download/af_photo.jpg > /dev/null
    dev content call --uri content://media --method scan_volume --arg external_primary > /dev/null 2>&1
    press open a 0.8
    on_editor
    open_row 'Alarm screen'
    tap_in Photo 0 2.5
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
        on_screen Waves 15 || problems="$problems; not back on the panel"
        sleep 1
        echo "kept: $(as_app "ls -l files/ring_photos 2> /dev/null | tail -1" | awk '{print $5}') bytes"
        tap Preview L 0.8
        await_ui 8 'ed_preview=true'
        [ "$(sn look_photo)" = true ] || problems="$problems; the photo does not show"
        size=$(kept_photo_size)
        echo "shown at $size"
        [ "$size" = 1200x1600 ] || problems="$problems; the photo came in as $size, not upright at 1200x1600"
        screenshot s62p_photo
        press_back
        on_screen Waves 5
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
. "$(dirname "$0")/a11y_audit.sh"
# The scenarios that need a device and test the runner itself (baseline, watchdog, INFRA): by name only.
. "$(dirname "$0")/selftest.sh"
# The probe of typed text in the text fields: by name only (probe_text.sh).
. "$(dirname "$0")/probe_text.sh"
# Probes of Android's own screens, by name only.
. "$(dirname "$0")/probe.sh"
# The ring-screen probe (Home before a ring, what the main process does): by name only.
. "$(dirname "$0")/ringprobe.sh"

ALL="s01 s03 s04 s05 s06 s07 s07d s10 s11 s13 s13b s13d s14 s15 s16 s16b s17 s19 s22 s23a s23b s23c s25 s26x2 s28 s29 s30 s31 s32 s33 s34 s36 s37 s38 s39 s40 s42 s43 s44 s45 s46 s47 s48 s48b s49 s50 s51 s52 s53 s55 s56 s57 s58 s59 s60 s61 s62 s63 s63b s63c s65 s66 s67 s68 s68b s69a s69b s69c s69d s70 big_screens s54"
ON_A_PHONE="s01 s03 s04 s05 s06 s07 s10 s15 s16 s16b s17 s22 s23a s23b s23c s25 s26x2 s28 s29 s30 s31 s32 s33 s36 s37 s38 s39 s42 s43 s45 s46 s47 s49 s50 s51 s52 s53 s55 s57 s58 s59 s61 s62"

: > "$RESULTS"
RUN_START=$SECONDS
# Started by itself, and not from a run that claimed the device already.
[ -n "${RUN_ID:-}" ] || claim_device
open_shells
chaos_hog_start
if on_emulator; then
    dev settings put global auto_time 0
    # What an earlier run that did not get to its end left.
    dev wm size reset
    dev wm density reset
    dev locksettings set-disabled false > /dev/null
    # A PIN that a run which was killed in s13 or s54 left, before the first reading of the baseline.
    dev locksettings clear --old 1234 > /dev/null 2>&1
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
# this one is still on its light ones. LAST names the ones for the end, when that other one
# is done: a restart takes half as long again next to a busy emulator.
RUN_LIST="${FIRST:-}"
for s in ${*:-$DEFAULT_LIST}; do
    case " ${SKIP:-} ${FIRST:-} ${LAST:-} " in
        *" $s "*) continue ;;
    esac
    RUN_LIST="$RUN_LIST $s"
done
for s in ${LAST:-}; do
    case " ${SKIP:-} " in
        *" $s "*) continue ;;
    esac
    case " ${*:-$DEFAULT_LIST} " in
        *" $s "*) RUN_LIST="$RUN_LIST $s" ;;
    esac
done
# A scenario whose restart failed must not leave the next one on a half-booted device.
boot_done() { [ "$(adb_ shell getprop sys.boot_completed 2> /dev/null | tr -d '\r')" = 1 ] && device_ok; }
ensure_booted() { # unit
    if on_emulator && ! boot_done; then
        note "the device is not booted before $1: waiting"
        wait_for 180 boot_done
        open_shells
    fi
}
for s in $RUN_LIST; do
    ensure_booted "$s"
    # big_screens is no unit: each of its scenarios is (with_screen).
    if [ "$s" = big_screens ]; then big_screens; else run_unit "$s"; fi
done
selftest_cleanup
chaos_hog_stop
dev am force-stop "$PKG"
echo
echo "===== SUMMARY ($SERIAL) ====="
cat "$RESULTS"
echo "passed: $(grep -c '^PASS' "$RESULTS")  flaky: $(grep -c '^FLAKY' "$RESULTS")  failed: $(grep -c '^FAIL' "$RESULTS")  skipped: $(grep -c '^SKIP' "$RESULTS")  in $(((SECONDS - RUN_START) / 60)) min $(((SECONDS - RUN_START) % 60)) s"
slow=$(sed -nE 's/^[A-Z]+ +([^ ]+) +\[([0-9]+)s\].*/\2 \1/p' "$RESULTS" | awk '$1 > 60 { printf "%s (%ss) ", $2, $1 }')
[ -z "$slow" ] || echo "over a minute: $slow"
# A device that broke is not a failed app: its lines are apart (baseline.sh).
if grep -q '^INFRA' "$RESULTS"; then
    echo "the device broke (INFRA, not a failure of the app): $(grep -c '^INFRA' "$RESULTS") line(s), $(grep '^INFRA' "$RESULTS" | grep -vc 'rerun follows') standing after the rerun"
fi
chaos_footer
run_exit_code "$RESULTS"
exit $?
