#!/bin/bash
# The acknowledged real touch (finger_do and what is built on it) and the touches through the app (tap, tap_has,
# tap_xy, go_tab) against a stubbed device (no adb): PASS or FAIL for each case. Usage: bash devtest/finger.test.sh
# The real functions of lib.sh run; `ui` and `dev` are replaced by stubs that record what would have been sent.
here=$(cd "${BASH_SOURCE[0]%/*}" && pwd)
tmp=$(mktemp -d) || exit 1
trap 'rm -rf "$tmp"' EXIT
bad=0

cat > "$tmp/drive.sh" << 'E'
# Env: LIB DIR CASE. The stub `ui` answers from the files AWAIT (one answer per line, the last one repeats) and READY.
export OUT="$DIR/out" TMPDIR="$DIR" RESULTS="$DIR/results.txt" CURRENT=case
mkdir -p "$OUT"
. "$LIB"
CURRENT=case
SENT="$DIR/sent.log"
: > "$SENT"
: > "$RESULTS"
n_await=0
ui() {
    echo "ui $1" >> "$SENT"
    case "$1" in tapwait:*) echo "${UI_EXTRA:-none}" > "$DIR/extra" ;; esac
    case "$1" in
        ready) if [ -n "$READY_FILE" ]; then
            n_ready=$(($(cat "$DIR/n_ready" 2> /dev/null || echo 0) + 1))
            echo "$n_ready" > "$DIR/n_ready"
            sed -n "${n_ready}p;\$p" "$READY_FILE" | head -1
        else echo "frames=1 screen=0 input_seen=7"; fi ;;
        await:*)
            n=$(($(cat "$DIR/n_await" 2> /dev/null || echo 0) + 1))
            echo "$n" > "$DIR/n_await"
            line=$(sed -n "${n}p" "$AWAIT_FILE")
            [ -n "$line" ] || line=$(tail -n 1 "$AWAIT_FILE")
            echo "$line"
            ;;
        tap*) echo "${TAP_ANSWER:-OK}" ;;
        press:*) echo "${PRESS_ANSWER:-OK}" ;;
        screen) echo "screen=${SCREEN_NOW:-0}" ;;
        *) echo OK ;;
    esac
}
dev() { echo "dev $*" >> "$SENT"; }
after_input() { echo after_input >> "$SENT"; }
await_settled() { echo settle >> "$SENT"; }
await_ui() { echo "await_ui $*" >> "$SENT"; [ -z "${AWAIT_UI_FAIL:-}" ]; }
screen_no() { echo "${SCREEN_NOW:-0}"; }
still_where() { echo "100 200"; }
SETTLED_AT=""
# The result line of a scenario that ends here: the real pass(), a fail() that only writes its line.
fail() { echo "FAIL  $CURRENT  $1" >> "$RESULTS"; }
SCENARIO_START=$SECONDS
: > "$STEP_FAILED_FILE"
case "$CASE" in
    tap_then_pass)
        tap "Hello" 2
        pass "all fine"
        ;;
    tolerated_then_pass)
        TAP_MAY_FAIL=1 tap "Hello" 2
        pass "all fine"
        ;;
    finger_then_pass)
        finger Hello
        pass "all fine"
        ;;
    press_then_pass)
        press open a
        pass "all fine"
        ;;
    tab_then_pass)
        go_tab Settings
        pass "all fine"
        ;;
    clean_pass) pass "all fine" ;;
    finger_ok) finger Hello ;;
    finger_swipe) finger_swipe 1 2 3 4 300 ;;
    tap) tap "Hello" 2 ;;
    tap_has) tap_has "Hello" L ;;
    tap_xy) tap_xy 10 20 ;;
    go_tab) go_tab Settings ;;
esac
echo "rc=$?" >> "$SENT"
E

# run_case name case [VAR=value...]
run_case() {
    local name=$1 case=$2
    shift 2
    mkdir -p "$tmp/$name"
    env LIB="$here/lib.sh" DIR="$tmp/$name" CASE="$case" "$@" bash "$tmp/drive.sh" > "$tmp/$name/stdout" 2>&1
}
verdict() { # name ok
    if [ "$2" = 1 ]; then
        echo "PASS  $1"
    else
        echo "FAIL  $1"
        bad=1
    fi
}
taps() { grep -c '^dev input' "$tmp/$1/sent.log"; }
has() { grep -q -- "$2" "$tmp/$1/sent.log"; }

