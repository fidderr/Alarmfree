use dioxus::prelude::*;

/// Toast notification level.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum ToastLevel {
    Info,
    Warning,
    Error,
}

/// A toast message to display.
#[derive(Clone, PartialEq, Debug)]
pub struct ToastMessage {
    pub message: String,
    pub level: ToastLevel,
}

/// Toast notification component. Renders at the bottom of the screen.
/// Auto-dismisses after a timeout based on level.
#[component]
pub fn Toast(msg: ToastMessage, on_dismiss: EventHandler<()>) -> Element {
    let level_class = match msg.level {
        ToastLevel::Info => "toast-info",
        ToastLevel::Warning => "toast-warning",
        ToastLevel::Error => "toast-error",
    };

    // Auto-dismiss timer
    let timeout_ms = match msg.level {
        ToastLevel::Info => 3000u64,
        ToastLevel::Warning => 5000,
        ToastLevel::Error => 10000,
    };

    use_future(move || async move {
        let (tx, rx) = futures_channel::oneshot::channel::<()>();
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(timeout_ms));
            let _ = tx.send(());
        });
        let _ = rx.await;
        on_dismiss.call(());
    });

    rsx! {
        div { class: "toast-container",
            div {
                class: "toast {level_class}",
                onclick: move |_| { on_dismiss.call(()); },
                p { "{msg.message}" }
            }
        }
    }
}
