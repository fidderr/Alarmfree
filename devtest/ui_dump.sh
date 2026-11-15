#!/bin/bash
# Prints the nodes of the app window from `uiautomator dump`: class text desc bounds flags.
# usage: SERIAL=emulator-5560 devtest/ui_dump.sh
export MSYS_NO_PATHCONV=1
ADB="${ADB:-$LOCALAPPDATA/Android/Sdk/platform-tools/adb.exe}"
SERIAL="${SERIAL:-emulator-5554}"
"$ADB" -s "$SERIAL" shell uiautomator dump /sdcard/ui.xml > /dev/null
"$ADB" -s "$SERIAL" shell cat /sdcard/ui.xml | python -I -c '
import re, sys
s = sys.stdin.read()
for m in re.finditer(r"<node ([^>]*)>", s):
    d = dict(re.findall(r"([\w-]+)=\"([^\"]*)\"", m.group(1)))
    if "com.android.systemui" in m.group(1):
        continue
    flags = [k for k in ("clickable", "checkable", "checked", "focusable", "focused", "password", "scrollable", "selected", "enabled") if d.get(k) == "true"]
    print(d["class"].split(".")[-1], repr(d["text"]), repr(d["content-desc"]), d["bounds"], ",".join(flags))
'
