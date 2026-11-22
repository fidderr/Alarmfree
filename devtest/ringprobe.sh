#!/bin/bash
# The ring-screen probe: an alarm rings while the app sits in the background, and the probe records when the main process took
# the ring up and when the ring screen came on. By name only (like selftest.sh), sourced by matrix.sh:
#   SCENARIOS="t11_ring_after_home_1s t12_ring_after_home_5s t13_ring_after_home_15s t14_ring_after_home_40s"
#   t10_ring_probe_smoke   1 round of each of 1 s and 5 s, to see that the probe itself reads.
# `Home` is pressed N seconds before the ring (N = 1, 5, 15, 40), PROBE_ROUNDS times (20), and each round is watched for
# PROBE_WATCH seconds (25) past the ring. One line per round in $OUT/ringprobe.txt (columns in PROBE_HEADER), the raw lines of
# the log that the numbers come from in $OUT/ringprobe_<name>_<n>.log, and a PASS line with the counts. The probe passes when
# it measured; what the numbers mean is read from them (devtest-out/<avd>/ringprobe.txt).
#
# Columns, ms past the alarm time T unless said otherwise ("-" is: did not happen in the window):
#   rx       the sentinel process got the alarm (MobileSentinel.AlarmRx onReceive)
#   sound    the ring sound started (MobileSentinel.FiringAudio playing)
#   hu_sent  the guardian sent the heads-up broadcast to the main process
#   hu_recv  the main process received it (SentinelJobsInit: JOB_HEADS_UP received)
#   freeze   Android froze the main process (am_freeze / "freezing <pid> com.fidderr.alarmfree"), ms past T, can be before T
#   unfreeze the main process was thawed
#   resumed  the app's activity was resumed (wm_set_resumed_activity)
#   banner   the heads-up of the ring notification was shown (sysui_heads_up_status)
#   bal      lines about a refused background activity start of the app
#   fire_s   first poll at which the app's own state of the alarm was `firing`, seconds past T (1 s grid)
#   screen_s first poll at which the app's activity was on top, seconds past T
#   home_b   seconds before T at which Home was really pressed (the wanted one is the scenario's)
#   end      at the end of the window: turn_off (the ring's own button on screen) / banner (Tap to open) / none
#   procs    the process list lines for the app (dumpsys activity lru) right after Home, 1-2 s after the ring, and at the end

PROBE_ROUNDS="${PROBE_ROUNDS:-20}"
PROBE_WATCH="${PROBE_WATCH:-25}"
PROBE_HEADER="avd moment round T rx sound hu_sent hu_recv freeze unfreeze resumed banner bal fire_s screen_s home_b end procs"

# The alarm's lines of the log, as ms past T. Needs T. Prints: rx sound hu_sent hu_recv freeze unfreeze resumed banner bal
probe_times() { # logfile (logcat -v epoch)
    awk -v t="$T" '
        function note(k, v) { if (!(k in at) || v < at[k]) at[k] = v }
        $1 ~ /^[0-9]+\.[0-9]+$/ && $1 > t - 120 && $1 < t + 120 {
            e = $1
            if ($0 ~ /AlarmRx.*onReceive: instance=a/ && $0 !~ /watchdog/) note("rx", e)
            if ($0 ~ /FiringAudio.*playing /) note("sound", e)
            if ($0 ~ /Guardian.*sendHeadsUpBroadcast/) note("hu_sent", e)
            if ($0 ~ /SentinelJobsInit.*JOB_HEADS_UP received/) note("hu_recv", e)
            if ($0 ~ /am_freeze|freezing [0-9]+ com\.fidderr\.alarmfree( |$)/ && $0 ~ /com\.fidderr\.alarmfree(\]|$| )/ && $0 !~ /alarmfree:sentinel/) note("freeze", e)
            if ($0 ~ /am_unfreeze|unfreezing|Unfreez/ && $0 ~ /com\.fidderr\.alarmfree(\]|$| )/ && $0 !~ /alarmfree:sentinel/) note("unfreeze", e)
            if ($0 ~ /wm_set_resumed_activity.*com\.fidderr\.alarmfree/) note("resumed", e)
            if ($0 ~ /sysui_heads_up_status.*com\.fidderr\.alarmfree/) note("banner", e)
            if ($0 ~ /[Bb]ackground activity (start|launch)|BAL_BLOCK|Abort background activity/) bal++
        }
        END {
            n = split("rx sound hu_sent hu_recv freeze unfreeze resumed banner", ks, " ")
            for (i = 1; i <= n; i++) printf "%s ", (ks[i] in at) ? sprintf("%d", (at[ks[i]] - t) * 1000) : "-"
            printf "%d\n", bal + 0
        }' "$1"
}

# The app's lines of the process list, short, on one line.
probe_procs() {
    dev "dumpsys activity lru | grep -i 'com.fidderr.alarmfree'" 2> /dev/null | # lint-ok: the probe reads the process list
        sed -E 's/^ *#[0-9]+: *//; s/ProcessRecord\{[0-9a-f]+ /{/; s/ +/ /g' | tr '\n' '~' | cut -c1-160
}

