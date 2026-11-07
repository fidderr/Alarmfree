# AlarmFree on Google Play: what goes where

Everything Play Console asks for, ready to paste. Package name: `com.fidderr.alarmfree`.

## Store listing (Grow users → Store presence → Main store listing)

| Field | Where it is |
|---|---|
| App name, short and full description | `listing/<language>.md`, one file per language: en-US, nl-NL, es-ES, fr-FR, de-DE. Add the other four as translations under *Manage translations → Add your own translations*. |
| App icon (512 × 512) | `graphics/icon-512.png` |
| Feature graphic (1024 × 500) | `graphics/feature-graphic-<language>.png`, one per language |
| Phone screenshots (1080 × 1920) | `screenshots/<language>/`, eight per language, numbered in the order to upload |
| Privacy policy | https://fidderr.github.io/Alarmfree/privacy.html (from `docs/` on `main`, served by GitHub Pages) |
| Category | Tools (or Lifestyle) |
| Tags | Alarm clock, Productivity |

"What's new" goes into the production release itself, from the same files.

To make them again after the app changes: build the debug APK, then
`bash devtest/store_shots.sh` (screenshots, all languages, about 8 minutes on the af36 emulator) and
`python store/make_graphics.py` (feature graphics).

## App content (Policy and programs → App content)

| Section | Answer |
|---|---|
| Privacy policy | the URL above |
| Ads | No, the app has no ads |
| App access | All functionality is available without special access (no login) |
| Content rating | Category *Utility, Productivity, Communication or Other*; no to every question about violence, sexuality, language, drugs, gambling, user interaction, sharing location and digital purchases. Expected: Everyone / PEGI 3 |
| Target audience | 13 and older (13-15, 16-17, 18+). Not under 13: that brings in the Families policy for nothing |
| News app | No |
| Data safety | **No** to "Does your app collect or share any of the required user data types?". Nothing leaves the phone: alarms, history, sounds and photos stay in the app's storage, the scanner (ZXing) and step counter work on the phone, there is no network code. With "No", Play skips the rest of the form |
| Government app, financial features, health | No |
| Exact alarm | Yes, core functionality: Alarm clock |
| Full-screen intent | Yes, core functionality: Alarm clock; pre-grant at install: Yes |

### Foreground service (media playback)

| Question | Answer |
|---|---|
| Foreground service type | Media playback |
| What the task does | Plays the alarm sound while an alarm rings, from the moment it goes off until the user stops it by completing the alarm's challenges or it reaches its ring limit. |
| What the user sees | The alarm screen over the lock screen and an ongoing notification for the ringing alarm. |
| Impact if the system delays or stops it | The alarm would not ring, or would stop ringing, at the time the user set: the user oversleeps. |
| Video | Upload `release/AlarmFree-foreground-service.mp4` (made by `devtest/store_video.sh`) to YouTube as *Unlisted* and paste the link. |

### Other permissions reviewers may ask about

| Permission | Why |
|---|---|
| `REQUEST_IGNORE_BATTERY_OPTIMIZATIONS` | Optional, asked from the Reliability screen when a battery saver can stop an alarm overnight: the core function of an alarm clock. |
| `SYSTEM_ALERT_WINDOW` | Optional, brings the alarm screen up while the phone is in use. |
| `ACTIVITY_RECOGNITION` | Only for the Steps challenge, when the user adds it. |
| `CAMERA` | Only for the Scan challenge, when the user adds it. |
| `INTERNET` | Needed by the built-in web view that draws the screens. No connections are made. |
