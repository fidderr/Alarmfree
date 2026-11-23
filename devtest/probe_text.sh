#!/bin/bash
# Probe: where a real touch at the end of a text field puts the cursor, and whether the keys that follow arrive.
# Sourced by matrix.sh, in no list: by name only ([only: t10_probe_label t11_probe_rename] on GitHub).
#   t10_probe_label    the label sheet of the editor: a field that is not focused; the touch focuses it
#   t11_probe_rename   the rename row of an own sound: a field that takes the focus by itself (focus-on-show), as in the
#                      Maestro flow s38; the touch then lands on a field that has the focus already
# Each round: open the field, touch its right end with a real finger (`input tap`), then one of these ways of typing "xqz"
# with real key events (the text and the cursor come from the app's debug channel, `ui cursor`):
#   kb      wait for keyboard=up, read the cursor (is it at the end of the text?), then one key at a time, a read after each
#   slow    pause P ms after the touch, then one key at a time, a read after each
#   burst   pause P ms after the touch, then the three keys in one `input keyevent` call
#   s38     pause P ms after the touch, then 20 Backspaces in one call, then the three keys one at a time (what Maestro
#           does: eraseText 20, inputText); the text must be "xqz" afterwards
# P is 0, 100, 300 and 1000 ms. One line per round in probe_text_<name>.tsv of the results; the PASS or FAIL line gives the
# totals. A FAIL means a cursor that was not at the end after the touch, or a key that did not arrive where it should.
# Where the touch put the cursor is read only in `kb` (before the first key) and `slow` (after the first key, minus one):
# `burst` and `s38` have no read between the touch and the keys, to keep their timing.

PROBE_KEYS="KEYCODE_X KEYCODE_Q KEYCODE_Z"
PROBE_PAUSES="0 100 300 1000"

# text|cursor|anchor|preedit of the focused field; "none|-1|-1|<what the app said>" when no field has the focus.
probe_read() {
    ui cursor | python -I -c '
import json, sys
raw = sys.stdin.read().strip()
try:
    d = json.loads(raw)
    print("|".join([d["t"], str(d["c"]), str(d["a"]), d["pre"]]))
except (ValueError, KeyError):
    print("|".join(["none", "-1", "-1", raw[:60].replace("|", "/")]))
' 2> /dev/null
}

# "x y w" of the first text field on screen, in device pixels, or "none".
probe_field() {
    ui inputs | python -I -c '
import json, sys
try:
    r = json.loads(sys.stdin.read())[0]
    print(r["x"], r["y"], r["w"])
except (ValueError, IndexError, KeyError):
    print("none")
' 2> /dev/null
}

# The text of the first text field on screen (focused or not).
probe_base() {
    ui inputs | python -I -c '
import json, sys
try:
    print(json.loads(sys.stdin.read())[0]["t"])
except (ValueError, IndexError, KeyError):
    print("")
' 2> /dev/null
}

# What the final text says about the typing: OK, or how it went wrong. base: the text before the touch.
probe_verdict() { # mode base final
    python -I -c '
import sys
mode, base, final = sys.argv[1:4]
typed = "xqz"
want = (base[:max(0, len(base) - 20)] + typed) if mode == "s38" else base + typed
got = sum(final.count(c) for c in typed)
if final == want:
    print("OK")
elif got < 3:
    print("LOST%d" % (3 - got))
elif got > 3:
    print("DUP")
elif mode == "s38" and final.startswith(typed) and len(final) > len(want):
    print("AT_START_ERASE_MISSED")
elif final == typed + base:
    print("AT_START")
elif sorted(final) == sorted(want):
    print("MISPLACED")
else:
    print("OTHER")
' "$1" "$2" "$3"
}

# Has the focused field at least $1 characters?
probe_grown() {
    local t
    IFS='|' read -r t _ <<< "$(probe_read)"
    [ "${#t}" -ge "$1" ]
}

# One line into the device log, to line a round up with what the input method says.
probe_mark() { dev log -t PROBE "$1" > /dev/null 2>&1; }

probe_key() { dev input keyevent "$1"; } # lint-ok: the probe types with real key events