printf 'OK 40ms\n' > "$tmp/f_ok"
printf 'TIMEOUT after 8000ms, Now: x\nOK 40ms\n' > "$tmp/f_lost_once"
printf 'TIMEOUT after 8000ms, Now: x\n' > "$tmp/f_lost_always"
printf 'TIMEOUT after 8000ms, the app had no window in front\n' > "$tmp/f_nowin"
printf 'frames=1 screen=0 input_seen=7\nframes=1 screen=0 input_seen=7\nframes=2 screen=3 input_seen=7\n' > "$tmp/f_ready_changed"
printf 'ERR the app has no window in front\n' > "$tmp/f_ready_nowin"
printf 'no answer from the app (request 1)\n' > "$tmp/f_noanswer"

run_case ok finger_ok AWAIT_FILE="$tmp/f_ok"
verdict "a touch the app counted is sent once, and the wait is for input_seen above the count before" \
    "$([ "$(taps ok)" = 1 ] && has ok 'await:8000:input_seen>7' && has ok 'rc=0' && echo 1)"

run_case lost1 finger_ok AWAIT_FILE="$tmp/f_lost_once"
verdict "a touch that was not counted, with the screen as before, is sent again once, and noted" \
    "$([ "$(taps lost1)" = 2 ] && has lost1 'rc=0' && grep -q 'touch lost by Android' "$tmp/lost1/out/results.txt" && echo 1)"

run_case lost3 finger_ok AWAIT_FILE="$tmp/f_lost_always"
verdict "a touch that never arrives is sent 6 times in all, then fails with a line that says so" \
    "$([ "$(taps lost3)" = 6 ] && has lost3 'rc=1' && grep -q 'no input reached the app in 6 touches' "$tmp/lost3/stdout" && echo 1)"

run_case changed finger_ok AWAIT_FILE="$tmp/f_lost_always" READY_FILE="$tmp/f_ready_changed"
verdict "no count but the screen changed: not touched again" \
    "$([ "$(taps changed)" = 1 ] && has changed 'rc=0' && echo 1)"

printf 'frames=1 screen=0 toast=false input_seen=7
frames=1 screen=0 toast=false input_seen=7
frames=2 screen=0 toast=true input_seen=7
' > "$tmp/f_ready_toast"
run_case toast finger_ok AWAIT_FILE="$tmp/f_lost_always" READY_FILE="$tmp/f_ready_toast"
verdict "a toast that comes or goes is not a change of the screen: the touch is still sent again"     "$([ "$(taps toast)" = 6 ] && has toast 'rc=1' && echo 1)"

run_case ack finger_ok AWAIT_FILE="$tmp/f_ok" FINGER_ACK_MS=1500
verdict "FINGER_ACK_MS sets how long the count is waited for"     "$(has ack 'await:1500:input_seen>7' && echo 1)"

run_case nowin finger_ok AWAIT_FILE="$tmp/f_nowin"
verdict "the app left the front after the touch: not touched again" \
    "$([ "$(taps nowin)" = 1 ] && has nowin 'rc=0' && echo 1)"

run_case noanswer finger_ok AWAIT_FILE="$tmp/f_noanswer"
verdict "no answer from the app is a failure, with no second touch" \
    "$([ "$(taps noanswer)" = 1 ] && has noanswer 'rc=1' && echo 1)"

run_case nocount finger_ok AWAIT_FILE="$tmp/f_ok" READY_FILE="$tmp/f_ready_nowin"
verdict "where the app cannot say (another screen in front): one touch as before, no await" \
    "$([ "$(taps nocount)" = 1 ] && ! has nocount 'await:' && has nocount 'rc=0' && echo 1)"

run_case swipe finger_swipe AWAIT_FILE="$tmp/f_lost_once"
verdict "a swipe goes through the same acknowledgement" \
    "$([ "$(taps swipe)" = 2 ] && has swipe 'dev input swipe 1 2 3 4 300' && echo 1)"

run_case tap tap AWAIT_FILE="$tmp/f_ok"
verdict "tap sends no touch from outside, only tap@2:Hello to the app, held there for 5 s, after the window is stable" \
    "$([ "$(taps tap)" = 0 ] && has tap 'await_ui 5 stable' && has tap 'ui tapwait:5000:tap@2:Hello' && has tap 'rc=0' && echo 1)"

run_case taph tap_has AWAIT_FILE="$tmp/f_ok"
verdict "tap_has sends tapc@L:Hello to the app" \
    "$([ "$(taps taph)" = 0 ] && has taph 'ui tapwait:5000:tapc@L:Hello' && echo 1)"

