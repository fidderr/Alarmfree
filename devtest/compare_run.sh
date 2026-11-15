#!/bin/bash
# One scenario, N times, old (debug channel) or new (A11Y=1): prints "mode scenario run PASS|FAIL seconds".
# usage: SERIAL=emulator-5560 devtest/compare_run.sh <old|new> <scenario> <runs>
cd "$(dirname "$0")/.." || exit 1
mode=$1 sc=$2 runs=$3
export A11Y=0
[ "$mode" = new ] && export A11Y=1
for i in $(seq 1 "$runs"); do
    t0=$SECONDS
    out=$(OUT="devtest-out/cmp_${SERIAL}_${mode}_${sc}_$i" bash devtest/matrix.sh "$sc" 2>&1)
    res=$(echo "$out" | grep -E "^(PASS|FAIL)" | head -1 | cut -c1-200)
    case "$res" in PASS*) r=PASS ;; FAIL*) r=FAIL ;; *) r="FAIL(no result)" ;; esac
    echo "$mode $sc $i $r $((SECONDS - t0))s  ${res#* }"
done
