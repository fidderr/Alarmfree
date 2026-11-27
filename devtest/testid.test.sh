#!/bin/bash
# A run of the suite on the test id (APP_ID_SUFFIX=.dev, build.sh) beside the Play app: nothing it sends may name the real app,
# and a phone run does not start on the real id when that is the Play install. No device: a stub stands for adb and logs
# every call. PASS or FAIL per case. Usage: bash devtest/testid.test.sh
here=$(cd "${BASH_SOURCE[0]%/*}" && pwd)
tmp=$(mktemp -d) || exit 1
trap 'rm -rf "$tmp"' EXIT
bad=0
REAL=com.fidderr.alarmfree

verdict() { # name ok
    if [ "$2" = 1 ]; then echo "PASS  $1"; else echo "FAIL  $1"; bad=1; fi
}

# The stub: logs its arguments; answers the package list with what INSTALLER says for the real app (and the test app, no installer).
cat > "$tmp/fakeadb" << 'S'
#!/bin/bash
echo "$*" >> "$FAKE_LOG"
case "$*" in
    *"pm list packages -i"*)
        [ -z "$INSTALLER" ] || echo "package:com.fidderr.alarmfree  installer=$INSTALLER"
        echo "package:com.fidderr.alarmfree.dev  installer=null"
        ;;
    *"echo ok"*) echo ok ;;
esac
exit 0
S
chmod +x "$tmp/fakeadb"

# run_lib pkg installer command...: lib.sh sourced with this PKG and the stub; prints what the command printed, and its exit code last.
run_lib() {
    local pkg=$1 installer=$2
    shift 2
    : > "$tmp/adb.log"
    (
        export OUT="$tmp/out" TMPDIR="$tmp" ADB="$tmp/fakeadb" SERIAL=emulator-5554 FAKE_LOG="$tmp/adb.log" INSTALLER="$installer"
        [ -z "$pkg" ] || export PKG="$pkg"
        . "$here/lib.sh"
        "$@"
        echo "rc=$?"
    ) 2>&1
}

# The id the suite runs on, and what is derived from it.
show_ids() { echo "$PKG $FILE_PREFIX"; }
out=$(run_lib "" "" show_ids)
[[ $out == "$REAL alarmfree"* ]] && verdict "without PKG the suite runs on the real id, its files are alarmfree-*" 1 || verdict "without PKG the suite runs on the real id, its files are alarmfree-*: $out" 0
out=$(run_lib "$REAL.dev" "" show_ids)
[[ $out == "$REAL.dev alarmfree.dev"* ]] && verdict "PKG from the environment is kept, its files are alarmfree.dev-*" 1 || verdict "PKG from the environment is kept, its files are alarmfree.dev-*: $out" 0
[ "alarmfree-rings.txt" != "alarmfree.dev-rings.txt" ] && [[ alarmfree.dev-rings.txt != alarmfree-* ]] && verdict "the glob of the real app's files does not match the test app's files" 1 || verdict "the glob of the real app's files does not match the test app's files" 0

# real_app_named: only the real id as a whole word, only while the run is on another id.
named() { # pkg text
    run_lib "$1" "" real_app_named "$2" | grep -q 'rc=0'
}
named "$REAL.dev" "am force-stop $REAL" && verdict "the real id in a command is named (test id)" 1 || verdict "the real id in a command is named (test id)" 0
named "$REAL.dev" "pm clear $REAL" && verdict "the real id at the end of the command is named" 1 || verdict "the real id at the end of the command is named" 0
named "$REAL.dev" "run-as $REAL sh -c 'cd /data/user_de/0/$REAL/files'" && verdict "the real id inside a path is named" 1 || verdict "the real id inside a path is named" 0
named "$REAL.dev" "am start -n $REAL/android.app.NativeActivity" && verdict "the real id before a component is named" 1 || verdict "the real id before a component is named" 0
named "$REAL.dev" "am force-stop $REAL.dev" && verdict "the test id is not the real id" 0 || verdict "the test id is not the real id" 1
named "$REAL.dev" "pidof $REAL.dev:sentinel" && verdict "the test id's sentinel process is not the real id" 0 || verdict "the test id's sentinel process is not the real id" 1
named "$REAL" "am force-stop $REAL" && verdict "on the real id nothing is refused (the default suite is unchanged)" 0 || verdict "on the real id nothing is refused (the default suite is unchanged)" 1

# dev and adb_ refuse before anything reaches the device.
out=$(run_lib "$REAL.dev" "" dev am force-stop "$REAL")
[[ $out == *"STOP  refused"* && $out == *"rc=126"* && ! -s $tmp/adb.log ]] && verdict "dev refuses a command that names the real app, nothing reaches adb" 1 || verdict "dev refuses a command that names the real app, nothing reaches adb: $out / $(cat "$tmp/adb.log")" 0
out=$(run_lib "$REAL.dev" "" adb_ shell pm clear "$REAL")
[[ $out == *"rc=126"* && ! -s $tmp/adb.log ]] && verdict "adb_ refuses a command that names the real app, nothing reaches adb" 1 || verdict "adb_ refuses a command that names the real app, nothing reaches adb: $out" 0
out=$(run_lib "$REAL.dev" "" adb_ shell pm clear "$REAL.dev")
[[ $out == *"rc=0"* && $(cat "$tmp/adb.log") == *"pm clear $REAL.dev"* ]] && verdict "adb_ sends the same command for the test id" 1 || verdict "adb_ sends the same command for the test id: $out" 0
out=$(run_lib "" "" adb_ shell pm clear "$REAL")
[[ $out == *"rc=0"* && $(cat "$tmp/adb.log") == *"pm clear $REAL"* ]] && verdict "adb_ on the real id sends it (the default suite is unchanged)" 1 || verdict "adb_ on the real id sends it (the default suite is unchanged): $out" 0

