"""Reads a `uiautomator dump` on stdin and answers questions about Android's own screens (the lock screen, the
shade, a file picker, a launcher dialog). The in-app tree is a11y_find.py's business; this one keeps every package.

usage: sys_nodes.py MODE [FILTER...]

MODE
  list              one line per node: package, class, resource id, text, description, flags, bounds
  find FILTER...    "x y" of the middle of the first node that matches every FILTER (a clickable one first), or none
  count FILTER...   how many nodes match
  text FILTER...    the text (or the description) of the first node that matches, or none
  len FILTER...     the number of characters in the text of the first node that matches (a PIN field shows one bullet
                    per digit), or -1 when no node matches
  box FILTER...     "x1 y1 x2 y2" of the first node that matches, or none
  focusbox          "x1 y1 x2 y2" of the first focused node that is smaller than 700 px high (a row or a card), or none
  focused           the bounds line of the focused nodes
  ok                exit 0 when the input holds a hierarchy with at least one node

FILTER  key=value (equal), key~value (contains) or key^regex (a regular expression, case ignored), key is one of: pkg cls rid text desc label clickable focused
        enabled checked password. `label` is the text, or the description when the text is empty. A value that
        starts with ! is negated: text!~Wrong.
"""
import re
import sys
import xml.etree.ElementTree as ET


def parse(xml):
    start = xml.find("<hierarchy")
    if start < 0:
        return None
    xml = xml[start:]
    end = xml.rfind("</hierarchy>")
    if end >= 0:
        xml = xml[: end + len("</hierarchy>")]
    try:
        root = ET.fromstring(xml)
    except ET.ParseError:
        return None
    nodes = []
    for el in root.iter("node"):
        m = re.match(r"\[(-?\d+),(-?\d+)\]\[(-?\d+),(-?\d+)\]", el.get("bounds", ""))
        box = tuple(int(v) for v in m.groups()) if m else (0, 0, 0, 0)
        text = el.get("text", "")
        desc = el.get("content-desc", "")
        nodes.append(
            {
                "pkg": el.get("package", ""),
                "cls": el.get("class", ""),
                "rid": el.get("resource-id", ""),
                "text": text,
                "desc": desc,
                "label": text or desc,
                "clickable": el.get("clickable", ""),
                "focused": el.get("focused", ""),
                "enabled": el.get("enabled", ""),
                "checked": el.get("checked", ""),
                "password": el.get("password", ""),
                "box": box,
            }
        )
    return nodes


def parse_filter(arg):
    m = re.match(r"([a-z]+)(!?)([=~^])(.*)$", arg, re.S)
    if not m:
        raise SystemExit("bad filter: " + arg)
    key, neg, op, value = m.groups()
    return key, bool(neg), op, value


def matches(node, filters):
    for key, neg, op, value in filters:
        have = node.get(key, "")
        if op == "=":
            hit = have == value
        elif op == "~":
            hit = value in have
        else:
            hit = re.search(value, have, re.I) is not None
        if hit == neg:
            return False
    return True


def centre(node):
    x1, y1, x2, y2 = node["box"]
    return "%d %d" % ((x1 + x2) // 2, (y1 + y2) // 2)


def main():
    if len(sys.argv) < 2:
        raise SystemExit(__doc__)
    mode = sys.argv[1]
    sys.stdin.reconfigure(encoding="utf-8", errors="replace")
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")
    nodes = parse(sys.stdin.read())
    if mode == "ok":
        sys.exit(0 if nodes else 1)
    if nodes is None:
        print("none" if mode in ("find", "text") else "0" if mode == "count" else "")
        sys.exit(1)
    filters = [parse_filter(a) for a in sys.argv[2:]]
    if mode == "list":
        for n in nodes:
            flags = "".join(
                f if n[k] == "true" else "-"
                for f, k in (("C", "clickable"), ("F", "focused"), ("E", "enabled"), ("K", "checked"), ("P", "password"))
            )
            print(
                "%s|%s|%s|%s|%s|%s|%s"
                % (
                    n["pkg"],
                    n["cls"].rsplit(".", 1)[-1],
                    n["rid"].split("/")[-1],
                    n["text"][:40].replace("\n", " "),
                    n["desc"][:40].replace("\n", " "),
                    flags,
                    "[%d,%d][%d,%d]" % n["box"],
                )
            )
    elif mode == "find":
        hits = [n for n in nodes if matches(n, filters) and n["box"][2] > n["box"][0] and n["box"][3] > n["box"][1]]
        hits.sort(key=lambda n: n["clickable"] != "true")
        print(centre(hits[0]) if hits else "none")
        sys.exit(0 if hits else 1)
    elif mode == "count":
        print(sum(1 for n in nodes if matches(n, filters)))
    elif mode == "text":
        hits = [n for n in nodes if matches(n, filters)]
        print(hits[0]["label"] if hits else "none")
        sys.exit(0 if hits else 1)
    elif mode == "box":
        hits = [n for n in nodes if matches(n, filters)]
        print("%d %d %d %d" % hits[0]["box"] if hits else "none")
        sys.exit(0 if hits else 1)
    elif mode == "focusbox":
        hits = [n for n in nodes if n["focused"] == "true" and n["box"][3] - n["box"][1] < 700]
        print("%d %d %d %d" % hits[0]["box"] if hits else "none")
        sys.exit(0 if hits else 1)
    elif mode == "len":
        hits = [n for n in nodes if matches(n, filters)]
        print(len(hits[0]["text"]) if hits else -1)
        sys.exit(0 if hits else 1)
    elif mode == "focused":
        for n in nodes:
            if n["focused"] == "true":
                print("%s %s [%d,%d][%d,%d]" % (n["pkg"], n["rid"], *n["box"]))
    else:
        raise SystemExit("unknown mode " + mode)


if __name__ == "__main__":
    main()
