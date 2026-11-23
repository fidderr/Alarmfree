#!/bin/bash
# What was on the device when a scenario failed (or an INFRA stood). Sourced by lib.sh.
#
# evidence_snapshot <fail|infra> <scenario> <what went wrong>
# At the first FAIL of a scenario (and at an INFRA) it saves next to the other files of the scenario:
#   <scenario>.fail.txt  focus, what is on top, lock screen, keyboard, and what the app says about itself (ui ready, ui texts)
#   <scenario>.fail.xml  uiautomator dump of the screen (shows the screens of Android itself)
#   <scenario>.fail.png  screenshot
# (`.infra.` instead of `.fail.` for an INFRA.) Once per kind and attempt of a scenario. It only reads: nothing on the device is
# changed, nothing is printed, no result line and no exit code are touched, and it never takes longer than EVIDENCE_DEADLINE
# seconds in all.
#   One device call (text, the app's answers, the dump), then one for the picture. It does not use the open shells (a stalled
#   shell would hold the run) and not the channel: the app is asked for at most 2 s per question, and a question that gets no
#   answer is written down as such. It never closes the channel (no debug_dead), so the scenario goes on as before.
#   A real phone: no picture and no dump unless AlarmFree, its keyboard or its permission dialogs have the focus. The owner's
#   rule: never capture the notification shade or a file picker.
EVIDENCE_DEADLINE="${EVIDENCE_DEADLINE:-20}"
EVIDENCE_KEYS=" "
EVIDENCE_TMP="${EVIDENCE_TMP:-/data/local/tmp}"

_ev_ms() {
    if [ -n "${EPOCHREALTIME:-}" ]; then
        local t=${EPOCHREALTIME/[.,]/}
        EV_NOW=$((t / 1000))
    else
        EV_NOW=$((SECONDS * 1000))
    fi
}

# The part of $1 (the whole answer) between "@@NAME" and the next "@@" line.
_ev_part() { # answer name
    local s=${1#*"@@$2"$'\n'}
    [ "$s" != "$1" ] || return 1
    case "$s" in *$'\n@@'*) s=${s%%$'\n@@'*} ;; esac
    printf '%s' "$s"
}

# The parts of the script for the shell on the device. The part for the app's shell has no single quotes: it is passed in them.
_ev_app_script() { # id
    local id=$1
    printf '%s' "cd $DATA/files 2> /dev/null || { echo \"the app has no files folder\"; exit 0; }
if [ -e debug_dead ]; then echo \"not asked: an earlier request got no answer and the channel is closed\"; exit 0; fi
for n in 1 2 3; do
    case \$n in 1) c=ui:ready ;; 2) c=ui:texts ;; *) c=ui:trace ;; esac
    id=$id\$n
    a=debug_answer_\$id.txt
    printf \"%s\\n\" \"\$id \$c\" > debug_request.tmp && mv debug_request.tmp debug_request.txt
    end=\$((SECONDS + 2))
    while [ ! -e \$a ] && [ \$SECONDS -lt \$end ]; do sleep 0.01; done
    if [ -e \$a ]; then
        echo \"== \$c\"; cat \$a; rm -f \$a
    else
        if mv debug_request.txt debug_request.withdrawn 2> /dev/null; then
            echo \"== \$c: the app did not take the request within 2 s (taken back)\"
        else
            echo \"== \$c: no answer from the app within 2 s (it took the request; the channel is left open)\"
        fi
        break
    fi
done"
}

_ev_device_script() { # phone(0|1) id
    local app
    app=$(_ev_app_script "$2")
    printf '%s' "f=\$(dumpsys window displays 2> /dev/null | grep -m1 mCurrentFocus)
ok=1
if [ $1 = 1 ]; then
    ok=0
    case \"\$f\" in *com.fidderr.alarmfree*|*nputMethod*|*inputmethod*|*permissioncontroller*|*packageinstaller*|*RequestIgnoreBatteryOptimizations*) ok=1 ;; esac
fi
echo \"@@PICTURE \$ok\"
echo \"@@FOCUS\"; echo \"\$f\"
echo \"@@TOP\"; dumpsys activity activities 2> /dev/null | grep -E \"topResumedActivity|ResumedActivity:\" | head -2
echo \"@@LOCK\"; dumpsys power 2> /dev/null | grep -m1 mWakefulness=
dumpsys window 2> /dev/null | grep -E \"isKeyguardShowing|mShowingLockscreen|mKeyguardGoingAway|isKeyguardLocked|mDreamingLockscreen|mOccluded|mCurrentFocus\" | head -8
echo \"@@KEYBOARD\"; dumpsys input_method 2> /dev/null | grep -E \"mInputShown|mShowRequested|mImeWindowVis|mCurMethodId\" | head -4
echo \"@@APP\"
run-as $PKG sh -c '$app'
if [ \$ok = 1 ]; then
    echo \"@@XML\"
    F=$EVIDENCE_TMP/af_fail.xml
    rm -f \$F
    uiautomator dump --windows \$F > /dev/null 2>&1
    [ -s \$F ] || uiautomator dump \$F > /dev/null 2>&1
    cat \$F 2> /dev/null; rm -f \$F
fi
echo
echo \"@@END\""
}

evidence_snapshot() { # kind scenario what
    local kind=$1 name=$2 what=$3 key
    key=" $kind:$name@${SCENARIO_START:-0} "
    case "$EVIDENCE_KEYS" in *"$key"*) return 0 ;; esac
    EVIDENCE_KEYS+="${key# }"
    {
        local t0 phone=0 id raw out base xml picture pic_ok left secs
        _ev_ms
        t0=$EV_NOW
        on_emulator || phone=1
        id=$$-${EPOCHREALTIME:-$SECONDS}
        id=${id//[.,]/}
        base="$OUT/$name.$kind"
        raw="$base.raw"
        timeout "$EVIDENCE_DEADLINE" "$ADB" -s "$SERIAL" shell "$(_ev_device_script "$phone" "$id")" > "$raw" 2> /dev/null
        out=$(< "$raw")
        out=${out//$'\r'/}
        rm -f "$raw"
        pic_ok=""
        if [[ $out == *"@@PICTURE 1"* ]]; then pic_ok=1; fi
        {
            echo "$kind  $name  at $(date +%T)  on $SERIAL"
            echo "what: $what"
            if [ -z "$out" ]; then
                echo "the device did not answer within ${EVIDENCE_DEADLINE}s"
            else
                local p
                for p in FOCUS TOP LOCK KEYBOARD APP; do
                    echo
                    echo "--- $p"
                    _ev_part "$out" "$p"
                    echo
                done
                [[ $out == *"@@PICTURE 0"* ]] && {
                    echo
                    echo "--- no picture and no dump: on a phone, something other than AlarmFree has the focus"
                }
            fi
        } > "$base.txt"
        if xml=$(_ev_part "$out" XML) && [ -n "$xml" ]; then printf '%s\n' "$xml" > "$base.xml"; fi
        if [ -n "$pic_ok" ]; then
            _ev_ms
            left=$((EVIDENCE_DEADLINE * 1000 - (EV_NOW - t0)))
            if [ "$left" -gt 500 ]; then
                [ "$left" -le 10000 ] || left=10000
                secs="$((left / 1000)).$(printf %03d $((left % 1000)))"
                picture="$base.png"
                timeout "$secs" "$ADB" -s "$SERIAL" exec-out screencap -p > "$picture" 2> /dev/null
                [ -s "$picture" ] || rm -f "$picture"
            else
                echo "no picture: no time left" >> "$base.txt"
            fi
        fi
    } 2> /dev/null || true
    return 0
}
