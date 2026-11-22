#!/bin/bash
# The Linux way of what lib.sh does through PowerShell on Windows: start and stop an emulator. Sourced by lib.sh when
# uname says Linux (the GitHub runners, .github/workflows/emulators.yml). EMULATOR names the emulator binary.

linux_emulator_start() {
    local port=${SERIAL#emulator-}
    mkdir -p "$OUT"
    setsid nohup "$EMULATOR" -avd "$AVD" -no-window -no-audio -no-boot-anim -no-snapshot -gpu "${EMULATOR_GPU:-swiftshader_indirect}" -port "$port" ${EMULATOR_EXTRA:-} >> "$OUT/emulator.log" 2>&1 < /dev/null &
}

linux_emulator_stop() {
    local port=${SERIAL#emulator-} i
    pkill -9 -f "(emulator|qemu-system).* -port $port( |\$)" 2> /dev/null
    for i in $(seq 20); do
        pgrep -f "(emulator|qemu-system).* -port $port( |\$)" > /dev/null || break
        sleep 0.5
    done
    return 0
}
