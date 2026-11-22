#!/bin/bash
# The closing checks of a release on one emulator: the Slint smoke test, the accessibility audit and the three Maestro
# flows (s38, s36, s50). Boots the emulator the way the suite does (lib.sh reboot_only + linux.sh), installs the APK,
# runs the checks one after the other and writes a result line per check (and per step of the smoke test).
# Usage: devtest/ci/closing.sh <avd>          needs ANDROID_HOME, /dev/kvm, an AVD from make_avd.sh, the APK in build/
#        devtest/ci/closing.sh --install-maestro   downloads Maestro 2.12.1 from its GitHub release and prints its path
# Results: devtest-out/closing_<avd>/<check>/output.txt (+ the check's own files), devtest-out/closing_<avd>/summary.md.
# Settings: APK, PORT (emulator port, 5554), MAESTRO (path of the maestro script; installed when not set), CHECKS
# (which of smoke a11y s38 s36 s50 to run, in that order; all by default).
cd "$(dirname "$0")/../.." || exit 1

MAESTRO_VERSION=2.12.1
MAESTRO_SHA256=54dbb0716425626604973cbb374c585ca30272375d9ce15177a1152765ddd9d7
MAESTRO_URL="https://github.com/mobile-dev-inc/maestro/releases/download/cli-$MAESTRO_VERSION/maestro.zip"

# Maestro from the project's own release (a zip with a maestro/ folder), checked against the digest GitHub lists for it.
install_maestro() {
    local dir="${MAESTRO_HOME:-$HOME/maestro-$MAESTRO_VERSION}" zip
    if [ ! -x "$dir/maestro/bin/maestro" ]; then
        zip=$(mktemp -d)/maestro.zip
        curl -fsSL --retry 3 -o "$zip" "$MAESTRO_URL" || { echo "Maestro download failed" >&2; return 1; }
        [ "$(sha256sum "$zip" | cut -d' ' -f1)" = "$MAESTRO_SHA256" ] || { echo "Maestro download: wrong sha256" >&2; return 1; }
        mkdir -p "$dir" && unzip -q -o "$zip" -d "$dir" || return 1
        rm -f "$zip"
    fi
    echo "$dir/maestro/bin/maestro"
}
if [ "${1:-}" = --install-maestro ]; then
    install_maestro
    exit
fi

[ -n "${1:-}" ] || { echo "usage: devtest/ci/closing.sh <avd>"; exit 1; }
avd=${1%%+*}
CHECKS=${CHECKS:-smoke a11y s38 s36 s50}
export ANDROID_AVD_HOME="${ANDROID_AVD_HOME:-$HOME/.android/avd}"
export ADB="${ADB:-$ANDROID_HOME/platform-tools/adb}" EMULATOR="${EMULATOR:-$ANDROID_HOME/emulator/emulator}"
export AVD=$avd SERIAL="emulator-${PORT:-5554}" EMULATOR_GPU="${EMULATOR_GPU:-swiftshader_indirect}"
export APK="${APK:-build/alarmfree-slint-debug.apk}"
BASE="devtest-out/closing_$avd"
rm -rf "$BASE"
mkdir -p "$BASE"
export OUT="$BASE/boot"
# shellcheck disable=SC1091
. devtest/lib.sh

summary="$BASE/summary.md"
overall=0
android="?"
# One row of the table: result, check, detail.
row() { printf '%s\t%s\t%s\n' "$1" "$2" "$3" >> "$BASE/rows.tsv"; }
out_of() { echo "$BASE/$1"; }

# A check in its own folder, its output in output.txt. Its exit code is left in CHECK_RC.
run_check() { # name limit command...
    local name=$1 limit=$2
    shift 2
    mkdir -p "$(out_of "$name")"
    echo "=== $name ($(date +%T))"
    # The clock a check moved goes back to the host's: the next one starts from real time.
    timeout 30 "$ADB" -s "$SERIAL" shell cmd alarm set-time $(($(date +%s) * 1000)) > /dev/null 2>&1
    OUT=$(out_of "$name") timeout -k 30 "$limit" "$@" > "$(out_of "$name")/output.txt" 2>&1
    CHECK_RC=$?
    tail -n 25 "$(out_of "$name")/output.txt"
    timeout 60 "$ADB" -s "$SERIAL" logcat -d -t 30000 -b all -v time > "$(out_of "$name")/logcat.txt" 2>&1
}

