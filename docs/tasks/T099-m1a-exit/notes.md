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
