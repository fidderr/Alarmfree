# AlarmFree: Dioxus to Slint port inventory

> Status: AlarmFree's `main` branch is now the Slint app. The Dioxus (WebView) version, up to 0.4.1, is on branch
> `dioxus`. This file is the inventory the port was measured against; it is kept as history and as the list of what is
> still open (the checklist in section 9 and the "Known gaps" under Status). It names `C:/Git/alarmfree-slint`, the
> working copy the port was made in before it became `main`. For how to build and test today, see the README.

Feature checklist the Slint port (Slint 1.18.1, `.slint` + Rust glue) is measured against. Derived from the code in
`C:/Git/alarmfree-slint` (a copy of AlarmFree 0.4.x, Rust + Dioxus 0.7 in an Android WebView). Section 9 is the
checklist. Everything the Rust core already does (alarm_ops, schedule, ordering, persistence, db, models, platform,
custom_sounds, ring_photos, history_persistence, firing_state, i18n, text) stays; only `src/app.rs`, `src/screens/**`,
`src/components/**`, `src/back.rs`, `src/reorder.*`, `src/debug_hooks.rs` (script part) are replaced.

Conventions: "t(key)" = translation lookup from `src/i18n/{en,nl,es,fr,de}.json` (480 keys, missing key returns the key
itself). "css" = `assets/style.css` (2619 lines). All paths are relative to `src/` unless noted.

Docs mismatches found while reading (the code wins, port the code):
- README says "colorblind themes with five accent colours" and "challenges in your order or at random". The code has 9
  themes (no colorblind; a saved "Colorblind" falls back to System, `settings.rs` test) and 9 accents; no random challenge order exists.
- `Prefs.notify_before_minutes` and `alarm_ops::give_notice_to_all` exist but no screen uses them (the notice is per alarm only).
- Dioxus 0.7 and the WebView are gone in the port: `style.css`, `reorder.js`, `WINDOW_WATCH`, `document::eval` all go away.

---

## 1. Screens / routes

Routes (`app.rs` `Route`): `/` Home, `/alarm/:id` AlarmConfig (`id` = `new`, `defaults`, `copy-<id>` or an alarm id),
`/practice/:id` Practice, `/history`, `/settings`, `/theme`, `/reliability`, `/welcome` FirstRun. The ringing screen
(AlarmFiring) is NOT a route: `App` renders it instead of the Router while `firing_id` is `Some`.

Navigation semantics to keep:
- `nav.push` for Home to AlarmConfig, Practice, Reliability, ThemeScreen, AlarmConfig("defaults") from Settings.
- `nav.replace` between the three main tabs (Home / History / Settings) via BottomNav: no stacking.
- Back (system button, see 8.4): top of the back stack handler first (sheets, preview, practice), else leave screen:
  Home sends the app to the background (`foregrounding::move_to_back`); History and Settings `replace` to Home;
  other screens `go_back` if possible else replace Home. Ignored entirely while an alarm rings.
- First launch: if `!prefs.setup_done`, `NavWatcher` replaces to `/welcome` once (`use_hook`).
- `nav_intent`: after a ring is finished AlarmFiring sets the next route (History for normal alarms, Home for `hidden`
  ones) and `NavWatcher` applies it when the Router remounts.
- Wide layout: when window width >= 840 css px (`SPLIT_FROM_WIDTH`) and route is Home or AlarmConfig, show a two-pane
  split: left Home list in a fixed 400 px column with a 1 px right border, right pane = the routed screen; on `/` the
  right pane shows the centred text t("home.pick"). Editor is keyed by `id` (`AlarmConfig` wraps `Editor { key: id }`) so
  picking another alarm in the list restarts the editor with fresh state. Other screens: single column centred, max-width 560.

### 1.1 Home (`screens/home/mod.rs`, `card.rs`)
Layout: header, optional problem banner, quick row, scrolling list (bottom padding for FAB), FAB, BottomNav, sheet layer.
- Header: label t("home.next_label"); big title = "<day name> <clock>" of the earliest future fire among enabled
  non-hidden alarms (`schedule::earliest_future` over `AlarmKit::next_fire`), or t("home.none_on"); subtitle "in 7 h 12 min"
  (`text::time_left` + t("home.in")); nothing if none. Two round icon buttons: Pause (opens PauseAll sheet; style `on`
  when every listed alarm is paused, `alarm_ops::all_paused_until`), Sort (opens Sort sheet).
- Problem banner (`reliability::first_problem(hidden_warnings)`): warning icon + t("reliability.banner"){check} +
  t("reliability.fix"); whole banner taps to push `/reliability`; a small X hides that warning (appends the check name to
  `prefs.hidden_warnings`, saved) only for `can_hide` checks (Overlay, BatteryUnrestricted, KeptWhenUnused, BootSignal).
  Re-evaluated 1.5 s after mount, then every 15 s (also drives re-render of countdowns / day labels).
- Quick row: label t("home.quick") and 5 chips "+5 +10 +15 +30 +60" (QUICK_MINUTES). Tap creates a one-time nap alarm
  (`alarm_ops::quick_alarm`: seconds round up, `delete_after_ring=true`, wake check 0, notice 0, label t("home.quick_alarm"),
  user defaults applied, `order = order_for_new`), toast t("home.quick_set"){minutes} with Undo. At 50 alarms
  (`MAX_ALARMS`) shows error toast t("alarm.limit_reached") instead.
- Alarm list (`ul #alarm-list`, or `#alarm-list-sorted` when sort != Own so dragging is off). Rows from
  `ordering::sorted_for_display`, hidden alarms removed, then Own = stored order, Time = (hour, minute), NextRing =
  AlarmKit next fire (`i64::MAX` when none). One `AlarmCard` per alarm (below).
- Empty state (no listed alarms): clock icon in circle, t("alarm.no_alarms"), t("alarm.tap_to_create"), and two template
  buttons: "Weekdays 07:00" (Mon-Fri 07:00) and "Weekend 09:00" (Sat+Sun 09:00) (`home.template_weekdays/weekend`, time
  formatted per 12/24 h); each creates an alarm from the defaults, toast "Rings in..." (`text::rings_in`) with Undo.
- FAB (+, bottom right): at limit shows error toast, else `push /alarm/new`.
- BottomNav (Alarms / History / Settings).
- Reorder: long press (450 ms) a card, drag vertically, drop = reorder, saved (`ordering::apply_reorder`, positions mapped
  from shown list to full list incl. hidden). Only when sort == Own. See 8.2.
- Data: reads `alarms`, `settings.prefs.{sort, time_format, week_start, alarm_defaults, hidden_warnings}`,
  `platform::alarm_kit_optional().next_fire/snoozed_until`; writes `alarms` via `persistence::save_alarms`, `settings`
  via `save_settings`. Every list mutation goes through `HomeState::change` = save + toast with Undo (restores the whole
  previous list and saves it).

AlarmCard (one row):
- Card = horizontal scroll-snap strip: face (full width, min-height 76) + swipe actions (right). Swipe left reveals
  [Skip next / Undo skip] (only if enabled, not one-time, and (skip active or a next fire exists)) and [Delete] (danger
  red, 64 px wide each). Skip tap toggles `skip_until_ms` (next fire or None), toast with Undo, and the strip scrolls back.
  Delete tap removes the alarm, toast t("home.deleted"){alarm} / t("home.deleted_plain") with Undo.
- Face: time button (opens `/alarm/<id>`; also the long-press drag handle, class `hold-to-drag`), 24 px time (min-width 76;
  12 h variant `h12` min-width 104, 20 px) + info block: label (if set) and one line "when": priority = snoozed until
  (t("home.snoozed_until"){time}, only if enabled or one-time) > paused until date (t("home.paused_until")) > skip
  (t("home.skips"){day}) > for one-time alarms Today / Tomorrow / short date > otherwise seven day letters in week-start
  order with enabled days highlighted (only highlighted when alarm is on). Disabled repeating alarms show the letters
  dimmed (`.off`). Kebab (three dots) button opens the AlarmMenu sheet; Switch (role switch) toggles `enabled`: when
  turning on, `schedule::refresh_one_time_date` and toast `text::rings_in`; turning off toast t("home.switched_off"); both
  with Undo.
- `alarm-card off` styling when disabled (dimmed time).

### 1.2 AlarmConfig / Editor (`screens/alarm_config/mod.rs`)
Modes by `id`: `new` (defaults prefilled), `copy-<id>` (label becomes t("alarm.copy_label"){label}), existing id, `defaults`
(edits what new alarms start from: hides time + repeat sections, title t("config.defaults_title"), Save writes
`prefs.alarm_defaults` with empty id and goes back). Titles: t("alarm.create") / t("alarm.edit").
- Header: back arrow (calls `leave`), title, trash button (only for existing, non-defaults) opens Delete sheet.
- Time section: big time button (opens Time sheet, clears `typed_time`) flanked by step buttons -1/-5 (left) and +1/+5
  (right) that wrap around 24 h; below, `rings-in` line (`rings_text`: t("config.rings_on"){day,time}, day = today /
  tomorrow / short date, from `schedule::next_ring_moment`; empty if none).
- Repeat section: seven day chips (aria-pressed, week-start order, letter label) toggling `days[d]`; presets chips
  Weekdays / Weekend / Every day (highlighted when `days` equals and not one-time; sets `one_time=false`); chip
  "One date" (`config.one_date`): switches to one-time, if the date is not ahead picks `schedule::next_occurrence`, and
  opens Date sheet. When one-time, the chips row is replaced by a primary button t("config.once_on"){date} that opens
  the Date sheet. Inline red error text under it (`validation.select_day`, `validation.date_past`).
- Two grouped lists of summary rows (each opens a sheet): Label (value = label or "None"), Sound (name, or
  "Vibration only"), Vibration (pattern name or "Off"), Snooze (counted summary "N x M min" or "No snooze"), Challenges
  (1-2 names joined, or "First +N", or "None"); then Ring look (backdrop name), Ring limit ("N min" / "Forever"), Wake
  check ("Off" / "after N min"), Notice before ("Off" / "N min"), plus a SwitchRow Max brightness.
- Save bar (sticky bottom): primary Save. Validation: one-time needs date+time in the future, repeating needs >=1 day.
  Builds the alarm (`build`): sanitized label and note (`sanitize::sanitize_label`), `days` cleared + `date` set if
  one-time, challenges via `ChallengeSetup::to_stored`, new id (uuid) for new/copy, `order = order_for_new` for new else
  original, `enabled = true`, `skip_until_ms = None` (pause kept). Existing: replace in list; new: push. Saves, toast
  `rings_in` (info), `go_back`.
- Unsaved-changes guard: `has_changes` compares built alarm to original (ignoring id/order/enabled/skip and the
  initial one-time date); back / header arrow / system Back with changes opens the Discard sheet, else leaves.
- State kept while temporary full-screen views replace the editor (editor state lives on): `trying` (challenge runner for
  "Try"), `previewing_look` (ring preview).
- Sound/vibration preview (`Preview`): at most one running; auto-stops after 15 s (`PREVIEW_MS`, token-guarded); stopped
  on sheet close, on save/leave, on Drop. Sound via `audio::play_as_alarm(path, loop=true)`; vibration via
  `haptics::vibrate_pattern(repeated_pattern(pattern, 15000))` (NOT through touch-feedback setting).
- Data: reads `alarms` (copy/edit source, used labels), `settings.prefs.{alarm_defaults, sound_names, week_start, time_format}`,
  `settings.min_volume_percent`; writes `alarms` or `prefs.alarm_defaults`.

### 1.3 Practice (`screens/practice.rs`)
Route `/practice/:id` = `ChallengeRunner` over the saved alarm's challenges; also reused inside the editor ("Try" and
"Try all"). Shows `ChallengeRun` in practice mode (header "Practice N/M", Exit link, no idle timer, no history, AlarmKit never
called). If the list is empty shows t("practice.empty") + Exit; after finishing shows t("practice.done") + Exit.
`on_exit` = go_back (route) or close (editor). Back = exit.

### 1.4 History (`screens/history.rs`)
- Header t("history.title") + trash icon (opens DeleteHistory sheet). Horizontal scrolling period pills:
  This week, Last week, This month, Last month, This year, Last year, Custom days (`PERIODS`); Custom shows an inline
  number field "Last [N] days" (1..365, default 30, clamped).
- Empty states: nothing ever recorded (t("history.no_events") + subtitle) vs nothing in period (t("history.none_in_period")).
- Summary card: three headline stats (Streak "N days", avg time to stop, avg snoozes with one decimal), bar chart
  (shown except for Custom days): 7 bars Mon.. by week-start order for a week, 5 bars (days 1, 8, 15, 22, 29) for a
  month, 12 month initials for a year; bar height = average dismissal seconds of Solved events scaled to max (56 px max,
  min 3 px, 0 if none); the current day/week/month bar is highlighted; toggle link "All stats / Fewer stats" expands a
  definition list: Alarms, avg reaction, avg total, avg time snoozed, avg snooze gap, avg challenge time, then per
  challenge-and-difficulty avg solve time.
- Streak = consecutive days (look back 400 days) in which every alarm was Solved/Backup; days without alarms neither
  break nor count.
- Event list (`ul`): row = "<short date> <time> <label>", sub line (outcome text for Missed (danger colour), Limit/Reboot
  (warn colour), Backup; else "snoozed once/N times" or challenge names), right value = total dismissal duration (only for
  Solved), chevron rotates when open. Tapping toggles a detail block (one open at a time): reaction time, each snooze
  length, each challenge solve (name + difficulty for Math/Memory/Typing) with duration, and a danger link "Delete" that
  opens a confirm sheet.
- Sheets: DeleteHistory (older than 30 days / 90 days / all) each followed by an AskDelete confirm sheet (title, note
  t("history.delete_all_note"), Delete danger + Cancel). Deleting toasts t("history.deleted") and refreshes.
- Data: `history_persistence::{query_history, aggregate_history, delete_event, delete_history}` with `HistoryFilter`
  (ThisWeek(week_start), LastWeek, ThisMonth, LastMonth, ThisYear, LastYear, LastDays(n)); `prefs.week_start`, 12/24 h.

### 1.5 Settings (`screens/settings/mod.rs`, `sheets.rs`)
Header t("settings.title") (large). Scroll content, then BottomNav:
- Reliability banner (tall): title + sub ("All fine" / "1 thing" / "N things", from `health::report(...).problems().len()`
  read once on mount), tinted warn if problems else `fine`; trailing "Check" text or chevron; pushes `/reliability`.
- General list: Theme (value = theme name; pushes `/theme`), Language (value), Time format (value), Week starts on (value),
  and "Add widget" row only when `home_widget::can_add()` (calls `home_widget::add()`).
- Alarms list: New alarm defaults (pushes `/alarm/defaults`), Minimum volume (value "Off"/"N%").
- Feel list: Touch feedback, Less motion, Confetti switch rows (see 3.1).
- Data list: Backup (sheet), Delete history (opens the same DeleteHistory sheet as History), About (value = app version
  `CARGO_PKG_VERSION`).
- Each change = `SettingsState::update`: write signal, `persistence::save_settings`, `platform::refresh_outside_app()`
  (re-renders widget/notification texts).

### 1.6 ThemeScreen (`screens/theme.rs`)
Screen (not a sheet) so the result is visible. Back handler = go back.
- Header with back, title t("settings.theme").
- Fixed live preview card (`aria-hidden`): a mock alarm row (07:00, label "Weekdays", Mon-Fri letters highlighted, on
  switch) plus a fake primary Save button and "Try" link.
- Theme grid: 9 tiles (System, Dark, Light, Black, Midnight, Forest, Plum, Sand, Mist): tile face = that theme's background
  with a card chip, text line and accent dot; System shows a diagonal 135 deg split of the dark and light backgrounds;
  selected tile ringed with accent. Tap sets `settings.theme` and saves.
- "Your own" row with rainbow dot (selected ring when `theme == Custom`): tapping seeds `prefs.custom_theme` from the
  current palette background (`own_colours`) and selects Custom. When on Custom, a list of three colour rows appears:
  Background, Cards (shows "Auto" when unset), Text ("Auto" when unset), each with colour dot + chevron opening the
  ColorSheet for that role.
- Accent section: nine swatches (shade for light/dark of the current background), selected ring; plus a 10th "wheel"
  swatch (conic gradient; centre dot shows the custom colour when active) opening ColorSheet(Accent).
