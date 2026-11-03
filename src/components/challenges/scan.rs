use std::sync::atomic::{AtomicBool, Ordering};

use dioxus::prelude::*;

use crate::app::{t, Translations};

static SCANNER_OPEN: AtomicBool = AtomicBool::new(false);

/// Whether the camera scanner is on screen right now.
pub fn scanner_open() -> bool {
    SCANNER_OPEN.load(Ordering::Relaxed)
}

#[derive(Clone, Copy, PartialEq)]
enum ScanState {
    Ready,
    Scanning,
    WrongCode,
    Done,
}

/// Scan a barcode or QR code: any code if `expected_value` is empty, otherwise that one.
#[component]
pub fn ScanChallenge(
    #[props(default = String::new())] expected_value: String,
    on_complete: EventHandler<()>,
) -> Element {
    let translations = use_context::<Translations>();
    let instruction = if expected_value.is_empty() {
        t(&translations, "challenge.scan.instruction_any")
    } else {
        t(&translations, "challenge.scan.instruction_specific")
    };
    let scan_label = t(&translations, "challenge.scan.scan_button");
    let mismatch_text = t(&translations, "challenge.scan.mismatch");
    let tap_retry = t(&translations, "challenge.scan.tap_retry");
    let tap_to_scan = t(&translations, "challenge.scan.tap_to_scan");
    let scanning_text = t(&translations, "challenge.scan.scanning");
    let point_camera = t(&translations, "challenge.scan.point_camera");
    let success_text = t(&translations, "challenge.scan.success");
    let mut state = use_signal(|| ScanState::Ready);

    let scan = move |_| {
        let expected = expected_value.trim().to_owned();
        state.set(ScanState::Scanning);
        spawn(async move {
            let (tx, rx) = futures_channel::oneshot::channel();
            // Blocks until the scanner is done.
            std::thread::spawn(move || {
                SCANNER_OPEN.store(true, Ordering::Relaxed);
                let result = mobile_sentinel::scanner::scan();
                SCANNER_OPEN.store(false, Ordering::Relaxed);
                let _ = tx.send(result);
            });
            // Nothing scanned: the user went back, or it took too long.
            let scanned = rx.await.ok().and_then(Result::ok).unwrap_or_default();
            let scanned = scanned.trim();
            if scanned.is_empty() {
                state.set(ScanState::Ready);
            } else if expected.is_empty() || scanned == expected {
                state.set(ScanState::Done);
                on_complete.call(());
            } else {
                log::warn!("[AlarmFree] Scanned something else than the expected code");
                state.set(ScanState::WrongCode);
            }
        });
    };

    rsx! {
        div { class: "challenge-wrapper",
            p { class: "challenge-instruction", "{instruction}" }
            div { class: "progress-ring-container",
                match state() {
                    ScanState::Ready | ScanState::WrongCode => rsx! {
                        button { class: "scan-button", onclick: scan, "{scan_label}" }
                        if state() == ScanState::WrongCode {
                            p { class: "run-wrong", role: "status", "{mismatch_text}" }
                            p { class: "progress-ring-label", "{tap_retry}" }
                        } else {
                            p { class: "progress-ring-label", "{tap_to_scan}" }
                        }
                    },
                    ScanState::Scanning => rsx! {
                        div { class: "scan-button scanning", "{scanning_text}" }
                        p { class: "progress-ring-label", "{point_camera}" }
                    },
                    ScanState::Done => rsx! {
                        div { class: "scan-button done", "\u{2713}" }
                        p { class: "progress-ring-label", "{success_text}" }
                    },
                }
            }
        }
    }
}
