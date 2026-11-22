#!/bin/bash
# The snapshot at a failure (devtest/evidence.sh) against a stand-in device: PASS or FAIL per case.
# Usage: bash devtest/evidence.test.sh      (no adb, no emulator)
#
# The real lib.sh and evidence.sh run, and the real text of the script for the device. `adb` is a stand-in that runs that text
# in a local bash whose dumpsys, uiautomator, run-as and screencap answer from files, and the app is a loop that takes
# files/debug_request.txt. Per case: FOCUS (the window with the focus), APP (answer | silent | taken | dead), HANG (adb does
# not answer), SERIAL.
here=$(cd "${BASH_SOURCE[0]%/*}" && pwd)
tmp=$(mktemp -d) || exit 1
APP_PIDS=""
cleanup() {
    local p
    for p in $APP_PIDS; do kill "$p" 2> /dev/null; done
    rm -rf "$tmp"
}
trap cleanup EXIT
bad=0
verdict() { # name ok detail
    if [ "$2" = 1 ]; then echo "PASS  $1"; else echo "FAIL  $1  $3"; bad=1; fi
}
bin="$tmp/bin"
mkdir -p "$bin"

cat > "$bin/adb" << 'E'
#!/bin/bash
# adb -s SERIAL shell SCRIPT | adb -s SERIAL exec-out screencap -p
echo "$*" >> "$FAKE/adb.calls"
[ -z "${HANG:-}" ] || exec sleep 30
shift 2
export PATH="$FAKE/bin:$PATH"
case "$1" in
    shell) exec bash -c "$2" ;;
    exec-out) shift; exec "$@" ;;
esac
E
cat > "$bin/dumpsys" << 'E'
#!/bin/bash
case "$*" in
    "window displays") echo "  mCurrentFocus=${FOCUS:-Window{1 u0 com.fidderr.alarmfree/android.app.NativeActivity}}" ;;
    "activity activities") echo "  topResumedActivity=ActivityRecord{1 u0 com.fidderr.alarmfree/android.app.NativeActivity t5}" ;;
    power) echo "  mWakefulness=Awake" ;;
    window) echo "    isKeyguardShowing=false"; echo "  mCurrentFocus=x" ;;
    input_method) echo "  mInputShown=false" ;;
