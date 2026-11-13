#!/bin/bash
# What the Slint window shows right now on SERIAL: the texts with their places, one per line.
#   SERIAL=emulator-5556 bash devtest/dump.sh [commands to run first, e.g. 'tap Settings']
cd "$(dirname "$0")/.." || exit 1
. devtest/lib.sh
open_shells
for c in "$@"; do eval "$c"; sleep 0.6; done
ui texts | python -I -c '
import json, sys
for e in json.loads(sys.stdin.read()):
    print("%-10s on=%-5s x=%4d y=%4d w=%4d  %s" % (e["r"], e["on"], e["x"], e["y"], e["w"], e["t"]))
'
