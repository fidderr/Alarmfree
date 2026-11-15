#!/bin/bash
# The scenarios' way to find and touch what is on screen, through Android's accessibility tree
# (`uiautomator dump`) instead of the app's own debug channel (src/ui/debug*.rs).
#
# `source` this after lib.sh (matrix.sh does it when A11Y=1). It replaces the functions that
# ask the app where a text is (`where`, `where_has`, `all_texts`, `fields`, `set_field`,
# `window_size`, `is_on`, `has_role`, `tap_in`): everything built on them (tap, tap_has, is_there,
# on_screen, scroll_to, tap_clear, ...) follows. What needs the app's state (`sn`, `press`) still
# goes to the debug channel, which only a debug build has. The tree works in a release build too:
# a_tap, a_wait, a_texts, a_checked below use nothing else.
#
# Needs the `accessibility` feature of Slint in the app. The tree is built when the first service
# asks for it: the first dump after the app started can be empty, so a dump is asked again
# until it holds a label.

A11Y_DIR="devtest"
A11Y_PY="devtest/a11y_find.py"
A11Y_W=""
A11Y_H=""
A11Y_XML=""
A11Y_STAMP=0

# The screen size in device pixels, once.
a11y_size() {
    [ -z "$A11Y_W" ] || return 0
    local s
    s=$(dev wm size | sed -nE 's/.*: ([0-9]+)x([0-9]+).*/\1 \2/p' | tail -1)
    A11Y_W=${s% *}
    A11Y_H=${s#* }
    case "$A11Y_W$A11Y_H" in *[!0-9]* | "") A11Y_W=1080 A11Y_H=1920 ;; esac
}

# The tree of the window now, in A11Y_XML. A dump just taken (under 0.15 s ago) is reused, so a
# check that asks several things does not dump for each.
a11y_dump() {
    local i xml
    a11y_size
    if [ "${A11Y_FRESH:-0}" -gt 0 ] && [ $((SECONDS * 1000)) -lt $((A11Y_STAMP + 150)) ]; then return 0; fi
    for i in 1 2 3 4 5 6 7 8; do
        xml=$(adb_ shell "uiautomator dump /sdcard/af_ui.xml > /dev/null 2>&1; cat /sdcard/af_ui.xml" 2> /dev/null | tr -d '\r')
        if [ "$(printf '%s' "$xml" | python -I "$A11Y_PY" ready "$A11Y_W" "$A11Y_H")" = ok ]; then
            A11Y_XML=$xml
            A11Y_STAMP=$((SECONDS * 1000))
            return 0
        fi
        sleep 0.3
    done
    A11Y_XML=$xml
    return 1
}
a11y_ask() { # mode [text [which]]
    a11y_dump
    printf '%s' "$A11Y_XML" | python -I "$A11Y_PY" "$1" "$A11Y_W" "$A11Y_H" "${2:-}" "${3:-}"
}

# ---- the functions of lib.sh, on the tree ----
where() { a11y_ask find "$1" "${2:-}"; }
where_has() { a11y_ask findc "$1" "${2:-}"; }
all_texts() { a11y_ask texts; }
fields() { a11y_ask fields; }
window_size() {
    local density
    a11y_size
    density=$(dev wm density | sed -nE 's/.*: ([0-9]+).*/\1/p' | tail -1)
    # Slint's scale factor is the density over 160.
    echo "$A11Y_W $A11Y_H $(awk -v d="${density:-420}" 'BEGIN { printf "%.4f", d / 160 }')"
}
is_on() { [ "$(a11y_ask checked "$1")" = true ]; }
# Only the roles that a class name tells: a Button, a switch (ToggleButton), a field (EditText).
has_role() { # text role
    a11y_dump
    printf '%s' "$A11Y_XML" | python -I -c '
import re, sys
want = sys.argv[1]
role = {"Button": "Button", "Switch": "ToggleButton", "TextInput": "EditText", "RadioButton": "RadioButton", "CheckBox": "CheckBox"}.get(sys.argv[2], sys.argv[2])
xml = sys.stdin.read()
for m in re.finditer(r"<node ([^>]*)>", xml):
    d = dict(re.findall(r"([\w-]+)=\"([^\"]*)\"", m.group(1)))
    if want in (d.get("text"), d.get("content-desc")) and d.get("class", "").endswith("." + role):
        sys.exit(0)
sys.exit(1)
' "$1" "$2"
}
# What a_tap does through the debug channel before: the window's own event, now a real touch.
tap_in() { tap "$@"; }

# The n-th text field takes this text: a touch on it, the old text taken out, the new one typed
# through the keyboard's key events, as a person does it.
set_field() { type_field "$@"; }
type_field() { # n text
    local line xy old keys
    line=$(a11y_ask fields | sed -n "$(($1 + 1))p")
    [ -n "$line" ] || { echo "no field $1" >&2; return 1; }
    xy=${line%% *}
    xy=$(echo "$line" | cut -d' ' -f1-2)
    old=${line#* }
    old=${old#* }
    # shellcheck disable=SC2086
    dev input tap $xy
    sleep 0.4
    keys=$((${#old} + 4))
    dev "input keyevent KEYCODE_MOVE_END; i=0; while [ \$i -lt $keys ]; do input keyevent KEYCODE_DEL; i=\$((i + 1)); done"
    # `input text` takes %s for a space; a shell would read a # or a quote.
    local typed=${2// /%s}
    dev input text "'${typed//\'/}'"
    sleep 0.3
}

# ---- new, and the same in a release build ----
# Wait until a label is on screen. Returns at once when it is.
a_wait() { # text [seconds]
    wait_for "${2:-6}" is_there "$1"
}
# A touch on the middle of the label, once it holds still. Waits for it to show.
a_tap() { # text [which] [seconds after]
    a_wait "$1" 6 || { echo "nothing says $1" >&2; return 1; }
    tap "$1" "${2:-}" "${3:-0.2}"
}
a_checked() { a11y_ask checked "$1"; }
a_texts() { all_texts | tr '\n' '|' | sed 's/|$//'; }
# Scroll the screen with a finger until the label shows, up to n times (default 6).
a_scroll_to() { # text [n]
    local i n=${2:-6} s x y w h
    for i in $(seq 1 "$n"); do
        is_there "$1" && return 0
        s=$(a11y_ask scroller)
        if [ "$s" = none ]; then
            a11y_size
            x=$((A11Y_W / 2)) y=$((A11Y_H * 70 / 100))
            dev input swipe "$x" "$y" "$x" $((A11Y_H * 40 / 100)) 400
        else
            read -r x y w h <<< "$s"
            dev input swipe $((x + w / 2)) $((y + h * 75 / 100)) $((x + w / 2)) $((y + h * 30 / 100)) 400
        fi
        sleep 0.5
    done
    is_there "$1"
}
