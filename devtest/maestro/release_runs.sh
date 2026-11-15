#!/bin/bash
# The release-build flow N times on one device: SERIAL=emulator-5560 devtest/maestro/release_runs.sh 10
cd "$(dirname "$0")/../.." || exit 1
[ -n "${SERIAL:-}" ] || { echo "SERIAL is not set"; exit 1; }
export MSYS_NO_PATHCONV=1 JAVA_HOME="C:/Program Files/Microsoft/jdk-17.0.21.21-hotspot" MAESTRO_CLI_NO_ANALYTICS=true MAESTRO_CLI_ANALYSIS_NOTIFICATION_DISABLED=true MAESTRO_DISABLE_UPDATE_CHECK=true
HERE=$(cd devtest/maestro && pwd -W)
for i in $(seq 1 "${1:-1}"); do
    t0=$SECONDS
    if cmd.exe /c "C:\Users\rfidd\tools\maestro-2.12.1\maestro\bin\maestro.bat --device $SERIAL test $HERE\release_smoke.yaml" > "devtest-out/rel_${SERIAL}_$i.log" 2>&1; then r=PASS; else r=FAIL; fi
    echo "maestro release_smoke $i $r $((SECONDS - t0))s"
done
echo RELDONE