probe_pause() { # milliseconds
    [ "$1" -gt 0 ] || return 0
    sleep "$(awk -v ms="$1" 'BEGIN { printf "%.3f", ms / 1000 }')" # lint-ok: the pause is the subject of the probe
}

# probe_round <tsv> <n> <mode> <pause>: the field is open, nothing typed yet. Sets PROBE_VERDICT and PROBE_CURSOR
# (end, wrong, or unknown: where the touch put the cursor).
probe_round() {
    local typed tsv=$1 n=$2 mode=$3 p=$4 row fx fy fw base tx t0 c0="-" cur c anchor pre seq="" k final fc line
    PROBE_VERDICT="NOFIELD" PROBE_CURSOR=unknown
    row=$(probe_field)
    case "$row" in [0-9]*) ;; *)
        printf '%s\t%s\t%s\tNOFIELD\t%s\n' "$n" "$mode" "$p" "$row" >> "$tsv"
        return 1
        ;;
    esac
    read -r fx fy fw <<< "$row"
    base=$(probe_base)
    tx=$((fx + fw / 2 - 20))
    probe_mark "round $n mode=$mode pause=$p base_len=${#base} touch at $tx,$fy"
    t0=${EPOCHREALTIME/[.,]/}
    dev input tap "$tx" "$fy" # lint-ok: a real touch is the subject of the probe
    case "$mode" in
        kb)
            await_ui 8 'keyboard=up' > /dev/null 2>&1 || seq="${seq}kb-never-up;"
            IFS='|' read -r _ c _ _ <<< "$(probe_read)"
            c0=$c
            ;;
        *) probe_pause "$p" ;;
    esac
    case "$mode" in
        kb | slow)
            typed=0
            for k in $PROBE_KEYS; do
                probe_key "$k"
                typed=$((typed + 1))
                # The key arrives a moment after the call returns: read the cursor when the text has grown, not before.
                wait_for 3 probe_grown $((${#base} + typed))
                IFS='|' read -r cur c anchor pre <<< "$(probe_read)"
                seq="${seq}${cur}|${c}|${anchor};"
                # The cursor after the first key says where the touch put it.
                [ "$c0" != "-" ] || c0=$((c - 1))
            done
            ;;
        burst)
            # shellcheck disable=SC2086
            dev input keyevent $PROBE_KEYS # lint-ok: three keys in one call
            ;;
        s38)
            # shellcheck disable=SC2086
            dev input keyevent $(printf '67 %.0s' $(seq 1 20)) # lint-ok: Backspace, 20 times in one call
            for k in $PROBE_KEYS; do probe_key "$k"; done
            ;;
    esac
    await_settled 2 > /dev/null 2>&1
    IFS='|' read -r final fc anchor pre <<< "$(probe_read)"
    PROBE_VERDICT=$(probe_verdict "$mode" "$base" "$final")
    case "$mode" in
        kb | slow) [ "$c0" = "${#base}" ] && PROBE_CURSOR=end || PROBE_CURSOR=wrong ;;
    esac
    printf '%s\t%s\t%s\t%s\tbase=[%s]\tcursor_after_touch=%s(%s)\tfinal=[%s]\tcursor=%s\tanchor=%s\tpreedit=[%s]\tkeys=[%s]\tms=%s\n' \
        "$n" "$mode" "$p" "$PROBE_VERDICT" "$base" "$c0" "$PROBE_CURSOR" "$final" "$fc" "$anchor" "$pre" "$seq" \
        "$(((${EPOCHREALTIME/[.,]/} - t0) / 1000))" >> "$tsv"
    probe_mark "round $n mode=$mode pause=$p verdict=$PROBE_VERDICT cursor=$PROBE_CURSOR final=[$final]"
}

# The order of the rounds: every mode at every pause, again and again, with `kb` at both ends of each turn.
probe_plan() { # turns
    local t m p
    for ((t = 0; t < $1; t++)); do
        [ -n "${PROBE_NO_IME:-}" ] || echo "kb 0"
        for m in slow burst s38; do
            for p in $PROBE_PAUSES; do echo "$m $p"; done
        done
        [ -n "${PROBE_NO_IME:-}" ] || echo "kb 0"
    done
}

