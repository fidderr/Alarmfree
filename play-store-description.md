# AlarmFree: Google Play listing

## Title

AlarmFree

## Short description (max 80 characters)

Alarm clock you only stop by solving a challenge. No ads, no tracking, offline.

## Full description

Wake up for real.

AlarmFree is a free alarm clock without ads, accounts or tracking, and it works offline. When it rings you do not just tap a button: you do a sum, walk some steps, scan the barcode on your toothpaste. Until you are up, it keeps ringing.

**Eight kinds of challenge**
- Math: a sum, from easy to extreme
- Memory: repeat a sequence of lit cells
- Typing: copy a passage, in your language
- Shake: shake your phone a number of times
- Steps: get up and walk
- Scan: any barcode or QR code, or one you saved
- Hold: press a bubble until it fills
- Reaction: pop targets before the time runs out

Up to 10 per alarm, in your order or at random. Try each one out before you depend on it.

**An alarm that rings**
- Rings from a part of the app that nothing on screen can stop, also when the app was closed long ago
- Comes back if it is killed, and keeps the volume up
- Still rings after a restart if it is less than 15 minutes late, and before you first unlock your phone
- Tells you what your phone is holding back, and takes you to the setting
- A test alarm that shows what your phone really does

**Your alarms, your way**
- Weekdays or one date, with your own label and a note for when it rings
- Quick alarms for a nap, in two taps
- Skip the next ring, or pause everything for a holiday
- 10 tones or your own sounds, fade in, vibrate first, minimum volume per alarm
- Snooze as often and as long as you like, or not at all
- A limit for how long it rings, and a wake-up check a few minutes later

**And**
- History with your streak, how long it takes you, and what went wrong if something did
- Home-screen widget with your next alarm
- Five themes and five accent colours
- English, Dutch, Spanish, French and German
- Works on tablets and foldables
- Open source (MIT)

Needs Android 12 or newer.

## What's new

- Rings from its own process: sound within a fraction of a second, whatever state the app is in
- New design: compact, with everything one tap away
- Quick alarms, pause until a date, skip, sort, drag, wake-up check, ring limit, widget, backups
- Reliability screen and test alarm
- Streak and chart in the history
- Needs Android 12 or newer

## Store settings

**Category:** Tools or Lifestyle.
**Content rating:** Everyone.
**Data safety:** no data collected or shared.
**Privacy policy (required, also for an offline app):** "AlarmFree does not collect, store elsewhere or transmit any personal information. Alarms, history and your own sounds stay on your device."

**Keywords:** alarm clock, wake up, challenges, heavy sleeper, offline, no ads, math alarm

**Graphics:** icon 512 × 512, feature graphic 1024 × 500, at least 2 phone screenshots.

**Package name (never changes):** com.fidderr.alarmfree

## What Play Console asks about

Each of these needs a declaration or a short justification, and the app's core function is the answer to all of them.

| Permission | Why |
|---|---|
| `USE_EXACT_ALARM`, `SCHEDULE_EXACT_ALARM` | An alarm clock. |
| `USE_FULL_SCREEN_INTENT` | The alarm screen over the lock screen. |
| `FOREGROUND_SERVICE_MEDIA_PLAYBACK` | Playing the alarm sound. Play asks for a short video of it. |
| `REQUEST_IGNORE_BATTERY_OPTIMIZATIONS` | Optional, asked by the user from the reliability screen. Play only allows it when battery optimisation harms the core function; a phone maker's battery saver holding back an alarm is that. |
| `SYSTEM_ALERT_WINDOW` | Bringing the alarm screen up while the phone is in use. Optional. |
| `ACTIVITY_RECOGNITION` | The steps challenge, only when the user adds it. |
| `CAMERA` | The scan challenge, only when the user adds it. |
| `INTERNET` | Dioxus talks to its own page over a socket on the phone. No data leaves it. |

How to build the bundle: see the README.
