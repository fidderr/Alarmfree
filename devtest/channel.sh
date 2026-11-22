#!/bin/bash
# The debug channel to the app (src/debug_hooks.rs): one request, answered in a file with its id. Sourced by
# lib.sh, slint_smoke.sh and slint_scan.sh; needs `as_app` (runs a command line in the app's shell). The script it
# sends has no single quotes, so an `as_app` that wraps it in single quotes works too.
#
# The host writes `files/debug_request.txt` (`<id> <command>`) and waits for `files/debug_answer_<id>.txt`, reads
# and removes it. One request at a time. The id is made up here (digits and dashes) and no request is ever sent twice.
#   No answer in STEP_DEADLINE seconds although the app took the request: it may still run it, so the app's files get
#   a mark (`debug_dead`) and nothing more is sent until the app process was started again (it removes the mark).
#   The app was started again while the request was waited for: the old process is gone, nothing can run late and
#   the channel stays open.
#   The app did not take the request within TAKE_DEADLINE seconds (no process, or one that is still starting):
#   the request is taken back, so it can never run later, and the channel stays open.
STEP_DEADLINE="${STEP_DEADLINE:-30}"
TAKE_DEADLINE="${TAKE_DEADLINE:-3}"
# The folder of the app's files as the shell on the device sees it (from the app's own folder).
CHANNEL_DIR="${CHANNEL_DIR:-files}"

# A text for inside double quotes in a shell on the device.
dq_escape() {
    local rest=$1 out="" c
    if [[ $rest != *\\* && $rest != *\"* && $rest != *\$* && $rest != *\`* ]]; then
        printf '%s' "$rest"
        return
    fi
    while [ -n "$rest" ]; do
        c=${rest:0:1}
        rest=${rest:1}
        case "$c" in '\' | '"' | '$' | '`') out+='\' ;; esac
        out+=$c
    done
    printf '%s' "$out"
}

# channel command [seconds: how long the command itself waits, on top of the deadline]
# Sets CHANNEL_OUT to the answer, or to a line that says there was none (then returns 2).
channel() {
    local id wait=$((STEP_DEADLINE + ${2:-0})) take=$TAKE_DEADLINE cmd=$1 out
    # A command that waits by itself (await) is waited for however slow the app is to take it.
    [ -z "${2:-}" ] || take=$wait
    id=$BASHPID-${EPOCHREALTIME:-$SECONDS}
    id=${id//[.,]/}
    CHANNEL_OUT=""
    # The script for the shell on the device, in parts. It has no single quotes, and what the device expands is escaped.
    # The app names its process in debug_proc: read before the request and again when no answer came.
    local get='n=; { read n < debug_proc; } 2> /dev/null'
    local send="printf \"%s\n\" \"$id $(dq_escape "$cmd")\" > debug_request.tmp && mv debug_request.tmp debug_request.txt"
    local takeback="if [ \$tk -gt 0 ] && [ \$SECONDS -ge \$tk ]; then tk=0; if mv debug_request.txt debug_request.withdrawn 2> /dev/null; then echo @@untaken; break; fi; fi"
    # Closed only for the process that took the request: when the app was started again meanwhile nothing can run late.
    # The mark is made first and the process looked at again, so a start in between cannot leave the mark behind.
    local close="[ -e \$a ] || { $get; if [ \"\$n\" = \"\$pr\" ]; then : > debug_dead; $get; [ \"\$n\" = \"\$pr\" ] && echo @@lost || { rm -f debug_dead; echo @@gone; }; else echo @@gone; fi; }"
    local giveup="if [ \$SECONDS -ge \$end ]; then $close; break; fi"
    out=$(ASK_TIMEOUT=$((wait + 30)) as_app "cd $CHANNEL_DIR; if [ -e debug_dead ]; then echo @@dead; else $get; pr=\$n; $send; a=debug_answer_$id.txt; tk=\$((SECONDS + $take + 1)); end=\$((SECONDS + $wait + 1)); while [ ! -e \$a ]; do $takeback; $giveup; sleep 0.01; done; if [ -e \$a ]; then cat \$a; rm -f \$a; fi; fi")
    case "$out" in
        @@dead*) CHANNEL_OUT="no answer from the app (not sent: an earlier request got none): $cmd" ;;
        @@untaken*) CHANNEL_OUT="no answer from the app (request $id: $cmd, the app did not take it)" ;;
        @@lost*) CHANNEL_OUT="no answer from the app (request $id: $cmd)" ;;
        @@gone*) CHANNEL_OUT="no answer from the app (request $id: $cmd, the app was started again meanwhile)" ;;
        '') CHANNEL_OUT="no answer from the app (request $id: $cmd, nothing came back from the device shell)" ;;
        *)
            CHANNEL_OUT=$out
            return 0
            ;;
    esac
    return 2
}
