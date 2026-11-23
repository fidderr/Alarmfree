#!/bin/bash
# What a scenario may not do (plan alarmfree-devtest-no-flake, rules 1 to 4), counted per file and rule:
#   sleep         a sleep command, also inside a quoted device command
#   raw-input     input tap|swipe|text|keyevent|touchscreen|draganddrop|motionevent given to the device directly
#   screen-dump   uiautomator
#   sleep-check   a fixed sleep directly before a line that reads the screen or the app (texts_of, sn, ui, state, is_there,
#                 case, if, [ ]): the read has to wait for what it expects (wait_for, on_screen, await_*), not for a time. The hits
#                 that were there when the rule came are in devtest/lint_sleep_check.txt (basename::the line after the sleep, one
#                 per hit). The list only shrinks: a hit that is not on it fails, and an entry that no longer hits fails too (remove it).
#   blind-repeat  tap_again, the suite's ask-again helper (retry_used), a try counter, an `again` helper, and a loop over
#                 a few numbers or by a counter (while [ "$i" -lt 3 ]) that does an action and leaves at the first success
# A comment is not code. A line that ends in `# lint-ok: <reason>` is allowed and counted apart.
#
# Usage: bash devtest/lint.sh [--report] [--list] [files...]
#   --list    every hit as file:line: rule: the line      --report    the exit code is always 0
# Default files: the devtest files that hold scenarios (a line that starts a `scenario`). lib.sh, the helpers, is shown
# apart and does not count for the exit code. Exit 1 when a count is above zero.

here=${BASH_SOURCE[0]%/*}
[ "$here" != "${BASH_SOURCE[0]}" ] || here=.
LINT_LIST="${LINT_LIST:-$here/lint_sleep_check.txt}"
report="" list="" files=() explicit=""
for arg in "$@"; do
    case "$arg" in
        --report) report=1 ;;
        --list) list=1 ;;
        -*)
            echo "usage: lint.sh [--report] [--list] [files...]" >&2
            exit 2
            ;;
        *)
            files+=("$arg")
            explicit=1
            ;;
    esac
done
# The sleep-check hits that are allowed (one entry per hit), and which of them were found again.
ALLOWED_SC=() USED_SC=()
if [ -r "$LINT_LIST" ]; then
    while IFS= read -r l || [ -n "$l" ]; do
        case "$l" in '' | '#'*) ;; *) ALLOWED_SC+=("$l") USED_SC+=(0) ;; esac
    done < "$LINT_LIST"
fi

if [ ${#files[@]} -eq 0 ]; then
    for f in "$here"/*.sh; do
        # probe.sh: measuring scripts for Android's own screens, by name only, in no run of the suite.
        case "${f##*/}" in lib.sh | lint.sh | chaos.sh | probe.sh | *.test.sh) continue ;; esac
        while IFS= read -r line || [ -n "$line" ]; do
            if [[ $line =~ ^[[:space:]]*scenario[[:space:]] ]]; then
                files+=("$f")
                break
            fi
        done < "$f"
    done
    [ -f "$here/lib.sh" ] && files+=("$here/lib.sh")
fi

