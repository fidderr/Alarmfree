#!/bin/bash
# Sabotage mode for the device suite, sourced by lib.sh. It does nothing unless CHAOS=1.
# A suite that is stable must stay green while its own actions go wrong on purpose:
#   CHAOS_DROP     percent of real touches (input tap, swipe, touchscreen, draganddrop) that are not sent (30)
#   CHAOS_TOUCH_MS before a touch that is sent: a random wait of 0 to this, the gap in which a layout can move (1500)
#   CHAOS_STEP_MS  before every `ui` command: a random wait of 0 to this (300)
#   CHAOS_HOG      busy loops inside the emulator for the whole run, so the app gets less processor time (2)
#   CHAOS_SEED     a number; unset: one is chosen and printed. The same seed and the same order of calls decide the same.
# Keys and typed text are never dropped. Every dropped touch is a line in $OUT/chaos.txt.
# Helpers run inside $( ), so what must outlive a call (the generator and the counts) is in a file, and nothing here
# starts a program per call.

CHAOS_ON=""
CHAOS_FD=""
CHAOS_HOG_PIDS=/data/local/tmp/af_chaos_hog.pids

# Seconds from milliseconds, without a program. A wait is a read that times out on a pipe nobody writes to.
chaos_wait() { # ms
    local t
    [ "$1" -gt 0 ] || return 0
    printf -v t '%d.%03d' $(($1 / 1000)) $(($1 % 1000))
    if [ -n "$CHAOS_FD" ]; then
        read -r -t "$t" -u "$CHAOS_FD" _ || :
    else
        sleep "$t"
    fi
}

chaos_load() { read -r CHAOS_S CHAOS_SENT CHAOS_DROPPED CHAOS_SEED < "$CHAOS_STATE"; }
chaos_save() { printf '%s %s %s %s\n' "$CHAOS_S" "$CHAOS_SENT" "$CHAOS_DROPPED" "$CHAOS_SEED" > "$CHAOS_STATE"; }
# CHAOS_R is a number from 0 to $1 - 1.
chaos_roll() {
    CHAOS_S=$(((CHAOS_S * 1103515245 + 12345) & 0x7fffffff))
    CHAOS_R=$(((CHAOS_S >> 8) % $1))
}

# dev calls this with its arguments. Returns 1 when the touch is dropped (dev then does not send it).
chaos_touch() {
    case "$*" in
        "input tap"* | "input swipe"* | "input touchscreen"* | "input draganddrop"*) ;;
        *) return 0 ;;
    esac
    chaos_load
    chaos_roll 100
    if [ "$CHAOS_R" -lt "$CHAOS_DROP" ]; then
        CHAOS_DROPPED=$((CHAOS_DROPPED + 1))
        chaos_save
        printf '%s | %s\n' "${CURRENT:-?}" "$*" >> "$OUT/chaos.txt"
        return 1
    fi
    CHAOS_SENT=$((CHAOS_SENT + 1))
    chaos_roll $((CHAOS_TOUCH_MS + 1))
    chaos_save
    chaos_wait "$CHAOS_R"
    return 0
}

# ui calls this before every command.
chaos_step() {
    chaos_load
    chaos_roll $((CHAOS_STEP_MS + 1))
    chaos_save
    chaos_wait "$CHAOS_R"
}

chaos_footer() {
    [ -n "$CHAOS_ON" ] || return 0
    chaos_load
    echo "CHAOS  sabotage on, seed $CHAOS_SEED, touches sent $CHAOS_SENT, touches dropped $CHAOS_DROPPED, busy loops alive at the end ${CHAOS_HOG_ALIVE:-?} of $CHAOS_HOG"
}

