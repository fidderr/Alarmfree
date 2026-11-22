#!/bin/bash
# The debug channel of the suite against a stand-in for the app (no device): PASS or FAIL per case.
# Usage: bash devtest/channel.test.sh
# The real `ui`, `action`, `await_ms` and `page_is_up` of lib.sh run. Only the shell on the device (as_app) is
# replaced by a shell in a temp folder, and the app by a loop that takes `files/debug_request.txt` and answers in
# `files/debug_answer_<id>.txt` after a delay that a case can set.
here=$(cd "${BASH_SOURCE[0]%/*}" && pwd)
tmp=$(mktemp -d) || exit 1
APP_PIDS=""
cleanup() {
    local p
    for p in $APP_PIDS; do kill "$p" 2> /dev/null; done
    rm -rf "$tmp"
}
trap cleanup EXIT
bad=0
verdict() { # name ok
    if [ "$2" = 1 ]; then
        echo "PASS  $1"
    else
        echo "FAIL  $1"
        bad=1
    fi
}
now_ms() { NOW=$((${EPOCHREALTIME/[.,]/} / 1000)); }

# The stand-in app. The delay (seconds, in `delay`) is read for each request; `silent` makes it take a request and never answer.
cat > "$tmp/app.sh" << 'E'
cd "$1/files" || exit 1
printf '%s
' "$$-${EPOCHREALTIME/[.,]/}" > debug_proc
while :; do
    if mv debug_request.txt debug_request.taking 2> /dev/null; then
        read -r line < debug_request.taking
        rm -f debug_request.taking
        echo "$line" >> ../requests.log
        id=${line%% *}
        cmd=${line#* }
        read -r delay < ../delay 2> /dev/null || delay=0
        if [ -e ../nowin ]; then
            case "$cmd" in
                ui:await:*) t=${cmd#ui:await:}; ans="TIMEOUT after ${t%%:*}ms, the app had no window in front" ;;
                ui:*) ans="ERR the app has no window in front" ;;
                *) ans=OK ;;
            esac
            printf '%s
' "$ans" > "debug_answer_$id.tmp"
            mv "debug_answer_$id.tmp" "debug_answer_$id.txt"
        elif [ ! -e ../silent ]; then
            (
                sleep "$delay"
                case "$cmd" in
                    ui:await:*) ans="OK 5ms" ;;
                    ui:window) ans="1080 2400 3.0" ;;
                    ui:*) ans="answer to $cmd" ;;
                    *) ans=OK ;;
                esac
                printf '%s\n' "$ans" > "debug_answer_$id.tmp"
                mv "debug_answer_$id.tmp" "debug_answer_$id.txt"
            ) &
        fi
    fi
    sleep 0.02
done
E

# new_case name [delay]: a fresh folder with the app running.
new_case() {
    DEV="$tmp/$1"
    mkdir -p "$DEV/files"
    echo "${2:-0}" > "$DEV/delay"
    start_app_standin
}
start_app_standin() {
    # A new process: what an earlier one left is removed, as the app does when it starts.
    rm -f "$DEV"/files/debug_answer_* "$DEV"/files/debug_dead "$DEV"/files/debug_request.taking
    bash "$tmp/app.sh" "$DEV" &
    APP_PID=$!
    APP_PIDS="$APP_PIDS $APP_PID"
}
stop_app_standin() { kill "$APP_PID" 2> /dev/null; wait "$APP_PID" 2> /dev/null; }
requests() { # number of requests the app took
    local n=0 l
    [ -f "$DEV/requests.log" ] && while IFS= read -r l; do n=$((n + 1)); done < "$DEV/requests.log"
    echo "$n"
}
answers_left() { # number of answer files in the app's folder
    local n=0 f
    for f in "$DEV"/files/debug_answer_*; do [ -e "$f" ] && n=$((n + 1)); done
    echo "$n"
}

export OUT="$tmp/out" TMPDIR="$tmp"
REAL_PATH=$PATH
# shellcheck disable=SC1091
. "$here/lib.sh"
as_app() { (
    PATH=$REAL_PATH
    cd "$DEV" && eval "$*"
); }
export STEP_DEADLINE=2 TAKE_DEADLINE=1

# 1. A normal exchange: the answer of the request, the file taken away, one request.
new_case normal
said=$(ui screen)
[ "$said" = "answer to ui:screen" ] && [ "$(requests)" = 1 ] && [ "$(answers_left)" = 0 ] && verdict "a normal exchange gives the answer and leaves no file" 1 || verdict "a normal exchange gives the answer and leaves no file ($said, $(requests) requests, $(answers_left) files)" 0
stop_app_standin

