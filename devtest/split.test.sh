#!/bin/bash
# The split of the full list into two parts (devtest/ci/split.py) and the merged table (devtest/ci/merge.sh), without a device.
# Usage: bash devtest/split.test.sh
here=$(cd "${BASH_SOURCE[0]%/*}" && pwd)
tmp=$(mktemp -d) || exit 1
trap 'rm -rf "$tmp"' EXIT
bad=0
verdict() { if [ "$2" = 1 ]; then echo "PASS  $1"; else echo "FAIL  $1"; bad=1; fi; }
split=$here/ci/split.py
all=$(sed -n 's/^ALL="\(.*\)"/\1/p' "$here/matrix.sh" | head -1)

# 1. Every unit of the full list is in exactly one part.
p1=$(python3 "$split" 2 1) p2=$(python3 "$split" 2 2)
dups=$(printf '%s\n' $p1 $p2 | sort | uniq -d | tr '\n' ' ')
missing=""
for u in $all; do case " $p1 $p2 " in *" $u "*) ;; *) missing="$missing $u" ;; esac; done
extra=$(for u in $p1 $p2; do case " $all " in *" $u "*) ;; *) echo "$u" ;; esac; done | tr '\n' ' ')
[ -z "$dups" ] && [ -z "$missing" ] && [ -z "$extra" ] && [ "$(printf '%s\n' $p1 $p2 | wc -l)" = "$(printf '%s\n' $all | wc -l)" ] &&
    verdict "every scenario of the full list is in exactly one part ($(printf '%s\n' $all | wc -l) units)" 1 ||
    verdict "every scenario of the full list is in exactly one part (twice: $dups; missing: $missing; not in the list: $extra)" 0

# 2. The parts' expected seconds differ by less than 10%.
check=$(python3 "$split" --check 2)
s1=$(printf '%s\n' "$check" | sed -n '1s/^part 1\/2: \([0-9]*\) s.*/\1/p') s2=$(printf '%s\n' "$check" | sed -n '2s/^part 2\/2: \([0-9]*\) s.*/\1/p')
hi=$((s1 > s2 ? s1 : s2)) lo=$((s1 > s2 ? s2 : s1))
[ "$hi" -gt 0 ] && [ $(((hi - lo) * 100 / hi)) -lt 10 ] && verdict "the parts differ by less than 10% in expected time ($s1 s and $s2 s)" 1 || verdict "the parts differ by less than 10% in expected time ($s1 s and $s2 s)" 0

# 3. Each part keeps the run order: big_screens first, the restarts with a PIN last, the rest in the order of the list.
first=$(for u in $p1 $p2; do :; done; printf '%s\n' $p1 | head -1)
last1=$(printf '%s\n' $p1 | tail -1) last2=$(printf '%s\n' $p2 | tail -1)
ordered=1
for part in "$p1" "$p2"; do
    prev=-1
    for u in $part; do
        case "$u" in big_screens | s13 | s13b | s13d) continue ;; esac
        pos=$(printf '%s\n' $all | grep -nx "$u" | cut -d: -f1)
        [ "$pos" -gt "$prev" ] || ordered=0
        prev=$pos
    done
done
[ "$first" = big_screens ] || case " $p2 " in *" big_screens "*) ;; *) ordered=0 ;; esac
case "$last1 $last2" in *s13*) ;; *) ordered=0 ;; esac
[ "$ordered" = 1 ] && verdict "each part runs in the order of the list, big_screens first, the restarts with a PIN last" 1 || verdict "each part runs in the order of the list ($last1 / $last2)" 0

# 4. A scenario that is not in durations.txt gets the default weight and still lands in a part; one part, one list.
mkdir -p "$tmp/new"
cp "$here/ci/split.py" "$tmp/new/split.py"
sed "s/^\(ALL=\"\)/\1s99 /" "$here/matrix.sh" | grep '^ALL=' > "$tmp/matrix.sh"
cp "$here/ci/durations.txt" "$tmp/new/durations.txt"
cp "$tmp/matrix.sh" "$tmp/matrix_all.sh"
mkdir -p "$tmp/new2/ci"
cp "$here/ci/split.py" "$here/ci/durations.txt" "$tmp/new2/ci/"
cp "$tmp/matrix.sh" "$tmp/new2/matrix.sh"
n1=$(python3 "$tmp/new2/ci/split.py" 2 1) n2=$(python3 "$tmp/new2/ci/split.py" 2 2)
case " $n1 $n2 " in *" s99 "*) verdict "a new scenario without a measured time lands in a part (default weight)" 1 ;; *) verdict "a new scenario without a measured time lands in a part" 0 ;; esac
[ "$(python3 "$split" 1 1)" = "$(python3 "$split" 1 1)" ] && [ "$(python3 "$split" 1 1 | wc -w)" = "$(printf '%s\n' $all | wc -l)" ] && verdict "one part is the whole list" 1 || verdict "one part is the whole list" 0

