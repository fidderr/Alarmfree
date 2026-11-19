#!/bin/bash
# The two tiers of the device suite and what each Android version needs, sourced by suite.sh.
#
#   QUICK  the core, about 4-5 minutes per Android version: an alarm rings with nothing running, after a
#          force-stop and after a restart, snooze and dismiss, a challenge, the screens open and go back,
#          notifications and the overlay denied. For every change, on every version.
#   FULL   every scenario of matrix.sh, before a release, on the release pair.
#
# Every scenario in QUICK is also in FULL (FULL is matrix.sh's own list).

QUICK="s01 s03 s04 s05 s06 s07 s10 s11 s14 s16 s16b s25 s26x2 s28 s33 s42"

# FULL runs what does not depend on Android itself on the oldest of a run only, and what depends on the size on the
# newest only: from Android 16 on an app has no say in how it turns on a big screen. The restarts with a PIN
# (s13*) work the same on all of them and take a minute each.
OLDEST_ONLY="s29 s30 s31 s32 s34 s38 s43 s44 s45 s46 s47 s50 s51 s52"
NEWEST_ONLY="s13 s13b s13d big_screens"
# The AVD that counts as the newest in a FULL run.
NEWEST_AVD=afg16

# The emulators of the quick matrix (Android 16 down to 10) and the ones that are off until phase 2 of the
# older-android work makes the app run there (min_sdk is 29: Android 8 = msd26, Android 9 = msd28 do not start it).
MATRIX_AVDS="afg16 afg15 afg14 afg13 afg12 msd30 msd29"
MATRIX_AVDS_OFF="msd28 msd26"

# The graphics of an emulator: Android 12's on the host's GPU crashed the emulator now and then in the middle of a
# run, and the older images are not made for it.
gpu_for() {
    case "$1" in
        afg16 | afg15 | afg14 | afg13) echo "${GPU_NEW:-host}" ;;
        *) echo swiftshader_indirect ;;
    esac
}

# tier_setup avd tier: LEG_LIST (empty = matrix.sh's own list), LEG_SKIP, LEG_FIRST, LEG_LAST for one emulator.
tier_setup() {
    local avd=$1 tier=$2
    LEG_LIST="" LEG_SKIP="" LEG_FIRST="" LEG_LAST=""
    if [ "$tier" = quick ]; then
        LEG_LIST=$QUICK
        return
    fi
    if [ "$avd" = "$NEWEST_AVD" ]; then
        # The big screens first: next to the other emulator's tour of every panel, late in its run, Android
        # 16 starved to the point of its own system UI freezing. The restarts with a PIN last: the others are
        # done by then.
        LEG_SKIP=$OLDEST_ONLY
        LEG_FIRST=big_screens
        LEG_LAST="s13b s13"
    else
        LEG_SKIP=$NEWEST_ONLY
    fi
}
