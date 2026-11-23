//! The oldest Android the app runs on is `min_sdk` in `sentinel.toml`, and nothing else names an API level.

use std::fs;

fn read(path: &str) -> String {
    fs::read_to_string(format!("{}/{path}", env!("CARGO_MANIFEST_DIR"))).unwrap()
}

#[test]
fn min_sdk_is_android_10() {
    let toml = read("sentinel.toml");
    let min = toml
        .lines()
        .find_map(|l| l.strip_prefix("min_sdk = "))
        .expect("sentinel.toml sets min_sdk");
    assert_eq!(min.trim(), "29");
}

#[test]
fn the_native_code_is_built_for_that_level_and_no_other() {
    let build = read("build.sh");
    assert!(build.contains("MIN_SDK=$(sed -n"), "build.sh reads min_sdk");
    for line in build.lines().filter(|l| l.contains("-clang")) {
        assert!(
            line.contains("${MIN_SDK}-clang"),
            "build.sh hard-codes an API level: {line}"
        );
    }
    assert!(
        !read(".cargo/config.toml").contains("linker ="),
        "the linker comes from build.sh"
    );
}

#[test]
fn the_window_hides_the_keyboard_when_it_comes_up() {
    let toml = read("sentinel.toml");
    let main = toml
        .split("[[android.declare.activity]]")
        .nth(1)
        .expect("sentinel.toml declares the main activity")
        .split("\n[")
        .next()
        .unwrap();
    assert!(main.contains("name = \"android.app.NativeActivity\""));
    assert!(
        main.contains("windowSoftInputMode = \"stateHidden|adjustResize\""),
        "a new window must not show the keyboard by itself"
    );
    assert!(main.contains("launcher = true") && main.contains("native_lib = \"main\""));
    assert!(main.contains("exported = true"));
}
