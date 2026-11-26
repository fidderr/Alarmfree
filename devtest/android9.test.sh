#!/bin/bash
# The script changes for Android 9 (and what they must not change on the other versions), without a device: the read of the
# focused window with noise from dumpsys, the lines that count as a crash in the scanner scenario, which permissions are granted
# where, the Doze that is only asked for again on Android 9, a locked restart whose encryption cannot be read, and the API level
# that is read once. PASS or FAIL per case. Usage: bash devtest/android9.test.sh
here=$(cd "${BASH_SOURCE[0]%/*}" && pwd)
tmp=$(mktemp -d) || exit 1
trap 'rm -rf "$tmp"' EXIT
bad=0
verdict() { # name ok detail
    if [ "$2" = 1 ]; then echo "PASS  $1"; else echo "FAIL  $1  $3"; bad=1; fi
}
export OUT="$tmp/out" TMPDIR="$tmp"
# shellcheck disable=SC1091
. "$here/lib.sh"

# ---- focused_window: the device shell joins stderr into the answer, dumpsys writes noise to it ----
mkdir -p "$tmp/bin"
cat > "$tmp/bin/dumpsys" << 'EOF'
#!/bin/bash
# displays: the focus line (Android 10 and up); the whole dump: older lines of an ANR report first, the focus now last.
echo "Failed to write while dumping service" >&2
case "$FAKE_API" in
    28)
        if [ "$1 $2" = "window displays" ]; then echo "  mFocusedApp=ActivityRecord{x}"; else
            echo "  mCurrentFocus=Window{aaa u0 old/ANR}"; echo "  mCurrentFocus=Window{bbb u0 com.fidderr.alarmfree/android.app.NativeActivity}"; fi ;;
    *)
        if [ "$1 $2" = "window displays" ]; then echo "  mCurrentFocus=Window{bbb u0 com.fidderr.alarmfree/android.app.NativeActivity}"; fi ;;
esac
EOF
chmod +x "$tmp/bin/dumpsys"
dev() { PATH="$tmp/bin:$PATH" bash -c "$*" 2>&1 | tr -d '\r'; }
for api in 34 28; do
    export FAKE_API=$api
    got=$(focused_window)
    lines=$(printf '%s\n' "$got" | grep -c .)
    verdict "focused_window on API $api is the one focus line, no dumpsys noise ($lines line(s))" "$([ "$lines" = 1 ] && printf '%s' "$got" | grep -q 'NativeActivity' && echo 1)" "got: $got"
    other=$(sys_focus | grep -v -e documentsui -e "$PKG" -e '^$' -e '^null')
    verdict "an app in front is no picker preview on API $api (the noise line is not a window)" "$([ -z "$other" ] && echo 1)" "left over: $other"
done

# ---- the lines that count as broken ----
benign_pair="11-25 12:25:45.556 15228 15250 I dderr.alarmfre: Rejecting re-init on previously-failed class java.lang.Class<dev.slint.android.SlintAndroidJavaHelper\$1>: java.lang.NoClassDefFoundError: Failed resolution of: Landroid/view/WindowInsetsAnimation\$Callback;
11-25 12:25:45.556 15228 15250 I dderr.alarmfre: Caused by: java.lang.ClassNotFoundException: Didn't find class \"android.view.WindowInsetsAnimation\$Callback\" on path: DexPathList[[dex file \"InMemoryDexFile[cookie=[0, 1]]\"],nativeLibraryDirectories=[/system/lib64]]"
messages="11-25 13:08:57.654  3128  3128 I AppCompatDelegate: java.lang.ClassNotFoundException: androidx.appcompat.app.AppCompatViewInflater"
helper_missing="11-25 13:10:00.000 15228 15250 E dderr.alarmfre: java.lang.ClassNotFoundException: Didn't find class \"dev.slint.android.SlintAndroidJavaHelper\" on path: DexPathList[[dex file]]"
helper_other="11-25 13:10:00.000 15228 15250 I dderr.alarmfre: java.lang.NoClassDefFoundError: Failed resolution of: Ldev/slint/android/SlintAndroidJavaHelper;"
crash="11-25 13:10:00.000 15228 15250 E AndroidRuntime: FATAL EXCEPTION: main"
dead_app_crash="11-25 13:10:01.000 9999 9999 E AndroidRuntime: FATAL EXCEPTION: main"
other_class="11-25 13:10:02.000 15228 15250 I dderr.alarmfre: java.lang.ClassNotFoundException: com.example.Other"
count_of() { printf '%s\n' "$1" | grep -c .; }
SDK_LEVEL=28
got=$(printf '%s\n%s\n' "$benign_pair" "$messages" | broke_lines)
verdict "Android 9: the helper's benign pair and the Messages line are left out" "$([ -z "$got" ] && echo 1)" "kept: $got"
for name in helper_missing helper_other crash dead_app_crash other_class; do
    got=$(printf '%s\n%s\n' "$benign_pair" "${!name}" | broke_lines)
    verdict "Android 9: $name stays a problem, next to the benign pair" "$([ "$(count_of "$got")" = 1 ] && echo 1)" "kept: $got"
done
SDK_LEVEL=34
got=$(printf '%s\n%s\n' "$benign_pair" "$messages" | broke_lines)
verdict "Android 14: nothing is left out (3 lines stay)" "$([ "$(count_of "$got")" = 3 ] && echo 1)" "kept: $got"

