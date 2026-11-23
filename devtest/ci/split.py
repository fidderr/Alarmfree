#!/usr/bin/env python3
"""Two parts per Android version: which units of the full list run in which part (devtest/ci/run.sh).

  split.py PARTS PART                 the units of part PART (1-based) of PARTS, in run order, one line, space separated
  split.py --measure FILE...          durations.txt from results.txt or unit_times.txt files of full runs (mean over the files)
  split.py --check PARTS              the parts' expected seconds and units, for a person to read

A unit is a word of ALL in matrix.sh (one scenario, or a group such as big_screens). The weight of a unit is its expected
seconds from durations.txt (the measured time of what it ran, plus the baseline and the clean start before each result);
a unit that is not in the file gets DEFAULT. Units are dealt out biggest first to the lighter part (longest processing time
first), and each part runs in the order of the full list, big_screens first and the restarts with a PIN last, as before.
"""
import os
import re
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
MATRIX = os.path.join(HERE, "..", "matrix.sh")
DURATIONS = os.path.join(HERE, "durations.txt")
FIRST = ["big_screens"]
LAST = ["s13b", "s13d", "s13"]
DEFAULT = 60
PER_RESULT = 3  # baseline, fresh and start of the app before every result line (49 s over 99 results in run 45252022555)


def all_units():
    for line in open(MATRIX, encoding="utf-8"):
        m = re.match(r'^ALL="([^"]*)"', line)
        if m:
            return m.group(1).split()
    raise SystemExit("split.py: no ALL= line in matrix.sh")


def run_order(units):
    first = [u for u in FIRST if u in units]
    last = [u for u in LAST if u in units]
    return first + [u for u in units if u not in first and u not in last] + last


def read_durations():
    d = {}
    if os.path.exists(DURATIONS):
        for line in open(DURATIONS, encoding="utf-8"):
            w = line.split()
            if len(w) == 2 and not line.startswith("#"):
                d[w[0]] = int(w[1])
    return d


def split(parts, units=None, dur=None):
    units = run_order(units or all_units())
    dur = read_durations() if dur is None else dur
    weight = {u: dur.get(u, DEFAULT) for u in units}
    load = [0] * parts
    got = [[] for _ in range(parts)]
    for u in sorted(units, key=lambda u: (-weight[u], units.index(u))):
        i = min(range(parts), key=lambda i: (load[i], i))
        got[i].append(u)
        load[i] += weight[u]
    return [[u for u in units if u in set(g)] for g in got], load


def unit_of(result_id, order, at):
    """The index of the unit a result belongs to: the unit that is its id (s07d), else the unit it is a lettered part of
    (s07a -> s07, s26_cold_ring_1 -> s26x2), else the unit that is running."""
    rid = result_id.split("@")[0]
    base = re.match(r"s\d+[a-z]*", rid)
    base = base.group(0) if base else rid
    base = {"s12": "s11", "s12b": "s11", "s11a": "s11", "s11b": "s11"}.get(base, base)
    if base in order:
        return order.index(base)
    for k, u in enumerate(order):
        if base.startswith(u) and re.fullmatch(r"[a-z]*", base[len(u):]) or re.sub(r"x\d+$", "", u) == base:
            return k
    return at


def measure(files):
    units = all_units()
    order = run_order(units)
    total = {u: 0.0 for u in units}
    for f in files:
        if os.path.basename(f) == "unit_times.txt":
            # `name seconds` per call of run_unit: the unit is the name (a size's scenarios, name@size, are big_screens).
            for line in open(f, encoding="utf-8", errors="replace"):
                w = line.split()
                if len(w) == 2 and w[1].isdigit():
                    u = "big_screens" if "@" in w[0] else w[0]
                    if u in total:
                        total[u] += int(w[1])
            continue
        at = 0
        for line in open(f, encoding="utf-8", errors="replace"):
            m = re.match(r"^(PASS|FAIL|SKIP|FLAKY|INFRA)  (\S+)\s+\[(\d+)s\]", line)
            if not m:
                continue
            rid, secs = m.group(2), int(m.group(3))
            if "@" in rid:
                u = "big_screens"
            else:
                k = unit_of(rid, order, at)
                u = order[k]
                at = k
            total[u] += secs + PER_RESULT
    n = max(1, len(files))
    for u in order:
        print(u, max(1, round(total[u] / n)))


def main(argv):
    if argv[:1] == ["--measure"]:
        measure(argv[1:])
        return 0
    if argv[:1] == ["--check"]:
        parts, load = split(int(argv[1]))
        for i, g in enumerate(parts):
            print(f"part {i + 1}/{len(parts)}: {load[i]} s expected, {len(g)} units: {' '.join(g)}")
        return 0
    parts, _ = split(int(argv[0]))
    print(" ".join(parts[int(argv[1]) - 1]))
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