# probe_run <scenario name> <open command> <close command> <turns>
probe_run() {
    local name=$1 open=$2 close=$3 turns=$4 tsv="$OUT/probe_text_$1.tsv" n=0 ime mode p bad_cursor=0 lost=0 misplaced=0 other=0 skipped=0 ok=0 summary
    PROBE_IMES="" PROBE_IME_NOTE=""
    if [ -n "${PROBE_NO_IME:-}" ]; then
        # The control: no input method is on, so every key goes straight to the window.
        PROBE_IMES=$(adb_ shell ime list -s | tr -d '\r' | tr '\n' ' ')
        for ime in $PROBE_IMES; do adb_ shell ime disable "$ime" > /dev/null 2>&1; done
        PROBE_IME_NOTE="# input methods on before: [$PROBE_IMES] on after: [$(adb_ shell ime list -s | tr -d '\r' | tr '\n' ' ')] default: [$(adb_ shell settings get secure default_input_method | tr -d '\r')]"
    fi
    printf 'round\tmode\tpause_ms\tverdict\tbase\tcursor_after_touch\tfinal_text\tfinal_cursor\tanchor\tpreedit\tkeys\tms\n' > "$tsv"
    [ -z "$PROBE_IME_NOTE" ] || echo "$PROBE_IME_NOTE" >> "$tsv"
    while read -r mode p; do
        n=$((n + 1))
        if ! $open; then
            skipped=$((skipped + 1))
            printf '%s\t%s\t%s\tSKIPPED\tthe field did not open\n' "$n" "$mode" "$p" >> "$tsv"
            continue
        fi
        await_settled 3 > /dev/null 2>&1
        probe_round "$tsv" "$n" "$mode" "$p" || skipped=$((skipped + 1))
        case "$PROBE_VERDICT" in
            OK) ok=$((ok + 1)) ;;
            LOST*) lost=$((lost + 1)) ;;
            AT_START* | MISPLACED) misplaced=$((misplaced + 1)) ;;
            NOFIELD) ;;
            *) other=$((other + 1)) ;;
        esac
        [ "$PROBE_CURSOR" != wrong ] || bad_cursor=$((bad_cursor + 1))
        $close || skipped=$((skipped + 1))
    done < <(probe_plan "$turns")
    for ime in $PROBE_IMES; do adb_ shell ime enable "$ime" > /dev/null 2>&1; done
    # What the input method and the app's framework said, with the round marks between.
    adb_ logcat -d -b all -v time 2> /dev/null | grep -E "PROBE|SLINTPROBE|IInputConnectionWrapper|InputMethodManager|onStartInput|ImeTracker|slint" | tail -n 40000 > "$OUT/probe_text_$name.logcat"
    summary="$n rounds, $ok right, $lost with a lost key, $misplaced typed in the wrong place, $other other, cursor not at the end after the touch in $bad_cursor, $skipped skipped"
    echo "$summary" > "$OUT/probe_text_$name.summary"
    if [ $((lost + misplaced + other + bad_cursor)) -gt 0 ]; then
        fail "$summary"
    else
        pass "$summary"
    fi
}

probe_open_label() {
    open_row Label || return 1
    await_settled 3 > /dev/null 2>&1
    # The text before the touch is always the same, so that 20 Backspaces clear it (PROBE_BASE="" starts from an empty label).
    ui "set:0:${PROBE_BASE-mytune-label}" > /dev/null
}
probe_close_label() { done_sheet; }

probe_label_body() { # scenario name
    scenario "$1"
    fresh
    seed mytune-label "$(alarm_json mytune-label 7 0 "$EVERY_DAY" null)"
    arm mytune-label
    on_card 10
    press open mytune-label 0.8
    on_editor || { fail "no editor"; return; }
    probe_run "$1" probe_open_label probe_close_label "${PROBE_TURNS:-8}"
}

probe_open_rename() {
    # A real touch on the pencil, as Maestro does; where it is not found, the app's own button.
    finger "Rename this sound" > /dev/null 2>&1 || press sound-rename "$(sound_row mytune)" > /dev/null 2>&1 || return 1
    await_ui 4 'editor_sheet!=None' > /dev/null 2>&1
    case "$(probe_field)" in [0-9]*) return 0 ;; esac
    return 1
}
probe_close_rename() {
    tap_in Done 0 0.3
    await_settled 3 > /dev/null 2>&1
}

