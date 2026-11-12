//! AlarmFree on a desktop, for working on the screens. The phone loads the library instead.

fn main() {
    if let Err(e) = alarmfree::ui::run() {
        eprintln!("the UI stopped: {e}");
    }
}
