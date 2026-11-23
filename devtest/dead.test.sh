#!/bin/bash
# A device that stops answering (run 45231328491, msd29, s26): the shell of the device and adb itself give no answer. Every call after the
# first one that found out must fail at once, and the loops that read the device's clock must end. No adb: a stub stands for it.
# Usage: bash devtest/dead.test.sh      (about 15 s)
here=$(cd "${BASH_SOURCE[0]%/*}" && pwd)
tmp=$(mktemp -d) || exit 1
trap 'rm -rf "$tmp"' EXIT
bad=0

printf '#!/bin/bash\n[ "$FAKE_ECHO" = ok ] && [[ "$*" == *"echo ok"* ]] && echo ok\nexit 0\n' > "$tmp/fakeadb"
chmod +x "$tmp/fakeadb"

cat > "$tmp/drive.sh" << 'D'
# Env: LIB DIR MODE FAKE_ECHO (ok: adb answers, anything else: it does not).
export OUT="$DIR/out" TMPDIR="$DIR" ADB="$DIR/fakeadb" SERIAL=emulator-5554 ASK_TIMEOUT=1 DEAD_PROBE_S=3 LOCK_AWAY_STEP=1
mkdir -p "$OUT"
. "$LIB"
RESULTS="$OUT/results.txt"
: > "$RESULTS"
CURRENT=sx
ms() { local t=${EPOCHREALTIME/[.,]/}; echo $((t / 1000)); }
# A shell that takes every command and never answers.
{ coproc DEVX { cat > /dev/null; sleep 20; }; } 2> /dev/null
exec 5>&"${DEVX[1]}" 6<&"${DEVX[0]}"
DEV_PID=$DEVX_PID
case "$MODE" in
    dead)
        t0=$(ms)
        dev date +%s > "$DIR/out1" 2> "$DIR/err1"
        echo $? > "$DIR/rc1"
        t1=$(ms)
        dev date +%s > "$DIR/out2" 2> "$DIR/err2"
        echo $? > "$DIR/rc2"
        t2=$(ms)
        adb_ shell echo x > "$DIR/out3" 2> /dev/null
        echo $? > "$DIR/rc3"
        t3=$(ms)
        echo "$((t1 - t0)) $((t2 - t1)) $((t3 - t2))" > "$DIR/times"
        [ -e "$DEAD_FILE" ] && echo flag > "$DIR/flag"
        open_shells 2> /dev/null
        [ -e "$DEAD_FILE" ] || echo cleared > "$DIR/cleared"
        ;;
    slow)
        # The shell does not answer but adb does: not dead, the next call asks again.
        dev date +%s > /dev/null 2> "$DIR/err1"
        [ -e "$DEAD_FILE" ] && echo flag > "$DIR/flag"
        ;;
    clock)
        # No clock from the device: the loops give up after three tries.
        now_s() { echo ""; }
        sound_on() { return 1; }
        sound_off() { return 1; }
        T=0
        await_sound 30 2> "$DIR/err1"
        echo $? > "$DIR/rc1"
        await_silence 30 2> "$DIR/err2"
        echo $? > "$DIR/rc2"
        ;;
esac
D

run_case() { # name mode [VAR=value...]
    local name=$1 mode=$2
    shift 2
    mkdir -p "$tmp/$name"
    cp "$tmp/fakeadb" "$tmp/$name/fakeadb"
    env LIB="$here/lib.sh" DIR="$tmp/$name" MODE="$mode" "$@" timeout 60 bash "$tmp/drive.sh" > "$tmp/$name/stdout" 2>&1
    echo $? > "$tmp/$name/exit"
}
verdict() { # name ok
    if [ "$2" = 1 ]; then echo "PASS  $1"; else echo "FAIL  $1"; bad=1; fi
}
first() { local l=""; [ -f "$1" ] && read -r l < "$1"; echo "$l"; }

run_case dead dead FAKE_ECHO=none
read -r d1 d2 d3 < "$tmp/dead/times"
verdict "a shell and adb that do not answer: the first call gives 124 after the shell's wait and sets the flag" "$([ "$(first "$tmp/dead/rc1")" = 124 ] && [ -e "$tmp/dead/flag" ] && [ "$d1" -ge 900 ] && echo 1)"
verdict "the next calls (dev and adb_) fail at once: 124, no output, under 300 ms each" "$([ "$(first "$tmp/dead/rc2")" = 124 ] && [ "$(first "$tmp/dead/rc3")" = 124 ] && [ ! -s "$tmp/dead/out2" ] && [ "$d2" -lt 300 ] && [ "$d3" -lt 300 ] && echo 1)"
verdict "the NOTE says the device does not answer" "$(grep -q 'the device does not answer' "$tmp/dead/err1" 2> /dev/null && echo 1)"
verdict "opening the shells again clears the flag" "$([ -e "$tmp/dead/cleared" ] && echo 1)"
run_case slow slow FAKE_ECHO=ok
verdict "a shell that does not answer while adb does is not a dead device (no flag)" "$([ ! -e "$tmp/slow/flag" ] && echo 1)"
run_case clock clock FAKE_ECHO=none
verdict "await_sound and await_silence end (rc 1) when the device gives no clock three times, instead of looping on an error" "$([ "$(first "$tmp/clock/exit")" = 0 ] && [ "$(first "$tmp/clock/rc1")" = 1 ] && [ "$(first "$tmp/clock/rc2")" = 1 ] && echo 1)"
exit $bad
