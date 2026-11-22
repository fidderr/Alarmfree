#!/bin/bash
# Sabotage mode against a stubbed device (no adb): PASS or FAIL for each case. Usage: bash devtest/chaos.test.sh
# The real `dev` and `ui` of lib.sh run; only the shell on the device (ask) is replaced by a file that records what
# would have been sent.
here=$(cd "${BASH_SOURCE[0]%/*}" && pwd)
tmp=$(mktemp -d) || exit 1
trap 'rm -rf "$tmp"' EXIT
bad=0

cat > "$tmp/drive.sh" << 'E'
# Env: LIB DIR MODE. Sourced lib.sh reads CHAOS, CHAOS_SEED and the others from the environment.
export OUT="$DIR/out" TMPDIR="$DIR"
. "$LIB"
SENT="$DIR/sent.log"
: > "$SENT"
ask() { printf '%s\n' "$4" >> "$SENT"; }
as_app() { :; }
chaos_wait() { echo "$1" >> "$DIR/waits"; }
DEV_PID=$$
case "$MODE" in
    touches) for ((i = 1; i <= 200; i++)); do dev input tap "$i" 5; done ;;
    subshell) for ((i = 1; i <= 200; i++)); do x=$(dev input tap "$i" 5); done ;;
    swipes) for ((i = 1; i <= 50; i++)); do dev input swipe "$i" 5 1 1 300; dev input touchscreen tap 1 1; dev input draganddrop 1 1 2 2; done ;;
    keys) for ((i = 1; i <= 100; i++)); do dev input keyevent 66; dev input text abc; done ;;
    steps) for ((i = 1; i <= 20; i++)); do ui ready > /dev/null; done ;;
esac
chaos_footer > "$DIR/footer"
E

# run_case name mode [VAR=value...]: a fresh folder, one run.
run_case() {
    local name=$1 mode=$2
    shift 2
    mkdir -p "$tmp/$name"
    env -u CHAOS -u CHAOS_SEED -u CHAOS_RUN -u CHAOS_DROP -u CHAOS_TOUCH_MS -u CHAOS_STEP_MS -u CHAOS_HOG \
        LIB="$here/lib.sh" DIR="$tmp/$name" MODE="$mode" "$@" bash "$tmp/drive.sh" > "$tmp/$name/stdout" 2>&1
}
verdict() { # name ok
    if [ "$2" = 1 ]; then
        echo "PASS  $1"
    else
        echo "FAIL  $1"
        bad=1
    fi
}
lines() { # file: number of lines, 0 when there is none
    local n=0 l
    [ -f "$1" ] || { echo 0; return; }
    while IFS= read -r l || [ -n "$l" ]; do n=$((n + 1)); done < "$1"
    echo "$n"
}
touches_sent() { local n=0 l; while IFS= read -r l; do case "$l" in "input tap"* | "input swipe"* | "input touchscreen"* | "input draganddrop"*) n=$((n + 1)) ;; esac; done < "$tmp/$1/sent.log"; echo "$n"; }

fast="CHAOS_TOUCH_MS=5 CHAOS_STEP_MS=5"

# 1. Off: everything is sent, nothing waits, nothing is written.
run_case off touches
sent=$(touches_sent off)
[ "$sent" = 200 ] && [ ! -e "$tmp/off/waits" ] && [ ! -e "$tmp/off/out/chaos.txt" ] && [ ! -e "$tmp/off/out/chaos.state" ] && [ ! -s "$tmp/off/footer" ] && verdict "sabotage off sends 200 of 200 touches, adds no wait and writes no chaos.txt" 1 || verdict "sabotage off sends 200 of 200 touches, adds no wait and writes no chaos.txt ($sent sent)" 0
run_case off_steps steps
[ ! -e "$tmp/off_steps/waits" ] && verdict "sabotage off adds no wait before ui commands" 1 || verdict "sabotage off adds no wait before ui commands" 0

