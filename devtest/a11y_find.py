"""Reads a `uiautomator dump` on stdin and answers questions about the accessibility tree.

usage: a11y_find.py MODE WIDTH HEIGHT [TEXT [WHICH]]

MODE
  find     "x y" of the middle of the WHICH-th (0, or L for the last) node whose label is TEXT, or none
  findc    the same for a label that has TEXT in it
  texts    every label on screen, one per line, top to bottom
  fields   one line per text field, top to bottom: "x y text"
  checked  true / false / none: the checked state of the node whose label is TEXT
  ready    ok when the tree holds more than the placeholder of an adapter that was just activated
  scroller "x y w h" of the biggest scrollable node, or none

A label is the text or the content description of a node. A node that sits inside another node
with the same label (a button and the text on it) counts once, as the outer one. Only nodes whose
middle is inside the screen count.
"""
import re
import sys
import xml.etree.ElementTree as ET


def parse(xml):
    start = xml.find("<?xml")
    if start < 0:
        start = xml.find("<hierarchy")
    if start < 0:
        return []
    xml = xml[start:]
    end = xml.rfind("</hierarchy>")
    if end >= 0:
        xml = xml[: end + len("</hierarchy>")]
    try:
        root = ET.fromstring(xml)
    except ET.ParseError:
        return []
    nodes = []
    for el in root.iter("node"):
        if el.get("package") == "com.android.systemui":
            continue
        m = re.match(r"\[(-?\d+),(-?\d+)\]\[(-?\d+),(-?\d+)\]", el.get("bounds", ""))
        if not m:
            continue
        x1, y1, x2, y2 = map(int, m.groups())
        label = el.get("text") or el.get("content-desc") or ""
        if label == "app_process64":
            label = ""
        nodes.append(
            dict(
                label=label,
                text=el.get("text") or "",
                desc=el.get("content-desc") or "",
                cls=el.get("class", "").split(".")[-1],
                b=(x1, y1, x2, y2),
                checked=el.get("checked") == "true",
                checkable=el.get("checkable") == "true",
                scrollable=el.get("scrollable") == "true",
                password=el.get("password") == "true",
            )
        )
    return nodes


def inside(a, b):
    return b[0] <= a[0] and b[1] <= a[1] and b[2] >= a[2] and b[3] >= a[3]


def visible(n, w, h):
    x1, y1, x2, y2 = n["b"]
    cx, cy = (x1 + x2) // 2, (y1 + y2) // 2
    return x2 > x1 and y2 > y1 and 0 <= cx < w and 0 <= cy < h


def labelled(nodes, w, h):
    """Labelled nodes on screen, top to bottom, with a duplicate inside its outer node dropped."""
    out = [n for n in nodes if n["label"] and visible(n, w, h)]
    keep = []
    for n in out:
        if any(o is not n and o["label"] == n["label"] and inside(n["b"], o["b"]) and o["b"] != n["b"] for o in out):
            continue
        keep.append(n)
    keep.sort(key=lambda n: (n["b"][1], n["b"][0]))
    return keep


def main():
    mode, w, h = sys.argv[1], int(sys.argv[2]), int(sys.argv[3])
    text = sys.argv[4] if len(sys.argv) > 4 else ""
    which = sys.argv[5] if len(sys.argv) > 5 else ""
    nodes = parse(sys.stdin.read())
    if mode == "ready":
        print("ok" if sum(1 for n in nodes if n["label"]) >= 1 else "no")
        return
    if mode == "scroller":
        s = [n for n in nodes if n["scrollable"]]
        if not s:
            print("none")
            return
        n = max(s, key=lambda n: (n["b"][2] - n["b"][0]) * (n["b"][3] - n["b"][1]))
        x1, y1, x2, y2 = n["b"]
        print(x1, y1, x2 - x1, y2 - y1)
        return
    lab = labelled(nodes, w, h)
    if mode in ("find", "findc"):
        hits = [n for n in lab if (n["label"] == text if mode == "find" else text in n["label"])]
        if not hits:
            print("none")
            return
        if which == "L":
            n = hits[-1]
        else:
            i = int(which) if which.isdigit() else 0
            if i >= len(hits):
                print("none")
                return
            n = hits[i]
        x1, y1, x2, y2 = n["b"]
        print((x1 + x2) // 2, (y1 + y2) // 2)
    elif mode == "texts":
        for n in lab:
            print(n["label"])
    elif mode == "fields":
        fields = [n for n in nodes if n["cls"] == "EditText" and visible(n, w, h)]
        fields.sort(key=lambda n: (n["b"][1], n["b"][0]))
        for n in fields:
            x1, y1, x2, y2 = n["b"]
            print((x1 + x2) // 2, (y1 + y2) // 2, n["text"])
    elif mode == "checked":
        hits = [n for n in lab if n["label"] == text and n["checkable"]]
        print("none" if not hits else ("true" if hits[0]["checked"] else "false"))


main()
