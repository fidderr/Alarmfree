use dioxus::prelude::*;

use crate::app::{t, Translations};

/// Unified scanner challenge (barcode or QR — the camera backend handles
/// both identically).
///
/// On Android: launches the native scanner via the gated `camera`
/// capability. On non-Android: tap simulates a successful scan.
///
/// If `expected_value` is empty, any scanned code dismisses; otherwise the
/// scanned value must match exactly.
#[component]
pub fn ScanChallenge(
    #[props(default = String::new())] expected_value: String,
    on_complete: EventHandler<()>,
) -> Element {
    let translations = use_context::<Translations>();
    let instruction_any = t(&translations, "challenge.scan.instruction_any");
    let instruction_specific = t(&translations, "challenge.scan.instruction_specific");
    let scan_label = t(&translations, "challenge.scan.scan_button");
    let mismatch_msg = t(&translations, "challenge.scan.mismatch");
    let tap_retry = t(&translations, "challenge.scan.tap_retry");
    let tap_to_scan = t(&translations, "challenge.scan.tap_to_scan");
    let scanning_text = t(&translations, "challenge.scan.scanning");
    let point_camera = t(&translations, "challenge.scan.point_camera");
    let success_text = t(&translations, "challenge.scan.success");
    let mut status = use_signal(|| "ready"); // "ready", "scanning", "success", "error"
    let mut error_msg = use_signal(String::new);
    let mut scan_requested = use_signal(|| false);
    let expected = expected_value.clone();
    let mismatch_msg_for_handler = mismatch_msg.clone();

    // Background task for scanning (same pattern as alarm_config).
    let _scan_task = use_future(move || {
        let expected_clone = expected.clone();
        let mismatch_msg_for_handler = mismatch_msg_for_handler.clone();
        async move {
            loop {
                // Poll for scan request.
                loop {
                    let (tx, rx) = futures_channel::oneshot::channel::<()>();
                    std::thread::spawn(move || {
                        std::thread::sleep(std::time::Duration::from_millis(100));
                        let _ = tx.send(());
                    });
                    let _ = rx.await;
                    if *scan_requested.read() {
                        break;
                    }
                }
                scan_requested.set(false);

                let (tx, rx) = futures_channel::oneshot::channel::<Result<String, String>>();

                std::thread::spawn(move || {
                    // Sole sanctioned scanning entry point (feature-gated).
                    let result = mobile_sentinel::scanner::scan().map_err(|e| format!("{}", e));
                    let _ = tx.send(result);
                });

                match rx.await {
                    Ok(Ok(value)) => {
                        let scanned = value.trim().to_string();
                        let expected = expected_clone.trim().to_string();
                        if scanned.is_empty() {
                            // User cancelled scan — go back to ready state.
                            status.set("ready");
                        } else if expected.is_empty() || scanned == expected {
                            status.set("success");
                            on_complete.call(());
                        } else {
                            eprintln!(
                                "[AlarmFree] Scan mismatch: scanned='{}' expected='{}'",
                                scanned, expected
                            );
                            error_msg.set(mismatch_msg_for_handler.clone());
                            status.set("error");
                        }
                    }
                    Ok(Err(_)) => {
                        // Scanner returned error (cancelled/timeout) — back to ready.
                        status.set("ready");
                    }
                    Err(_) => {
                        status.set("ready");
                    }
                }
            }
        }
    });

    rsx! {
        div { class: "challenge-wrapper",
            p { class: "challenge-instruction",
                if expected_value.is_empty() {
                    "{instruction_any}"
                } else {
                    "{instruction_specific}"
                }
            }

            div { class: "progress-ring-container",
                match &**status.read() {
                    "ready" | "error" => rsx! {
                        div {
                            class: "progress-ring",
                            onclick: move |_| {
                                status.set("scanning");
                                scan_requested.set(true);
                            },
                            div { class: "progress-ring-text", "{scan_label}" }
                        }
                        if *status.read() == "error" {
                            p { style: "color: var(--error); font-size: 13px; text-align: center; margin-top: 8px;",
                                "{error_msg}"
                            }
                            p { class: "progress-ring-label", "{tap_retry}" }
                        } else {
                            p { class: "progress-ring-label", "{tap_to_scan}" }
                        }
                    },
                    "scanning" => rsx! {
                        div { class: "progress-ring",
                            div { class: "progress-ring-text",
                                style: "font-size: 14px;",
                                "{scanning_text}"
                            }
                        }
                        p { class: "progress-ring-label", "{point_camera}" }
                    },
                    _ => rsx! {
                        div { class: "progress-ring",
                            style: "border-color: var(--success);",
                            div { class: "progress-ring-text",
                                style: "color: var(--success); font-size: 40px;",
                                "\u{2713}"
                            }
                        }
                        p { class: "progress-ring-label",
                            style: "color: var(--success);",
                            "{success_text}"
                        }
                    },
                }
            }
        }
    }
}
