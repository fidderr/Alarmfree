#!/bin/bash
# The design walk: every panel and choice of the app, a picture of each, for a person to look over after the screens changed.
# Not a test: it is in neither tier (s57 in FULL is the short check that each panel opens on a real screen).
#
# Usage: bash devtest/walk.sh [avd]    (default afg16; results in devtest-out/walk_<avd>/ and walk_<avd>.log)
cd "$(dirname "$0")/.." || exit 1
. devtest/suite.sh
export S57_FULL=1
avd="${1:-afg16}"
check_apk || exit 1
run_leg walk walk "$avd" "${PORT_BASE}"
