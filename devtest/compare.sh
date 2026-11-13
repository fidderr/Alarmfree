#!/bin/bash
# Side by side: AlarmFree (Dioxus, UI in a WebView) and AlarmFree Slint (native UI) on one
# device. Prints a table: APK size, cold start (median of RUNS), memory (PSS) after the start
# and after opening the editor, idle CPU after 60 s on the home list, and the frame times of
# scrolling a list of 14 alarms (jank).
#
#   devtest/compare.sh                       # the APKs below, on emulator-5556
#   SERIAL=bb741d88 ORIG_APK=... SLINT_APK=... RUNS=7 devtest/compare.sh      # the real phone
#
# Only two serials are ever used: emulator-5556 and the phone bb741d88. Anything else is refused.
# Both APKs have to be builds for the device's ABI and release builds (a debug build says
# nothing about speed). The original is built from a copy of its source in build/original,
# never in C:/Git/Alarmfree (see docs/SLINT_PORT.md, "Comparing"). An unsigned APK is signed
# (copy) with the debug key to install.
#
# Safety: every tap, swipe, Back and Home press first checks that the focused window is one of the
# two AlarmFree activities, and aborts otherwise (never press in another app). Data is cleared
# only for the two AlarmFree packages: where `pm clear` is refused (the phone) the package is
# uninstalled and installed again, which is the same fresh state. The alarm volume is set to 1 for
# the run and restored.
#
# Positions are derived from the device (wm size, wm density), in dp: px = dp * density / 160.
# Calibrated with screenshots on the phone (1080x2376, 480 dpi = 360 x 792 dp), both apps, empty
# list and with alarms, and checked on the emulator (1080x2400, 420 dpi = 411 x 914 dp):
#   quick "+5" pill:      72 dp from the left, 165 dp from the top. The pill spans about 140-173 dp
#                         (empty list) and 160-193 dp (with an alarm, the "next alarm" sub line
#                         pushes it down) on the original, 155-171 dp / 155-188 dp on the Slint copy;
#                         165 dp is inside every range. Its width spans 48-100 dp.
#   + (new alarm) button: 71 dp from the right (centre), 125 dp above the bottom (the button spans
#                         95-151 dp above the bottom and is 56 dp wide).
#   Get started button:   horizontally centred, 57 dp above the bottom (it spans 31-83 dp).
#   Both are measured from the top of the 3-button navigation bar where there is one (the emulator:
#   48 dp, settings secure navigation_mode 0; the phone uses gestures), which puts them at 173 and
#   105 dp above the emulator's bottom edge (the old fixed taps there were 156 and 90 dp).
#                         (this fixed dp position is only the first guess: with the reliability banners the
#                         pill is looked for on the screen, see find_pill_y.)
#   swipes:               at the horizontal centre, inside the list: see scroll_frames.
set -uo pipefail
export MSYS_NO_PATHCONV=1

cd "$(dirname "$0")/.."
SDK="${ANDROID_HOME:-$(cygpath -u "$LOCALAPPDATA" 2>/dev/null || echo "$LOCALAPPDATA")/Android/Sdk}"
ADB="${ADB:-$SDK/platform-tools/adb.exe}"
SERIAL="${SERIAL:-emulator-5556}"
RUNS="${RUNS:-7}"
IDLE_SECONDS="${IDLE_SECONDS:-60}"
OUT="${OUT:-devtest-out/compare}"
ORIG_APK="${ORIG_APK:-build/original/target/dx/alarmfree/release/android/app/app/build/outputs/apk/release/app-release-unsigned.apk}"
SLINT_APK="${SLINT_APK:-build/alarmfree-slint-release.apk}"
ORIG_PKG=com.fidderr.alarmfree
ORIG_ACT=dev.dioxus.main.MainActivity
SLINT_PKG=com.fidderr.alarmfree.slint
SLINT_ACT=com.fidderr.alarmfree.slint.MainActivity
mkdir -p "$OUT"

case "$SERIAL" in
  emulator-5556 | bb741d88) ;;
  *) echo "SERIAL must be emulator-5556 or bb741d88 (got $SERIAL)"; exit 2 ;;
esac
dev() { "$ADB" -s "$SERIAL" "$@"; }
shell() { dev shell "$@" | tr -d '\r'; }

