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

## Item 2: E2E-NFR1-4 "after a reload, earlier history ..." — a scroll undone by a render before its event

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

### What the reading of the code had left open (before the probe)

Three ways for the history row to go for good: the list goes back to the bottom and stays (`ScrollKeeper.stuck`), the history state becomes `none`, or the renderer reloads. The probe settled it: the first.

### The probe on the runner, two at a time

Run [41007324304](https://github.com/boris-w/cupper/actions/runs/41007324304), 228eb5e (the test with a recorder, reverted in 82908ce), `times=300`, `workers=2`: **11 of 300 failed** (3.7 %), 10.6–46.8 s each.

| Where it failed | Times |
|---|---|
| `say t4` never visible after the scroll to the top | 8 |
| `say t4` visible, gone by its `boundingBox` | 2 |
| the history button detached during the click (the CI signature) | 1 |

- In every `error-context.md` the list shows turns 17–23: it was at the bottom and stayed there.
- main.log of each: `started`, `host connection`, two `window connected` (the start and the reload); no reconnect, no "renderer gone". host.log: no warning or error.
- The record of the click failure (frames after the reload, ms): the session view complete at 259; the list at the bottom from 291, measured at 341 (`scrollTop` 2492); marks "t23 visible" 411, "scrolled" 488, "click" 625; **no frame saw the list anywhere but at the bottom** until the end (2987). Yet the test had seen `say t4` and the history button between 488 and 625. So the top rows were in the DOM only between frames.

### The cause

- `ScrollKeeper.stuck` changes only in React's `onScroll`. Every render of `Rows` that finds it set puts the list at the bottom (`useLayoutEffect`).
- TanStack Virtual 3.17.11 ends "scrolling" 150 ms after the last scroll event, on a timer that **reads the offset from the DOM** (`observeOffset`: `debounce(..., () => cb(readOffset(element), false), isScrollingResetDelay)`), and renders. A timer is a task: it can run between frames.
- The test scrolls to the top (`scrollTop = 0`, a task) about 70–150 ms after the list last stuck to the bottom. If the timer fires before the next frame dispatches the scroll event: the virtualizer reads 0 and renders the top rows; `onScroll` has not run, so `stuck` is still set and the layout effect writes `scrollTop = scrollHeight`. The virtualizer keeps offset 0 until the next scroll event, so `say t4` and the history button stay in the DOM for a while; the test can see them.
- The next frame dispatches the scroll event: the list is at the bottom, `onScroll` keeps `stuck`, the virtualizer renders the bottom rows. From then on every render keeps it there; the history button is out of the virtual range for good.
- With one copy frames come often and the window is narrow (0/200); with two apps on the runner frames are late and it opens (11/300); the CI failures were in runs with two workers.
- Who else can hit it: a scroll made by script between frames while the list is settling, such as find-in-page or assistive technology. A user's wheel or keys scroll in the frame that also dispatches the event, so they do not.

### Reproduced locally (deterministic, jsdom)

`UNIT-stream-anchor` "a scroll whose event has not come yet is not undone by a render before it": the list at the bottom (`scrollTop` 800 of 1000, a scroll event), then `scrollTop = 0` without an event, then 300 ms for TanStack's own timer. Before the fix it fails: `scrollTop` is 1000. After the fix it is 0.

### Fix (`packages/ui-core/src/stream`, 8525a89)

- The keeper records where the list was when it last saw a scroll (`scrolled`) or put it (`put`): its own writes in the layout effect, and the virtualizer's, through a `scrollToFn` that wraps TanStack's `elementScroll`. These are the virtualizer's corrections as rows are measured.
- `follows(top, height, client)`, before the layout effect puts the list at the bottom: a list more than 1 px from where it was last scrolled or put was scrolled, and its event has not come yet; `stuck` is worked out from it first.
- Growth at the bottom does not move `scrollTop`, so the list still follows what streams in (new unit test "at the bottom it follows what comes in"). The browser clamping a shrunk list leaves it at the bottom, so it stays stuck.
- Not done: the test waiting for the list to settle, or scrolling with the wheel. Either would hide the race rather than end it.
