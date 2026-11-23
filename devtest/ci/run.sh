#!/bin/bash
# One Android version of the full suite on a Linux runner (what EVERYWHERE=1 runs locally), then a markdown table
# for the job summary. Usage: devtest/ci/run.sh <avd>      Run from anywhere; needs the APK in build/ (APK=...).
# Needs ANDROID_HOME, /dev/kvm, an AVD made by make_avd.sh. Results: devtest-out/ci_<avd>/ and devtest-out/ci_<avd>.log.
# Switches from the environment, handed on to the suite: SCENARIOS="s50 s53" runs only those (tiers.sh), CHAOS=1 is sabotage
# mode (chaos.sh; CHAOS_SEED, CHAOS_DROP and the others too).
# Exit code: 1 for a FAIL (or an incomplete run), else 4 when the device broke and the rerun did not pass (INFRA), else 0.
cd "$(dirname "$0")/../.." || exit 1
# infra_standing: an INFRA line that no second run followed.
# shellcheck disable=SC1091
. devtest/baseline.sh
avd=${1%%+*}
export ANDROID_AVD_HOME="${ANDROID_AVD_HOME:-$HOME/.android/avd}"
export ADB="$ANDROID_HOME/platform-tools/adb" EMULATOR="$ANDROID_HOME/emulator/emulator"
export SKIP_APK_CHECK=1 EVERYWHERE=1 LANES=1 PORT_BASE=5554 EMULATOR_GPU="${EMULATOR_GPU:-swiftshader_indirect}"
# +swangle in the version token: SwiftShader through ANGLE (the emulator's newer software renderer) instead of swiftshader_indirect.
case "+$1+" in *+swangle+*) export EMULATOR_GPU=swangle_indirect ;; esac
export AVDS=$avd
[ -z "${SCENARIOS:-}" ] || export SCENARIOS
# A name that is no scenario of the suite would be run as a command: stop before anything starts.
for id in ${SCENARIOS:-}; do
    grep -qE "^${id}\(\)" devtest/matrix.sh devtest/panels.sh devtest/selftest.sh devtest/probe_text.sh || { echo "STOP  $id is no scenario of devtest/matrix.sh, panels.sh, selftest.sh or probe_text.sh"; exit 2; }
done
[ "${CHAOS:-}" != 1 ] || export CHAOS
"$ADB" start-server > /dev/null 2>&1
start=$SECONDS
bash devtest/all.sh ci
rc=$?
res="devtest-out/ci_$avd/results.txt" meta="devtest-out/ci_$avd.meta"
p=$(grep -c '^PASS' "$res" 2> /dev/null); f=$(grep -c '^FAIL' "$res" 2> /dev/null); s=$(grep -c '^SKIP' "$res" 2> /dev/null)
# A scenario that printed its header and no result was cut short (the run ended inside it): that is a failure, not a pass.
incomplete=""
for name in $(grep -ahE '^=== s[0-9a-z_]+$' "devtest-out/ci_$avd.log" | sed 's/^=== //' | sort -u); do
    # By the id (s07_ring has its results as s07a, s07b, s07c).
    grep -aqE "^(PASS|FAIL|SKIP|INFRA)  ${name%%_*}[a-z0-9]*_" "$res" 2> /dev/null || incomplete="$incomplete $name"
done
# The scenarios that run last (tiers.sh LEG_LAST) are the ones an early end loses without a header.
# With SCENARIOS only the named ones were asked for, and each of them must have a result (s07 has s07a, s07b, s07c).
if [ -n "${SCENARIOS:-}" ]; then
    # s26x2 is two runs of s26, and big_screens runs other scenarios at other sizes: neither has a result of its own name.
    for id in $SCENARIOS; do
        case $id in
            s26x2) re='s26([a-z0-9]*_|[[:space:]])' ;;
            big_screens) re='[a-z0-9_]+@[0-9]+x[0-9]+' ;;
            *) re="${id}([a-z]*_|[[:space:]])" ;;
        esac
        grep -aqE "^(PASS|FAIL|SKIP|INFRA)  $re" "$res" 2> /dev/null || incomplete="$incomplete $id(never ran)"
    done
