use std::time::Duration;

use crate::app::{t, Route, Translations};
use crate::components::{Icon, IconName};
use crate::models::{AggregateMetrics, AlarmEvent, ChallengeType, Difficulty, HistoryFilter};
use dioxus::prelude::*;

const FILTER_KEYS: &[&str] = &[
    "filter.this_week",
    "filter.last_week",
    "filter.this_month",
    "filter.last_month",
    "filter.this_year",
    "filter.last_year",
];

fn key_to_filter(key: &str, custom_days: u16) -> HistoryFilter {
    match key {
        "filter.this_week" => HistoryFilter::ThisWeek,
        "filter.last_week" => HistoryFilter::LastWeek,
        "filter.this_month" => HistoryFilter::ThisMonth,
        "filter.last_month" => HistoryFilter::LastMonth,
        "filter.this_year" => HistoryFilter::ThisYear,
        "filter.last_year" => HistoryFilter::LastYear,
        "custom" => HistoryFilter::LastDays(custom_days as u32),
        _ => HistoryFilter::ThisWeek,
    }
}

/// Format a Duration as "Xm YYs" or "Xs" or "Xms" depending on size.
fn format_duration(d: Duration) -> String {
    let secs = d.as_secs();
    if secs >= 60 {
        format!("{}m {:02}s", secs / 60, secs % 60)
    } else if secs > 0 {
        format!("{}s", secs)
    } else {
        let ms = d.as_millis();
        if ms == 0 {
            "0s".to_string()
        } else {
            format!("{}ms", ms)
        }
    }
}

/// Display name (translated, short) for a challenge type.
fn challenge_type_name(translations: &Translations, t_kind: ChallengeType) -> String {
    let key = match t_kind {
        ChallengeType::Math => "challenge.tag.math",
        ChallengeType::MemoryGame => "challenge.tag.memory",
        ChallengeType::Typing => "challenge.tag.typing",
        ChallengeType::ShakeToWake => "challenge.tag.shake",
        ChallengeType::StepCount => "challenge.tag.steps",
        ChallengeType::Scan => "challenge.tag.scan",
        ChallengeType::HoldButton => "challenge.tag.hold",
        ChallengeType::Reaction => "challenge.tag.reaction",
    };
    t(translations, key)
}

/// Display name for difficulty (lowercase, translated).
fn difficulty_name(translations: &Translations, d: Difficulty) -> String {
    let key = match d {
        Difficulty::Easy => "difficulty.easy_lower",
        Difficulty::Medium => "difficulty.medium_lower",
        Difficulty::Hard => "difficulty.hard_lower",
        Difficulty::Extreme => "difficulty.extreme_lower",
    };
    t(translations, key)
}

/// Whether a challenge type's difficulty is meaningful (varies what the
/// challenge does). Shake / steps / scan / hold are config-only or
/// fixed-behavior challenges with no difficulty selector, so the difficulty
/// stored on a history row is ignored when rendering.
fn challenge_has_difficulty(t_kind: ChallengeType) -> bool {
    matches!(
        t_kind,
        ChallengeType::Math | ChallengeType::MemoryGame | ChallengeType::Typing
    )
}

