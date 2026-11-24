#!/bin/bash
# The quick tier (the core scenarios of tiers.sh, a few minutes per Android version) on the whole matrix, two emulators at the same time.
#
# Usage: bash devtest/quick.sh [avd...]    default: the matrix of tiers.sh (Android 17 down to 10)
#   results in devtest-out/quick_<avd>/ and quick_<avd>.log, a table at the end (NAME=x changes "quick")
cd "$(dirname "$0")/.." || exit 1
. devtest/suite.sh
if [ $# -gt 0 ]; then
    avds=("$@")
else
    read -ra avds <<< "$MATRIX_AVDS"
fi
suite_main "${NAME:-quick}" quick "${avds[@]}"
