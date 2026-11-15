"""Lists what an accessibility service cannot name on the screen in a `uiautomator dump` (stdin).

A node is reported when it can take focus or a click and has no text and no description, or when it
has a name but a plain View class (no role). Layout containers that take no focus are fine.
usage: a11y_audit.py SCREEN_NAME
"""
import re
import sys
import xml.etree.ElementTree as ET

xml = sys.stdin.read()
start = xml.find("<hierarchy")
end = xml.rfind("</hierarchy>")
root = ET.fromstring(xml[start : end + len("</hierarchy>")])
name = sys.argv[1] if len(sys.argv) > 1 else "screen"
for el in root.iter("node"):
    if el.get("package") == "com.android.systemui":
        continue
    cls = el.get("class", "").split(".")[-1]
    label = el.get("text") or el.get("content-desc") or ""
    if label == "app_process64":
        continue
    focusable = el.get("focusable") == "true" or el.get("clickable") == "true"
    b = el.get("bounds")
    if focusable and not label:
        print(f"{name}: UNNAMED {cls} {b}")
    elif focusable and cls == "View" and el.get("checkable") != "true":
        print(f"{name}: NO ROLE {cls} {label!r} {b}")
