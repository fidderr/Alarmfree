#!/bin/bash
# s64: an update from the Play app (Dioxus 0.4.1, same package) to this Slint build keeps the user's
# alarms, settings, own sound and history, and an alarm armed by the old app still rings.
#
#   DX_APK=build/dioxus-0.4.1-debug.apk bash devtest/upgrade.sh <avd> <port>
# Build the old one in a worktree of the dioxus branch at 2f65c82 (see README "Upgrade test").
cd "$(dirname "$0")/.." || exit 1
avd=${1:-afg16} port=${2:-5558}
DX_APK=${DX_APK:-build/dioxus-0.4.1-debug.apk}
NEW_APK=${APK:-build/alarmfree-slint-debug.apk}
export AVD=$avd SERIAL=emulator-$port OUT="devtest-out/upgrade_$avd"
[ "$avd" = afg12 ] && export EMULATOR_GPU=swiftshader_indirect
. devtest/lib.sh
claim_device
rm -f "$RESULTS"
[ -n "${NO_BOOT:-}" ] || reboot_and_wait || { echo "FAIL $avd did not start"; exit 1; }
RUN_START=$SECONDS
adb_ logcat -c

SLINT_ACTIVITY=$ACTIVITY
on_home() { wait_for "${1:-6}" on_screen_no 0; }
dx_start() { ACTIVITY=dev.dioxus.main.MainActivity start_app; }
dump_state() { # what must be the same before and after
    printf "SELECT id, data FROM alarms ORDER BY id;\n" | sql | sed -E 's/"order":[0-9]+,?//'
    printf "SELECT '--', theme, locale, min_volume_percent, prefs FROM settings;\n" | sql
    printf "SELECT '--', id, alarm_id, alarm_label, fire_timestamp, outcome FROM alarm_events ORDER BY id;\n" | sql
    as_app "ls files/sounds/custom; cmp files/sounds/custom/mytune.ogg files/sounds/default/air.ogg && echo same-bytes"
}

scenario s64_update_from_play_version
adb_ uninstall "$PKG" > /dev/null 2>&1
said=$(adb_ install -r -g "$DX_APK" 2>&1)
case "$said" in *Success*) ;; *) fail "old app did not install: $said"; exit 1 ;; esac
shells_open || open_shells
grant_all
dx_start
wait_file files/alarmfree.db 90 || { fail "the old app made no database"; exit 1; }
sleep 4
dev am force-stop "$PKG"
problems=""

# What a user has: four alarms (days, sounds, challenges, a label, one off, one with their own sound),
# settings, history, one ring that really happened.
next_minute
T1=$T
as_app "mkdir -p files/sounds/custom && cp files/sounds/default/air.ogg files/sounds/custom/mytune.ogg"
# The emulator's zone, not the host's:
off=$(dev date +%z | tr -d '\r'); sign=${off:0:1}; oh=${off:1:2}; om=${off:3:2}
shift_s=$((10#$oh * 3600 + 10#$om * 60))
[ "$sign" = "-" ] && shift_s=$((-shift_s))
LATER_H=$(date -u -d "@$((T1 + 180 + shift_s))" +%-H); LATER_M=$(date -u -d "@$((T1 + 180 + shift_s))" +%-M)
seed ring "$(alarm_json ring "$T_H" "$T_M" "$EVERY_DAY" null air 0 0)"
seed later "$(alarm_json later "$LATER_H" "$LATER_M" "$EVERY_DAY" null funk 0 0 2 5)"
seed weekdays "$(alarm_json weekdays 6 45 '[false,true,true,true,true,true,false]' null happy 0 20 3 1 '[["challenge.math","{\"difficulty\":\"hard\"}"]]')"
seed own "$(alarm_json own 9 15 '[true,false,false,false,false,false,true]' null mytune.ogg 0 10)"
seed off "$(alarm_json off 22 0 "$EVERY_DAY" null power 0 0 | sed 's/"enabled":true/"enabled":false/')"
MIN_VOLUME=70 use_settings '"Light"' '"Dutch"' '"setup_done":true,"language_follows_phone":false,"touch_feedback":false,"confetti":false,"sound_names":{"mytune.ogg":"My tune"},"hidden_warnings":["Overlay"]'
history_row h1 1
history_row h2 3
dx_start
wait_file files/sentinel/armed/later.armed 30 || problems="$problems; the old app did not arm 'later'"
await_ring ring 30 || problems="$problems; the old app's ring did not happen"
action dismiss:ring
await_state ring scheduled 10 || problems="$problems; ring state after dismissing: $(state ring)"
sleep 2
# Not force-stopped: a user's app is not, and a stopped app gets no MY_PACKAGE_REPLACED.
printf "PRAGMA wal_checkpoint(TRUNCATE);\n" | sql > /dev/null
before=$(dump_state)
echo "$before" > "$OUT/before.txt"
armed_before=$(as_app "ls files/sentinel/armed" | tr '\n' ' ')
hist_before=$(history_count)
[ "$(echo "$before" | grep -c '^weekdays|\|^own|\|^off|\|^later|\|^ring|')" = 5 ] || problems="$problems; the old app's state is not what was seeded"
dead

# The update, over the old one. The app is not opened: the alarm armed for later must ring by itself.
said=$(adb_ install -r -g "$NEW_APK" 2>&1)
case "$said" in *Success*) ;; *) fail "update did not install: $said"; exit 1 ;; esac
shells_open || open_shells
echo "version now: $(dev dumpsys package "$PKG" | grep -m1 versionName | tr -d ' ')"
sleep 3
dead
note "alarms of the package after the update: $(dev dumpsys alarm | grep -c "$PKG") lines, $(dev dumpsys alarm | grep -E 'RTC_WAKEUP.*fidderr|Alarm\{.*fidderr' | head -5 | cut -c1-200)"
jump_to $((T1 + 180 - 4))
sleep 1.5
kill_all
T=$((T1 + 180))
if await_sound 45; then
    note "the armed alarm rang by itself, ${SOUND_AT}s after its time"
    on_screen Uitzetten 15 > /dev/null 2>&1 || problems="$problems; the ring screen is not in the saved language"
    screenshot s64_ring_after_update
    action dismiss:later
    await_state later scheduled 10 || problems="$problems; dismissing the ring: state $(state later)"
    wait_for 8 sound_off || problems="$problems; still sounding after dismissing"
