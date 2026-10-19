# T072 report

- Executor: subagent (TypeScript engineer). Same executor as T071 and T087.
- Branch: `feat/T072-views-reducer`, cut from main `23cfda8`
- Date: 2026-10-19
- Outcome: everything in the brief is done. The PR CI run in a clean worktree passes with UNIT-views-parity green, and the new nightly `differential` job passes on its own with UNIT-views-differential green. Size: 971 lines.

## What I did

**TS views (`packages/host-client/src/views/`)**
- **`StreamView<T>`** is the base class and implements T087's `StreamSink`.
  - `reset` empties the view and sets `complete = false`; `live` sets `complete = true`; `failed` stores the error.
  - `apply` only folds bodies where `isKnownEvent` is true.
  - Unknown types, and known types with malformed data, go into an `unknown` list (key `unknown`) so the UI can show placeholders. They stay out of the view JSON, because the Rust views ignore them.
  - A body that does not fit throws `ViewError`, and the view is left unchanged.
- **`IndexView` and `SessionView`** keep their state in the generated JSON shapes, and `toJSON()` returns it. The rules follow `cupper-protocol::view` one by one:
  - turns are inserted by `turn_no`; a turn newer than all others takes in `tail`;
  - entries without a turn join the running turn, otherwise `tail`;
  - a request whose turn is not in the view goes to `detached`, and its turn's history page puts it back in place;
  - `late_by_ref`;
  - `ItemPatch` sets only the fields that item kind has; an unknown kind takes all fields;
  - `snapshot.recorded` keeps one entry per phase;
  - `may_still_run` is omitted when false.
  - There is no `trim` and no snapshot op generation; those are host-side.
- **`Versions`** gives each entry a version number for T080's `useSyncExternalStore`.
  - Keys look like `item:<id>`, `request:<id>`, `turn:<id>`, `turns`, `tail`, `detached`, `session`; on the index stream, `project:<id>`, `projects`, `host` and so on. The key `view` changes with every change.
  - Late output also changes the version of the tool call it reports on.
  - `flush()` notifies only the keys that changed, once per call; T080 calls it once per animation frame.

**Cross-validation, UNIT-views-parity (pr)**
- `protocol/fixtures/views.json`: each case is `{name, view, ops, expected}`.
  - The Rust test `view::parity` writes it with `UPDATE_GOLDEN=1` (the convention from 5.9.3); otherwise it checks the file.
  - The TS side folds the same ops and compares the JSON with `toStrictEqual`.
- The cases come from three sources:
  - every sample in `events.json`, with each item and request given its own ID, so every kind of item meets every patch;
  - scenarios: history before the window plus a tail, a detached request put back in place, late output;
  - one deterministic random host stream, folded three ways: the full log, a trimmed snapshot, and the snapshot plus history.
- Wired into `xtask ci protocol`: the Rust step's filter adds `unit_views_parity`, and there is a new TS step `views-ts`.
- `tests/fixtures.rs` now lists `views.json` among the checked files. The file is Rust's own pretty-printed JSON, so it is added to `.prettierignore`.

**Differential test, UNIT-views-differential (nightly)**
- As 5.9.1 specifies, Rust's proptest generates the input. An `#[ignore]` test in `view::parity` uses the existing host model to write 300 random streams, each folded three ways (900 cases), to the file named by `CUPPER_VIEW_CASES`.
- The TS half reads that file. When the variable is not set, the whole group is skipped, so it does not run in PR CI.
- New job `cargo xtask ci differential` (`crates/xtask/src/ci/jobs/differential.rs`), listed only in the nightly composite. It runs the Rust half, then the TS half; the cases file goes in that run's log directory.

## Key trade-offs

- **How JSON equality holds:** the TS views store the event data as parsed by zod. zod leaves Rust-written JSON unchanged except for dropping unknown keys, and Rust drops unknown fields the same way. The TS side round-trips through `JSON.stringify` before comparing, so what is compared is the JSON as it would be sent, and `undefined` values are not ignored.
- **Unknown events stay out of the JSON:** they are kept in `unknown`, in arrival order, with their event and `fallback` when they arrived live. Where placeholders sit in the message stream is left to the UI task, T082.
- **Simplest way to run the differential:** Rust writes a file and Vitest reads it. Rust does not have to start Node, and Node does not have to load TS sources directly.

## Verification

- `cargo xtask ci pr --record docs/tasks/T072-views-reducer/ci/` in a clean worktree, at `b209084`, all pass:
  - rust 1074, protocol 185 (including `views-ts`), ts 314, license 1, testdata-scan 2;
  - the summary shows UNIT-views-parity as `pass`.
- `cargo xtask ci differential --record …` at `65a9ead` passes with UNIT-views-differential as `pass`.
- Against the acceptance criteria:
  - [x] UNIT-views-parity: 8 cases pass on both sides.
  - [x] UNIT-views-differential: on top of the recorded run, I ran it 4 times locally with different random seeds, 900 cases each; all equal.
  - [x] Both IDs match `tests/ids.toml` as they are; no changes were needed.
  - [x] CI passes in a clean worktree.
- Mutation checks, each made and then reverted. Each made between 1 and 4 parity cases fail:
  - removing the notice's patchable field;
  - always writing `may_still_run`;
  - not building `late_by_ref`;
  - a new turn not taking in `tail`;
  - sending a request for an unknown turn to `tail` instead of `detached`.
- Unit tests (`views.test.ts`) cover: versions changing by key, notification only on `flush`, late output touching its tool call; the sink's `reset`, `live` and `failed`; unknown and malformed bodies going to placeholders and leaving the JSON unchanged; five bodies that do not fit throwing and leaving the view unchanged; and, used as a `StreamSync` sink, a `ViewError` making the stream start over.
- No new dependencies. No personal information in the diff, and no claude.ai links in the commit messages.

## Size

- 971 lines: about 520 code and 450 tests. That is 12% over my estimate of about 870, and under the ~1,500 limit.
- `views.json` is about 128 KB of data written by Rust and is not counted.

## Commits

```
1d646dc docs(task): T072 differential CI summary
65a9ead docs(task): T072 pr CI summary
b209084 docs(task): T072 notes: estimate, line counts and trade-offs
223542e test(protocol): differential test of the TS and Rust views, nightly
f4e9be4 test(protocol): cross-check the TS views against the Rust views
6c30923 feat(host-client): add the index and session views with versions by key
```

## Open issues and findings outside the scope

- **Design doc gap.** The CI job table in design 5.1.6 has no `differential` job, and the nightly arrow in its flowchart does not include it. Both need adding. I did not edit the design document.
- **Possible bookkeeping mismatch for the differential evidence.** The recorded summary marks the test with `suite: "differential"`, while `tests/ids.toml` registers UNIT-views-differential under `nightly`. Your nightly run will record it under `suite: "nightly"`. If `trace-ids status` only counts the nightly target, then only that run will count.
- **T080:**
  - Call `versions.flush()` once per animation frame.
  - Subscribe components with `versions.subscribe(key, cb)` and `versions.of(key)`.
  - Use `complete` to show "loading" and `error` to show subscription failures.
  - Production builds still validate every event with zod; the "only non-delta events" optimization is not done.
- **T082:** the `unknown` list (key `unknown`) holds the bodies to show as placeholders, with their event and `fallback` when they arrived live. Their position in the message stream is still to be decided.
- **Regenerating the fixture:** whenever the Rust views or the protocol types change the view JSON, regenerate `views.json` with `UPDATE_GOLDEN=1 cargo test -p cupper-protocol --lib view::parity`, and review the diff in the PR.
