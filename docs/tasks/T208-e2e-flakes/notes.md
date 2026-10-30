# T208 notes

## Estimate

- Item 1 (`E2E-quit-confirm` idle): test only, about +25 −10 in `tools/e2e`.
- Item 2 (E2E-NFR1-4 history): unknown until the cause is known; a test fix about +20, a product fix in `ui-core` with a unit test about +60.
- Total about 100, under the stop line 150.

## Where the runner's attachments went

Neither failed run kept its attachments. `xtask ci e2e` runs `test-e2e` and then `test-e2e-focus` with the same Playwright output directory, and Playwright empties it at the start of each run; the focus run passed, so `tools/e2e/dist/test-results/` was empty when the job uploaded it ("No files were found with the provided path"). Only the JSON reports in `e2e-macos-15-logs` are left: they name the attachments but do not contain them. So the evidence below comes from those reports, the code and local runs. Reported as out of scope.

## Item 1: `E2E-quit-confirm: idle, ...` — exit 3 from `cupper status`

### What exit 3 means

`crates/cupper-cli/src/run.rs`: 3 is `Failure::HostGone`, "the host is not running, or has exited": the socket is missing or refuses the connection, or the connection ends (EOF, or the peer exited). With `--json` the CLI prints its error as a line on stdout (`{"error":{"code":"host_gone",...}}`), not on stderr, which is why the CI message ended in an empty `stderr`. `Cli.ok()` only showed stderr; it now shows the stdout lines too.

### The host's state at that moment: it had exited, as designed

- The e2e build is `dev-auth` (`apps/desktop/package.json` `build:e2e`, `xtask host-bundle --kind e2e`).
- In a `dev-auth` build the host records its parent at startup (`supervisor/mod.rs`: `parent: cfg!(feature = "dev-auth").then(parent_id)`). The supervisor's watch looks every `supervisor_observe` (1 s); once the parent changed it is orphaned, and with no running turn it exits in order at once (`supervisor/watch.rs`, `WatchCore::decide`).
- The host's parent is the app's main process (`cupper-desktop-native` `launch.rs`, `cupper_sys::spawn`). Playwright's `close` comes when that process has exited (`onExit` of the launched process). So after `close` the host exits at the next look, within 1 s.
- The test then starts `cupper status`. Usually it is answered before that look; when the CLI is late (here a nightly with two workers), the host has stopped accepting or is gone: exit 3.

Design: 5.8.1 (08-security, degraded mode) says a `dev-auth` host whose parent exits is orphaned: no new turns, and it exits once the running turns end. The watch puts the orphaned state before idleness. The 10-minute idle rule of 5.2.13 only decides in a release build. PRD v1.2 section 10 / Q22: "the UI crashing does not affect sessions" holds for release builds only. T007 wrote the test from the idle rule (its notes: "the host exits by the idle rule"), which is the release build's behaviour, not the e2e build's.

### Local evidence (one copy, a probe spec that is not committed)

| `status` started after `close` | exit | output | host gone after `close` |
|---|---|---|---|
| 0 ms | 0 (answered in 6 ms) | status, `clients` without `desktop_ui` | 1,019 ms |
| 1,500 ms | **3** | `{"error":{"code":"host_gone","message":"the host is not running, or has exited"}}` | 1,544 ms |

host.log of both runs, about 0.6 s after `close`:

```
WARN  cupper_host::supervisor::watch  no new sessions or turns  {closed: Orphaned}
INFO  cupper_host::supervisor         orderly exit              {exit: Orphaned, reason: user_quit}
INFO  cupper_host::supervisor::audit  audit  {event: host.exiting, detail: {"reason":"orphaned","force":false}}
```

So the failure is a race the test always runs: `status` against the next look of the watch. Its odds are about the CLI's latency over 1 s.

### Fix (test only; the orchestrator agreed the product is right)

- Kept: no dialog (`close` comes without the stand-in answering).
- Then: the host's processes are gone within 15 s, and the host's own audit says why: `host.exiting` is exactly `[{reason: "orphaned", force: false}]`. A `host/exit` from the app would be `requested`; checked with a probe that answers "quit" with a running turn: `[{"reason":"requested","force":true}]`.
- Dropped: `status`, `pid` and the clients check.
- Title: "... leaves the host to its own exit rule"; the ID is the same.
- The release build's half (the host stays, 10-minute idle rule): IT-idle-exit (`crates/cupper-host/tests/lifecycle.rs`) and UNIT-orphan-policy cover the rule; SMOKE-SIGNED-reattach (T181, not done yet) the host outliving the app. Named in the test's comment.
- Design and plan: no text says "idle rule" for this test except the T208 row of the plan, which quotes the old title.

### On the runner (e2e-repeat)

