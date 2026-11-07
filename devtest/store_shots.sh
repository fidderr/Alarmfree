#!/bin/bash
# Screenshots for the Play Store listing: 1080x1920 (9:16, which Play takes), a clean status bar,
# and alarms someone could really have, in the language of the listing. Not a test: a picture of
# each screen a visitor of the store should see, reached the way the tour reaches it.
#
# Usage: bash devtest/store_shots.sh [lang...]    (en nl es fr de, all of them by default)
# Pictures in store/screenshots/<lang>/. Uses the debug APK that is already built.
cd "$(dirname "$0")/.." || exit 1
export APK="${APK:-target/dx/alarmfree/debug/android/app/app/build/outputs/apk/debug/app-debug.apk}"
export AVD="${AVD:-af36}" SERIAL="${SERIAL:-emulator-5554}" OUT="devtest-out/store"
RESULTS="$OUT/results.txt"
CURRENT=store_shots
. devtest/lib.sh
: > "$RESULTS"
LANGS=${*:-en nl es fr de}

# Per language: the labels, the note (the app's own example text) and the setting's name.
labels() {
    case "$1" in
        en) LANGUAGE=English L_WAKE="Wake up" L_WEEKEND="Weekend" L_DENTIST="Dentist" L_GYM="Gym" L_NOTE="Take the laptop charger" ;;
        nl) LANGUAGE=Dutch L_WAKE="Opstaan" L_WEEKEND="Weekend" L_DENTIST="Tandarts" L_GYM="Sportschool" L_NOTE="Neem de laptoplader mee" ;;
        es) LANGUAGE=Spanish L_WAKE="Levantarse" L_WEEKEND="Fin de semana" L_DENTIST="Dentista" L_GYM="Gimnasio" L_NOTE="Llevar el cargador" ;;
        fr) LANGUAGE=French L_WAKE="Lever" L_WEEKEND="Week-end" L_DENTIST="Dentiste" L_GYM="Salle de sport" L_NOTE="Prendre le chargeur" ;;
        de) LANGUAGE=German L_WAKE="Aufstehen" L_WEEKEND="Wochenende" L_DENTIST="Zahnarzt" L_GYM="Fitnessstudio" L_NOTE="Ladegerät mitnehmen" ;;
        *) echo "unknown language $1"; exit 1 ;;
    esac
}

# Dark, blue, 24 hours. Moving backgrounds only for the ring itself.
settings_for() { # less_motion
    use_settings '"Dark"' "\"$LANGUAGE\"" "\"setup_done\":true,\"language_follows_phone\":false,\"less_motion\":$1,\"time_format\":\"H24\",\"accent\":\"Blue\""
}

# A status bar without the emulator's own noise: full battery and signal, no icons, this time.
status_bar() { # hhmm
    dev settings put global sysui_demo_allowed 1 > /dev/null
    for c in "enter" "clock -e hhmm $1" "battery -e level 100 -e plugged false" \
        "network -e wifi show -e level 4 -e fully true" "network -e mobile hide" \
        "notifications -e visible false" "status -e alarm hide -e volume hide -e bluetooth hide -e location hide"; do
        dev "am broadcast -a com.android.systemui.demo -e command $c" > /dev/null
    done
}

