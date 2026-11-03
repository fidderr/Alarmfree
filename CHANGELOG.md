# Changelog

All notable changes to **AlarmFree** are documented here.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project aims to adhere to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Changed

- **Rings first, from its own process.** The sound starts from a plan made when the alarm was
  armed, with no UI and no Rust. Measured on a real phone with every process dead: within 200 ms
  of the alarm's time.
- **New design**: compact cards, an editor that fits on one screen with everything else one tap
  away in a sheet, one set of sizes and colours, half the CSS.
- Back does the same on every Android version: closes a sheet, leaves a screen, and on Home goes
  to the background instead of ending the app.
- Your files live in device-encrypted storage, so an alarm rings before the first unlock. Alarms
  from before this version start over once.
- Needs Android 12 or newer, on a 64-bit phone. The download went from 10.4 to 9.4 MB.

### Added

- Ring limit per alarm: 30 to 120 minutes or forever, which starts over when you do something.
- Native backup screen for when the alarm's own screen cannot come up.
- Reliability screen with what the phone allows and a way to each setting, a first-run version
  of it, and a test alarm through the real ring path. The record of how fast rings were can be
  saved, for someone with a phone nobody here has.
- Quick alarms for a nap, skip the next ring from the card, pause one or all until a date, sort
  by time or next ring, swipe to skip or delete, Undo, templates when the list is empty.
- Wake-up check, note while ringing, full brightness, vibrate first, three vibration patterns,
  minimum volume per alarm, fade in up to 5 minutes, try the alarm for 10 seconds.
- Presets and random order for challenges. Typed time, 12-hour wheels, a date picker with quick
  choices. What new alarms start from can be set up.
- Your own sounds can be renamed, and keep the name they came with.
- From an alarm's menu: use it for new alarms.
- History: outcomes (missed, stopped at the limit, phone restarted, backup screen), streak, chart,
  delete one, and it starts on the day your week starts on.
- Themes System and Pure black, five accent colours, language following the phone, 12 or 24
  hours, first day of the week, less motion, confetti and touch feedback switches.
- Home-screen widget, a quiet notification before an alarm, alarms to a file and back.
- Tablets and foldables: the list and an alarm side by side.

### Fixed

- The screen could stand still until touched after coming back from another one, such as the
  camera, with everything behind it waiting too, also what ends an alarm after a challenge.
- The keyboard no longer covers what you type in: on Android 15 and newer the window does not
  shrink for it.
- The button at the bottom of a panel no longer covers its last line, such as Discard, and a
  message no longer covers a panel's buttons: it moves to the top while one is open.
- Scanning again right after cancelling a scan no longer loses what was scanned.
- An alarm no longer waits behind what else the phone is doing, as just after it started.
- No banner over the alarm's screen when it rings while you use the phone: it hid the date for some
  seconds.
- Opening the camera for a scan silenced an alarm for as long as it stayed open, which was a way
  out. After 3 minutes it rings again, with the camera still up to scan.
- A code scanned after the camera had been open for a minute was ignored, and so was a file picked
  after a few minutes. Now you can take your time, also to walk to the code in another room.
- Deleting an alarm from its editor can be undone, as it can from the list.
- One word for one thing: phone, turn off, try. "In 3 d 12 h" instead of "in 3 d 12 h 46 min".
- On a narrow phone the day letters of a card no longer run under its menu.
- A snoozed alarm rang with the process dead instead of being reported as stopped by a restart.
- A one-time alarm rang with the process dead instead of being switched off unheard.
- Reopening the app or saving another alarm no longer cancels a snooze.
- A restart no longer loses an alarm that came due: under 15 minutes late it rings, later it is
  reported as missed.
- Dark mode, a font change or folding the phone no longer restarts the app in the middle of a ring.
- A sound that cannot play falls back to another, and in the end to vibration, instead of silence.
- Two alarms at once: dismissing one keeps the other loud.
- The volume keys and the system panel can no longer silence a ring.
- Typing: capitals and a skipped letter no longer fail you, and the words are in your language.
- Two shake or step challenges in a row switched the sensor off for the second.
- A one-time alarm whose date has passed can be switched on again, and stays on.