esac
E
cat > "$bin/uiautomator" << 'E'
#!/bin/bash
eval "f=\${$#}"
printf '%s' '<?xml version="1.0"?><hierarchy rotation="0"><node text="Alarms" bounds="[0,0][10,10]"/></hierarchy>' > "$f"
E
cat > "$bin/run-as" << 'E'
#!/bin/bash
shift
exec "$@"
E
cat > "$bin/screencap" << 'E'
#!/bin/bash
printf '\x89PNG\r\n\x1a\nfake'
E
chmod +x "$bin"/*

# The stand-in app: takes a request and answers (APP=answer), takes it and never answers (taken), or is not there (silent).
cat > "$tmp/app.sh" << 'E'
cd "$1/files" || exit 1
while :; do
    if mv debug_request.txt debug_request.taking 2> /dev/null; then
        read -r line < debug_request.taking
        rm -f debug_request.taking
        id=${line%% *}
        cmd=${line#* }
        echo "$line" >> ../requests.log
        [ -e ../taken ] && continue
        case "$cmd" in
            ui:ready) ans="screen=Alarms ringing=false" ;;
            ui:texts) ans='[{"text":"Alarms"}]' ;;
            *) ans=OK ;;
        esac
        printf '%s\n' "$ans" > "debug_answer_$id.tmp"
        mv "debug_answer_$id.tmp" "debug_answer_$id.txt"
    fi
    sleep 0.02
done
E

# Env: FAKE FOCUS HANG SERIAL EVIDENCE_DEADLINE. Sourced lib.sh reads OUT and ADB.
cat > "$tmp/drive.sh" << 'E'
export OUT="$FAKE/out" TMPDIR="$FAKE" ADB="$FAKE/bin/adb"
export SERIAL="${SERIAL:-emulator-5554}"
mkdir -p "$OUT"
. "$LIB"
DATA="$FAKE/data"
RESULTS="$OUT/results.txt"
: > "$RESULTS"
write_log() { :; }
CURRENT=s99_demo
SCENARIO_START=$SECONDS
case "$MODE" in
    twice)
        fail "first problem" > "$FAKE/stdout"
        echo "MARK" > "$OUT/s99_demo.fail.txt"
        rm -f "$OUT/s99_demo.fail.png" "$OUT/s99_demo.fail.xml"
        fail "second problem" >> "$FAKE/stdout"
        ;;
    one) fail "the alarm did not ring" > "$FAKE/stdout"; echo "rc=$?" >> "$FAKE/stdout" ;;
    infra) evidence_snapshot infra s99_demo "the device restarted" ;;
    timed)
        a=${EPOCHREALTIME/[.,]/}
        fail "x" > "$FAKE/stdout"
        b=${EPOCHREALTIME/[.,]/}
        echo "$(((b - a) / 1000))" > "$FAKE/ms"
        echo after >> "$FAKE/stdout"
        ;;
esac
E

# run_case MODE [VAR=value...]: a fresh folder, one run. The app (when asked for) runs for the case.
run_case() {
    local mode=$1 app=answer v
    shift
    FAKE="$tmp/case$((++n))"
    mkdir -p "$FAKE/data/files" "$FAKE/out"
    ln -s "$bin" "$FAKE/bin"
    for v in "$@"; do case "$v" in APP=*) app=${v#APP=} ;; esac; done
    [ "$app" = dead ] && : > "$FAKE/data/files/debug_dead"
    [ "$app" = taken ] && : > "$FAKE/data/taken"
    local pid=""
    case "$app" in answer | taken) bash "$tmp/app.sh" "$FAKE/data" & pid=$! ; APP_PIDS="$APP_PIDS $pid" ;; esac
    env "FAKE=$FAKE" "EVIDENCE_TMP=$FAKE" "LIB=$here/lib.sh" "MODE=$mode" "$@" bash "$tmp/drive.sh" > "$FAKE/drive.out" 2>&1
    [ -z "$pid" ] || kill "$pid" 2> /dev/null
    OUTD="$FAKE/out"
}
has() { [ -e "$1" ]; }
n=0

run_case one
t="$OUTD/s99_demo.fail.txt"
ok=0
has "$t" && has "$OUTD/s99_demo.fail.png" && has "$OUTD/s99_demo.fail.xml" && ok=1
verdict "the first fail writes the text, the dump and the picture" "$ok" "$(ls "$OUTD")"
ok=0
if has "$t"; then
    c=$(< "$t")
    [[ $c == *"what: the alarm did not ring"* && $c == *"com.fidderr.alarmfree/android.app.NativeActivity"* && $c == *"isKeyguardShowing=false"* && $c == *"mInputShown=false"* && $c == *"screen=Alarms ringing=false"* && $c == *'"text":"Alarms"'* ]] && ok=1
fi
verdict "the text has what was asked: focus, lock screen, keyboard, the app's own words" "$ok" "${c:0:400}"
ok=0
has "$OUTD/s99_demo.fail.xml" && [[ $(< "$OUTD/s99_demo.fail.xml") == *'text="Alarms"'* ]] && ok=1
verdict "the dump is the screen's xml" "$ok" ""
ok=0
has "$OUTD/s99_demo.fail.png" && [[ $(head -c 4 "$OUTD/s99_demo.fail.png" | tr -d '\0') == *PNG* ]] && ok=1
verdict "the picture is a png" "$ok" ""
out=$(< "$FAKE/stdout")
exp="FAIL  s99_demo  [0s] the alarm did not ring"$'\n'"rc=0"
res=$(< "$OUTD/results.txt")
ok=0
[ "$out" = "$exp" ] && [ "$res" = "FAIL  s99_demo  [0s] the alarm did not ring" ] && ok=1
verdict "result line, results file and exit status are the same as without a snapshot" "$ok" "[$out] [$res]"
ok=0
[[ $(ls "$OUTD") != *raw* ]] && ok=1
verdict "no scratch file is left" "$ok" ""

run_case twice
ok=0
c=""
has "$OUTD/s99_demo.fail.txt" && c=$(< "$OUTD/s99_demo.fail.txt")
[ "$c" = MARK ] && ! has "$OUTD/s99_demo.fail.png" && ! has "$OUTD/s99_demo.fail.xml" && ok=1
verdict "a second fail of the same scenario writes nothing more" "$ok" "[${c:0:60}] $(ls "$OUTD")"
ok=0
[ "$(grep -c '^FAIL' "$OUTD/results.txt")" = 2 ] && ok=1
verdict "both fails still have their result line" "$ok" ""

run_case infra
ok=0
has "$OUTD/s99_demo.infra.txt" && has "$OUTD/s99_demo.infra.png" && has "$OUTD/s99_demo.infra.xml" && [[ $(< "$OUTD/s99_demo.infra.txt") == *"what: the device restarted"* ]] && ok=1
verdict "an INFRA writes the same three files" "$ok" "$(ls "$OUTD")"

run_case timed HANG=1 EVIDENCE_DEADLINE=2
ms=$(< "$FAKE/ms")
ok=0
[ "$ms" -ge 1900 ] && [ "$ms" -lt 3500 ] && [[ $(< "$FAKE/stdout") == *after* ]] && ok=1
verdict "a device that does not answer costs no more than the deadline, the scenario goes on (${ms} ms of 2000)" "$ok" ""
ok=0
has "$OUTD/s99_demo.fail.txt" && [[ $(< "$OUTD/s99_demo.fail.txt") == *"did not answer"* ]] && ! has "$OUTD/s99_demo.fail.png" && ok=1
verdict "and it is written down that the device did not answer" "$ok" ""

run_case one APP=taken
ok=0
c=$(< "$OUTD/s99_demo.fail.txt")
[[ $c == *"no answer from the app within 2 s"* && ! -e "$FAKE/data/files/debug_dead" ]] && ok=1
verdict "an app that does not answer is written down and the channel is not closed" "$ok" "${c:0:600}"
ok=0
has "$OUTD/s99_demo.fail.png" && has "$OUTD/s99_demo.fail.xml" && ok=1
verdict "and the picture and the dump are still taken" "$ok" ""
run_case timed APP=taken
ms=$(< "$FAKE/ms")
ok=0
[ "$ms" -lt 5000 ] && ok=1
verdict "an app that does not answer costs about 2 s, not more (${ms} ms)" "$ok" ""

run_case one APP=silent
c=$(< "$OUTD/s99_demo.fail.txt")
ok=0
[[ $c == *"did not take the request within 2 s (taken back)"* && ! -e "$FAKE/data/files/debug_request.txt" ]] && ok=1
verdict "an app that is not there: the request is taken back" "$ok" "${c:0:600}"

run_case one APP=dead
c=$(< "$OUTD/s99_demo.fail.txt")
ok=0
[[ $c == *"not asked: an earlier request got no answer"* ]] && [ ! -e "$FAKE/data/files/debug_request.txt" ] && ok=1
verdict "a channel that is closed already is not asked" "$ok" "${c:0:600}"

run_case one SERIAL=R58M12345A FOCUS='Window{2 u0 NotificationShade}'
ok=0
c=$(< "$OUTD/s99_demo.fail.txt")
! has "$OUTD/s99_demo.fail.png" && ! has "$OUTD/s99_demo.fail.xml" && [[ $c == *"no picture and no dump"* ]] && ok=1
verdict "a phone with the shade in front: no picture and no dump" "$ok" "$(ls "$OUTD")"
grep -q "screencap" "$FAKE/adb.calls" && ok=0 || :
verdict "and the picture was not even asked for" "$ok" ""

run_case one SERIAL=R58M12345A FOCUS='Window{3 u0 com.google.android.documentsui/com.android.documentsui.picker.PickActivity}'
ok=0
! has "$OUTD/s99_demo.fail.png" && ! has "$OUTD/s99_demo.fail.xml" && ok=1
verdict "a phone with a file picker in front: no picture and no dump" "$ok" "$(ls "$OUTD")"

run_case one SERIAL=R58M12345A
ok=0
has "$OUTD/s99_demo.fail.png" && has "$OUTD/s99_demo.fail.xml" && ok=1
verdict "a phone with AlarmFree in front: picture and dump" "$ok" "$(ls "$OUTD")"

run_case one SERIAL=emulator-5554 FOCUS='Window{2 u0 NotificationShade}'
ok=0
has "$OUTD/s99_demo.fail.png" && has "$OUTD/s99_demo.fail.xml" && ok=1
verdict "an emulator with the shade in front: picture and dump (no such rule there)" "$ok" ""

exit "$bad"
