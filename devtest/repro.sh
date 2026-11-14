#!/bin/bash
# Rebuild a user's situation on an emulator from a bug report, and print what the app did.
#
#   SERIAL=emulator-5558 bash devtest/repro.sh <alarmfree-report.txt> [alarmfree-setup.json]
#
# What it does: installs the debug APK (APK=..., default build/alarmfree-slint-debug.apk), sets
# the emulator's time zone from the report's "Time zone:" line, puts the alarms of the setup file
# in the app (as they were saved, names and notes included), starts the app, and prints the
# timeline: the report's log lines, then the alarms with the next ring AlarmKit has for each.
# Without a setup file the alarms are rebuilt from the report's "== Alarms" lines (time, repeat,
# on or off). Read docs/BUG_REPORTS.md for how to read the rest. Emulator only.

cd "$(dirname "$0")/.." || exit 1
report=${1:?usage: repro.sh <alarmfree-report.txt> [alarmfree-setup.json]}
setup=${2:-}
[ -f "$report" ] || { echo "no such report: $report"; exit 1; }
export SERIAL="${SERIAL:-emulator-5558}"
case "$SERIAL" in emulator-*) ;; *) echo "emulators only: SERIAL=$SERIAL"; exit 1 ;; esac
export OUT="${OUT:-devtest-out/repro}"
APK="${APK:-build/alarmfree-slint-debug.apk}"
. devtest/lib.sh
[ -f "$APK" ] || { echo "no APK at $APK (TARGET=x86_64-linux-android ./build.sh)"; exit 1; }
device_ok || { echo "$SERIAL does not answer"; exit 1; }

zone=$(sed -nE 's/^Time zone: ([A-Za-z_\/+-]+).*/\1/p' "$report" | head -1)
echo "report: $(sed -nE 's/^App: (.*)/\1/p' "$report" | head -1), time zone: ${zone:-unknown}"

said=$(adb_ install -r -g "$APK" 2>&1)
case "$said" in *Success*) ;; *) echo "install failed: $said"; exit 1 ;; esac
claim_device
open_shells
dev settings put global auto_time 0
dev settings put global auto_time_zone 0
if [ -n "$zone" ] && [ "$zone" != "?" ]; then
    dev "service call alarm 3 s16 $zone" > /dev/null
    echo "time zone now: $(dev date +%Z%z)"
fi
dev am force-stop "$PKG"
dev pm clear "$PKG" > /dev/null
dev pm grant "$PKG" android.permission.POST_NOTIFICATIONS > /dev/null 2>&1
start_app
wait_for 40 page_is_up
wait_file files/alarmfree.db 60
dev am force-stop "$PKG"
use_settings '"Dark"' '"English"' '"setup_done":true,"language_follows_phone":false'

if [ -n "$setup" ]; then
    python - "$setup" <<'PY' > "$OUT/seed.sql"
import json, sys, uuid
for a in json.load(open(sys.argv[1], encoding="utf-8")):
    a["id"] = a.get("id") or str(uuid.uuid4())
    data = json.dumps(a, ensure_ascii=False).replace("'", "''")
    print(f"INSERT OR REPLACE INTO alarms (id, data) VALUES ('{a['id']}', '{data}');")
PY
else
    python - "$report" <<'PY' > "$OUT/seed.sql"
import json, re, sys, uuid
days = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"]
for line in open(sys.argv[1], encoding="utf-8"):
    m = re.match(r"id=(\S+) (\d\d):(\d\d) (once \S+|[A-Za-z,]+) (on|off)", line)
    if not m:
        continue
    repeat = m.group(4)
    a = {"id": str(uuid.uuid4()), "time_hour": int(m.group(2)), "time_minute": int(m.group(3)),
         "label": m.group(1), "days": [d in repeat.split(",") for d in days],
         "date": repeat[5:] if repeat.startswith("once ") and repeat[5:] != "undated" else None,
         "skip_until_ms": None, "fade_in_secs": 0, "ring_limit_secs": 1800, "order": 0,
         "enabled": m.group(5) == "on", "sound": "air", "sound_mode": 0, "challenges": [],
         "snooze_count": 3, "snooze_interval": 5}
    print("INSERT OR REPLACE INTO alarms (id, data) VALUES ('%s', '%s');" % (a["id"], json.dumps(a).replace("'", "''")))
PY
fi
echo "alarms put in the app: $(grep -c INSERT "$OUT/seed.sql")"
sql < "$OUT/seed.sql"
start_app
wait_for 40 page_is_up
sleep 4

echo
echo "===== the report's log ====="
sed -n '/^== Log ==/,$p' "$report" | tail -n +2
echo
echo "===== what the app did here ====="
as_app "cat files/sentinel/diag.log" | cut -c1-240
echo
echo "===== the alarms, as the report lists them ====="
sed -n '/^== Alarms/,/^== Alarm setup/p' "$report" | sed '$d'
