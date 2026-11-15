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
