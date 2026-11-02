//! The one Back rule, on every Android version: close the sheet that is open, else
//! leave the screen, and on Home go to the background.
//!
//! Android sends Back here through `mobile_sentinel::foregrounding::on_back`, so the
//! system never finishes the activity, which would end the process. Whatever is on
//! top registers with [`use_back_handler`]; the routes are handled in
//! `app::NavWatcher`.

use std::cell::RefCell;
use std::rc::Rc;

use dioxus::prelude::*;
use tokio::sync::Notify;

/// A Back press waiting to be picked up by the UI. Android calls on its own thread.
pub static BACK_PRESSED: Notify = Notify::const_new();

/// One thing that wants Back, by a number to take it away with.
type Entry = (u64, EventHandler<()>);

/// What is on top right now, innermost last.
#[derive(Clone, Default)]
pub struct BackStack(Rc<RefCell<Vec<Entry>>>);

impl BackStack {
    /// Hand Back to whatever is on top. Returns whether something took it.
    pub fn pop(&self) -> bool {
        let top = self.0.borrow().last().map(|(_, handler)| *handler);
        match top {
            Some(handler) => {
                handler.call(());
                true
            }
            None => false,
        }
    }
}

/// While the calling component is on screen, Back calls `on_back`, unless something
/// opened after it is still there.
pub fn use_back_handler(on_back: EventHandler<()>) {
    let stack = use_context::<BackStack>();
    let id = use_hook({
        let stack = stack.clone();
        move || {
            static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
            let id = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            stack.0.borrow_mut().push((id, on_back));
            id
        }
    });
    use_drop(move || stack.0.borrow_mut().retain(|(other, _)| *other != id));
}
