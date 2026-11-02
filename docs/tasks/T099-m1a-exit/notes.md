# T099 notes

## Candidate and branch

- Exit candidate: `18b3640` on main (T215 merged and archived).
- This branch starts at `18b3640`. Against it, the branch may change only workflows and test tooling, and this task's folder; the nfr1 runs on the branch test the product code of `18b3640`. Each run's commit is checked with `git diff --stat 18b3640 <commit>` before it is counted.

## Estimate

- `e2e-repeat` (the only code change): the tests tagged `@focus` run after the rest one at a time, as `xtask ci e2e` runs them, so that the whole nfr1 suite runs at CI's parallelism in one call; a `load` input starts busy processes while the tests run (the slow variant). About +30 −10.
- The record, the notes and the committed summaries do not count.
- Total about 40, well under the stop line 200.

## What the runners cannot produce

Checked every M1a ID of `tests/ids.toml` against its suite and trace-ids' rules: a result counts only in the suite it is registered in, and `--milestone` reads committed summaries of one commit only. Eight IDs come from no runner and had no evidence:

- PERF-host-cold-start, PERF-index-snapshot, PERF-host-memory, PERF-snapshot: the `perf` job, never on a hosted runner (T197).
- PERF-bridge-gate: the `@perf` tests of `tools/e2e/src/gate.spec.ts`, in the `e2e` job, which leaves them out on a hosted runner.
- IT-sys-peer-current: registered in `pr`, but since T211 every PR job runs on a hosted runner, where the peer checks count for -macos15 or -macos26. The orchestrator chose to register it in `nightly` (3ad4747): it comes from the author's nightly run.
- IT-ticket-auth: still `conditional` though K2 passed (T078) and T079 is not dispatched; now `na` (3ad4747), as plan 1.6 says.
- MAN-keyboard: no evidence at all. The procedure is `man-keyboard.md`.

## A: the nightly run on the author's machine

`perf` and `e2e` both append to `testdata/perf/history.jsonl`, and a job refuses a work tree that is not clean, so after one of them the other would not run: the composite `nightly` checks once, at its start. So the whole composite runs, in the main work tree at `18b3640`:

```sh
git rev-parse --short HEAD         # 18b3640
git status --porcelain             # empty
cargo xtask ci nightly --record .private/t099-nightly
```

- About 45–60 minutes with a warm target (60–90 from scratch); an estimate, not measured on that machine.
- Nobody at the machine: `ui` and `e2e` open windows and take the focus. The screen must stay unlocked; the jobs keep the display awake (`caffeinate -d`).
- Afterwards:
  - to T099: `.private/t099-nightly/nightly-18b3640ee377.json` (copied into `ci/local/` here and committed); on a failure also the job's logs, `.private/ci-logs/<run>/`;
  - to main, by the orchestrator as usual: `testdata/perf/history.jsonl`.

## Candidate 18b3640: the pr run failed (superseded)

### The failure

