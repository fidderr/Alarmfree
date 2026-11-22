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
PKG=com.fidderr.alarmfree
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

# Ask the snapshot until it says what we wait for (a grep pattern), for up to $2 seconds. With $3 set, a window that
# does not answer (the app went to the background) is brought back to the front.
wait_for() {
    local pattern=$1 limit=${2:-20} revive=${3:-} i=0 snap=""
    while [ "$i" -lt "$limit" ]; do
        snap=$(ui snapshot 2>&1 | grep -v "^sh:")
        if echo "$snap" | grep -q -- "$pattern"; then
            return 0
        fi
        if [ -n "$revive" ] && ! echo "$snap" | grep -q '"screen":'; then
            dev monkey -p "$PKG" 1 > /dev/null 2>&1
        fi
        sleep 1
        i=$((i + 1))
    done
    LAST="$snap"
    return 1
}

step() { # name pattern [seconds] [revive]
    if wait_for "$2" "${3:-20}" "${4:-}"; then
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

# The first start has two screens before the list: the intro (6), then the reliability setup (4). Back leaves each one.
# Asked from the app which one is open, and pressed again if a key was lost, until the list (0) is there.
first_run_screens() {
    local i=0 s
    while [ "$i" -lt 30 ]; do
        s=$(ui screen | sed -n 's/.*screen=\([0-9]*\).*/\1/p')
        case "$s" in
            0) return 0 ;;
            4 | 6) ui press:back > /dev/null ;;
        esac
        sleep 1
        i=$((i + 1))
    done
    return 1
}
first_run_screens
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
# Snoozing sends the app to the background, where the window does not answer: whether that happens before the first
# question or after it, the wait brings the window back and reads the state from it.
step "snooze ends the ring screen" '"ringing":false' 20 revive
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