# 5. --measure maps the lettered results to their unit and adds the time of the other versions.
cat > "$tmp/res_a.txt" << 'R'
PASS  s01_repeating_dead  [20s] x
PASS  s07a_kill_main_mid_ring  [1s] x
PASS  s07b_kill_sentinel_mid_ring  [2s] x
PASS  s07d_end_again  [30s] x
PASS  s12_passed_during_restart  [10s] x
PASS  s11a_reboot_mid_ring  [0s] x
PASS  s34_nothing_clipped@1600x2560  [10s] x
NOTE  s01  something
R
cp "$tmp/res_a.txt" "$tmp/res_b.txt"
m=$(python3 "$split" --measure "$tmp/res_a.txt" "$tmp/res_b.txt")
get() { printf '%s\n' "$m" | awk -v u="$1" '$1 == u { print $2 }'; }
[ "$(get s01)" = 23 ] && [ "$(get s07)" = 9 ] && [ "$(get s07d)" = 33 ] && [ "$(get s11)" = 16 ] && [ "$(get big_screens)" = 13 ] &&
    verdict "--measure: s07a and s07b count for s07, s12 and s11a for s11, a size for big_screens, plus 3 s a result" 1 ||
    verdict "--measure ($(printf '%s\n' "$m" | grep -E '^(s01|s07|s07d|s11|big_screens) ' | tr '\n' ' '))" 0

# 5b. unit_times.txt (what run_unit measured): the seconds of a unit as they are, a size's scenarios are big_screens.
mkdir -p "$tmp/ut"
printf 's01 40
s01 20
s19@1600x2560 30
s34@2208x1840 12
s13 100
' > "$tmp/ut/unit_times.txt"
m=$(python3 "$split" --measure "$tmp/ut/unit_times.txt")
get() { printf '%s
' "$m" | awk -v u="$1" '$1 == u { print $2 }'; }
[ "$(get s01)" = 60 ] && [ "$(get big_screens)" = 42 ] && [ "$(get s13)" = 100 ] && verdict "--measure reads unit_times.txt: seconds per unit, sizes are big_screens" 1 || verdict "--measure reads unit_times.txt ($(printf '%s
' "$m" | grep -E '^(s01|big_screens|s13) ' | tr '
' ' '))" 0

# 6. The merged table: counts per version over its parts, FLAKY apart, a missing part is said and the gate is not green.
mkdir -p "$tmp/m/afg16-1/devtest-out/ci_afg16" "$tmp/m/afg16-2/devtest-out/ci_afg16" "$tmp/m/msd30-1/devtest-out/ci_msd30"
printf 'PASS  a_x  [1s] ok\nPASS  b_x  [1s] ok\nNOTE  a  hi\n' > "$tmp/m/afg16-1/devtest-out/ci_afg16/results.txt"
printf 'PASS  c_x  [1s] ok\nFLAKY  d_x  [2s] failed the first run (x) and passed the second (y)\nSKIP  e_x  [0s] no\n' > "$tmp/m/afg16-2/devtest-out/ci_afg16/results.txt"
printf 'PASS  a_x  [1s] ok\nFAIL  b_x  [1s] bad\nINFRA  c_x  [1s] broke\n' > "$tmp/m/msd30-1/devtest-out/ci_msd30/results.txt"
printf '### afg16\n\n| passed | flaky | failed | skipped | minutes | suite rc |\n|---|---|---|---|---|---|\n| 2 | 0 | 0 | 0 | 21 | 0 |\n' > "$tmp/m/afg16-1/devtest-out/ci_afg16.summary.md"
printf '### afg16\n\n| passed | flaky | failed | skipped | minutes | suite rc |\n|---|---|---|---|---|---|\n| 1 | 1 | 0 | 1 | 23 | 0 |\n' > "$tmp/m/afg16-2/devtest-out/ci_afg16.summary.md"
out=$(bash "$here/ci/merge.sh" "$tmp/m" 2)
printf '%s\n' "$out" | grep -q '^| afg16 | 2/2 | 3 | 1 | 0 | 0 | 1 | 5 | 23 |$' && verdict "the merged table adds the parts of a version (passed, flaky, failed, INFRA, skipped, slowest part)" 1 || verdict "the merged table adds the parts of a version ($(printf '%s\n' "$out" | grep afg16))" 0
printf '%s\n' "$out" | grep -q '^| msd30 | 1/2 (MISSING: 1 part(s) gave no results) | 1 | 0 | 1 | 1 | 0 | 3 |' && verdict "a part without results is said in the table" 1 || verdict "a part without results is said in the table ($(printf '%s\n' "$out" | grep msd30))" 0
printf '%s\n' "$out" | grep -q '^GATE: not green. 1 FAIL, 1 INFRA, 1 FLAKY, 1 part(s) without results.$' && printf '%s\n' "$out" | grep -q 'FLAKY  d_x' && verdict "the gate counts FAIL, INFRA, FLAKY and a missing part, and lists their lines" 1 || verdict "the gate counts FAIL, INFRA, FLAKY and a missing part ($(printf '%s\n' "$out" | grep GATE))" 0
rm -rf "$tmp/m/msd30-1"; mkdir -p "$tmp/m/msd30-1/devtest-out/ci_msd30" "$tmp/m/msd30-2/devtest-out/ci_msd30"
printf 'PASS  a_x  [1s] ok\n' > "$tmp/m/msd30-1/devtest-out/ci_msd30/results.txt"; printf 'PASS  b_x  [1s] ok\n' > "$tmp/m/msd30-2/devtest-out/ci_msd30/results.txt"
rm -rf "$tmp/m/afg16-2"; mkdir -p "$tmp/m/afg16-2/devtest-out/ci_afg16"; printf 'PASS  c_x  [1s] ok\n' > "$tmp/m/afg16-2/devtest-out/ci_afg16/results.txt"
bash "$here/ci/merge.sh" "$tmp/m" 2 | grep -q '^GATE: green. 0 FAIL, 0 INFRA, 0 FLAKY, every part present.$' && verdict "all parts clean: the gate is green" 1 || verdict "all parts clean: the gate is green" 0
exit $bad
