#!/bin/bash
# The queue behind all.sh and quick.sh: a list of emulators, up to LANES at a time (2: that is what 16 GB has
# room for), each leg booted, installed once, run, shut down. Sourced, with the working directory at the repository.
#
# suite_main <name> <tier> <avd...>    results in devtest-out/<name>_<avd>/ and <name>_<avd>.log, a table at the end
# Settings: LANES, PORT_BASE (lane n uses PORT_BASE + 2n), APK, SKIP_APK_CHECK=1, NO_SNAPSHOT_BOOT=1

. devtest/tiers.sh
export APK="${APK:-build/alarmfree-slint-debug.apk}"
LANES="${LANES:-2}"
PORT_BASE="${PORT_BASE:-5600}"
ADB="${ADB:-$LOCALAPPDATA/Android/Sdk/platform-tools/adb.exe}"

# A build that fails leaves the last APK where it was: this would then test something older.
check_apk() {
    [ -z "${SKIP_APK_CHECK:-}" ] || return 0
    local built repo
    built=$(stat -c %Y "$APK") || return 1
    for repo in . ../mobile-sentinel; do
        # A snapshot copy of devtest/ (not a repository) has nothing to compare.
        git -C "$repo" rev-parse --git-dir > /dev/null 2>&1 || continue
        if [ "$built" -lt "$(git -C "$repo" log -1 --format=%ct)" ]; then
            echo "$APK is older than the last commit in $repo: build first (or SKIP_APK_CHECK=1)"
            return 1
        fi
    done
}

# The start of a leg. A cold boot takes 30 s or more (the first boot of a new AVD 90 s); an emulator snapshot of
# the booted, untouched system (taken here the first time, 2.5 GB in the AVD's folder) loads in 7 s. Only for this
# first start: the restarts the scenarios do themselves (s11, s13) stay cold boots, which is the point of them.
# NO_SNAPSHOT_BOOT=1 turns it off; a snapshot goes stale with a new system image: delete <avd>.avd/snapshots/suite_clean.
boot_leg() {
    local dir="$USERPROFILE/.android/avd/$AVD.avd/snapshots/suite_clean"
    if [ -n "${NO_SNAPSHOT_BOOT:-}" ]; then
        reboot_and_wait
        return
    fi
    if [ -d "$dir" ]; then
        emulator_stop
        powershell.exe -NoProfile -Command "Start-Process -WindowStyle Hidden -FilePath '$(cygpath -w "$EMULATOR")' -ArgumentList '-avd $AVD -no-window -no-audio -no-boot-anim -snapshot suite_clean -no-snapshot-save -gpu ${EMULATOR_GPU:-host} -port ${SERIAL#emulator-}'"
        timeout 120 "$ADB" -s "$SERIAL" wait-for-device &&
            timeout 120 "$ADB" -s "$SERIAL" shell 'while [ -z "$(getprop sys.boot_completed)" ]; do sleep 0.3; done' || return 1
        # The guest clock stays at the time of the snapshot for a while: the host's time now.
        timeout 20 "$ADB" -s "$SERIAL" shell cmd alarm set-time $(($(date +%s) * 1000)) > /dev/null
        open_shells
        return 0
    fi
    reboot_and_wait || return 1
    timeout 60 "$ADB" -s "$SERIAL" emu avd snapshot save suite_clean > /dev/null 2>&1 || note "no snapshot saved"
}