| Run | Commit | Copies at once | Result |
|---|---|---|---|
| [41002041448](https://github.com/boris-w/cupper/actions/runs/41002041448) | 7771c71 (before) | 1 | 200/200 passed, 1.5–5.4 s each |
| [41003132122](https://github.com/boris-w/cupper/actions/runs/41003132122) | 541d1cb (after) | 1 | 200/200 passed, 1.3–4.1 s each |
| [41007332700](https://github.com/boris-w/cupper/actions/runs/41007332700) | 228eb5e (after) | 2 | 200/200 passed, 1.3–5.7 s each, 244 s in all |

Unloaded the race almost never loses: the CLI answers in milliseconds, against a look every second. The CI failure came from a nightly with two Playwright workers. The fixed test does not race at all: it waits for the exit, which always comes.

## Not failing unloaded

Both tests pass 200 times out of 200 one copy at a time on the runner (runs 41002041448 and 41002049698, both at 7771c71). Both CI failures were in runs with two Playwright workers (`xtask ci e2e`). So `e2e-repeat` got a `workers` input (9733858, default 1; a later `--workers` overrides the `--workers 1` of `test:e2e:one`, checked locally). It stays as a CI change.

## Item 2: E2E-NFR1-4 "after a reload, earlier history ..." — in progress

### What the CI failure says (run 40651531549, the JSON report only)

- The click's call log: the locator resolved to the history button (`加载更早的历史`), the click waited for it to be visible, enabled and stable, then "element was detached from the DOM, retrying", and nothing more for 30 s. Read as: no button with that name came back in 30 s (the call log shows no second resolution; not checked against Playwright's logging code).
- Before that the test had passed: `say t23` visible, the scroll to the top, `say t4` visible, `say t3` count 0. So the list was at the top with the history row rendered when the click started, and the row went away for good during the stability check (two frames).
- The test took 43.8 s; it passes in 22.8 s in a nightly with two workers and 9.5–17.6 s alone on the runner.

### Unloaded on the runner

Run [41002049698](https://github.com/boris-w/cupper/actions/runs/41002049698), 7771c71, one copy: 200/200 passed.

### Locally (one copy)

- The test passes (about 6 s).
- A probe that records the list per animation frame after the reload (scrollTop, scrollHeight, rows, whether the history button is there, sync counters, connection state, the session view's seq): the session view is complete about 80 ms after the reload; the list renders at the bottom, is measured and stays at the bottom; the test's scroll puts it at 0 and the history button appears; no resets, gaps or reconnects. The click then loads turns 1–3.
- The same with the renderer slowed 20 times (`Emulation.setCPUThrottlingRate`): every step 15 times slower, same order, passes.

### What can take the history row away (code reading; none confirmed)

The row is index 0 of the virtual list, shown while the view's first turn is after turn 1 (`StreamList.tsx` `useHistory`). It goes away for good only if one of these happens:

1. **The list leaves the top and stays at the bottom.** `ScrollKeeper.stuck` is set only by `onScroll` (within 24 px of the bottom) and on mount; while it is set, every render of `Rows` scrolls to the bottom (`useLayoutEffect`). Ways to set it while at the top: a remount of `Rows` (a fresh keeper), or a scroll event while the content fits the viewport (an emptied view while `scrollTop > 0`). Neither found: `SessionPage` is keyed by the session ID, `StreamList` only switches on its store, which `Subscriptions` keeps; at `scrollTop` 0 an emptied view fires no scroll event. TanStack Virtual 3.17.11's own scroll writes (`applyScrollAdjustment`, `_retryClampedAdjustment`) only adjust for items above the offset, of which there are none at 0; React's `onScroll` listener is on the element from its creation, before TanStack's, so `stuck` is cleared before TanStack re-renders on the test's scroll.
2. **The history state becomes `none`.** Either turn 1 comes into the view (only a history page brings it; only the button calls it), or the view is empty: a view reset (`StreamSync.#drop`, on any `ViewError` or a snapshot reply) refills within milliseconds, so the row would come back and the click retry; not for 30 s unless the refill never arrives (a lost connection).
3. **The renderer reloads or navigates** (back to the new-session page, no stream list at all): fits "detached and never back" best, but nothing in the app reloads it, and a crash would make Playwright report a crashed page, not a timeout.

Checked and ruled out: late `snapshot.recorded` events for turns outside the snapshot's window (the turn machine drops snapshots of turns other than the current one, and both are written before `turn.completed`).

### Probe on the runner, two at a time (running when work stopped)

- 228eb5e is a probe commit (to be reverted before the PR): the same recorder in the real test, from the reload until the click, with marks at "t23 visible", "scrolled" and "click"; on a failed click (timeout 10 s) it attaches the record as `t208-probe.json`. e2e-repeat keeps `tools/e2e/dist/test-results/` and `repeat.json`, so a failure brings main.log, host.log, `error-context.md` (the page's aria snapshot) and the record.
- Run [41007324304](https://github.com/boris-w/cupper/actions/runs/41007324304): `times=300`, `workers=2`. Not finished when work stopped.

### Next

1. Read run 41007324304: failures, their `t208-probe.json` (body attachment in `repeat.json`), `error-context.md`, main.log ("renderer gone", reconnects) and host.log.
2. If it fails: the record says which of 1–3 above happened; fix the test's wait or the product (stop and report if the product fix is more than small), and add a case that reproduces it.
3. If 300 pass: the load of two copies of this test is not CI's; next try a heavier mix (`times` higher, or two specs), or add the recorder's last frames to the test's failure output for good and wait for CI. Report to the orchestrator before choosing.
4. Revert 228eb5e; after the fix, e2e-repeat both tests 100+ times with `workers=2`; then the PR.
