#!/bin/bash
# Side by side: AlarmFree (Dioxus, UI in a WebView) and AlarmFree Slint (native UI) on one
# emulator. Prints a table: APK size, cold start (median of RUNS), memory (PSS) after the start
# and after opening the editor, and the frame times of scrolling the list (jank).
#
#   devtest/compare.sh                       # uses the APKs below, on emulator-5556
#   SERIAL=emulator-5556 ORIG_APK=... SLINT_APK=... RUNS=5 devtest/compare.sh
#
# Both APKs have to be builds for the device's ABI and release builds (a debug build says
# nothing about speed). The original is built from a copy of its source in build/original,
# never in C:/Git/Alarmfree (see docs/SLINT_PORT.md, "Comparing"):
#   (cd build/original && TARGET=x86_64-linux-android ./build.sh release)
#   TARGET=x86_64-linux-android ./build.sh release
# They are unsigned or debug-signed; this script signs a copy with the debug key to install.
set -uo pipefail
export MSYS_NO_PATHCONV=1

cd "$(dirname "$0")/.."
SDK="${ANDROID_HOME:-$LOCALAPPDATA/Android/Sdk}"
ADB="${ADB:-$SDK/platform-tools/adb.exe}"
SERIAL="${SERIAL:-emulator-5556}"
RUNS="${RUNS:-5}"
OUT="${OUT:-devtest-out/compare}"
ORIG_APK="${ORIG_APK:-build/original/target/dx/alarmfree/release/android/app/app/build/outputs/apk/release/app-release-unsigned.apk}"
SLINT_APK="${SLINT_APK:-build/alarmfree-slint-release.apk}"
ORIG_PKG=com.fidderr.alarmfree
ORIG_ACT=dev.dioxus.main.MainActivity
SLINT_PKG=com.fidderr.alarmfree.slint
SLINT_ACT=com.fidderr.alarmfree.slint.MainActivity
mkdir -p "$OUT"

# The phone and the other job's emulator are never touched.
case "$SERIAL" in
  emulator-5556) ;;
  *) [ "${ALLOW_OTHER_SERIAL:-}" = 1 ] || { echo "SERIAL must be emulator-5556 (set ALLOW_OTHER_SERIAL=1 to override)"; exit 2; } ;;
esac
dev() { "$ADB" -s "$SERIAL" "$@"; }
shell() { dev shell "$@" | tr -d '\r'; }

[ -f "$ORIG_APK" ] || { echo "missing $ORIG_APK"; exit 1; }
[ -f "$SLINT_APK" ] || { echo "missing $SLINT_APK"; exit 1; }

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

dev install -r "$ORIG_INSTALL" > /dev/null || { echo "install of the original failed"; exit 1; }
dev install -r "$SLINT_INSTALL" > /dev/null || { echo "install of the Slint copy failed"; exit 1; }
# The phone's own permission prompts must not cover what is measured.
for pkg in $ORIG_PKG $SLINT_PKG; do
  for p in POST_NOTIFICATIONS ACTIVITY_RECOGNITION CAMERA; do dev shell pm grant $pkg android.permission.$p > /dev/null 2>&1 || true; done
done
shell input keyevent KEYCODE_WAKEUP > /dev/null
shell wm dismiss-keyguard > /dev/null 2>&1 || true

# Both apps from clean data, past the original's welcome page (its "Get started" button), so
# both show their empty alarm list. Alarms left over from an earlier run would ring in the middle
# of a measurement.
fresh_state() {
  stop_all
  shell pm clear $ORIG_PKG > /dev/null
  shell pm clear $SLINT_PKG > /dev/null
  for pkg in $ORIG_PKG $SLINT_PKG; do
    for p in POST_NOTIFICATIONS ACTIVITY_RECOGNITION CAMERA; do dev shell pm grant $pkg android.permission.$p > /dev/null 2>&1 || true; done
  done
  shell am start -W -n "$ORIG_PKG/$ORIG_ACT" > /dev/null
  sleep 5
  shell input tap 540 2164
  sleep 2
  stop_all
}

median() { sort -n | awk '{ a[NR] = $1 } END { if (NR % 2) print a[(NR + 1) / 2]; else print (a[NR / 2] + a[NR / 2 + 1]) / 2 }'; }

home_screen() { shell input keyevent KEYCODE_HOME; sleep 1; }
stop_all() { shell am force-stop $ORIG_PKG; shell am force-stop $SLINT_PKG; sleep 1; }

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

# PSS in MB of the app's process, and of the WebView processes it brought along.
pss_kb() { # pkg
  shell dumpsys meminfo "$1" | grep -m1 "TOTAL PSS:" | sed -E 's/.*TOTAL PSS: *([0-9]+).*/\1/'
}
webview_pss_kb() {
  # Every process of the WebView package; only the app under test has a page open.
  shell dumpsys meminfo | awk '
    /^Total PSS by process:/ { on = 1; next }
    /^Total PSS by OOM/ { on = 0 }
    on && /(webview|sandboxed_process)/ { gsub(",", "", $1); sum += $1 }
    END { print sum + 0 }'
}

measure_memory() { # pkg act -> "after_start_kb after_edit_kb webview_kb"
  local pkg="$1" act="$2" a b w w2 base
  stop_all
  home_screen
  base=$(webview_pss_kb)
  shell am start -W -n "$pkg/$act" > /dev/null
  sleep 5
  a=$(pss_kb "$pkg")
  w=$(webview_pss_kb)
  shell input tap 965 1990 # the + button: the editor
  sleep 3
  b=$(pss_kb "$pkg")
  w2=$(webview_pss_kb)
  shell input keyevent KEYCODE_BACK
  sleep 1
  [ "$w2" -gt "$w" ] && w=$w2
  # What the WebView's helper processes held before the app started is not the app's.
  w=$((w - base))
  [ "$w" -lt 0 ] && w=0
  echo "$a $b $w"
}