# The busy loops. They carry a marker in their command line, and only a process with the marker is stopped (a number
# from an earlier boot can belong to something else now). A restart of the emulator takes them along: scenario() asks
# again at its start.
chaos_hog_script() { # stop|ensure
    local n=$CHAOS_HOG f=$CHAOS_HOG_PIDS
    local stop="k=0; for p in \$(cat $f 2> /dev/null); do grep -q af_chaos_hog /proc/\$p/cmdline 2> /dev/null && kill \$p && k=\$((k + 1)); done; rm -f $f"
    if [ "$1" = stop ]; then
        echo "$stop; echo \$k"
        return
    fi
    echo "live=0; for p in \$(cat $f 2> /dev/null); do grep -q af_chaos_hog /proc/\$p/cmdline 2> /dev/null && live=\$((live + 1)); done; if [ \$live -ne $n ]; then $stop; i=0; while [ \$i -lt $n ]; do sh -c 'while :; do :; done # af_chaos_hog' > /dev/null 2>&1 & echo \$! >> $f; i=\$((i + 1)); done; fi"
}
# Never on a phone: its owner's battery is not part of the test.
chaos_hog_ensure() { [ -z "$CHAOS_ON" ] || case "$SERIAL" in emulator-*) dev "$(chaos_hog_script ensure)" > /dev/null 2>&1 ;; esac; }
# CHAOS_HOG_ALIVE: how many were still running when they were stopped, for the footer (loops that died with their
# shell would make the sabotage weaker than it says).
chaos_hog_stop() {
    [ -n "$CHAOS_ON" ] || return 0
    case "$SERIAL" in emulator-*) ;; *) return 0 ;; esac
    CHAOS_HOG_ALIVE=$(dev "$(chaos_hog_script stop)" 2> /dev/null)
    CHAOS_HOG_ALIVE=${CHAOS_HOG_ALIVE//[!0-9]/}
}
# At the start of a run: what an earlier run left is stopped first.
chaos_hog_start() {
    [ -n "$CHAOS_ON" ] || return 0
    chaos_hog_stop
    chaos_hog_ensure
}

# A setting is a whole number of at most 9 digits (a leading zero is not octal); anything else is the default.
chaos_num() { # name default
    local v=${!1:-}
    case "$v" in '' | *[!0-9]* | ??????????*) v=$2 ;; esac
    printf -v "$1" '%s' $((10#$v))
}

chaos_init() {
    [ "${CHAOS:-}" = 1 ] || return 0
    CHAOS_ON=1
    chaos_num CHAOS_DROP 30
    chaos_num CHAOS_TOUCH_MS 1500
    chaos_num CHAOS_STEP_MS 300
    chaos_num CHAOS_HOG 2
    CHAOS_STATE="$OUT/chaos.state"
    local fifo="${TMPDIR:-/tmp}/af_chaos_wait_${SERIAL:-none}" t0 t1
    [ -p "$fifo" ] || mkfifo "$fifo" 2> /dev/null
    # shellcheck disable=SC2261
    { exec {CHAOS_FD}<> "$fifo"; } 2> /dev/null || CHAOS_FD=""
    # Where this does not wait (a bash without EPOCHREALTIME is not asked): the plain sleep.
    if [ -n "$CHAOS_FD" ]; then
        t0=${EPOCHREALTIME/[.,]/}
        read -r -t 0.2 -u "$CHAOS_FD" _ || :
        t1=${EPOCHREALTIME/[.,]/}
        case "$t0$t1" in '' | *[!0-9]*) ;; *) [ $(((t1 - t0) / 1000)) -ge 150 ] || CHAOS_FD="" ;; esac
    fi
    # A script that runs another (one.sh, all.sh and the matrix) shares one run: the first to start makes it.
    if [ "${CHAOS_RUN:-}" = "$OUT" ] && [ -s "$CHAOS_STATE" ]; then
        chaos_load
        return 0
    fi
    case "${CHAOS_SEED:-}" in '' | *[!0-9]* | ??????????*) CHAOS_SEED=$(((RANDOM << 15 | RANDOM) + 1)) ;; *) CHAOS_SEED=$((10#$CHAOS_SEED)) ;; esac
    CHAOS_S=$(((CHAOS_SEED * 2654435761 + 1) & 0x7fffffff))
    CHAOS_SENT=0 CHAOS_DROPPED=0
    chaos_save
    : > "$OUT/chaos.txt"
    export CHAOS_SEED CHAOS_RUN="$OUT"
    echo "CHAOS  sabotage on: seed $CHAOS_SEED, $CHAOS_DROP% of touches dropped, touches wait up to $CHAOS_TOUCH_MS ms, steps up to $CHAOS_STEP_MS ms, $CHAOS_HOG busy loops on the device"
}
chaos_init
