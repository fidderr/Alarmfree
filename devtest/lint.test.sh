#!/bin/bash
# The lint's own cases: PASS or FAIL for each. Usage: bash devtest/lint.test.sh
# Each case writes a small scenario file and reads what lint.sh says about it.
here=${BASH_SOURCE[0]%/*}
[ "$here" != "${BASH_SOURCE[0]}" ] || here=.
lint="$here/lint.sh"
dir=$(mktemp -d) || exit 1
trap 'rm -rf "$dir"' EXIT
bad=0

# fixture name line...: the file $dir/name.sh with these lines.
fixture() {
    local name=$1
    shift
    printf '%s\n' "$@" > "$dir/$name.sh"
}
verdict() { # name ok
    if [ "$2" = 1 ]; then
        echo "PASS  $1"
    else
        echo "FAIL  $1"
        bad=1
    fi
}
# found name rule: the lint lists a hit of this rule.
found() {
    local out
    out=$(bash "$lint" --list --report "$dir/$1.sh")
    case "$out" in *"$1.sh:"*": $2: "*) return 0 ;; esac
    return 1
}
count() { # name: the number after the rule in the file's row
    bash "$lint" --report "$dir/$1.sh" | sed -n "s/\$/ /; s/^[^ ]*$1.sh .*$2 *\([0-9]*\) .*/\1/p" | head -1
}

fixture sleep_hit 'scenario s99' '    sleep 2'
found sleep_hit sleep && verdict "a sleep is found" 1 || verdict "a sleep is found" 0

fixture sleep_device 'scenario s99' '    dev "sleep 1; echo hi"'
found sleep_device sleep && verdict "a sleep inside a quoted device command is found" 1 || verdict "a sleep inside a quoted device command is found" 0

fixture sleep_comment 'scenario s99' '    # sleep 2 would be too short' '    echo ok  # sleep 3' '    pass "asleep, not a sleep"'
[ "$(count sleep_comment sleep)" = 0 ] && verdict "the word in a comment is not found" 1 || verdict "the word in a comment is not found" 0

fixture raw 'scenario s99' '    dev input tap 10 20'
found raw raw-input && verdict "a raw input is found" 1 || verdict "a raw input is found" 0

fixture dump 'scenario s99' '    dev uiautomator dump /sdcard/x.xml'
found dump screen-dump && verdict "a screen dump is found" 1 || verdict "a screen dump is found" 0

fixture again 'scenario s99' '    tap_again'
found again blind-repeat && verdict "tap_again is found" 1 || verdict "tap_again is found" 0

fixture retry 'scenario s99' '    for _ in 1 2 3; do' '        tap "Turn off"' '        on_screen SCAN 8 && break' '    done'
found retry blind-repeat && verdict "a hand-made retry is found" 1 || verdict "a hand-made retry is found" 0

fixture steps 'scenario s99' '    for k in 1 2 3 4; do' '        sheet_nudge' '    done'
[ "$(count steps blind-repeat)" = 0 ] && verdict "a loop of steps with no early exit is not a retry" 1 || verdict "a loop of steps with no early exit is not a retry" 0

fixture comment0 'scenario s99' '# a `sleep 2` or `input tap` or uiautomator named in a comment at column 0' '#sleep 3'
[ "$(count comment0 sleep)" = 0 ] && [ "$(count comment0 raw-input)" = 0 ] && [ "$(count comment0 screen-dump)" = 0 ] && verdict "a comment at the start of the line is not found" 1 || verdict "a comment at the start of the line is not found" 0

fixture whileretry 'scenario s99' '    i=0' '    while [ "$i" -lt 3 ]; do' '        tap "Create new alarm"' '        on_editor 2 && return 0' '        i=$((i + 1))' '    done'
found whileretry blind-repeat && verdict "a retry by a counter is found" 1 || verdict "a retry by a counter is found" 0

fixture whilepoll 'scenario s99' '    i=0' '    while [ "$i" -lt 3 ]; do' '        echo waiting' '        i=$((i + 1))' '    done'
[ "$(count whilepoll blind-repeat)" = 0 ] && verdict "a counter loop that does no action is not a retry" 1 || verdict "a counter loop that does no action is not a retry" 0

fixture quotedsleep 'scenario s99' '    x=$(dev "true && sleep 1")' '    r=$( sleep 2 )'
[ "$(count quotedsleep sleep)" = 2 ] && verdict "a sleep after && and inside \$( ) is found" 1 || verdict "a sleep after && and inside \$( ) is found" 0

fixture noword 'scenario s99' '    echo "a sleep_mode, wake_sleep and sleeping_time"' '    pass "the screen went to sleep"' '    sleepy() { :; }' '    tap_input_text'
[ "$(count noword sleep)" = 0 ] && [ "$(count noword raw-input)" = 0 ] && verdict "words that only contain a rule word are not found" 1 || verdict "words that only contain a rule word are not found" 0

fixture ok 'scenario s99' '    sleep 2  # lint-ok: the clock needs this second'
out=$(bash "$lint" --list "$dir/ok.sh")
rc=$?
[ "$rc" = 0 ] && [ "$(count ok sleep)" = 0 ] && [ "$(count ok allowed)" = 1 ] && [[ $out != *"ok.sh:"*": sleep:"* ]] && verdict "a lint-ok line counts as allowed" 1 || verdict "a lint-ok line counts as allowed" 0