[ -f "$ORIG_APK" ] || { echo "missing $ORIG_APK"; exit 1; }
[ -f "$SLINT_APK" ] || { echo "missing $SLINT_APK"; exit 1; }

# ---- Display geometry and the dp helpers ----
SIZE=$(shell wm size | tail -1 | sed -E 's/.*: *//')
W=${SIZE%x*}
H=${SIZE#*x}
DENS=$(shell wm density | tail -1 | sed -E 's/.*: *//')
NAVMODE=$(shell settings get secure navigation_mode)
NAV_DP=0 # height of the 3-button navigation bar (48 dp) when there is one; the phone uses gestures
[ "$NAVMODE" = 0 ] && NAV_DP=48
px() { awk -v d="$1" -v n="$DENS" 'BEGIN { printf "%d", d * n / 160 + 0.5 }'; }

# ---- Focus guard ----
focus_ok() {
  local i
  for i in 1 2 3; do
    shell dumpsys window | grep mCurrentFocus | grep -Eq 'com\.fidderr\.alarmfree(\.slint)?/' && return 0
    sleep 1
  done
  return 1
}
need_focus() {
  focus_ok || { echo "ABORT: the focused window is not an AlarmFree activity: $(shell dumpsys window | grep mCurrentFocus)" >&2; kill -TERM $$; exit 3; }
}
tap() { [ -n "${1:-}" ] && [ -n "${2:-}" ] || { echo "ABORT: tap without position" >&2; kill -TERM $$; exit 3; }; need_focus; shell input tap "$1" "$2"; }                       # px
tap_dp() { tap "$(px "$1")" "$(px "$2")"; }                            # dp from the left, from the top
tap_dp_r() { tap "$((W - $(px "$1")))" "$(px "$2")"; }                 # dp from the right, from the top
tap_dp_br() { tap "$((W - $(px "$1")))" "$((H - $(px $NAV_DP) - $(px "$2")))"; } # dp from the right, from the bottom (above the navigation bar)
swipe() { need_focus; shell input swipe "$@"; }
back_key() { need_focus; shell input keyevent KEYCODE_BACK; }

tap_plus() { tap_dp_br 71 125; }
tap_start() { tap "$((W / 2))" "$((H - $(px $NAV_DP) - $(px 57)))"; }

# ---- Signing, so a release APK installs ----
BT=$(ls -d "$SDK"/build-tools/* | sort -V | tail -1)
KEYSTORE="$(cygpath -w "${DEBUG_KEYSTORE:-$HOME/.android/debug.keystore}" 2>/dev/null || echo "${DEBUG_KEYSTORE:-$HOME/.android/debug.keystore}")"
sign() { # in out
  local in="$1" out="$2" aligned="$OUT/aligned.apk"
  rm -f "$aligned" "$out"
  "$BT/zipalign.exe" -f -p 4 "$in" "$aligned"
  "$BT/apksigner.bat" sign --ks "$KEYSTORE" --ks-pass pass:android --out "$(cygpath -w "$out" 2>/dev/null || echo "$out")" "$(cygpath -w "$aligned" 2>/dev/null || echo "$aligned")" >/dev/null
}
needs_sign() { ! unzip -l "$1" | grep -q "META-INF/.*\.\(RSA\|EC\|DSA\)"; }
prepare() { # apk name
  if needs_sign "$1"; then
    sign "$1" "$OUT/$2-signed.apk"
    echo "$OUT/$2-signed.apk"
  else
    echo "$1"
  fi
}
ORIG_INSTALL=$(prepare "$ORIG_APK" original)
SLINT_INSTALL=$(prepare "$SLINT_APK" slint)

size_of() { stat -c %s "$1"; }
mb() { awk -v b="$1" 'BEGIN { printf "%.1f", b / 1048576 }'; }

# ---- Device settings changed for the run, restored on exit ----
ALARM_VOL=$(shell cmd media_session volume --stream 4 --get | sed -n 's/.*volume is \([0-9]*\) in range.*/\1/p')
STAYON=$(shell settings get global stay_on_while_plugged_in)
restore() {
  [ -n "$ALARM_VOL" ] && shell cmd media_session volume --stream 4 --set "$ALARM_VOL" > /dev/null 2>&1
  [ -n "$STAYON" ] && shell settings put global stay_on_while_plugged_in "$STAYON" > /dev/null 2>&1
}
trap restore EXIT
trap "exit 143" TERM # an abort from inside $( ) ends the whole script
shell cmd media_session volume --stream 4 --set 1 > /dev/null 2>&1
# The screen has to stay on for the 60 s idle measurement.
shell settings put global stay_on_while_plugged_in 7 > /dev/null 2>&1
shell input keyevent KEYCODE_WAKEUP > /dev/null
shell wm dismiss-keyguard > /dev/null 2>&1 || true

stop_all() { shell am force-stop $ORIG_PKG; shell am force-stop $SLINT_PKG; sleep 1; }
# Home only while an AlarmFree window is in front (after a force-stop the launcher already is).
home_screen() { if focus_ok; then shell input keyevent KEYCODE_HOME; fi; sleep 1; }
median() { sort -n | awk '{ a[NR] = $1 } END { if (NR % 2) print a[(NR + 1) / 2]; else print (a[NR / 2] + a[NR / 2 + 1]) / 2 }'; }

grant_all() { # pkg (refused on some phones: ignored)
  local p
  for p in POST_NOTIFICATIONS ACTIVITY_RECOGNITION CAMERA; do dev shell pm grant "$1" android.permission.$p > /dev/null 2>&1 || true; done
}

# A package from clean data: pm clear, or where that is refused uninstall + install. Removes its
# scheduled alarms as well.
reset_pkg() { # pkg apk
  if ! shell pm clear "$1" 2>&1 | grep -q Success; then
    shell am force-stop "$1"
    dev uninstall "$1" > /dev/null
    dev install -r "$2" > /dev/null || { echo "install of $1 failed"; exit 1; }
  fi
  grant_all "$1"
}

# The big primary button at the bottom (Get started, and on the Slint copy also the reliability
# screen's Test alarm) is saturated blue (any theme) at its left end; the home list has no such
# colour there.
# One pixel of a raw screen capture, taken only while an AlarmFree window has the focus.
blue_button_shown() {
  need_focus
  dev exec-out screencap > "$OUT/px.raw"
  local hdr=$(( $(stat -c %s "$OUT/px.raw") - W * H * 4 )) x=$(px 24) y=$((H - $(px $NAV_DP) - $(px 57))) rgb
  rgb=$(tail -c +$((hdr + (y * W + x) * 4 + 1)) "$OUT/px.raw" | head -c 3 | od -An -tu1)
  rm -f "$OUT/px.raw"
  echo "  pixel at the button: $rgb" >&2
  echo $rgb | awk '{ exit !($3 > 150 && $3 - $1 > 80) }'
}

# Past the first run page (Get started; the Slint copy may then open its reliability screen, which
# is left with Back): both show their empty alarm list.
first_run() { # pkg act
  local i
  stop_all
  shell am start -W -n "$1/$2" > /dev/null
  sleep 5
  tap_start
  sleep 2
  for i in 1 2 3; do
    blue_button_shown || break
    back_key
    sleep 2
  done
  stop_all
}

# Both apps show a reliability banner above the quick row while something is not allowed on the
# phone, and again at every start. Its height differs per app and message (1 to 3 lines), and it
# moves the pill by up to 90 dp, so the pill is looked for on the screen: in a raw screen capture
# (taken only while an AlarmFree window has the focus) the column 62 dp from the left is scanned
# from 125 to 330 dp from the top, and every stretch that differs from the page background (read
# 4 dp from the left edge) is a run. The banner is a run taller than 40 dp, the pill (cut near its
# round end) one of 8 to 40 dp, the alarm list below it a long one; the middle of the first
# run of pill height is the centre. Works in the light and the dark theme.
find_pill_y() { # -> y in px
  need_focus
  dev exec-out screencap > "$OUT/px.raw"
  local hdr=$(( $(stat -c %s "$OUT/px.raw") - W * H * 4 )) x=$(px 62) y0=$(px 125) y1=$(px 330) y rgb bg res
  px_at() { tail -c +$((hdr + ($2 * W + $1) * 4 + 1)) "$OUT/px.raw" | head -c 3 | od -An -tu1; }
  bg=$(px_at "$(px 4)" "$y0")
  res=$(for ((y = y0; y < y1; y += 3)); do echo "$y $(px_at "$x" "$y") $bg"; done | awk -v mn="$(px 8)" -v mx="$(px 40)" '
    { d = 0; for (i = 0; i < 3; i++) { v = $(2 + i) - $(5 + i); if (v < 0) v = -v; if (v > d) d = v }
      if (d > 5) { if (!on) { first = $1; on = 1 } last = $1 }
      else if (on) { on = 0; h = last - first + 3; if (!found && h >= mn && h <= mx) { found = 1; print int((first + last) / 2) } } }')
  rm -f "$OUT/px.raw"
  echo "$res"
}
PILL_Y_PX=
locate_pill() {
  local y
  y=$(find_pill_y)
  if [ -n "$y" ]; then
    PILL_Y_PX=$y
  elif [ -z "$PILL_Y_PX" ]; then
    echo "ABORT: the quick pill was not found on the screen" >&2; kill -TERM $$; exit 3
  else
    echo "  pill not found this time, keeping the last position" >&2
  fi
  echo "  quick pill at $PILL_Y_PX px ($((PILL_Y_PX * 160 / DENS)) dp)" >&2
}
tap_pill() { tap "$(px 72)" "$PILL_Y_PX"; }

[ -n "${JANK_ONLY:-}" ] || dev install -r "$ORIG_INSTALL" > /dev/null || { echo "install of the original failed"; exit 1; }
[ -n "${JANK_ONLY:-}" ] || dev install -r "$SLINT_INSTALL" > /dev/null || { echo "install of the Slint copy failed"; exit 1; }

cold_start() { # pkg act -> median ms
  local pkg="$1" act="$2" times=() i t
  for ((i = 0; i < RUNS; i++)); do
    stop_all
    home_screen
    t=$(shell am start -W -n "$pkg/$act" | sed -n 's/^TotalTime: *//p')
    if [ -z "$t" ] || [ "$t" = 0 ]; then
      stop_all; sleep 3; home_screen
      t=$(shell am start -W -n "$pkg/$act" | sed -n 's/^TotalTime: *//p')
    fi
    times+=("$t")
    sleep 2
  done
  printf '%s\n' "${times[@]}" > "$OUT/$pkg.starts"
  printf '%s\n' "${times[@]}" | median
}

# ---- Memory ----
# "Total PSS by process" lists every process with its pid: the app's own (pkg and pkg:sentinel)
# and the WebView ones. On the phone the renderer is com.google.android.webview:sandboxed_process0
# ..., and other apps (Chrome, ...) have such processes too: the ones that appeared while the app
# ran (new pids) are the app's.
pss_kb() { # pkg: the main process
  shell dumpsys meminfo "$1" | grep -m1 "TOTAL PSS:" | sed -E 's/.*TOTAL PSS: *([0-9]+).*/\1/'
}
proc_list() { # "pid kb name" per process
  shell dumpsys meminfo | awk '
    /^Total PSS by process:/ { on = 1; next }
    /^Total PSS by OOM/ { on = 0 }
    on && match($0, /\(pid [0-9]+/) { pid = substr($0, RSTART + 5, RLENGTH - 5); kb = $1; gsub(/[,K:]/, "", kb); print pid, kb, $2 }'
}
app_procs_kb() { # pkg -> PSS of pkg and pkg:*
  proc_list | awk -v p="$1" '$3 == p || index($3, p ":") == 1 { s += $2 } END { print s + 0 }'
}
new_webview_pids() { # before-file -> pids of WebView processes that are not in the file
  proc_list | awk -v f="$1" '
    BEGIN { while ((getline l < f) > 0) { split(l, a, " "); seen[a[1]] = 1 } }
    !($1 in seen) && $3 ~ /(webview|sandboxed_process)/ { print $1, $2 }'
}
new_webview_kb() { new_webview_pids "$1" | awk '{ s += $2 } END { print s + 0 }'; }

measure_memory() { # pkg act -> "after_start_kb after_edit_kb webview_kb app_procs_after_edit_kb"
  local pkg="$1" act="$2" a b w w2 ap before="$OUT/$1.procs-before"
  stop_all
  home_screen
  proc_list > "$before"
  shell am start -W -n "$pkg/$act" > /dev/null
  sleep 5
  a=$(pss_kb "$pkg")
  w=$(new_webview_kb "$before")
  tap_plus # the editor
  sleep 3
  b=$(pss_kb "$pkg")
  w2=$(new_webview_kb "$before")
  ap=$(app_procs_kb "$pkg")
  proc_list > "$OUT/$pkg.procs-after-editor"
  back_key
  sleep 1
  [ "$w2" -gt "$w" ] && w=$w2
  echo "$a $b $w $ap"
}

# ---- Idle CPU: user + system CPU ticks (/proc/pid/stat, 100 Hz) of the app's processes and its
# new WebView ones over IDLE_SECONDS on the home list with the screen on, as a percentage of one core ----
ticks() { # pids...
  local pid t=0 v
  for pid in "$@"; do
    v=$(shell cat /proc/$pid/stat 2> /dev/null | sed -E 's/^[0-9]+ \(.*\) //' | awk '{ print $12 + $13 }')
    t=$((t + ${v:-0}))
  done
  echo $t
}
measure_idle() { # pkg act -> cpu percent of one core
  local pkg="$1" act="$2" before="$OUT/$1.idle-before" pids t0 t1
  stop_all
  home_screen
  proc_list > "$before"
  shell am start -W -n "$pkg/$act" > /dev/null
  sleep 8
  need_focus
  pids=$( { shell pidof "$pkg"; shell pidof "$pkg:sentinel"; new_webview_pids "$before" | awk '{ print $1 }'; } | tr '\n' ' ')
  echo "  idle pids: $pids" >&2
  t0=$(ticks $pids)
  sleep "$IDLE_SECONDS"
  need_focus
  t1=$(ticks $pids)
  shell dumpsys cpuinfo | grep -E "$pkg|webview" > "$OUT/$pkg.cpuinfo" 2> /dev/null
  shell "top -b -n 1" | grep -E "$pkg" > "$OUT/$pkg.top" 2> /dev/null
  awk -v a="$t0" -v b="$t1" -v s="$IDLE_SECONDS" 'BEGIN { printf "%.2f", (b - a) / 100 / s * 100 }'
}

# ---- Scrolling ----
# Quick alarms, so there is a list to scroll.
seed_alarms() { # pkg act
  stop_all
  home_screen
  shell am start -W -n "$1/$2" > /dev/null
  sleep 4
  local i delay=0.6
  [ "$1" = "$SLINT_PKG" ] && delay=1.5 # the Slint copy needs time to rebuild the list
  for ((i = 0; i < 14; i++)); do
    # The first alarm adds the "next alarm" sub line, which moves the pill down: look again after it.
    if [ $i = 0 ]; then locate_pill; elif [ $i = 1 ]; then sleep 0.5; locate_pill; fi
    tap_pill
    sleep $delay
  done
  sleep 1
}

# The frames of the app's own layer from SurfaceFlinger, which sees every app whatever draws it.
# `dumpsys gfxinfo` only counts what Android's own renderer draws. SurfaceFlinger keeps the last
# 127 frames, so each trial scrolls for 1.2 s and reads them straight after. Android 16 prints
# the layers as "RequestedLayerState{pkg/Activity#id parentId=..}": the one without a window-token
# prefix is the buffer layer that gets the frames (older Androids print "pkg/Activity#id").
layer_of() { # pkg
  shell dumpsys SurfaceFlinger --list | grep -oE "(^|\\{)$1/[^ ]*Activity#[0-9]+" | sed 's/^{//' | sort -t'#' -k2 -n | tail -1
}

frame_times() { # layer -> "present_ts delta_ms period_ms" per frame (period = the vsync period in the first line)
  shell dumpsys SurfaceFlinger --latency "'$1'" | awk '
    NR == 1 { period = $1 / 1000000; next }
    NF == 3 && $2 < 9000000000000000000 && $2 > 0 { if (prev) print $2, ($2 - prev) / 1000000, period; prev = $2 }'
}

scroll_frames() { # pkg act -> "frames janky p50 p90 p95 p99 period_ms" (ms between frames; janky > 1.5 x the vsync period)
  local pkg="$1" act="$2" layer trial all y1 y2
  all="$OUT/$pkg.frames"
  seed_alarms "$pkg" "$act"
  layer=$(layer_of "$pkg")
  echo "  layer: $layer" >&2
  shell dumpsys gfxinfo "$pkg" reset > /dev/null 2>&1
  : > "$all"
  if [ -n "${PERFETTO_NAME:-}" ]; then # a trace of the same workload (JANK_ONLY runs)
    dev shell perfetto -c - --txt -o "/data/misc/perfetto-traces/$PERFETTO_NAME.pftrace" < devtest/jank.pbtxt > "$OUT/perfetto.log" 2>&1 &
    PERFETTO_PID=$!
    sleep 3
  fi
  # Swipes stay inside the list (the quick row and the banner above it do not scroll): from 70 dp
  # below the pill's centre to 82 % of the height (above the tab bar and the navigation bar), 1.2 s
  # each, so the finger moves for the whole swipe and the 127 frames read afterwards are all of it.
  y2=$((PILL_Y_PX + $(px 70))); y1=$((H * 82 / 100))
  for trial in 1 2 3 4 5 6; do
    swipe $((W / 2)) $y1 $((W / 2)) $y2 1200
    frame_times "$layer" | awk '$2 < 100' >> "$all"
    swipe $((W / 2)) $y2 $((W / 2)) $y1 1200
    frame_times "$layer" | awk '$2 < 100' >> "$all"
  done
  if [ -n "${PERFETTO_NAME:-}" ]; then
    wait "$PERFETTO_PID"
    dev pull "/data/misc/perfetto-traces/$PERFETTO_NAME.pftrace" "$OUT/$PERFETTO_NAME.pftrace" > /dev/null
    shell rm -f "/data/misc/perfetto-traces/$PERFETTO_NAME.pftrace"
  fi
  shell dumpsys gfxinfo "$pkg" > "$OUT/$pkg.gfxinfo"
  # The last 127 frames are read after every swipe, so a frame can be read twice: one per timestamp.
  sort -u -k1,1 "$all" | awk '{ print $2, $3 }' | sort -n | awk '
    { a[NR] = $1; per[$2]++; if ($1 > 1.5 * $2) janky++ }
    END {
      if (NR == 0) { print "0 0 0 0 0 0 0"; exit }
      for (p in per) if (per[p] > best) { best = per[p]; bp = p }
      printf "%d %d %.1f %.1f %.1f %.1f %.2f\n", NR, janky, a[int(NR * 0.5) + 1], a[int(NR * 0.9) + 1], a[int(NR * 0.95) + 1], a[int(NR * 0.99) + 1], bp
    }'
}

gfx_summary() { # pkg -> "frames janky"
  awk '/^Total frames rendered:/ { f = $4 } /^Janky frames:/ && !j { j = $3 } END { print (f + 0), (j + 0) }' "$OUT/$1.gfxinfo"
}

measure_app() { # prefix pkg act apk -> sets <prefix>_* variables
  local pre="$1" pkg="$2" act="$3" apk="$4" v
  reset_pkg "$pkg" "$apk"
  first_run "$pkg" "$act"
  v=$(cold_start "$pkg" "$act");     printf -v "${pre}_START" '%s' "$v"
  v=$(measure_memory "$pkg" "$act"); printf -v "${pre}_MEM" '%s' "$v"
  v=$(measure_idle "$pkg" "$act");   printf -v "${pre}_IDLE" '%s' "$v"
  v=$(scroll_frames "$pkg" "$act");  printf -v "${pre}_SCROLL" '%s' "$v"
  v=$(gfx_summary "$pkg");           printf -v "${pre}_GFX" '%s' "$v"
  # The seeded alarms ring in 5 minutes: gone right away.
  reset_pkg "$pkg" "$apk"
}

# JANK_ONLY=orig|slint: only the scrolling measurement of one app (installed already), nothing else.
if [ -n "${JANK_ONLY:-}" ]; then
  if [ "$JANK_ONLY" = slint ]; then P=$SLINT_PKG; A=$SLINT_ACT; else P=$ORIG_PKG; A=$ORIG_ACT; fi
  echo "display: $(shell dumpsys display | grep -m1 -oE 'renderFrameRate [0-9.]+')" >&2
  [ -n "${JANK_FRESH:-}" ] && { reset_pkg "$P" "${JANK_APK:?}"; first_run "$P" "$A"; }
  echo "frames janky p50 p90 p95 p99 period_ms: $(scroll_frames "$P" "$A")"
  echo "gfxinfo: $(gfx_summary "$P")"
  [ -n "${JANK_FRESH:-}" ] && reset_pkg "$P" "$JANK_APK" # the seeded alarms ring in 5 minutes: gone
  stop_all
  exit 0
fi
echo "device: $SERIAL ($(shell getprop ro.product.cpu.abi), Android $(shell getprop ro.build.version.release)), ${W}x${H} at $DENS dpi, $RUNS cold starts each" >&2
echo "display: $(shell dumpsys display | grep -m1 -oE 'renderFrameRate [0-9.]+')" >&2
echo "measuring the original..." >&2
measure_app O $ORIG_PKG $ORIG_ACT "$ORIG_INSTALL"
echo "measuring the Slint copy..." >&2
measure_app S $SLINT_PKG $SLINT_ACT "$SLINT_INSTALL"
stop_all

read -r O_M1 O_M2 O_WV O_AP <<< "$O_MEM"
read -r S_M1 S_M2 S_WV S_AP <<< "$S_MEM"
read -r O_TOT O_JANK O_P50 O_P90 O_P95 O_P99 O_PER <<< "$O_SCROLL"
read -r S_TOT S_JANK S_P50 S_P90 S_P95 S_P99 S_PER <<< "$S_SCROLL"
read -r O_GF O_GJ <<< "$O_GFX"
read -r S_GF S_GJ <<< "$S_GFX"

kb() { awk -v k="${1:-0}" 'BEGIN { printf "%.0f", k / 1024 }'; }
pct() { awk -v j="${1:-0}" -v t="${2:-0}" 'BEGIN { if (t > 0) printf "%.1f%%", 100 * j / t; else print "n/a" }'; }
row() { printf '%-54s %22s %22s\n' "$1" "$2" "$3"; }

{
  echo
  row "" "AlarmFree (WebView)" "AlarmFree Slint"
  row "APK size (MB)" "$(mb "$(size_of "$ORIG_APK")")" "$(mb "$(size_of "$SLINT_APK")")"
  row "Cold start, median of $RUNS (ms, am start -W)" "$O_START" "$S_START"
  row "  the runs (ms)" "$(tr '\n' ' ' < "$OUT/$ORIG_PKG.starts")" "$(tr '\n' ' ' < "$OUT/$SLINT_PKG.starts")"
  row "PSS after start, main process (MB)" "$(kb "$O_M1")" "$(kb "$S_M1")"
  row "PSS after opening the editor, main process (MB)" "$(kb "$O_M2")" "$(kb "$S_M2")"
  row "WebView processes started by the app (PSS, MB)" "$(kb "$O_WV")" "$(kb "$S_WV")"
  row "PSS of all the app's own processes (MB)" "$(kb "$O_AP")" "$(kb "$S_AP")"
  row "PSS app processes + WebView after the editor (MB)" "$(kb $((O_AP + O_WV)))" "$(kb $((S_AP + S_WV)))"
  row "Idle CPU, ${IDLE_SECONDS} s on the home list (% of one core)" "$O_IDLE" "$S_IDLE"
  row "Vsync period while scrolling (ms)" "$O_PER" "$S_PER"
  row "Frames seen while scrolling (SurfaceFlinger)" "$O_TOT" "$S_TOT"
  row "Janky frames (> 1.5 x vsync period apart)" "$O_JANK ($(pct "$O_JANK" "$O_TOT"))" "$S_JANK ($(pct "$S_JANK" "$S_TOT"))"
  row "Time between frames p50 / p90 (ms)" "$O_P50 / $O_P90" "$S_P50 / $S_P90"
  row "Time between frames p95 / p99 (ms)" "$O_P95 / $O_P99" "$S_P95 / $S_P99"
  row "gfxinfo: frames / janky (see notes)" "$O_GF / $O_GJ" "$S_GF / $S_GJ"
} | tee "$OUT/table.txt"

echo
echo "alarms still scheduled by the two apps (must print nothing):"
shell dumpsys alarm | grep -i alarmfree
echo "raw: $OUT (cold start times per run: *.starts, frame deltas: *.frames, gfxinfo: *.gfxinfo, processes: *.procs-after-editor, cpu: *.cpuinfo *.top)"
