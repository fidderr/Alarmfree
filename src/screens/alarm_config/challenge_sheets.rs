//! What it takes to turn the alarm off.

use dioxus::prelude::*;

use super::{EditorSheet, EditorState};
use crate::app::{t, Translations};
use crate::components::{Chips, GroupTitle, Icon, IconName, Sheet, SheetDone, Stepper};
use crate::models::setup::{difficulty_name, CHALLENGE_TYPES};
use crate::models::{ChallengeSetup, Difficulty};

/// The most challenges an alarm takes.
const MAX_CHALLENGES: usize = 10;

const DIFFICULTIES: [Difficulty; 4] = [
    Difficulty::Easy,
    Difficulty::Medium,
    Difficulty::Hard,
    Difficulty::Extreme,
];

/// Ask for the permission a challenge needs (steps, camera) before it is added or tried.
fn request_challenge_permission(setup: &ChallengeSetup) {
    let permission = match setup {
        ChallengeSetup::Steps { .. } => Some("android.permission.ACTIVITY_RECOGNITION"),
        ChallengeSetup::Scan { .. } => Some("android.permission.CAMERA"),
        _ => None,
    };
    if let Some(permission) = permission {
        let _ = mobile_sentinel::permissions::request(permission);
    }
}

/// What an alarm's challenge is set to, in a few words.
fn challenge_summary(translations: &Translations, setup: &ChallengeSetup) -> String {
    let difficulty = |d: Difficulty| t(translations, &format!("difficulty.{}", difficulty_name(d)));
    match setup {
        ChallengeSetup::Math { difficulty: d }
        | ChallengeSetup::Memory { difficulty: d }
        | ChallengeSetup::Typing { difficulty: d } => difficulty(*d),
        ChallengeSetup::Shake { count } => {
            t(translations, "challenge.summary.shakes").replace("{count}", &count.to_string())
        }
        ChallengeSetup::Steps { count } => {
            t(translations, "challenge.summary.steps").replace("{count}", &count.to_string())
        }
        ChallengeSetup::Hold { count, secs } => {
            crate::text::counted(translations, "challenge.summary.hold", *count)
                .replace("{secs}", &secs.to_string())
        }
        ChallengeSetup::Reaction {
            targets,
            secs,
            rounds,
        } => t(
            translations,
            if *rounds == 1 {
                "challenge.summary.reaction_one"
            } else {
                "challenge.summary.reaction"
            },
        )
        .replace("{count}", &targets.to_string())
        .replace("{secs}", &secs.to_string())
        .replace("{rounds}", &rounds.to_string()),
        ChallengeSetup::Scan { code } if code.is_empty() => {
            t(translations, "challenge.summary.any")
        }
        ChallengeSetup::Scan { code } => {
            t(translations, "challenge.summary.code").replace("{code}", code)
        }
    }
}

/// An example of what a difficulty means for this challenge.
fn difficulty_example(translations: &Translations, setup: &ChallengeSetup) -> String {
    let (name, difficulty) = match setup {
        ChallengeSetup::Math { difficulty } => ("math", difficulty),
        ChallengeSetup::Memory { difficulty } => ("memory", difficulty),
        ChallengeSetup::Typing { difficulty } => ("typing", difficulty),
        _ => return String::new(),
    };
    t(
        translations,
        &format!("challenge.example.{name}_{}", difficulty_name(*difficulty)),
    )
}

