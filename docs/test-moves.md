# Device scenarios moved to `cargo test`

Scenarios that only look at the window and what it saves run headless on Slint's testing backend
(`src/ui/flow_tests.rs`, `src/ui/panel_tests.rs`) with the app's clock fixed (`src/clock.rs`: a
Wednesday at ten, so no test depends on the weekday or the minute). On the device they stay only
for what needs a real phone.

| Old scenario (devtest/matrix.sh) | New test | What stays on the device |
|---|---|---|
| s25 back navigation | `ui::flow_tests::s25_back_navigation` | Back on Home goes to the background, same process |
| s30 quick alarm and undo | `s30_quick_alarm_and_undo` (made, toast, Undo, saved state) | the OS alarm is armed (s30 unchanged) |
| s31 pause all | `s31_pause_all` (pause past a week, Undo) | armed time moves in the OS (s31 unchanged) |
| s43 delete history | `s43_delete_history` | nothing (stub in matrix.sh) |
| s46 twelve hours | `s46_twelve_hours` | nothing (stub) |
| s47 use for new alarms | `s47_use_for_new_alarms` | nothing (stub) |
| s57 every panel | `ui::panel_tests` (Part A) | short walk (and walk.sh) |
| s61 colours (themes, Sand) | `s61_colours_of_ones_own` | wheel by finger, code, own theme, restart (s61) |
| s65 large size fits | `s65_large_size_fits` (two looks, home and editor) | one look on a real window (s65) |
| s58 try sound/vibration: durations | `editor_sound::tests::trying_a_sound_lasts_5_s_and_15_s...` | real sound and vibration with shortened times |
| schedule math (new) | `tests/schedule_props.rs` (proptest) | |

A scenario left as a name in matrix.sh (`moved_to_cargo`) reports SKIP with the test that replaced it.
