#!/bin/bash
# Helpers for driving the real phone safely: every input and capture first checks the focus.
export MSYS_NO_PATHCONV=1
export PATH="$PATH:$LOCALAPPDATA/Android/Sdk/platform-tools"
S=bb741d88
OUT=/c/Git/alarmfree-slint/devtest-out/phone/design
WOUT=C:/Git/alarmfree-slint/devtest-out/phone/design
O=com.fidderr.alarmfree; OA=dev.dioxus.main.MainActivity
L=com.fidderr.alarmfree.slint; LA=com.fidderr.alarmfree.slint.MainActivity
sh() { adb -s $S shell "$@" | tr -d '\r'; }
focus() { sh dumpsys window | grep mCurrentFocus; }
ok() { focus | grep -Eq 'com\.fidderr\.alarmfree(\.slint)?/'; }
need() { ok || { echo "ABORT focus: $(focus | grep -oE 'Window\{[^}]*\}')"; return 1; }; }
tap() { need && sh input tap "$1" "$2"; }
back() { need && sh input keyevent KEYCODE_BACK; }
swp() { need && sh input swipe "$@"; }
# shot name : raw capture only if an AlarmFree window has focus; status bar cropped; stored as <name>.png
shot() { need || return 1; adb -s $S exec-out screencap -p > $OUT/_raw.png && python -I -c "
from PIL import Image
im=Image.open('$WOUT/_raw.png').convert('RGB'); im.crop((0,120,im.width,im.height)).save('$WOUT/$1.png')
im.crop((0,120,im.width,im.height)).resize((540,(im.height-120)//2)).save('$WOUT/_v.png')"; rm -f $OUT/_raw.png; }
stats() { # counts of crash/anr entries of our packages only
  local c a b
  c=$(adb -s $S logcat -b crash -d | grep -c -i -E "alarmfree")
  for t in data_app_crash data_app_anr data_app_native_crash; do
    echo "$t: $(sh dumpsys dropbox --print $t | grep -E '^Process:|^Package:' | grep -c -i alarmfree)"
  done
  echo "logcat crash buffer lines naming alarmfree: $c"
  echo "pids: orig=$(sh pidof $O) origSentinel=$(sh pidof $O:sentinel) slint=$(sh pidof $L) slintSentinel=$(sh pidof $L:sentinel)"
}
vt() { tap $(( $1 * 2 )) $(( $2 * 2 + 120 )); }   # tap in coordinates of the 540-wide preview (_v.png)
sheet() { # name x y : tap, capture, Back (closes the sheet)
  vt $2 $3; sleep 1.3; shot $1; back; sleep 1.2; }
mont() { local o=$1 w=$2; shift 2; local a=(); for n in "$@"; do a+=("$WOUT/$n.png"); done; python -I C:/Git/alarmfree-slint/devtest/phone_montage.py $WOUT/_m.png $w "${a[@]}"; }
# minutes since 07:00 of (phone time + $1 minutes, rounded up to the next full minute)
delta07() { local h m s; read h m s <<<"$(sh date +%H:%M:%S | tr ':' ' ')"; h=$((10#$h)); m=$((10#$m)); s=$((10#$s)); local t=$((h*60+m+$1)); [ $s -gt 5 ] && t=$((t+1)); echo $((t-420)); LAST_TARGET=$(printf '%02d:%02d' $((t/60)) $((t%60))); }
# plus buttons of the Slint editor (+5 at 476,200 ; +1 at 476,122 in preview coordinates)
s_time() { local d=$1 i; for ((i=0;i<d/5;i++)); do vt 476 200; sleep .25; done; for ((i=0;i<d%5;i++)); do vt 476 122; sleep .25; done; }
s_new() { # label ahead_min [math] -> creates a Slint alarm; sets TARGET
  local d; d=$(delta07 $2); TARGET=$(date -d "" +%H:%M 2>/dev/null); local lb=$1
  vt 473 941; sleep 1.5; s_time $d
  vt 270 529; sleep 1.3; vt 270 340; sleep .6; sh input text $lb; sleep .5; back; sleep 1; vt 270 1035; sleep 1
  vt 270 614; sleep 1.3; vt 270 265; sleep .6; back; sleep 1
  vt 270 1053; sleep 2; }
o_time() { local d=$1 i; for ((i=0;i<d/5;i++)); do vt 474 200; sleep .25; done; for ((i=0;i<d%5;i++)); do vt 474 135; sleep .25; done; }
o_new() { local d; d=$(delta07 $2); local lb=$1
  vt 474 942; sleep 1.5; o_time $d
  vt 270 520; sleep 1.5; vt 270 652; sleep 1; sh input text $lb; sleep .8; vt 270 568; sleep 1
  vt 270 601; sleep 1.3; vt 200 208; sleep .6; vt 270 1035; sleep 1
  vt 270 1041; sleep 2; }
o_aurora() { vt 270 950; sleep 1.3; vt 208 564; sleep .6; vt 270 1035; sleep 1; }
o_math() { vt 270 845; sleep 1.3; vt 270 924; sleep 1.3; vt 100 270; sleep 1.3; vt 75 797; sleep .6; vt 270 1035; sleep 1; vt 270 1035; sleep 1; }
s_math() { vt 270 871; sleep 1.3; vt 270 945; sleep 1.3; vt 100 270; sleep 1.3; vt 78 825; sleep .6; vt 270 1035; sleep 1; vt 270 1035; sleep 1; }
s_new2() { # label ahead_min [math]  (Slint)
  local d; d=$(delta07 $2); local lb=$1
  vt 473 941; sleep 1.5; s_time $d
  vt 270 529; sleep 1.3; vt 270 340; sleep .6; sh input text $lb; sleep .5; back; sleep 1; vt 270 1035; sleep 1
  vt 270 614; sleep 1.3; vt 270 265; sleep .6; back; sleep 1
  [ -n "$3" ] && s_math
  shot cur; vt 270 1053; sleep 2; }
