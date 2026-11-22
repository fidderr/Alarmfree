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
    afg16) api=36 ;; afg15) api=35 ;; afg14) api=34 ;; afg13) api=33 ;; afg12) api=31 ;; msd30) api=30 ;; msd29) api=29 ;;
    *) echo "unknown AVD $avd"; exit 1 ;;
esac
bin="$ANDROID_HOME/cmdline-tools/latest/bin"
[ -d "$bin" ] || bin=$(dirname "$(ls "$ANDROID_HOME"/cmdline-tools/*/bin/sdkmanager | head -1)")
tag=google_apis
case "$opts" in *+default+*) tag=default ;; esac
image="system-images;android-$api;$tag;x86_64"
cores=4
case "$opts" in *+c2+*) cores=2 ;; *+c3+*) cores=3 ;; esac
yes | "$bin/sdkmanager" --licenses > /dev/null || true
"$bin/sdkmanager" --install "$image" "emulator" "platform-tools" > /dev/null
echo no | "$bin/avdmanager" create avd -n "$avd" -k "$image" -d pixel --force
cfg="$ANDROID_AVD_HOME/$avd.avd/config.ini"
cat >> "$cfg" << 'EOF'
hw.lcd.width=1080
hw.lcd.height=1920
hw.lcd.density=420
hw.ramSize=2048
vm.heapSize=256
hw.audioInput=no
hw.audioOutput=no
hw.camera.back=none
hw.camera.front=none
hw.gps=no
hw.keyboard=yes
hw.gpu.enabled=yes
hw.gpu.mode=swiftshader_indirect
disk.dataPartition.size=6G
EOF
echo "hw.cpu.ncore=$cores" >> "$cfg"
echo "AVD $avd ready (API $api, image $tag, $cores cores)"
