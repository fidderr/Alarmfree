#!/bin/bash
# The target line of an AVD's .ini, sourced by make_avd.sh and tested by devtest/split.test.sh.
# avdmanager of cmdline-tools 12.0 (what a runner has) writes target=android-0 for an Android 17 image. The emulator then does
# not know the API level: it keeps the RAM of the AVD instead of the 4 GB it forces from Android 17 on, and the guest's
# surfaceflinger and system_server abort in mapper.ranchu.so (hasReadColorBufferDma) on every screencap, rotation and gesture
# handle. With the right target none of that happens (2026-11-24, emulators 37.3.10 and 37.4.2, image 37.2 r7).
# avd_fix_target <avd .ini> <api>: android-0 becomes android-<api>; succeeds when the file ends up with target=android-<api>.
avd_fix_target() {
    sed -i "s/^target=android-0\r\?$/target=android-$2/" "$1"
    grep -q "^target=android-$2\r\?$" "$1"
}