probe_rename_body() { # scenario name
    scenario "$1"
    fresh
    as_app "mkdir -p files/sounds/custom && cp files/sounds/default/air.ogg files/sounds/custom/mytune.ogg"
    seed a "$(alarm_json a 7 0 "$EVERY_DAY" null)"
    arm a
    on_card 10
    press open a 0.8
    on_editor || { fail "no editor"; return; }
    open_row Sound || { fail "no sound sheet"; return; }
    probe_run "$1" probe_open_rename probe_close_rename "${PROBE_TURNS:-8}"
}

t10_probe_label() { probe_label_body t10_probe_label; }
t11_probe_rename() { probe_rename_body t11_probe_rename; }
# The same with no input method on: where the faults are the input method's, they are gone here.
t12_probe_label_noime() { PROBE_NO_IME=1 probe_label_body t12_probe_label_noime; }
t13_probe_rename_noime() { PROBE_NO_IME=1 probe_rename_body t13_probe_rename_noime; }

# t16_probe_softkeys: the same field, but typed the way a person does it: by touching the keys of the keyboard itself.
# The positions of the keys are read once, from the keyboard's own window (uiautomator dump, a key carries its letter as
# text or description); where they cannot be read the probe ends with SKIP and never guesses.
#   per round: empty label, one touch on the field, wait for keyboard=up, a fixed word without a pause between the
#   touches, Backspace twice, two more letters, then the text is read back. Nothing of the keyboard is switched off:
#   the word is no word of any language, nothing ends it, so there is nothing to correct; a capital first letter
#   (auto-capitalisation) is counted as right and counted apart.
PROBE_WORD=qzxvkj
PROBE_TAIL=mw
declare -A PROBE_KEY=()