- ColorSheet (role = Accent | Background | Card | Text): preset swatches (accent presets; or 10 background / 10 card / 10
  text presets in `theme.rs`), `ColorControls` (wheel, brightness slider, hex field, optional "Auto" chip for Card/Text),
  note t("theme.adjusted") when the shown colour differs from the chosen one (auto contrast fix), Done. Changes apply
  live to the whole app; saved on finger lift / field change / sheet close.
- Data: `settings.theme`, `prefs.{accent, custom_accent, custom_theme}`, `mobile_sentinel::display::system_dark()`.

### 1.7 Reliability (`screens/reliability.rs`)
Header with back + "?" help (intro + test note + "last ring took X.Y s" from AlarmKit timing log). List of 10 checks
(`CHECKS` order, failing ones sorted first): ExactAlarms, Notifications, FullScreen, Overlay, BatteryUnrestricted,
BackgroundAllowed, KeptWhenUnused, ChannelImportance, NotTotalSilence, BootSignal. Row: ok (check) / warn icon, title, note
(only when failing), "Fix" small button (not for BootSignal). Fix: Notifications = `permissions::request(POST_NOTIFICATIONS)`;
BatteryUnrestricted = `battery::request_exemption()`; others = `health::open_settings(check, FOREGROUND_CHANNEL_ID)`.
Report re-polled every 1.5 s so the list updates when the user returns from system settings. Second list: "App settings"
(`permissions::open_app_settings`), and "Save ring record" (only if AlarmKit timing log is non-empty; writes
`alarmfree-rings.txt` to Downloads, toast with place or failure). Sticky bottom bar: "Test alarm" primary (adds a hidden
one-time test alarm 60 s ahead via `alarm_ops::test_alarm`, toast t("reliability.test_set")); while one is pending shows
t("reliability.test_set") note + quiet "Cancel test" (removes alarms with `test`).
- FirstRun (`/welcome`): same list, but header is the large t("setup.title"), intro paragraph t("setup.intro"), no back
  arrow/help, and the bottom button is primary t("setup.done") which sets `prefs.setup_done = true` (saved) and replaces
  to Home. Back button does the same.

### 1.8 AlarmFiring (`screens/alarm_firing.rs`, `ring_face.rs`)
Shown instead of the router while `firing_id` is set (rendered with `key = alarm_id`). Pure view over AlarmKit state.
- `RingFace`: full-screen, padding top 56+safe, sides 20, bottom 24+safe; backdrop (5.2), date line ("Thursday 5 Nov"), big live
  clock (96 px, 64 px for 12 h; size divided by system font scale `--text-zoom`; style per `RingClock`), label (28 px bold,
  falls back to t("alarm.fallback_label")), optional note (16 px, text2), then children.
- Below (spacer pushes to bottom): Snooze button t("ring.snooze"){minutes,left} only while `snoozes_left > 0`; challenge
  hint t("ring.one_challenge") / t("ring.challenges"){count} plus one dot per challenge when >1 (or an empty 48 px gap when
  none); primary "Turn off" (t("ring.turn_off")).
- Snooze: decrements `snoozes_left`, `kit.snooze(id)`; if no other session is ringing (`kit.current_session()` is None)
  `foregrounding::finish_activity()`; else the screen moves to the next ringing alarm.
- Turn off: records `first_interaction` once; `prepare(setups, locale)` generates challenges; none = `kit.mark_solved` +
  `kit.dismiss` + finish; else `kit.pause(id)` (sound pauses) and show `ChallengeRun` (real mode).
- Real-mode `ChallengeRun`: `on_solved` = mark_solved + dismiss + finish; `on_idle` (60 s with no touch/sensor change)
  returns to the ring screen and `kit.resume(id)` (sound back); `on_long_scan(true/false)` while the camera has been open
  >= 180 s: resume / pause again.
- Finish: record history event (`history_persistence::record_alarm_event`, skipped when alarm `hidden`) with fire time from
  the first ring, reaction time to the last ring, snoozes, challenge solves, total time; `finish_alarm` (wake-check
  alarm creation via `alarm_ops::wake_check_alarm` if `wake_check_minutes > 0`; one-time alarms switch off or are removed
  by `schedule::finish_if_one_time`); then `nav_intent` = History (Home when hidden).
- Side effects: `kit.note_ui_up(id)` on mount; `display::set_max_brightness()` (re-applied every 5 s) and
  `restore_brightness()` on drop when the alarm has `max_brightness`; clock re-read every 5 s.
- Heartbeat: `App` sends `jobs::ui_heartbeat()` every second while firing only if a UI round trip succeeds within 3 s
  (see 8.5). The Slint port must keep an equivalent liveness signal (the event loop answering a posted closure).

### 1.9 ChallengeRun (`screens/challenge_run.rs`)
Shared by ring and practice. Top bar: progress t("challenge.progress"|"practice.progress"){current,total}; Exit link
(practice only); dots (when total > 1; filled up to current). Real mode shows a thin time-left bar (width = 100 - idle%,
1 s linear transition, role=progressbar). Body = the current challenge component (keyed per index). "Can't do this one?"
link (t("challenge.fallback.link"); hidden for Hold, during fallback, and during celebration) opens a Sheet
(fallback.title / explain; Cancel and confirm) that swaps the current challenge for 20 holds of 3000 ms (back arrow
cancels the fallback). Solving: records `ChallengeSolve { type (HoldButton if fallback), difficulty, duration }`, haptic
pattern [0,45,60,90], shows Celebration for 1800 ms (confetti 28 particles unless `prefs.confetti` off; label "i / total"
if > 1), then next challenge or `on_solved(solves)`. Idle clock: any click / input / touch resets to 0; sensor change
(shake or step count) also resets and calls `kit.note_activity()`; scanner open pauses the idle clock; 60 s idle fires
`on_idle` once. Debug override of idle/scan limits via `platform::shortened("idle_secs"|"scan_quiet_secs")`.

---

## 2. Sheets and dialogs

Base `Sheet` component (`components/ui.rs`): scrim (rgba 0,0,0,0.55, full screen, tap closes) + bottom panel (radius 28
top, max-height 90%, scrolls, max-width 560 centred, slide-up 0.18 s) with grab handle, title row (title, optional "?"
help toggle that expands an accent-soft help paragraph in place, spacer, optional top-right `action` slot), optional
`note` (always visible warning text), content, bottom padding for insets. Opening registers a Back handler (Back closes)
and bumps `OpenSheets` (toasts move to the top while any sheet is open). `SheetDone` = primary full-width button
(`quiet` = secondary). `SheetItem` = full-width choice row (selected tick style / danger red). `Chips<T>` single-select
(optional `grow` = equal width). `Stepper` = title, minus round button, value, plus round button. `GroupTitle` = small
heading. `StartStop` = top-right Start/Stop link with play/stop icon.

### 2.1 Home sheets (`home/sheets.rs`)
| Sheet | Purpose | Controls |
|---|---|---|
| Sort | list order, help t("home.sort_note") | 3 SheetItems: Own order, By time, By next ring; selected shown; saves `prefs.sort` and closes |
| PauseAll | holiday pause, help t("home.pause_note") | "End pause" (only when all paused: `resume_all`), "Until tomorrow" (start of day +1), "For a week" (start of day +7), "Until a date..." (opens PauseDate). Each toasts t("home.paused_message"){date} with Undo |
| PauseDate(None or alarm id) | pick pause end date, help t("home.pause_date_note") | native date input (min = tomorrow, default today+7), Apply button; must be > now; `alarm_ops::pause_all` or sets one alarm's `pause_until_ms` |
| AlarmMenu(id) | per-alarm actions, title = "<time> <label>" | Practice (only if alarm has challenges; pushes `/practice/<id>`), Skip next / Undo skip (same condition as swipe), Duplicate (limit check then `/alarm/copy-<id>`), Pause alarm / End pause (pause_active toggles; pause opens PauseDate(Some(id))), "Use as default for new alarms" (`prefs.alarm_defaults = as_defaults(alarm)`, info toast), Delete (danger, with Undo toast) |

### 2.2 Editor sheets (`alarm_config/*_sheets.rs`)
| Sheet | Purpose | Controls / options |
|---|---|---|
| Time (`TimePicker`) | set the time | two drum wheels (hours 0-23 or 12 h with AM/PM label, minutes 0-59) + numeric text field "type a time" accepting `7`, `730`, `07:30`, `7.30`, `7 30`, `7h30` (`parse_typed_time`; applies live when valid); Done |
| Date | one-time date | native date input (min today), "Quick" chips Today / Tomorrow / Next week (+7 d); help t("config.date_note"); Done |
| Label | name and ring note | Label text field (max 100), Note text field (max 100, shown while ringing), "Used before" chips of existing non-hidden labels (sorted, dedup); help; Done |
| Sound | pick sound, volume, fade | Start/Stop preview (top right, hidden when vibration-only), radio "No sound" (vibration only, `sound_mode=2`), radio list of 10 bundled tones then custom sounds (selection sets `sound`; if `sound_mode==2` it becomes 0; if a preview is playing it switches to the new sound); custom rows have rename (inline field max 60 + check; empty name removes the alias) and delete (X, then inline red "Delete" + keep X confirm; deleting replaces the sound in the draft, all alarms, and defaults by `factory_alarm().sound`, removes alias, deletes file); "Import sound" button (file picker thread `custom_sounds::pick_audio_file_native`; error toast t("sound.not_a_sound") or db.save_failed); volume slider 0-100 step 5 with % readout (writes `volume_percent`; default shown = global minimum); "Fade in" chips Off/15 s/30 s/1 min/2 min/5 min (`FADE_SECONDS` 0,15,30,60,120,300); Done |
| Vibration | pattern and delay | Start/Stop preview; chips Off / Pulse / Steady / Heartbeat (Off sets `sound_mode` 1 unless 2; a pattern sets `vibration` and moves `sound_mode` 1 to 0; choosing a pattern vibrates immediately); when `sound_mode==0`: "Sound starts" chips Together / 15 s / 30 s / 1 min after vibration (`sound_delay_secs` 0,15,30,60); Done |
| Snooze | count and length | Stepper length (1..60 min; by 1 up to 10, by 5 above: `longer_or_shorter`); Stepper count (0..10 times; 0 = no snooze); help |
| Challenges | list, order, try | top-right "Try all" (when >0); rows: drag handle (grip), number, name + summary (opens Detail), "Try" link, X remove; drag by handle reorders (`list_edit::move_item`); "Add challenge" button while < 10 (`MAX_CHALLENGES`); Done |
| AddChallenge | choose type | 8 options (name + short description: Math, Typing, Memory, Reaction, Hold, Steps, Shake, Scan, order of `CHALLENGE_TYPES`); choosing appends with defaults, requests runtime permission (Steps = ACTIVITY_RECOGNITION, Scan = CAMERA) and opens its Detail; "Back" quiet button |
| ChallengeDetail(index) | configure one | per-type controls (see 4); top-right "Try"; help = long description; "Remove challenge" danger link; Done returns to the Challenges sheet; Back/scrim also returns to the list sheet |
| Limit | max ring time | chips 30 min / 60 / 90 / 120 / Forever (`ring_limit_secs` Some(1800..7200) or None) |
| WakeCheck | confirm-awake check | chips Off / 3 / 5 / 10 min (`wake_check_minutes`) |
| Notice | silent notice before ring | chips Off / 5 / 10 / 30 / 60 min (`notify_before_minutes`) |
| RingLook | lock-screen look | top-right "Preview" link (full-screen preview), 4-column grid of 8 backdrop tiles (live miniature each; Photo tile shows "+" until a photo exists and then picks one), "Choose another photo" link when Photo selected, Ring colour row (only for backdrops that take a colour; shows "Auto" + dot + chevron), Clock style chips (Clean, Thin, Bold, Classic, Mono), Done |
| RingColour | colour the background moves in | live backdrop preview, accent preset swatches, `ColorControls` with an "Auto" chip (clears to accent/sunrise default), Preview link, Done returns to RingLook |
| Delete | confirm deleting the alarm | "Delete" danger (stops preview, removes, toast with Undo, goes back) and "Keep" |
| Discard | unsaved changes | "Discard changes" danger (go back) and "Keep editing" |

Ring preview (`RingPreview`, not a sheet): full-screen `RingFace` of the draft with fake Snooze (if count>0), hint text
t("ring.preview_hint") and a fake Turn off; any tap or Back closes it; the buttons do nothing.

### 2.3 Settings sheets (`settings/sheets.rs`)
| Sheet | Controls |
|---|---|
| Language | "Follow phone" (maps `mobile_sentinel::device_language()` to a locale), then English, Nederlands, Espanol, Francais, Deutsch; sets `prefs.language_follows_phone`, `settings.locale`, reloads translations, `platform::set_ring_language(locale)` |
| TimeFormat | Follow phone / 24-hour / 12-hour (`prefs.time_format`) |
| WeekStart | Monday / Sunday / Saturday (`prefs.week_start`) |
| Volume | help t("settings.min_volume_desc"); chips Off, 25, 50, 75, 100 % (`settings.min_volume_percent`, applied live via `platform::set_min_volume`); Done |
| Backup | note t("settings.backup_note"); "Save to file" (writes `alarmfree-YYYY-MM-DD.json` via `platform::save_to_downloads`, toast with place or failure) and "Restore from file" (file picker thread, `alarm_ops::import_json`, appends up to the room left under 50, assigns order, toast t("settings.backup_restored") counted, with Undo; bad file = error toast; cancel = silent) |
| DeleteHistory | same component as History (30 d / 90 d / all, each confirmed) |
| About | title "AlarmFree <version>", note t("app.description"), line t("app.tagline"), Done |

### 2.4 Other dialogs
- Fallback sheet in ChallengeRun (1.9).
- Toast (not a dialog, `components/toast.rs`): see 6.

---

## 3. Settings

### 3.1 App settings (`models/settings.rs`; persisted by `persistence::save_settings` JSON in the db)
| Setting | Field | Type / options | Default | UI |
|---|---|---|---|---|
| Theme | `theme` | System, Dark, Light, Black, Midnight, Forest, Plum, Sand, Mist, Custom | System | ThemeScreen |
| Language | `locale` + `prefs.language_follows_phone` | English, Dutch, Spanish, French, German; follow phone | English / true | Language sheet |
| Min volume | `min_volume_percent` | 0,25,50,75,100 | 50 | Volume sheet |
| Time format | `prefs.time_format` | System, H24, H12 | System | sheet |
| Week start | `prefs.week_start` | Monday, Sunday, Saturday | Monday | sheet |
| Sort | `prefs.sort` | Own, Time, NextRing | Own | Home sort sheet |
| Touch feedback | `prefs.touch_feedback` | bool (also `feedback::set_enabled`) | true | switch |
| Less motion | `prefs.less_motion` | bool (adds `less-motion` class) | false | switch |
| Confetti | `prefs.confetti` | bool (28 particles or 0) | true | switch |
| Accent | `prefs.accent` | Blue, Teal, Purple, Pink, Amber, Coral, Sage, Sky, Lime, Custom | Blue | ThemeScreen |
| Custom accent | `prefs.custom_accent` | Option<Rgb> (used when accent == Custom) | None | wheel |
| Custom theme | `prefs.custom_theme` | Option<{background, card?, text?}> | None | ThemeScreen |
| Sound names | `prefs.sound_names` | map file to display name | empty | rename |
| New-alarm defaults | `prefs.alarm_defaults` | Option<AlarmData> | None (then `factory_alarm`) | defaults editor, menu "make default" |
| Hidden warnings | `prefs.hidden_warnings` | Vec<check name> | empty | Home banner X |
| Setup done | `prefs.setup_done` | bool | false | FirstRun |
| (unused) | `prefs.notify_before_minutes` | u8 | 0 | no UI |

Factory alarm (`alarm_ops::factory_alarm`): 07:00, Mon-Fri, fade-in 60 s, ring limit `DEFAULT_RING_LIMIT_SECS`, first
bundled sound, sound_mode 0, 3 snoozes of 5 min, vibration 0, no challenges.

