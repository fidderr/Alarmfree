#!/bin/bash
# The quick tier (the core scenarios of tiers.sh, a few minutes per Android version) on the whole matrix, two emulators at the same time.
#
# Usage: bash devtest/quick.sh [avd...]    default: the matrix of tiers.sh (Android 16 down to 10); WITH_OLD=1 adds Android 9 and 8
#   results in devtest-out/quick_<avd>/ and quick_<avd>.log, a table at the end (NAME=x changes "quick")
cd "$(dirname "$0")/.." || exit 1
. devtest/suite.sh
if [ $# -gt 0 ]; then
    avds=("$@")
else
    read -ra avds <<< "$MATRIX_AVDS${WITH_OLD:+ $MATRIX_AVDS_OFF}"
fi
suite_main "${NAME:-quick}" quick "${avds[@]}"