# ---- grant_all: the storage permissions only where the app declares them ----
adb_() { GRANTED="$*"; }
SDK_LEVEL=28
grant_all
verdict "grant_all on Android 9 grants WRITE and READ_EXTERNAL_STORAGE" "$(case "$GRANTED" in *"pm grant $PKG android.permission.WRITE_EXTERNAL_STORAGE"*"pm grant $PKG android.permission.READ_EXTERNAL_STORAGE"*) echo 1 ;; esac)" "$GRANTED"
SDK_LEVEL=33
grant_all
verdict "grant_all on Android 13 grants no storage permission, and the rest as before" "$(case "$GRANTED" in *EXTERNAL_STORAGE*) ;; *"pm grant $PKG android.permission.CAMERA"*"appops set $PKG RUN_ANY_IN_BACKGROUND allow"*) echo 1 ;; esac)" "$GRANTED"

# ---- force_idle: only Android 9 asks again ----
fail() { FAILED="$*"; }
wait_for() { return 0; }
dev() {
    echo "$*" >> "$tmp/calls"
    case "$*" in
        "dumpsys deviceidle force-idle") echo "Unable to go deep idle; not enabled" ;;
        "dumpsys deviceidle get deep") echo INACTIVE ;;
    esac
}
for api in 28 33; do
    SDK_LEVEL=$api FAILED=""
    : > "$tmp/calls"
    force_idle
    CALLS=$(tr '
' '|' < "$tmp/calls")
    asked=$(grep -c 'enable deep' "$tmp/calls")
    count=$(grep -c 'force-idle' "$tmp/calls")
    if [ "$api" = 28 ]; then
        verdict "force_idle on Android 9 enables deep idle and asks again ($count force-idle calls)" "$([ "$asked" = 1 ] && [ "$count" -gt 1 ] && echo 1)" "$CALLS"
    else
        verdict "force_idle on Android 13 asks once and fails with what Android said ($count call)" "$([ "$asked" = 0 ] && [ "$count" = 1 ] && case "$FAILED" in *"not enabled"*) echo 1 ;; esac)" "$CALLS / $FAILED"
    fi
done

# ---- no_locked_boot_here (matrix.sh): file encryption runs the scenario, block skips it, no answer fails it ----
eval "$(sed -n '/^no_locked_boot_here()/,/^}/p' "$here/matrix.sh")"
scenario() { CURRENT=$1; }
skip() { RESULT="SKIP $*"; }
fail() { RESULT="FAIL $*"; }
for fake in file block ""; do
    RESULT=""
    dev() { echo "$fake"; }
    no_locked_boot_here s13_x && rc=0 || rc=1
    case "$fake" in
        file) verdict "ro.crypto.type=file: the scenario runs (status 1, nothing reported)" "$([ "$rc" = 1 ] && [ -z "$RESULT" ] && echo 1)" "rc=$rc $RESULT" ;;
        block) verdict "ro.crypto.type=block: skipped with its reason" "$([ "$rc" = 0 ] && case "$RESULT" in "SKIP no locked boot"*block*) echo 1 ;; esac)" "rc=$rc $RESULT" ;;
        "") verdict "no answer for ro.crypto.type: a FAIL, not a skip" "$([ "$rc" = 0 ] && case "$RESULT" in "FAIL "*"could not be read"*) echo 1 ;; esac)" "rc=$rc $RESULT" ;;
    esac
done

# ---- the API level is asked once ----
unset SDK_LEVEL
: > "$tmp/reads"
dev() { echo read >> "$tmp/reads"; echo 28; }
sdk_level > /dev/null
a=$(sdk_level)
b=$(sdk_level)
N=$(grep -c read "$tmp/reads")
verdict "the API level is read once when the runner primes it ($N read, $a $b)" "$([ "$N" = 1 ] && [ "$a$b" = 2828 ] && echo 1)" "reads: $N"
verdict "the runner primes it in the main shell before it runs anything" "$(grep -q '^sdk_level > /dev/null' "$here/matrix.sh" && echo 1)" "no line in matrix.sh"

# ---- the picker's own crash is named, an app crash and a clean log are not ----
dev() { printf '%s\n' "11-26 03:04:34.351 E/AndroidRuntime(15443): FATAL EXCEPTION: main" "11-26 03:04:34.351 E/AndroidRuntime(15443): Process: com.google.android.documentsui, PID: 15443" "11-26 03:04:34.351 E/AndroidRuntime(15443): android.database.StaleDataException: Attempted to access a cursor after it has been closed."; }
got=$(picker_crash)
verdict "a crash of DocumentsUI is named by its exception" "$([ "$got" = "android.database.StaleDataException: Attempted to access a cursor after it has been closed." ] && echo 1)" "got: $got"
dev() { printf '%s\n' "11-26 03:04:34.351 E/AndroidRuntime(1): FATAL EXCEPTION: main" "11-26 03:04:34.351 E/AndroidRuntime(1): Process: com.fidderr.alarmfree, PID: 1" "11-26 03:04:34.351 E/AndroidRuntime(1): java.lang.Boom"; }
picker_crash > /dev/null
verdict "a crash of another app is not the picker's" "$([ $? -ne 0 ] && echo 1)"
dev() { :; }
picker_crash > /dev/null
verdict "a clean log is no crash" "$([ $? -ne 0 ] && echo 1)"

# ---- the picker waits for a still list once for each opening ----
waits=0
picker_wait_list() { waits=$((waits + 1)); }
sys_dump() { return 0; }
PICKER_STILL=""
picker_dump; picker_dump; picker_dump
verdict "three dumps of one opening of the picker wait for the list once ($waits)" "$([ "$waits" = 1 ] && echo 1)" "waits: $waits"
PICKER_STILL=""
picker_dump; picker_dump
verdict "after a touch that loads another listing (PICKER_STILL cleared) the next dump waits again ($waits)" "$([ "$waits" = 2 ] && echo 1)" "waits: $waits"

exit $bad