# Every language on the same evening: the coming Thursday (from the host's clock, which does not move
# with the emulator's), so "this week" in the history has four mornings in it.
zone_s() {
    local off
    off=$(dev date +%z | tr -d '\r')
    case "$off" in [+-][0-9][0-9][0-9][0-9]) ;; *) off=+0000 ;; esac
    echo $((${off:0:1}(10#${off:1:2} * 3600 + 10#${off:3:2} * 60)))
}
at_time_of_day() { # day (0 = that Thursday) hour minute -> unix s
    local z local_s dow
    z=$(zone_s)
    local_s=$(($(date +%s) + z))
    dow=$(((local_s / 86400 + 4) % 7))
    echo $((local_s - local_s % 86400 + ((4 - dow + 7) % 7 + $1) * 86400 + $2 * 3600 + $3 * 60 - z))
}

challenges='[["challenge.math","{\"difficulty\":\"medium\"}"],["challenge.steps","{\"count\":20}"],["challenge.memory","{\"difficulty\":\"easy\"}"],["challenge.scan","{}"]]'
seed_alarms() {
    local weekdays='[true,true,true,true,true,false,false]' weekend='[false,false,false,false,false,true,true]' later
    later=$(local_date_in 3)
    ALARM_EXTRA=",\"note\":\"$L_NOTE\",\"ring_look\":{\"backdrop\":\"Aurora\"},\"notify_before_minutes\":10"
    seed wake "$(alarm_json wake 6 45 "$weekdays" null air 0 60 3 5 "$challenges")"
    ALARM_EXTRA=',"ring_look":{"backdrop":"Sunrise"}'
    seed weekend "$(alarm_json weekend 9 30 "$weekend" null happy 0 30 2 10 '[["challenge.memory","{\"difficulty\":\"easy\"}"]]')"
    ALARM_EXTRA=
    seed dentist "$(alarm_json dentist 14 15 "$NO_DAYS" "\"$later\"" lofi 0 0 1 5 '[["challenge.hold","{\"count\":1,\"hold_ms\":3000}"]]')"
    seed gym "$(alarm_json gym 18 45 "$EVERY_DAY" null funk 1 0 0 5 '[["challenge.shake","{\"count\":20}"]]')"
    {
        printf "UPDATE alarms SET data = replace(data, '\"enabled\":true', '\"enabled\":false') WHERE id='gym';\n"
        for pair in "wake|$L_WAKE" "weekend|$L_WEEKEND" "dentist|$L_DENTIST" "gym|$L_GYM"; do
            printf "UPDATE alarms SET data = replace(data, '\"label\":\"%s\"', '\"label\":\"%s\"') WHERE id='%s';\n" "${pair%%|*}" "${pair#*|}" "${pair%%|*}"
        done
        printf "UPDATE alarms SET data = replace(data, '\"order\":0', '\"order\":1') WHERE id='weekend';\n"
        printf "UPDATE alarms SET data = replace(data, '\"order\":0', '\"order\":2') WHERE id='dentist';\n"
        printf "UPDATE alarms SET data = replace(data, '\"order\":0', '\"order\":3') WHERE id='gym';\n"
    } | sql
}

# Two weeks of mornings: a streak, a couple of snoozes, one ring that stopped by itself.
seed_history() {
    local now i day snoozes gaps
    now=$(now_s)
    {
        for i in 0 1 2 3 4 5 6 7 8 9 10 11 12 13; do
            [ "$i" = 9 ] && continue
            day=$(date -u -d "@$((now - i * 86400))" +%Y-%m-%dT05:45:00+00:00)
            snoozes=$((i % 4 == 0 ? 1 : 0))
            gaps=$([ "$snoozes" = 1 ] && echo '[5]' || echo '[]')
            printf "INSERT INTO alarm_events (id, alarm_id, alarm_label, fire_timestamp, first_interaction_timestamp, dismiss_timestamp, snooze_count, snooze_intervals, challenge_solves, total_dismissal_time_ms, outcome, seen) VALUES ('e%s', 'wake', '%s', '%s', '%s', '%s', %s, '%s', '[{\"challenge_type\":\"challenge.math\",\"difficulty\":\"medium\",\"time_ms\":%s000},{\"challenge_type\":\"challenge.steps\",\"difficulty\":\"medium\",\"time_ms\":%s000}]', %s, 'solved', 1);\n" \
                "$i" "$L_WAKE" "$day" "$day" "$day" "$snoozes" "$gaps" "$((14 + i % 5 * 3))" "$((25 + i % 3 * 6))" "$(((70 + i % 6 * 22 + snoozes * 300) * 1000))"
        done
        day=$(date -u -d "@$((now - 9 * 86400))" +%Y-%m-%dT05:45:00+00:00)
        printf "INSERT INTO alarm_events (id, alarm_id, alarm_label, fire_timestamp, dismiss_timestamp, total_dismissal_time_ms, outcome, seen) VALUES ('l1', 'wake', '%s', '%s', '%s', 0, 'limit', 1);\n" "$L_WAKE" "$day" "$day"
    } | sql
}