# Quick alarms, so there is a list to scroll: the + 5 pill is where it is on both.
seed_alarms() { # pkg act
  stop_all
  home_screen
  shell am start -W -n "$1/$2" > /dev/null
  sleep 4
  local i
  for ((i = 0; i < 14; i++)); do
    shell input tap 216 385
    sleep 0.6
  done
  sleep 1
}

# The frames of the app's own layer from SurfaceFlinger, which sees every app whatever draws it
# (dumpsys gfxinfo only counts what Android's own renderer draws, which neither a WebView page
# nor Slint's GPU surface is). It keeps the last 127 frames, so each trial scrolls for 1.5 s
# and reads them straight after.
layer_of() { # pkg
  shell dumpsys SurfaceFlinger --list | grep "^$1/.*Activity#" | head -1
}

frame_times() { # layer -> the deltas of the present times, in ms, one per line
  shell dumpsys SurfaceFlinger --latency "'$1'" | awk '
    NR == 1 { period = $1; next }
    NF == 3 && $2 < 9000000000000000000 && $2 > 0 { if (prev) print ($2 - prev) / 1000000; prev = $2 }
    END { if (NR == 0) print "" }'
}

scroll_frames() { # pkg act -> "frames janky p50 p90 p95 p99" (ms between frames; janky > 1.5 x 16.7 ms)
  local pkg="$1" act="$2" layer trial all="$OUT/$pkg.frames"
  seed_alarms "$pkg" "$act"
  layer=$(layer_of "$pkg")
  echo "  layer: $layer" >&2
  : > "$all"
  for trial in 1 2 3 4 5 6; do
    shell input swipe 540 1700 540 600 1500
    frame_times "$layer" | awk '$1 < 100' >> "$all"
    shell input swipe 540 600 540 1700 1500
    frame_times "$layer" | awk '$1 < 100' >> "$all"
  done
  sort -n "$all" | awk '
    { a[NR] = $1; if ($1 > 25) janky++ }
    END {
      if (NR == 0) { print "0 0 0 0 0 0"; exit }
      printf "%d %d %.1f %.1f %.1f %.1f\n", NR, janky, a[int(NR * 0.5) + 1], a[int(NR * 0.9) + 1], a[int(NR * 0.95) + 1], a[int(NR * 0.99) + 1]
    }'
}

fresh_state
echo "device: $SERIAL ($(shell getprop ro.product.cpu.abi), Android $(shell getprop ro.build.version.release)), $RUNS cold starts each" >&2
echo "measuring the original..." >&2
O_START=$(cold_start $ORIG_PKG $ORIG_ACT)
read -r O_M1 O_M2 O_WV <<< "$(measure_memory $ORIG_PKG $ORIG_ACT)"
read -r O_TOT O_JANK O_P50 O_P90 O_P95 O_P99 <<< "$(scroll_frames $ORIG_PKG $ORIG_ACT)"
echo "measuring the Slint copy..." >&2
shell pm clear $SLINT_PKG > /dev/null
for p in POST_NOTIFICATIONS ACTIVITY_RECOGNITION CAMERA; do dev shell pm grant $SLINT_PKG android.permission.$p > /dev/null 2>&1 || true; done
S_START=$(cold_start $SLINT_PKG $SLINT_ACT)
read -r S_M1 S_M2 S_WV <<< "$(measure_memory $SLINT_PKG $SLINT_ACT)"
read -r S_TOT S_JANK S_P50 S_P90 S_P95 S_P99 <<< "$(scroll_frames $SLINT_PKG $SLINT_ACT)"

kb() { awk -v k="${1:-0}" 'BEGIN { printf "%.0f", k / 1024 }'; }
pct() { awk -v j="${1:-0}" -v t="${2:-0}" 'BEGIN { if (t > 0) printf "%.1f%%", 100 * j / t; else print "n/a" }'; }
row() { printf '%-44s %16s %16s\n' "$1" "$2" "$3"; }

{
  echo
  row "" "AlarmFree (WebView)" "AlarmFree Slint"
  row "APK size (MB)" "$(mb "$(size_of "$ORIG_APK")")" "$(mb "$(size_of "$SLINT_APK")")"
  row "Cold start, median of $RUNS (ms, am start -W)" "$O_START" "$S_START"
  row "PSS after start, app process (MB)" "$(kb "$O_M1")" "$(kb "$S_M1")"
  row "PSS after opening the editor (MB)" "$(kb "$O_M2")" "$(kb "$S_M2")"
  row "WebView processes (PSS, MB)" "$(kb "$O_WV")" "$(kb "$S_WV")"
  row "PSS app + WebView after the editor (MB)" "$(kb $((O_M2 + O_WV)))" "$(kb $((S_M2 + S_WV)))"
  row "Frames seen while scrolling" "$O_TOT" "$S_TOT"
  row "Janky frames (> 25 ms apart)" "$O_JANK ($(pct "$O_JANK" "$O_TOT"))" "$S_JANK ($(pct "$S_JANK" "$S_TOT"))"
  row "Time between frames p50 / p90 (ms)" "$O_P50 / $O_P90" "$S_P50 / $S_P90"
  row "Time between frames p95 / p99 (ms)" "$O_P95 / $O_P99" "$S_P95 / $S_P99"
} | tee "$OUT/table.txt"
echo
echo "raw: $OUT (cold start times per run: *.starts, gfxinfo: *.gfxinfo)"