run_case tapx tap_xy AWAIT_FILE="$tmp/f_ok"
verdict "tap_xy sends tapxy:10 20 to the app" \
    "$([ "$(taps tapx)" = 0 ] && has tapx 'ui tapxy:10 20' && ! has tapx 'tapwait' && echo 1)"

run_case tapnone tap AWAIT_FILE="$tmp/f_ok" TAP_ANSWER=none
verdict "tap fails when the app says the element is not there, and says what the app said" \
    "$(has tapnone 'rc=1' && grep -q 'the app said: none' "$tmp/tapnone/stdout" && echo 1)"

run_case tabok go_tab AWAIT_FILE="$tmp/f_ok"
verdict "go_tab taps once through the app and waits for the screen it names" \
    "$([ "$(taps tabok)" = 0 ] && has tabok 'ui tapwait:5000:tap@L:Settings' && has tabok 'await_ui 8 screen=3' && has tabok 'rc=0' && echo 1)"

run_case tabbad go_tab AWAIT_FILE="$tmp/f_ok" AWAIT_UI_FAIL=1 SCREEN_NOW=0
verdict "go_tab fails, and says which screen the app shows, when the screen is not reached" \
    "$(has tabbad 'rc=1' && grep -q 'the app shows screen 0, not 3' "$tmp/tabbad/stdout" && echo 1)"

# What the app refuses because a finger could not do it (under the keyboard, outside the window) fails the tap with the app's words;
# once the keyboard is down the element is asked for again.
run_case kbd tap AWAIT_FILE="$tmp/f_ok" TAP_ANSWER="ERR under the keyboard: 5 5000, it starts at 4800"
verdict "a tap under the keyboard waits for the keyboard to go, asks again, and fails with the app's words when it stays"     "$([ "$(grep -c '^ui tapwait:5000:tap@2:Hello' "$tmp/kbd/sent.log")" = 2 ] && has kbd 'await_ui 2 keyboard=down' && has kbd 'rc=1' && grep -q 'the app said: ERR under the keyboard' "$tmp/kbd/stdout" && echo 1)"
run_case kbd2 tap AWAIT_FILE="$tmp/f_ok" TAP_ANSWER="ERR under the keyboard: 5 5000, it starts at 4800" AWAIT_UI_FAIL=1
verdict "the keyboard that does not go is not waited for twice: one ask"     "$([ "$(grep -c '^ui tapwait:5000:tap@2:Hello' "$tmp/kbd2/sent.log")" = 1 ] && has kbd2 'rc=1' && echo 1)"
run_case outside tap_xy AWAIT_FILE="$tmp/f_ok" TAP_ANSWER="ERR outside the window (1080x2400): 5 9000"
verdict "a touch outside the window fails the step with the app's words, no waiting"     "$(has outside 'rc=1' && ! has outside 'keyboard=down' && grep -q 'the app said: ERR outside the window' "$tmp/outside/stdout" && echo 1)"

# The wait of a tap: held in the app (one request), 5 s unless told, none for TAP_WAIT=0, a NOTE when it took over a second.
run_case wait2 tap AWAIT_FILE="$tmp/f_ok" TAP_WAIT=2
verdict "TAP_WAIT sets how long the app holds the tap, and the channel waits that long on top of its deadline" \
    "$(has wait2 'ui tapwait:2000:tap@2:Hello' && [ "$(cat "$tmp/wait2/extra")" = 2 ] && echo 1)"
run_case wait0 tap AWAIT_FILE="$tmp/f_ok" TAP_WAIT=0
verdict "TAP_WAIT=0 is the plain tap, answered at once, with no wait in the app" \
    "$(has wait0 'ui tap@2:Hello' && ! has wait0 'tapwait' && has wait0 'rc=0' && echo 1)"
run_case waited tap AWAIT_FILE="$tmp/f_ok" TAP_ANSWER="OK waited 2300ms"
verdict "a tap that waited over a second is a tap that landed, and the results say how long it waited" \
    "$(has waited 'rc=0' && grep -q 'NOTE  case  the tap (tap@2:Hello) waited 2300ms for its element' "$tmp/waited/out/results.txt" && [ ! -s "$tmp/waited/out/step_failed.txt" ] && echo 1)"
run_case waitnone tap AWAIT_FILE="$tmp/f_ok" TAP_ANSWER=none TAP_WAIT=3
verdict "a tap whose element never came fails after the wait, and the line says what the app said, how long it waited and where the step is" \
    "$(has waitnone 'rc=1' && grep -q 'nothing says Hello (the app said: none, waited 3s) \[drive.sh:' "$tmp/waitnone/stdout" && grep -q 'waited 3s' "$tmp/waitnone/out/step_failed.txt" && echo 1)"