### 3.2 Per alarm (`AlarmData`, `app.rs`)
| Field | Meaning / options | Edited in |
|---|---|---|
| `id` | uuid ("" for defaults) | n/a |
| `time_hour`, `time_minute` | 0-23, 0-59 | time section / Time sheet |
| `days: [bool;7]` | Mon first; none selected + `date` = one-time | repeat section |
| `date: Option<YYYY-MM-DD>` | one-time date | Date sheet |
| `label`, `note` | <= 100 chars, sanitized | Label sheet |
| `enabled` | on/off | Home switch (editor save forces true) |
| `skip_until_ms` | skip next occurrence | swipe / menu |
| `pause_until_ms` | holiday pause | menu / pause-all |
| `order` | manual position, 0 = unsorted | drag |
| `sound` | file name; `sound_mode` 0 sound+vib, 1 sound only, 2 vibration only | Sound / Vibration sheets |
| `vibration` | 0 Pulse `[0,500,300,500]`, 1 Steady `[0,2000,100]`, 2 Heartbeat `[0,100,100,100,700]` | Vibration sheet |
| `sound_delay_secs` | 0,15,30,60 (only with sound_mode 0) | Vibration sheet |
| `volume_percent` | Option<0..100 step 5>; None = global minimum | Sound sheet |
| `fade_in_secs` | 0,15,30,60,120,300 | Sound sheet |
| `snooze_count` | 0-10; `snooze_interval` 1-60 min | Snooze sheet |
| `challenges: Vec<(type, json)>` | up to 10 | Challenges sheets |
| `ring_limit_secs` | Some(1800/3600/5400/7200) or None | Limit sheet |
| `wake_check_minutes` | 0,3,5,10 | Wake check sheet |
| `notify_before_minutes` | 0,5,10,30,60 | Notice sheet |
| `max_brightness` | bool | switch row |
| `ring_look` | `{backdrop, colour?, clock, photo?}` | Ring look sheets |
| `delete_after_ring` | naps, wake checks, tests (not user editable) | n/a |
| `test`, `hidden` | test alarm / not listed (wake checks, tests) | n/a |

---

## 4. Challenges

Registry: `models/setup.rs` `ChallengeSetup` (+ `CHALLENGE_TYPES` order: math, typing, memory, reaction, hold, steps, shake,
scan), generation in `models/challenge.rs` `ChallengeEngine`, run in `screens/challenge_run.rs` + `components/challenges/*`.
All challenge components are told `on_complete`/`on_solved` once; each lives under `.challenge-wrapper` with an instruction line.
Difficulty names: Easy, Medium, Hard, Extreme (`difficulty.<name>`); new Math/Memory/Typing default to Medium.

| # | Challenge | Setup fields (range, default, step in editor) | Rules |
|---|---|---|---|
| 1 | Math | difficulty | Sum shown, answer on on-screen keypad (1-9, backspace, 0, check). Solved automatically as soon as the typed digits equal the answer (max 12 digits); the check key only shows "incorrect" (`wrong` state, display shakes red via class). Easy `XX +/- XX` (10..98), Medium `XXX +/- XXX` (100..998), Hard `a x b` (11..48 x 2..18), Extreme `a x b` (100..498 x 2..11); subtraction is always big minus small |
| 2 | Typing | difficulty | Passage of random everyday words in the user's locale (word pools for en/nl/de/fr/es, unaccented). Words: Easy 5-10, Medium 15-20, Hard 25-35, Extreme 40-60; min accuracy 80/85/90/95 %. Passage drawn as per-character spans (correct green, incorrect red, cursor highlight on next, pending), words never break mid-word; multiline text area (no autocorrect/capitalize/spellcheck, autofocus, soft keyboard); live "Accuracy N%" (good/bad colour) next to "min N%", progress "typed/total". Accuracy = 1 - Levenshtein/longest, case-insensitive, trailing spaces ignored (`typing_accuracy`). Done when accurate enough AND typed + `floor(len*(1-min))` >= len; if all typed but too wrong shows "incorrect" |
| 3 | Memory | difficulty | Grid 3x3 (Easy, Medium) or 4x3 cells (Hard, Extreme); sequence length 4/6/9/12, no repeats. Display phase: after 300 ms each cell lights 1 s, 500 ms pause, then input. Input: tap cells in order; correct tap marks it with its order number; a wrong tap marks the cell red (shake 0.3 s) and counts; 3 wrongs replay the sequence from the start; a correct tap clears red marks and wrong count; tapped cells cannot be re-tapped; status line "done/total" + "wrong x/3" (fixed height); hints below per phase |
| 4 | Shake | `count` 5..200 (step 5), default 20 | `sensors::reset_shake_count` + `start_accelerometer` on mount (effect), `stop_accelerometer` on drop; poll `shake_count()` every 200 ms; ring shows count/required + unit; done at >= required. Permission: none |
| 5 | Steps | `count` 5..200 (step 5), default 20 | `sensors::start_step_counter` / `stop_step_counter`; poll `step_count()` every 250 ms; ring progress. Permission ACTIVITY_RECOGNITION requested when added/tried |
| 6 | Hold | `count` 1..30 (default 10), `secs` 1..15 (default 3) | Bubble at a random position in the play area (position as 0..1 fractions mapped inside a 64 px safe margin). Press the bubble and hold until its ring fills (CSS stroke transition over `hold_ms`, min 300 ms); release anywhere in the area drains it and cancels; completion = pop animation 280 ms (check icon), haptic ticks 18 / 45 / 130 ms, new bubble elsewhere; progress "done / required unit". Optional back arrow (fallback mode) |
| 7 | Reaction | `targets` 3..30 (default 10), `secs` 2..10 (default 3), `rounds` 1..5 (default 2) | Round 1 opens with a 3 s "Get ready" countdown (1.5 s between later rounds). Up to 3 targets visible; each pop spawns a replacement until the round's quota is met; targets avoid each other (min separation 0.34, 32 tries); timer bar drains over the time limit (min 1 s, polled every 40 ms); time-out resets to round 1 with full countdown (haptic 60); clearing a round haptic 80, final 130; progress "round/rounds . popped/targets". Pointer-down pops (no click delay) |
| 8 | Scan | `code` string (empty = any code) | Button opens the SDK camera scanner (`scanner::scan()` blocking in a thread; `SCANNER_OPEN` flag read by ChallengeRun to pause the idle clock). States Ready, Scanning ("point camera"), WrongCode (red text + "tap to retry"), Done (check). Success when trimmed scan equals trimmed expected (or any if empty). In the editor: text field for the code plus "Scan now" button that fills it from a scan. Permission CAMERA requested when added/tried. Code sanitized by `sanitize_reference_value` on save |

Editor summaries (`challenge_summary`): difficulty name; "N shakes"/"N steps"; Hold "N x S s" (counted key);
Reaction "N targets / S s / R rounds" (`reaction_one` variant if rounds==1); Scan "any code"/"code X". Detail sheet also
shows a difficulty example line (`challenge.example.<type>_<difficulty>`) and a description as help (`challenge.desc.*`).
Shared progress ring (`ProgressRing`): 180 px circle, track + accent arc (stroke-dashoffset, 0.3 s ease), centre text
"count / required", unit label below. Hold button, ring, reaction target, memory cell visuals are listed in 5.4.

Practice mode: same generators and components via `ChallengeRunner` (`practice.rs`): generated once per run (`prepare`),
Exit always available, nothing written to history or AlarmKit. Editor "Try" (one) and "Try all" call it in place
and request the needed permission first.

Fallback: any non-Hold challenge can be swapped (confirm sheet) for 20 holds of 3 s (`FALLBACK_HOLDS`, `FALLBACK_HOLD_MS`);
history records it as HoldButton.

---

## 5. Theme system, ring looks, animations

### 5.1 Palettes (`models/theme.rs`) and derivation
Base colours (background / card / text): Dark `101218/1b1e27/f2f3f7`, Light `f5f6f9/ffffff/14161c`, Black
`000000/101116/f2f3f7`, Midnight `0b1220/162033/eef2fa`, Forest `0e1714/18241f/eef4f0`, Plum `17101c/241a2c/f4eff7`,
Sand `f3ede2/fbf7ef/2a2419`, Mist `e9eef2/f7f9fb/1c252c`. System = Dark when the phone is dark else Light (re-evaluated on
a dark-mode change). Custom = `CustomTheme { background, card?, text? }`; missing card/text computed: card = background
mixed 7 % toward white if dark, 60 % toward white if light (or 4 % toward black if the page is nearly white); text =
light/dark default.
Accents `(dark shade, light shade)`: Blue 7cb0ff/1f5fd6, Teal 5fd4c4/00736a, Purple b9a0ff/6a3fd1, Pink ff94c8/b8246f,
Amber ffc45c/8a5a00, Coral ff9d8a/c2452d, Sage 9ad49a/3c7d4a, Sky 86d4f0/0b7285, Lime cfe37a/5f7a00; Custom wheel colour is
used for both. Palette `accent` = shade for light/dark background then `readable_on(bg, card)`.
`MIN_CONTRAST` = 3.0: text/accent/warn/danger/ok are nudged toward white or black (20 steps) until >= 3.0 against both
background and card; `is_light` = more contrast against black than white.

Derived variables (`Palette::css_vars`; in Slint put these into a `Palette` global struct): `--bg --surface(card)
--surface2 --line --text --text2 --accent --on-accent --accent-soft --accent-glow --warn --warn-soft --danger --ok
--ok-glow --off --knob-off --inverse --on-inverse --bar-soft`. Formulas (mix(a,b,t) = a toward b):
- surface2 = card.mix(text, raised), line = card.mix(text, line), off = card.mix(text, offAmt); (raised, line, offAmt, soft) =
  light (0.09, 0.12, 0.23, 0.12), dark (0.06, 0.09, 0.145, 0.17).
- text2 = text.mix(bg, 0.3).readable_on(bg, card); accent-soft = bg.mix(accent, soft); bar-soft = card.mix(accent, 0.45).
- accent-glow = rgba(accent, 0.35 light / 0.45 dark); ok-glow = rgba(ok, 0.55); warn-soft = bg.mix(warn, 0.15).
- warn/danger/ok = light `8f5400/b3261e/1b7a4b`, dark `ffb454/ff8a7a/6fd6a0`, each readable_on(bg, card).
- on-accent = accent.mix(black, 0.85) if accent is light else white; knob-off = white (light) else text2;
  inverse = text; on-inverse = card.
Other root tokens: radii 8 / 14 / 20, nav height 64, `--safe-top/bottom/left/right`, `--text-zoom` (font scale probe),
sizes: type 11 (day letters), 12, 14, 16, 20, 24 (list times), 28 (titles), 64/96 (ring clock). Spacing in steps of 4.
Also set on theme change: `display::set_light_system_bars(palette.light)` and `display::set_system_bar_color(background_argb)`;
initial window background = saved palette background (`app::start_background`) to avoid a flash.

### 5.2 Colour editor (`components/color.rs` `ColorControls`) and wheel
Inputs `hsv` (kept apart from rgb so black keeps its hue) and `typed` (hex field text while editing). Elements:
- 200 x 200 px circular wheel: conic hue gradient (from red at 3 o'clock, clockwise: red, yellow, green, cyan, blue,
  magenta) with a radial white-to-transparent overlay (saturation), a darkening overlay with opacity `min(1 - value, 0.65)`,
  and a thumb (circle filled with the shown colour) at `radius * s` along the hue angle. Pointer-down / move (while
  touching) maps `(x, y)` to hue = atan2 degrees, saturation = distance / radius clamped to 1 (outside = rim); on near-black
  (value < 0.05) a touch sets value to 1. Pointer up / cancel = `on_release` (saves). Touch-action none (must not scroll).
- Brightness slider 0-100 (`value`), change = `on_colour`, release = `on_release`.
- Hex text field (max 7, no autocapitalize): `Rgb::parse` accepts `#rrggbb`, `rrggbb`, `#rgb`; valid input applies live and
  moves the wheel; raw text is shown while typing; change event = release. `extra` slot beside it (the "Auto" chip).
- Swatch row above (selected ring).

### 5.3 Ring looks (`models/ring_look.rs`, `screens/ring_face.rs`, css 1255-1500)
`RingLook { backdrop, colour?, clock, photo? }`, stored in `AlarmData.ring_look`; unknown/corrupt values deserialize to
defaults. `colours(accent)` returns 3 colours: base = chosen colour, else orange `ff8a4c` for Sunrise, else the theme
accent; neighbours = hue +35 and -35 degrees (same S, V); if a neighbour lands in hue 50-100 (olive zone) both go the same
other way (+-35, +-70); Sunrise ignores the olive rule. Exposed as `--ring-1..3`. Backdrops take no touches; `Plain` and `Photo`
take no colour (`takes_a_colour`). Unit `u` = 1vmin (window) on the ring screen, 0.3vmin and height 76 px on a tile / live
preview; `h` = window height. Everything under half strength so text stays readable. On `ring-dressed` screens the date,
clock, label, note and challenge hint get a halo `text-shadow` in the background colour.

| Backdrop | Parts | How it is drawn |
|---|---|---|
| Plain | 0 | nothing |
| Aurora | 3 | three 130u circles with radial gradient (ring-N to transparent at 66 %): 1 at top -20 % left -40 % opacity .5, drift 19 s ease-in-out alternate to (+36u, +26u) scale 1.2; 2 at top 16 % right -55 % opacity .42, 25 s to (-40u, +34u) scale .85; 3 at bottom -28 % left -18 % opacity .38, 31 s to (+28u, -44u) scale 1.15 |
| Sunrise | 2 | sky: full-screen linear gradient bottom-up `ring-1, ring-3 36 %, transparent 76 %` opacity .55, fades in from .08 over 45 s ease-out then stays; sun: 120u circle, centred, bottom 2 %, radial `ring-2, ring-1 30 %, transparent 68 %` opacity .85, rises from +0.34h and opacity .2 over 45 s ease-out then stays |
| Waves | 3 | three 280u squares centred horizontally with top at 56 / 62 / 68 % of h, border-radius 44 / 42 / 46 %, colours 1/2/3, opacity .22 / .20 / .18, rotate 360 deg linear 23 s / 31 s (reverse) / 41 s, so the top edge rolls |
| Stars | 3 | three full-screen layers of 5-7 tiny radial-gradient dots (radius .48-.96u, min 1 px) in colour 1/2/3 with fixed positions (see css 1388-1425); each layer twinkles (opacity to .2 and up 3u) alternate: 5 s, 7 s (delay 1 s, reversed), 11 s (delay 2 s); layer opacity 1 / .85 / .9. On the real ring screen a vertical mask hides stars between 10 % and 44 % of height (behind the time) |
| Rings | 4 | concentric 46u circles with 2 px border at left 50 %, top 22 %; start scales 1 / 1.7 / 2.4 / 3.1 and opacity .36 / .26 / .16 / .08; borders ring-1, ring-2, ring-1, ring-3; animation `ring-out` 9 s ease-out infinite (scale 0.6 to 4, opacity .6 to 0), delays -2.25 s steps |
| Bubbles | 9 | circles 7u-30u with radial gradient (transparent to 52 % then ring colour to the rim, colours cycle 1,2,3) opacity .6, fixed left/top, each rises bottom to top (`bubble-up`: from +h, 50 % at 0 with +4u sway, to -h) linear infinite, durations 19-37 s with negative delays |
| Photo | image | user-picked image (`ring_photos::import()` native picker, stored in app files, `data_url(name)` loader, pruned at start) covering the screen with slow zoom to 1.09 over 40 s alternate, plus a 50 % black scrim; text forced white (`--text #fff`, `--bg #000`), snooze button black 50 %; falls back to Plain if the file is unreadable |

Clock styles (class on the big clock): Clean (default: weight 500, letter-spacing -2), Thin (weight 200, -1), Bold (800, -3),
Classic (serif 400, -1), Mono (monospace 500, -6; -4 for 12 h). Less motion: backdrops render their static first frame.

