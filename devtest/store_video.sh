#!/bin/bash
# The video Google Play asks for with the foreground service declaration (media playback): the
# user switches an alarm on and locks the phone, the alarm rings over the lock screen with its
# sound playing from a foreground service, and solving the challenge stops both. Captions say
# what happens, for the reviewer: a screen recording carries no sound.
#
# Usage: bash devtest/store_video.sh    -> release/AlarmFree-foreground-service.mp4
cd "$(dirname "$0")/.." || exit 1
export APK="${APK:-target/dx/alarmfree/debug/android/app/app/build/outputs/apk/debug/app-debug.apk}"
export AVD="${AVD:-af36}" SERIAL="${SERIAL:-emulator-5554}" OUT="devtest-out/store"
RESULTS="$OUT/results_video.txt"
CURRENT=store_video
. devtest/lib.sh
: > "$RESULTS"
FFMPEG=${FFMPEG:-ffmpeg}
FINAL=release/AlarmFree-foreground-service.mp4
mkdir -p release

reboot_and_wait || { echo "the emulator did not start"; exit 1; }
said=$(adb_ install -r -g "$APK" 2>&1)
case "$said" in *Success*) ;; *) echo "install failed: $said"; exit 1 ;; esac
dev wm size 1080x1920
dev wm density 420
# The status bar in 24 hours, like the app: a reviewer sees the same time twice.
OLD_12_24=$(dev settings get system time_12_24)
dev settings put system time_12_24 24

fresh
use_settings '"Dark"' '"English"' '"setup_done":true,"language_follows_phone":false,"less_motion":false,"time_format":"H24","accent":"Blue"'
next_minute 40
ALARM_EXTRA=',"note":"Take the laptop charger","ring_look":{"backdrop":"Aurora"}'
seed wake "$(alarm_json wake "$T_H" "$T_M" "$EVERY_DAY" null air 0 0 3 5 '[["challenge.math","{\"difficulty\":\"easy\"}"]]')"
ALARM_EXTRA=
{
    printf "UPDATE alarms SET data = replace(data, '\"label\":\"wake\"', '\"label\":\"Wake up\"') WHERE id='wake';\n"
    printf "UPDATE alarms SET data = replace(data, '\"enabled\":true', '\"enabled\":false') WHERE id='wake';\n"
} | sql
HM=$(printf '%02d:%02d' "$T_H" "$T_M")
start_app
on_screen .alarm-card 20 || { echo "no list"; exit 1; }
sleep 1

dev rm -f /sdcard/fgs.mp4
adb_ shell screenrecord --bit-rate 8000000 --time-limit 170 /sdcard/fgs.mp4 &
REC=$!
sleep 1.5
REC_START=$SECONDS
sleep 2
T_SWITCH=$((SECONDS - REC_START))
click .alarm-switch 0 3
T_LOCK=$((SECONDS - REC_START))
dev input keyevent KEYCODE_SLEEP
if await_ring wake 75 && on_screen .ring-off 20; then
    T_RING=$((SECONDS - REC_START))
    sleep 7
    T_STOP=$((SECONDS - REC_START))
    click .ring-off 0 1.5
    on_screen .math-expression 5 || echo "no challenge"
    SUM=$(read_text .math-expression)
    digits=$(($(echo "$SUM" | sed 's/×/*/; s/ //g')))
    while [ -n "$digits" ]; do
        js "[...document.querySelectorAll('.math-key')].find(k => k.textContent === '${digits:0:1}').click(); return 'ok'" > /dev/null
        digits=${digits:1}
        sleep 0.7
    done
    solved() { state_is wake scheduled; }
    wait_for 8 solved || echo "solving it did not end it: state=$(state wake)"
    sleep 4
else
    echo "the alarm did not ring: state=$(state wake)"
    T_RING=$((SECONDS - REC_START)) T_STOP=$T_RING
fi
T_END=$((SECONDS - REC_START))
dev pkill -INT screenrecord
wait "$REC"
sleep 1
adb_ pull /sdcard/fgs.mp4 "$(cygpath -m "$OUT/fgs_raw.mp4")" > /dev/null
dev rm -f /sdcard/fgs.mp4
dev am force-stop "$PKG"
dev wm size reset
dev wm density reset
case "$OLD_12_24" in 12 | 24) dev settings put system time_12_24 "$OLD_12_24" ;; *) dev settings delete system time_12_24 > /dev/null ;; esac
emulator_stop

# Captions: one text file each, so nothing in them needs escaping for ffmpeg.
C="$OUT/captions"
mkdir -p "$C"
printf 'The user switches on\nan alarm for %s.' "$HM" > "$C/1.txt"
printf 'The phone is locked,\nthe app is in the background.' > "$C/2.txt"
printf '%s: the alarm rings over the lock screen.\nIts sound plays from a foreground service\n(media playback). The recording has no audio.' "$HM" > "$C/3.txt"
printf 'The user stops it by solving a sum.\nThe sound and the foreground service stop.' > "$C/4.txt"
FONT="C\\:/Windows/Fonts/segoeui.ttf"
cap() { # file from to
    printf "drawtext=fontfile='%s':textfile='%s':fontsize=30:fontcolor=white:line_spacing=8:box=1:boxcolor=black@0.7:boxborderw=18:x=(w-text_w)/2:y=h-text_h-70:enable='between(t,%s,%s)'" \
        "$FONT" "$(cygpath -m "$C/$1.txt" | sed 's/:/\\:/')" "$2" "$3"
}
FILTER="scale=720:-2,$(cap 1 0 "$T_LOCK"),$(cap 2 "$T_LOCK" "$T_RING"),$(cap 3 "$T_RING" "$T_STOP"),$(cap 4 "$T_STOP" $((T_END + 5)))"
"$FFMPEG" -y -loglevel error -i "$OUT/fgs_raw.mp4" -vf "$FILTER" -c:v libx264 -pix_fmt yuv420p -crf 23 -an "$OUT/fgs_captioned.mp4" || exit 1
# The locked phone is half a minute of black: keep three seconds of it. The longest stretch
# without frames in the recording is that time.
read -r GAP_FROM GAP_TO <<< "$(ffprobe -v error -select_streams v -show_entries packet=pts_time -of csv=p=0 "$OUT/fgs_raw.mp4" |
    awk -F, 'NR > 1 && $1 - prev > best { best = $1 - prev; from = prev; to = $1 } { prev = $1 } END { print from, to }')"
CUT_FROM=$(awk -v f="$GAP_FROM" 'BEGIN { print f + 3 }')
CUT_TO=$(awk -v t="$GAP_TO" 'BEGIN { print t - 0.3 }')
"$FFMPEG" -y -loglevel error -i "$OUT/fgs_captioned.mp4" -filter_complex     "[0:v]trim=0:$CUT_FROM,setpts=PTS-STARTPTS[a];[0:v]trim=start=$CUT_TO,setpts=PTS-STARTPTS[b];[a][b]concat=n=2:v=1[v]"     -map "[v]" -c:v libx264 -pix_fmt yuv420p -crf 23 -an "$FINAL" &&
    echo "video: $FINAL ($(du -h "$FINAL" | cut -f1)), switch at ${T_SWITCH}s, lock ${T_LOCK}s, ring ${T_RING}s, stop ${T_STOP}s, end ${T_END}s; cut ${CUT_FROM}-${CUT_TO}s"
