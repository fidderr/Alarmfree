#!/bin/bash
# Round 3 of the design pass helpers. Source it: source devtest/r3.sh
source /c/Git/alarmfree-slint/devtest/phone_lib.sh
OUT=/c/Git/alarmfree-slint/devtest-out/phone/r3; WOUT=C:/Git/alarmfree-slint/devtest-out/phone/r3
mkdir -p $OUT
go_o() { sh am force-stop $L; sh am force-stop $O; sh am start -n $O/$OA >/dev/null; sleep 3; }
go_s() { sh am force-stop $O; sh am force-stop $L; sh am start -n $L/$LA >/dev/null; sleep 3; }
# side-by-side montage of o-<n>.png and s-<n>.png into _p.png (400 px per side)
pair() { python -I C:/Git/alarmfree-slint/devtest/phone_montage.py $WOUT/_p.png ${2:-450} $WOUT/o-$1.png $WOUT/s-$1.png; }
# final pass: the same screens in either app. Usage: fin_a o|s (settings, theme, history, reliability)
fin_a() {
  local p=$1; [ $p = o ] && go_o || go_s; sleep 2
  shot $p-f-home
  vt 450 1060; sleep 1.5; shot $p-f-settings1
  swp 270 1700 270 300 300; sleep 1; shot $p-f-settings2
  vt 270 238; sleep 1.5; shot $p-f-newalarms; back; sleep 1.2
  vt 450 1060; sleep 1.2; swp 270 1700 270 300 300; sleep 1; vt 270 781; sleep 1.4; shot $p-f-backup; back; sleep 1
  vt 90 1060; sleep 1.2; vt 450 1060; sleep 1.3; vt 270 334; sleep 1.5; shot $p-f-theme
  vt 270 806; sleep 1; swp 270 1500 270 500 300; sleep 1; shot $p-f-own; vt 270 670; sleep 1.5; shot $p-f-colour; vt 270 1035; sleep 1; back; sleep 1
  vt 270 1060; sleep 1.4; shot $p-f-history; vt 270 476; sleep 1.2; shot $p-f-hist-stats; vt 270 794; sleep 1.2; shot $p-f-hist-detail
  vt 450 1060; sleep 1.3; vt 270 165; sleep 1.6; shot $p-f-rel1; swp 270 1700 270 400 300; sleep 1; shot $p-f-rel2
}
# fin_b o|s: add six challenges to the 07:00 draft (not saved) and practice each
fin_b() {
  local p=$1; [ $p = o ] && go_o || go_s; sleep 2
  vt 200 445; sleep 1.7; vt 270 846; sleep 1.5
  local ay; [ $p = o ] && ay=921 || ay=923
  for y in 277 367 457 547 637 817; do vt 270 $ay; sleep 1.3; vt 100 $y; sleep 1.2; vt 270 1035; sleep 1.2; need || return; done
  for spec in math:336 typing:432 memory:528 reaction:624 hold:720 shake:816; do
    local n=${spec%%:*} y=${spec##*:} px; [ $p = o ] && px=393 || px=405
    vt $px $y; sleep 2.2; shot $p-f-pr-$n; vt 460 92; sleep 1.6; need || return
  done
}
