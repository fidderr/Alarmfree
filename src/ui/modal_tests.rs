//! A sheet covers the screen, so a screen reader must not reach what is behind it (`accessible-modal`):
//! with a sheet open the home screen's buttons are hidden in the accessibility tree, and back after it closes.

use std::cell::Cell;
use std::rc::Rc;

use accesskit::{Node, TreeUpdate};
use i_slint_core::accessibility::accesskit_tree::{AccessKitHost, AccessKitTree};
use i_slint_core::window::WindowInner;

use super::a11y_tests::{alarm, window_with};
use super::*;

#[derive(Default)]
struct Host {
    dirty: Cell<u32>,
}

impl AccessKitHost for Host {
    fn window_has_focus(&self) -> bool {
        true
    }
    fn nodes_dirty(&self) {
        self.dirty.set(self.dirty.get() + 1);
    }
    fn focus_dirty(&self) {}
}

fn node<'a>(update: &'a TreeUpdate, label: &str) -> &'a Node {
    update
        .nodes
        .iter()
        .find(|(_, n)| n.label() == Some(label))
        .map(|(_, n)| n)
        .unwrap_or_else(|| panic!("no node labelled {label:?}"))
}

#[test]
fn an_open_sheet_hides_the_screen_behind_it() {
    let (ui, ctx) = window_with(vec![alarm("a", 7, "Work")]);
    home::refresh(&ctx);
    let adapter = WindowInner::from_pub(ui.window()).window_adapter();
    let mut tree = AccessKitTree::new(Rc::downgrade(&adapter), Rc::new(Host::default()));

    let update = tree.build_new_tree();
    assert!(!node(&update, "+5").is_hidden(), "the quick chip is there");
    assert!(!node(&update, "Sort alarms").is_hidden());

    ui.set_home_sheet(HomeSheet::Sort);
    let update = tree.build_new_tree();
    assert!(
        node(&update, "+5").is_hidden(),
        "the chip behind the sheet is hidden"
    );
    assert!(node(&update, "Sort alarms").is_hidden());

    ui.set_home_sheet(HomeSheet::None);
    let update = tree.build_new_tree();
    assert!(
        !node(&update, "+5").is_hidden(),
        "back after the sheet closes"
    );
    assert!(!node(&update, "Sort alarms").is_hidden());
}

#[test]
fn the_toast_is_a_live_region_with_an_undo_button() {
    use accesskit::{Action, ActionRequest, Live, Role, TreeId};

    let (ui, _ctx) = window_with(vec![alarm("a", 7, "Work")]);
    let adapter = WindowInner::from_pub(ui.window()).window_adapter();
    let mut tree = AccessKitTree::new(Rc::downgrade(&adapter), Rc::new(Host::default()));
    let undone = Rc::new(Cell::new(0));
    let seen = undone.clone();
    ui.on_toast_undo(move || seen.set(seen.get() + 1));

    // Always in the tree, but with nothing to say: the live region changes when the message comes.
    let update = tree.build_new_tree();
    assert!(
        !update.nodes.iter().any(|(_, n)| n.label() == Some("Undo")),
        "no Undo while nothing is said"
    );

    ui.set_toast_text("Quick alarm set for 30 min".into());
    ui.set_toast_has_undo(true);
    ui.set_toast_visible(true);
    let update = tree.build_new_tree();
    let message = update
        .nodes
        .iter()
        .map(|(_, n)| n)
        .find(|n| {
            n.label() == Some("Quick alarm set for 30 min")
                || n.value() == Some("Quick alarm set for 30 min")
        })
        .unwrap_or_else(|| panic!("the message is a node: {:#?}", update.nodes));
    assert_eq!(message.live(), Some(Live::Polite));
    let (undo_id, undo) = update
        .nodes
        .iter()
        .find(|(_, n)| n.label() == Some("Undo"))
        .expect("Undo is a node");
    assert_eq!(undo.role(), Role::Button);

    tree.handle_request(ActionRequest {
        action: Action::Click,
        target_tree: TreeId::ROOT,
        target_node: *undo_id,
        data: None,
    })
    .expect("the click is understood")
    .invoke(ui.window());
    assert_eq!(undone.get(), 1, "its default action undoes");
}
