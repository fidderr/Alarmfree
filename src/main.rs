//! AlarmFree - A free, ad-free mobile alarm clock with challenge-based dismissal.

fn main() {
    // The page's colour before it has painted anything: Dioxus's own is white, and a dark app
    // flashed. The mobile configuration only exists in a build for a phone.
    #[cfg(target_os = "android")]
    dioxus::LaunchBuilder::new()
        .with_cfg(
            dioxus::mobile::Config::new().with_background_color(alarmfree::app::start_background()),
        )
        .launch(alarmfree::app::App);
    #[cfg(not(target_os = "android"))]
    dioxus::launch(alarmfree::app::App);
}
