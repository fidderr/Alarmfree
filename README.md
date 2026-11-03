# AlarmFree

A free, ad-free, fully offline alarm clock for Android. When it rings, you can only turn it
off by doing something awake: solve a sum, walk some steps, scan the barcode on your
toothpaste. It keeps ringing whatever happens to the app or the phone until you did, or until
the limit you set.

No accounts, no tracking, no ads. Everything ships in the APK. Needs Android 12 or newer, on a
64-bit phone.

It is written in Rust on top of [mobile-sentinel](https://github.com/fidderr/mobile-sentinel),
a separate generic SDK. The UI is [Dioxus](https://dioxuslabs.com) in Android's WebView. The
app has no Kotlin of its own: what Android needs is the SDK's, and the SDK only ships what
`Cargo.toml` asks for.

- [What it does](#what-it-does)
- [The challenges](#the-challenges)
- [How a ring works](#how-a-ring-works)
- [What it holds up against](#what-it-holds-up-against)
- [Project layout](#project-layout)
- [Build and install](#build-and-install)
- [Tests](#tests)
- [Data](#data)
- [Languages](#languages)
- [Rules of this code](#rules-of-this-code)
- [Releasing](#releasing)

## What it does

**Alarms.** Weekdays, or one date. Your own label, and a note shown while it rings. Quick
alarms for a nap (+5 to +60 minutes) that delete themselves. Skip the next ring, pause one or
all until a date, duplicate, drag into your own order, or sort by time or next ring. Up to 50.

**Sound.** 10 bundled tones and your own files, renamed as you like. Tapping one plays it as
the alarm will. Fade in up to 5 minutes, a minimum volume per alarm or for all, three vibration
patterns, vibrate first with the sound after a delay, or vibration only.

**Ringing.** Up to 10 challenges per alarm, in your order or at random, with Gentle, Normal and
Hard presets, each one to try out first. 0 to 10 snoozes of 1 to 60 minutes. A ring limit of 30
minutes to forever. An optional wake-up check a few minutes later. Optional full brightness.

**Telling the truth.** A reliability screen with what the phone allows, a way to each setting,
and a test alarm that goes through the whole real ring path. History with outcomes (solved,
missed, stopped at the limit, phone restarted), a streak, a chart and timings per challenge.

**The rest.** System, dark, light, pure black and colorblind themes with five accent colours.
Five languages. 12 or 24 hours, and which day a week starts on. A home-screen widget with the
next alarm. An optional quiet notification before an alarm. Alarms to a file and back. Undo on
what can be undone. Works on tablets and foldables, with the list and an alarm side by side.

## The challenges

| Challenge | What you do | Settings |
|---|---|---|
| **Math** | A sum, on a keypad. | Easy to Extreme. |
| **Memory** | Repeat a sequence of lit cells. | Easy to Extreme: 4 to 12 cells. |
| **Typing** | Copy a passage in your language. Capitals do not count, a skipped letter costs one letter. | Easy to Extreme: 5 to 60 words, 80 to 95% right. |
| **Shake** | Shake the phone. | 5 to 200 times. |
| **Steps** | Walk, by the phone's step counter. | 5 to 200 steps. |
| **Scan** | Scan a barcode or QR code: any, or one you saved. | The code. |
| **Hold** | Press a bubble until it fills. | 1 to 30 times, 1 to 15 seconds. |
| **Reaction** | Pop targets before the time runs out. Too late starts over. | Targets, time, rounds. |

A challenge that cannot be done (a broken sensor, a lost code) can be swapped for 20 holds of 3
seconds. That is slow on purpose. A minute without doing anything brings the sound back.

## How a ring works

Nothing here waits for the UI, because the process it lives in is usually dead at ring time.

```
setAlarmClock, carrying a ring plan Rust wrote when it armed the alarm
  -> SentinelAlarmReceiver in :sentinel
       marks the job active, arms a watchdog, starts the ring service
  -> SentinelForegroundService in :sentinel (no Rust, no WebView)
       foreground notification, sound and vibration from the plan
  -> MAIN comes up: the state machine runs, and the screen with it
  -> SentinelJobGuardian in :sentinel keeps it that way
```

`:sentinel` is the one process without the UI framework, so nothing that ends the UI can stop
the sound. Measured on a real phone (OnePlus, Android 16) with every process dead: sound within
200 ms of the alarm's time.

Policy stays in Rust, in `src/platform.rs` and the SDK's AlarmKit; Kotlin only executes it.

## What it holds up against

| What happens | What it does |
|---|---|
| Home, Recents | Brought straight back. |
| Back, volume keys | Swallowed. The minimum volume is applied again every second. |
| The UI process is killed | Sound goes on, the screen comes back. |
| The ring process is killed | Android or the watchdog alarm brings it back within seconds. |
| Both killed, as a swipe does on some phones | The same, and a provider in the new `:sentinel` starts guarding again. |
| The UI freezes | A heartbeat through the page stops, the guardian restarts it. After two failures a native backup screen takes over. |
| A dark-mode switch, font change, rotation or fold | Handled in place, no restart. |
| A sound that cannot play | The next one: the system default, then a bundled one, then vibration. |
| Restart in the middle of a ring | Counts as dismissed, with a notice. |
| Phone off at ring time | Under 15 minutes late it still rings; later it is reported as missed. |
| Before the first unlock | Files are device-encrypted, so it rings, on the backup screen. |
| A restart nobody told the app about | Detected, and explained on the reliability screen. |
| Force-stop | Android removes the alarms and no app can prevent that. Detected at the next start, and told. |

## Project layout

```
Cargo.toml        the SDK features this app uses: all it gets
sentinel.toml     icon, assets, direct boot, orientation for build_sentinel
build.sh          dx build + build_sentinel; see below
sign-aab.sh       signs a Play bundle
assets/           icon, style.css (compiled in), sounds/default
devtest/          scenarios on a device or emulator; see Tests
src/
  app.rs          root component, routes, the back and firing watchers, AlarmData
  platform.rs     what AlarmFree wants of the SDK, and an alarm as an AlarmSpec
  alarm_ops.rs    new alarms, quick alarms, wake-up checks, pausing, backups
  schedule.rs     one-time dates, next ring, countdowns
  persistence.rs  alarms and settings, a database that cannot be read
  history_persistence.rs, db/    history and the SQLite schema
  custom_sounds.rs   bundled and imported sounds
  firing_state.rs    which alarm rings, from the SDK's records
  back.rs            Back, the same on every Android version
  feedback.rs, text.rs, timer.rs, sanitize.rs, ordering.rs, list_edit.rs, reorder.rs/.js
  debug_hooks.rs     what devtest drives the app with; not in a build for users
  models/         alarm, challenge, history, settings, what a challenge is set to
  screens/        home/, alarm_config/, settings/ (screen and sheets), alarm_firing,
                  challenge_run, history, practice, reliability
  components/     challenges/, ui (Sheet, Row, Chips...), time_picker, toast, icon, celebration
  i18n/           en, nl, es, fr, de
```

## Build and install

You need Rust with the Android targets, the Android SDK and NDK (`ANDROID_HOME`,
`ANDROID_NDK_HOME`), a JDK 17, and `cargo install dioxus-cli --version 0.7.x`.

```bash
./build.sh                 # debug APK, with the published mobile-sentinel
./build.sh install         # and install it
./build.sh release         # release APK, unsigned
./build.sh aab             # Play bundle, unsigned; then ./sign-aab.sh

./build.sh workspace ...   # the same, with mobile-sentinel from a sibling folder
```

For `workspace`, also add this to `Cargo.toml`, and do not commit it:

```toml
[patch.crates-io]
mobile-sentinel = { path = "../mobile-sentinel" }
```

`TARGET=x86_64-linux-android ./build.sh ...` builds for an emulator. `RELEASE_TEST=1 ./build.sh
workspace release` builds what the scenarios need of a release: R8 and the optimisation of the
real thing, but debuggable, signed with the debug key, and with the debug hooks in.

`dx` generates the Android project on every run, so `build_sentinel` always runs after it.

## Tests

`cargo fmt --check`, `cargo clippy --all-targets -- -D warnings` and `cargo test` run with no
device. They also check that every translation has every key and placeholder, and that every
key the code names exists.

`devtest/` drives a real app over adb and judges only what can be seen from outside: is the
sound on, what is on screen, what is armed, what is saved.

```bash
SERIAL=emulator-5554 devtest/matrix.sh            # every scenario on one device
SERIAL=emulator-5554 devtest/matrix.sh s01 s07    # just these
bash devtest/all.sh                               # Android 16 and 12 at once, 16 also on big screens
```

All of it should take under 15 minutes on an emulator, and no scenario over a minute: the
summary names the ones that do. An emulator's clock moves instead of a scenario waiting for the
time to pass, what can be asked is asked until it is true, what can happen during the same ring
or restart does, and two shells on the device stay open instead of starting adb for every call.

On a real phone: `NO_LOCK=1` for a phone with a PIN (nothing here sets or asks for one), and
`PHONE_SQLITE=<path>` for a `sqlite3` pushed to it. A phone's clock is left alone, so there the
time has to pass, and it only gets what needs no restart and changes none of its settings. Some
scenarios need things only a person can give: a reboot with a PIN, real sound and vibration, an
overnight run.

No device outside the emulators and one OnePlus has seen this yet. Phone makers differ, which
is what the test alarm and the saved record of how fast rings were are for.

## Data

- **The SDK** owns the runtime state: one JSON record per alarm and job files, under
  `files/sentinel/`, read from both processes.
- **The app** owns a SQLite database: alarms as JSON rows (`AlarmData` in `app.rs`), settings
  (with `Prefs` as one JSON column, so a new preference needs no migration), and history. A
  schema change gets a migration in `db/migrations.rs`, in a transaction. A database that cannot
  be read is kept as `alarmfree.db.corrupt-<time>`, and the user is told.

Files are in device-encrypted storage (`direct_boot` in `sentinel.toml`), because an alarm has
to ring before the first unlock.

## Languages

`src/i18n/<code>.json`, compiled in. `t(&translations, "key")` looks a text up, and
`{placeholders}` are filled in with `replace`. To add a language: the JSON file with every key,
a `Locale` variant, and an arm in `load_translations`.

## Rules of this code

- **Policy in Rust, Kotlin executes.** A capability the app needs goes into mobile-sentinel,
  without knowing about this app, and is enabled here. The SDK has a test that it stays generic.
- **No free way out of a ring.** Whatever ends one without the user is recorded and told.
- **No `<input type="file">`, CDN fonts or remote anything** in the page. Icons are inline SVG.
- `INTERNET` stays in the manifest: Dioxus talks to its page over a socket on the phone itself,
  and without it the page stays blank. Nothing else uses it, and nothing leaves the phone.
- WebView 91 is the oldest there is (Android 12): no `color-mix` or `:has` in the CSS.
- Classes set from JavaScript do not show up in Rust: see `reorder.js` before removing CSS.
- The page applies changes at once, not in an animation frame (`WINDOW_WATCH` in `app.rs`): the
  UI's tasks wait for the last change, and a WebView that is not drawing has no frames.

## Releasing

1. Publish mobile-sentinel first if this release needs a newer one, and bump the version here
   and `SENTINEL_VERSION` in `build.sh`.
2. Bump `version` in `Cargo.toml` and `version_code` in `Dioxus.toml` (higher than any upload).
3. `./build.sh aab && ./sign-aab.sh`. The result is in `release/`. The upload keystore is not in
   this repository, and every release needs the same one; `sign-aab.sh` says what to do when
   there is none.
4. `play-store-description.md` has the listing texts and what Play Console asks about.

## License

MIT. See [LICENSE](LICENSE).