run_case nowait tap AWAIT_FILE="$tmp/f_ok" TAP_ANSWER=none TAP_WAIT=0
verdict "a tap that must not wait says no wait in its failure line" \
    "$(has nowait 'rc=1' && grep -q 'the app said: none) \[' "$tmp/nowait/stdout" && echo 1)"
run_case tol tolerated_then_pass AWAIT_FILE="$tmp/f_ok" TAP_ANSWER=none
verdict "a step that may fail is written to tolerated_steps.txt with its scenario and its line, and stays out of step_failed.txt" \
    "$(grep -q '^case | nothing says Hello (the app said: none, waited 5s) | drive.sh:' "$tmp/tol/out/tolerated_steps.txt" && [ ! -s "$tmp/tol/out/step_failed.txt" ] && echo 1)"

# A touch that arrived after the wait ran out: the count is read again, and the touch is not sent twice.
printf 'frames=1 screen=0 input_seen=7\nframes=1 screen=0 input_seen=7\nframes=1 screen=0 input_seen=8\n' > "$tmp/f_ready_late"
run_case late finger_ok AWAIT_FILE="$tmp/f_lost_always" READY_FILE="$tmp/f_ready_late"
verdict "a touch counted only after the wait ran out is not sent again, and it is no failure" \
    "$([ "$(taps late)" = 1 ] && has late 'rc=0' && ! grep -q 'touch lost' "$tmp/late/out/results.txt" && echo 1)"

# The count never moves and the app answers: the touch is still sent again (the late-count read must not hide a lost touch).
printf 'frames=1 screen=0 input_seen=7
' > "$tmp/f_ready_same"
run_case notlate finger_ok AWAIT_FILE="$tmp/f_lost_always" READY_FILE="$tmp/f_ready_same"
verdict "a count that stays where it was does not stop the resends"     "$([ "$(taps notlate)" = 6 ] && has notlate 'rc=1' && echo 1)"

# No PASS after a failed step: the results of the case say FAIL with the step in the line, and no PASS.
no_pass() { # name, what the FAIL line has to say
    grep -q "^FAIL  case  a step failed, so this is no PASS (all fine): $2" "$tmp/$1/out/results.txt"         && ! grep -q '^PASS' "$tmp/$1/out/results.txt"
}
run_case pass_tap tap_then_pass AWAIT_FILE="$tmp/f_ok" TAP_ANSWER=none
verdict "a scenario that ends with PASS after a tap the app did not find ends with FAIL, naming the tap"     "$(no_pass pass_tap 'nothing says Hello' && echo 1)"
run_case pass_tol tolerated_then_pass AWAIT_FILE="$tmp/f_ok" TAP_ANSWER=none
verdict "a step marked TAP_MAY_FAIL fails for its caller but does not stop the PASS"     "$(grep -q '^PASS  case' "$tmp/pass_tol/out/results.txt" && ! grep -q '^FAIL' "$tmp/pass_tol/out/results.txt" && echo 1)"
run_case pass_finger finger_then_pass AWAIT_FILE="$tmp/f_lost_always"
verdict "a real touch that never arrived leaves no PASS"     "$(no_pass pass_finger 'touch on Hello: no input reached the app in 6 touches' && echo 1)"
run_case pass_press press_then_pass AWAIT_FILE="$tmp/f_ok" PRESS_ANSWER="ERR unknown callback open"
verdict "a press the app does not know leaves no PASS"     "$(no_pass pass_press 'press open: ERR unknown callback' && echo 1)"
run_case pass_tab tab_then_pass AWAIT_FILE="$tmp/f_ok" AWAIT_UI_FAIL=1 SCREEN_NOW=0
verdict "a tab that did not open leaves no PASS"     "$(no_pass pass_tab 'go_tab Settings: the app shows screen 0, not 3' && echo 1)"
run_case pass_clean clean_pass AWAIT_FILE="$tmp/f_ok"
verdict "a scenario with no failed step keeps its PASS"     "$(grep -q '^PASS  case' "$tmp/pass_clean/out/results.txt" && ! grep -q '^FAIL' "$tmp/pass_clean/out/results.txt" && echo 1)"

[ "$bad" = 0 ] || { echo "FAIL  some finger cases"; exit 1; }
echo "PASS  all finger cases"
