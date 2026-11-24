#!/bin/bash
# Makes the AVD of one local name (afg16 ... msd29, tiers.sh) on a Linux runner, set up like the owner's: Google APIs
# x86_64 image, 1080x1920 at 420 dpi, 2 GB RAM, 4 cores, no camera, no GPS, a hardware keyboard.
# Usage: devtest/ci/make_avd.sh <avd>      Needs ANDROID_HOME (sdkmanager and avdmanager in cmdline-tools).
set -euo pipefail
token=$1 avd=${1%%+*}
opts="+${1#*+}+"
export ANDROID_AVD_HOME="${ANDROID_AVD_HOME:-$HOME/.android/avd}"
mkdir -p "$ANDROID_AVD_HOME"
case "$avd" in
    afg17) api=37.2 ;; afg16) api=36 ;; afg15) api=35 ;; afg14) api=34 ;; afg13) api=33 ;; afg12) api=31 ;; msd30) api=30 ;; msd29) api=29 ;;
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
# The owner's afg* AVDs end with this block; msd30 and msd29 are different AVDs (audio in and out on, a back camera, no
# hardware keyboard, GPS, heap 228M; read from their config.ini on 2026-11-22). Android 11 with audio output off left the
# audio policy service unanswering on a runner, so those two are made like the local ones. +noaudio gives the old recipe.
case "$avd" in
    msd30 | msd29)
        audio=yes camera=emulated gps=yes keyboard=no heap=228M
        case "$opts" in *+noaudio+*) audio=no camera=none gps=no keyboard=yes heap=256 ;; esac
        ;;
    *) audio=no camera=none gps=no keyboard=yes heap=256 ;;
esac
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
