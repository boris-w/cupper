# T087 report

- Executor: subagent (TypeScript engineer). Same executor as T071.
- Branch: `feat/T087-host-client-sync`, cut from main `bf1b787`
- Date: 2026-10-19
- Outcome: everything in the brief is done, and the CI run in a clean worktree passes with UNIT-hc-sync green. The task came to 609 lines, above the 450–560 estimate and well under the ~1,500 limit.

## What I did

**`packages/host-client/src/sync.ts` (`StreamSync`)**
- One stream's pure core, with no I/O. The rules follow the Rust `Mirror` one by one; the difference is that the view lives in the caller's `StreamSink` (to be implemented by T072 and T080).
- What the sink receives:
  - `reset()` when the view is dropped;
  - `apply(body, event?)` for each snapshot op or live event;
  - `live()` when the view is complete and events follow, which is after the last snapshot page or after a replay that fits the cursor;
  - `failed(error)` when the subscription fails.
- If `apply` throws, it is treated as a `ViewError`.
- Each entry point (`subscribed`, `snapshot`, `events`, `reset`) returns the parameters to subscribe again with, the same as `Mirror::handle`.
- `events` and `ops` are first read as `unknown[]`, then parsed one at a time with `zEvent.safeParse` or `zEventBody.safeParse`.
- An event of a known type with malformed data is logged and still passed to the sink, which shows a placeholder.
- The three counts follow the T071 ruling at the end of 5.7.6:
  - `gaps`;
  - `duplicates`;
  - `resets`: the host's `stream/reset`, plus each time a complete view is dropped.

**`HostClient.subscribe(stream, sink)` and `counters` (in `client.ts`)**
- Built only on T071's hook points. In its constructor, the client registers its own `onNotification` and `onState` listeners first, so streams see every notification and state change before any other listener.
- `stream/*` notifications are routed to the matching `StreamSync`. `stream/subscribe` goes through `call`, so its result keeps its place among the notifications.
- On `ready` every stream subscribes again from its cursor. On `reconnecting`, `failed` or `closed` it waits and keeps the cursor.
- Unsubscribing removes the stream from routing and sends `stream/unsubscribe`. A late subscribe result is ignored.
- Subscribing to the same stream twice throws.

**`index.ts`** exports `StreamSync`, `StreamSink`, `SyncCounters` and `Cursor`. `notes.md` holds the estimate, line counts and trade-offs.

## Key trade-offs

- **Where the view lives:** the sink owns it, so "drop the view" means calling `sink.reset()`.
  - The sink may build the snapshot in a staging view until `live()`. That matches Rust, where `view()` stays empty until the last page.
- **Events with an unreadable envelope:** earlier events in the same message are still applied. The stream then subscribes again without a cursor, so the snapshot goes around the bad event, same as in Rust.
- **Counting rules:** gaps and duplicates are classified by the event's first seq, which is `from` if present, otherwise `seq`.
  - A first seq at or below the cursor is a duplicate, including a range event that overlaps it.
  - Any other mismatch is a gap.
  - `resets` is only counted when a cursor existed. An error halfway through loading a snapshot is not counted, and neither is a `stream/reset` that arrives while waiting and is ignored.
  - Result: resuming from the cursor with the same `data_epoch` leaves all three counts at 0, and an `epoch_changed` counts 1 reset with no duplicates for the reused seqs.
- **Malformed subscribe result:** it goes to `sink.failed` as code `-32603`, and the stream stays waiting. Rust disconnects here, but the renderer does not hold the connection, and reconnecting would only bring back the same result.

## Verification

- `cargo xtask ci pr --record docs/tasks/T087-host-client-sync/ci/` in a clean worktree, at `75fa5ce`, all pass: rust 1073, protocol 176, ts 301, license 1, testdata-scan 2. The summary is committed (`87f7a56`) and lists UNIT-hc-sync as `pass`.
- Against the acceptance criteria:
  - [x] UNIT-hc-sync has 10 tests:
    - 8 test the pure core: the snapshot pages build the view; snapshots for any reason, plus a replay that does not fit the cursor; a gap or a duplicate subscribes again with the cursor and is counted; a `ViewError` or unreadable envelope subscribes again without the cursor, including during a snapshot; `stream/reset`; a `data_epoch` change; data ignored while waiting, and subscription failures; one malformed event or op affecting only itself.
    - 2 run end to end through `connectShell` and the mock host: a snapshot, then the host drops the connection, the client reconnects on its own and resumes from its cursor with `{after_seq, epoch}`; and data only reaching streams this client subscribed to, with a result that arrives after unsubscribing being ignored.
  - [x] Test titles start with `UNIT-hc-sync`, and CI matched them.
  - [x] CI passes in a clean worktree.
- Mutation checks, each made and then reverted; every one made the matching test fail:
  - `stream/reset` dropping the cursor;
  - a snapshot not dropping the cursor;
  - the duplicate/gap boundary shifted by one;
  - a whole page restarting on an op without a type;
  - resubscribing after a reconnect only for streams without a cursor.
- No new dependencies. No personal information in the diff, and no claude.ai links in the commit messages.

## Size

- 609 lines: 302 code and 307 tests.
- That is above the brief's 450–560 estimate, about 22% over my own ~500, and under the ~1,500 limit.
- Most of the overrun is in `sync.ts` (226 lines against ~150 estimated). The file header spells out each rule matching the Rust `Mirror`, every member of `StreamSink` and `SyncCounters` is documented, and Prettier spreads the four snapshot-page checks over several lines.

## Commits

```
87f7a56 docs(task): T087 pr CI summary
75fa5ce docs(task): T087 notes: estimate, line counts and trade-offs
1847eb6 feat(host-client): add HostClient.subscribe with resubscription on reconnect
b2464c9 feat(host-client): add StreamSync, the sync rules of one stream
```

## Open issues and findings outside the scope

- **T072 and T080: implementing `StreamSink`.**
  - Snapshot ops come through `apply(body)` with no event. Live events come through `apply(body, event)`.
  - The view is complete once `live()` is called.
  - If `apply` throws, the stream starts over without a cursor.
  - Bodies of a known type with malformed data still arrive, as the unknown member (`isKnownEvent` is false), so the view needs a placeholder for them.
- **T080: test hook.** `client.counters` is the sum across all streams; E2E's `window.__cupperTest` can export it as is.
- **T080: production validation.** The "production builds only validate non-delta events" optimization is still not done; every event is parsed with zod.
