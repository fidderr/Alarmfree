# Changelog

All notable changes to **AlarmFree** are documented here.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project aims to adhere to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- **Eight more languages:** Italian, Portuguese (Brazil), Swedish, Danish, Norwegian (Bokmal), Finnish, Turkish and
  Indonesian, thirteen in all, with typing words for each and a store listing each. The phone's Norwegian (`nb`, `no`) and
  Indonesian (`id`, `in`) settings map to them. The texts were translated without a native reader, so some wording may be
  off. Store screenshots and graphics for them are not made yet.
- **Edit lock.** Per alarm (Edit lock in the editor: off, 1, 5, 10, 15 or 30 minutes, 1, 2, 3, 6 or 10 hours), the last
  stretch before the alarm rings it cannot be switched off, saved with a change, deleted, skipped or paused (pause all is
  refused while one is locked), and the Skip on its reminder does nothing. A lock mark shows on the card. Doing the alarm's
  challenges (an alarm without any asks for 20 holds of 3 seconds) opens it for 5 minutes; the open state is kept in memory
  only. Duplicating, rearranging, exporting, importing and Undo are not blocked. It cannot stop force-stopping the app,
  clearing its data or uninstalling it. Quick alarms, wake-up checks and the test alarm never lock.

### Changed

- **Shorter ring limits.** An alarm can stop by itself after 1, 3, 5, 10, 15 or 30 minutes too (the shortest was 30).

## [0.6.0] - 2026-11-20

### Added

- **Android 10 and 11.** The app now runs on Android 10 and newer (it needed Android 12). Tested on Android 10, 12 and 16
  emulators. Needs mobile-sentinel 0.7.0.
- **Challenge haptics.** Beating any challenge plays a strong, rising three-pulse pattern, so you know it worked without
  looking. Each counted squat gives a soft tick. Both follow the touch feedback setting.
- **Squat challenge.** Squat with the phone held steady in both hands; the SDK's motion detector counts the squats (5 to 100,
  10 by default). It is in the challenge list, the try-out, the history timings and backups, in all five languages, and the usual
  "Can't do this one? Hold instead" still swaps it for 20 holds. The detector's limits are first guesses, to be tuned with real
  recordings: `devtest/squat_record.sh` records them on a phone. Moving the phone straight up and down with the arms, without
  turning it, is counted too (a known limit). Needs mobile-sentinel 0.7.0.

### Changed

