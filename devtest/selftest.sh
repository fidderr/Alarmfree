#!/bin/bash
# Scenarios that test the runner itself (devtest/baseline.sh) on a device. Sourced by matrix.sh, in no list: by name only,
#   SCENARIOS="t01_leftovers t02_after_leftovers t03_device_breaks t04_app_dies"      ([only: ...] on GitHub)
#
# What the results must show:
#   t01_leftovers         PASS  (it leaves a font scale, a screen size and the keyboard on purpose)
#   t02_after_leftovers   PASS  and NOTE lines under its own name for the font scale, the screen size and the keyboard
#   t03_device_breaks     INFRA (the system process was killed), then PASS from the second run on the restarted device
#   t04_app_dies          PASS  and no INFRA (the app's processes were killed; that is the app)
# t02 needs t01 right before it in the same run. They are for an emulator: the leftovers are put back there (by the baseline
# of the scenario after t01, and by selftest_cleanup at the end of the run when t01 was the last).

SELFTEST_LEFT=""
# What t01 left, put back, when no scenario came after it to do that. Called by the runner when the list is done.
selftest_cleanup() {
    [ -n "$SELFTEST_LEFT" ] || return 0
    SELFTEST_LEFT=""
    dev wm size reset > /dev/null 2>&1
    dev settings put system font_scale 1.0 > /dev/null 2>&1
    hide_keyboard
}

# Leaves what a scenario can leave, then ends.
t01_leftovers() {
    scenario t01_leftovers
    on_emulator || { skip "leaves a screen size, a font scale and the keyboard: for an emulator"; return; }
    fresh
    local size w h x y
    size=$(dev wm size | sed -n 's/^Physical size: //p' | head -1)
    w=${size%x*}
    h=${size#*x}
    SELFTEST_LEFT=1
    dev settings put system font_scale 1.3
    dev wm size $((w * 5 / 6))x$((h * 5 / 6))
    seed a "$(alarm_json a 7 0 "$EVERY_DAY" null)"
    arm a
    on_card 10
    press open a 0.8
    on_editor
    tap_has Label 0 0.6
    await_ui 3 'editor_sheet!=None' || { fail "no label panel to type in"; return; }
    read -r x y <<< "$(field_xy)"
    dev input tap "$x" "$y" # lint-ok: the keyboard only comes up for a real touch on a field
    wait_for 8 keyboard_up || { fail "the keyboard did not come up"; return; }
    pass "left on purpose: font scale 1.3, screen size $((w * 5 / 6))x$((h * 5 / 6)), the keyboard up"
}

# Starts from a baseline that was read and found right, and the results say what t01 left.
t02_after_leftovers() {
    scenario t02_after_leftovers
    local problems="" want
    grep -q '^PASS  t01_leftovers' "$RESULTS" || { skip "t01_leftovers did not run just before"; return; }
    SELFTEST_LEFT=""
    fresh
    baseline_read
    baseline_diffs
    [ ${#BL_BAD[@]} -eq 0 ] || problems="$problems; the baseline is not clean: ${BL_BAD[*]}"
    for want in "font scale" "screen size" "keyboard is up"; do
        grep -q "^NOTE  t02_after_leftovers  .*$want" "$RESULTS" || problems="$problems; no NOTE for: $want"
    done
    if [ -z "$problems" ]; then
        pass "clean baseline, and a NOTE for each thing t01 left"
    else
        fail "${problems#; }"
    fi
}

# Kills the system process in the middle. The first run must end as INFRA and the second, on the restarted device, pass:
# the scenario knows it is the second run from a marker file of this run on the host.
t03_device_breaks() {
    scenario t03_device_breaks
    on_emulator || { skip "kills the system process: for an emulator"; return; }
    # Without root `su 0 kill` does nothing and nothing would break: not a failure of anything.
    [ "$(dev "su 0 id -u" 2> /dev/null)" = 0 ] || { skip "no root on this image (su 0): the system process cannot be killed"; return; }
    fresh
    local marker="$OUT/t03_device_breaks.marker" seen="" pid="" now=""
    read -r seen < "$marker" 2> /dev/null
    if [ -n "${RUN_ID:-}" ] && [ "$seen" = "$RUN_ID" ]; then
        pass "second run on the restarted device: nothing killed, system_server $(dev pidof system_server)"
        return
    fi
    pid=$(dev pidof system_server)
    [ -n "$pid" ] || { fail "no system_server to kill"; return; }
    echo "${RUN_ID:-none}" > "$marker"
    kill_pid_once "$pid"
    came_back() {
        now=$(dev pidof system_server)
        [ -n "$now" ] && [ "$now" != "$pid" ]
    }
    wait_for 120 came_back || { fail "system_server $pid was killed but no new one came up in 120s (no root on this image?)"; return; }
    pass "system_server $pid killed, a new one ($now) is up"
}

# Kills the app's own processes: the device is whole, so this is the app's business and never INFRA.
t04_app_dies() {
    scenario t04_app_dies
    fresh
    local pid
    seed a "$(alarm_json a 7 0 "$EVERY_DAY" null)"
    arm a
    on_card 10
    pid=$(dev pidof system_server)
    kill_all
    pass "the app's processes were killed; system_server ${pid} is untouched"
}