# "<key> <x> <y>" for each key of the typed letters and the Backspace key found in the keyboard's window; <key> is the
# letter or `del`. Reads the dump from stdin.
probe_keys_parse() { # letters
    python -I -c '
import re, sys
import xml.etree.ElementTree as ET
want = set(sys.argv[1])
dele = {"delete", "backspace", "verwijderen", "wissen"}
try:
    root = ET.parse(sys.stdin).getroot()
except ET.ParseError:
    sys.exit(0)
best = {}
for n in root.iter():
    m = re.match(r"\[(\d+),(\d+)\]\[(\d+),(\d+)\]$", n.get("bounds", ""))
    pkg = n.get("package", "").lower()
    if not m or not ("inputmethod" in pkg or "keyboard" in pkg):
        continue
    x1, y1, x2, y2 = map(int, m.groups())
    area = (x2 - x1) * (y2 - y1)
    if area <= 0:
        continue
    names = [n.get("text", "").strip().lower(), n.get("content-desc", "").strip().lower()]
    found = [c for c in names if len(c) == 1 and c in want]
    if any(c in dele for c in names) or re.search(r"delete|backspace|key_pos_del", n.get("resource-id", ""), re.I):
        found = ["del"]
    for key in found[:1]:
        if key not in best or area < best[key][0]:
            best[key] = (area, (x1 + x2) // 2, (y1 + y2) // 2)
for key, (_, x, y) in best.items():
    print(key, x, y)
' "$1" 2> /dev/null
}

# Fills PROBE_KEY from the keyboard on the screen. True when the keys of the word and Backspace are all found.
probe_keys_read() {
    local dump="$OUT/probe_text_t16.keys.xml" file=/data/local/tmp/af_keys.xml mode key x y letters
    letters="$PROBE_WORD$PROBE_TAIL"
    PROBE_KEY=()
    # The window flag exists from Android 11 on; before it the dump holds the app's window only.
    for mode in --windows ""; do
        # shellcheck disable=SC2086
        dev uiautomator dump $mode "$file" > /dev/null 2>&1 # lint-ok: the positions of the keys are read once per run, from the keyboard's own window
        dev cat "$file" > "$dump" 2> /dev/null
        while read -r key x y; do
            [ -z "$key" ] || PROBE_KEY[$key]="$x $y"
        done < <(probe_keys_parse "$letters" < "$dump")
        probe_keys_missing > /dev/null || return 0
    done
    return 1
}

# Prints the keys that are not found; false when none is missing.
probe_keys_missing() {
    local i c missing=""
    for ((i = 0; i < ${#PROBE_WORD} + ${#PROBE_TAIL}; i++)); do
        c=${PROBE_WORD:i:1}
        [ "$i" -lt "${#PROBE_WORD}" ] || c=${PROBE_TAIL:i-${#PROBE_WORD}:1}
        [ -n "${PROBE_KEY[$c]:-}" ] || missing="$missing$c"
    done
    [ -n "${PROBE_KEY[del]:-}" ] || missing="$missing delete"
    [ -n "$missing" ] || return 1
    echo "$missing"
}

# The places of the touches of one round, one "x y" per line.
probe_soft_taps() {
    local i
    for ((i = 0; i < ${#PROBE_WORD}; i++)); do echo "${PROBE_KEY[${PROBE_WORD:i:1}]}"; done
    echo "${PROBE_KEY[del]}"
    echo "${PROBE_KEY[del]}"
    for ((i = 0; i < ${#PROBE_TAIL}; i++)); do echo "${PROBE_KEY[${PROBE_TAIL:i:1}]}"; done
}

# The touches of one round: as one device command, so that they follow each other with nothing added between them, or
# (PROBE_SLOW) each in a device call of its own, which puts a call's time between two touches.
probe_soft_type() {
    local xy seq=""
    if [ -n "${PROBE_SLOW:-}" ]; then
        while read -r xy; do
            dev input tap $xy # lint-ok: touching the keys of another app's keyboard is the point of this probe
        done < <(probe_soft_taps)
        return
    fi
    while read -r xy; do
        seq="${seq}input tap $xy; " # lint-ok: touching the keys of another app's keyboard is the point of this probe
    done < <(probe_soft_taps)
    dev "$seq"
}

# The device log from this round's begin mark to now, for a wrong round: what the keyboard and the app said, with the
# trace of the vendored backend when it is on. `logcat -d` reads the whole buffer, which is why it is taken at once.
probe_slice() { # scenario name, round
    adb_ logcat -d -b all -v time 2> /dev/null | awk -v m="$1 round $2 begin" 'index($0, m) { p = 1 } p' | head -n 4000 > "$OUT/probe_text_$1.wrong_$2.logcat"
}

# Verdict of a typed text: right, right with a capital first letter (CAP), or wrong. The control types into a field with
# no auto-capitalisation, so it compares without regard to case.
probe_soft_judge() { # want final
    if [ "$2" = "$1" ]; then
        echo OK
    elif [ "$2" = "${1^}" ]; then
        echo CAP
    else
        echo WRONG
    fi
}

# probe_soft_body <scenario name> <open command> <close command> <field read command>: the rounds of t16, t17 and t18. The
# open command leaves the keyboard up over a focused empty field; the read command prints the text of that field.
probe_soft_rounds() {
    local name=$1 open=$2 close=$3 read=$4 rounds=${ROUNDS:-40} n right=0 capital=0 skipped=0 want final wrong="" missing
    adb_ logcat -G 16M > /dev/null 2>&1
    want="${PROBE_WORD:0:$((${#PROBE_WORD} - 2))}$PROBE_TAIL"
    for ((n = 1; n <= rounds; n++)); do
        $open || { skipped=$((skipped + 1)); continue; }
        if [ "${#PROBE_KEY[@]}" -eq 0 ]; then
            # The keys sit in the same places in every round: read once.
            if ! wait_for 8 probe_keys_read; then
                missing=$(probe_keys_missing)
                skip "no keys found in the keyboard's window (not found: ${missing:-everything}); dump in probe_text_t16.keys.xml"
                $close
                return
            fi
        fi
        probe_mark "$name round $n begin"
        probe_soft_type
        await_settled 2 > /dev/null 2>&1
        final=$($read)
        probe_mark "$name round $n end [$final]"
        printf '%s\t%s\n' "$n" "$final" >> "$OUT/probe_text_$name.tsv"
        case "$(probe_soft_judge "$want" "$final")" in
            OK) right=$((right + 1)) ;;
            CAP)
                right=$((right + 1))
                capital=$((capital + 1))
                ;;
            *)
                wrong="$wrong [$final]"
                probe_slice "$name" "$n"
                ;;
        esac
        $close
    done
    if [ "$right" -eq "$rounds" ]; then
        pass "$right of $rounds right, typed on the keys of the keyboard, expected [$want]; nothing of the keyboard switched off; first letter capital in $capital"
    else
        fail "$right of $rounds right, $skipped rounds not run, expected [$want]; wrong:$wrong; nothing of the keyboard switched off; first letter capital in $capital"
    fi
}

probe_soft_body() { # scenario name
    scenario "$1"
    fresh
    seed mytune-label "$(alarm_json mytune-label 7 0 "$EVERY_DAY" null)"
    arm mytune-label
    on_card 10
    press open mytune-label 0.8
    on_editor || { fail "no editor"; return; }
    PROBE_BASE=""
    probe_soft_rounds "$1" probe_soft_open probe_close_label probe_base
    PROBE_BASE=mytune-label
}

# The label field of the app, empty, with the keyboard up after one real touch.
probe_soft_open() {
    local row fx fy fw
    probe_open_label || return 1
    row=$(probe_field)
    case "$row" in [0-9]*) ;; *)
        probe_close_label
        return 1
        ;;
    esac
    read -r fx fy fw <<< "$row"
    dev input tap "$fx" "$fy" # lint-ok: a real touch on the field brings the keyboard up
    if ! await_ui 8 'keyboard=up'; then
        probe_close_label
        return 1
    fi
    await_settled 2 > /dev/null 2>&1
}

t16_probe_softkeys() { probe_soft_body t16_probe_softkeys; }
# The same with a device call between two touches, to tell a race of the keyboard with the app from a plain fault.
t17_probe_softkeys_slow() { PROBE_SLOW=1 probe_soft_body t17_probe_softkeys_slow; }

# t18_probe_softkeys_control: the same touches on a text field of Android itself (the search of Settings, a plain
# EditText of another app), to tell what the keyboard does on its own from what the app does to it.
PROBE_CONTROL_INTENTS=("-a android.settings.APP_SEARCH_SETTINGS" "-a android.intent.action.INSERT -t vnd.android.cursor.dir/contact")

# The text of the first EditText of the screen, from one dump.
probe_control_read() {
    dev uiautomator dump /data/local/tmp/af_ctl.xml > /dev/null 2>&1 # lint-ok: the text of a field of another app can only be read this way
    dev cat /data/local/tmp/af_ctl.xml 2> /dev/null | python -I -c '
import sys
import xml.etree.ElementTree as ET
try:
    root = ET.parse(sys.stdin).getroot()
except ET.ParseError:
    sys.exit(0)
for n in root.iter():
    if n.get("class") == "android.widget.EditText":
        print(n.get("text", ""))
        break
' 2> /dev/null
}

# Starts the field of the control fresh; the first call looks for an intent that brings a keyboard up and remembers it.
probe_control_open() {
    local intent
    for intent in "${PROBE_CONTROL_INTENTS[@]}"; do
        [ -z "${PROBE_CONTROL_INTENT:-}" ] || [ "$intent" = "$PROBE_CONTROL_INTENT" ] || continue
        # shellcheck disable=SC2086
        dev am start -S $intent > /dev/null 2>&1
        if wait_for 10 keyboard_up; then
            PROBE_CONTROL_INTENT=$intent
            # The dump takes the time the app's own wait for a still layout takes in t16.
            probe_control_read > /dev/null
            return 0
        fi
    done
    return 1
}
probe_control_close() { dev input keyevent KEYCODE_BACK > /dev/null 2>&1; } # lint-ok: leaves the field of the other app

t18_probe_softkeys_control() {
    scenario t18_probe_softkeys_control
    fresh
    PROBE_CONTROL_INTENT=""
    if ! probe_control_open; then
        skip "no field of Android's own found: no keyboard came up for ${PROBE_CONTROL_INTENTS[*]}"
        return
    fi
    probe_control_close
    probe_soft_rounds t18_probe_softkeys_control probe_control_open probe_control_close probe_control_read
    dev am force-stop com.google.android.settings.intelligence > /dev/null 2>&1
}