# What the input guard of a phone lets through: the focus has to be the app of this run.
allows() { # pkg focus
    run_lib "$1" "" guard_allows "$2" input tap 1 1 | grep -q 'rc=0'
}
allows "$REAL.dev" "  mCurrentFocus=Window{1 u0 $REAL.dev/android.app.NativeActivity}" && verdict "a tap goes to the test app's window" 1 || verdict "a tap goes to the test app's window" 0
allows "$REAL.dev" "  mCurrentFocus=Window{1 u0 $REAL/android.app.NativeActivity}" && verdict "no tap goes to the real app's window (test id)" 0 || verdict "no tap goes to the real app's window (test id)" 1
allows "$REAL" "  mCurrentFocus=Window{1 u0 $REAL.dev/android.app.NativeActivity}" && verdict "no tap goes to the test app's window on the real id" 0 || verdict "no tap goes to the test app's window on the real id" 1
allows "$REAL" "  mCurrentFocus=Window{1 u0 $REAL/android.app.NativeActivity}" && verdict "a tap goes to the real app's window on the real id (unchanged)" 1 || verdict "a tap goes to the real app's window on the real id (unchanged)" 0

# The Play install: refused on the real id only, and only when it comes from Google Play.
out=$(run_lib "" com.android.vending refuse_play_install)
[[ $out == *"STOP  $REAL on this device comes from Google Play"* && $out == *"rc=1"* ]] && verdict "the real id installed by Play is refused" 1 || verdict "the real id installed by Play is refused: $out" 0
out=$(run_lib "" null refuse_play_install)
[[ $out == *"rc=0"* ]] && verdict "the real id installed by adb is not refused" 1 || verdict "the real id installed by adb is not refused: $out" 0
out=$(run_lib "" "" refuse_play_install)
[[ $out == *"rc=0"* ]] && verdict "the real id that is not installed is not refused" 1 || verdict "the real id that is not installed is not refused: $out" 0
out=$(run_lib "$REAL.dev" com.android.vending refuse_play_install)
[[ $out == *"rc=0"* ]] && verdict "the test id is never refused for the Play install of the real app" 1 || verdict "the test id is never refused for the Play install of the real app: $out" 0
out=$(run_lib "" com.android.vending installer_of "$REAL.dev")
[[ $out == *"null"* && $out != *"vending"* ]] && verdict "installer_of looks at the exact package, not a prefix" 1 || verdict "installer_of looks at the exact package, not a prefix: $out" 0

# phone.sh itself: on the real id with the Play install it stops before it changes anything on the phone.
mkdir -p "$tmp/repo/devtest"
cp "$here"/*.sh "$tmp/repo/devtest/"
: > "$tmp/adb.log"
out=$(cd "$tmp/repo" && PKG=$REAL ADB="$tmp/fakeadb" FAKE_LOG="$tmp/adb.log" INSTALLER=com.android.vending TMPDIR="$tmp" APK=x.apk SERIAL=FAKE PHONE_SQLITE=none bash devtest/phone.sh refuse 2>&1)
log=$(cat "$tmp/adb.log")
[[ $out == *"STOP  $REAL on this device comes from Google Play"* ]] && verdict "phone.sh stops on the real id installed by Play, with the message" 1 || verdict "phone.sh stops on the real id installed by Play, with the message: $out" 0
[[ $log != *" install "* && $log != *"pm clear"* && $log != *"force-stop"* && $log != *"settings put"* && $log != *"input "* ]] && verdict "phone.sh changed nothing on the phone before it stopped" 1 || verdict "phone.sh changed nothing on the phone before it stopped: $log" 0
: > "$tmp/adb.log"
out=$(cd "$tmp/repo" && ADB="$tmp/fakeadb" FAKE_LOG="$tmp/adb.log" INSTALLER=com.android.vending TMPDIR="$tmp" APK="$tmp/not-an-apk.apk" SERIAL=FAKE PHONE_SQLITE=none bash devtest/phone.sh refuse2 2>&1)
log=$(cat "$tmp/adb.log")
[[ $out == *"is not the app com.fidderr.alarmfree.dev"* ]] && verdict "phone.sh runs on the .dev id by default and stops on an APK that is not that app" 1 || verdict "phone.sh runs on the .dev id by default and stops on an APK that is not that app: $out" 0
[[ $log != *" install "* && $log != *"pm clear"* && $log != *"force-stop"* && $log != *"settings put"* ]] && verdict "phone.sh did not install or clear before it stopped on the APK" 1 || verdict "phone.sh did not install or clear before it stopped on the APK: $log" 0
[[ $log != *"$REAL "* && $log != *"$REAL/"* && $log != *"$REAL" ]] && verdict "no call of phone.sh named the real id" 1 || verdict "no call of phone.sh named the real id: $log" 0

exit $bad
