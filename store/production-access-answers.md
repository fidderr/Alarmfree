# Production access questionnaire: answers for AlarmFree

For the Play Console form that comes after the closed test. The answers are in your voice; the template from the
testing provider's report was written for any app and is wrong for this one in four places (marked "Corrected"
below). Check the [brackets]: they are facts only you have (numbers, dates, the provider's name as you want it
said).

## Q1. How did you recruit your testers?

I used a paid closed-testing service, Testers Community, to get testers who run the app on a range of real phones
and Android versions, because I do not have a group of testers around me. They were people who use their phones
every day, which is the audience of an alarm clock: no special skills needed. [Number] testers took part over
[number] days.

## Q2. How easy was it to recruit testers?

Easy. The service supplied the testers, and they installed the closed-test build from the Play link without help.

## Q3. Describe the engagement you received from testers during the closed test

The testers installed the app, went through its functions (setting alarms, the challenges that switch an alarm off,
the Reliability screen, backup and restore, themes and languages), and reported back in two written reports. They
reported that all functions worked and that they found no crashes or bugs. They also gave suggestions. [Add
anything you saw yourself in the Play Console statistics: number of testers that stayed opted in, sessions.]

## Q4. Provide a summary of the feedback that you received from testers

How it was collected: two written reports from the testing service, plus the Play Console's closed-test numbers.

- Findings: no crashes, no bugs, every function worked as described.
- Suggestions: a short walkthrough for new users with a clear Skip; store screenshots that show the features
  (the challenges, an alarm ringing) with short captions; a way to rate the app that does not nag; a way to send
  feedback; a help or FAQ section; accessibility (text size, audio cues). Also named: A/B testing and performance
  monitoring, which I did not take on (see Q8).

## Q5. Who is the intended audience of your app?

People who want an alarm clock they can customize, without ads, accounts or tracking: students and working
people who have trouble getting up, shift workers, and anyone who sleeps through ordinary alarms. The challenges
(a sum, a pattern to repeat, steps to walk, a code to scan) make it for people who need more than a snooze button.
Everyone old enough to use a phone alarm; the app collects nothing from anybody.

## Q6. How does your app provide value to users?

- An alarm that does not stop until you are actually awake: challenges such as math, memory, typing, steps, shaking,
  holding and scanning a code, with a slow backup challenge so nobody is locked in.
- It tells you when your phone would stop an alarm (battery saver, exact alarms, background limits) and has a test
  alarm to prove it works, instead of failing quietly.
- Privacy: no ads, no accounts, no analytics, no tracking. Alarms stay on the phone; it is open source.
- Free, and in five languages (English, Dutch, German, Spanish, French).

## Q7. How many installs do you expect in your first year?

Your choice. A sober suggestion for a new free app with no marketing budget is the lowest bracket the form offers
(for example under 1,000, or 0 to 10,000 if that is the smallest option): most of it will come from search and
word of mouth. The template's 10,000 to 100,000 is not an estimate I can support. Pick the bracket you would
bet on, since Google may compare it with what happens.

## Q8. What changes did you make to your app based on what you learned during the closed test?

Corrected: the template said "we plan to"; this lists what was actually done, in the update that followed the test
([version], [date]).

- Added a short introduction for new users: three cards (what the app does, the challenges, why the setup
  matters), a Skip on every card, and "Show intro again" in Settings. It leads into the existing first-run setup.
- Added a Help page: why an alarm might not ring (with the steps per phone maker), how challenges and the backup
  challenge work, how to back up and restore, and what the app collects.
- Added "Rate AlarmFree" to Settings. It opens the Play listing when the user taps it; there is no pop-up and no
  automatic prompt.
- Added "Report a problem or idea" to Settings: the user writes what happened, sees exactly what will be sent, and
  sends it from their own email app. It can attach a diagnostic log that the app keeps on the phone (events and
  times, never alarm names or notes) and, if the user chooses, their alarm setup. After a crash or a missed alarm
  the app offers to send a report once.
- Made the screens work with large text and screen readers: [list what the accessibility check changed].
- Replaced the store screenshots with ones that show the features, each with a short caption, in five languages.
- Not done on purpose: A/B testing and remote performance monitoring, because both need analytics, which would break
  the promise of no tracking.

## Q9. How did you decide that your app is ready for production?

The testers found no crashes or bugs and every function worked. Before the test I ran the app's automatic checks
and a long scripted test on Android 12 and Android 16 emulators and on my own phone [confirm this is true for the build you tested]. After the test I made the
changes above, and ran the same checks again. The open points were suggestions about onboarding and help, not
faults, and those are done. [Add anything about crash-free numbers from the Play Console.]

## Q10. What did you do differently from what was expected or planned?

Corrected: the template said the feedback was "overwhelmingly positive" in general terms. The honest summary:
the test found no bugs, so the changes are about first-time use and support, not repairs. I took the suggestions
that fit the app and left out the two that conflict with it (analytics-based A/B testing and performance
monitoring). [If the number of testers or days was less than planned, say so here.]

---

## Data safety (Play Console, App content)

AlarmFree does not send data off the phone. The one new way data leaves the phone is the bug report, and it only
does that when the user presses send in their own email app. Google's Data safety help defines collecting as
"transmitting data from your app off a user's device"; the app transmits nothing, and the user-initiated send is done
by the email app, not by AlarmFree. So the form stays:

- Does your app collect or share any of the required user data types: **No**.
- Is all of the user data collected by your app encrypted in transit: not asked when nothing is collected.
- Data deletion: not applicable (nothing is collected).
- Privacy policy: the page you already have in the listing (source: `docs/privacy.html`, changed: it now describes reports). Check that the URL in the Console serves the new version.

Corrected: the template and the old privacy page said the app needs the INTERNET permission for a web view. The
current app is not a web view and the manifest has no INTERNET permission; the old row is removed.

A cautious alternative, if you prefer to over-disclose: Data types > App info and performance > Diagnostics: tick
"Collected", not shared, optional (the user can choose), purpose "App functionality". It is not required by the
wording above; ask Play support if unsure. The policy text is the same either way.

Also in the Console after this release: Target audience and content: unchanged. Ads: No ads. App access: all
functionality is available without login.
