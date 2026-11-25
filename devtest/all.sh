#!/bin/bash
# The whole suite (every scenario) on the release pair, Android 16 and 12, two emulators at the same time, with the APK that is
# already built (APK says which, for a release). min_sdk is 28, so Android 9 is the oldest the app runs on.
#
# Usage: bash devtest/all.sh [name]    results in devtest-out/<name>_<avd>/ and <name>_<avd>.log, a table at the end
#   AVDS="afg16 afg14 afg12" bash devtest/all.sh    other emulators (the matrix is in tiers.sh); LEG=afg12 runs one; LANES=1 one at a time
cd "$(dirname "$0")/.." || exit 1
. devtest/suite.sh
read -ra avds <<< "${LEG:-${AVDS:-afg16 afg12}}"
suite_main "${1:-full}" full "${avds[@]}"