### Removed

- The rate limit after wrong answers, which never stopped anybody and only wasted time.
- Android 8 to 11 and 32-bit phones.

## [0.2.1] - 2026-10-16

### Changed

- Built on **mobile-sentinel 0.2.1**: a ringing alarm now comes back after an
  OEM "swipe-up cleaner" kills the app (watchdog alarm + service relaunch),
  Back can no longer close the alarm on phones without predictive back, real
  alarms use `setAlarmClock`, and closing the app no longer logs a native
  crash.
- Layout respects the system bars on Android 15+ (edge-to-edge): nothing hides
  under the status/navigation bar; dark status-bar icons in the light theme.
- Long alarm labels and wrong-answer messages wrap instead of overflowing.
- About shows the real app version (was hard-coded `v0.1.0`).
- Smaller and lighter: release bundle 24.3 MB → 12.9 MB (size-optimised
  release profile, stripped `libmain.so`, sounds re-encoded to Ogg Vorbis,
  native code no longer extracted on install); the firing check reads the
  alarm store only when it changed; one shared timer instead of a thread per
  sleep.
- A database that can't be read is kept as `alarmfree.db.corrupt-<time>` and
  the user is told, instead of silently starting empty.

### Fixed

- An alarm whose challenges are all unknown types no longer traps the user.

## [0.1.2]

The first documented baseline of the app. Everything below describes the surface
as it stands today; subsequent releases will track changes against it.

### Added

- **Challenge-based dismissal** — an alarm can't be dismissed until its
  challenge gate is solved. Up to 10 challenges per alarm, run in order, with
  per-challenge solve time recorded.
- **Eight challenge types:**
  - **Math** — arithmetic on a keypad (Easy / Medium / Hard / Extreme).
  - **Memory** — reproduce a lit-cell sequence on a 3×3 or 4×3 grid.
  - **Typing** — copy a passage; length + min accuracy scale with difficulty.
  - **Shake** — shake the device a set number of times.
  - **Steps** — walk a set number of steps (hardware step counter).
  - **Scanner** — scan any barcode/QR, or a specific saved code.
  - **Hold** — press-and-hold a bubble until it fills, repeated.
  - **Reaction** — pop targets against a timer, across several rounds.
- **Scheduling** — weekday-repeating, one-time, or specific-date alarms; the
  mode is derived from the day selection.
- **Snooze** — 0–10 snoozes, 1–30 minute intervals, configured per alarm.
- **Sounds** — 10 bundled tones plus user-imported custom sounds (mp3 / m4a /
  ogg / wav), with in-app preview.
- **Sound modes** — sound + vibrate, sound only, or vibrate only.
- **History** — per-firing `AlarmEvent` records and aggregate metrics (reaction
  time, dismiss time, per-challenge average by type+difficulty, snooze counts /
  intervals / total time snoozed); filter by week / month / year / custom range,
  bulk-delete by age.
- **Anti-kill defense** — kiosk lock-task, foreground service, `:sentinel`
  job-guardian resurrection, exclude-from-recents, full-screen intent.
- **5 languages** — English, Dutch, Spanish, French, German — with every visible
  string routed through the i18n layer (embedded via `include_str!`).
- **3 themes** — dark, light, colorblind.
- **Fully offline** — no network, no CDN, inline SVG icons, bundled sounds,
  embedded translations.
- **Built on mobile-sentinel** — uses the `alarm-kit`, `scanner`, `haptics`,
  `audio`, `overlay`, `permissions`, `sensors`, `foregrounding`, and
  `media_picker` features. `src/platform.rs` is the single glue file that
  installs AlarmKit; the UI drives only AlarmKit lifecycle methods.
- **Persistence** — SQLite (`db/`, schema v1) for the alarm list, settings, and
  history; the SDK's `ContextStore` owns the alarm runtime state.
- **Documentation** — rewritten `README.md`.

[Unreleased]: https://github.com/fidderr/alarmfree