# 2. The answer comes after the deadline: request 1 gets none, and its late answer is never given to request 2.
new_case late 3.5
said1=$(ui snapshot)
rc1=$?
now_ms
t0=$NOW
said2=$(ui screen)
rc2=$?
now_ms
took=$((NOW - t0))
case "$said1" in "no answer from the app"*) ok1=1 ;; *) ok1=0 ;; esac
case "$said2" in "no answer from the app"*) ok2=1 ;; *) ok2=0 ;; esac
[ "$rc1" = 2 ] && [ "$ok1" = 1 ] && verdict "no answer in time: ui says so and returns 2" 1 || verdict "no answer in time: ui says so and returns 2 (rc $rc1, [$said1])" 0
[ "$rc2" = 2 ] && [ "$ok2" = 1 ] && [ "$took" -lt 800 ] && [ "$(requests)" = 1 ] && verdict "after a missing answer the next ui fails at once and writes no request" 1 || verdict "after a missing answer the next ui fails at once and writes no request (rc $rc2, ${took} ms, [$said2], $(requests) requests)" 0
sleep 3
said3=$(ui screen)
[ "$said3" != "answer to ui:snapshot" ] && [ "$(requests)" = 1 ] && verdict "the late answer to request 1 is never given to request 2" 1 || verdict "the late answer to request 1 is never given to request 2 ([$said3], $(requests) requests)" 0
action snooze:a 2> /dev/null
[ $? = 2 ] && [ "$(requests)" = 1 ] && verdict "after a missing answer an action is not sent either" 1 || verdict "after a missing answer an action is not sent either ($(requests) requests)" 0
# The app process starts again: the channel is open again.
echo 0 > "$DEV/delay"
stop_app_standin
start_app_standin
said4=$(ui screen)
[ "$said4" = "answer to ui:screen" ] && verdict "after the app was started again, ui is answered" 1 || verdict "after the app was started again, ui is answered ([$said4])" 0
stop_app_standin

# 3. An action waits for its OK.
new_case action 1
now_ms
t0=$NOW
action snooze:a
rc=$?
now_ms
took=$((NOW - t0))
last=$(tail -1 "$DEV/requests.log")
[ "$rc" = 0 ] && [ "$took" -ge 900 ] && [ "${last#* }" = "snooze:a" ] && verdict "action waits for its OK" 1 || verdict "action waits for its OK (rc $rc, ${took} ms, [$last])" 0
action dismiss:b
action hang_ui
[ "$(requests)" = 3 ] && [ "$(answers_left)" = 0 ] && verdict "every action is answered and its file taken away" 1 || verdict "every action is answered and its file taken away ($(requests) requests, $(answers_left) files)" 0
stop_app_standin

# 4. An action nobody answers fails and closes the channel.
new_case action_lost 4
touch "$DEV/silent"
action snooze:a 2> "$tmp/err"
rc=$?
case "$(< "$tmp/err")" in *"no answer from the app"*) said=1 ;; *) said=0 ;; esac
rm -f "$DEV/silent"
ui screen > /dev/null
[ "$rc" = 2 ] && [ "$said" = 1 ] && [ "$(requests)" = 1 ] && verdict "an action without an answer fails with a message and closes the channel" 1 || verdict "an action without an answer fails with a message and closes the channel (rc $rc, said $said, $(requests) requests)" 0
stop_app_standin

# 4b. The app is started again while a request it took is waited for: the old process is gone, so nothing can
# still run, and the channel is not closed for the new process.
new_case restart
touch "$DEV/silent"
(ui screen > "$tmp/restart.out"; echo $? > "$tmp/restart.rc") &
bg=$!
sleep 0.5
stop_app_standin
rm -f "$DEV/silent"
start_app_standin
wait "$bg"
said=$(< "$tmp/restart.out")
rc=$(< "$tmp/restart.rc")
[ "$rc" = 2 ] && [ ! -e "$DEV/files/debug_dead" ] && verdict "an app started again while a request was waited for leaves the channel open" 1 || verdict "an app started again while a request was waited for leaves the channel open (rc $rc, [$said], dead mark $([ -e "$DEV/files/debug_dead" ] && echo yes || echo no))" 0
said=$(ui screen)
[ "$said" = "answer to ui:screen" ] && verdict "the new process is answered" 1 || verdict "the new process is answered ([$said])" 0
stop_app_standin