### 5.4 Animations (`@keyframes` and transitions in style.css)
| Name | Animates | Where used |
|---|---|---|
| `aurora-one/two/three` | translate + scale of blobs | Aurora backdrop |
| `sunrise-sky`, `sunrise-sun` | sky opacity from .08; sun rise + opacity | Sunrise backdrop (45 s once) |
| `wave-turn` | rotate 360 deg | Waves backdrop |
| `stars-twinkle` | opacity .2 + translateY -3u | Stars backdrop |
| `ring-out` | scale .6 to 4 + fade | Rings backdrop |
| `bubble-up` | translate bottom to top | Bubbles backdrop |
| `photo-drift` | scale to 1.09 | Photo backdrop |
| `sheet-up` | slide up 24 px + fade, 0.18 s | `.sheet` |
| `toast-in` | rise 8 px + fade, 0.15-0.18 s | toast, help text paragraphs |
| `cell-shake` | translateX 0, -4, +4, 0 (0.3 s) | `.memory-cell.wrong` |
| `bubble-in` | scale .3 to 1.12 to 1 + fade, 0.42 s overshoot | new hold bubble |
| `bubble-pop` | scale 1.08, 1.22, .5 + fade, 280 ms | `.hold-bubble.popping` |
| `hold-ripple` | scale 1 to 1.9 + fade, 280 ms | ripple on pop (`.hold-bubble.popping::after`) |
| `celebration-fade` | overlay dims 0 to .25 to 0, 1800 ms | Celebration overlay |
| `check-pop` | scale 0 rotate -30 to 1.15 to 1, 500 ms | Celebration check |
| `confetti-burst` | outward `--dx/--dy` (radius 220 px), falls +200 px, rotates 900 deg, fades, 1600 ms with per-piece delay | Celebration pieces |
| `reaction-count-pop` | scale .7 to 1.1 to 1, 1 s repeating | reaction countdown number |
| `reaction-spawn` | scale .2 to 1.15 to 1, 0.22 s | new reaction target |
| `reaction-target-pulse` | accent ring expands 0 to 12 px, 1.1 s repeating | reaction target |
Transitions: switch track/knob 0.15 s; chip/cell colours 0.2-0.25 s; hold ring fill `--hold-ms` linear (drain 0.35 s ease-out);
reaction timer width `--react-ms` linear; run-timer width 1 s linear; progress ring offset 0.3 s; time wheel snap 0.24 s
cubic-bezier(.22,1,.36,1); history chevron rotate 0.2 s; reorder neighbour shift 0.15 s.
Less motion / prefers-reduced-motion: every animation and transition 0.01 ms, iterations 1, EXCEPT functional ones
(`hold-ring-fill`, `reaction-timer-fill`, `run-timer-fill`); celebration overlay animation none.

---

## 6. Shared components: the Slint component list to build

From `components/ui.rs` unless noted. Props in brackets.

