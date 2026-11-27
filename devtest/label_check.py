"""What a screen reader gets on a screen: a `uiautomator dump` (stdin) checked for labels.
Reports a root named app_process64, interactive nodes without a name, and names that
two or more interactive nodes of the screen share. usage: label_check.py SCREEN_NAME
"""
import collections
import os
import sys
import xml.etree.ElementTree as ET

xml = sys.stdin.read()
root = ET.fromstring(xml[xml.find("<hierarchy") : xml.rfind("</hierarchy>") + len("</hierarchy>")])
name = sys.argv[1] if len(sys.argv) > 1 else "screen"
bad = 0
roots = []
interactive = []
for el in root.iter("node"):
    if el.get("package") != os.environ.get("PKG", "com.fidderr.alarmfree"):
        continue
    cls = el.get("class", "").split(".")[-1]
    label = el.get("content-desc") or el.get("text") or ""
    if cls == "View" and el.get("bounds", "").startswith("[0,0]") and label in ("", "AlarmFree", "app_process64"):
        roots.append(label)
    if el.get("clickable") == "true" or el.get("checkable") == "true" or cls in ("Button", "ToggleButton", "Switch", "EditText"):
        interactive.append((cls, label, el.get("bounds")))
# Android names the window from the activity: the root node carries no label of its own, but never app_process64.
if "app_process64" in roots or any("app_process64" in (el.get("text", "") + el.get("content-desc", "")) for el in root.iter("node")):
    print(f"{name}: ROOT is named app_process64")
    bad += 1
for cls, label, bounds in interactive:
    if not label.strip():
        print(f"{name}: UNNAMED {cls} {bounds}")
        bad += 1
count = collections.Counter(label for _, label, _ in interactive if label.strip())
for label, n in count.items():
    if n > 1:
        print(f"{name}: DUPLICATE {label!r} x{n}")
        bad += 1
print(f"{name}: {len(interactive)} interactive nodes, {bad} findings")