else
    problems="$problems; after the update the armed alarm did not ring (state=$(state later), job=$(job_status later))"
fi

# Open the new app: the same things are there, as the window shows them.
dev am force-stop "$PKG"
ACTIVITY=$SLINT_ACTIVITY start_app
wait_for 40 page_is_up || problems="$problems; the window did not come up"
wait_for 10 on_screen_no 0 || problems="$problems; not on the alarm list (screen $(screen_no))"
sleep 1
dev am force-stop "$PKG"
sleep 1
printf "PRAGMA wal_checkpoint(TRUNCATE);\n" | sql > /dev/null
after=$(dump_state)
echo "$after" > "$OUT/after.txt"
# 'later' has rung and been dismissed in between: its state is not compared, nor the history row that made.
strip() { grep -v '^later|' | grep -v '^-- *|.*later' ; }
if [ "$(echo "$before" | strip)" != "$(echo "$after" | strip)" ]; then
    problems="$problems; stored data changed: $(diff <(echo "$before" | strip) <(echo "$after" | strip) | head -c 600)"
fi
[ "$(history_count)" -ge "$hist_before" ] || problems="$problems; history lost rows"
armed_after=$(as_app "ls files/sentinel/armed" | tr '\n' ' ')
[ "$armed_after" = "$armed_before" ] || problems="$problems; armed before: $armed_before, after: $armed_after"

# And on the screens.
ACTIVITY=$SLINT_ACTIVITY start_app
wait_for 40 page_is_up
on_home 10 || problems="$problems; not on the alarm list"
rows=$(ui snapshot | grep -oE '"label":"[^"]*","on":(true|false)' | tr '\n' ' ')
for want in weekdays own off later ring; do
    case "$rows" in *"\"$want\""*) ;; *) problems="$problems; no card for $want ($rows)" ;; esac
done
screenshot s64_after_update
tap Geschiedenis L 0.5
wait_for 5 on_screen_no 2 || problems="$problems; no history screen"
screenshot s64_history_after_update
if [ -z "$problems" ]; then
    pass "update kept 5 alarms, settings, own sound, history; the armed alarm rang by itself"
else
    fail "${problems#; }"
fi
dev am force-stop "$PKG"
echo "===== SUMMARY ($SERIAL) ====="
cat "$RESULTS"
echo "in $(((SECONDS - RUN_START) / 60)) min $(((SECONDS - RUN_START) % 60)) s"
[ -n "${KEEP:-}" ] || emulator_stop
