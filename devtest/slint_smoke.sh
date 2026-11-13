#!/bin/bash
# A short check of the Slint build on one emulator, driven through the `ui:` commands of the
# devtest channel (src/debug_hooks.rs): make an alarm, ring it by moving the clock, snooze,
# let it ring again and turn it off. Prints PASS or FAIL per step. Debug build only.
#
#   SERIAL=emulator-5556 bash devtest/slint_smoke.sh
#
# The old scenarios (matrix.sh) drive the page with `js:` and do not run against this build.

export MSYS_NO_PATHCONV=1
ADB="${ADB:-$LOCALAPPDATA/Android/Sdk/platform-tools/adb.exe}"
SERIAL="${SERIAL:-emulator-5556}"
PKG=com.fidderr.alarmfree.slint
FILES="/data/user_de/0/$PKG/files"
OUT="${OUT:-devtest-out}"
mkdir -p "$OUT"
FAILS=0

dev() { timeout 60 "$ADB" -s "$SERIAL" shell "$@" | tr -d '\r'; }
as_app() { dev "run-as $PKG sh -c '$1'"; }

# One command to the app, and its answer.
ui() {
    as_app "cd $FILES && rm -f debug_script_result.txt && echo ui:$1 > debug_action.tmp && mv debug_action.tmp debug_action.txt && i=0; while [ ! -e debug_script_result.txt ] && [ \$i -lt 500 ]; do sleep 0.02; i=\$((i + 1)); done; cat debug_script_result.txt 2> /dev/null"
}

# Ask the snapshot until it says what we wait for (a grep pattern), for up to $2 seconds.
wait_for() {
    local pattern=$1 limit=${2:-20} i=0 snap=""
    while [ "$i" -lt "$limit" ]; do
        snap=$(ui snapshot 2>&1 | grep -v "^sh:")
        if echo "$snap" | grep -q -- "$pattern"; then
            return 0
        fi
        sleep 1
        i=$((i + 1))
    done
    LAST="$snap"
    return 1
}

step() { # name pattern [seconds]
    if wait_for "$2" "${3:-20}"; then
        echo "PASS  $1"
    else
        echo "FAIL  $1"
        echo "      last snapshot: ${LAST:0:300}"
        FAILS=$((FAILS + 1))
    fi
}

jump_clock() { # seconds
    local now
    now=$(dev date +%s)
    dev cmd alarm set-time $(((now + $1) * 1000)) > /dev/null
}

dev pm clear "$PKG" > /dev/null
dev monkey -p "$PKG" 1 > /dev/null 2>&1
step "the window answers" '"screen":' 40

# The first run screen comes before the list.
if ui screen | grep -q "screen=4"; then
    ui press:back > /dev/null
fi
step "the alarm list is empty" '"rows":\[\]' 10
step "the alarm list is open" '"screen":0' 10

ui press:quick:0 > /dev/null
step "a quick alarm is on the list" '"rows":\[{[^]]*"on":true' 10

# The home-screen widget text follows the next alarm (the SDK keeps it in a file).
if as_app "cat $FILES/sentinel/home_widget.txt" | grep -q "Quick alarm"; then
    echo "PASS  the widget text names the next alarm"
else
    echo "FAIL  the widget text names the next alarm"
    FAILS=$((FAILS + 1))
fi

# The alarm rings 5 minutes from now: move the clock to just before it.
# (A quick alarm rounds up to the minute, so the moment is read from what is armed.)
armed_ms=$(as_app "cat $FILES/sentinel/armed/*.armed 2> /dev/null | head -n 1")
if [ -n "$armed_ms" ] && [ "$armed_ms" -gt 0 ] 2> /dev/null; then
    dev cmd alarm set-time $((armed_ms - 5000)) > /dev/null
else
    jump_clock 290
fi
step "the alarm rings" '"ringing":true' 40
"$ADB" -s "$SERIAL" exec-out screencap -p > "$OUT/slint_smoke_ring.png"

ui press:ring-snooze > /dev/null
step "snooze ends the ring screen" '"ringing":false' 20
# Snoozing sends the app to the background, where the window does not answer.
dev monkey -p "$PKG" 1 > /dev/null 2>&1
step "the card says it is snoozed" '"when":"[^"]' 10

# The snooze is 5 minutes (the default): ring again.
jump_clock 310
step "it rings again after the snooze" '"ringing":true' 40

ui press:ring-turn-off > /dev/null
dev monkey -p "$PKG" 1 > /dev/null 2>&1
step "turn off ends the ring screen" '"ringing":false' 20
# A normal alarm that was turned off lands on History (src/ui/ring.rs: a hidden one leaves none).
step "history shows what just happened" '"screen":2' 10

if [ "$FAILS" -eq 0 ]; then
    echo "ALL PASS"
else
    echo "$FAILS step(s) FAILED"
    exit 1
fi
