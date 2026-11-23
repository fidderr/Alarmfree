#!/bin/bash
# The parts of every version in one table, and the FAIL, INFRA and FLAKY lines of all of them (the last job of emulators.yml).
# Usage: devtest/ci/merge.sh <dir> <parts>
# <dir> holds one folder per part, <avd>-<part>/, each with the part's devtest-out/ci_<avd>/results.txt and ci_<avd>.summary.md.
# A part that has no results is said, never left out: a version is only counted green when all its parts are there.
dir=${1:?dir} parts=${2:-2}
avds=$(for d in "$dir"/*-[0-9]*/; do [ -d "$d" ] || continue; n=$(basename "$d"); echo "${n%-*}"; done | sort -u)
[ -n "$avds" ] || { echo "No results of any part were found."; exit 0; }
echo "### All parts, per version"
echo
echo "| version | parts | passed | flaky | failed | INFRA | skipped | results | minutes (slowest part) |"
echo "|---|---|---|---|---|---|---|---|---|"
tp=0 tfl=0 tf=0 ti=0 missing=0
lists=""
for avd in $avds; do
    p=0 fl=0 f=0 i=0 s=0 got=0 mins=0
    for part in $(seq 1 "$parts"); do
        res=$(ls "$dir/$avd-$part"/*/*/results.txt 2> /dev/null | head -1)
        if [ -z "$res" ]; then missing=$((missing + 1)); continue; fi
        got=$((got + 1))
        p=$((p + $(grep -ac '^PASS' "$res"))); fl=$((fl + $(grep -ac '^FLAKY' "$res"))); f=$((f + $(grep -ac '^FAIL' "$res")))
        i=$((i + $(grep -ac '^INFRA' "$res"))); s=$((s + $(grep -ac '^SKIP' "$res")))
        sm=$(ls "$dir/$avd-$part"/*/ci_*.summary.md 2> /dev/null | head -1)
        m=$(grep -a '^| [0-9]' "$sm" 2> /dev/null | head -1 | awk -F'|' '{gsub(/ /, "", $6); print $6}')
        case "$m" in '' | *[!0-9]*) ;; *) [ "$m" -le "$mins" ] || mins=$m ;; esac
        lists="$lists$(grep -ah '^\(FAIL\|INFRA\|FLAKY\)' "$res" | cut -c1-240 | sed "s/^/- \`$avd $part\/$parts  /; s/\$/\`/")"$'\n'
    done
    note=""
    [ "$got" = "$parts" ] || note=" (MISSING: $((parts - got)) part(s) gave no results)"
    echo "| $avd | $got/$parts$note | $p | $fl | $f | $i | $s | $((p + fl + f + i + s)) | $mins |"
    tp=$((tp + p)) tfl=$((tfl + fl)) tf=$((tf + f)) ti=$((ti + i))
done
echo
if [ "$tf" = 0 ] && [ "$ti" = 0 ] && [ "$tfl" = 0 ] && [ "$missing" = 0 ]; then
    echo "GATE: green. 0 FAIL, 0 INFRA, 0 FLAKY, every part present."
else
    echo "GATE: not green. $tf FAIL, $ti INFRA, $tfl FLAKY, $missing part(s) without results."
fi
if [ -n "${lists//[$'\n']/}" ]; then
    echo
    echo "FAIL, INFRA and FLAKY lines (version, part):"
    echo
    printf '%s' "$lists"
fi
exit 0