/// The list of them, and how they run.
#[component]
pub(super) fn ChallengesSheet() -> Element {
    let EditorState {
        mut sheet,
        mut draft,
        mut setups,
        mut trying,
        ..
    } = use_context();
    let translations = use_context::<Translations>();
    let current = draft.read().clone();
    let done_text = t(&translations, "action.done");
    let list = setups.read().clone();
    let count = list.len();
    let try_text = t(&translations, "challenge.try");
    let remove_text = t(&translations, "config.remove_challenge");
    let drag_text = t(&translations, "config.drag_challenge");
    let order_options = vec![
        (false, t(&translations, "config.order_fixed")),
        (true, t(&translations, "config.order_random")),
    ];
    rsx! {
        Sheet {
            title: t(&translations, "config.challenges"),
            note: t(&translations, "config.challenges_note"),
            on_close: move |_| sheet.set(None),
            div { id: "challenge-list", class: "challenge-list",
                for (index, setup) in list.iter().cloned().enumerate() {
                    {
                        let name = t(&translations, setup.type_key());
                        let summary = challenge_summary(&translations, &setup);
                        let to_try = setup.clone();
                        rsx! {
                            div { key: "{index}-{setup.type_key()}", class: "challenge-row",
                                span { class: "drag-handle", "aria-label": "{drag_text}",
                                    Icon { name: IconName::Grip, size: 18 }
                                }
                                span { class: "challenge-number", "aria-hidden": "true", "{index + 1}" }
                                button {
                                    class: "challenge-open",
                                    onclick: move |_| sheet.set(Some(EditorSheet::Detail(index))),
                                    span { class: "challenge-name", "{name}" }
                                    span { class: "challenge-summary", "{summary}" }
                                }
                                button {
                                    class: "link-btn",
                                    onclick: move |_| {
                                        request_challenge_permission(&to_try);
                                        trying.set(Some(vec![to_try.clone()]));
                                    },
                                    "{try_text}"
                                }
                                button {
                                    class: "icon-btn",
                                    "aria-label": "{remove_text} {name}",
                                    onclick: move |_| {
                                        setups.write().remove(index);
                                    },
                                    Icon { name: IconName::Xmark, size: 18 }
                                }
                            }
                        }
                    }
                }
            }
            if count < MAX_CHALLENGES {
                button {
                    class: "btn btn-quiet",
                    onclick: move |_| sheet.set(Some(EditorSheet::AddChallenge)),
                    {t(&translations, "config.add_challenge")}
                }
            }
            if count > 0 {
                button {
                    class: "btn btn-quiet",
                    onclick: move |_| {
                        let list = setups.peek().clone();
                        for setup in &list {
                            request_challenge_permission(setup);
                        }
                        trying.set(Some(list));
                    },
                    {t(&translations, "config.try_all")}
                }
            }
            GroupTitle { text: t(&translations, "config.order") }
            Chips::<bool> {
                grow: true,
                options: order_options,
                selected: Some(current.random_order),
                on_pick: move |random: bool| draft.write().random_order = random,
            }
            SheetDone { label: done_text.clone(), on_click: move |_| sheet.set(None) }
        }
    }
}

/// Picking a kind of challenge to add.
#[component]
pub(super) fn AddChallengeSheet() -> Element {
    let EditorState {
        mut sheet,
        mut setups,
        ..
    } = use_context();
    let translations = use_context::<Translations>();
    rsx! {
        Sheet {
            title: t(&translations, "config.select_challenge"),
            note: t(&translations, "config.add_note"),
            on_close: move |_| sheet.set(Some(EditorSheet::Challenges)),
            for key in CHALLENGE_TYPES {
                button {
                    class: "add-option",
                    onclick: move |_| {
                        if let Some(setup) = ChallengeSetup::new(key) {
                            request_challenge_permission(&setup);
                            let mut list = setups.write();
                            list.push(setup);
                            let index = list.len() - 1;
                            drop(list);
                            sheet.set(Some(EditorSheet::Detail(index)));
                        }
                    },
                    span { class: "add-name", {t(&translations, key)} }
                    span { class: "add-desc", {t(&translations, &key.replace("challenge.", "challenge.desc."))} }
                }
            }
            SheetDone {
                label: t(&translations, "challenge.config.back"),
                on_click: move |_| sheet.set(Some(EditorSheet::Challenges)),
            }
        }
    }
}

