#!/bin/bash
# hide_keyboard of lib.sh against a stubbed keyboard (no adb): how many BACKs are sent, and when. A BACK that goes to the app and
# not to the keyboard closes the sheet the app shows. PASS or FAIL per case. Usage: bash devtest/hidekeyboard.test.sh
here=$(cd "${BASH_SOURCE[0]%/*}" && pwd)
tmp=$(mktemp -d) || exit 1
trap 'rm -rf "$tmp"' EXIT
bad=0
verdict() { # name ok detail
    if [ "$2" = 1 ]; then echo "PASS  $1"; else echo "FAIL  $1  $3"; bad=1; fi
}
export OUT="$tmp/out" TMPDIR="$tmp"
# shellcheck disable=SC1091
. "$here/lib.sh"
RESULTS="$tmp/results.txt"

# The keyboard: up until GOES_AT (microseconds of EPOCHREALTIME) once the first BACK was sent, or never down (STUCK=1).
now_us() { local t=${EPOCHREALTIME/[.,]/}; echo "$t"; }
dev() {
    [ "$*" = "input keyevent KEYCODE_BACK" ] || return 0
    echo "$(now_us) up=$(keyboard_up && echo 1 || echo 0)" >> "$tmp/backs"
    [ -e "$tmp/first" ] || now_us > "$tmp/first"
}
after_input() { :; }
keyboard_up() {
    [ -n "${STUCK:-}" ] && return 0
    [ -e "$tmp/first" ] || return 0
    [ "$(now_us)" -lt $(($(cat "$tmp/first") + ${GOES_AFTER_MS:-0} * 1000)) ]
}

run_case() { # name goes-after-ms stuck
    rm -f "$tmp/backs" "$tmp/first"
    GOES_AFTER_MS=$2 STUCK=$3 hide_keyboard > /dev/null 2>&1
    echo "$(grep -c . "$tmp/backs" 2> /dev/null || echo 0)"
}

n=$(run_case quick 300 "")
verdict "a keyboard that goes within 0.3 s: one BACK" "$([ "$n" = 1 ] && echo 1)" "BACKs: $n"
n=$(run_case slow 4000 "")
verdict "a keyboard that needs 4 s to go: still one BACK (a second one would reach the app after the keyboard is gone)" "$([ "$n" = 1 ] && echo 1)" "BACKs: $n"
rm -f "$tmp/backs" "$tmp/first"
touch "$tmp/first"
echo 0 > "$tmp/first"
GOES_AFTER_MS=0 hide_keyboard > /dev/null 2>&1
verdict "no keyboard up: no BACK at all" "$([ ! -s "$tmp/backs" ] && echo 1)" "a BACK was sent"
# A keyboard that never goes: BACK again only when it is read as up, at most three times, and the failure says so.
n=$(run_case stuck 0 1)
verdict "a keyboard that stays up: three BACKs at most" "$([ "$n" = 3 ] && echo 1)" "BACKs: $n"
# Every BACK was sent with the keyboard read as up at that moment.
verdict "each BACK came with the keyboard up at that moment" "$(! grep -q 'up=0' "$tmp/backs" && echo 1)" "$(cat "$tmp/backs")"
exit $bad