shot() { # name
    # The emulator's own "no screen lock" notice can come in after the status bar was set up.
    dev "am broadcast -a com.android.systemui.demo -e command notifications -e visible false" > /dev/null
    sleep 0.6
    adb_ exec-out screencap -p > "$DIR/$1.png"
    echo "      $1"
}
row_of_editor() { js "document.querySelectorAll('.row-btn')[$1].click(); return 'ok';" > /dev/null; sleep 0.4; }
open_wake() {
    local n
    n=$(js "return [...document.querySelectorAll('.alarm-label')].findIndex(e => e.textContent === '$L_WAKE');")
    click .alarm-main "${n:-0}" 0.4
    on_screen .time-big 5 || echo "      no editor for $L_WAKE"
}

one_language() {
    labels "$1"
    DIR="store/screenshots/$1"
    mkdir -p "$DIR"
    rm -f "$DIR"/*.png
    echo "=== $1 ($LANGUAGE)"

    # The evening before: the list, the editor and what is behind it.
    fresh
    settings_for true
    set_clock "$(at_time_of_day 0 22 30)"
    status_bar 2230
    seed_alarms
    seed_history
    start_app
    on_screen .alarm-card 20 || { echo "      no list"; return 1; }
    sleep 1.5
    shot 01_alarms

    open_wake
    shot 06_editor

    row_of_editor 4
    on_screen .challenge-row 4 && shot 04_challenges
    press_back
    gone .sheet 3 || press_back

    row_of_editor 5
    on_screen .look-grid 4 && shot 05_alarm_screens
    press_back
    gone .sheet 3 || press_back
    press_back
    on_screen '.sheet, .alarm-card' 4
    is_there .sheet && click '.sheet-item.danger' 0 0.3

    click .nav-item 1 0.4
    on_screen .screen-title-large 5 && sleep 0.8 && shot 07_history

    click .nav-item 2 0.4
    if on_screen .row-btn 5; then
        js "document.querySelectorAll('.row-btn')[0].click(); return 'ok';" > /dev/null
        on_screen .theme-tile 5 && sleep 0.5 && shot 08_themes
    fi
    dev am force-stop "$PKG"

    # The alarm itself, at a quarter to seven, moving.
    fresh
    settings_for false
    T=$(at_time_of_day 1 6 45)
    set_clock $((T - 8))
    seed_history
    status_bar 0645
    ALARM_EXTRA=",\"note\":\"$L_NOTE\",\"ring_look\":{\"backdrop\":\"Aurora\"}"
    seed wake "$(alarm_json wake 6 45 "$EVERY_DAY" null air 0 0 3 5 "$challenges")"
    ALARM_EXTRA=
    printf "UPDATE alarms SET data = replace(data, '\"label\":\"wake\"', '\"label\":\"%s\"') WHERE id='wake';\n" "$L_WAKE" | sql
    arm wake
    if await_ring wake 70 && on_screen .ring-off 15; then
        sleep 2
        shot 02_ringing
        click .ring-off 0 0.3
        on_screen .math-expression 5 && sleep 1 && shot 03_challenge_math
    else
        echo "      the alarm did not ring: state=$(state wake)"
    fi
    dev am force-stop "$PKG"
}

START=$SECONDS
reboot_and_wait || { echo "the emulator did not start"; exit 1; }
said=$(adb_ install -r -g "$APK" 2>&1)
case "$said" in *Success*) ;; *) echo "install failed: $said"; exit 1 ;; esac
dev wm size 1080x1920
dev wm density 420
# What the warning's Fix does on a phone: no battery saver in the way, so no warning on the list.
dev dumpsys deviceidle whitelist +"$PKG" > /dev/null
dev appops set "$PKG" AUTO_REVOKE_PERMISSIONS_IF_UNUSED ignore
for l in $LANGS; do one_language "$l"; done
dev "am broadcast -a com.android.systemui.demo -e command exit" > /dev/null
# Back to how the test suite expects this emulator: its reliability scenarios look for these.
dev dumpsys deviceidle whitelist -"$PKG" > /dev/null
dev appops set "$PKG" AUTO_REVOKE_PERMISSIONS_IF_UNUSED default
dev wm size reset
dev wm density reset
emulator_stop
echo "done in $(((SECONDS - START) / 60)) min $(((SECONDS - START) % 60)) s"
