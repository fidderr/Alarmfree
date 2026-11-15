# Bug reports

How a report gets to you, how to read it, and how to reproduce it.

## What the user does

Settings, Help and feedback, "Report a problem or idea". They pick Problem or Idea, answer "What happened?", "What did you
expect?" and "Roughly when?" (filled in with the last alarm that rang), and choose:

- **Attach the diagnostic log** (on by default): the app's own event log, see below.
- **Attach my alarm setup** (off by default): the same file as Back up, so you can rebuild the exact alarms. It
  holds the names and notes of the alarms, so the screen says so.

"Show what is sent" shows the whole report text. "Send with my email app" opens the user's own email app with the
subject `AlarmFree 0.5.1 (12401): <first words>` and the files attached. Nothing leaves the phone unless the user
presses send there; the app has no internet permission. With no email app, the report is saved to Downloads
(`alarmfree-report-<date>.txt`, and `alarmfree-setup-<date>.json`) and the screen shows where to send it.

After a panic or a missed alarm, Home shows a dismissable banner "Something went wrong last time. Send a report?"
once (the newest log line is remembered as seen).

The address is the constant `CONTACT_EMAIL` in `src/report.rs` (the public developer address of the Play listing).

## What a report looks like

`alarmfree-report.txt`, in sections:

```
AlarmFree report
Type: Problem            (or Idea)
App: AlarmFree 0.5.1 (12401)
Created: <time with offset>
What happened: / What did you expect: / Roughly when:      the user's words

== Device ==             maker, model, Android version and build, language, time zone (id and offset)
== Permissions ==        every check of the Reliability screen: ok or BLOCKED, plus the channel importance
== Alarms (n) ==         one line per alarm: id prefix, time, repeat, on/off, snooze, challenge types,
                         skipped/paused. Never the name or the note.
== Alarm setup file ==   attached or not
== Log ==                the last 600 lines of the diagnostic log
```

Alarm names and notes are never written to the log. As a second guard the report replaces any occurrence of an
alarm's name or note (three characters or more) with `[text]`.

## The log

One line per event: `<local time with offset> <tag> <message>`. Two files of 256 KB (`files/sentinel/diag.log` and
`diag.1.log`); the older one goes when the newer is full. Tags:

| Tag | Meaning |
|---|---|
| `start` | the process came up: "for the screen" or "for a wake-up with no screen"; the screen opened (cold, or warm when a wake-up started the process first) |
| `wake` | why the app was woken without a screen (written by the SDK): `BOOT_COMPLETED`, `TIME_SET`, `TIMEZONE_CHANGED`, `MY_PACKAGE_REPLACED`, an alarm going off... |
| `permissions` | the Reliability checks at the start |
| `rearm` | the alarms armed at a start or wake-up: how many, and what the phone's restart ended, missed, or finished |
| `alarm` | saved, added, enabled, disabled, deleted: id prefix, time, repeat |
| `schedule` | what AlarmManager was given: `next=` is the local time of the next ring |
| `ring` | `started` with the lateness (`3s after its time`, or `after snooze 2`), `snoozed`, `stopped how=solved`, `stopped how=ended outside the screen`, and ring ends by the limit, the backup screen or a restart |
| `missed` | an alarm whose time passed without it ringing (`at=` is when it was due) |
| `sentinel` | the guardian restarted a frozen app, or handed over to the backup screen |
| `panic` | a Rust panic: thread, file and line, message |
| `report` | the user opened the email app |

## Reading a report: where to look

- **"It did not ring."** `alarm` lines: was it on, which time and repeat? `schedule`: did it have a next ring?
  `rearm` and `wake` after the phone's restart or a time or time zone change: was it armed again? `missed`: it
  never rang. `permissions`: any BLOCKED means the phone could stop it (battery, exact alarms, background). Device
  and Android version say which maker's rules apply (see the Help page of the app).
- **"It rang late."** `ring started ... Ns after its time`. A few seconds is normal; minutes point at battery
  optimisation or Doze. Compare with `schedule next=`.
- **"It crashed / froze."** `panic` has the place. `sentinel` lines show the guardian restarting a frozen screen.
  A `start` after a gap without a `ring stopped` means the process was killed.
- **Time zone trouble.** The report's `Time zone:` and the `wake ... TIMEZONE_CHANGED` lines, and each alarm's
  time are in local time with an offset in front.

## Reproducing

```
SERIAL=emulator-5558 bash devtest/repro.sh alarmfree-report.txt [alarmfree-setup.json]
```

It installs the debug APK, sets the emulator's time zone from the report, puts the alarms of the setup file in the
app (without it: rebuilds them from the report's alarm lines), starts the app and prints the report's log, what the
app logged here, and the alarms. Build the APK first: `TARGET=x86_64-linux-android ./build.sh`. Then move the
emulator's clock (the devtest scripts do this) to the time in the report and watch.

## Privacy and Google Play

The report is built on the phone and only leaves it through the user's own email app. The Play Console Data safety
form: see `store/production-access-answers.md`, section "Data safety".