fixture clean 'scenario s99' '    await_screen home' '    tap_in "Save"'
bash "$lint" "$dir/clean.sh" > /dev/null
[ $? = 0 ] && verdict "a clean file gives exit 0" 1 || verdict "a clean file gives exit 0" 0

bash "$lint" "$dir/sleep_hit.sh" > /dev/null
rc_plain=$?
bash "$lint" --report "$dir/sleep_hit.sh" > /dev/null
rc_report=$?
[ "$rc_plain" = 1 ] && verdict "a dirty file gives exit 1" 1 || verdict "a dirty file gives exit 1" 0
[ "$rc_report" = 0 ] && verdict "--report gives exit 0 on a dirty file" 1 || verdict "--report gives exit 0 on a dirty file" 0

# The rule against a fixed sleep right before a read (sleep-check); its list only shrinks.
fixture sc_next 'scenario s99' '    tap "Done"' '    sleep 1' '    on_screen_has "Saved" 2 || problems="$problems; not saved: $(texts_of)"'
found sc_next sleep-check && verdict "a sleep right before a read of the screen is found (sleep-check)" 1 || verdict "a sleep right before a read of the screen is found (sleep-check)" 0
fixture sc_same 'scenario s99' '    sleep 1; case "$(texts_of)" in *Saved*) ;; *) fail x ;; esac'
found sc_same sleep-check && verdict "a sleep and a read on one line is found" 1 || verdict "a sleep and a read on one line is found" 0
fixture sc_case 'scenario s99' '    sleep 0.5' '    case "$(sn ringing)" in true) ;; esac'
found sc_case sleep-check && verdict "a sleep before a case on the app's state is found" 1 || verdict "a sleep before a case on the app's state is found" 0
fixture sc_none 'scenario s99' '    sleep 1' '    tap "Done"' '    on_screen_has "Saved" 5'
[ "$(count sc_none sleep-check)" = 0 ] && verdict "a sleep before a tap is not a sleep-check (the wait after it is a wait)" 1 || verdict "a sleep before a tap is not a sleep-check" 0
fixture sc_verdict 'scenario s99' '    sleep 1' '    if [ -z "$problems" ]; then' '        pass ok' '    fi'
[ "$(count sc_verdict sleep-check)" = 0 ] && verdict "a sleep before the verdict of the scenario is not a sleep-check" 1 || verdict "a sleep before the verdict of the scenario is not a sleep-check" 0
fixture sc_ok 'scenario s99' '    sleep 1  # lint-ok: the clock needs this second' '    case "$(sn ringing)" in true) ;; esac'
[ "$(count sc_ok sleep-check)" = 0 ] && verdict "a lint-ok sleep is not a sleep-check" 1 || verdict "a lint-ok sleep is not a sleep-check" 0
out=$(bash "$lint" "$dir/sc_next.sh")
rc=$?
[ "$rc" = 1 ] && [[ $out == *"FAIL  1 sleep(s) right before a read that are not on the list"* ]] && verdict "a sleep-check that is not on the list fails the lint" 1 || verdict "a sleep-check that is not on the list fails the lint" 0
printf '%s
' 'sc_next.sh::on_screen_has "Saved" 2 || problems="$problems; not saved: $(texts_of)"' > "$dir/list.txt"
out=$(LINT_LIST="$dir/list.txt" bash "$lint" "$dir/sc_next.sh")
[[ $out == *"sleep-check   0 (listed 1)"* ]] && [[ $out != *"not on the list"* ]] && verdict "a hit that is on the list is listed, not new" 1 || verdict "a hit that is on the list is listed, not new" 0
printf '%s
' 'matrix.sh::this line is gone' > "$dir/stale.txt"
out=$(LINT_LIST="$dir/stale.txt" bash "$lint" --report)
[[ $out == *"STALE"*"this line is gone"* ]] && [[ $out == *"the list only shrinks"* ]] && verdict "an entry of the list that no longer hits is stale (the list only shrinks)" 1 || verdict "an entry of the list that no longer hits is stale" 0
out=$(bash "$lint" --report)
[[ $out != *STALE* ]] && [[ $out != *"not on the list"* ]] && verdict "the list of the repository matches the code (no new hit, no stale entry)" 1 || verdict "the list of the repository matches the code" 0

# A script with a CR at the end of its lines (CRLF) cannot run on the Linux machines (run 45280008934: every part stopped at the first line of matrix.sh).
crs=""
for f in "$here"/*.sh "$here"/ci/*.sh "$here"/ci/*.py; do
    [ -f "$f" ] || continue
    [ "$(tr -cd '\r' < "$f" | wc -c)" -le 5 ] || crs="$crs ${f##*/}" # a few CR inside a quoted word are fine, a CRLF file has one per line
done
[ -z "$crs" ] && verdict "no script or tool of devtest has CRLF line ends (it would not run on a Linux machine)" 1 || verdict "no script or tool of devtest has CRLF line ends (found in:$crs)" 0

exit "$bad"
