#!/bin/bash
# clock_clear_of_fixed_alarms and fresh_clock (lib.sh) without a device: a clock just below the hour of an alarm that scenarios seed
# at a fixed time (7:00, 8:00, 9:00, 14:05) is moved to 20:00 the same day, any other clock is left alone. Usage: bash devtest/clockaway.test.sh
here=$(cd "${BASH_SOURCE[0]%/*}" && pwd)
tmp=$(mktemp -d) || exit 1
trap 'rm -rf "$tmp"' EXIT
bad=0

cat > "$tmp/drive.sh" << 'D'
# Env: LIB DIR NOW (unix s on the device), OFF (the device's +hhmm), SERIAL. The real functions of lib.sh run; the device is replaced.
export OUT="$DIR/out" TMPDIR="$DIR"
mkdir -p "$OUT"
. "$LIB"
now_s() { echo "$NOW"; }
dev() { case "$*" in "date +%z") echo "$OFF" ;; esac; }
set_clock() { echo "$1" > "$DIR/set"; }
fresh_clock
D
# 2026-11-24 00:00:00 UTC
day=1795478400
run_case() { # name hh mm ss offset serial
    mkdir -p "$tmp/$1"
    local zone=0
    case "$5" in +0100) zone=3600 ;; -0500) zone=-18000 ;; esac
    env LIB="$here/lib.sh" DIR="$tmp/$1" NOW=$((day + $2 * 3600 + $3 * 60 + $4 - zone)) OFF="$5" SERIAL="${6:-emulator-5554}" bash "$tmp/drive.sh" > "$tmp/$1/stdout" 2> "$tmp/$1/stderr"
}
verdict() { # name ok
    if [ "$2" = 1 ]; then echo "PASS  $1"; else echo "FAIL  $1"; bad=1; fi
}
set_to() { cat "$tmp/$1/set" 2> /dev/null; }

run_case before_seven 6 59 54 +0000
verdict "06:59:54 (the run that rang the 7:00 alarm in s38) goes to 20:00 the same day" "$([ "$(set_to before_seven)" = $((day + 72000)) ] && echo 1)"
run_case after_seven 7 0 20 +0000
verdict "07:00:20 goes to 20:00 too" "$([ "$(set_to after_seven)" = $((day + 72000)) ] && echo 1)"
run_case before_two 14 4 30 +0000
verdict "14:04:30 (the 14:05 alarm of s51) goes to 20:00" "$([ "$(set_to before_two)" = $((day + 72000)) ] && echo 1)"
run_case early 6 29 59 +0000
verdict "06:29:59 is left alone" "$([ -z "$(set_to early)" ] && echo 1)"
run_case late 14 30 0 +0000
verdict "14:30:00 is left alone" "$([ -z "$(set_to late)" ] && echo 1)"
run_case night 3 10 0 +0000
verdict "03:10 is left alone" "$([ -z "$(set_to night)" ] && echo 1)"
run_case evening 20 0 0 +0000
verdict "20:00 is left alone" "$([ -z "$(set_to evening)" ] && echo 1)"
run_case zone_plus 8 15 0 +0100
verdict "08:15 in a zone one hour ahead of UTC goes to 20:00 of that zone (19:00 UTC)" "$([ "$(set_to zone_plus)" = $((day + 72000 - 3600)) ] && echo 1)"
run_case zone_minus 6 45 0 -0500
verdict "06:45 in a zone five hours behind UTC goes to 20:00 of that zone (01:00 UTC the next day)" "$([ "$(set_to zone_minus)" = $((day + 72000 + 18000)) ] && echo 1)"
run_case phone 6 59 54 +0000 R5CT123
verdict "a phone's clock is never touched" "$([ -z "$(set_to phone)" ] && echo 1)"
run_case no_zone 6 59 54 ""
verdict "a device that gives no zone is left alone" "$([ -z "$(set_to no_zone)" ] && echo 1)"

# cpu_idle_pct (lib.sh), used by settle_device: the idle share between two readings of /proc/stat.
idle_of() { (export OUT="$tmp/idle" TMPDIR="$tmp"; mkdir -p "$OUT"; . "$here/lib.sh"; cpu_idle_pct "$1" "$2"); }
verdict "a busy guest (10 of 100 jiffies idle) reads 10" "$([ "$(idle_of "cpu  100 0 100 800 0 0 0 0 0 0" "cpu  145 0 145 810 0 0 0 0 0 0")" = 10 ] && echo 1)"
verdict "a quiet guest (95 of 100 idle, iowait counts as idle) reads 95" "$([ "$(idle_of "cpu  100 0 100 800 50 0 0 0 0 0" "cpu  102 0 103 890 55 0 0 0 0 0")" = 95 ] && echo 1)"
verdict "a line that is not /proc/stat gives nothing" "$([ -z "$(idle_of "garbage" "cpu  1 2 3 4 5 6 7 8 9 10")" ] && echo 1)"
verdict "two equal readings give nothing" "$([ -z "$(idle_of "cpu  1 2 3 4 5 6 7 8 9 10" "cpu  1 2 3 4 5 6 7 8 9 10")" ] && echo 1)"
exit $bad
