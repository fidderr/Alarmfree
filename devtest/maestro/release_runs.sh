#!/bin/bash
# The release-build flow N times on one device: SERIAL=emulator-5560 devtest/maestro/release_runs.sh 10
# Maestro's driver sometimes is not up yet and the run ends after about 4 s with no step done: that run
# is started again (up to 3 times, counted as "startup"), only a run that did steps counts.
# A run that fails on a step keeps its logcat in devtest-out/rel_<serial>_<n>.logcat.
cd "$(dirname "$0")/../.." || exit 1
[ -n "${SERIAL:-}" ] || { echo "SERIAL is not set"; exit 1; }
export MSYS_NO_PATHCONV=1 JAVA_HOME="C:/Program Files/Microsoft/jdk-17.0.21.21-hotspot" MAESTRO_CLI_NO_ANALYTICS=true MAESTRO_CLI_ANALYSIS_NOTIFICATION_DISABLED=true MAESTRO_DISABLE_UPDATE_CHECK=true
ADB="${ADB:-$LOCALAPPDATA/Android/Sdk/platform-tools/adb.exe}"
HERE=$(cd devtest/maestro && pwd -W)
pass=0 fail=0 startup=0
for i in $(seq 1 "${1:-1}"); do
    t0=$SECONDS
    for try in 1 2 3; do
        "$ADB" -s "$SERIAL" logcat -c
        log="devtest-out/rel_${SERIAL}_$i.log"
        if cmd.exe /c "C:\Users\rfidd\tools\maestro-2.12.1\maestro\bin\maestro.bat --device $SERIAL test $HERE\release_smoke.yaml" > "$log" 2>&1; then r=PASS; else r=FAIL; fi
        if [ $r = FAIL ] && ! grep -q -E 'COMPLETED|FAILED' "$log"; then
            startup=$((startup + 1))
            echo "maestro release_smoke $i startup failure (try $try)"
            continue
        fi
        break
    done
    if [ $r = PASS ]; then pass=$((pass + 1)); else
        fail=$((fail + 1))
        "$ADB" -s "$SERIAL" logcat -d -v time > "devtest-out/rel_${SERIAL}_$i.logcat" 2>&1
    fi
    echo "maestro release_smoke $i $r $((SECONDS - t0))s"
done
echo "RELDONE pass=$pass fail=$fail startup_failures=$startup"