# 5. No app process: the request is taken back, nothing stays behind, and a later start works.
new_case absent
stop_app_standin
said=$(ui screen)
rc=$?
left=$(ls "$DEV/files" 2> /dev/null)
case "$left" in *debug_request.txt*) pending=1 ;; *) pending=0 ;; esac
[ "$rc" = 2 ] && [ "$pending" = 0 ] && [ ! -e "$DEV/files/debug_dead" ] && verdict "without an app process the request is taken back and the channel stays open" 1 || verdict "without an app process the request is taken back and the channel stays open (rc $rc, pending $pending, [$said])" 0
page_is_up && verdict "page_is_up is false without an answer" 0 || verdict "page_is_up is false without an answer" 1
start_app_standin
said=$(ui screen)
[ "$said" = "answer to ui:screen" ] && [ "$(requests)" = 1 ] && verdict "the request that was taken back never runs, the next one does" 1 || verdict "the request that was taken back never runs, the next one does ([$said], $(requests) requests)" 0
page_is_up && verdict "page_is_up is true with an answer" 1 || verdict "page_is_up is true with an answer" 0
stop_app_standin

# 6. await: its own time on top of the deadline.
new_case await 3
UI_QUIET=1 await_ms 2000 'screen=0'
rc=$?
[ "$rc" = 0 ] && [ "$(requests)" = 1 ] && verdict "an await is waited for longer than the step deadline" 1 || verdict "an await is waited for longer than the step deadline (rc $rc)" 0
stop_app_standin

# 7. A command with an apostrophe and a space reaches the app whole.
new_case quote
said=$(ui "tap:Don't turn off")
last=$(tail -1 "$DEV/requests.log")
[ "${last#* }" = "ui:tap:Don't turn off" ] && [ "$said" = "answer to ui:tap:Don't turn off" ] && verdict "a request with an apostrophe arrives whole" 1 || verdict "a request with an apostrophe arrives whole ([$last], [$said])" 0
stop_app_standin

# 8. A normal exchange starts no program on the host (everything is a builtin; only the stand-in device uses programs).
new_case programs
errors=$(
    PATH=/nonexistent
    ui screen > "$tmp/programs.out"
    action snooze:a
    UI_QUIET=1 await_ms 1000 'screen=0'
) 2>&1
said=$(< "$tmp/programs.out")
[ -z "$errors" ] && [ "$said" = "answer to ui:screen" ] && verdict "a normal exchange starts no program on the host" 1 || verdict "a normal exchange starts no program on the host ([$errors], [$said])" 0
stop_app_standin

# 9. The sabotage hook stays in ui.
new_case chaos
chaos_calls=0
chaos_step() { chaos_calls=$((chaos_calls + 1)); }
CHAOS_ON=1 ui screen > /dev/null
ui screen > /dev/null
[ "$chaos_calls" = 1 ] && verdict "ui calls the sabotage hook, only in sabotage mode" 1 || verdict "ui calls the sabotage hook, only in sabotage mode ($chaos_calls)" 0
stop_app_standin

# 10. The app has no window in front: it answers at once, the channel stays open, and the helpers say it is no hang.
new_case nowin
touch "$DEV/nowin"
said=$(ui screen)
rc=$?
[ "$rc" = 0 ] && [ "$said" = "ERR the app has no window in front" ] && [ ! -e "$DEV/files/debug_dead" ] && verdict "a command while the app has no window in front is answered with the error and no channel is closed" 1 || verdict "a command while the app has no window in front is answered with the error and no channel is closed (rc $rc, [$said])" 0
page_is_up && verdict "page_is_up is false while the app has no window in front" 0 || verdict "page_is_up is false while the app has no window in front" 1
err=$(await_ms 3000 '~moved' 2>&1 > /dev/null)
rc=$?
case "$err" in *"WAIT "*) wait_line=1 ;; *) wait_line=0 ;; esac
case "$err" in *"the app had no window in front"*) said_it=1 ;; *) said_it=0 ;; esac
[ "$rc" = 1 ] && [ "$wait_line" = 0 ] && [ "$said_it" = 1 ] && [ ! -e "$DEV/files/debug_dead" ] && verdict "an await while the app has no window in front returns 1 and prints no WAIT line" 1 || verdict "an await while the app has no window in front returns 1 and prints no WAIT line (rc $rc, [$err])" 0
rm -f "$DEV/nowin"
said=$(ui screen)
[ "$said" = "answer to ui:screen" ] && verdict "after the window is back the next command is answered" 1 || verdict "after the window is back the next command is answered ([$said])" 0
stop_app_standin

exit "$bad"