re_sleep='(^|[^[:alnum:]_./-])sleep[[:space:]]+[0-9$"'"'"']'
re_raw='(^|[^[:alnum:]_])input[[:space:]]+(tap|swipe|text|keyevent|touchscreen|draganddrop|motionevent)([^[:alnum:]_]|$)'
re_dump='uiautomator'
# A line that reads the screen or the app's state.
re_check='texts_of|(^|[^[:alnum:]_])(sn|ui|state|is_there|on_screen[[:alnum:]_]*|top_activity|own_notifications)[[:space:]]|^[[:space:]]*(case|if|\[\[?|!)[[:space:]]'
# The verdict of a scenario (if [ -z "$problems" ]) reads no screen.
re_verdict='^[[:space:]]*if[[:space:]]+\[[[:space:]]+-[zn][[:space:]]+"\$\{?(problems|why|left)\}?"'
re_sleep_inline='^[[:space:]]*sleep[[:space:]]+[0-9$"'"'"'][^;&|]*(;|&&)[[:space:]]*(.*)$'
re_sleep_line='^[[:space:]]*sleep[[:space:]]+[0-9$"'"'"'][^;&|]*$'
re_blind='(^|[^[:alnum:]_])(tap_again|retry_used)([^[:alnum:]_]|$)|(^|[^[:alnum:]_])try=|(^|[^[:alnum:]_])again[[:space:]]*\(\)|^[[:space:]]*again[[:space:]]*(;|$)'
re_count_loop='^[[:space:]]*for[[:space:]]+[[:alnum:]_]+[[:space:]]+in([[:space:]]+[0-9]+)+[[:space:]]*;[[:space:]]*do([[:space:]]|$)'
# The same by a counter: while [ "$i" -lt 3 ]; do
re_count_while='^[[:space:]]*while[[:space:]].*\[[[:space:]]+"?\$\{?[[:alnum:]_]+\}?"?[[:space:]]+-l[te][[:space:]]+"?[0-9]+"?[[:space:]]+\][^;]*;[[:space:]]*do([[:space:]]|$)'
re_act='(^|[^[:alnum:]_])(tap[[:alnum:]_]*|dev[[:space:]]+input|[[:alnum:]_]*scroll[[:alnum:]_]*|[[:alnum:]_]*nudge[[:alnum:]_]*|[[:alnum:]_]*swipe[[:alnum:]_]*|click[[:alnum:]_]*|press[[:alnum:]_]*)([^[:alnum:]_]|$)'
re_exit='(^|[^[:alnum:]_])(break|return[[:space:]]+0)([^[:alnum:]_]|$)'
re_open='^[[:space:]]*(for|while|until)[[:space:]].*;[[:space:]]*do([[:space:]]|$)'
re_done='^[[:space:]]*done([[:space:]]|;|$)'

