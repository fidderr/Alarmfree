#!/bin/bash
# Record real movements for tuning the squat counter: starts the counter and the accelerometer
# recorder in the app on SERIAL, waits, stops, and pulls the CSV to devtest-out/squat/<name>.csv.
#   SERIAL=<device> bash devtest/squat_record.sh <name> [seconds] [expected squats]
#
# <name>    what it is, like squats_slow_10 or walking (letters, digits, _ and -)
# seconds   how long to record (default 40): do the movements while it runs
# expected  the true number of squats (0 for walking, shaking, pumping the arms); written into the
#           file as "# expect: N". Without it add that line by hand before using the file as a fixture.
#
# Keep the app open on the screen: a background app gets no sensor events. Needs a debug build or a
# release with RELEASE_TEST (src/debug_hooks.rs). The file is then copied to
# mobile-sentinel/tests/fixtures/squat/ and `cargo test --all-features --test squat_fixtures` run
# there (docs/SQUAT_TUNING.md).
cd "$(dirname "$0")/.." || exit 1
name=${1:?usage: squat_record.sh <name> [seconds] [expected squats]}
seconds=${2:-40}
expected=${3:-}
case "$name" in *[!A-Za-z0-9_-]*) echo "name: letters, digits, _ and - only"; exit 1 ;; esac
. devtest/lib.sh
open_shells
page_is_up || { echo "the app does not answer: open it on $SERIAL first"; exit 1; }
as_app "rm -f files/squat_record.csv files/squat_record.txt" > /dev/null
action "squat_record:start"
echo "recording $seconds s on $SERIAL: go"
sleep "$seconds"
action "squat_record:stop"
has_result() { [ -n "$(as_app 'cat files/squat_record.txt 2> /dev/null')" ]; }
wait_for 10 has_result
mkdir -p devtest-out/squat
file=devtest-out/squat/$name.csv
adb_ exec-out "run-as $PKG cat $DATA/files/squat_record.csv" > "$file"
if [ -n "$expected" ]; then
    sed -i "2a # expect: $expected" "$file"
fi
echo "the app counted: $(as_app 'cat files/squat_record.txt 2> /dev/null')"
echo "saved $file ($(wc -l < "$file") lines)"