# 2. On with seed 7: about 30% dropped, one line per drop.
# shellcheck disable=SC2086
run_case seed7 touches CHAOS=1 CHAOS_SEED=7 $fast
sent=$(touches_sent seed7)
drops=$(lines "$tmp/seed7/out/chaos.txt")
[ "$drops" -ge 40 ] && [ "$drops" -le 80 ] && [ $((sent + drops)) = 200 ] && verdict "CHAOS=1 CHAOS_SEED=7 leaves 40 to 80 of 200 unsent, one line per drop ($drops dropped, $sent sent)" 1 || verdict "CHAOS=1 CHAOS_SEED=7 leaves 40 to 80 of 200 unsent, one line per drop ($drops dropped, $sent sent)" 0
read -r first < "$tmp/seed7/out/chaos.txt"
case "$first" in *"| input tap "*) verdict "a dropped touch is written with its command" 1 ;; *) verdict "a dropped touch is written with its command ($first)" 0 ;; esac
footer=$(cat "$tmp/seed7/footer")
# No busy loop was stopped in this run (nothing asked the device), which the footer says with a question mark.
[ "$footer" = "CHAOS  sabotage on, seed 7, touches sent $sent, touches dropped $drops, busy loops alive at the end ? of 2" ] && verdict "the footer says seed, touches sent and dropped" 1 || verdict "the footer says seed, touches sent and dropped ($footer)" 0

# 3. The same seed drops the same touches, another seed does not.
# shellcheck disable=SC2086
run_case seed7b touches CHAOS=1 CHAOS_SEED=7 $fast
# shellcheck disable=SC2086
run_case seed8 touches CHAOS=1 CHAOS_SEED=8 $fast
cmp -s "$tmp/seed7/out/chaos.txt" "$tmp/seed7b/out/chaos.txt" && verdict "the same seed twice drops the same touches" 1 || verdict "the same seed twice drops the same touches" 0
cmp -s "$tmp/seed7/out/chaos.txt" "$tmp/seed8/out/chaos.txt" && verdict "another seed drops other touches" 0 || verdict "another seed drops other touches" 1

# 4. Keys and text are never dropped, even when every touch is.
# shellcheck disable=SC2086
run_case keys keys CHAOS=1 CHAOS_SEED=7 CHAOS_DROP=100 $fast
[ "$(lines "$tmp/keys/sent.log")" = 200 ] && [ "$(lines "$tmp/keys/out/chaos.txt")" = 0 ] && verdict "key events and typed text are always sent" 1 || verdict "key events and typed text are always sent" 0
# shellcheck disable=SC2086
run_case swipes swipes CHAOS=1 CHAOS_SEED=7 CHAOS_DROP=100 $fast
[ "$(touches_sent swipes)" = 0 ] && [ "$(lines "$tmp/swipes/out/chaos.txt")" = 150 ] && verdict "swipe, touchscreen and draganddrop are touches too" 1 || verdict "swipe, touchscreen and draganddrop are touches too" 0

# 5. Inside $( ): the generator and the counts survive the subshell.
# shellcheck disable=SC2086
run_case sub subshell CHAOS=1 CHAOS_SEED=7 $fast
cmp -s "$tmp/seed7/out/chaos.txt" "$tmp/sub/out/chaos.txt" && [ "$(touches_sent sub)" = "$(touches_sent seed7)" ] && verdict "it works inside \$( ): the same decisions as outside" 1 || verdict "it works inside \$( ): the same decisions as outside" 0

# 6. Steps are delayed (the wait is asked for once per ui command), within the limit.
# shellcheck disable=SC2086
run_case steps steps CHAOS=1 CHAOS_SEED=7 CHAOS_TOUCH_MS=5 CHAOS_STEP_MS=300
w=$(lines "$tmp/steps/waits")
over=0
while IFS= read -r ms; do [ "$ms" -le 300 ] || over=1; done < "$tmp/steps/waits"
[ "$w" -ge 10 ] && [ "$over" = 0 ] && verdict "every ui command waits 0 to CHAOS_STEP_MS first" 1 || verdict "every ui command waits 0 to CHAOS_STEP_MS first ($w waits)" 0

# 7. The real wait (no stub) takes about as long as asked and starts no program.
(
    export OUT="$tmp/w/out" TMPDIR="$tmp" CHAOS=1 CHAOS_SEED=3
    mkdir -p "$OUT"
    . "$here/lib.sh"
    sleep() { echo "sleep called" > "$tmp/w/slept"; }
    t0=$(date +%s%N)
    chaos_wait 400
    t1=$(date +%s%N)
    echo $(((t1 - t0) / 1000000)) > "$tmp/w/ms"
) > /dev/null 2>&1
ms=$(cat "$tmp/w/ms" 2> /dev/null)
[ "${ms:-0}" -ge 350 ] && [ "${ms:-0}" -le 1500 ] && [ ! -e "$tmp/w/slept" ] && verdict "the wait is a read that times out, not a sleep (${ms} ms)" 1 || verdict "the wait is a read that times out, not a sleep (${ms:-none} ms)" 0

exit "$bad"