| Slint component | Source | Props / callbacks | Notes |
|---|---|---|---|
| `Sheet` | ui.rs | title, note, help, action slot, content slot, closed() | scrim + panel, slide-up, "?" toggle, registers Back (see 7), counts into `open-sheets` |
| `SheetDone`, `SheetItem` | ui.rs | label, quiet / danger / selected, clicked() | primary vs quiet; selected/danger styles |
| `GroupTitle` | ui.rs | text | |
| `Chips` (generic) | ui.rs | options [(value,label)], selected, grow, picked(value) | build as `[string]` + selected-index; value mapping in Rust |
| `Stepper` | ui.rs | title, value, less(), more() | two round buttons |
| `StartStop` | ui.rs | running, start(), stop() | play/stop icon + text |
| `Row` / `SwitchRow` | ui.rs | title, value, clicked / on, toggled(bool) | chevron / switch |
| `Switch` | ui.rs | on | track + knob, animated 0.15 s |
| `ScreenHeader` | ui.rs | title, help, back(), extra slot | back arrow, title, optional "?" with in-place help paragraph |
| `BottomNav` | ui.rs | current route | 3 items clock / bar-chart / gear icons, labels t("nav.*"), active highlight |
| `HelpButton` | ui.rs | open | circle "?" toggling help text |
| `Icon` | components/icon.rs | name, size, colour | 19 outline icons: Clock, ChartBar, Gear, Trash, Check, Xmark, Plus, ChevronRight, ArrowLeft, Grip, EllipsisVertical, Pause, Sort, Warning, Pencil, Skip, Touch, Play, Stop. Stroke 2 (3 for EllipsisVertical), round caps; use `Path` with SVG commands (24x24 view box) |
| `Toast` | components/toast.rs | message, level (info/error), undo present, dismiss(), undo() | bottom (top while a sheet is open), auto-dismiss 3 s info / 6 s with undo / 10 s error, tap dismisses, Undo button stops propagation; a new message replaces and restarts the timer |
| `Celebration` | components/celebration.rs | label, particles | overlay dim + check pop + N confetti pieces (angle = i*360/N, radius 220, delay (i%6)*30 ms, hue (i*47)%360, size 6 + (i%4)*2 px) |
| `TimePicker` / `WheelColumn` | components/time_picker.rs | hour, minute, twenty-four, changed(h,m) | 2 drum wheels, 7 visible rows, row pitch 40 px; drag up = increase, 1:1 finger tracking with snap on release; tap neighbour = +/-1/2; wraps 24/60; haptic tick 8 ms per step; swallow the click after a drag; 12 h hour labels carry AM/PM; centre 24 px bold-ish, near 20 px .8, far 16 px .5, edge 14 px .28; 3D tilt (rotateX 20 deg per row) can be approximated with scale/opacity |
| `ColorControls` | components/color.rs | hsv, typed, code, colour-changed(rgb), released(), extra slot | see 5.2 |
| `ProgressRing` | challenges/progress_ring.rs | count, required, unit | `Path` arc (circumference 282.74 at r 45) |
| `MathChallenge`, `MemoryGameChallenge`, `TypingChallenge`, `ShakeChallenge`, `StepCountChallenge`, `HoldButtonChallenge`, `ReactionChallenge`, `ScanChallenge` | challenges/*.rs | per 4; `complete()` | state machines in Rust, view in Slint |
| `AlarmCard` | home/card.rs | alarm view struct, next-fire text, handlers | swipe-reveal strip (custom `Flickable` with snap), long-press drag |
| `RingFace`, `RingBackdrop` | screens/ring_face.rs | look, date, clock, label, note, children | shared by ring screen, preview, tiles |
| `CheckRow`, `Banner` | reliability.rs / home | | status rows and the tappable banner |
| `Field` | `.field` | text, placeholder, input-type (numeric, text, date), max-length, edited(text) | wraps `TextInput`; date needs a custom picker (see 8.6) |
| `Slider` | `.slider` | min, max, step, value | volume 0-100/5 and brightness 0-100 |
| `Chip` / `Pill` / `DayChip` / `Swatch` / `ThemeTile` / `LookTile` / `BarChart` / `Dots` | various | | small leaf components |

---

## 7. State map (Dioxus to Slint)

Recommendation: one `global AppState` (Slint `export global`) for UI-visible, read-mostly data and callbacks; a Rust
`AppCore` (Rc<RefCell<..>>, UI thread) that owns the real data (`Vec<AlarmData>`, `AppSettings`, translations) and pushes
snapshots to Slint via `VecModel`/properties; ephemeral per-screen state lives in the screen's own properties.

| Dioxus state | Holds | Slint mapping |
|---|---|---|
| `settings: Signal<AppSettings>` (context) | theme, locale, min volume, prefs | Rust `Store.settings` + global `Settings` properties (theme index, time-format, week-start, sort, three bools, locale index, `24h` resolved bool); setters = callbacks into Rust which save (`persistence::save_settings`) then call `platform::refresh_outside_app` where Dioxus did |
| `alarms: Signal<Vec<AlarmData>>` (context) | all alarms incl. hidden | Rust `Store.alarms`; Slint gets `[AlarmRow]` (`VecModel`) of the listed, sorted, display-ready rows (time text, label, when text, day flags, enabled, skip_active, can_skip, id). Rebuild on any change and on the 15 s tick; actions pass the `id` |
| `translations: Signal<HashMap>` | current language strings | Rust `Translator` + a global `T` of properties, OR set a global `Strings` struct; changing language rebuilds all strings (Slint `@tr` is gettext-based and does not match the JSON, so do NOT use `@tr`). Keep `t(key)` in Rust and push resolved strings, or expose a `string -> string` pure callback `T.t(key)` |
| `toast_signal: Signal<Option<ToastMessage>>` | message, level, undo snapshot | Rust `ToastController` (owns the undo `Vec<AlarmData>` snapshot and the dismiss timer); Slint global `Toast { visible, text, level, has-undo, top }` + callbacks `dismiss()`, `undo()` |
| `OpenSheets(Signal<u32>)` | open sheet count | Rust counter in `SheetStack`; Slint property `Toast.top` |
| `BackStack` + `BACK_PRESSED` + `leave_screen: Signal<u32>` | Back handler stack, press queue, "leave screen" tick | Rust `BackDispatcher`: `Vec<(id, Box<dyn Fn()>)>`, `on_back` from SDK posts to the UI thread (`slint::invoke_from_event_loop`), pops the top handler, else navigates; ignored while firing |
| `Route` + `NavWatcher` + `nav_intent` | current route, intent after ring | Rust `Navigator` + global `Nav { current-page: int, can-go-back }`; stack `Vec<Route>`; `push/replace/go_back` callbacks; `nav_intent` becomes a field applied when the ring screen closes |
| `firing_id: Signal<Option<String>>` + `FiringWatch` poll (500 ms, `firing_state::store_marker/peek_firing_alarm`) | which alarm rings | Rust `FiringWatcher` (Timer 500 ms) -> global `Firing { active, alarm-id }`; when active the root shows the ring page instead of the nav page |
| `wide: WideScreen(Signal<bool>)` | width >= 840 css px | Slint `window.width` binding in the root; compute logical px (Android density) so the 840 css px threshold still means 840 dp |
| `safe_area` (App local) + `--safe-*` | top/bottom/left/right insets (bottom = max(nav bar, keyboard)) | global `Insets { top, bottom, left, right }` set from `display::safe_area_insets()` by Rust `InsetsWatcher` (poll at 0 / 300 / 1500 ms after start and on window resize/focus/keyboard) |
| `system_dark` (App local) | phone dark mode | Rust: `display::system_dark()` read on resize/focus; re-derive palette |
| `palette` (derived from settings + system_dark) | colour set | global `Palette` struct (all 20 tokens + `light`), rebuilt in Rust by `Palette::css_vars`-equivalent; also call `set_system_bars` |
| `text-zoom` (CSS var) | Android font scale | global `Scale.text-zoom` read from Android config (needed to keep ring clock size constant) |
| `motion_class` | `less_motion` | global `Settings.less-motion` bool; animated properties use `duration: Settings.less-motion ? 0ms : X` |
| Home local: `tick`, `problem`, `sheet`, `pause_date` | timer re-render, banner check, open sheet, picked date | Rust `HomeController` with `Timer` (1.5 s then 15 s) + Slint property `sheet: int` + `pause-date: string` |
| `HomeState` (context) | alarms, toast, translations, sheet, pause date + `change()` | Rust `fn change(message, mutate)` = snapshot, mutate, save, toast with undo |
| Editor `EditorState`: `draft`, `one_time`, `date_value`, `setups`, `sheet`, `error`, `typed_time`, `renaming`, `trying`, `preview`, `previewing_look` | working copy and dialog state | Rust `EditorSession` (owns `draft: AlarmData`, `original`, `setups: Vec<ChallengeSetup>`, `one_time`, `date`, preview handles) + Slint `Editor` page properties (flattened strings/ints/bools); `has_changes` computed in Rust |
| `Preview` (handle, token, sounding, vibrating) | running preview | Rust `PreviewPlayer` with generation token + 15 s `Timer`; Slint props `sounding`, `vibrating` |
| `SettingsState` | settings + open sheet | Slint page `sheet` int; actions call `Store` setters |
| History locals: `period`, `custom_days`, `deleting`, `show_all`, `open_event`, `confirm_delete`, `refresh` and the memo | period filters etc. | Rust `HistoryController` computing a `HistoryView` struct (stats, bars `[Bar]`, events `[EventRow]`); Slint holds ephemeral toggles |
| Reliability `current` + 1.5 s poll | `HealthReport` | Rust `HealthPoller` (Timer 1.5 s) -> `[CheckRow]` model (title, note, ok, can-fix) + `first_problem` banner string for Home |
| ThemeScreen `picking`, ColorSheet `hsv`, `typed` | role being picked, colour editor | Slint props + Rust `ColorEditor` (role, hsv, typed) |
| `AlarmFiring`: `snoozes_left`, `first_interaction`, `running`, `now`, `RingTimes` | ring screen state | Rust `RingController` (loads `RingTimes` from `kit.session`, owns the snooze/turn-off logic and `finished()`), Slint `Ring` page props |
| `ChallengeRun`: `index`, `idle_secs`, `celebrating`, `solves`, `started_at`, `fallback`, `asking_fallback` | run state | Rust `RunController` (1 s idle timer, sensor deltas, celebration timer) + Slint props; each challenge has its own Rust state machine struct driven by `Timer`s |

Rust-side bridge objects to name in the port: `AppCore/Store`, `Translator`, `Navigator`, `BackDispatcher`,
`ToastController`, `SheetStack`, `InsetsWatcher`, `PaletteService`, `HomeController`, `EditorSession`, `PreviewPlayer`,
`HistoryController`, `HealthPoller`, `FiringWatcher`, `RingController`, `RunController`, `ReorderController`,
`FilePicker` (sound, photo, backup; each blocks in a thread and posts the result back), `DatePicker`, `HapticsBridge`
(wraps `feedback::tick/pattern`), `LivenessBeat` (replaces `ui_round_trip`).

---

## 8. Platform glue used by the UI

### 8.1 `document::eval` snippets (all must be replaced)
- `app.rs` `WINDOW_WATCH` (see 8.3) and `ui_round_trip` (`dioxus.send(1)` round trip, 3 s timeout) and the debug `script_loop`.
- `reorder.rs`/`reorder.js` (8.2).
- `home/card.rs`: after Skip, scroll all `.alarm-card` back to `left: 0` smoothly.
- `debug_hooks.rs`: `js:<script>` action runs arbitrary script in the page and writes `debug_script_result.txt`. This is how
  `devtest/` scenarios press buttons and read the screen. The port needs a replacement: a debug-only action file/command
  channel (e.g. `tap:<element-id>`, `text:<id>:<value>`, `dump` writing a tree of visible text/ids) or accessibility labels,
  otherwise `devtest/all.sh` cannot run. Other actions (`snooze:<id>`, `dismiss:<id>`, `hang_ui`, `debug_<name>.txt` numeric
  overrides read by `platform::shortened`) are UI-independent and stay. `hang_ui` must block the Slint event loop thread.

### 8.2 `reorder.js` (drag to reorder)
- Params: container id (`alarm-list`, `challenge-list`), row class, optional handle class, long press 450 ms.
- Home list: no handle; a long press on a row (movement tolerance 10 px cancels) starts the drag; buttons are ignored except
  the card's main button (`hold-to-drag`); inputs/labels/links ignored. Challenge list: starts immediately on the grip handle.
- While dragging: row follows the finger (translateY), gets raised style (surface2, shadow), neighbours shift by one slot to
  show the target, the scroll container auto-scrolls (finger within 110 px of the top or 170 px of the bottom: 14 px per
  16 ms), page scroll is blocked, haptic 15 ms on start, context menu suppressed.
- Drop: rows reset without animation; clicks within 400 ms are swallowed; reports `(from, to)` row indexes only if changed;
  cancel reports nothing. Rust remaps shown-index to full-list index (hidden alarms excluded) and saves. Port as
  `ReorderController` + a Slint list item with `TouchArea` and pointer-event tracking inside a `Flickable`.

### 8.3 `WINDOW_WATCH` (inside `app.rs`)
Sets `window.interpreter.headless = true` (apply DOM changes immediately instead of in animation frames, so a hidden WebView
still processes UI tasks, e.g. dismissing after a challenge); measures the text zoom (probe 100 px element -> `--text-zoom`);
sends the inner width on resize, visibility change and dark-scheme change (drives `wide`); while a text field is focused
polls every 200 ms (stops after ~1 s without focus) and scrolls the focused field into view above the soft keyboard
(`innerHeight - --safe-bottom`). Port needs: width callback, dark-mode change callback, font-scale read, keyboard-aware
scrolling (8.6), and headless-equivalent liveness (the event loop must keep processing while the window is not drawn).

### 8.4 Back handling
SDK `foregrounding::on_back(|| ...)` (the activity never finishes on Back, so the process lives): a press is queued; the UI
pops the top `use_back_handler` entry (sheets, ring preview, practice/ChallengeRunner, ThemeScreen, editor leave guard,
Reliability leave) else applies the route rule (1). Drop of a component removes its handler. Ignored while `firing_id` is set
(the SDK's kiosk blocks Back, Home, Recents, volume keys during a ring).

### 8.5 Window insets, system bars, heartbeat
- `display::safe_area_insets()` (top, bottom, left, right, keyboard; bottom = max(bar, keyboard)), rounded to whole px, read
  at 0 / 300 / 1500 ms after start and on every window event; used for padding of screens, sheets, ring screen, FAB and nav bar.
- `display::set_light_system_bars`, `set_system_bar_color` after theme change and after a window recreate.
- `mobile_sentinel::keep_event_loop_awake()` at start (keeps the event loop alive when the window is not drawn).
- Heartbeat: every 1 s while ringing `jobs::ui_heartbeat()` only after a UI round trip proved the UI thread works; if it stops
  the `:sentinel` guardian restarts the app; after two UI failures the SDK shows its native backup screen.
- Start-up in `App`: `platform::init_platform`, `feedback::set_enabled`, `history_persistence::delete_history(5 years)`,
  `ring_photos::prune`, DB recovery notices (`db.recovered`, `db.volatile`, `db.unreadable`) and unseen outcome notices
  (`alarm.ended_by_reboot`, `alarm.missed`, `alarm.ended_by_limit`) as one error toast; every 500 ms: `take_save_failure` toast
  `db.save_failed`; `take_ring_end_news` reloads the alarm list and shows outcome notices.

### 8.6 Soft keyboard and pickers
Text inputs that need the soft keyboard: Time sheet typed time (numeric), Label, Note (both max 100), sound rename (max 60),
scan code, hex colour (max 7, no autocapitalize), history custom days (number 1-365), Typing challenge (multiline, no autocorrect /
capitalize / spellcheck, autofocus). The field must stay visible above the keyboard (see 8.3). Native `<input type="date">`
is used in two places (Pause date sheet, alarm Date sheet; with `min`): Slint has no native date picker, so build a small
month-grid date picker component (or call the SDK if one exists) and keep the `YYYY-MM-DD` contract. Range inputs: two
(volume, brightness). Pointer events with captured drag: time wheels, colour wheel, hold bubble, reaction targets, reorder.
File/photo pickers: `custom_sounds::pick_audio_file_native`, `ring_photos::import`, `media_picker::pick_files` (backup) all
block, so run them on a thread and post the result back to the UI thread.

### 8.7 SDK calls per screen
- Home: `platform::alarm_kit_optional()` (`next_fire`, `snoozed_until`), `persistence::save_*`, `health::report` (via reliability).
- Editor: `audio::play_as_alarm/stop`, `haptics::vibrate_pattern/cancel`, `permissions::request`, `scanner::scan`, `media_picker` (via
  helpers), `display::system_dark`, `display::uses_24_hour_clock` (via `text::uses_24_hours`).
- Settings: `health::report`, `home_widget::can_add/add`, `device_language`, `platform::set_ring_language/set_min_volume/refresh_outside_app/save_to_downloads`, `media_picker::pick_files`.
- Reliability: `health::report/open_settings`, `permissions::request/open_app_settings`, `battery::request_exemption`, AlarmKit `timing_log`.
- Ring: AlarmKit `session/current_session/pause/resume/snooze/mark_solved/dismiss/note_ui_up/note_activity`,
  `foregrounding::finish_activity`, `display::set_max_brightness/restore_brightness`, `jobs::ui_heartbeat`, `sensors::*`, `scanner::scan`, `haptics` (via `feedback`).
- App: `foregrounding::on_back/move_to_back`, `display::*` (bars, insets, dark), `keep_event_loop_awake`.

---

## Status (what is built, newest first)

Milestones are commits on branch `slint-port`; the SDK parts are on `native-host` of mobile-sentinel.

- **M4 (so far)** Home panels: sort (own/time/next ring), pause all (until tomorrow, a week, a date), alarm menu (skip next / undo skip, duplicate, pause until a date, end pause, use for new alarms, delete) with undo toasts; the toast moves to the top while a panel is open; panels scroll when long.
- **M3** `devtest/compare.sh`: APK size, cold start, PSS, SurfaceFlinger frame times, side by side with the original.
- **M2** Home list, editor (time wheel sheet, -1/-5/+1/+5, days and presets, label, sound with preview and "add your own", snooze), save, delete, discard guard, ring screen (clock, date, label, note, snooze, turn off), firing poll, heartbeat, history event on dismiss, wake-up check, one-time alarms switching off.
- **M1** Slint skeleton on a NativeActivity (`libmain.so`), `build.sh` with `build_sentinel` (at that time on the hand-made `android/` project, see "Generated host" below), theme from `Palette`, insets, system bars, back button, toast with undo, quick alarms, empty-state templates, alarm switch.

Known gaps right now: see "Gaps found by the audit" and the checklist below. Still open there: accessibility unverified (no TalkBack), the OS reduce-animations switch not seen working, kiosk / locked-boot / guardian paths on a phone, `devtest/all.sh` and `matrix.sh` rewritten for the debug channel.

### Comparing

`devtest/compare.sh` measures both apps on one emulator (`emulator-5556`, AVD `af31`). The original is built from a copy of
its source in `build/original` (never in `C:/Git/Alarmfree`):

    (cd build/original && TARGET=x86_64-linux-android ./build.sh release)   # dx needs ANDROID_NDK_HOME = ndk 28.2
    TARGET=x86_64-linux-android ./build.sh release
    devtest/compare.sh

It starts from clean data (the original's welcome page is passed with its "Get started" button), takes the median of 5 cold
starts (`am start -W`), PSS from `dumpsys meminfo` after the start and after opening the editor (plus the WebView helper
processes, which `dumpsys meminfo <pkg>` leaves out), and the time between frames of the app's layer from
`dumpsys SurfaceFlinger --latency` while a list of 14 alarms is scrolled (`dumpsys gfxinfo` sees no frames of either app,
because neither is drawn by Android's own renderer). An emulator without a real GPU says little about smoothness: read the
frame rows on a phone.

Run on the emulator (Android 12, x86_64, release builds, Slint at feature parity, commit 1ea9c8b plus the SDK on native-host):

    AlarmFree (WebView)  AlarmFree Slint
    APK size (MB)                    18.8   42.9
    Cold start, median of 5 (ms)     625    271
    PSS after start (MB)             86     62
    PSS after the editor (MB)        93     67
    WebView processes (PSS, MB)      43     0
    PSS app + WebView, editor (MB)   136    67
    Frames seen while scrolling      144    980
    Janky frames (> 25 ms apart)     84 (58.3%)   612 (62.4%)
    Time between frames p50 / p90    30.8 / 57.8 ms   32.8 / 38.4 ms
    Time between frames p95 / p99    79.0 / 79.0 ms   52.0 / 83.4 ms

How to read it: the emulator draws with a software GPU, so every frame row is dominated by that (both apps sit near 30 ms
here); size, cold start and memory are less affected. The Slint APK is bigger because Skia and the std widgets are linked in
statically (the original's WebView lives in the system). Earlier runs on the same device varied by a few hundred ms in cold
start and by tens of percent in jank, so compare only large differences, and repeat on the phone. The numbers before the
first-run page was handled by `compare.sh` (Slint scrolling the welcome page) were wrong and are not used.

## Parity summary

Audit of the checklist against the original (`2f65c82`) at `eb15e66`: 70 lines `[x]`, 26 lines `[~]`, 11 lines `[ ]` (the `[ ]` count
includes the gaps found by the audit). Every screen, sheet, setting and challenge of the original exists in the Slint port;
what is not at parity yet:
- Missing behaviour: navigation to History (Home for a hidden alarm) after a ring ends; wheel haptic tick (8 ms); "?" help on
  the Sort and Pause-all sheets; Android font-scale compensation of the big clocks; OS reduce-animations preference; less motion
  on the card swipe and reorder shift; `keep_event_loop_awake` call at start; accessibility labels and roles (none at all).
- Ported but never tried: Photo backdrop and the photo picker, real code scan, step counter, 180 s camera limit, typing to
  completion, real sound and vibration, max-brightness on the ring, DB-recovery and unseen-outcome toasts, validation error
  texts, the colour wheel with a finger, system dark switch, `hang_ui` guardian restart, auto-scroll while dragging a card.
- Not done at all: `devtest/all.sh` and `matrix.sh` scenarios (need rewriting against `ui:snapshot`), everything on a phone
  (kiosk ring path, locked-boot screen), tablet AVD, fmt/clippy/test re-run after the audit.

## 9. Port checklist (suggested order)

Order: A core loop (home list, editor basics + time picker, ring screen), then B challenges, C editor sheets, D the rest.
Legend: `[x]` ported and exercised on emulator-5556 (status sections below) or covered by tests; `[~]` ported, but part of it is
only compile-checked or not tried (the note says what); `[ ]` missing. Audited against the original (`2f65c82`) on branch
`slint-port` at `eb15e66`. Nothing here has been tried on a phone yet.

### Phase 0: foundations
- [x] Slint 1.18.1 app on Android (NativeActivity) starts, edge-to-edge, correct window background before first paint (`start_background`)
- [x] `AppCore/Store` loads/saves alarms and settings through the existing `persistence` (no format change)
- [x] Translator: JSON locales en/nl/es/fr/de, `t(key)` pushed to Slint, language switch live (no `@tr`) (German checked on all screens, step 4)
- [x] `Palette` global (`Pal`) with all 20 derived tokens from `AppSettings.palette(system_dark)`; light/dark/system bar colours set
- [x] Insets global done; font scale: `Zoom.scale` (from `display::font_scale()`, read at start and every 2 s in `settings::apply_motion`) multiplies every text size in `ui/*.slint`; the big clocks (editor time, ring clock) are left unscaled, which is what the original's division by `--text-zoom` came to; seen on the OnePlus at font_scale 1.3 (texts larger, clock unchanged); wide preset chips elide a bit more
- [x] Navigator with push / replace / go_back and the three-tab rule; first-run redirect when `!setup_done` (screen index + `editor_trail`; first run seen in step 4)
- [x] BackDispatcher (SDK `on_back` to UI thread): top handler first, else route rule, Home = move to background, ignored while ringing (`ui::back`)
- [x] Wide two-pane layout at >= 840 dp (list 400 + detail, empty-pane text), editor restarts per picked alarm (seen with a size override, no tablet AVD)
- [x] Toast controller: info 3 s, error 10 s, undo 6 s, tap dismiss, top placement while a sheet is open, replace restarts timer
- [~] Startup tasks: `init_platform`, feedback enabled, history prune (5 y), ring_photos prune, DB recovery + unseen outcome notices, 500 ms poll (save failure, ring-end news, firing state) are all in `ui::run`/`ring::poll`; the poll was seen working, but the DB recovered / volatile / unreadable toasts and the unseen-outcome toasts were never triggered; `mobile_sentinel::keep_event_loop_awake()` (called by the original at start) is NOT called in the port
- [x] Debug build command channel replacing `js:` (tap/type/dump) so `devtest/` can run; `snooze:`/`dismiss:`/`hang_ui`/`shortened` kept (ui: commands + devtest/slint_smoke.sh; the old matrix scenarios are not ported)

### Phase 1: core loop
Home
- [x] Header: next alarm title (day + time) and "in X h Y min" subtitle, "none on" state
- [x] Pause button (highlighted when all paused) and Sort button
- [x] Problem banner (first reliability problem, tap to Reliability, X to hide for hideable checks) (step 3)
- [x] Quick-alarm chips +5 +10 +15 +30 +60 (nap alarm, auto-delete, toast + undo, limit toast at 50)
- [x] Alarm list rows: time (24 h / 12 h), label, when-text priority (snoozed / paused / skip / today / tomorrow / date), day letters in week-start order (`when_text` is a copy of the original)
- [x] Enable switch per card with toast ("rings in ...", "switched off") and undo
- [x] Open editor by tapping a card; FAB new alarm (limit check); kebab opens menu sheet
- [x] Swipe card sideways to reveal Skip next / Undo skip and Delete (snap), delete with undo toast (seen: Delete reveal and snap; Skip action not tried by hand)
- [x] Long-press (450 ms) drag to reorder in Own sort, saves order, hidden alarms excluded, auto-scroll near edges (seen: lift, neighbours shift, drop saved; auto-scroll not tried)
- [x] Empty state with icon, texts and Weekdays 07:00 / Weekend 09:00 template buttons
- [~] Countdown and labels refresh on the 1.5 s / 15 s tick (timers in `ui::run` call `home::refresh`; the tick itself was not watched over a minute change)
- [x] BottomNav with active state, no stacking
Editor basics
- [x] Editor modes new / existing / copy-<id> (label "(copy)") / defaults (copy from the alarm menu "Duplicate")
- [x] Big time button + -1 -5 +1 +5 steps with wrap; "rings in" line
- [~] Time sheet: two drum wheels (drag, tap neighbours, wrap, 12 h AM/PM) and typed-time field (`text::parse_typed_time`, tested); haptic tick (8 ms) per wheel step through `feedback::tick`; wheel finger drag only tried with the emulator mouse
- [x] Day chips in week-start order, presets Weekdays / Weekend / Every day, "One date" mode with Date sheet and quick chips (the date sheet is a month grid, the original used the system date input)
- [~] Validation errors (select a day, date in the past), Save builds alarm (sanitize, order, enabled, skip cleared) and toasts "rings in": save was exercised, the two error texts were never triggered by hand (code is a copy of the original)
- [x] Unsaved-changes guard: Discard sheet on back / arrow / system Back
- [x] Delete sheet (existing alarms only) with undo toast
- [x] Summary rows list with all values and sheets wired (Label, Sound, Vibration, Snooze, Challenges, Ring look, Ring limit, Wake check, Notice, Max brightness switch)
Ring screen
- [~] Full-screen RingFace: date, live clock (5 s), label/fallback, note, insets, big-clock sizes; the clock is NOT divided by the system font scale (see Phase 0)
- [x] Clock styles Clean / Thin / Bold / Classic / Mono (one screenshot per style; fonts by name `Noto Serif` / `Droid Sans Mono`)
- [x] Snooze button (text with minutes and left; hides at 0; `kit.snooze`, finish activity when no other ring) (snooze in `devtest/slint_smoke.sh`)
- [x] Challenge hint ("N challenges" + one dot per challenge when more than one) or 48 px gap; Turn off: no challenges = mark solved + dismiss; else pause + run (step 5a)
- [~] Max-brightness handling (set / re-apply every 5 s / restore) and `note_ui_up`: written like the original (`ring::start`, `ring::poll`, `ring::stop`), the brightness itself was not observed on the emulator
- [~] Finish: history event, wake-check alarm, one-time switch-off done and seen; afterwards History (Home for a hidden alarm) opens like the original
- [~] Liveness: `ui_heartbeat` is called on the UI thread every 500 ms poll while ringing (the original did a JS round trip every 1 s); `hang_ui` (guardian restart on a frozen UI) was not run

### Phase 2: challenges
- [x] ChallengeRun shell: progress text, dots, Exit (practice), time-left bar (real), idle 60 s -> back to ring + resume, any touch resets
- [~] Idle clock also reset by shake/step sensor change (+ `note_activity`); scanner-open pauses idle; 180 s camera limit -> resume/pause (code only; the 180 s limit and the sensor resets were not exercised)
- [x] Celebration overlay 1800 ms (check pop, confetti 28 or 0 per setting, "i / total" label), haptic pattern on solve
- [x] Fallback link + confirm sheet -> 20 x 3 s holds (with back arrow), recorded as HoldButton
- [x] Math: keypad, auto-solve on match, max 12 digits, check key shows incorrect, 4 difficulties
- [~] Typing: per-character colouring, cursor, word wrap, accuracy readout, soft keyboard textarea, incorrect state seen; typing to completion not seen; no autocorrect-off flag in Slint
- [x] Memory: 3x3 / 4x3 grid, display phase timing, input rules, 3 wrongs replay, wrong-cell shake, progress/wrong text
- [x] Shake: accelerometer start/stop, 200 ms poll, progress ring (virtual sensor)
- [~] Steps: step counter start/stop, 250 ms poll, progress ring, ACTIVITY_RECOGNITION request (compile-checked only; the step sensor was never fed, status "not verified")
- [x] Hold: bubble random position with safe margin, press-and-hold fill, release cancel, pop, haptics, progress (haptics themselves not observable)
- [x] Reaction: countdown, 3 visible targets with spacing, replacement spawn, timer bar, time-out reset, rounds, haptics (haptics not observable)
- [~] Scan: camera scanner thread, states (ready / scanning / wrong / done), any-code vs expected, CAMERA request (screen and permission prompt seen; no real code was ever scanned)
- [x] Practice route and in-editor Try / Try all (no history, no AlarmKit), empty / done messages, Back = exit
- [x] ProgressRing component

### Phase 3: editor sheets
- [x] Sheet base: scrim, slide-up, grab handle, title, "?" help toggle, note, top-right action slot, Back closes
- [x] Label sheet: label + note fields (100), used-before chips
- [~] Sound sheet: radio list (No sound + 10 bundled + custom), Start/Stop preview (15 s auto stop), switch-while-playing (playback of the preview not verified by ear)
- [x] Sound sheet: custom sound rename (inline, empty removes), delete with inline confirm and fallback replacement everywhere, import via picker + error toasts
- [x] Sound sheet: volume slider 0-100/5, fade-in chips (Off, 15 s, 30 s, 1, 2, 5 min)
- [~] Vibration sheet: Off / Pulse / Steady / Heartbeat chips with immediate preview, sound delay chips (when sound + vibration), sound_mode rules (real vibration feel not verified)
- [x] Snooze sheet: interval stepper (1-60, step 1 to 10 then 5), count stepper (0-10)
- [x] Challenges sheet: rows with grip drag-reorder, number, name + summary, Try, remove, Try all, Add (max 10)
- [x] Add challenge sheet (8 types with descriptions, permission request on add)
- [x] Challenge detail sheet: difficulty chips + example (Math / Memory / Typing), steppers (Shake, Steps, Hold x2, Reaction x3), Scan code field + Scan now, Try, Remove, help text
- [x] Ring limit (30 / 60 / 90 / 120 / Forever), Wake check (Off / 3 / 5 / 10), Notice (Off / 5 / 10 / 30 / 60) sheets (a notice 10 minutes before a one-time alarm was seen)
- [~] Ring look sheet: 8 live backdrop tiles, colour row (Auto / dot), clock chips, Preview link seen; photo picker (+ other photo) never tried
- [~] Ring colour sheet: live backdrop, accent swatches, ColorControls with Auto chip (wheel tapped, never dragged with a finger)
- [x] Full-screen ring preview (tap or Back closes, fake buttons)
- [x] Defaults editor (no time/repeat), "Use as default" from card menu

### Phase 4: backdrops, themes, polish
- [x] Aurora backdrop (3 drifting radial blobs)
- [x] Sunrise backdrop (45 s sky + sun rise, orange default)
- [x] Waves backdrop (3 rotating rounded squares)
- [x] Stars backdrop (3 twinkling dot layers, text-band mask on ring screen)
- [x] Rings backdrop (4 expanding rings)
- [x] Bubbles backdrop (9 rising bubbles)
- [~] Photo backdrop (cover, slow zoom, scrim, forced white text) with unreadable-file fallback: written (`look::load_photo`, `photo-drift`), never shown because the picker was not used
- [x] Ring colour triad (+-35 deg neighbours, olive avoidance, sunrise exception) and halo text-shadow on dressed screens
- [~] Less-motion switch honoured by every animation (functional timers exempt): backdrops, wheels, sheets, switches, celebration use `Motion`; the card swipe snap (180 ms) and the reorder shift (150 ms) in `ui/home.slint` do not; the OS "reduce animations" preference is read (`display::animations_off()`), not checked visually
- [x] ThemeScreen: preview card, 9 theme tiles (System diagonal split), Your own row, Background / Cards / Text rows with Auto, accent swatches + wheel swatch
- [~] Colour editor: wheel (conic + radial, dim overlay, thumb), brightness slider, hex field, swatch presets, adjusted-contrast note, save on release (finger drag on the wheel not tried, only taps)
- [~] Palette derivation parity (contrast nudging, tokens) incl. system dark switching and system bar colours: derivation is the unchanged `models` code and bars were checked on Mist, Sand and a custom pink; the live switch when the phone goes dark (2 s timer in `ui::run`) was not tried

### Phase 5: remaining screens
- [x] History: period pills (7), custom days field, empty states, summary stats, streak, bar chart (week / month / year, current highlighted), expandable all stats (custom days typing not tried by hand)
- [x] History events: rows with outcome colours, expandable details, per-event delete confirm
- [x] History / Settings: Delete history sheet (30 d / 90 d / all) with confirm sheet and toast
- [x] Settings: reliability banner (problem count) and Theme row (step 3 and 4); General list (Language, Time format, Week start, Add widget) done
- [x] Settings: Alarms list (defaults editor entry, minimum volume), Feel switches (touch feedback, less motion, confetti)
- [x] Settings sheets: Language (follow phone + 5), Time format, Week start, Volume chips (live `set_min_volume`, not tried on a phone), About
- [x] Settings: Backup save to Downloads / restore from picker (limit, undo, errors)
- [x] Settings changes call `refresh_outside_app`; language change also `set_ring_language`
- [x] Reliability screen: 10 checks sorted, Fix actions per check, 1.5 s refresh, help text with last ring time, app settings, save ring record (Fix buttons not tried on a phone)
- [x] Reliability test alarm: add / pending note / cancel
- [x] FirstRun welcome screen (large title, intro, same list, Done sets `setup_done`)
- [~] Home sheets: Sort (3), PauseAll (end / tomorrow / week / date), PauseDate, AlarmMenu (Practice, Skip, Duplicate, Pause, Make default, Delete) all done and seen; the "?" help of the Sort, Pause-all and pause-date sheets is there
- [x] Date picker component replacing `<input type=date>` (min date support)
- [~] Toast outcome notices and error toasts shown as in 8.5 (code is a copy; see the Phase 0 startup line for what was never triggered)
- [~] Haptics through `feedback` (reorder 15 ms, hold 18/45/130, reaction 15/60/80/130, solve pattern, touch-feedback switch honoured) done; the wheel tick (8 ms) is there; haptics are not observable on an emulator
- [ ] Accessibility labels / roles on switches, chips (checked), progress bar, buttons: added for the shared widgets, home, settings, theme, reliability, history chart (see the gaps list); unverified without TalkBack

### Phase 6: verification
- [~] `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test` green (model/core tests untouched): ran before each commit per the working rules, not re-run by this audit
- [ ] Emulator `devtest/all.sh` scenarios pass with the new debug channel (only `devtest/slint_smoke.sh` passes; `matrix.sh`/`all.sh` still read the page with `js:` and need rewriting)
- [ ] Ring path on a phone: kiosk (Back / Home / Recents blocked), locked-boot backup screen, guardian restart on frozen UI
- [~] Tablet / fold width: split layout seen with `wm size` override; rotation and font-scale changes in place not tried

### Gaps found by the audit
Original behaviour with no line (or a wrong line) above, found by reading `2f65c82` screens against the port:
- [x] After a ring ends (turned off, challenge solved) the port goes to History (Home for a hidden alarm) like the original (`ui/ring.rs` `finished`); seen on emulator-5556: Turn off -> History with the new row
- [x] `mobile_sentinel::keep_event_loop_awake()`: not needed, so not called. It wakes the looper every 500 ms for the life of the process (a Dioxus/tao workaround). Tested on emulator-5556 after the system file picker closed: the picked sound arrived and was selected with no touch, and an error toast (10 s timer) disappeared on its own
- [x] Haptic tick (8 ms) per step on the time wheels (`editor::set_hour/set_minute`); the haptics themselves are not observable on the emulator
- [x] "?" help text on the Sort sheet (seen on emulator-5556) and the Pause-all sheet (`home.pause_note`, same code path, not opened by hand)
- [ ] Accessibility (added in code, NOT verified: the Slint Android backend shows no accessibility tree to uiautomator, so only a phone with TalkBack can confirm; swatches of the colour grids have no names yet): aria labels for every icon button, switch role/state, chart label (`history.chart_aria`), progress label (`challenge.time_left`), reliability ok/attention icons (`reliability.ok`, `reliability.attention`), theme code and custom accent (`theme.code`, `accent.custom`); screen readers read nothing useful
- [x] OS "reduce animations" preference: `display::animations_off()` (new, SDK native-host) is read at start and every 2 s and joins the in-app switch in `Motion.less` (not seen working by hand)
- [x] Less motion reaches the card swipe snap and reorder shift in `ui/home.slint` (`Motion.ms`)
- [x] Font scale: the app follows the phone's font size (`Zoom.scale`, see Phase 0); the big clocks stay fixed
Covered by shared, unchanged logic (not gaps): `HISTORY_KEEP_DAYS` prune and ring photo prune (`ui::run`), `give_notice_to_all` and reminder taps (`platform.rs`, run by `init_platform`), home-widget refresh (`persistence::save_*` and settings call `refresh_outside_app`), sound names, notify-before, ring photo import (`ring_photos::import`, error toast `ring.photo_failed`).


## Status: challenges and practice (step 1 of the port)
Done and seen on emulator-5556: the challenge run over the ring screen (idle bar, 60 s idle back to ringing, solve ->
mark_solved + dismiss + history row with the Math solve), practice from the alarm menu ("Try its challenges") and from the
editor (Try, Try all), the editor's Challenges row with the list sheet (grip drag reorder, Try, remove, Add), the Add sheet and
the detail sheet (difficulty chips, steppers, scan code field), Math (keypad, wrong, solve), Typing (colouring, accuracy, soft
keyboard), Memory (display, input, replay after 3 wrongs), Shake (virtual sensor), Hold (fill, pop, fallback), Reaction
(countdown, pop, round clear), Scan screen + CAMERA prompt, celebration with confetti. Logic: `src/challenges/` (pure, tested);
UI glue: `src/ui/challenge_run.rs`, `src/ui/challenge_sheets.rs`; Slint: `ui/challenges.slint`, `ui/challenge_sheets.slint`.
Not verified: a real camera scan (the emulator has no code to scan), steps sensor, the 180 s camera limit, the time-out reset
of Reaction was seen but the haptics are not observable, typing to completion. Differences: no autocorrect-off on the typing
field (Slint has no such flag), the backspace key is an icon, wrong memory cells shake via a Slint animation.


## Status: history and settings (step 2 of the port)
Done and seen on emulator-5556: bottom navigation between Alarms, History and Settings (back from History/Settings goes to
Alarms), History (period pills, custom days field, empty states, summary with streak, chart, all stats, event rows with
details and per-event delete confirm, delete-history sheets with confirm and toast), Settings (Language, Time format, Week
start, Add widget, New alarms defaults editor, Minimum volume chips, Touch feedback, Less motion, Confetti, Backup save to
Downloads and restore through the file picker with undo toast, Delete history, About). Language, time format and week start
change the open screens live (`Tr.rev` bump, `set_ring_language`, `refresh_outside_app`); Less motion zeroes the decorative
animations through the `Motion` global. Logic: `src/history_view.rs` (tested); glue: `src/ui/{history,settings,panel}.rs`;
Slint: `ui/{history,settings,panel}.slint`; the editor got a defaults mode (no time/repeat, saves to `prefs.alarm_defaults`).
Not done: Reliability banner and Theme row (steps 3 and 4). Not verified by hand: Less motion visually, Volume chips on a
phone, the widget row action, the custom days field typing. Rows use the original 14 px/54 px list style; the older `Row`
widget (16 px) used by the editor is unchanged.

## Status: editor sheets and reliability (step 3 of the port)
Done and seen on emulator-5556: every row of the editor in the original's grouping (Label, Sound, Vibration, Snooze,
Challenges / Alarm screen, Ring limit, Are you up?, Notice before, Full brightness) with all its sheets: label + ring note +
labels used before, time (wheels plus typed entry, `text::parse_typed_time`), date (month grid `ui/datepicker.slint`,
`calendar.rs`; also used by the pause-until-a-date panel, min tomorrow), sound (volume slider, fade-in, rename and delete of
own sounds with the fallback everywhere, import with both error toasts, preview), vibration (patterns, try, sound delay),
ring limit / wake-up check / notice, ring look (8 tiles, photo picker, colour row, clock styles), ring colour (swatches, wheel,
brightness, code, Automatic) and the full-screen preview. A one-time alarm saved for a picked date rang on that date and the
quiet notice posted 10 minutes before it. Reliability screen (`src/reliability.rs`, `src/ui/reliability.rs`,
`ui/reliability.slint`): all 10 checks, Fix per check, help, test alarm and cancel, app settings; banner on Home with Fix and
hide; Settings banner row; re-check every 1.5 s on the screen and every 15 s on Home. First-run mode exists
(`reliability::open(ctx, true)`) but is not started at launch: that is step 4.
Differences: labels used before are put in lines by a width estimate; the date picker is a
month grid, where the original used the system date input. Not verified: real vibration feel, sound playback of the preview,
photo picking, Fix buttons on a phone, hiding a warning, the colour wheel by finger drag.


## Status: theme, first run, language (step 4 of the port)
Done and seen on emulator-5556: theme screen (`src/ui/theme.rs`, `ui/theme_screen.slint`, screen 5, opened by the Theme row at the
top of Settings General, back returns to Settings): fixed preview card, 9 theme tiles (System split dark/light), "Your own theme"
row, Background / Cards / Text rows with Automatic, accent swatches plus the wheel swatch, and the colour sheet (swatches, wheel,
brightness, code, Automatic chip, "changed a little" note, Done; back closes the sheet first). Every change applies at once to
`Pal` and the system bars (status bar icons dark on a light theme, checked on Mist, Sand and the custom pink) and is saved on
tile / swatch tap, finger lift, valid code and sheet close. The wheel callbacks of `Hue` go to the theme screen while its sheet
is open, else to the editor's ring colour sheet. First run: started at launch when `!setup_done` (same as the original's NavWatcher);
"Get started" or back sets `setup_done` and goes to Home. The original asks for no permission at start, only from the Fix button
and challenges, which are ported. Language: after switching to German all of Settings, Theme, Home, editor and History re-read
and nothing clipped, except the Home banner text ("Akkusparen: Wecker koennten zu ..." elides). Not checked: fr/es/nl screenshots of
every screen, finger drag on the wheel (tapped only), the original's hard-coded colours in challenge screens (white text on photo).

## Status: ring screen and animated backdrops (step 5a of the port)
Done and seen on emulator-5556 (editor preview, one screenshot per backdrop and per clock style, and a real ring with Aurora, Mono and
two challenges): `ui/ring.slint` follows the original's `RingFace` (padding 56/20/24 plus insets, date 16, clock 96 or 64 for 12 h with
the five clock styles, label 28 bold, note, snooze, "N challenges" hint with one dot per challenge when more than one, 48 px gap when
none, 72 px Turn off). Dressed screens (Aurora to Bubbles, Photo) get the text halo, drawn as copies of the text in the background colour
(Slint has no text shadow). `ui/backdrop.slint`: the keyframes of the ring screen (aurora-one/two/three, sunrise-sky/sun, wave-turn,
stars-twinkle, ring-out, bubble-up, photo-drift) driven by one 50 ms `Timer` in `Backdrop` that only runs while a moving backdrop shows,
not with less motion, and not on the still tiles. The sunrise takes 45 s from when it appears. Stars now repeat the half-height pattern
twice like the original and keep out of the band behind the time on the ring screen and its preview. Less motion shows the still first
frame (sunrise: its end state, as the original's 0.01 ms animation). The preview and the real ring use the same `RingScreen`.
CPU on the x86 emulator (debug build, software GL): Aurora about 40 % of one core on the ring, near 0 once the app is not on screen
(snooze, or the ring left). With animation at the screen rate it was 100 %, hence the 20 frames a second.
Differences: easing is smoothstep / quadratic ease-out instead of CSS cubic-bezier; the Classic and Mono clock use the Android fonts
`Noto Serif` and `Droid Sans Mono` by name (the generic names did not resolve); the tiles in the ring look sheet stay still while the live
colour preview animates; the clock is not divided by the system font scale. Not verified: the Photo backdrop (needs the picker) and its
forced white text, less motion on a device, a phone.

## Status: remaining items (step 5b of the port)
Done and seen on emulator-5556 (`wm size 1800x2400` + `wm density 280`, reset afterwards):
- Swipe and drag on the list (`ui/home.slint` `AlarmCard`, `src/ui/home.rs`): one `TouchArea` per card decides tap (open / menu / switch by
  x position), sideways swipe (face follows the finger, snaps open or closed at half of the 64 dp actions, one card open at a time, Skip when
  `can_skip`, Delete) and 450 ms hold to lift the card (shadow, neighbours slide by one slot, drop calls `home::reorder`, which maps shown
  positions to `ordering::apply_reorder` as the original handler did). A drag is only possible in the user's own order (the original let
  you drag and then ignored the drop). Haptic tick 15 ms at lift. Edge auto-scroll is written (16 ms timer, 14 dp a step) but not tried.
- Wide layout: `MainWindow.split` = width >= 840 dp on Home and the editor: list in 400 dp, 1 px line, editor or the `home.pick` text on the
  right; the editor restarts for every picked alarm and back returns to the alarm before it, as the original's route stack did
  (`Ctx.editor_trail`). Differences: editor sheets cover only the right pane (the left pane is dimmed and blocked); a tablet AVD was not
  available, so a size and density override on the phone AVD was used.
- About: Slint's `AboutSlint` plus a licence line in the About panel (std-widgets compile on the Android build).
- Widget: the Settings row asked the launcher to pin the widget (logcat: `CONFIRM_PIN_APPWIDGET` started in the launcher); the widget
  text file (`sentinel/home_widget.txt`) held the next alarm and the smoke script checks it. The launcher dialog was not answered.
- Devtest channel: `ui:screen`, `ui:snapshot`, `ui:press:<callback>:<arg>` (documented in `src/debug_hooks.rs`, debug builds only,
  `src/ui/debug.rs`) and `devtest/slint_smoke.sh` (alarm, widget text, ring, snooze, ring again, turn off: ALL PASS on emulator-5556).
  The old `matrix.sh`/`all.sh` scenarios read the page with `js:` and every one of them needs rewriting against `ui:snapshot`.

## Scroll measurement note
`devtest/compare.sh` used to measure the first-run page, not the list: the Slint copy starts on the first-run screen, and its quick pill sits lower (reliability banner, "next alarm" sub line). The script now gets past the first-run page and taps the right pill, with 14 alarms on the list. On emulator-5556 (release, x86_64): before the shadow change 908 frames, p50 32.9 ms, 706 janky; after 1058 frames, p50 31.4 ms, 607 janky. A plain screen (Settings) scrolls with the same p50 of about 30 ms, so the floor is the emulator's input rate, not the list. A drop shadow on every card (blur 0 when not lifted) made each card an offscreen layer; the shadow now exists only on the lifted card. Idle: 0 frames in 4 s on Home.

## Phone comparison

Real phone: OnePlus 13 (ColorOS, Android 16, arm64), 1080x2376 at 480 dpi, panel up to 120 Hz. Release builds of both, arm64 only:
the original from a copy of its source (`build/original`, its stray x86_64 library removed from the APK so the size is fair) and the
Slint port at commit `1ea9c8b`. Measured with `devtest/compare.sh` (two full runs, 7 cold starts each; raw tables in
`docs/phone/perf-run8.txt` and `perf-run9.txt`). The phone refuses `pm clear` for adb, so the script reinstalls the two packages
for a clean state.

| | Original (WebView) run 8 / run 9 | Slint run 8 / run 9 |
|---|---|---|
| APK size | 18.6 MB | 44.1 MB |
| Cold start, median of 7 (`am start -W`) | 155 / 152 ms | 87 / 101 ms |
| PSS after start, main process | 123 / 110 MB | 67 / 68 MB |
| PSS with the editor open, main process | 124 / 123 MB | 74 / 74 MB |
| WebView helper processes | 58 / 58 MB | 0 |
| PSS, app + WebView, editor open | 188 / 187 MB | 80 / 80 MB |
| Idle CPU, 60 s on the home list (% of one core) | 0.40 / 0.37 | 0.22 / 0.20 |
| Display refresh while scrolling | 90 Hz (11.1 ms) | 60 Hz (16.7 ms) |
| Frames while scrolling (SurfaceFlinger) | 1230 / 1239 | 806 / 803 |
| Janky frames (more than 1.5 x vsync apart) | 3 (0.2%) / 3 (0.2%) | 12 (1.5%) / 13 (1.6%) |
| Frame interval p50 / p99 | 11.1 / 11.1 ms | 16.6 / 49.7 ms |

Frame source: SurfaceFlinger `--latency` of the app's layer, relative to the real vsync period. `dumpsys gfxinfo` reports about
1200 frames for the original (render time only, p99 about 10 ms) and none for Slint, which draws on its own GPU surface.

Verdicts on performance:
- **Start and memory: Slint wins clearly.** About 40% faster cold start, about 55% less memory in total (no WebView), and about
  half the idle CPU.
- **Size: the original wins clearly** (18.6 against 44.1 MB), because Skia and the std widgets are linked into Slint's library.
- **Scrolling: the original is smoother on this phone.** ColorOS runs the WebView at 90 Hz and Slint at only 60 Hz, and Slint
  has about 12 stalls of 33 to 99 ms per run where the original has 3. Both are fine in the numbers; the 60 against 90 Hz
  difference is visible by eye, which only the user can judge. This was then fixed: see "Refresh rate" below.

Oddities of the Slint build found while measuring (fixed in design round 1, see "Design fixes" below):
- The reliability banner on Home has no close cross when its text spans the full width, and it is one line with an ellipsis where
  the original's wraps over up to 3 lines.
- The "Fix" buttons on the welcome and reliability pages are as wide as their text, with no padding; the welcome page's intro text
  starts left of its heading and the card has no side margins (the original has both).

### Design fixes (before and after)

Round 1 of the design pass, checked on the phone (Dutch, default theme, 1080x2376 at 480 dpi). Each line: what changed, the
Slint capture before (`docs/phone/design/before/<screen>.jpg`) and the pair after (original left, Slint right,
`docs/phone/design/after/<screen>-orig-slint.jpg`).

Refresh rate: the Java hack in `MainActivity` is gone; `src/ui/mod.rs::run()` calls the SDK's
`mobile_sentinel::display::set_preferred_frame_rate(PreferredFrameRate::Highest)`. Scrolling 14 alarms on the phone
(`JANK_ONLY=slint JANK_FRESH=1 devtest/compare.sh`): display mode 120 Hz, vsync period 8.33 ms, 1249 frames seen, 7 janky,
p50 / p90 / p95 / p99 all 8.3 ms. Without the request (`perf-run8.txt`): 60 Hz, 806 frames, 12 janky, p99 49.7 ms.

Shared look (`ui/widgets.slint`, `components.slint`, `panel.slint`, `home_sheets.slint`, `main.slint`):
- Font: the original's `Roboto, system-ui, sans-serif` does not find a family called Roboto on ColorOS, so the WebView draws the
  system font (`SysFont-Regular.ttf`, family "OPPO Sans 4.0 EN"); Slint took Roboto. The window now sets
  `default-font-family: "OPPO Sans 4.0 EN"` (a phone without that family falls back to Slint's default).
- The "text 10% larger" came from Roboto's metrics and from sizes that were not the CSS ones: list rows were 16 px / 56 dp where the
  CSS has 14 px weight 500 / 54 dp, and `line-height` values (14/20, 12/16) were missing: Slint's `line-height-factor` is relative
  to the font's natural line height, so 20/14 is 1.22 and 16/12 is 1.14 here (font ascent plus descent is 1.172 em).
- Titles: screen titles 20 px weight 500 (were 700), sheet titles 20 px weight 500 (were 18 px 700).
- Sheet: `--surface` background instead of the page colour, radius 28, scrim 0.55, grab handle in `--off`, 20 dp side padding, title row
  44 dp starting 22 dp down, body from 74 dp, help text box gap, max height 90%. Round buttons and the in-sheet steppers use
  `--surface2` (on the raised sheet `--surface` would vanish); the stepper is 64 dp high with 14 px titles.
- Dividers: `Separator` (and Settings' `Inset`) are inset by 16 dp and take no room, like the CSS overlay line, so a row is 54 dp not 55.
- Focus ring on `TextField` (2 px accent outline, 2 px offset), the pause button when active is filled accent with `--on-accent`.

Per screen:
- Welcome page: Fix buttons have 14 dp padding, intro text starts at the heading, row and note heights as the CSS. `before/firstrun.jpg`, `after/firstrun-orig-slint.jpg`
- Home with the reliability banner and empty state: banner wraps to 3 lines, the close cross is inside the banner and always there
  when the check can be hidden, the empty clock has no filled circle, template buttons 44 dp, "Quick" label weight 500, next-alarm
  header line heights. `before/home-empty-banner.jpg`, `after/home-empty-banner-orig-slint.jpg`
- Home with an alarm (banner, quick row; the card itself is round 2): `before/home-alarms.jpg`, `after/home-alarms-orig-slint.jpg`
- Alarm menu sheet: raised surface, 20 px title, 52 dp items without the 8 dp gaps, danger and selected colours. `before/alarm-menu.jpg`, `after/alarm-menu-orig-slint.jpg`
- Settings: banner row spacing, section gaps (24 dp), list title height, inset dividers, 54 dp rows. `before/settings1.jpg`, `after/settings1-orig-slint.jpg`
- Reliability: card top margin 16 dp, `?` right next to the title, Fix buttons, note line height. `before/reliability.jpg`, `after/reliability-orig-slint.jpg`
- Editor (shared rows only): 54 dp rows with 14 px titles and inset dividers; days, time, presets are round 2. `before/editor-top.jpg`, `after/editor-top-orig-slint.jpg`
- Snooze sheet: sheet look and stepper. `before/snooze-sheet.jpg`, `after/snooze-sheet-orig-slint.jpg`

Still different after round 1 (round 2 below fixed these): the editor's days, preset chips, time digits and Save bar; the alarm card (time weight, weekday
letter spacing, row dividers); the time wheel, sound, vibration and ring-limit chips (the original wraps chips over rows, Slint has
one row); the Label sheet focuses its field at once and opens the keyboard (the original waits for a tap); the toast's wrapping;
the original's text is a little lighter on dark in some places by antialiasing only.

#### Round 2: the alarm list and the editor with its sheets

Checked on the phone (Dutch, default theme), both apps with the same three alarms (06:20 once tomorrow, 07:00 "Werk" on weekdays,
08:40 on the weekend). Each line: the Slint capture before (`docs/phone/design/before/<screen>.jpg`, from before round 1) and the pair
after (original left, Slint right, `docs/phone/design/after/<screen>-orig-slint.jpg`). Captures come from `devtest/r2.sh` (`capset o|s`).

Shared pieces (`ui/widgets.slint`, `components.slint`, `src/ui/chipflow.rs`):
- Chips wrap like the original's `.chips` flex row: `ChoiceChips` asks Rust (`ChipLayout.positions`, `chipflow.rs`) for the line, x and
  width of each chip. A line takes chips while the next one fits, and the room left on a line is shared out equally (`flex: 1 1 auto`,
  min 44 dp, 12 dp padding, 8 dp gap). Text widths come from the advances of the phone's system font at weight 500 (a table of 352 values;
  Slint has no way to measure text before layout). `small` is the editor's repeat presets (12 px text in a 30 dp pill inside a 44 dp touch
  place, 6 dp gap). The ring limit sheet now wraps "Forever" onto a second row, vibration, fade, ring clock, quick date, notice,
  wake-up check and the challenge difficulty chips use the same component. Unit tests in `chipflow.rs`.
- `Sheet` has a pinned main button (`done-text`, `finish`, `done-quiet`) that stays at the bottom while the rest scrolls, the same 52 dp
  button for every sheet; the action top right takes a filled play or stop icon (`action-icon`), `GroupTitle` has the original's 20 dp
  top margin, `PrimaryButton` has a `compact` form (48 dp, 14 px) for the buttons under a list.
- Radio rows: 14 px text (500 when picked), 20 dp mark with a 2 dp ring, picked has a 6 dp accent ring, 52 dp rows without gaps.
- Wheel: 96 dp wide panel on `--surface2`, 44 dp capsule in `--accent-soft`, the number picked in the accent colour at 28 px bold,
  neighbours 20 px, fading out at the top and bottom over 26 percent of the height (an overlay in the sheet colour instead of the
  original's CSS mask).
- Date picker: the month in a field like the others (page colour, thin line, radius 14), today with the soft accent, the day picked
  filled, with the "Date" label above like the original's date field; the pause-until-date sheet uses the same and a pinned button.
- Backdrop tiles: the sunrise on a tile starts at its dimmest, like the original's 45 second animation does when the sheet opens.

Per screen:
- Home, alarm card: dividers between cards from where the text starts, weekday letters 11 px in 13.2 dp cells, time digits a little
  lighter and wider (weight 450, +1.1 px: the original draws tabular digits), the pause and sort buttons on the right edge, the quick
  chips equally wide, 88 dp room under the list (was 120 plus the gesture bar). The snoozed card is unchanged: its text rule was
  already the original's `when_text` (label and state line both). `before/home-alarms.jpg`, `after/home-alarms-orig-slint.jpg`;
  swiped open `before/home-swipe.jpg`, `after/home-swipe-orig-slint.jpg`; a switched-off card `after/home-off-orig-slint.jpg`
- Home list scrolled to the end: 88 dp under the last card like the original, the + clear of it. `before/home-bottom.jpg`, `after/home-bottom-orig-slint.jpg`
- Alarm menu: unchanged since round 1, verified at the same spot. `before/alarm-menu.jpg`, `after/alarm-menu-orig-slint.jpg`
- Editor: 56 by 36 dp step pills, 64 px time (44 px in 12 hour) at weight 450, round 38 dp day buttons in 44 dp touch places,
  44 dp "once" button, repeat chips, 20 and 16 dp gaps between the parts, Save bar 12 dp above and 16 dp below the button on the page
  colour. `before/editor-top.jpg`, `after/editor-top-orig-slint.jpg`; `before/editor-bottom.jpg`, `after/editor-bottom-orig-slint.jpg`
- Time sheet: the wheel, `Type it` field. `before/time-sheet.jpg`, `after/time-sheet-orig-slint.jpg`
- Label sheet: the name field no longer takes focus by itself (a tap opens the keyboard, as in the original; the "used before" chips
  and Done stay in reach). `before/label-sheet.jpg`, `after/label-sheet-orig-slint.jpg`
- Sound sheet: Start with a play icon, 52 dp rows, pinned Done, compact import button. `before/sound-sheet.jpg`, `after/sound-sheet-orig-slint.jpg`
- Vibration sheet: "Right away" fits, play icon. `before/vibration-sheet.jpg`, `after/vibration-sheet-orig-slint.jpg`
- Snooze, ring limit, wake-up check and notice sheets: chips wrap, pinned Done. `before/snooze-sheet.jpg`, `after/snooze-sheet-orig-slint.jpg`;
  `before/limit-sheet.jpg`, `after/limit-sheet-orig-slint.jpg`; `before/wake-sheet.jpg`, `after/wake-sheet-orig-slint.jpg`;
  `before/notice-sheet.jpg`, `after/notice-sheet-orig-slint.jpg`
- Discard and delete sheets: a red plain item and the pinned button, like the original. `after/discard-sheet-orig-slint.jpg`
- Challenges sheets: pinned Done, compact add button, rows with dividers and room for a wrapping description, Back pinned above the
  gesture bar, difficulty chips wrap. `before/challenges-sheet.jpg`, `after/challenges-sheet-orig-slint.jpg`;
  `before/challenges-sheet-filled.jpg`, `after/challenges-sheet-filled-orig-slint.jpg`; `before/add-challenge-sheet.jpg`,
  `after/add-challenge-sheet-orig-slint.jpg`; `before/challenge-detail.jpg`, `after/challenge-detail-orig-slint.jpg`
- Ring look sheet: muted sunrise tile, smaller tiles. `before/ringlook-sheet.jpg`, `after/ringlook-sheet-orig-slint.jpg`
- Date sheet: see the date picker above. `before/date-sheet.jpg`, `after/date-sheet-orig-slint.jpg`

Still different after round 2: the date sheet shows a calendar where the original has the system date field (kept on purpose);
the animated tiles of the ring look sheet (Rings, Bubbles, Stars, Waves) show another moment of their animation than the original's;
the wheel's neighbours sit 40 dp apart where the original's perspective makes them about 43; chip widths are estimated from the
system font, so on another font a chip can be a few dp narrower or wider than the text wants (text is then cut with an ellipsis);
preset chips differ by a few dp in width from the original's real measurements.

#### Round 3: ring screen, challenges, settings, history, theme, toasts

Checked on the phone (Dutch, default theme, light theme where noted), both apps in the same state. Captures come from `devtest/r3.sh`
(`fin_a`, `fin_b`, plus the ring look preview, a real ring with two challenges in each app and a real ring without challenges).
`before/<screen>.jpg` is the earlier Slint capture of the design review (taken before round 1: round 3 had no separate "before"
state for screens that rounds 1 and 2 left alone), `after/<screen>-orig-slint.jpg` is original left, Slint right.

Shared pieces:
- `TabularText` (`ui/components.slint`, `Glyphs.split` in `src/ui/chipflow.rs`): every digit of a time sits in a cell as wide as the
  widest digit, like `font-variant-numeric: tabular-nums` of the original (Slint has no such feature). Used by the ring clock, the
  editor time, the alarm card time and the theme preview. The ring clock styles pass their own weight and spacing.
- Screen titles wrap over two lines instead of an ellipsis ("Settings for new alarms"), the bottom bar has three equal thirds,
  toasts wrap like the original and are 52 dp high with Undo, the Undo button has its 10 dp padding.
- `TextField` can centre its text (the colour sheet's hex field).

Per screen:
- Ring screen: the dark halo around the digits is gone (the original's `text-shadow` is a faint glow); it stays only on a photo.
  The hint under the snooze button ("2 challenges to turn it off" with dots, or the preview hint) is 14 px and 16 dp high like the
  original, so snooze and turn-off sit at the same height. Clock styles use the phone's serif and monospace families (the previous
  family names did not exist on the phone, so the serif and monospace styles drew in a wrong weight). `before/ring-hint.jpg`, `after/ring-hint-orig-slint.jpg`;
  backdrops `after/ring-aurora-orig-slint.jpg`, `ring-sunrise`, `ring-stars`, `ring-rings`, `ring-bubbles`, `ring-waves`;
  clock styles `after/ring-clock-thin-orig-slint.jpg`, `ring-clock-bold`, `ring-clock-classic`, `ring-clock-mono`
- Challenge screens: the practice header is 44 dp high with the Stoppen link (24 dp in a real run), the "Can't solve this?" link has
  its 8 dp margin, the memory grid its 8 dp margin, hold and reaction their 8 dp gap above the field, the typing field shows the
  accent focus ring, the add-challenge list has no gaps between rows (60 dp, like the original), the "Can't solve this?" sheet has a
  plain Cancel item. `before/practice-math.jpg`, `after/practice-math-orig-slint.jpg`; `after/practice-typing`, `practice-memory`,
  `practice-reaction`, `practice-hold`, `practice-shake`, `practice-end`, `cantsolve-sheet` (all `-orig-slint.jpg`)
- Settings: unchanged rows and sections (they already matched); the backup sheet's note is behind the `?` like the original, the
  "Settings for new alarms" title wraps. `before/settings1.jpg`, `after/settings1-orig-slint.jpg`, `after/settings2-orig-slint.jpg`;
  `before/backup-sheet.jpg`, `after/backup-sheet-orig-slint.jpg`; `before/new-alarms.jpg`, `after/new-alarms-orig-slint.jpg`
- History: title centred in its 44 dp row. Data, stats, detail and the delete sheet match. `before/history.jpg`,
  `after/history-orig-slint.jpg`; `before/history-stats.jpg`, `after/history-stats-orig-slint.jpg`; `before/history-detail.jpg`,
  `after/history-detail-orig-slint.jpg`; `before/history-delete-sheet.jpg`, `after/history-delete-sheet-orig-slint.jpg`
- Theme screen: tiles in rows that stretch with a wrapping label (a long name wraps over two lines), 54 dp rows with 14 px titles,
  the accent circles spread over the full width, "Accent colour" title spacing, the preview card with the original's time weight,
  day spacing and 33 dp pills. The preview card stays pinned: the original's `.theme-preview` sits outside its scroll area too (I
  checked it on the phone with an own theme, where the page scrolls), so it does not scroll away in either app. Own-theme colour
  sheet: hex field centred, 8 dp more room above Done. `before/theme.jpg`, `after/theme-orig-slint.jpg`;
  `before/theme-own.jpg`, `after/theme-own-orig-slint.jpg`; `before/colour-sheet.jpg`, `after/colour-sheet-orig-slint.jpg`
- Reliability: the last row ("This app in the phone's settings") wraps like the original. `before/reliability.jpg`,
  `after/reliability-orig-slint.jpg`; `before/reliability2.jpg`, `after/reliability2-orig-slint.jpg`
- Toasts: `before/toast-undo.jpg`, `after/toast-undo-orig-slint.jpg`
- Light theme: `before/light-home.jpg`, `after/light-home-orig-slint.jpg`; `before/light-editor.jpg`, `after/light-editor-orig-slint.jpg`;
  `after/light-settings-orig-slint.jpg`. A custom theme was only looked at in the theme screen and its colour sheet.

Still different after round 3: the colour sheet's brightness slider has a bigger thumb in Slint; the monospace clock style shows
the default sans in Slint (the phone's monospace family is not found by name); typing passages and sums are random, so those pairs
show different text; the reliability rows are a few dp taller or shorter per row than the original's; the original's hold and reaction
instruction line sits 4 dp higher; the animated backdrops show another moment of their animation; the home header "Fr 11:26" is
not tabular (only the clocks and cards are); the dark colour sheet is 17 dp lower than the original's.

### Refresh rate, and where the scroll jank comes from

Cause of the 60 Hz: a native window runs at the display's default rate unless it asks for more (a WebView page is raised by the system
itself). Asking for the fastest display mode of the same size (`preferredDisplayModeId`, the mechanism ColorOS honours) made Slint scroll at
120 Hz. It is now a generic opt-in of the SDK (`mobile_sentinel::display::set_preferred_frame_rate`, branch native-host, commit 852fe5c; off
unless an app asks) and the Slint build asks for `Highest` at start (`src/ui/mod.rs`).

| Scrolling 14 alarms on the phone | Refresh | Frames | Janky (> 1.5 x vsync) | p50 / p99 gap |
|---|---|---|---|---|
| Slint, request off (runs 8 and 9) | 60 Hz | 806 / 803 | 12 / 13 | 16.6 / 49.7 ms |
| Slint, request on | 120 Hz | 1249 | 7 (0.6%) | 8.3 / 8.3 ms |
| Original (runs 8 and 9, and a run under Perfetto) | 90 Hz | 1230 to 1430 | 3 / 3 / 0 | 11.1 / 11.1 ms |

Perfetto traces of the same workload for both builds (sched with thread names, gfx/view/input/binder atrace, CPU frequency, the frame
timeline): `docs/phone/jank-original.pftrace.gz` and `docs/phone/jank-slint.pftrace.gz` (about 15 MB each, not in git history:
they are kept next to the repo), scripts `devtest/jank_slint.py`, `jank_original.py`, `jank_phase.py`, config `devtest/jank.pbtxt`.
Findings:
- **Renderer:** Skia on OpenGL ES (Adreno 830), not Vulkan and not the software renderer (`eglSwapBuffers`, a GPU completion fence thread,
  SurfaceFlinger "RE GLES (Ganesh)"). Android's own renderer is not involved, which is why `dumpsys gfxinfo` shows 0 frames for Slint.
  Slint 1.18.1 offers `renderer-skia-vulkan`; nothing in the trace points to the GPU, so it was not tried.
- **Per-frame cost is small:** one frame is one `android_main` run of 2 to 5 ms on a performance core, mostly at the minimum clock
  (0.56 GHz; the same work takes about 1.9 ms at 2.4 GHz). No GC, no prime-core work, SurfaceFlinger on time for all 1628 display
  frames, GPU fence wait about 1 ms, `dequeueBuffer` at most 1.5 ms.
- **The "stalls" are late input, not slow rendering:** Slint draws only when something changes. Of 21 gaps over 12.5 ms in 1624 swaps,
  10 sit at a swipe start (35 to 42 ms after DOWN: the first MOVE of `input swipe` arrives that late and there is nothing to draw
  before it), 6 are one late MOVE (a 16.5 ms gap between two MOVEs) and 2 are idle time between swipes. Only 3 frames were real UI
  work: the first swipe after start (86.7 ms, 21.5 ms of work at 0.56 GHz), one release frame (26 ms, a 24.7 ms run on one core at
  1.56 GHz: cause not identified, first use of a glyph or shader is a guess) and one 19.5 ms frame. The original keeps a
  `Choreographer` frame going on every vsync while a finger is down, so it hides the same input gaps; its five worst frames were
  15.3 to 18.3 ms present gaps with 2.3 to 5.7 ms of UI and 3.4 to 4.9 ms of RenderThread work, sleeping 10 to 12 ms on late input.
- **Fix proposals:** (A) redraw on every vsync while a touch is down (animation-tick on the pressed list): hides the gaps in the metric by
  drawing identical frames, costs battery, no visible gain: not recommended. (B) an ADPF performance hint session for the UI thread
  to avoid the 0.56 GHz clock on the first frames (the only candidate for real jank; not done). The 24 ms release spike and the 87 ms
  first swipe would need simpleperf to explain.
- **Verdict:** with the 120 Hz request on, Slint's scrolling is at least as smooth as the original's by the numbers. Whether it feels
  so is for the user to judge.

### Generated host: before and after

The app no longer has an `android/` folder. `sentinel.toml` says `generate = true` (mobile-sentinel, branch `native-host`): `build_sentinel` writes the
whole Gradle project (AGP 8.7.0, manifest, theme, R8 keep rules) into `target/sentinel-android/` on every build. What is left of the app besides
Rust is `sentinel.toml` and `assets/`. `build.sh` builds `libalarmfree.so` with cargo, stages it as `build/jniLibs/<abi>/libmain.so`, keeps the
unstripped copy as `build/libmain.so.unstripped` (for crash symbols) and runs `build_sentinel` without `--project`. The activity is the platform's
`android.app.NativeActivity` (no Java class any more): `ALARMFREE_ACTIVITY_FQCN` in `src/platform.rs` and the devtest scripts name it.
Release builds run R8 and resource shrinking, and AGP strips the library (the NDK version comes from `ANDROID_NDK_HOME`, 30.0.16248370).

Measured on the OnePlus 13 with `devtest/compare.sh` (7 cold starts, release, arm64, refresh request on, 120 Hz), before = the APK of `526c27f`
(`android/` project, no R8, unstripped library), after = generated host. Same phone, same run order, same script.

| | Before | After |
|---|---|---|
| APK | 47,322,251 B (45.1 MB) | 28,956,410 B (27.6 MB) |
| `lib/arm64-v8a/libmain.so` | 38,302,312 B, has `.symtab` | 23,540,888 B, no `.symtab` |
| `classes.dex` | 3,933,520 B | 420,592 B |
| Cold start, median of 7 (`am start -W`) | 96 ms (82 to 101) | 93 ms (87 to 117) |
| PSS after start, main process | 70 MB | 69 MB |
| PSS with the editor open, main process | 72 MB | 71 MB |
| PSS, all processes of the app | 78 MB | 75 MB |
| Idle CPU, 60 s on the home list | 0.20% of a core | 0.22% |
| Scrolling 14 alarms (SurfaceFlinger frames) | 1249 frames, 6 janky (0.5%), p50/p99 8.3/8.3 ms | 1000 frames, 2 janky (0.2%), p50/p99 8.3/8.3 ms |

The size gain is the stripped library and R8 (dex 3.9 MB to 0.4 MB); the old build had neither. Start, memory and
scrolling are within the noise of the script (the original WebView build measured in the same two runs: 161/159 ms, 191/196 MB, 0 janky).
Raw tables: `docs/phone/generated-host-before.txt` and `generated-host-after.txt`.

Manifest, old template against the generated one (checked in the merged manifest inside the APK with `aapt2 dump xmltree`):
- Same: `hasCode`, `supportsRtl`, `enableOnBackInvokedCallback`, label, `AppTheme`, `launchMode=singleInstance`, `showWhenLocked`, `turnScreenOn`,
  `windowSoftInputMode=adjustResize`, the whole `configChanges` list, `android.app.lib_name=main`, icon, portrait lock, direct boot.
- Different: `extractNativeLibs` is now `false` (the library is loaded straight from the APK; was `true`), and `allowNativeHeapPointerTagging=false`
  is gone (the old template carried it from the start with no recorded reason). Both are untested beyond the smoke test below.
- Generator bug found and fixed in the SDK (`3da1e8c` on `native-host`): the generated manifest named `launchMode="singleTask"` and `build_sentinel`
  then added `singleInstance`, a duplicate attribute that broke the manifest merge. The generator now leaves `launchMode` to `build_sentinel`.

Smoke test on the phone with the release build: first-run page, editor (label, sound picker, "no sound, only vibration"), save and enable, ring
(the ring screen opens over the launcher), snooze (5 min), the ring again after the snooze ("nog 2"), turn off, a Math challenge alarm solved at the
ring (history shows it), edit and save, settings backup written to Download (removed afterwards), widget row (the launcher's pin dialog came up and was
dismissed with Back), the camera permission prompt for the Scan challenge (denied with Back), Back out of sheets, editor discard sheet and the home
list, Back at home, reopen, force-stop and cold reopen with the alarms kept. `logcat` held no `ClassNotFoundException`, `NoSuchMethodError`, `No
implementation found` or `UnsatisfiedLinkError` for the app. Unverified: a ring over a locked screen, a reboot (direct boot), the widget actually on a
home screen, a real scan, and everything the user judges by eye or ear.

### Stability (phone, about 07:50 to 08:36, phone clock, alarms 2 to 4 minutes ahead)

| | Original | Slint |
|---|---|---|
| Crashes (`logcat -b crash`, dropbox crash/ANR/native, our packages only) | 0 | 0 |
| ANRs | 0 | 0 |
| Frozen UIs | 0 | 0 |
| Rings that did not start or came late | 0 | 0 |
| Real rings | about 7 | about 9 |

Core loop run at least 3 times per app: create and edit an alarm, ring, snooze, ring again, turn off, a Math (Easy) challenge solved at
the ring, practice mode from the editor and from the menu, History, Settings, Theme, Back through every screen to the background,
reopen, `am kill` and reopen. Every ring started at the scheduled minute plus 0 to 5 ms in both apps (logcat `AlarmManager sending alarm`).
An alarm scheduled before `am kill` still rang on time afterwards in both apps, and both the `:sentinel` and the main process came
back (`am kill` is a background kill like the system's; it does not cancel alarms like `force-stop` does, which is why it was used instead
of swiping recents). Both apps stay portrait with rotation forced to landscape. With the phone unlocked and the launcher in front, a
ring after a kill does not take the screen in either app: it waits until the app is opened. Not tested: lock-screen unlock behaviour
(the phone had to stay unlocked), the locked-boot backup screen, the guardian restart on a frozen UI. Differences seen in the Slint
build: its kiosk logs "relaunch dispatched" about every 1.3 s while another ring is in front (two alarms in the same minute; both
rang, no defect found), the card keeps the label next to the "Snoozed until" line like the original's rule says.

### Design comparison and verdicts

53 pairs were captured on the phone (original and Slint, English, system theme which is dark here): `docs/phone/design/` (paired
images, `overview.jpg`), the fixes and their before/after pairs are listed under "Design fixes (before and after)" above (rounds 1 to 3).
Verdict per area after the fixes, judged from the pairs: shared look (font, sizes, titles, sheets, dividers) matches; Home, alarm
card, menu, swipe: matches; editor and all its sheets: matches, with Slint's own calendar kept for the date sheet; ring screen and all
backdrops: matches (Aurora a little stronger than the original's); challenges and practice: matches; Settings, History, Theme,
reliability, toasts: match. Differences that remain on purpose or are small: the date sheet calendar, animated look tiles showing
another moment, the wheel neighbour spacing (40 against 43 dp), chip widths come from a table of the OPPO Sans advances (another
font can be a few dp off), the phone-specific font family name (on another phone Slint uses its default), the colour sheet's slider
thumb, the monospace clock style falling back to the default sans, the home header not tabular.

Test yourself on the phone: how smooth scrolling feels at 120 Hz, the feel of the time wheel and reorder, haptics (wheel ticks,
challenges), alarm sound and fade, the camera scan and step challenges (the step permission was never granted during the tests),
TalkBack (roles and labels exist but only TalkBack can confirm them), the lock-screen unlock path, and whether the OPPO Sans text feels right.

## Emulator suite (devtest) on Slint, status 2026-11-14

Branch `slint-devtest`. `devtest/matrix.sh`, `all.sh`, `panels.sh` and `lib.sh` now drive the Slint app: the debug
build made by `./build.sh` carries the cargo feature `devtest` (`src/ui/debug_elements.rs`: texts with places,
roles and on/off state from Slint's element tree via `i-slint-backend-testing`, typed keys, in-window taps, boxes of a
component) next to the existing `ui:` commands (`src/ui/debug.rs`: snapshot with palette, look, insets; more
callbacks by name for buttons that are only pictures: the sound rename and delete, accent swatches, the trash can,
the history's delete, challenge removal). Real touches are `adb input`. See README "Tests" for the use and for which
scenarios passed on the last run. Findings:

- The suite found no app bug so far. Script-side things it taught: the last rows of Settings sit under the bottom bar
  until scrolled, the editor and its panels scroll, a panel slides in and takes touches only once it holds still,
  the icon-only buttons have no accessible label, the first touch in the first seconds after a start can be lost,
  and a text typed with `adb input text` loses what follows a space on this emulator.
- Open: s56d/s56e (screen off during a challenge: afterwards the lock screen has the focus, not the ring screen;
  the Dioxus line was not compared, so it is not yet known whether this is the script or the app), s55 (rewritten
  for the camera that closes itself, not yet run), s58, s48.
- Never run: s13 to s13d (PIN restart), s33, s57, s60 to s63, `big_screens`, s60p_*, s62p, and a timed full `all.sh`.
- `release` and `fast` builds do not pass the feature; `debug_hooks` is `cfg(debug_assertions)` only.
