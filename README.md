# AlarmFree

A free, ad-free, fully offline alarm clock for Android — with a twist: when it
rings, you have to **solve a challenge** before it will shut up. Do some mental
math, walk a few steps, scan the barcode taped to your bathroom mirror. The
alarm doesn't let go until you've actually woken up.

No accounts, no tracking, no network calls, no ads. Everything ships in the APK
and runs on-device.

AlarmFree is built entirely in Rust on top of
[`mobile-sentinel`](../../crates/mobile-sentinel/README.md). The UI is
[Dioxus](https://dioxuslabs.com) rendered in Android's WebView; every decision —
when to fire, what to play, how to survive a process kill, when to release the
kiosk — lives in Rust. Kotlin is only the JNI surface that talks to the Android
framework.

---

## Contents

- [What it does](#what-it-does)
- [The challenges](#the-challenges)
- [How it's built](#how-its-built)
- [Project layout](#project-layout)
- [Build & install](#build--install)
- [How firing works](#how-firing-works)
- [Anti-kill defense](#anti-kill-defense)
- [Data model](#data-model)
- [Internationalization](#internationalization)
- [Conventions & constraints](#conventions--constraints)
- [Roadmap](#roadmap)
- [License](#license)

## What it does

- **Flexible schedules** — weekday-repeating, one-time, or a specific date. The
  mode is derived from your day selection, not a separate toggle.
- **Configurable snooze** — 0 to 10 snoozes, 1 to 30 minutes apart, per alarm.
- **10 bundled tones** plus your own imported sounds (`mp3` / `m4a` / `ogg` /
  `wav`), with in-app preview before you commit.
- **Up to 10 challenges per alarm**, run in order. Each one's solve time is
  recorded.
- **Rich history** with aggregate metrics — reaction time, total dismiss time,
  per-challenge average solve time, snooze counts and intervals, time snoozed.
  Filter by week / month / year / custom range; bulk-delete by age.
- **5 languages** — English, Dutch, Spanish, French, German. Every visible
  string flows through the i18n layer.
- **3 themes** — dark, light, and a colorblind-friendly palette.
- **Aggressive anti-kill** — kiosk lock-task, a foreground service, a
  cross-process job guardian that resurrects the app, and exclude-from-recents.
- **Offline by default** — no CDN, no network, inline SVG icons, bundled sounds,
  embedded translations.

## The challenges

Eight challenge types, each configured per alarm. Math, Memory, and Typing have
four difficulty levels (Easy / Medium / Hard / Extreme); the rest are pure
count/time settings.

| Challenge | What you do | Config |
|---|---|---|
| **Math** | Solve an arithmetic problem on a calculator keypad. | Difficulty sets operand size + operation. |
| **Memory** | Reproduce a sequence of lit cells on a 3×3 (Easy/Medium) or 4×3 (Hard/Extreme) grid. | Difficulty sets sequence length (4 / 6 / 9 / 12). |
| **Typing** | Copy a passage drawn from a large word pool. Case-insensitive, full-word wrapping. | Difficulty sets length (5–10 … 40–60 words) + min accuracy (80–95%). |
| **Shake** | Shake the device a chosen number of times. | Shake count (5–100). |
| **Steps** | Get up and walk a chosen number of steps (hardware step counter). | Step count. |
| **Scanner** | Scan a barcode or QR code — any code, or a specific saved one (e.g. your toothpaste). | Expected value (empty = accept any). |
| **Hold** | Press and hold a bubble until it fills, repeated a chosen number of times. | Hold count + per-hold duration. |
| **Reaction** | Pop a set number of targets before a timer runs out, across several rounds. Miss the timer and the round restarts. | Targets, time limit, rounds. |

Solve the gate, watch a quick celebration, and the alarm dismisses. Fail to
engage and it keeps ringing — that's the whole point.

## How it's built

AlarmFree is a thin consumer of mobile-sentinel. The SDK owns everything hard
about being an alarm; the app owns its UI, its own data, and the glue.

It enables exactly the SDK features it calls — nothing padded:

```toml
mobile-sentinel = { path = "../../crates/mobile-sentinel", features = [
    "alarm-kit", "scanner", "haptics", "audio", "overlay", "permissions",
    "sensors", "foregrounding", "media_picker",
] }
```

`alarm-kit` brings the whole alarm backend (scheduling, firing, snooze, dismiss,
kiosk, foreground service, exact-alarm, sound library, job guardian, recurrence,
snooze policy). The rest are leaf capabilities the UI touches directly: `scanner`
for the scan challenge, `sensors` for shake/steps, `audio` for sound preview,
`media_picker` for importing custom sounds, plus `haptics`, `overlay`,
`permissions`, and `foregrounding`.

The single glue file is **`src/platform.rs`**. It runs `mobile_sentinel::init`,
installs the Android `FiringSink` and sound backend, installs AlarmKit with
AlarmFree's `AlarmClassConfig` (notification channel `alarmfree_firing`, activity
`dev.dioxus.main.MainActivity`, full kiosk lockdown), and calls
`kit.on_startup(&alarms)` to re-arm everything. All app-specific values live
here and arrive in the SDK via config — never baked into the crate.

```
┌───────────────────────────────────────────────┐   ┌──────────────────┐
│  MAIN process                                  │   │  :sentinel       │
│                                                │   │                  │
│  Dioxus UI (Android WebView)                   │   │  Job Guardian    │
│   ├─ screens: home / config / firing /         │   │  polls ~100 ms   │
│   │           history / settings               │   │                  │
│   ├─ components: 8 challenges, time picker,     │   │  MAIN dead?      │
│   │              toast, celebration            │   │    → start it    │
│   └─ i18n (en / nl / es / fr / de)             │   │  MAIN alive?     │
│                                                │   │    → heads-up     │
│  App-owned Rust                                │   └──────────────────┘
│   ├─ models/challenge.rs — challenge generation │
│   ├─ db/ + history_persistence — SQLite         │
│   ├─ custom_sounds.rs — bundled + imported      │
│   └─ platform.rs — AlarmKit install + UI glue   │
│                                                │
│  mobile-sentinel (the SDK)                     │
│   AlarmKit · ContextStore · Job Guardian client │
│   Recurrence (DST-correct) · Snooze · Sounds    │
│   feature-gated capabilities                    │
└───────────────────────────────────────────────┘
```

Two Android processes share the filesystem as IPC. MAIN runs the UI and all
logic; `:sentinel` is a dumb guardian that keeps MAIN alive while an alarm needs
to fire. Kotlin executes (`MediaPlayer.start`, `AlarmManager.setExact`,
`startForeground`, `OnBackInvokedCallback`); Rust decides.

### What the UI calls

The screens only ever talk to AlarmKit lifecycle methods — they never reach
around the SDK with raw JNI:

- `create_with_id` / `update` — schedule or edit (via `schedule_alarm_from_ui`)
- `delete` — cancel
- `pause(id)` / `resume(id)` — entering a challenge screen / challenge timed out
- `snooze(id)` / `dismiss(id)` — user actions
- `mark_solved(id)` — challenge gate passed, so the next dismiss succeeds

The app also registers a `:sentinel` job per alarm so the guardian can resurrect
MAIN mid-fire.

## Project layout

```
apps/alarmfree/
├── Cargo.toml            // dioxus + chrono + rusqlite + mobile-sentinel
├── Dioxus.toml
├── sentinel.toml         // build config (activity + icon + assets)
├── build.sh              // one-shot: dx build → build_sentinel → adb install
├── assets/
│   ├── icon.webp         // launcher icon
│   ├── style.css         // dark / light / colorblind themes
│   └── sounds/
│       ├── default/      // 10 bundled mp3s
│       └── custom/       // user-imported, written at runtime
└── src/
    ├── main.rs / lib.rs
    ├── app.rs            // root component, Router, locale signal, t(), AlarmData
    ├── platform.rs       // sentinel init + AlarmKit wiring + UI helpers
    ├── persistence.rs    // SQLite alarms + settings
    ├── history_persistence.rs  // history query façade
    ├── custom_sounds.rs  // bundled + custom sound management
    ├── firing_state.rs   // cross-process firing detection (500 ms file poll)
    ├── sanitize.rs       // input sanitization for scanned values
    ├── db/               // schema + migrations + CRUD + AggregateMetrics
    ├── models/           // alarm / challenge / history / settings types
    ├── screens/          // home / alarm_config / alarm_firing / history / settings
    ├── components/       // challenges/, celebration, icon, time_picker, toast
    └── i18n/             // en, nl, es, fr, de  (embedded via include_str!)
```

## Build & install

You need:

- Rust 1.76+ with the Android target: `rustup target add aarch64-linux-android`.
- Android SDK + NDK (`ANDROID_HOME` / `ANDROID_NDK_HOME`).
- The Dioxus CLI: `cargo install dioxus-cli --version 0.7.x`.
- A connected device or emulator with USB debugging on.

### One-shot

```bash
# Default: uses the published mobile-sentinel crate (the real end-user flow)
bash apps/alarmfree/build.sh                     # build only (debug)
bash apps/alarmfree/build.sh install             # build + install on default device
bash apps/alarmfree/build.sh install <device-id> # build + install on a specific device

# Final builds (release / Play Store AAB)
bash apps/alarmfree/build.sh release
bash apps/alarmfree/build.sh aab

# Local development (uses the local sibling mobile-sentinel/ source instead of the published crate)
bash apps/alarmfree/build.sh workspace
bash apps/alarmfree/build.sh workspace install
bash apps/alarmfree/build.sh workspace aab
```

### Manual

```bash
# 1. Compile Rust → libdioxus.so for Android
dx build --platform android --target aarch64-linux-android --package alarmfree

# 2. Wire mobile-sentinel into the generated Android project (manifest, Kotlin, icons, sounds)
cargo run -p mobile-sentinel --bin build_sentinel -- --app alarmfree

# 3. Install
adb install -r target/dx/alarmfree/debug/android/app/app/build/outputs/apk/debug/app-debug.apk
```

> Step 1 reports `BUILD FAILED` on duplicate icon resources — **expected**. The
> `.so` is still produced. Step 2 fixes the icons, copies the sounds, wires
> Kotlin, and runs Gradle. Because `dx build` regenerates the Android project
> every run, `build_sentinel` must run **after** it.

Output APK:
`target/dx/alarmfree/debug/android/app/app/build/outputs/apk/debug/app-debug.apk`.

### Release / Google Play Store (Internal Testing)

The app package name (used by Play Store as the unique app ID) is set in `Dioxus.toml`:

```toml
[bundle]
identifier = "com.fidderr.alarmfree"
```

(or under `[android] identifier = "..."` to override only for Android).

**Never change the package name after the first Play Store release.**

#### Quick one-command flow for Internal Testing (recommended)

From the `alarmfree/` directory:

```bash
./build.sh aab
./sign-aab.sh
```

- `build.sh aab` builds the production-style release AAB (using the published `mobile-sentinel` crate — the real flow).
- `sign-aab.sh` auto-detects Android Studio JDK (jbr/bin), generates the upload keystore the first time (`upload-keystore.jks` with your passwords), signs the AAB, and drops a ready-to-upload file into the `release/` folder (e.g. `AlarmFree-InternalTesting-20250602-2228.aab`).

After that you can **literally just copy the file** from `release/` and upload it to Play Console internal testing.

**Important for every new Play release:** bump the `version = "..."` line in `Cargo.toml` first (e.g. `0.1.1` → `0.1.2`).  
`build.sh` will automatically derive a new higher `versionCode` (major*10000 + minor*100 + patch scheme) so Play accepts it.  
The Cargo version also becomes the visible "version name" in the store.

If you ever see the error "You can't rollout this release because it doesn't allow any existing users to upgrade to the newly added app bundles", it means your new AAB's versionCode is not higher than one already serving users (e.g. from an earlier accidental high code like 12345). In that case, edit `Dioxus.toml` and force a high explicit one (higher than your current max in the console, e.g. 12346):
```toml
[android]
version_code = 12346   # must be > any previously used code
```
Then rebuild with `./build.sh aab && ./sign-aab.sh`. See `release/FUTURE_RELEASES_HOWTO.txt` for full troubleshooting + why you can't "start over" with low codes like 102 on the same package.

For local development while also changing `mobile-sentinel` source:

```bash
./build.sh workspace aab
./sign-aab.sh
```

The script will only error (with hints) if it really can't find the JDK tools.

#### Manual / advanced path

To build a release AAB:

```bash
./build.sh aab
```

This runs `dx build --release` + `build_sentinel --release --aab`.

The unsigned AAB lands at:
`target/dx/alarmfree/release/android/app/app/build/outputs/bundle/release/app-release.aab`

You can also do `./build.sh release` for a release APK (unsigned).

Note: `alarmfree/.cargo/config.toml` is a standard Cargo configuration file (newly added). It sets Rust linker flags (`-Wl,-z,max-page-size=16384`) for Android targets. This makes `libmain.so` (the native Rust binary) compatible with devices that use 16 KB memory pages. Play Console now flags AABs that don't support this; the config fixes the "does not support 16 KB" warning for `libmain.so`. Cargo/dx automatically uses it when building from this directory. It has no effect on non-Android builds or feature inclusion.

For local development of both the app and mobile-sentinel together, use the workspace flag:
```bash
./build.sh workspace aab
./build.sh workspace release
```

Upload the **signed** AAB to Google Play Console (create app with the exact package name `com.fidderr.alarmfree` on first creation).

### Signing the AAB for Play Store (required)

The easiest way for internal testing is to just run:

```bash
./sign-aab.sh
```

(after `./build.sh aab` or `workspace aab`). It handles keystore creation + signing + copying a clean file into `release/`.

The `./build.sh aab` command produces an **unsigned** AAB.

Google strongly recommends using **Google Play App Signing**:

1. **Create an upload keystore** (do this once, store the `.jks` file and passwords safely — this is your "upload key"):

   On Windows (Git Bash / MINGW64 or Command Prompt, keytool must be in PATH — usually from Android Studio's `jre` or `jdk`):

   ```bash
   keytool -genkey -v -keystore upload-keystore.jks -alias upload -keyalg RSA -keysize 2048 -validity 10000
   ```

   - Choose a strong password for the store and the key.
   - Remember the alias (`upload` in the example).

2. **Sign the AAB** you just built (replace paths and passwords):

   ```bash
   jarsigner -verbose -sigalg SHA256withRSA -digestalg SHA-256 \
     -keystore upload-keystore.jks \
     -storepass YOUR_STORE_PASSWORD \
     -keypass YOUR_KEY_PASSWORD \
     "target/dx/alarmfree/release/android/app/app/build/outputs/bundle/release/app-release.aab" \
     upload
   ```

   On Windows you may need to use forward slashes or quote the whole path carefully.

3. **Verify the signature** (optional but recommended):

   ```bash
   jarsigner -verify -verbose -certs "target/dx/alarmfree/release/android/app/app/build/outputs/bundle/release/app-release.aab"
   ```

4. In Play Console:
   - Create your app and enter the package name `com.fidderr.alarmfree` **exactly** on the first screen.
   - Go to **App integrity > App signing** and enable Play App Signing.
   - Upload the **signed** AAB (the one you just jarsigner'ed) under **Production** (or a test track like Closed testing first).

After the first successful upload with Play App Signing enabled, Google will generate the final app signing key. From then on you only ever sign with your local upload key when you build new versions.

**Critical for future releases:**
Once you have uploaded at least one AAB signed with a particular keystore (the `upload-keystore.jks`), the certificate inside that keystore becomes your registered upload key on Play Store.
- You **must** keep using the **exact same** .jks file for every future signed release.
- You do **not** re-run keytool -genkey or give "other details" for new releases.
- The `sign-aab.sh` script automatically reuses the existing keystore if present (it only generates if the file is missing).
- If you ever create a brand new keystor, it will have a different certificate. Uploading bundles signed with it will fail, and you will need to go through Google's upload key reset process.
- The only things that matter for new releases are: the same jks file + the correct password + the correct alias ("upload").

**Tip:** You can also configure signing directly in Dioxus (see `[android.signing]` in Dioxus docs) so that future `./build.sh aab` runs produce a signed AAB automatically. For now the manual `jarsigner` step is the simplest.

### About the "personal information" keytool asks for (CN, OU, O, L, ST, C)

When `sign-aab.sh` (or manual keytool) asks for your name, organization, city, etc., this information is written into the **public certificate** that signs the AAB.

- It becomes visible to Google Play when you upload the bundle.
- Anyone with technical tools can extract and read the certificate details from a signed AAB/APK.
- It does **not** appear in the Play Store page, description, or get installed on users' phones in any visible way.
- It is completely normal and required for every Android app.

You do **not** need to use highly personal real-world details. Safe choices:
- First/last name: your GitHub name or "fidderr"
- Organization: "fidderr" or the name you publish under
- City/State/Country: your actual city/country, or leave blank / use "XX"

The private key (what the passwords protect) never leaves your machine. Only the public certificate travels with the AAB you upload.

If you are uncomfortable, you can always delete the `upload-keystore.jks` and run `./sign-aab.sh` again to generate a fresh one before your first Play Store upload.

### What if I lose the upload key / passwords?

This is very important to understand.

The keystore you create (`upload-keystore.jks`) is your **upload key**, not the final app signing key. Because we use Google Play App Signing, Google holds the real app signing key.

**Scenarios:**

- **Before you have ever uploaded anything to this package name** (`com.fidderr.alarmfree`):  
  No problem at all. Just delete the keystore (or let the script generate a new one) and start over. Run `./sign-aab.sh` again.

- **After the app has been created in Play Console and at least one signed AAB has been uploaded + Play App Signing is enabled**:  
  You can still recover, but it is painful.  
  Google provides an "Upload key reset" process:  
  https://support.google.com/googleplay/android-developer/answer/7384423

  You will have to prove you own the developer account and the app. The process can take several days to a couple of weeks. During that time you **cannot publish new versions**.

  Also see the section above: "Critical for future releases" — you must keep the same keystore file.

**Strong recommendations (do these now):**

- Store the `upload-keystore.jks` file as an attachment in a password manager (Bitwarden, 1Password, etc.).
- Store the two passwords (store password and key password) in the same password manager entry.
- Make at least one encrypted offline backup (e.g. VeraCrypt container on a USB stick or another computer).
- Write down the alias (`upload`) and that it is a 2048-bit RSA key valid for ~27 years.
- Never put the keystore or passwords in the git repo (already prevented by `.gitignore`).

Losing the upload key after going live is annoying but recoverable thanks to Play App Signing.  
Losing it before your first upload is basically free.

### What you actually upload to Play Console

- The **signed** `.aab` file (the most important artifact).
- Store listing assets (see `play-store-description.md` for title, short description, full description, what's new, etc.):
  - High-res icon (512×512 png)
  - Feature graphic (1024×500)
  - Screenshots (at least 2 phone screenshots, more is better)
  - Privacy policy URL (you must provide one — even for fully offline apps)
- Fill in the Content rating questionnaire.
- Set up pricing & distribution (free or paid).
- Add testers if using a closed track.

Once the AAB is uploaded and the listing is complete, you can submit for review. The first release usually takes a few days for initial review. Subsequent releases are faster.

### Host checks (must stay green)

```bash
cargo check  --workspace
cargo test   --workspace
cargo clippy --workspace -- -D warnings
cargo fmt    --all -- --check
```

These run on your dev machine with no device — the SDK's host fallbacks make the
whole app testable off-Android. On-device behavior still needs a real
`build.sh install` to confirm.

## How firing works

```
exact alarm fires (or a snooze elapses)
  → :sentinel alarm receiver activates the job + starts the guardian
  → guardian: MAIN dead? → start it.  MAIN alive? → broadcast a heads-up.
  → MAIN starts → AlarmKit::on_startup() re-engages any Firing state
       OR the heads-up callback → kit.handle_job_heads_up() → Fire
  → start_firing: audio plays, kiosk locks, FGS notification posts,
       full-screen intent wakes the screen
  → user taps Dismiss → challenges render → each solve recorded → celebration
  → kit.dismiss(id): audio stops, FGS stops, kiosk releases, job deactivated
  → an AlarmEvent is written to SQLite (history)
  → recurring alarms re-arm their next occurrence
```

If MAIN dies mid-fire — swiped from recents, killed by the OS low-memory killer,
or by an aggressive OEM — the guardian notices within ~100 ms, restarts MAIN, and
`on_startup()` finds the `Firing` state in the ContextStore and re-engages: audio
resumes, kiosk reapplies, the firing screen comes back. You keep fighting the app
until the challenge is done. That is the design.

The firing screen itself is detected cross-process: a 500 ms poller in
`firing_state.rs` reads the shared ContextStore and flips the UI to the firing
screen (rendered outside the Router) the moment the state machine writes
`firing`.

## Anti-kill defense

| Escape attempt | Defense |
|---|---|
| Press HOME | Consumed; kiosk relaunches the activity (50 ms debounce). |
| Press BACK | `OnBackInvokedCallback` consumes it (predictive-back enabled). |
| Swipe from recents | Task excluded from recents. |
| Kill the MAIN process | `:sentinel` guardian detects it within ~100 ms, restarts MAIN, AlarmKit re-engages. |
| Force-stop via Settings | **Not** currently defended — would require mobile-sentinel's `accessibility` (ultra-protection) feature, which this build deliberately leaves off. |

## Data model

### SDK-owned vs. app-owned

- **mobile-sentinel** owns the alarm *runtime* state: a per-alarm `ContextStore`
  JSON record + the `:sentinel` job files. Don't hand-edit these.
- **AlarmFree** owns its own SQLite database (the alarm list, settings, firing
  history) and the custom-sound files. The app converts its `AlarmData` into the
  SDK's `AlarmSpec` in `platform::alarm_data_to_spec` when talking to AlarmKit.

### Alarm (`app.rs`)

```rust
pub struct AlarmData {
    pub id: String,
    pub time_hour: u8,
    pub time_minute: u8,
    pub label: String,
    pub days: [bool; 7],                   // Mon..Sun mask
    pub enabled: bool,
    pub sound: String,                     // bundled stem or custom token
    pub sound_mode: usize,                 // 0 = sound+vibrate, 1 = sound, 2 = vibrate
    pub challenges: Vec<(String, String)>, // (type_key, config_json)
    pub snooze_count: u8,
    pub snooze_interval: u8,               // minutes
}
```

The schedule mode is **derived**: any day set → recurring (`Weekdays`); no days
set → a one-time alarm for the next occurrence of `time_hour:time_minute`.

### Challenge config schemas

Stored as JSON in `AlarmData::challenges[i].1`:

| Type key | Schema |
|---|---|
| `challenge.math` | `{"difficulty": "easy"\|"medium"\|"hard"\|"extreme"}` |
| `challenge.memory` | `{"difficulty": "…"}` (3×3 easy/medium, 4×3 hard/extreme; length 4/6/9/12) |
| `challenge.typing` | `{"difficulty": "…"}` (5–10 / 15–20 / 25–35 / 40–60 words; 80/85/90/95% accuracy) |
| `challenge.shake` | `{"count": <n>}` |
| `challenge.steps` | `{"count": <n>}` |
| `challenge.scan` | `{"value": "<expected>"}` (empty = accept any code) |
| `challenge.hold` | `{"count": <n>, "hold_ms": <ms>}` |
| `challenge.reaction` | `{"count": <targets>, "time_ms": <ms>, "rounds": <n>}` |

### History (`models/history.rs`)

```rust
pub struct AlarmEvent {
    pub id: String,
    pub alarm_id: AlarmId,
    pub alarm_label: String,
    pub fire_timestamp: DateTime<Utc>,
    pub first_interaction_timestamp: Option<DateTime<Utc>>,
    pub dismiss_timestamp: DateTime<Utc>,
    pub snooze_count: u8,
    pub snooze_intervals_minutes: Vec<u8>,     // one entry per snooze tap
    pub challenge_solves: Vec<ChallengeSolve>,
    pub total_dismissal_time: Duration,
}

pub struct AggregateMetrics {
    pub total_events: u32,
    pub avg_snooze_count: f64,
    pub avg_snooze_interval_minutes: f64,
    pub avg_total_snoozed: Duration,
    pub avg_reaction_time: Duration,
    pub avg_total_challenge_time: Duration,
    pub avg_total_dismissal_time: Duration,
    pub avg_solve_per_challenge: BTreeMap<(ChallengeType, Difficulty), Duration>,
}
```

`AlarmEvent` derives `reaction_time`, `total_snoozed`, and
`total_challenge_time`. The aggregate computation normalizes
difficulty-less challenge types (shake, steps, scan, hold, reaction) so they
collapse into a single bucket per type.

### SQLite schema (`db/schema.rs`, current = v1)

- `alarms(id PK, data)` — the full `AlarmData` is stored as a JSON blob in
  `data`, so adding a field only bumps the migration version, never a column.
- `settings(id PK CHECK (id = 1), theme, locale, default_snooze_count,
  default_snooze_interval_minutes)` — single-row settings.
- `alarm_events(id PK, alarm_id, alarm_label, fire_timestamp,
  first_interaction_timestamp, dismiss_timestamp, snooze_count,
  snooze_intervals, challenge_solves, total_dismissal_time_ms)` — indexed on
  `fire_timestamp` and `alarm_id`.

> Pre-1.0, the schema ships as a single clean migration. Uninstall + reinstall
> on a schema change. Post-Play-Store, migrations become additive only.

## Internationalization

Translations live in `src/i18n/<locale>.json` and are embedded at compile time
with `include_str!`. The `t(&translations, "key.name")` helper looks up by key
and falls back to the key text if missing; placeholders are substituted with
`String::replace("{name}", value)`.

Every user-visible string in `screens/`, `components/`, and the firing flow goes
through `t()`. Adding a locale is two steps:

1. Add `<locale>.json` with every key from `en.json`.
2. Add a `Locale` variant + a `load_translations` arm in `i18n/mod.rs`, and list
   the language in Settings.

## Conventions & constraints

These come straight from the WebView's quirks — ignore them and the UI freezes:

- **No `<input type="file">`** — it freezes the Dioxus Android WebView. Use
  `mobile_sentinel::media_picker::pick_file(&[...])`.
- **Inline SVG icons only** (`components/icon.rs`). No Font Awesome, no CDN
  webfonts — they freeze the WebView. Fully offline, always.
- **Dioxus signals are `!Send`** — reach AlarmKit from background / `!Send`
  contexts via `crate::platform::alarm_kit()` / `alarm_kit_optional()`.
- **RSX `"{var}"` interpolation auto-escapes HTML** — never use
  `dangerous_inner_html`. Scanned barcode/QR values are sanitized via
  `sanitize.rs`.
- **`build_sentinel` runs after `dx build`** — `dx` regenerates the Android
  project every run.
- New platform capabilities belong in **mobile-sentinel** (added app-neutral),
  then enabled here and called — never reached around the SDK with raw JNI.

## Roadmap

- iOS port (waiting on mobile-sentinel's iOS backends).
- Play Store release with a signed APK + additive migrations.
- Alarm sharing (export / import).
- Watch / wearable companion via the sensor pipeline.

## License

MIT. See [LICENSE](LICENSE).
