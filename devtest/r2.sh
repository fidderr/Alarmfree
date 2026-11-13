#!/bin/bash
# Round 2 of the design pass: capture the same screens in both apps (alarms: 06:20 once tomorrow, 07:00 Werk
# weekdays, 08:40 weekend). Source after nothing; it sources phone_lib.sh itself.
# Usage: source devtest/r2.sh; capset o|s
source /c/Git/alarmfree-slint/devtest/phone_lib.sh
OUT=/c/Git/alarmfree-slint/devtest-out/phone/r2; WOUT=C:/Git/alarmfree-slint/devtest-out/phone/r2
mkdir -p $OUT

# $1 prefix o|s ; the app must be on its home with the three alarms.
capset() {
  local p=$1 add_y rek_y
  if [ "$p" = o ]; then add_y=921; rek_y=285; else add_y=923; rek_y=245; fi
  sleep 5; shot $p-home-4
  swp 900 1240 250 1240 300; sleep 1; shot $p-home-swipe; swp 250 1240 900 1240 300; sleep 1
  vt 405 560; sleep 1.3; shot $p-alarm-menu; back; sleep 1.2
  vt 270 560; sleep 1.6; shot $p-editor-top
  sheet $p-time-sheet 270 165
  sheet $p-label-sheet 270 522
  sheet $p-sound-sheet 270 603
  sheet $p-vibration-sheet 270 684
  sheet $p-snooze-sheet 270 765
  vt 270 846; sleep 1.3; shot $p-challenges-sheet
  vt 270 $add_y; sleep 1.3; shot $p-add-challenge-sheet
  vt 270 $rek_y; sleep 1.5; shot $p-challenge-detail
  vt 270 1035; sleep 1.3; shot $p-challenges-sheet-filled
  back; sleep 1.2
  swp 270 1700 270 600 300; sleep 1; shot $p-editor-bottom
  sheet $p-ringlook-sheet 270 595
  sheet $p-limit-sheet 270 676
  sheet $p-wake-sheet 270 757
  sheet $p-notice-sheet 270 838
  back; sleep 1.3; shot $p-discard-sheet
}
