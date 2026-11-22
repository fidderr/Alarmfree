#!/bin/bash
# One scenario as a Maestro flow, with the state set before and checked after through the
# devtest helpers (a debug build: run-as, sqlite). The flow itself needs only the accessibility tree.
#   SERIAL=emulator-5560 devtest/maestro/run.sh s38 [runs]
# No telemetry: MAESTRO_CLI_NO_ANALYTICS, MAESTRO_DISABLE_UPDATE_CHECK and "enabled": false in ~/.maestro/analytics.json.
# The device is always named: maestro and adb take the first one otherwise.
cd "$(dirname "$0")/../.." || exit 1
[ -n "${SERIAL:-}" ] || { echo "SERIAL is not set"; exit 1; }
sc=$1 runs=${2:-1}
export OUT="${OUT:-devtest-out/maestro_${SERIAL}_$sc}"
. devtest/lib.sh
MAESTRO=${MAESTRO:-'C:\Users\rfidd\tools\maestro-2.12.1\maestro\bin\maestro.bat'}
export JAVA_HOME="${JAVA_HOME:-C:/Program Files/Microsoft/jdk-17.0.21.21-hotspot}" MAESTRO_CLI_NO_ANALYTICS=true MAESTRO_CLI_ANALYSIS_NOTIFICATION_DISABLED=true MAESTRO_DISABLE_UPDATE_CHECK=true
claim_device
open_shells
scenario() { CURRENT=$1; SCENARIO_START=$SECONDS; }
HERE=$(cd devtest/maestro && pwd -W 2> /dev/null || pwd)

maestro_flow() { # flow file
    local bs='\' flow
    # On a Linux runner MAESTRO is the maestro script itself (devtest/ci/closing.sh installs it).
    if [ "$(uname -s)" = Linux ]; then
        "$MAESTRO" --device "$SERIAL" test "$HERE/$1" > "$OUT/maestro_$sc.log" 2>&1
        return
    fi
    flow="${HERE}${bs}${1}"
    cmd.exe /c "$MAESTRO --device $SERIAL test $flow" > "$OUT/maestro_$sc.log" 2>&1
}

setup_s50() { fresh; }
check_s50() { problems=""; }

setup_s38() {
    fresh
    as_app "mkdir -p files/sounds/custom && cp files/sounds/default/air.ogg files/sounds/custom/mytune.ogg"
    seed a "$(alarm_json a 7 0 "$EVERY_DAY" null)"
    arm a
}
check_s38() {
    problems=""
    printf "SELECT prefs FROM settings;\n" | sql | grep -q '"accent":"Purple"' || problems="$problems; accent not kept"
    printf "SELECT prefs FROM settings;\n" | sql | grep -q '"mytune.ogg":"Morning"' || problems="$problems; name not saved"
}

setup_s36() {
    fresh
    seed a "$(alarm_json a 7 0 "$EVERY_DAY" null)"
    seed b "$(alarm_json b 8 0 "$EVERY_DAY" null)"
    seed c "$(alarm_json c 9 0 "$EVERY_DAY" null)"
    arm a
}
check_s36() {
    problems=""
    local order
    dev am force-stop "$PKG"
    start_app
    wait_for 20 on_card_rows
    order=$(ui snapshot | grep -oE '"label":"[^"]*","on"' | sed -E 's/^"label":"//; s/","on"$//' | tr '\n' ' ' | sed 's/ $//')
    [ "$order" = "b c a" ] || problems="$problems; order is '$order'"
}
on_card_rows() { ui snapshot | grep -q '"rows":\[{'; }

for i in $(seq 1 "$runs"); do
    t0=$SECONDS
    mkdir -p "$OUT"
    scenario "$sc"
    "setup_$sc"
    if maestro_flow "$sc.yaml"; then ok=1; else ok=0; fi
    "check_$sc"
    if [ "$ok" = 1 ] && [ -z "$problems" ]; then r=PASS; else r=FAIL; fi
    echo "maestro $sc $i $r $((SECONDS - t0))s  flow_ok=$ok ${problems#; }"
done