#[component]
pub fn History() -> Element {
    let translations = use_context::<Translations>();
    let mut filter = use_signal(|| FILTER_KEYS[0].to_string());
    let mut custom_days = use_signal(|| 30u16);
    let mut show_delete_menu = use_signal(|| false);
    let mut refresh = use_signal(|| 0u32);

    let current_filter = key_to_filter(&filter.read(), *custom_days.read());
    let _ = *refresh.read();
    let events = crate::history_persistence::query_history(&current_filter);
    let metrics = crate::history_persistence::aggregate_history(&current_filter);

    let history_title = t(&translations, "history.title");
    let no_events = t(&translations, "history.no_events");
    let events_subtitle = t(&translations, "history.events_subtitle");
    let nav_alarms = t(&translations, "nav.alarms");
    let nav_history = t(&translations, "nav.history");
    let nav_settings = t(&translations, "nav.settings");
    let delete_history_text = t(&translations, "history.delete_history");
    let delete_history_aria = t(&translations, "history.delete_history_aria");
    let delete_all_text = t(&translations, "history.delete_all");
    let delete_30_text = t(&translations, "history.delete_30");
    let delete_90_text = t(&translations, "history.delete_90");
    let cancel_text = t(&translations, "action.cancel");
    let custom_days_text = t(&translations, "filter.custom_days");
    let days_placeholder = t(&translations, "filter.days_placeholder");
    let days_unit = t(&translations, "filter.days_unit");

    let filter_labels: Vec<String> = FILTER_KEYS.iter().map(|k| t(&translations, k)).collect();

    rsx! {
        div { class: "screen history-screen",
            div { class: "history-header",
                h2 { "{history_title}" }
                button {
                    class: "header-icon-btn delete-icon",
                    "aria-label": "{delete_history_aria}",
                    onclick: move |_| { show_delete_menu.set(true); },
                    Icon { name: IconName::Trash }
                }
            }

            // Filter selector
            div { class: "filter-dropdown-container",
                select {
                    class: "filter-dropdown",
                    value: "{filter}",
                    onchange: move |evt| { filter.set(evt.value()); },
                    for (i, fl) in filter_labels.iter().enumerate() {
                        {
                            let key = FILTER_KEYS[i].to_string();
                            let label = fl.clone();
                            rsx! { option { value: "{key}", "{label}" } }
                        }
                    }
                    option { value: "custom", "{custom_days_text}" }
                }
                if *filter.read() == "custom" {
                    div { class: "custom-days-row",
                        input {
                            r#type: "number",
                            class: "custom-days-input",
                            min: "1",
                            max: "365",
                            placeholder: "{days_placeholder}",
                            value: "{custom_days}",
                            onchange: move |evt| {
                                if let Ok(v) = evt.value().parse::<u16>() {
                                    custom_days.set(v.clamp(1, 365));
                                }
                            },
                        }
                        span { class: "custom-days-label", "{days_unit}" }
                    }
                }
            }

            // Aggregate / event content
            if events.is_empty() {
                div { class: "empty-state",
                    div { class: "empty-icon", Icon { name: IconName::ChartBar } }
                    p { class: "empty-title", "{no_events}" }
                    p { class: "empty-subtitle", "{events_subtitle}" }
                }
            } else {
                div { class: "history-content",
                    // Top: aggregate summary card
                    AggregateCard { metrics: metrics.clone() }

                    // Per-event cards
                    div { class: "event-list",
                        for event in events.iter() {
                            EventCard { key: "{event.id}", event: event.clone() }
                        }
                    }
                }
            }

            nav { class: "bottom-nav",
                Link { to: Route::Home {}, class: "nav-item",
                    Icon { name: IconName::Clock }
                    " {nav_alarms}"
                }
                Link { to: Route::History {}, class: "nav-item active",
                    Icon { name: IconName::ChartBar }
                    " {nav_history}"
                }
                Link { to: Route::Settings {}, class: "nav-item",
                    Icon { name: IconName::Gear }
                    " {nav_settings}"
                }
            }
        }

        // Delete bottom sheet
        if *show_delete_menu.read() {
            div {
                class: "bottom-sheet-overlay",
                onclick: move |_| { show_delete_menu.set(false); },
                div {
                    class: "bottom-sheet",
                    onclick: move |evt| { evt.stop_propagation(); },
                    h3 { "{delete_history_text}" }
                    button {
                        class: "sheet-option destructive",
                        onclick: move |_| {
                            crate::history_persistence::delete_history(None);
                            refresh.set(refresh() + 1);
                            show_delete_menu.set(false);
                        },
                        "{delete_all_text}"
                    }
                    button {
                        class: "sheet-option",
                        onclick: move |_| {
                            crate::history_persistence::delete_history(Some(30));
                            refresh.set(refresh() + 1);
                            show_delete_menu.set(false);
                        },
                        "{delete_30_text}"
                    }
                    button {
                        class: "sheet-option",
                        onclick: move |_| {
                            crate::history_persistence::delete_history(Some(90));
                            refresh.set(refresh() + 1);
                            show_delete_menu.set(false);
                        },
                        "{delete_90_text}"
                    }
                    button {
                        class: "sheet-option cancel",
                        onclick: move |_| { show_delete_menu.set(false); },
                        "{cancel_text}"
                    }
                }
            }
        }
    }
}

