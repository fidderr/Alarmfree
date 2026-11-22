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

exit "$bad"