/// The settings of one of them.
#[component]
pub(super) fn ChallengeDetailSheet(index: usize) -> Element {
    let EditorState {
        mut sheet,
        mut setups,
        mut trying,
        ..
    } = use_context();
    let translations = use_context::<Translations>();
    let done_text = t(&translations, "action.done");
    let Some(setup) = setups.read().get(index).cloned() else {
        return rsx! {};
    };
    let name = t(&translations, setup.type_key());
    let description = t(
        &translations,
        &setup.type_key().replace("challenge.", "challenge.desc."),
    );
    let example = difficulty_example(&translations, &setup);
    let mut update = move |change: &dyn Fn(&mut ChallengeSetup)| {
        if let Some(setup) = setups.write().get_mut(index) {
            change(setup);
        }
    };
    let to_try = setup.clone();
    let controls = match setup {
        ChallengeSetup::Math { difficulty }
        | ChallengeSetup::Memory { difficulty }
        | ChallengeSetup::Typing { difficulty } => rsx! {
            GroupTitle { text: t(&translations, "challenge.config.difficulty") }
            Chips::<Difficulty> {
                grow: true,
                options: DIFFICULTIES
                    .iter()
                    .map(|d| (*d, t(&translations, &format!("difficulty.{}", difficulty_name(*d)))))
                    .collect::<Vec<_>>(),
                selected: Some(difficulty),
                on_pick: move |picked: Difficulty| update(&|setup| match setup {
                    ChallengeSetup::Math { difficulty }
                    | ChallengeSetup::Memory { difficulty }
                    | ChallengeSetup::Typing { difficulty } => *difficulty = picked,
                    _ => {}
                }),
            }
            p { class: "example", "{example}" }
        },
        ChallengeSetup::Shake { count } => rsx! {
            Stepper {
                title: t(&translations, "challenge.config.how_many_shakes"),
                value: "{count}",
                on_less: move |_| update(&|s| if let ChallengeSetup::Shake { count } = s { *count = count.saturating_sub(5).max(5) }),
                on_more: move |_| update(&|s| if let ChallengeSetup::Shake { count } = s { *count = (*count + 5).min(200) }),
            }
        },
        ChallengeSetup::Steps { count } => rsx! {
            Stepper {
                title: t(&translations, "challenge.config.how_many_steps"),
                value: "{count}",
                on_less: move |_| update(&|s| if let ChallengeSetup::Steps { count } = s { *count = count.saturating_sub(5).max(5) }),
                on_more: move |_| update(&|s| if let ChallengeSetup::Steps { count } = s { *count = (*count + 5).min(200) }),
            }
        },
        ChallengeSetup::Hold { count, secs } => rsx! {
            Stepper {
                title: t(&translations, "challenge.config.how_many_holds"),
                value: "{count}",
                on_less: move |_| update(&|s| if let ChallengeSetup::Hold { count, .. } = s { *count = count.saturating_sub(1).max(1) }),
                on_more: move |_| update(&|s| if let ChallengeSetup::Hold { count, .. } = s { *count = (*count + 1).min(30) }),
            }
            Stepper {
                title: t(&translations, "challenge.config.hold_duration"),
                value: t(&translations, "text.seconds").replace("{seconds}", &secs.to_string()),
                on_less: move |_| update(&|s| if let ChallengeSetup::Hold { secs, .. } = s { *secs = secs.saturating_sub(1).max(1) }),
                on_more: move |_| update(&|s| if let ChallengeSetup::Hold { secs, .. } = s { *secs = (*secs + 1).min(15) }),
            }
        },
        ChallengeSetup::Reaction {
            targets,
            secs,
            rounds,
        } => rsx! {
            Stepper {
                title: t(&translations, "challenge.config.how_many_targets"),
                value: "{targets}",
                on_less: move |_| update(&|s| if let ChallengeSetup::Reaction { targets, .. } = s { *targets = targets.saturating_sub(1).max(3) }),
                on_more: move |_| update(&|s| if let ChallengeSetup::Reaction { targets, .. } = s { *targets = (*targets + 1).min(30) }),
            }
            Stepper {
                title: t(&translations, "challenge.config.time_limit"),
                value: t(&translations, "text.seconds").replace("{seconds}", &secs.to_string()),
                on_less: move |_| update(&|s| if let ChallengeSetup::Reaction { secs, .. } = s { *secs = secs.saturating_sub(1).max(2) }),
                on_more: move |_| update(&|s| if let ChallengeSetup::Reaction { secs, .. } = s { *secs = (*secs + 1).min(10) }),
            }
            Stepper {
                title: t(&translations, "challenge.config.how_many_rounds"),
                value: "{rounds}",
                on_less: move |_| update(&|s| if let ChallengeSetup::Reaction { rounds, .. } = s { *rounds = rounds.saturating_sub(1).max(1) }),
                on_more: move |_| update(&|s| if let ChallengeSetup::Reaction { rounds, .. } = s { *rounds = (*rounds + 1).min(5) }),
            }
        },
        ChallengeSetup::Scan { code } => rsx! {
            label { class: "field-label",
                {t(&translations, "challenge.config.scan_value")}
                input {
                    class: "field",
                    r#type: "text",
                    placeholder: t(&translations, "challenge.config.scan_placeholder"),
                    value: "{code}",
                    oninput: move |evt| {
                        let text = evt.value();
                        update(&|s| if let ChallengeSetup::Scan { code } = s { code.clone_from(&text) });
                    },
                }
            }
            button {
                class: "btn btn-quiet",
                onclick: move |_| {
                    spawn(async move {
                        let (tx, rx) = futures_channel::oneshot::channel::<String>();
                        std::thread::spawn(move || {
                            let _ = tx.send(mobile_sentinel::scanner::scan().unwrap_or_default());
                        });
                        if let Ok(value) = rx.await {
                            if !value.is_empty() {
                                if let Some(ChallengeSetup::Scan { code }) = setups.write().get_mut(index) {
                                    *code = value;
                                }
                            }
                        }
                    });
                },
                {t(&translations, "challenge.config.scan_now")}
            }
        },
    };
    rsx! {
        Sheet {
            title: name,
            note: description,
            on_close: move |_| sheet.set(Some(EditorSheet::Challenges)),
            {controls}
            button {
                class: "btn btn-quiet",
                onclick: move |_| {
                    request_challenge_permission(&to_try);
                    trying.set(Some(vec![to_try.clone()]));
                },
                {t(&translations, "challenge.config.test")}
            }
            button {
                class: "btn btn-quiet danger",
                onclick: move |_| {
                    setups.write().remove(index);
                    sheet.set(Some(EditorSheet::Challenges));
                },
                {t(&translations, "config.remove_from_alarm")}
            }
            SheetDone { label: done_text.clone(), on_click: move |_| sheet.set(Some(EditorSheet::Challenges)) }
        }
    }
}
