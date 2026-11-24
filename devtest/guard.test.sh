#!/bin/bash
# guard_allows of lib.sh: what may be sent to a real phone while which window has the focus. PASS or FAIL per case.
# Usage: bash devtest/guard.test.sh   (no device)
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
tmp=$(mktemp -d) || exit 1
trap 'rm -rf "$tmp"' EXIT
bad=0
export OUT="$tmp/out" TMPDIR="$tmp"
# shellcheck disable=SC1091
. "$here/lib.sh"
case_() { # name expected(allow|refuse) focus command...
    local name=$1 want=$2 focus=$3 got=refuse
    shift 3
    guard_allows "$focus" "$@" && got=allow
    if [ "$got" = "$want" ]; then echo "PASS  $name"; else echo "FAIL  $name  wanted $want, got $got"; bad=1; fi
}
APP='  mCurrentFocus=Window{1 u0 com.fidderr.alarmfree/android.app.NativeActivity}'
DET='  mCurrentFocus=Window{2 u0 com.android.settings/com.android.settings.applications.InstalledAppDetails}'
LAU='  mCurrentFocus=Window{3 u0 com.android.launcher/com.android.launcher.Launcher}'
SHD='  mCurrentFocus=Window{4 u0 NotificationShade}'
case_ "a tap on the app" allow "$APP" input tap 10 10
case_ "Back on App info of the app" allow "$DET" input keyevent KEYCODE_BACK
case_ "a tap on App info is refused" refuse "$DET" input tap 10 10
case_ "a swipe on App info is refused" refuse "$DET" input swipe 1 2 3 4
case_ "Home on App info is refused" refuse "$DET" input keyevent KEYCODE_HOME
case_ "Back on the launcher is refused" refuse "$LAU" input keyevent KEYCODE_BACK
case_ "Back on the shade is refused" refuse "$SHD" input keyevent KEYCODE_BACK
exit $bad
