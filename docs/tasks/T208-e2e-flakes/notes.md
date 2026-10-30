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
