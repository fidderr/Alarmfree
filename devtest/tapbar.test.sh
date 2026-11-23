#!/bin/bash
# tap_above_bar (lib.sh) against a stubbed app (no adb): a row of a scrolling page is only tapped when all of it is above the
# bar at the bottom of the page. Usage: bash devtest/tapbar.test.sh
here=$(cd "${BASH_SOURCE[0]%/*}" && pwd)
tmp=$(mktemp -d) || exit 1
trap 'rm -rf "$tmp"' EXIT
bad=0

cat > "$tmp/drive.sh" << 'D'
# Env: LIB DIR ROW (middle of the row, device px, before any scroll), STEP (what one swipe moves), MAXOFF (how far the page
# scrolls at most), BAR (the bar's one button: its middle y). The row is 142 px high and the button 137 (the reliability screen
# on a 1080x1920 window at 2.625). The real tap_above_bar of lib.sh runs; only the app and the swipe are replaced.
export OUT="$DIR/out" TMPDIR="$DIR"
mkdir -p "$OUT"
. "$LIB"
echo 0 > "$DIR/off"
hide_keyboard() { echo hide >> "$DIR/calls"; }
await_settled() { return 0; }
sheet_scroll() {
    local off
    off=$(($(cat "$DIR/off") + STEP))
    [ "$off" -le "$MAXOFF" ] || off=$MAXOFF
    echo "$off" > "$DIR/off"
    echo scroll >> "$DIR/calls"
}
tap_has() { echo "tap:$1" >> "$DIR/calls"; return 0; }
ui() {
    case "$1" in
        window) echo "1080 1794 2.625" ;;
        texts)
            printf '[{"h":142,"r":"Button","t":"Save the record","w":1017,"x":540,"y":%d},{"h":137,"r":"Button","t":"%s","w":1017,"x":540,"y":%d}]' \
                $((ROW - $(cat "$DIR/off"))) "${BARTEXT:-Test alarm}" "$BAR"
            ;;
    esac
}
tap_above_bar "Save the record" "Test alarm" "Cancel the test alarm"
echo "rc=$?" > "$DIR/rc"
D

run_case() { # name row step maxoff [bar y] [bar text]
    mkdir -p "$tmp/$1"
    env LIB="$here/lib.sh" DIR="$tmp/$1" ROW="$2" STEP="$3" MAXOFF="$4" BAR="${5:-1683}" BARTEXT="${6:-Test alarm}" bash "$tmp/drive.sh" > "$tmp/$1/stdout" 2> "$tmp/$1/stderr"
}
verdict() { # name ok
    if [ "$2" = 1 ]; then echo "PASS  $1"; else echo "FAIL  $1"; bad=1; fi
}
count() { grep -c "^$2" "$tmp/$1/calls"; }

# The row's middle at 1670 is behind the button (1615 to 1752); one swipe moves the page 199 px, the row ends at 1612 then.
run_case hidden 1670 199 199
verdict "a row behind the bar's button is scrolled clear first, then tapped once" "$([ "$(cat "$tmp/hidden/rc")" = rc=0 ] && [ "$(count hidden scroll)" = 1 ] && [ "$(count hidden tap:)" = 1 ] && echo 1)"
verdict "the keyboard is put away before anything else" "$([ "$(head -1 "$tmp/hidden/calls")" = hide ] && echo 1)"
run_case clear 1200 199 199
verdict "a row that is clear is tapped without a swipe" "$([ "$(cat "$tmp/clear/rc")" = rc=0 ] && [ "$(count clear scroll)" = 0 ] && [ "$(count clear tap:)" = 1 ] && echo 1)"
# The old check (the text is there) tapped a row whose middle was inside the window; here its middle is above the bar (1590 of
# 1616) but the row still reaches under it.
run_case partly 1590 80 199
verdict "a row whose middle is clear but whose end is under the bar is scrolled first" "$([ "$(count partly scroll)" -ge 1 ] && [ "$(count partly tap:)" = 1 ] && echo 1)"
run_case stuck 1670 0 0
verdict "a page that does not move: no tap, six swipes, a failed step with the numbers" "$([ "$(cat "$tmp/stuck/rc")" = rc=1 ] && [ "$(count stuck tap:)" = 0 ] && [ "$(count stuck scroll)" = 6 ] && grep -q "no clear place to tap 'Save the record': its bottom 1741, the bar's top 1614 less 31 px" "$tmp/stuck/stderr" && echo 1)"
verdict "the failed step is kept for the scenario" "$([ -s "$tmp/stuck/out/step_failed.txt" ] && echo 1)"
run_case slow 1670 50 199
verdict "a page that moves a little each swipe is scrolled until the row is clear (four swipes), then tapped" "$([ "$(cat "$tmp/slow/rc")" = rc=0 ] && [ "$(count slow scroll)" = 4 ] && [ "$(count slow tap:)" = 1 ] && echo 1)"
run_case cancel 1560 100 300 1683 "Cancel the test alarm"
verdict "the other bar text counts as the bar too" "$([ "$(cat "$tmp/cancel/rc")" = rc=0 ] && [ "$(count cancel tap:)" = 1 ] && [ "$(count cancel scroll)" -ge 1 ] && echo 1)"
exit $bad
