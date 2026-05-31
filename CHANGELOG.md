# Changelog

All notable changes to **AlarmFree** are documented here.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project aims to adhere to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

Pre-1.0, the SQLite schema ships as a single clean migration — no back-compat
shims. Uninstall/reinstall on a schema change.

## [Unreleased]

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
