//! A word being composed by an input method (the preedit text) is not in `TextInput.text` yet, so
//! a placeholder that only looks at `text` is drawn under it. The testing backend can inject the
//! composition events the way an Android keyboard sends them.

use i_slint_core::input::{InternalKeyEvent, KeyEventType};
use slint::platform::WindowEvent;
use slint::ComponentHandle;

slint::slint! {
    import { TextField } from "../../ui/widgets.slint";
    export component FieldWindow inherits Window {
        width: 300px;
        height: 100px;
        out property <bool> shown: field.placeholder-shown;
        field := TextField { placeholder: "Naam van de wekker"; focus-on-show: true; }
    }
}

fn placeholder_shown(app: &FieldWindow) -> bool {
    app.get_shown()
}

fn compose(app: &FieldWindow, event_type: KeyEventType, text: &str) {
    app.window()
        .dispatch_event(WindowEvent::internal(InternalKeyEvent {
            event_type,
            preedit_text: text.into(),
            ..Default::default()
        }));
}

#[test]
fn the_placeholder_goes_while_a_word_is_being_composed() {
    i_slint_backend_testing::init_no_event_loop();
    let app = FieldWindow::new().unwrap();
    app.show().unwrap();
    assert!(
        placeholder_shown(&app),
        "an empty field shows its placeholder"
    );
    compose(&app, KeyEventType::UpdateComposition, "Werk");
    assert!(
        !placeholder_shown(&app),
        "the placeholder is drawn under the word being composed"
    );
    compose(&app, KeyEventType::CommitComposition, "Werk");
    assert!(!placeholder_shown(&app), "a committed word keeps it away");
}
