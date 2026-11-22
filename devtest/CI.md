# The suite on GitHub

`.github/workflows/emulators.yml` builds the x86_64 debug APK once and runs the full suite (what `EVERYWHERE=1 bash devtest/all.sh` runs on the PC) on Android 10 to 16, one GitHub machine per version, all at the same time.

- Start: Actions > Emulators > Run workflow (pick a version or all), or `gh workflow run emulators.yml --ref <branch> -f version=all`. The workflow file must be on the default branch for that. A push to `ci-emulators` also starts it (one version, or `[avds: afg14 msd29]` / `[avds: all]` in the commit message).
- Results: the table per version is in the run's summary page. Per version the artifact `results-<avd>` holds `devtest-out/` (results.txt, log, pictures) as a .tgz. Artifacts are kept 3 days.
- Duration: see the table (minutes) of the last run.
- On a runner the suite uses `devtest/linux.sh` (start and stop of the emulator), AVDs from `devtest/ci/make_avd.sh`, and `devtest/ci/run.sh`. Windows behaviour is unchanged.
- Known SDK bug (mobile-sentinel 0.7.0, `run_gradle` in build_sentinel.rs): it starts the relative path `target/sentinel-android/gradlew` with that folder as working directory, which fails on Linux ("No such file"). The workflow works around it with a symlink; the SDK should use an absolute path.

- Options per version, joined by `+`: `msd30+default` (AOSP image without Google APIs), `afg15+c2` / `c3` (2 or 3 emulator cores instead of 4). Used in the push commit message: `[avds: afg15 msd30+default]`.
- Each job also writes `host_load.txt` (load and free memory every 15 s) into its results; a failed scenario has `<name>.anr.txt` (system stack dumps) and `<name>.audio.txt` (audio server state).
- Asking again: a scenario that repeats a step (a lost touch, a machine that used a limit up) calls `retry_used` (lib.sh). Its PASS line says `(ASKED AGAIN: ...)` with the host load against the cores at that moment; the job summary lists every use, and a use on a machine that was not starved (load below the cores) is a FINDING that fails the job. s22b does not ask again: its purpose is that one hold ends the alarm.
