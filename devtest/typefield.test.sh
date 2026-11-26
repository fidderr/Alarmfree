#!/bin/bash
# type_field against a stubbed app (no adb): one typing, and what the app answers decides. Usage: bash devtest/typefield.test.sh
here=$(cd "${BASH_SOURCE[0]%/*}" && pwd)
tmp=$(mktemp -d) || exit 1
trap 'rm -rf "$tmp"' EXIT
bad=0

cat > "$tmp/drive.sh" << 'D'
# Env: LIB DIR ANSWER (what `heldat` says) and SETTEXT (what `settext` says, default: OK 540 600). The real type_field of
# lib.sh runs; only the app is replaced.
export OUT="$DIR/out" TMPDIR="$DIR"
. "$LIB"
await_settled() { return 0; }
ui() {
    case "$1" in
        # The keyboard: KB px of the 1920, and the field's middle: Y, or LOW for the first RISE reads (the sheet has not scrolled yet).
        snapshot) echo "{\"insets_bottom_px\":${KB-0}.0}" ;;
        window) echo "1080 1920 2.6" ;;
        inputs)
            n=$(cat "$DIR/reads" 2> /dev/null || echo 0)
            echo $((n + 1)) > "$DIR/reads"
            if [ "$n" -lt "${RISE-0}" ]; then echo "[{\"y\":300},{\"y\":${LOW-1500}}]"; else echo "[{\"y\":300},{\"y\":${Y-600}}]"; fi ;;
        settext:*) echo "$1" >> "$DIR/calls"; echo "${SETTEXT-OK 540 600}" ;;
        heldat:*) echo "$1" >> "$DIR/calls"; echo "$ANSWER" ;;
        *) echo "$1" >> "$DIR/calls" ;;
    esac
}
type_field "${FIELD:-1}" 'Take pills'
echo "rc=$?" > "$DIR/rc"
D

run_case() { # name heldat-answer [settext-answer]
    mkdir -p "$tmp/$1"
    env LIB="$here/lib.sh" DIR="$tmp/$1" ANSWER="$2" SETTEXT="${3-OK 540 600}" FIELD_MAY_GO="${MAYGO-}" FIELD="${FIELD-}" KB="${KB-0}" Y="${Y-600}" LOW="${LOW-1500}" RISE="${RISE-0}" bash "$tmp/drive.sh" > "$tmp/$1/stdout" 2> "$tmp/$1/stderr"
}
verdict() { # name ok
    if [ "$2" = 1 ]; then echo "PASS  $1"; else echo "FAIL  $1"; bad=1; fi
}
settext_calls() { if [ -e "$tmp/$1/calls" ]; then grep -c '^settext:' "$tmp/$1/calls"; else echo 0; fi; }

run_case same 'TEXT:Take pills'
verdict "the field holds the text: done after one typing, asked where it was typed" "$([ "$(cat "$tmp/same/rc")" = rc=0 ] && [ "$(settext_calls same)" = 1 ] && grep -q '^heldat:540:600$' "$tmp/same/calls" && echo 1)"
MAYGO=1 run_case gone 'GONE'
verdict "the field left the screen with the last key, and the caller said it may: done after one typing" "$([ "$(cat "$tmp/gone/rc")" = rc=0 ] && [ "$(settext_calls gone)" = 1 ] && echo 1)"
run_case lost 'GONE'
verdict "no field and nothing typed (a touch that hit no field), the caller did not say it may go: fails, once" "$([ "$(cat "$tmp/lost/rc")" = rc=1 ] && [ "$(settext_calls lost)" = 1 ] && echo 1)"
verdict "the failure of a lost field says what the field holds" "$(grep -q 'it holds \[GONE\]' "$tmp/lost/stderr" && echo 1)"
run_case short 'TEXT:Take pill'
verdict "the field holds less: fails after one typing, no second typing" "$([ "$(cat "$tmp/short/rc")" = rc=1 ] && [ "$(settext_calls short)" = 1 ] && echo 1)"
verdict "the failure says what the field holds" "$(grep -q 'it holds \[TEXT:Take pill\]' "$tmp/short/stderr" && echo 1)"
run_case nofield 'TEXT:Take pills' 'none'
verdict "no such field: fails after one try, without a read-back" "$([ "$(cat "$tmp/nofield/rc")" = rc=1 ] && [ "$(settext_calls nofield)" = 1 ] && ! grep -q '^heldat' "$tmp/nofield/calls" && echo 1)"
verdict "after a typing the keyboard is waited for (a named state), before the read-back" "$(grep -n -E '^await:10000:keyboard=up$|^heldat:' "$tmp/same/calls" | head -2 | cut -d: -f2- | tr '
' ' ' | grep -q '^await:10000:keyboard=up heldat:' && echo 1)"
verdict "a keyboard that is not up 10 s after the typing is said in a NOTE (with the field), the typing still goes on" "$(grep -q 'type_field 1: the keyboard was not up 10 s after the typing' "$tmp/same/stderr" && [ "$(cat "$tmp/same/rc")" = rc=0 ] && echo 1)"
FIELD=next run_case tab 'TEXT:Take pills'
verdict "after a Tab typing the keyboard is not waited for, and no NOTE says it was not up" "$([ "$(cat "$tmp/tab/rc")" = rc=0 ] && ! grep -q 'keyboard=up' "$tmp/tab/calls" && ! grep -q 'keyboard was not up' "$tmp/tab/stderr" && echo 1)"
MAYGO=1 run_case maygo2 'TEXT:Take pills'
verdict "when the typing may take its own field away the keyboard is not waited for, and no NOTE" "$([ "$(cat "$tmp/maygo2/rc")" = rc=0 ] && ! grep -q 'keyboard=up' "$tmp/maygo2/calls" && ! grep -q 'keyboard was not up' "$tmp/maygo2/stderr" && echo 1)"
# The keyboard is up (900 of 1920 px, from 1020): a field at 1500 is under it until the sheet scrolls.
KB=900 Y=1500 run_case under 'TEXT:Take pills'
verdict "a field that stays under the keyboard: fails without a touch, says where it is" "$([ "$(cat "$tmp/under/rc")" = rc=1 ] && [ "$(settext_calls under)" = 0 ] && grep -q 'stays under the keyboard (its middle at 1500, the keyboard from 1020)' "$tmp/under/stderr" && echo 1)"
KB=900 RISE=3 run_case rises 'TEXT:Take pills'
verdict "a field that is under the keyboard for a moment and then scrolled up: typed once, after the wait" "$([ "$(cat "$tmp/rises/rc")" = rc=0 ] && [ "$(settext_calls rises)" = 1 ] && echo 1)"
KB=900 Y=700 run_case above 'TEXT:Take pills'
verdict "a field above the keyboard: typed once at once" "$([ "$(cat "$tmp/above/rc")" = rc=0 ] && [ "$(settext_calls above)" = 1 ] && echo 1)"
exit $bad
