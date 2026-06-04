# AlarmFree — Google Play Store Listing Texts

**App Title (on Play Console):**  
AlarmFree

**Short Description** (max 80 characters):  
Free alarm clock that makes you solve challenges to dismiss. No ads. No tracking. Fully offline.

(Character count: 78)

---

## Full Description (copy-paste ready)

Wake up for real.

AlarmFree is a completely free, ad-free, and fully offline alarm clock for Android that refuses to let you go back to sleep until you've actually woken up.

When your alarm rings, you don't just tap a button. You have to complete one or more challenges first — mental math, memory sequences, typing passages, shaking your phone, walking steps, scanning a barcode, holding a bubble, or a fast reaction game. Up to 10 challenges can be chained per alarm. Solve them all and the alarm finally dismisses. Miss the timer or get it wrong and the challenge restarts.

### Why it works
- **Challenge-based dismissal** forces real engagement instead of half-asleep tapping.
- **Aggressive anti-kill protection** keeps the alarm firing even if you try to swipe it away, kill the app, or press Home/Back.
- **No snooze abuse** — you control how many snoozes (0–10) and the interval, but you still have to face challenges when you finally get up.
- **Rich history & stats** show your reaction time, how long you snoozed, and average solve time per challenge type and difficulty.

### Flexible alarms
- Weekday-repeating, one-time, or specific-date alarms (mode is derived automatically from your selection).
- 10 high-quality bundled alarm tones + support for your own imported sounds (mp3, m4a, ogg, wav).
- Per-alarm sound mode: sound + vibration, sound only, or vibration only.
- Custom labels and up to 50 alarms.

### Eight challenge types (with difficulty scaling)
- **Math** — Solve arithmetic problems (Easy to Extreme).
- **Memory** — Reproduce sequences on a grid.
- **Typing** — Accurately copy passages of increasing length.
- **Shake** — Shake your device a set number of times.
- **Steps** — Walk a chosen number of steps (uses hardware step counter).
- **Scanner** — Scan any barcode/QR or a specific saved code.
- **Hold** — Press and hold a visual bubble until it fills (repeat).
- **Reaction** — Pop targets before a timer runs out, across multiple rounds.

### Privacy & offline by design
- No accounts. No tracking. No internet required.
- No ads. No analytics. No data leaves your device.
- Everything (sounds, translations, icons) is bundled inside the app.
- Open source (MIT license).

### Beautiful & accessible
- 3 themes: Dark, Light, and a colorblind-friendly palette.
- 5 languages: English, Dutch, Spanish, French, German.
- Clean, modern interface built with Dioxus.

### Built differently
AlarmFree is written in Rust and uses a powerful native alarm SDK (mobile-sentinel) for rock-solid scheduling, exact alarms, foreground services, kiosk mode, and cross-process resurrection. The result is an alarm that is genuinely hard to ignore — exactly what many people need.

Perfect for heavy sleepers, shift workers, students, or anyone tired of "just 5 more minutes" turning into an hour.

Download AlarmFree and start waking up properly.

---

## What's New (for v0.1.0 / initial release)

- First public release
- 8 challenge types with 4 difficulty levels on most
- Flexible scheduling, per-alarm snooze, custom sounds
- Detailed alarm history with metrics and filters
- Aggressive anti-kill protection (kiosk + job guardian)
- 5 languages and 3 themes
- Fully offline with no ads or tracking
- Open source

---

## How to Build the App Bundle (AAB) for Play Store (Internal Testing)

**Recommended (easiest) flow — just copy the final file:**

From inside the `alarmfree/` directory:

```bash
./build.sh aab
./sign-aab.sh
```

- The first command builds the release AAB (using the published crate — the "real" flow you will use for actual releases).
- The second command generates an upload keystore (first time only), signs the AAB, and places a ready-to-upload signed bundle in the `release/` folder (e.g. `AlarmFree-InternalTesting-20250601-1430.aab`).

You can then **literally just copy** the file from `release/` and upload it.

For testing while also editing the `mobile-sentinel` source at the same time:

```bash
./build.sh workspace aab
./sign-aab.sh
```

### Manual build (unsigned AAB)

From inside the `alarmfree/` directory:

```bash
./build.sh aab
```

This produces an **unsigned** AAB at:

`target/dx/alarmfree/release/android/app/app/build/outputs/bundle/release/app-release.aab`

The `build_sentinel` tool will print the exact path when it finishes.

### Signing the AAB (required for Play Store)