# CODE: the line without its trailing comment (a " #" with an even number of double quotes before it).
strip_comment() {
    local rest=$1 pre="" q
    CODE=$1
    case "$1" in '#'*)
        CODE=""
        return
        ;;
    esac
    while [[ $rest == *" #"* ]]; do
        pre+=${rest%% \#*}
        q=${pre//[^\"]/}
        if [ $((${#q} % 2)) -eq 0 ]; then
            CODE=$pre
            return
        fi
        pre+=" #"
        rest=${rest#*" #"}
    done
}

# lint_file file: sets N_SLEEP N_RAW N_DUMP N_BLIND N_ALLOWED, prints hits with --list.
hit() { # rule
    if [ -n "$allowed" ]; then
        N_ALLOWED=$((N_ALLOWED + 1))
        return
    fi
    case "$1" in
        sleep) N_SLEEP=$((N_SLEEP + 1)) ;;
        sleep-check)
            local key k
            text=${line#"${line%%[![:space:]]*}"}
            key="${file##*/}::$text"
            for k in "${!ALLOWED_SC[@]}"; do
                if [ "${ALLOWED_SC[k]}" = "$key" ] && [ "${USED_SC[k]}" = 0 ]; then
                    USED_SC[k]=1
                    N_SCL=$((N_SCL + 1))
                    return
                fi
            done
            N_SC=$((N_SC + 1))
            ;;
        raw-input) N_RAW=$((N_RAW + 1)) ;;
        screen-dump) N_DUMP=$((N_DUMP + 1)) ;;
        blind-repeat) N_BLIND=$((N_BLIND + 1)) ;;
    esac
    if [ -n "$list" ]; then
        text=${line#"${line%%[![:space:]]*}"}
        printf '%s:%s: %s: %s\n' "$file" "$((i + 1))" "$1" "$text"
    fi
}
lint_file() {
    file=$1
    N_SLEEP=0 N_RAW=0 N_DUMP=0 N_BLIND=0 N_ALLOWED=0 N_SC=0 N_SCL=0
    local lines=() i j depth body_act body_exit after_sleep="" code
    mapfile -t lines < "$file"
    for ((i = 0; i < ${#lines[@]}; i++)); do
        line=${lines[i]%$'\r'}
        allowed=""
        case "$line" in *"# lint-ok:"*) allowed=1 ;; esac
        strip_comment "$line"
        case "$CODE" in *[![:space:]]*) ;; *) continue ;; esac
        # A read right after a fixed sleep: the line before is a sleep of its own, or the sleep is on the same line before ; or &&.
        code=$CODE
        if [ -n "$after_sleep" ] && [[ $code =~ $re_check ]] && ! [[ $code =~ $re_verdict ]]; then
            hit sleep-check
        elif [ -z "$allowed" ] && [[ $code =~ $re_sleep_inline ]]; then
            [[ ${BASH_REMATCH[2]} =~ $re_check ]] && hit sleep-check
        fi
        after_sleep=""
        [[ $code =~ $re_sleep_line ]] && [ -z "$allowed" ] && after_sleep=1
        [[ $CODE =~ $re_sleep ]] && hit sleep
        [[ $CODE =~ $re_raw ]] && hit raw-input
        [[ $CODE =~ $re_dump ]] && hit screen-dump
        if [[ $CODE =~ $re_blind ]]; then
            hit blind-repeat
        elif [[ $CODE =~ $re_count_loop || $CODE =~ $re_count_while ]]; then
            # A loop over a few numbers that acts and leaves at the first success is a retry made by hand.
            depth=0 body_act="" body_exit=""
            for ((j = i; j < ${#lines[@]}; j++)); do
                strip_comment "${lines[j]%$'\r'}"
                [[ $CODE =~ $re_open ]] && depth=$((depth + 1))
                if [ "$j" -gt "$i" ]; then
                    [[ $CODE =~ $re_act ]] && body_act=1
                    [[ $CODE =~ $re_exit ]] && body_exit=1
                fi
                if [[ $CODE =~ $re_done ]]; then
                    depth=$((depth - 1))
                    [ "$depth" -gt 0 ] || break
                fi
            done
            [ -n "$body_act" ] && [ -n "$body_exit" ] && hit blind-repeat
        fi
    done
}

row() { printf '%-26s sleep %4s  raw-input %4s  screen-dump %4s  blind-repeat %4s  allowed %4s  sleep-check %3s (listed %s)\n' "$1" "$2" "$3" "$4" "$5" "$6" "$7" "$8"; }

t_sleep=0 t_raw=0 t_dump=0 t_blind=0 t_allowed=0 t_sc=0 t_scl=0 h_row=""
for file in "${files[@]}"; do
    if [ ! -f "$file" ]; then
        echo "no such file: $file" >&2
        exit 2
    fi
    lint_file "$file"
    if [ "${file##*/}" = lib.sh ]; then
        h_row=$(row "helpers: ${file##*/}" "$N_SLEEP" "$N_RAW" "$N_DUMP" "$N_BLIND" "$N_ALLOWED" "$N_SC" "$N_SCL")
        continue
    fi
    row "$file" "$N_SLEEP" "$N_RAW" "$N_DUMP" "$N_BLIND" "$N_ALLOWED" "$N_SC" "$N_SCL"
    t_sleep=$((t_sleep + N_SLEEP)) t_raw=$((t_raw + N_RAW)) t_dump=$((t_dump + N_DUMP))
    t_blind=$((t_blind + N_BLIND)) t_allowed=$((t_allowed + N_ALLOWED)) t_sc=$((t_sc + N_SC)) t_scl=$((t_scl + N_SCL))
done
# The list only shrinks: an entry that no longer hits is a finding too (the whole default set was read, so every entry could hit).
stale=0
if [ -z "$explicit" ]; then
    for k in "${!ALLOWED_SC[@]}"; do
        [ "${USED_SC[k]}" = 1 ] && continue
        stale=$((stale + 1))
        echo "STALE  $LINT_LIST: no longer hits, remove it: ${ALLOWED_SC[k]}"
    done
fi
total=$((t_sleep + t_raw + t_dump + t_blind))
row "total (scenario files)" "$t_sleep" "$t_raw" "$t_dump" "$t_blind" "$t_allowed" "$t_sc" "$t_scl"
[ "$t_sc" -eq 0 ] || echo "FAIL  $t_sc sleep(s) right before a read that are not on the list (wait for the state instead, or give the line a lint-ok reason)"
[ "$stale" -eq 0 ] || echo "FAIL  $stale entr(ies) of the list no longer hit: the list only shrinks"
[ -z "$h_row" ] || echo "$h_row  (not counted)"
if [ "$total" -gt 0 ]; then
    echo "FAIL  $total thing(s) a scenario may not do"
else
    echo "PASS  nothing a scenario may not do"
fi
[ -n "$report" ] && exit 0
[ "$total" -eq 0 ] && [ "$t_sc" -eq 0 ] && [ "$stale" -eq 0 ]
