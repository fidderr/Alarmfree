#!/bin/bash
# The ring-screen probe: an alarm rings while the app sits in the background, and the probe records when the main process took
# the ring up and when the ring screen came on. By name only (like selftest.sh), sourced by matrix.sh:
#   SCENARIOS="t11_ring_after_home_1s t12_ring_after_home_5s t13_ring_after_home_15s t14_ring_after_home_40s"
#   t15_ring_after_quick_home   Home right after the app was started (what s63 does: the window is not up yet), ring 6 s past start
#   t16_ring_over_another_app   Home 34 s before the ring, then Settings in front, so the app is a background app of the past
#   t17_ring_after_home_90s     Home 90 s before the ring
#   t10_ring_probe_smoke        2 rounds each of 5, 15, 1, 40 s, to see that the probe itself reads.
# For t11 to t14 and t17 `Home` is pressed N seconds before the ring (N = 1, 5, 15, 40, 90), PROBE_ROUNDS times (20), and each round
# is watched for PROBE_WATCH seconds (25) past the ring. One line per round in $OUT/ringprobe.txt (columns in PROBE_HEADER), the raw
# lines of the log that the numbers come from in $OUT/ringprobe_<name>_m<moment>_<n>.log, and a PASS line with the counts. The probe
# passes when it measured; what the numbers mean is read from them.
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
#   home_b   seconds before T at which Home was really pressed
#   end      at the end of the window: turn_off (the ring's own button on screen) / app_on_top / banner (Tap to open) / none
#   st_pre   the main process as the system lists it (dumpsys activity lru) at T-1 s, then at T+1 s, then at the end

PROBE_ROUNDS="${PROBE_ROUNDS:-20}"
PROBE_WATCH="${PROBE_WATCH:-25}"
PROBE_HEADER="avd kind moment round T rx sound hu_sent hu_recv freeze unfreeze resumed banner bal fire_s screen_s home_b end st_pre st_post st_end"

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
            if ($0 ~ /ackground activity (start|launch).*(blocked|aborted|not allowed)|BAL_BLOCK|Abort background activity/) bal++
        }
        END {
            n = split("rx sound hu_sent hu_recv freeze unfreeze resumed banner", ks, " ")
            for (i = 1; i <= n; i++) printf "%s ", (ks[i] in at) ? sprintf("%d", (at[ks[i]] - t) * 1000) : "-"
            printf "%d\n", bal + 0
        }' "$1"
}

# The main process (not the sentinel one) as the system lists it: its importance and process state, on one word.
probe_main_state() {
    local s
    s=$(dev "dumpsys activity lru | grep 'com.fidderr.alarmfree/u'" 2> /dev/null | # lint-ok: the probe reads the process list
        sed -nE 's/^ *#[0-9]+: *(.*) [0-9]+:com\.fidderr\.alarmfree\/u.*/\1/p' | head -1 | tr ' ' '_')
    echo "${s:-gone}"
}

# The end of a round, as a person sees it: the app's screen on top (and its own button), or only the banner, or neither.
probe_end() {
    local x
    if top_is_main; then
        if UI_QUIET=1 await_text "Turn off" 3; then echo turn_off; else echo app_on_top; fi
        return
    fi
    dev uiautomator dump --windows /sdcard/af_probe.xml > /dev/null 2>&1 # lint-ok: what is on screen with the main process stalled
    x=$(dev cat /sdcard/af_probe.xml 2> /dev/null)
    dev rm -f /sdcard/af_probe.xml
    case "$x" in
        *'text="Tap to open"'*) echo banner ;;
        *) echo none ;;
    esac
}