The AAB from the build script is unsigned.

**Recommended flow: Google Play App Signing**

The `sign-aab.sh` helper (run after `./build.sh aab`) now **auto-detects** the Android Studio JDK (jbr/bin) on Windows when run from Git Bash/MINGW64. No manual export PATH needed in most cases — it just works.

For every future release after the first one, simply run the same two commands again. The script will detect the existing upload-keystore.jks and reuse the exact same certificate. You do not re-enter "other details".

1. Generate an upload keystore (run once):

   ```bash
   keytool -genkey -v -keystore upload-keystore.jks -alias upload -keyalg RSA -keysize 2048 -validity 10000
   ```

   Store this file and the passwords somewhere safe. This is your upload key.

### About the certificate details keytool asks you for

The fields (first/last name, organization, city, state, country) go into the **public certificate** embedded in your signed AAB.

- Google sees them when you upload.
- Technically inclined people can read them from the signed bundle.
- They do **not** show up on your Play Store listing or get pushed to users.

This is completely standard for Android signing. You can use your public name / "fidderr" / generic location. Avoid putting home street address or phone numbers. City + country is fine.

You can always delete the keystore and generate a new one (with different details) **before your first upload** to Play Console if you want to change anything.

**For all future releases after the first upload:**
You must keep using the exact same upload-keystore.jks file (and its certificate). 
The "other details" (CN, O, etc.) are baked into the certificate at creation time.
Creating a new keystore later will give you a different certificate, which Play Store will reject for new version uploads.
You will then need to request an upload key reset via Google support.
The sign-aab.sh script will correctly reuse the existing keystore for future builds — just run the normal `./build.sh aab && ./sign-aab.sh` flow.

2. Sign the AAB (example for Windows Git Bash):

   ```bash
   jarsigner -verbose -sigalg SHA256withRSA -digestalg SHA-256 \
     -keystore upload-keystore.jks \
     -storepass YOUR_STORE_PASSWORD \
     -keypass YOUR_KEY_PASSWORD \
     "target/dx/alarmfree/release/android/app/app/build/outputs/bundle/release/app-release.aab" \
     upload
   ```

3. Verify (optional):

   ```bash
   jarsigner -verify -verbose -certs "target/dx/.../app-release.aab"
   ```

Upload the **signed** `.aab` to the Play Console.

### What if I lose the upload key / passwords?

**Critical information:**

The keystore (`upload-keystore.jks`) is only your **upload key**.

Because we use **Google Play App Signing** (strongly recommended), Google manages the actual app signing key.

**Recovery scenarios:**

- Before any upload to this package name (`com.fidderr.alarmfree`):  
  Completely safe. Just generate a new keystore and start over.

- After the app exists in Play Console and Play App Signing has been enabled:  
  You can request an upload key reset through Google Play Developer support:  
  https://support.google.com/googleplay/android-developer/answer/7384423

  You must prove ownership. It can take days to weeks, and you won't be able to release new versions in the meantime.

**For new releases after the first upload:**
You must keep using the exact same `upload-keystore.jks` file (the certificate details are fixed forever for this app).
Creating a different keystore later (different "details", even with the same password) will cause Play to reject new uploads until you do a key reset.
The `sign-aab.sh` script correctly re-uses the existing keystore — just run the normal build + sign flow.

**Prevention (do this immediately after creating the keystore):**

- Attach `upload-keystore.jks` to an entry in a password manager.
- Save both passwords in the password manager.
- Make an encrypted backup on at least one other device / offline medium.
- Note the alias is `upload` and it was created with 2048-bit RSA.

---

## Additional Play Console Tips

**Category:** Productivity or Lifestyle  
**Content Rating:** Everyone  

**Privacy Policy:** Required. You can use a simple statement such as:

"AlarmFree does not collect, store, or transmit any personal information. All data (alarms, history, custom sounds) stays exclusively on the user's device. No network calls are made."

Link to a privacy policy page (you can host a simple one on GitHub Pages or use a free generator).

**Graphics required:**
- App icon: 512 × 512 px
- Feature graphic: 1024 × 500 px
- Screenshots: minimum 2 (phone), ideally 4–8 showing different screens and challenges

**Keywords you can use:**
alarm clock, wake up, challenges, heavy sleeper, offline, no ads, math alarm, reaction game, anti-snooze

**App package name (must match exactly when creating the app in Play Console):**
com.fidderr.alarmfree

---

This file is kept in sync with the project. Update the "What's New" section for every new release you publish.