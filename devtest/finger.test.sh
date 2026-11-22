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
SENT="$DIR/sent.log"
: > "$SENT"
: > "$RESULTS"
n_await=0
ui() {
    echo "ui $1" >> "$SENT"
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
case "$CASE" in
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
verdict "a touch that never arrives is sent 3 times in all, then fails with a line that says so" \
    "$([ "$(taps lost3)" = 3 ] && has lost3 'rc=1' && grep -q 'no input reached the app in 3 touches' "$tmp/lost3/stdout" && echo 1)"

run_case changed finger_ok AWAIT_FILE="$tmp/f_lost_always" READY_FILE="$tmp/f_ready_changed"
verdict "no count but the screen changed: not touched again" \
    "$([ "$(taps changed)" = 1 ] && has changed 'rc=0' && echo 1)"

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
verdict "tap sends no touch from outside, only tap@2:Hello to the app" \
    "$([ "$(taps tap)" = 0 ] && has tap 'ui tap@2:Hello' && has tap 'rc=0' && echo 1)"

run_case taph tap_has AWAIT_FILE="$tmp/f_ok"
verdict "tap_has sends tapc@L:Hello to the app" \
    "$([ "$(taps taph)" = 0 ] && has taph 'ui tapc@L:Hello' && echo 1)"

run_case tapx tap_xy AWAIT_FILE="$tmp/f_ok"
verdict "tap_xy sends tapxy:10 20 to the app" \
    "$([ "$(taps tapx)" = 0 ] && has tapx 'ui tapxy:10 20' && echo 1)"

run_case tapnone tap AWAIT_FILE="$tmp/f_ok" TAP_ANSWER=none
verdict "tap fails when the app says the element is not there, and says what the app said" \
    "$(has tapnone 'rc=1' && grep -q 'the app said: none' "$tmp/tapnone/stdout" && echo 1)"

run_case tabok go_tab AWAIT_FILE="$tmp/f_ok"
verdict "go_tab taps once through the app and waits for the screen it names" \
    "$([ "$(taps tabok)" = 0 ] && has tabok 'ui tap@L:Settings' && has tabok 'await_ui 8 screen=3' && has tabok 'rc=0' && echo 1)"

run_case tabbad go_tab AWAIT_FILE="$tmp/f_ok" AWAIT_UI_FAIL=1 SCREEN_NOW=0
verdict "go_tab fails, and says which screen the app shows, when the screen is not reached" \
    "$(has tabbad 'rc=1' && grep -q 'the app shows screen 0, not 3' "$tmp/tabbad/stdout" && echo 1)"

[ "$bad" = 0 ] || { echo "FAIL  some finger cases"; exit 1; }
echo "PASS  all finger cases"
