//! What a screen reader holds on to: its focus is a node of the accessibility tree, and the tree
//! builds a node's id from the item that is drawn. A list that is redrawn from a new model
//! destroys and creates every item, so the focus is lost. These tests refresh the screens with
//! unchanged data and look at which elements survive.

use std::rc::Rc;

use i_slint_backend_testing::{ElementHandle, ElementQuery};

use super::*;

fn window_with(alarms: Vec<AlarmData>) -> (MainWindow, Rc<Ctx>) {
    i_slint_backend_testing::init_no_event_loop();
    let ui = MainWindow::new().unwrap();
    ui.window().set_size(slint::PhysicalSize::new(400, 800));
    let ctx = new_ctx(&ui, AppSettings::default(), alarms);
    let weak = Rc::downgrade(&ctx);
    ui.global::<Tr>().on_lookup(move |key, _| {
        weak.upgrade()
            .map(|ctx| i18n::t(&ctx.tr.borrow(), key.as_str()))
            .unwrap_or_else(|| key.to_string())
            .into()
    });
    ui.show().unwrap();
    (ui, ctx)
}

fn alarm(id: &str, hour: u8, label: &str) -> AlarmData {
    let mut alarm = crate::alarm_ops::factory_alarm();
    alarm.id = id.into();
    alarm.time_hour = hour;
    alarm.label = label.into();
    alarm
}

fn cards(ui: &MainWindow) -> Vec<ElementHandle> {
    ElementQuery::from_root(ui)
        .match_descendants()
        .match_accessible_role(i_slint_backend_testing::AccessibleRole::ListItem)
        .match_predicate(|e| {
            e.accessible_label()
                .is_some_and(|l| l.contains("Work") || l.contains("Gym") || l.contains("Run"))
        })
        .find_all()
}

fn buttons(ui: &MainWindow) -> Vec<ElementHandle> {
    ElementQuery::from_root(ui)
        .match_descendants()
        .match_accessible_role(i_slint_backend_testing::AccessibleRole::Button)
        .find_all()
}

fn chips(ui: &MainWindow) -> Vec<ElementHandle> {
    buttons(ui)
        .into_iter()
        .filter(|b| b.accessible_label().is_some_and(|l| l.starts_with('+')))
        .collect()
}

fn labels(elements: &[ElementHandle]) -> Vec<String> {
    elements
        .iter()
        .map(|e| e.accessible_label().unwrap_or_default().to_string())
        .collect()
}

#[test]
fn a_refresh_with_the_same_alarms_keeps_the_cards_and_chips() {
    let (ui, ctx) = window_with(vec![alarm("a", 7, "Work"), alarm("b", 9, "Gym")]);
    home::refresh(&ctx);
    let old_cards = cards(&ui);
    assert!(!old_cards.is_empty(), "the alarm cards are there");
    let old_chips = chips(&ui);
    assert!(!old_chips.is_empty(), "the quick chips are there");

    home::refresh(&ctx);

    // A repeater that is filled again leaves its destroyed items behind in the tree for a while,
    // so they are counted: a refresh that changes nothing must not create a single item.
    assert_eq!(old_cards.len(), cards(&ui).len());
    assert_eq!(old_chips.len(), chips(&ui).len());
    assert!(old_cards
        .iter()
        .chain(old_chips.iter())
        .all(|e| e.is_valid()));
}

#[test]
fn a_refresh_updates_the_card_that_changed_in_place() {
    let (ui, ctx) = window_with(vec![alarm("a", 7, "Work"), alarm("b", 9, "Gym")]);
    home::refresh(&ctx);
    let before = cards(&ui);
    assert!(labels(&before).iter().any(|l| l.contains("Gym")));

    ctx.alarms.borrow_mut()[1].label = "Run".into();
    home::refresh(&ctx);

    assert!(before.iter().all(|c| c.is_valid()));
    let after = labels(&before);
    assert!(after.iter().any(|l| l.contains("Run")), "{after:?}");
    assert!(!after.iter().any(|l| l.contains("Gym")), "{after:?}");

    ctx.alarms.borrow_mut().pop();
    home::refresh(&ctx);
    let first = before
        .iter()
        .find(|c| c.accessible_label().is_some_and(|l| l.contains("Work")))
        .unwrap();
    assert!(
        first.is_valid(),
        "removing the last alarm keeps the first card"
    );
}

#[test]
fn a_card_is_one_node_with_the_time_name_and_days() {
    let (ui, ctx) = window_with(vec![alarm("a", 7, "Work")]);
    home::refresh(&ctx);
    let card = cards(&ui).into_iter().next().unwrap();
    assert_eq!(card.accessible_label().unwrap(), "07:00, Work, Weekdays");

    let texts = card
        .query_descendants()
        .match_accessible_role(i_slint_backend_testing::AccessibleRole::Text)
        .find_all();
    assert_eq!(
        labels(&texts),
        Vec::<String>::new(),
        "the digits, the name and the day letters are not nodes of their own"
    );

    ctx.alarms.borrow_mut()[0].days = [true, false, true, false, false, false, false];
    ctx.alarms.borrow_mut()[0].label.clear();
    home::refresh(&ctx);
    assert_eq!(card.accessible_label().unwrap(), "07:00, Monday, Wednesday");
}

