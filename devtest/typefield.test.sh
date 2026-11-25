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
        settext:*) echo "$1" >> "$DIR/calls"; echo "${SETTEXT-OK 540 600}" ;;
        heldat:*) echo "$1" >> "$DIR/calls"; echo "$ANSWER" ;;
        *) echo "$1" >> "$DIR/calls" ;;
    esac
}
type_field 1 'Take pills'
echo "rc=$?" > "$DIR/rc"
D

run_case() { # name heldat-answer [settext-answer]
    mkdir -p "$tmp/$1"
    env LIB="$here/lib.sh" DIR="$tmp/$1" ANSWER="$2" SETTEXT="${3-OK 540 600}" FIELD_MAY_GO="${MAYGO-}" bash "$tmp/drive.sh" > "$tmp/$1/stdout" 2> "$tmp/$1/stderr"
}
verdict() { # name ok
    if [ "$2" = 1 ]; then echo "PASS  $1"; else echo "FAIL  $1"; bad=1; fi
}
settext_calls() { grep -c '^settext:' "$tmp/$1/calls"; }

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
exit $bad
