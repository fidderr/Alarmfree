fn main() {
    // devtest/ finds elements by their text and place; that needs the compiler's debug info.
    let config = slint_build::CompilerConfiguration::new()
        .with_debug_info(std::env::var_os("CARGO_FEATURE_DEVTEST").is_some());
    slint_build::compile_with_config("ui/main.slint", config).expect("compile the UI");
    version_from_sentinel_toml();
}

/// `sentinel.toml` is the one place the Android version lives: the versionCode reaches the code as
/// `ALARMFREE_VERSION_CODE`, and a versionName that differs from the crate version stops the build,
/// so a release can not go out with the two out of step.
fn version_from_sentinel_toml() {
    println!("cargo:rerun-if-changed=sentinel.toml");
    let toml = std::fs::read_to_string("sentinel.toml").expect("read sentinel.toml");
    let value = |key: &str| {
        toml.lines()
            .map(str::trim)
            .find_map(|line| {
                let (k, v) = line.split_once('=')?;
                (k.trim() == key).then(|| {
                    v.split('#')
                        .next()
                        .unwrap_or("")
                        .trim()
                        .trim_matches('"')
                        .to_string()
                })
            })
            .unwrap_or_else(|| panic!("sentinel.toml has no {key}"))
    };
    let code = value("version_code");
    code.parse::<u32>()
        .unwrap_or_else(|_| panic!("sentinel.toml: version_code {code:?} is not a number"));
    let name = value("version_name");
    let crate_version = std::env::var("CARGO_PKG_VERSION").unwrap();
    assert_eq!(
        name, crate_version,
        "sentinel.toml has version_name {name:?} but Cargo.toml has version {crate_version:?}: change both"
    );
    println!("cargo:rustc-env=ALARMFREE_VERSION_CODE={code}");
}
