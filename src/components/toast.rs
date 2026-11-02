use dioxus::prelude::*;

use crate::app::{t, AlarmData, Translations};

/// Toast notification level.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum ToastLevel {
    Info,
    Warning,
    Error,
}

/// A message at the bottom of the screen.
#[derive(Clone, PartialEq, Debug)]
pub struct ToastMessage {
    pub message: String,
    pub level: ToastLevel,
    /// The alarm list to go back to if the user taps Undo. As data, since the screen
    /// that made the change may be gone by then.
    pub undo: Option<Vec<AlarmData>>,
}

impl ToastMessage {
    pub fn info(message: String) -> Self {
        Self {
            message,
            level: ToastLevel::Info,
            undo: None,
        }
    }

    pub fn error(message: String) -> Self {
        Self {
            message,
            level: ToastLevel::Error,
            undo: None,
        }
    }

    pub fn with_undo(message: String, before: Vec<AlarmData>) -> Self {
        Self {
            message,
            level: ToastLevel::Info,
            undo: Some(before),
        }
    }
}

/// A message at the bottom of the screen, gone after a while. It stays longer the more
/// it matters and can undo what it reports.
#[component]
pub fn Toast(
    msg: ToastMessage,
    on_dismiss: EventHandler<()>,
    on_undo: EventHandler<Vec<AlarmData>>,
) -> Element {
    let translations = use_context::<Translations>();
    let level_class = match msg.level {
        ToastLevel::Info => "toast-info",
        ToastLevel::Warning => "toast-warning",
        ToastLevel::Error => "toast-error",
    };

    let timeout_ms = match (&msg.undo, msg.level) {
        (Some(_), _) => 6000u64,
        (None, ToastLevel::Info) => 3000,
        (None, ToastLevel::Warning) => 5000,
        (None, ToastLevel::Error) => 10000,
    };

    use_future(move || async move {
        crate::timer::sleep(std::time::Duration::from_millis(timeout_ms)).await;
        on_dismiss.call(());
    });

    let undo_text = t(&translations, "action.undo");
    rsx! {
        div {
            class: "toast {level_class}",
            role: "status",
            onclick: move |_| on_dismiss.call(()),
            p { class: "toast-text", "{msg.message}" }
            if let Some(before) = msg.undo.clone() {
                button {
                    class: "toast-undo",
                    onclick: move |evt| {
                        evt.stop_propagation();
                        on_undo.call(before.clone());
                    },
                    "{undo_text}"
                }
            }
        }
    }
}
