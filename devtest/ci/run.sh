#!/bin/bash
# One Android version of the full suite on a Linux runner (what EVERYWHERE=1 runs locally), then a markdown table
# for the job summary. Usage: devtest/ci/run.sh <avd>      Run from anywhere; needs the APK in build/ (APK=...).
# Needs ANDROID_HOME, /dev/kvm, an AVD made by make_avd.sh. Results: devtest-out/ci_<avd>/ and devtest-out/ci_<avd>.log.
cd "$(dirname "$0")/../.." || exit 1
avd=$1
export ANDROID_AVD_HOME="${ANDROID_AVD_HOME:-$HOME/.android/avd}"
export ADB="$ANDROID_HOME/platform-tools/adb" EMULATOR="$ANDROID_HOME/emulator/emulator"
export SKIP_APK_CHECK=1 EVERYWHERE=1 LANES=1 PORT_BASE=5554 EMULATOR_GPU="${EMULATOR_GPU:-swiftshader_indirect}"
export AVDS=$avd
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
    grep -aqE "^(PASS|FAIL|SKIP)  ${name%%_*}[a-z0-9]*_" "$res" 2> /dev/null || incomplete="$incomplete $name"
done
# The scenarios that run last (tiers.sh LEG_LAST) are the ones an early end loses without a header.
for id in s13 s13b s13d; do
    grep -aqE "^(PASS|FAIL|SKIP)  ${id}_" "$res" 2> /dev/null || incomplete="$incomplete $id(never ran)"
done
[ -z "$incomplete" ] || rc=1
stop=$(grep -h '^STOP' "devtest-out/ci_$avd.log" 2> /dev/null | head -1)
{
    echo "### $avd (Android $(sed -n 's/.*android=\([^ ]*\).*/\1/p' "$meta" 2> /dev/null | head -1))"
    echo
    echo "| passed | failed | skipped | minutes | suite rc |"
    echo "|---|---|---|---|---|"
    echo "| ${p:-0} | ${f:-0} | ${s:-0} | $(((SECONDS - start) / 60)) | $rc |"
    [ -z "$incomplete" ] || { echo; echo "INCOMPLETE (no result): \`${incomplete# }\`"; }
    [ -z "$stop" ] || { echo; echo "STOPPED: \`$stop\`"; }
    if [ "${f:-0}" -gt 0 ]; then
        echo; echo "Failed:"; echo
        grep -h '^FAIL' "$res" | cut -c1-160 | sed 's/^/- `/; s/$/`/'
    fi
} > "devtest-out/ci_$avd.summary.md"
cat "devtest-out/ci_$avd.summary.md"
[ -z "${GITHUB_STEP_SUMMARY:-}" ] || cat "devtest-out/ci_$avd.summary.md" >> "$GITHUB_STEP_SUMMARY"
exit "$rc"
