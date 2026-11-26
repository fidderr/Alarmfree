#!/bin/bash
# Makes the AVD of one local name (afg16 ... msd28, tiers.sh) on a Linux runner, set up like the owner's: Google APIs
# x86_64 image, 1080x1920 at 420 dpi, 2 GB RAM, 4 cores, no camera, no GPS, a hardware keyboard.
# Usage: devtest/ci/make_avd.sh <avd>      Needs ANDROID_HOME (sdkmanager and avdmanager in cmdline-tools).
set -euo pipefail
token=$1 avd=${1%%+*}
opts="+${1#*+}+"
export ANDROID_AVD_HOME="${ANDROID_AVD_HOME:-$HOME/.android/avd}"
mkdir -p "$ANDROID_AVD_HOME"
case "$avd" in
    afg17) api=37.2 ;; afg16) api=36 ;; afg15) api=35 ;; afg14) api=34 ;; afg13) api=33 ;; afg12) api=31 ;; msd30) api=30 ;; msd29) api=29 ;; msd28) api=28 ;;
    *) echo "unknown AVD $avd"; exit 1 ;;
esac
bin="$ANDROID_HOME/cmdline-tools/latest/bin"
[ -d "$bin" ] || bin=$(dirname "$(ls "$ANDROID_HOME"/cmdline-tools/*/bin/sdkmanager | head -1)")
tag=google_apis
case "$opts" in *+default+*) tag=default ;; esac
# Android 17's images are the 16 KB page size ones.
[ "$avd" != afg17 ] || tag=google_apis_ps16k
image="system-images;android-$api;$tag;x86_64"
cores=4
case "$opts" in *+c2+*) cores=2 ;; *+c3+*) cores=3 ;; esac
yes | "$bin/sdkmanager" --licenses > /dev/null || true
"$bin/sdkmanager" --install "$image" "emulator" "platform-tools" > /dev/null
echo no | "$bin/avdmanager" create avd -n "$avd" -k "$image" -d pixel --force
cfg="$ANDROID_AVD_HOME/$avd.avd/config.ini"
# avdmanager of cmdline-tools 12.0 writes target=android-0 for an Android 17 image, and Android 17 then aborts in the graphics
# mapper (avd_target.sh). The other versions are left as avdmanager made them.
. "$(dirname "$0")/avd_target.sh"
if [ "$avd" = afg17 ]; then
    avd_fix_target "$ANDROID_AVD_HOME/$avd.ini" "$api" || { echo "AVD $avd has no target=android-$api:"; cat "$ANDROID_AVD_HOME/$avd.ini"; exit 1; }
    # What the run was proven with (emulator 37.3.10, image 37.2 r7): a change is said in the log, not made silently.
    emu_rev=$(sed -n 's/^Pkg.Revision=\([0-9.]*\).*/\1/p' "$ANDROID_HOME/emulator/source.properties")
    img_rev=$(sed -n 's/^Pkg.Revision=\([0-9.]*\).*/\1/p' "$ANDROID_HOME/system-images/android-$api/$tag/x86_64/source.properties")
    echo "afg17: emulator $emu_rev, image android-$api $tag r$img_rev, target $(sed -n 's/^target=//p' "$ANDROID_AVD_HOME/$avd.ini")"
    [ "$emu_rev" = 37.3.10 ] || echo "::warning::afg17 was proven with emulator 37.3.10, this runner has $emu_rev"
    [ "$img_rev" = 7 ] || echo "::warning::afg17 was proven with image android-$api $tag r7, this runner has r$img_rev"
fi
# The owner's afg* AVDs end with this block; msd30, msd29 and msd28 are different AVDs (audio in and out on, a back camera, no
# hardware keyboard, GPS, heap 228M; read from their config.ini on 2026-11-22). Android 11 with audio output off left the
# audio policy service unanswering on a runner, so those three are made like the local ones. +noaudio gives the old recipe.
case "$avd" in
    msd30 | msd29 | msd28)
        audio=yes camera=emulated gps=yes keyboard=no heap=228M
        case "$opts" in *+noaudio+*) audio=no camera=none gps=no keyboard=yes heap=256 ;; esac
        # The emulated camera (and the virtual scene) of the Android 9 image stalls the whole emulator process, with every thread
        # idle and adb and the console silent, as soon as an app streams preview frames to its callback, which the scanner's
        # camera does (msd28, 2026-11-25; the system camera app, which has no preview callback, is fine, and without a camera the
        # scanner screen opens like on the afg* AVDs).
        [ "$avd" != msd28 ] || camera=none
        ;;
    *) audio=no camera=none gps=no keyboard=yes heap=256 ;;
esac
# afg12 as well, with the audio device on: with the output off the goldfish audio HAL (audio@6.0-impl.ranchu) blocks in pcm_write
# for about 4 s and then fails with an I/O error. AudioService's screen-on receiver waits behind it on the system_server main
# thread, and so does the player setup of the first ring after the screen came on: the sound of s26 came 3.9 to 4.5 s (once 7.2 s)
# after delivery in 5 of 24 cold rings, and audioserver aborted in TimeCheckThread when the block outlasted its 5 s (run 45885511953,
# afg12 with the output off against afg12 with it on: 5 asks and 4 audioserver restarts against none in 24 cold rings each).
[ "$avd" != afg12 ] || audio=yes
case "$opts" in *+noaudio+*) audio=no ;; esac
cat >> "$cfg" << EOF
hw.lcd.width=1080
hw.lcd.height=1920
hw.lcd.density=420
hw.ramSize=2048
vm.heapSize=$heap
hw.audioInput=$audio
hw.audioOutput=$audio
hw.camera.back=$camera
hw.camera.front=none
hw.gps=$gps
hw.keyboard=$keyboard
hw.gpu.enabled=yes
hw.gpu.mode=swiftshader_indirect
disk.dataPartition.size=6G
EOF
echo "hw.cpu.ncore=$cores" >> "$cfg"
echo "AVD $avd ready (API $api, image $tag, $cores cores)"
