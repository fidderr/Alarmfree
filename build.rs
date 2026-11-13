fn main() {
    // devtest/ finds elements by their text and place; that needs the compiler's debug info.
    let config = slint_build::CompilerConfiguration::new()
        .with_debug_info(std::env::var_os("CARGO_FEATURE_DEVTEST").is_some());
    slint_build::compile_with_config("ui/main.slint", config).expect("compile the UI");
}
