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