# run_leg name tier avd port: one emulator from boot to shutdown. Its own folder, log and meta file.
run_leg() {
    local name=$1 tier=$2 avd=$3 port=$4
    local log="devtest-out/${name}_$avd.log" meta="devtest-out/${name}_$avd.meta"
    mkdir -p devtest-out
    : > "$meta"
    (
        local start=$SECONDS boot_s=0 said
        export AVD=$avd SERIAL=emulator-$port OUT="devtest-out/${name}_$avd"
        export EMULATOR_GPU="${EMULATOR_GPU:-$(gpu_for "$avd")}"
        tier_setup "$avd" "$tier"
        # shellcheck disable=SC1091
        . devtest/lib.sh
        claim_device
        local b=$SECONDS
        boot_leg || { echo "FAIL  $avd did not start"; exit 1; }
        boot_s=$((SECONDS - b))
        echo "android=$(adb_ shell getprop ro.build.version.release | tr -d '\r') boot_s=$boot_s" >> "$meta"
        # A failed install would go on to test what is already on it.
        said=$(adb_ install -r -g "$APK" 2>&1)
        case "$said" in
            *Success*) ;;
            *)
                echo "STOP  $avd: the install failed: $said"
                exit 1
                ;;
        esac
        # Without a working window nearly everything fails, and slowly: stop the run at once.
        fresh
        export CLEAN_COPY
        start_app
        wait_for 60 page_is_up && wait_for 30 on_screen_no 0 || { echo "STOP  $avd: the window does not come up on the alarm list"; exit 1; }
        SKIP=$LEG_SKIP FIRST=$LEG_FIRST LAST=$LEG_LAST bash devtest/matrix.sh $LEG_LIST
        emulator_stop
        echo "seconds=$((SECONDS - start))" >> "$meta"
    ) > "$log" 2>&1
    local rc=$?
    echo "rc=$rc" >> "$meta"
    echo "=== $avd (rc $rc)"
    sed -n '/===== SUMMARY/,$p' "$log" | grep -E '^(FAIL|STOP|passed:|over a minute)'
    grep -E '^(FAIL|STOP)' "$log" | grep -v "^FAIL  [a-z0-9_]* " | head -3
}

# The table: AVD, Android, passed, failed, time.
suite_table() {
    local name=$1 avd meta res p f s android rc
    echo
    printf '%-8s %-8s %7s %7s %8s %6s  %s\n' AVD Android passed failed skipped time notes
    shift
    for avd in "$@"; do
        meta="devtest-out/${name}_$avd.meta"
        res="devtest-out/${name}_$avd/results.txt"
        [ -s "$meta" ] || continue
        android=$(sed -n 's/.*android=\([^ ]*\).*/\1/p' "$meta" | head -1)
        s=$(sed -n 's/^seconds=//p' "$meta")
        rc=$(sed -n 's/^rc=//p' "$meta")
        p=$(grep -c '^PASS' "$res" 2> /dev/null)
        f=$(grep -c '^FAIL' "$res" 2> /dev/null)
        printf '%-8s %-8s %7s %7s %8s %3s:%02d  %s\n' "$avd" "${android:--}" "${p:-0}" "${f:-0}" "$(grep -c '^SKIP' "$res" 2> /dev/null)" "$(((${s:-0}) / 60))" "$(((${s:-0}) % 60))" \
            "$(grep -h '^STOP' "devtest-out/${name}_$avd.log" 2> /dev/null | head -1)$([ "${rc:-0}" != 0 ] && echo " rc=$rc")"
        grep -h '^FAIL' "$res" 2> /dev/null | sed 's/^/           /' | cut -c1-200
    done
}

suite_main() {
    local name=$1 tier=$2 avd lane started=0 total
    shift 2
    local avds=("$@") pids=() list=()
    [ ${#avds[@]} -gt 0 ] || { echo "no emulators given"; return 1; }
    check_apk || return 1
    local start=$SECONDS
    for lane in $(seq 0 $((LANES - 1))); do pids[lane]=""; done
    for avd in "${avds[@]}"; do
        while :; do
            for lane in $(seq 0 $((LANES - 1))); do
                if [ -n "${pids[lane]}" ] && ! kill -0 "${pids[lane]}" 2> /dev/null; then pids[lane]=""; fi
                if [ -z "${pids[lane]}" ]; then
                    echo "start $avd on lane $lane (port $((PORT_BASE + 2 * lane))), $((SECONDS - start)) s into the run"
                    run_leg "$name" "$tier" "$avd" $((PORT_BASE + 2 * lane)) &
                    pids[lane]=$!
                    break 2
                fi
            done
            sleep 3
        done
    done
    wait
    suite_table "$name" "${avds[@]}"
    echo "all of it in $(((SECONDS - start) / 60)) min $(((SECONDS - start) % 60)) s ($tier tier, $LANES lanes)"
    for avd in "${avds[@]}"; do
        grep -q '^FAIL\|^STOP' "devtest-out/${name}_$avd/results.txt" 2> /dev/null && return 1
        grep -q '^STOP' "devtest-out/${name}_$avd.log" 2> /dev/null && return 1
    done
    return 0
}