- `ci` on main at 18b3640, run 41392847292: 7 of 8 jobs passed; `rust (macos-15)` failed in its `test-host-test-harness` step, 2284 passed and 1 failed: `it_lifecycle_startup_ends_what_the_last_run_left_running` (`crates/cupper-host/tests/lifecycle.rs:119`), "SIGTERM, then SIGKILL", left `[Some(15), Some(15)]`, right `[Some(15), Some(9)]`.
- The same test failed the same way earlier that day on main at 8f96864 (run 41377958324, its rust job's `rust-test-host-test-harness.log`), unrecorded until now.
- The rust job stops at its first failed step, so its later steps (`test-host-shuttle`, `golden`, `test-native`) did not run: that is why PROP-stream-seq showed "no test ran" in a trace-ids preview of these summaries, not a gap of its own.

### Cause: the test races its own process, not the product

- The test starts `sh -c "trap '' TERM; /bin/sleep 60; :"` in a session of its own, records it as a group of a previous run's adapter, and starts the host right away.
- The host's startup step 6 (`supervisor/reap.rs`, `reap`) sends SIGTERM to the recorded groups, waits until they are gone or `reap_term_wait` is over (200 ms in this test), then sends SIGKILL.
- Nothing makes the test wait until the shell has run `trap '' TERM`. When the shell is that slow (on a 3-core runner the binary's 19 tests run three at a time, each with a host and its children), SIGTERM comes first and the shell dies of it: `[15, 15]`. Once the trap is set, SIGTERM is ignored and only SIGKILL can end it, so `[15, 15]` means exactly this.
- The host did what the design says. The only other test that ignores a signal (`cupper-sys` `proc.rs:465`) prints `ready` after its trap and waits for it.
- Not reproduced on the author's machine: it would have meant building the host's tests while the user worked at it.
- The orchestrator dispatched T216 (test only) for the fix; T099 depends on it. Every run is redone on the next candidate.

### What ran on 18b3640 anyway (supporting evidence only)

| Run | What | Result |
|---|---|---|
| e2e-repeat [41393695454](https://github.com/boris-w/cupper/actions/runs/41393695454) | nfr1 × 20, 2 at once, load 0, on 8024616 (product = 18b3640) | the rest 180/180 in 12.8 min (median 7.5 s, max 20.8 s), `@focus` 60/60 in 5.0 min (median 5.7 s); unexpected 0, flaky 0, skipped 0, no retry |
| e2e-repeat [41393728018](https://github.com/boris-w/cupper/actions/runs/41393728018) | nfr1 × 5, 2 at once, load 2 (2 busy processes on 3 cores) | the rest 45/45 in 4.0 min (median 9.3 s, max 25.0 s), `@focus` 15/15 in 2.0 min (max 31.9 s); unexpected 0, flaky 0, skipped 0, no retry; load averages at the end 21.80 25.34 17.93 |
| nightly [41393760316](https://github.com/boris-w/cupper/actions/runs/41393760316) | 6 jobs | all passed |
| ci [41392847292](https://github.com/boris-w/cupper/actions/runs/41392847292) | 8 jobs | rust failed as above; the other 7 passed |

The 12 tests per run: `nfr1.spec.ts`'s 10 (7 of them not `@focus`) and `nfr1-restart.spec.ts`'s 2 (E2E-NFR1-5).

### What T216–T218 found and fixed before the next candidate

- **T216** (test only, PR #34): the tool script prints `ready` after its trap and the test reads it before it raises the host. A 5 s delay before the trap reproduced `[15, 15]` on the old test and passed on the new one. A runner repeat of the whole lifecycle binary turned up T218.
- **T217** (product, `ui-core`, PR #35), from MAN-keyboard: a click on the request panel now moves the keyboard focus into it; RP-3 reads the page's focus (`document.activeElement`), so a request that arrives while the window is in the background is still taken over; a lost blur no longer lets a later panel take the focus from the sidebar. Design 5.7.8 RP-3 got its wording.
- **T218** (product, host runtime, PR #36): `it_recording_export_two_instances_read_back_and_sanitize` failed 2 in 600 under load. Cause: in the ~10 ms between a prompt failing with `adapter_exited` and `AdapterExited`, the session was still linked to the dead instance and a new prompt went to it and was dropped. The runtime now lets `AdapterExited` end the turn, after the instance is gone (design 5.2.6's crash table). 600/600 after the fix.

## Candidate ec459fd

- main after T216, T217, T218 and T099's first part (PR #33, merged as 7b67605); the branch `chore/T099-m1a-exit-2` starts there.
- Every run of the brief is done again on it; `e2e-repeat` runs with `--ref main`, so it tests that commit itself.
- MAN-keyboard was done on 18b3640's e2e build: its evidence is bound to Electron 44.9.0, not to a commit (brief, second ruling).

## A was not run

The user decided that load and performance runs stay off their machine for now (`docs/meetings/2026-11-02-perf-local/minutes.md`, D1). So PERF-host-cold-start, PERF-index-snapshot, PERF-host-memory, PERF-snapshot, PERF-bridge-gate and IT-sys-peer-current have no result, and `trace-ids verify --milestone M1a` stops at "7 of 288 IDs not done" (IT-sys-peer waits for -current). The registry is left as it is; the record lists the seven with their reasons and the options for the sync meeting.