# One round. kind: home (Home `moment` seconds before the ring), quick (Home right after the app was started, as s63 does),
# app (Home `moment` seconds before the ring, then Settings in front). Appends a line to ringprobe.txt, sets PROBE_LINE.
probe_round() { # kind moment n
    local kind=$1 moment=$2 n=$3 now home_at pid_main end fire_s="-" screen_s="-" st_pre="-" st_post="-" st_end log times
    fresh
    case "$kind" in
        quick) next_minute 6 ;;
        *) next_minute $((moment + 14)) ;;
    esac
    seed a "$(alarm_json a "$T_H" "$T_M" "$EVERY_DAY" null)"
    arm a
    if [ "$kind" != quick ]; then
        # Home at T - moment: wait for that second.
        until_past $((T - moment))
    fi
    adb_ logcat -c -b all
    go_home
    home_at=$(now_s)
    pid_main=$(main_pid)
    [ "$kind" != app ] || dev am start -a android.settings.SETTINGS > /dev/null
    # Watch until the end of the window.
    while :; do
        now=$(now_s)
        [ "$now" -gt $((T + PROBE_WATCH)) ] && break
        if [ "$now" -ge $((T - 1)) ] && [ "$st_pre" = "-" ]; then st_pre=$(probe_main_state); fi
        if [ "$now" -ge $((T + 1)) ]; then
            [ "$st_post" != "-" ] || st_post=$(probe_main_state)
            if [ "$fire_s" = "-" ] && state_is a firing; then fire_s=$((now - T)); fi
            if [ "$screen_s" = "-" ] && top_is_main; then screen_s=$((now - T)); fi
            [ "$fire_s" != "-" ] && [ "$screen_s" != "-" ] && break
        else
            sleep 0.3 # lint-ok: the probe waits for the ring
        fi
    done
    [ "$st_pre" != "-" ] || st_pre=$(probe_main_state)
    st_end=$(probe_main_state)
    end=$(probe_end)
    log="$OUT/ringprobe_${CURRENT}_m${moment}_$n.log"
    adb_ logcat -d -b all -v epoch 2> /dev/null | tr -d '\r' |
        grep -E "AlarmRx|FiringAudio|Guardian|SentinelJobsInit|am_freeze|am_unfreeze|freezing|unfreez|wm_set_resumed|wm_create_activity|sysui_heads_up|notification_enqueue.*alarmfree|ackground activity|BAL_|Job heads-up|AlarmKit|ActivityTaskManager.*alarmfree" > "$log"
    times=$(probe_times "$log")
    # A round where nothing rang at all: what the alarm manager and the app say about the alarm.
    case "$times" in
        "- "*)
            {
                echo "NO RING: armed at ms: $(armed_at_ms a), T*1000 = $((T * 1000)), device time now $(now_s)"
                dev "dumpsys alarm | grep -i -B1 -A3 alarmfree | head -40"
                as_app "tail -n 8 files/sentinel/diag.log"
            } >> "$log" 2>&1
            ;;
    esac
    PROBE_LINE="$AVD_NAME $kind $moment $n $T $times $fire_s $screen_s $((T - home_at)) $end $st_pre $st_post $st_end"
    [ -s "$OUT/ringprobe.txt" ] || echo "$PROBE_HEADER" > "$OUT/ringprobe.txt"
    echo "$PROBE_LINE" >> "$OUT/ringprobe.txt"
    echo "main pid at Home: $pid_main, device time at Home: $home_at" >> "$log"
    [ "$kind" != app ] || dev am force-stop com.android.settings
}

ring_probe() { # kind moment
    local kind=$1 moment=$2 i done_n=0 took_up=0 screen=0 late=0 frozen=0 sound=0
    AVD_NAME=${AVD_NAME:-${AVDS:-device}}
    scenario "$CURRENT_PROBE"
    for ((i = 1; i <= PROBE_ROUNDS; i++)); do
        probe_round "$kind" "$moment" "$i" || true
        done_n=$((done_n + 1))
        # shellcheck disable=SC2086
        set -- $PROBE_LINE
        # 1 avd 2 kind 3 moment 4 round 5 T 6 rx 7 sound 8 hu_sent 9 hu_recv 10 freeze 11 unfreeze 12 resumed 13 banner 14 bal
        # 15 fire_s 16 screen_s 17 home_b 18 end 19 st_pre 20 st_post 21 st_end
        [ "$7" != "-" ] && sound=$((sound + 1))
        [ "$9" != "-" ] && took_up=$((took_up + 1))
        [ "${16}" != "-" ] && screen=$((screen + 1))
        [ "${10}" != "-" ] && frozen=$((frozen + 1))
        [ "$kind" = quick ] || { [ "${17}" -lt $((moment - 2)) ] 2> /dev/null && late=$((late + 1)); }
        echo "      round $i: pre=${19} sound=$7 hu_sent=$8 hu_recv=$9 freeze=${10} resumed=${12} fire_s=${15} screen_s=${16} end=${18}"
    done
    pass "$kind, Home ${moment}s before the ring, $done_n rounds: sound $sound, main took the ring up $took_up, ring screen on top $screen, main frozen in the window $frozen, Home late $late"
}

t10_ring_probe_smoke() {
    local m
    CURRENT_PROBE=t10_ring_probe_smoke
    for m in 5 15 1 40; do PROBE_ROUNDS=2 ring_probe home "$m"; done
}
t11_ring_after_home_1s() { CURRENT_PROBE=t11_ring_after_home_1s ring_probe home 1; }
t12_ring_after_home_5s() { CURRENT_PROBE=t12_ring_after_home_5s ring_probe home 5; }
t13_ring_after_home_15s() { CURRENT_PROBE=t13_ring_after_home_15s ring_probe home 15; }
t14_ring_after_home_40s() { CURRENT_PROBE=t14_ring_after_home_40s ring_probe home 40; }
t15_ring_after_quick_home() { CURRENT_PROBE=t15_ring_after_quick_home ring_probe quick 0; }
t16_ring_over_another_app() { CURRENT_PROBE=t16_ring_over_another_app ring_probe app 34; }
t17_ring_after_home_90s() { CURRENT_PROBE=t17_ring_after_home_90s ring_probe home 90; }