/// Top-of-list summary card showing averages across the filtered period.
#[component]
fn AggregateCard(metrics: AggregateMetrics) -> Element {
    if metrics.total_events == 0 {
        return rsx! { div {} };
    }

    let translations = use_context::<Translations>();
    let total_events = metrics.total_events;
    let avg_reaction = format_duration(metrics.avg_reaction_time);
    let avg_dismiss = format_duration(metrics.avg_total_dismissal_time);
    let avg_challenge = format_duration(metrics.avg_total_challenge_time);
    let avg_snooze_count = format!("{:.1}", metrics.avg_snooze_count);
    let avg_snooze_interval = t(&translations, "history.minutes_short").replace(
        "{m}",
        &format!("{:.1}", metrics.avg_snooze_interval_minutes),
    );
    let avg_total_snoozed = format_duration(metrics.avg_total_snoozed);

    let title = t(&translations, "history.period_summary");
    let subtitle =
        t(&translations, "history.alarms_count").replace("{count}", &total_events.to_string());
    let label_avg_reaction = t(&translations, "history.avg_reaction");
    let label_avg_total = t(&translations, "history.avg_total");
    let label_avg_challenge = t(&translations, "history.avg_challenge");
    let label_avg_snooze_count = t(&translations, "history.avg_snooze_count");
    let label_avg_snooze_gap = t(&translations, "history.avg_snooze_gap");
    let label_avg_time_snoozed = t(&translations, "history.avg_time_snoozed");
    let label_avg_solve = t(&translations, "history.avg_solve_per_challenge");

    rsx! {
        div { class: "history-card aggregate-card",
            div { class: "history-card-header",
                div {
                    p { class: "history-card-title", "{title}" }
                    p { class: "history-card-subtitle", "{subtitle}" }
                }
                Icon { name: IconName::ChartBar, size: 24 }
            }

            div { class: "history-stat-grid",
                div { class: "history-stat",
                    span { class: "history-stat-value", "{avg_reaction}" }
                    span { class: "history-stat-label", "{label_avg_reaction}" }
                }
                div { class: "history-stat",
                    span { class: "history-stat-value", "{avg_dismiss}" }
                    span { class: "history-stat-label", "{label_avg_total}" }
                }
                div { class: "history-stat",
                    span { class: "history-stat-value", "{avg_challenge}" }
                    span { class: "history-stat-label", "{label_avg_challenge}" }
                }
                div { class: "history-stat",
                    span { class: "history-stat-value", "{avg_snooze_count}" }
                    span { class: "history-stat-label", "{label_avg_snooze_count}" }
                }
                div { class: "history-stat",
                    span { class: "history-stat-value", "{avg_snooze_interval}" }
                    span { class: "history-stat-label", "{label_avg_snooze_gap}" }
                }
                div { class: "history-stat",
                    span { class: "history-stat-value", "{avg_total_snoozed}" }
                    span { class: "history-stat-label", "{label_avg_time_snoozed}" }
                }
            }

            // Per-challenge averages
            if !metrics.avg_solve_per_challenge.is_empty() {
                div { class: "history-section-divider" }
                p { class: "history-section-title", "{label_avg_solve}" }
                div { class: "history-challenge-list",
                    for ((ctype, diff), avg) in metrics.avg_solve_per_challenge.iter() {
                        {
                            let name = challenge_type_name(&translations, *ctype);
                            let show_diff = challenge_has_difficulty(*ctype);
                            let diff_name = difficulty_name(&translations, *diff);
                            let dur = format_duration(*avg);
                            rsx! {
                                div { class: "history-challenge-row",
                                    span { class: "history-challenge-name",
                                        "{name}"
                                        if show_diff {
                                            " "
                                            span { class: "history-challenge-difficulty", "{diff_name}" }
                                        }
                                    }
                                    span { class: "history-challenge-time", "{dur}" }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Single alarm event card — always shows the full breakdown.
#[component]
fn EventCard(event: AlarmEvent) -> Element {
    let translations = use_context::<Translations>();
    let local_fire: chrono::DateTime<chrono::Local> =
        event.fire_timestamp.with_timezone(&chrono::Local);
    let time_str = local_fire.format("%H:%M").to_string();
    let date_str = local_fire.format("%b %d").to_string();
    let label = event.alarm_label.clone();

    let dismiss_str = format_duration(event.total_dismissal_time);
    let reaction_str = event
        .reaction_time()
        .map(format_duration)
        .unwrap_or_else(|| "—".to_string());
    let snooze_count = event.snooze_count;
    let snooze_total = format_duration(event.total_snoozed());
    let challenge_total = format_duration(event.total_challenge_time());
    let challenges_count = event.challenge_solves.len();

    let snooze_pill_key = if snooze_count == 1 {
        "history.snooze_pill_one"
    } else {
        "history.snooze_pill_other"
    };
    let snooze_pill =
        t(&translations, snooze_pill_key).replace("{count}", &snooze_count.to_string());

    let label_reaction = t(&translations, "history.reaction");
    let label_challenges = t(&translations, "history.challenges");
    let label_snoozed = t(&translations, "history.snoozed");
    let title_snooze_intervals = t(&translations, "history.snooze_intervals");
    let title_challenges_solved = t(&translations, "history.challenges_solved")
        .replace("{count}", &challenges_count.to_string());

    rsx! {
        div { class: "history-card event-card",
            div { class: "event-card-header",
                div {
                    div { class: "event-card-time-row",
                        span { class: "event-card-time", "{time_str}" }
                        span { class: "event-card-date", "{date_str}" }
                    }
                    if !label.is_empty() {
                        p { class: "event-card-label", "{label}" }
                    }
                }
                div { class: "event-card-summary",
                    span { class: "event-card-total", "{dismiss_str}" }
                    if snooze_count > 0 {
                        span { class: "event-card-snooze-pill", "{snooze_pill}" }
                    }
                }
            }

            div { class: "event-card-body",
                div { class: "history-stat-grid",
                    div { class: "history-stat",
                        span { class: "history-stat-value", "{reaction_str}" }
                        span { class: "history-stat-label", "{label_reaction}" }
                    }
                    div { class: "history-stat",
                        span { class: "history-stat-value", "{challenge_total}" }
                        span { class: "history-stat-label", "{label_challenges}" }
                    }
                    if snooze_count > 0 {
                        div { class: "history-stat",
                            span { class: "history-stat-value", "{snooze_total}" }
                            span { class: "history-stat-label", "{label_snoozed}" }
                        }
                    }
                }

                if !event.snooze_intervals_minutes.is_empty() {
                    div { class: "history-section-divider" }
                    p { class: "history-section-title", "{title_snooze_intervals}" }
                    div { class: "history-snooze-list",
                        for (i, mins) in event.snooze_intervals_minutes.iter().enumerate() {
                            {
                                let n = i + 1;
                                let m = *mins;
                                let row_label = t(&translations, "history.snooze_n")
                                    .replace("{n}", &n.to_string());
                                let row_time = t(&translations, "history.minutes_short")
                                    .replace("{m}", &m.to_string());
                                rsx! {
                                    div { class: "history-snooze-row",
                                        span { class: "history-snooze-num", "{row_label}" }
                                        span { class: "history-snooze-time", "{row_time}" }
                                    }
                                }
                            }
                        }
                    }
                }

                if challenges_count > 0 {
                    div { class: "history-section-divider" }
                    p { class: "history-section-title", "{title_challenges_solved}" }
                    div { class: "history-challenge-list",
                        for solve in event.challenge_solves.iter() {
                            {
                                let name = challenge_type_name(&translations, solve.challenge_type);
                                let show_diff = challenge_has_difficulty(solve.challenge_type);
                                let diff = difficulty_name(&translations, solve.difficulty);
                                let dur = format_duration(solve.duration);
                                rsx! {
                                    div { class: "history-challenge-row",
                                        span { class: "history-challenge-name",
                                            "{name}"
                                            if show_diff {
                                                " "
                                                span { class: "history-challenge-difficulty", "{diff}" }
                                            }
                                        }
                                        span { class: "history-challenge-time", "{dur}" }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