# The end of a round, as a person sees it: the ring's own button, or only the banner, or neither.
probe_end() {
    local x
    dev uiautomator dump --windows /sdcard/af_probe.xml > /dev/null 2>&1 # lint-ok: what is on screen with the main process stalled
    x=$(dev cat /sdcard/af_probe.xml 2> /dev/null)
    dev rm -f /sdcard/af_probe.xml
    case "$x" in
        *'text="Turn off"'*) echo turn_off ;;
        *'text="Tap to open"'*) echo banner ;;
        *) echo none ;;
    esac
}

# One round: Home `moment` seconds before the ring. Appends a line to ringprobe.txt, sets PROBE_LINE.
probe_round() { # moment n
    local moment=$1 n=$2 now home_at pid_main st fire_s="-" screen_s="-" procs1 procs2 procs3 end log times
    local took_procs=""
    fresh
    next_minute $((moment + 14))
    seed a "$(alarm_json a "$T_H" "$T_M" "$EVERY_DAY" null)"
    arm a
    # Home at T - moment: wait for that second, then the keys of the device are used.
    until_past $((T - moment))
    adb_ logcat -c -b all
    go_home
    home_at=$(now_s)
    pid_main=$(main_pid)
    procs1=$(probe_procs)
    # Watch until the end of the window.
    while :; do
        now=$(now_s)
        [ "$now" -gt $((T + PROBE_WATCH)) ] && break
        if [ "$now" -ge $((T + 1)) ]; then
            [ -n "$took_procs" ] || { procs2=$(probe_procs); took_procs=1; }
            if [ "$fire_s" = "-" ] && state_is a firing; then fire_s=$((now - T)); fi
            if [ "$screen_s" = "-" ] && top_is_main; then screen_s=$((now - T)); fi
            # Both seen: nothing more to wait for, but the log of the next seconds is not needed either.
            [ "$fire_s" != "-" ] && [ "$screen_s" != "-" ] && break
        else
            sleep 0.3 # lint-ok: the probe waits for the ring
        fi
    done
    procs3=$(probe_procs)
    end=$(probe_end)
    log="$OUT/ringprobe_${CURRENT}_$n.log"
    adb_ logcat -d -b all -v epoch 2> /dev/null | tr -d '\r' |
        grep -E "AlarmRx|FiringAudio|Guardian|SentinelJobsInit|am_freeze|am_unfreeze|freezing|unfreez|wm_set_resumed|wm_create_activity|sysui_heads_up|notification_enqueue.*alarmfree|ackground activity|BAL_|Job heads-up|AlarmKit|ActivityTaskManager.*alarmfree" > "$log"
    times=$(probe_times "$log")
    PROBE_LINE="$AVD_NAME $moment $n $T $times $fire_s $screen_s $((T - home_at)) $end ${procs1:--}|${procs2:--}|${procs3:--}"
    [ -s "$OUT/ringprobe.txt" ] || echo "$PROBE_HEADER" > "$OUT/ringprobe.txt"
    echo "$PROBE_LINE" >> "$OUT/ringprobe.txt"
    # Which of the process rows is the main process (pid at Home) is in the raw lines.
    echo "main pid at Home: $pid_main, device time at Home: $home_at" >> "$log"
}

ring_probe() { # moment
    local moment=$1 i done_n=0 took_up=0 screen=0 late=0 frozen=0 sound=0 fields
    AVD_NAME=${AVD_NAME:-${AVDS:-device}}
    scenario "$CURRENT_PROBE"
    for ((i = 1; i <= PROBE_ROUNDS; i++)); do
        probe_round "$moment" "$i" || true
        done_n=$((done_n + 1))
        # shellcheck disable=SC2086
        set -- $PROBE_LINE
        # fields: 1 avd 2 moment 3 round 4 T 5 rx 6 sound 7 hu_sent 8 hu_recv 9 freeze 10 unfreeze 11 resumed ...
        [ "$6" != "-" ] && sound=$((sound + 1))
        [ "$8" != "-" ] && took_up=$((took_up + 1))
        [ "${15}" != "-" ] && screen=$((screen + 1))
        [ "$9" != "-" ] && frozen=$((frozen + 1))
        [ "${16}" -lt $((moment - 2)) ] 2> /dev/null && late=$((late + 1))
        echo "      round $i: sound=$6 hu_sent=$7 hu_recv=$8 freeze=$9 resumed=${11} fire_s=${14} screen_s=${15} end=${17}"
    done
    pass "Home ${moment}s before the ring, $done_n rounds: sound $sound, main took the ring up $took_up, ring screen on top $screen, main frozen in the window $frozen, Home late $late"
}

t10_ring_probe_smoke() {
    CURRENT_PROBE=t10_ring_probe_smoke
    PROBE_ROUNDS=1 ring_probe 5
    PROBE_ROUNDS=1 ring_probe 1
}
t11_ring_after_home_1s() { CURRENT_PROBE=t11_ring_after_home_1s ring_probe 1; }
t12_ring_after_home_5s() { CURRENT_PROBE=t12_ring_after_home_5s ring_probe 5; }
t13_ring_after_home_15s() { CURRENT_PROBE=t13_ring_after_home_15s ring_probe 15; }
t14_ring_after_home_40s() { CURRENT_PROBE=t14_ring_after_home_40s ring_probe 40; }