mark() { # PASS|FAIL|STOP name detail
    echo "$1  $2  $3"
    row "$1" "$2" "$3"
    [ "$1" = PASS ] || overall=1
}

# ---- the device ----
boot_ok=0
echo "=== boot $avd"
"$ADB" start-server > /dev/null 2>&1
if reboot_only; then
    android=$(adb_ shell getprop ro.build.version.release | tr -d '\r')
    said=$(adb_ install -r -g "$APK" 2>&1)
    case "$said" in
        *Success*) boot_ok=1 ;;
        *) mark STOP install "$said" ;;
    esac
    # What the suite sets on an emulator before its scenarios: the clock stays where a check puts it.
    adb_ shell settings put global auto_time 0 > /dev/null
else
    mark STOP boot "$avd did not come up"
fi
echo "android=$android"

if [ "$boot_ok" = 1 ]; then
    for check in $CHECKS; do
        case "$check" in
            smoke)
                run_check smoke 20m bash devtest/slint_smoke.sh
                # One row per step of the script, then the verdict of the whole.
                while IFS= read -r l; do row "${l%% *}" "smoke step" "${l#*  }"; done < <(grep -aE '^(PASS|FAIL)  ' "$(out_of smoke)/output.txt")
                if [ "$CHECK_RC" = 0 ] && grep -aq '^ALL PASS' "$(out_of smoke)/output.txt"; then
                    mark PASS smoke "$(grep -ac '^PASS ' "$(out_of smoke)/output.txt") steps"
                else
                    mark FAIL smoke "rc=$CHECK_RC, $(grep -ac '^FAIL ' "$(out_of smoke)/output.txt") step(s) failed"
                fi
                ;;
            a11y)
                A11Y=1 run_check a11y 40m bash devtest/matrix.sh a11y_audit
                res="$(out_of a11y)/results.txt"
                found=$(grep -acE ': (UNNAMED|NO ROLE) ' "$(out_of a11y)/output.txt")
                notfound=$(grep -acE 'row not found|^no home' "$(out_of a11y)/output.txt")
                services=$(timeout 30 "$ADB" -s "$SERIAL" shell settings get secure enabled_accessibility_services 2> /dev/null | tr -d '\r')
                if grep -aq '^PASS  a11y_audit' "$res" 2> /dev/null && ! grep -aqE '^(FAIL|STOP)' "$res"; then
                    mark PASS a11y "audit done; unnamed or role-less nodes: ${found:-0}; rows or home not found: ${notfound:-0}; enabled services: ${services:-none}"
                else
                    mark FAIL a11y "rc=$CHECK_RC: $(grep -aE '^(FAIL|STOP)' "$res" 2> /dev/null | head -1 | cut -c1-200)"
                fi
                ;;
            s38 | s36 | s50)
                [ -n "${MAESTRO:-}" ] || MAESTRO=$(install_maestro) || { mark FAIL "maestro $check" "Maestro did not install"; continue; }
                export MAESTRO
                run_check "maestro_$check" 25m bash devtest/maestro/run.sh "$check"
                line=$(grep -aE "^maestro $check " "$(out_of "maestro_$check")/output.txt" | tail -1)
                case "$line" in
                    *" PASS "*) mark PASS "maestro $check" "$line" ;;
                    *) mark FAIL "maestro $check" "rc=$CHECK_RC ${line:-no result line}" ;;
                esac
                ;;
            *) mark FAIL "$check" "unknown check" ;;
        esac
    done
else
    overall=1
fi
emulator_stop

{
    echo "### Closing checks, $1 (Android $android)"
    echo
    echo '| result | check | detail |'
    echo '|---|---|---|'
    awk -F'\t' '{ printf "| %s | %s | `%s` |\n", $1, $2, substr($3, 1, 220) }' "$BASE/rows.tsv"
} > "$summary"
cat "$summary"
[ -z "${GITHUB_STEP_SUMMARY:-}" ] || cat "$summary" >> "$GITHUB_STEP_SUMMARY"
exit "$overall"