fn named(ui: &MainWindow, label: &str) -> Vec<ElementHandle> {
    ElementHandle::find_by_accessible_label(ui, label).collect()
}

fn all_labels(ui: &MainWindow) -> Vec<String> {
    labels(
        &ElementQuery::from_root(ui)
            .match_descendants()
            .match_predicate(|e| e.accessible_label().is_some())
            .find_all(),
    )
}

fn open_editor(ctx: &Rc<Ctx>) {
    home::create(ctx);
}

fn open_sheet(ui: &MainWindow, ctx: &Rc<Ctx>, sheet: EditorSheet) {
    ui.set_ed_sheet(sheet);
    editor::sheet_opened(ctx, sheet);
    editor::sync(ctx);
}

#[test]
fn no_label_starts_or_ends_with_a_space() {
    let (ui, ctx) = window_with(vec![alarm("a", 7, "Work"), alarm("b", 9, "")]);
    home::refresh(&ctx);
    let mut seen = all_labels(&ui);
    settings::enter(&ctx);
    seen.extend(all_labels(&ui));
    history::enter(&ctx);
    seen.extend(all_labels(&ui));
    ui.set_screen(0);
    open_editor(&ctx);
    seen.extend(all_labels(&ui));
    for sheet in [
        EditorSheet::Label,
        EditorSheet::Sound,
        EditorSheet::RingLook,
    ] {
        open_sheet(&ui, &ctx, sheet);
        seen.extend(all_labels(&ui));
    }
    let bad: Vec<_> = seen.iter().filter(|l| l.trim() != l.as_str()).collect();
    assert!(bad.is_empty(), "{bad:?}");
    assert!(seen.iter().any(|l| l == "Add widget") || seen.iter().any(|l| l == "New alarms"));
}

#[test]
fn the_day_buttons_of_the_editor_say_the_whole_day() {
    let (ui, ctx) = window_with(vec![]);
    open_editor(&ctx);
    for day in [
        "Monday",
        "Tuesday",
        "Wednesday",
        "Thursday",
        "Friday",
        "Saturday",
        "Sunday",
    ] {
        let found = named(&ui, day);
        assert!(!found.is_empty(), "no button says {day}");
        assert!(found.iter().any(|e| e.accessible_checkable() == Some(true)));
    }
    assert!(named(&ui, "Mon").is_empty());
}

#[test]
fn the_time_button_and_the_chips_have_no_second_node_for_their_text() {
    let (ui, ctx) = window_with(vec![alarm("a", 7, "Work")]);
    home::refresh(&ctx);
    let texts_of_chips = named(&ui, "+5")
        .into_iter()
        .filter(|e| e.accessible_role() == Some(i_slint_backend_testing::AccessibleRole::Text))
        .count();
    assert_eq!(texts_of_chips, 0, "a chip is one node");
    open_editor(&ctx);
    let time: Vec<_> = named(&ui, "Time: 07:00");
    assert!(!time.is_empty());
    let inner = time[0]
        .query_descendants()
        .match_accessible_role(i_slint_backend_testing::AccessibleRole::Text)
        .find_all();
    assert!(inner.is_empty(), "{:?}", labels(&inner));
}

#[test]
fn the_note_field_is_named_and_its_placeholder_is_only_a_hint() {
    let (ui, ctx) = window_with(vec![]);
    open_editor(&ctx);
    open_sheet(&ui, &ctx, EditorSheet::Label);
    let note = named(&ui, "A note for when it rings");
    let field = note
        .iter()
        .find(|e| e.accessible_role() == Some(i_slint_backend_testing::AccessibleRole::TextInput))
        .expect("the note field is named for what it is");
    assert_eq!(
        field.accessible_placeholder_text().as_deref(),
        Some("Take the laptop charger")
    );
    assert!(named(&ui, "Take the laptop charger").is_empty());
}

#[test]
fn the_preview_button_says_what_it_does() {
    let (ui, ctx) = window_with(vec![]);
    open_editor(&ctx);
    open_sheet(&ui, &ctx, EditorSheet::Sound);
    assert!(!named(&ui, "Play preview").is_empty());
    assert!(named(&ui, "Start").is_empty());
}

#[test]
fn look_tiles_and_history_periods_are_buttons_with_a_state() {
    let (ui, ctx) = window_with(vec![]);
    open_editor(&ctx);
    open_sheet(&ui, &ctx, EditorSheet::RingLook);
    let plain = named(&ui, "Plain");
    assert!(
        plain.iter().any(|e| e.accessible_role()
            == Some(i_slint_backend_testing::AccessibleRole::Button)
            && e.accessible_checkable() == Some(true)),
        "the Plain tile is a checkable button"
    );
    ui.set_ed_sheet(EditorSheet::None);
    ui.set_screen(0);
    history::enter(&ctx);
    let period = ui.get_h_period() as usize;
    let en = i18n::load_translations(crate::models::Locale::English);
    let first = named(&ui, &i18n::t(&en, crate::history_view::PERIODS[period]));
    assert!(first.iter().any(|e| e.accessible_checked() == Some(true)));
}