else
    # An INFRA line that stands for a scenario that did not run has the unit's own name, then a space (baseline.sh).
    for id in s13 s13b s13d; do
        grep -aqE "^(PASS|FAIL|SKIP|INFRA)  ${id}(_|[[:space:]])" "$res" 2> /dev/null || incomplete="$incomplete $id(never ran)"
    done
fi
# The device broke and the second run did not pass: not an app failure, but the round is not green either.
if infra_standing "$res" && [ "$rc" = 0 ]; then rc=4; fi
[ -z "$incomplete" ] || rc=1
# Asking again on a machine that was not starved is a finding: the job fails on it.
grep -qs NOT-STARVED "devtest-out/ci_$avd/retries.txt" && rc=1
stop=$(grep -h '^STOP' "devtest-out/ci_$avd.log" 2> /dev/null | head -1)
{
    echo "### $1 (Android $(sed -n 's/.*android=\([^ ]*\).*/\1/p' "$meta" 2> /dev/null | head -1))"
    echo
    echo "| passed | failed | skipped | minutes | suite rc |"
    echo "|---|---|---|---|---|"
    echo "| ${p:-0} | ${f:-0} | ${s:-0} | $(((SECONDS - start) / 60)) | $rc |"
    [ -z "${SCENARIOS:-}" ] || { echo; echo "Only these scenarios: \`$SCENARIOS\`"; }
    # Sabotage mode: the numbers are in the run's chaos.state (generator, sent, dropped, seed), the dropped ones in chaos.txt.
    if [ "${CHAOS:-}" = 1 ]; then
        echo
        if read -r _ csent cdrop cseed < "devtest-out/ci_$avd/chaos.state" 2> /dev/null; then
            echo "Sabotage on: seed $cseed, touches sent $csent, touches dropped $cdrop (the dropped ones: chaos.txt)"
        else
            echo "Sabotage on, but no chaos.state was written: the suite did not get to use it."
        fi
    fi
    # Every scenario that asked again, and whether the machine was starved at that moment (a retry on a machine that was
    # not is a finding: it may be hiding a fault).
    retries="devtest-out/ci_$avd/retries.txt"
    if [ -s "$retries" ]; then
        echo; echo "Passed or failed only after asking again:"; echo
        sed 's/^/- `/; s/$/`/' "$retries" | cut -c1-230
        if grep -q 'NOT-STARVED' "$retries"; then echo; echo "FINDING: asked again on a machine that was NOT starved: $(grep -c 'NOT-STARVED' "$retries") time(s)."; fi
    else
        echo; echo "Asked again: never."
    fi
    [ -z "$incomplete" ] || { echo; echo "INCOMPLETE (no result): \`${incomplete# }\`"; }
    [ -z "$stop" ] || { echo; echo "STOPPED: \`$stop\`"; }
    if [ "${f:-0}" -gt 0 ]; then
        echo; echo "Failed:"; echo
        grep -h '^FAIL' "$res" | cut -c1-160 | sed 's/^/- `/; s/$/`/'
    fi
    # The device itself broke (the system process or the audio server was replaced, a new boot): not the app. A line that ends
    # in "rerun follows" was run again on a restarted device; the others stand.
    if grep -aq '^INFRA' "$res" 2> /dev/null; then
        echo; echo "The device broke (INFRA, not a failure of the app), $(grep -ac '^INFRA' "$res") line(s), $(grep -a '^INFRA' "$res" | grep -avc 'rerun follows') standing after the rerun:"; echo
        grep -ah '^INFRA' "$res" | cut -c1-230 | sed 's/^/- `/; s/$/`/'
    fi
} > "devtest-out/ci_$avd.summary.md"
cat "devtest-out/ci_$avd.summary.md"
[ -z "${GITHUB_STEP_SUMMARY:-}" ] || cat "devtest-out/ci_$avd.summary.md" >> "$GITHUB_STEP_SUMMARY"
exit "$rc"