- **The notice before an alarm is now a reminder.** Per alarm: off or on, how long before (5, 10, 15,
  30, 45 minutes, 1 to 12 hours, 16 or 24 hours), a text of your own (up to 120 characters; empty
  shows the alarm's label as before), and silent or with the notification sound. Skip and Undo stay
  on it. An old notice becomes a silent reminder of the same length, also from a backup file; the
  "for all alarms" setting keeps working. Needs the next SDK (`ReminderSound` on `set_reminder`).
- **Sheets slide up** from below the screen when they open.

### Fixed

- The "rings in" text for a time that falls in the daylight saving gap shows the first moment after the gap.
- The theme card's day letters shrink at large display sizes (640 dpi), so the switch next to them fits.

## [0.5.3] - 2026-11-19

versionCode 12403. Same app as 0.5.2; only the Play listing reach changes.

### Fixed

- The app no longer requires a portrait screen (`android.hardware.screen.portrait` is optional), so
  Play offers it on devices without one, such as Chromebooks, Android XR and some tablets.

## [0.5.2] - 2026-11-16

Accessibility with TalkBack, versionCode 12402. The UI now runs on a Slint fork
(github.com/fidderr/slint, pinned revision) that adds the Android accessibility tree, and on an
AccessKit fork for hidden nodes and announced state changes; the three patched crates in `vendor/`
are gone (their changes are in the fork).

### Added

- Every button, switch, chip, tile and alarm card has a name and a role; an alarm card reads as one
  item ("07:00, Work, Weekdays"); the editor's day buttons carry the full day names.
- Sheets, panels and the ring screen are modal for screen readers: what is behind them is not reachable.
- Messages such as "Alarm deleted" are announced (a live region that stays in the tree), and Undo is a button.
- A Cancel button on the delete-history chooser.

### Changed

- Lists refresh in place, so a screen reader keeps its focus when the list is redrawn.
- Alarm labels are trimmed when saved.
- A word being composed by the keyboard no longer shows over a field's placeholder.
- Devtest: accessibility-tree host tests, a label dump per screen, a focus guard for phone runs, and
  the Maestro release smoke flow.

## [0.5.1] - 2026-11-15

The first Play release of the Slint app (0.5.0 was not uploaded), versionCode 12401, on
mobile-sentinel 0.6.0 from crates.io.

### Fixed

- With a large display size or text, or a narrow screen, the alarm card no longer runs past the right
  edge (the on/off switch was cut off), and the editor's weekday circles and settings card fit the
  screen: they shrink with the window instead of keeping fixed minimum widths.
- A tap on a sound in the sound list plays it for a few seconds again, as before; another tap takes
  over, Start/Stop plays it longer, and closing the list stops it.

### Changed

- Slightly smaller side margins on every screen (16 dp to 12 dp), so less space is wasted.

## [0.5.0] - 2026-11-14

The app is now the Slint version (drawn natively with Skia in a NativeActivity, no WebView), under
the Play app's id `com.fidderr.alarmfree`: it installs as an update over 0.4.1 (versionCode 12400)
and keeps the alarms, settings, own sounds and history. Built on mobile-sentinel 0.6.0 from this
workspace (path dependency until that crate release is published).

### Added

- A short intro of three cards (what the app does, the challenges, why the setup matters) before the
  first-run setup, with Skip on every card, and "Show intro again" in Settings.
- Settings, "Help and feedback": a Help page (why an alarm might not ring, per phone maker; challenges and the
  backup challenge; backup and restore; what the app collects), "Rate AlarmFree" (opens the Play listing, no
  prompt), and "Report a problem or idea".
- Report a problem or idea: type, what happened, what was expected, roughly when (filled in with the last alarm
  that rang), the diagnostic log and, if chosen, the alarm setup file; "Show what is sent"; sends with the user's
  own email app (the report is saved to Downloads when there is none). Nothing is sent by the app, which still
  has no internet permission. `docs/BUG_REPORTS.md` explains how to read a report; `devtest/repro.sh` rebuilds
  the situation on an emulator.
- A diagnostic log of events (start, alarms saved, enabled and deleted with time and repeat only, scheduled,
  ring started with lateness, ring stopped, missed, re-armed, permissions, restarts, panics). Never an alarm's
  name or note. Two files of 256 KB.
- A banner on Home, once, after a panic or a missed alarm: "Something went wrong last time. Send a report?".
- Screen reader labels on the ring screen's buttons, the number keys, the memory cells and two challenge links.
- `store/make_captions.py`: the store screenshots with a caption each, in five languages.
- `store/production-access-answers.md`: the answers for the Play production-access questionnaire.

### Changed

- The privacy page describes reports, and no longer lists an internet permission the app does not have.
- At a large font size (1.3), the repeat presets in the editor and the next-alarm title on Home no longer cut
  their text short.

### Fixed

- Back on Android 14 and later: Slint's own back callback won over the SDK's, so Back on the
  alarm list or in a sheet left the app. It is no longer registered (`vendor/`).
- A touch on a ringing alarm's screen, or in its challenge, counts as activity for the ring limit
  (the platform's touch hook does not see a NativeActivity's touches).
- Android 14 and later keep the task through Home; the window is made again. The old surface is
  released when the window ends, instead of when the new one arrives.

## [0.4.1] - 2026-11-11

Built on mobile-sentinel 0.5.0.

### Changed

- **A phone that restarted and was not unlocked yet**, as after an update in the night. The alarm
  rang there on a screen that asked for 20 long holds, which had nothing to do with the alarm's
  own challenges. Now a screen says the phone has to be unlocked, with one button for that. The
  ring is quiet for a minute after the button, and with the phone unlocked the alarm's own screen
  is there. Still locked after that minute, it sounds again.

### Fixed

- Unlocking the phone in the middle of such a ring could leave the alarm ringing behind a
  notification, with no screen to turn it off on.
- The phone cleaning the app up while it was locked could leave the alarm on the screen with the
  holds, also after the unlock.
- An alarm that ended on the backup screen could be in the history twice.

## [0.4.0] - 2026-11-05

### Added

- **Themes**: their own screen with nine themes, soft accent colours and a colour wheel for the
  accent on any theme, and a theme of your own with its background, cards and text. Text and
  accent always keep 3:1 contrast, so an alarm cannot become unreadable.
- **A ring screen of your own** per alarm: a moving background (aurora, sunrise, waves, stars,
  rings, bubbles) or your own photo, its colour, and a style for the clock, with a full screen
  preview. It stands still with Less motion on.
- **A notice before an alarm, per alarm**: a silent pop-up 5 to 60 minutes before, with Skip to
  skip that ring, and Undo until its time. It works with the app closed.
- A `?` next to each panel's title explains it, instead of notes everywhere.

### Changed

- More room everywhere: rows, cards, buttons and panels, and smaller day bubbles further apart.
- Trying a sound: a tap only picks it, Start plays it for 15 s. A tap on a vibration tries it
  for 15 s, whatever the touch feedback is set to, and Heartbeat now plays whole.
- Try all is at the top of the challenges panel. "Ben je wakker?" says what the wake-up check
  does. Settings are grouped by what they are about.
- The notification no longer shows as a banner over the alarm's screen once that is up.

### Removed

- Try for 10 s, the random order of challenges (alarms keep their set order) and the
  Colorblind theme (it falls back to following the phone).

## [0.3.1] - 2026-11-04

### Fixed

- The release built from the published SDK: its build tool now takes the modules from the
  release build's own record (mobile-sentinel 0.3.1), and the build script installs the tool
  version the lock file names instead of a pinned old one. 0.3.0 never reached the store.

## [0.3.0] - 2026-11-04

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
- One-minute buttons beside the time, next to the five-minute ones. History rows show they
  open, and deleting history, one entry or a period, asks first. A test alarm can be called
  off. Steps count as you take them.
- Native backup screen for when the alarm's own screen cannot come up.
- Reliability screen with what the phone allows and a way to each setting, a first-run version
  of it, and a test alarm through the real ring path. The record of how fast rings were can be
  saved, for someone with a phone nobody here has.
- Quick alarms for a nap, skip the next ring from the card, pause one or all until a date, sort
  by time or next ring, swipe to skip or delete, Undo, templates when the list is empty.
- Wake-up check, note while ringing, full brightness, vibrate first, three vibration patterns,
  minimum volume per alarm, fade in up to 5 minutes, try the alarm for 10 seconds.
- Random order for challenges. Typed time, 12-hour wheels, a date picker with quick
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

- The app no longer starts on a white page: the window, the splash screen and the page are
  the theme's colour from the first frame.
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
- Recents got you out of a ringing alarm on Android 16: it comes straight back now.
- The big clock no longer runs into the edge of the screen with large text, and a long name in "Used
  before" no longer runs off it.
- Up to Android 14 the bars at the top and bottom of the screen take the colour of the app.
- History says "Nothing in this period" instead of "No alarm events yet" for a period without any.
- A scan challenge on a locked phone no longer asks for your PIN before the camera shows.
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
