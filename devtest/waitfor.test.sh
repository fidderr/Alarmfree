#!/bin/bash
# wait_for of lib.sh: N seconds really pass. PASS or FAIL per case. Usage: bash devtest/waitfor.test.sh   (no device)
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

never() { return 1; }
now_us() { local t=${EPOCHREALTIME/[.,]/}; echo "$t"; }

# Each case starts at a different point within a second, as the old code was early by the part of the second already gone.
for pause in 0 0.3 0.6 0.9; do
    sleep "$pause"
    a=$(now_us)
    wait_for 2 never && rc=0 || rc=1
    b=$(now_us)
    ms=$(((b - a) / 1000))
    ok=0
    [ "$rc" = 1 ] && [ "$ms" -ge 2000 ] && [ "$ms" -lt 2600 ] && ok=1
    verdict "wait_for 2 on a condition that never holds takes 2.0 to 2.6 s (start +$pause: ${ms} ms)" "$ok" "took ${ms} ms, status $rc"
done

a=$(now_us)
wait_for 5 true && rc=0 || rc=1
b=$(now_us)
ms=$(((b - a) / 1000))
ok=0
[ "$rc" = 0 ] && [ "$ms" -lt 300 ] && ok=1
verdict "a condition that holds at once returns at once (${ms} ms)" "$ok" "took ${ms} ms, status $rc"

# A condition that becomes true after 1 s: found, and well before the limit.
t_end=$(($(now_us) + 1000000))
late() { [ "$(now_us)" -ge "$t_end" ]; }
a=$(now_us)
wait_for 5 late && rc=0 || rc=1
b=$(now_us)
ms=$(((b - a) / 1000))
ok=0
[ "$rc" = 0 ] && [ "$ms" -ge 900 ] && [ "$ms" -lt 1500 ] && ok=1
verdict "a condition that holds after 1 s is found (${ms} ms)" "$ok" "took ${ms} ms, status $rc"

# Without EPOCHREALTIME (an old bash): never less than N seconds.
a=$(now_us)
(unset EPOCHREALTIME; wait_for 2 never) && rc=0 || rc=1
b=$(now_us)
ms=$(((b - a) / 1000))
ok=0
[ "$rc" = 1 ] && [ "$ms" -ge 2000 ] && [ "$ms" -lt 3300 ] && ok=1
verdict "without EPOCHREALTIME wait_for 2 takes 2 to 3 s (${ms} ms)" "$ok" "took ${ms} ms"

exit "$bad"
