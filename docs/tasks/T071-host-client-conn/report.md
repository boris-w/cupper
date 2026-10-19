# T071 report

- Executor: subagent (TypeScript engineer, desktop and protocol client)
- Branch: `feat/T071-host-client-conn`
- Date: 2026-10-19
- Scope: as narrowed by the second ruling (plan v1.3.9). The sync part (`sync.ts`, `HostClient.subscribe`, UNIT-hc-sync) moved to T087.
- Overall: everything in the narrowed scope is done, and `cargo xtask ci pr` passes in a clean worktree. **The task ended at 1,858 lines, about 6% over the ~1,750 limit** but under the 1,900 hard stop from the first ruling; the reasons are under "Size" below.

## What I did

**ShellAPI protocol part (`@cupper/shell-api`)**
- New types: `HostPort`, `Handshake`, `ConnectionState`, `PrivilegedAction` and `PrivilegedResult`, plus `KnownConnectionFailure` and `ConnectionFailure`.
  - `Handshake` is `InitializeResult` without `host_proof`.
  - `ConnectionState` is an object union; `failed` carries an open-enum `reason`.
- `ShellAPI` gains `host.open`, `host.onConnectionState` and `requestPrivileged`. Doc comments record the agreed conventions: local rejections go through `onData`, and on a reconnect the old connection's data comes before `reconnecting` and the new one's after `ready`.
- Browser mock:
  - `host` is injected by the caller, so shell-api does not depend on host-client in reverse.
  - `requestPrivileged` records every action, and `controls.setPrivilegedResult` sets what it returns.
- shell-api now depends on `@cupper/protocol` for types only.

**`@cupper/host-client` (new package)**
- **Package rules:** `exports` has `.`, `./mock` and `./contract`. A new ESLint rule keeps `node:*` and `electron` out of everything except the tests and `testing.ts`.
- **`frame.ts`:**
  - Encodes and incrementally decodes `u32 | u8 | payload`, encodes and decodes the `0x02` header, and enforces the 64,000-byte limit.
  - An oversized or zero length is rejected from the length prefix alone; blob offsets stay within 2^53−1.
- **`handshake.ts`:** `handshake(port, options)`.
  - Reads `host/hello`, negotiates the version, sends `initialize` once and validates the result.
  - Credentials come from the caller through `auth(hello)` and `verifyHostProof`; `dev_key` and `harness` require `host_proof`.
  - Release builds refuse a host with `weak_auth`.
  - Failure reasons are semantic codes: `version_unsupported`, `unauthenticated`, `forbidden`, `weak_auth`, `host_unverified`, `protocol_error`, and `closed`, which is retryable.
  - The bytes after the response are returned in `rest`, and the caller can abort or time out through an `AbortSignal`.
- **`client.ts` (`HostClient`):**
  - State machine: connecting, handshaking, ready, reconnecting, failed, closed. After the first `ready`, every later attempt counts as `reconnecting`.
  - Frames are cut one chunk at a time, in arrival order even when a listener's request gets an immediate reply, and each chunk is acked right after it is handled.
  - Requests are matched to responses by `id`.
  - On a lost connection, requests in flight fail with `ConnectionLost`. Commands keep their `command_id` and are sent again first on the next `attach`; only `close()` fails them.
  - Notifications and `0x02` chunks go to their listeners. A broken frame closes the port and sets `failed: protocol_error`.
  - `uuidv7()` generates `command_id`s.
- **`shell.ts`:** `connectShell(shell.host, client)` drives the client from the renderer.
- **`mock/host.ts`:** the frame-level mock host.
  - Requests are answered from `methods.json` by default, and a test can override any method.
  - It matches the real host: hello first, nonce check, a failed or repeated `initialize` gets an error and then a disconnect, and `not_initialized` before the handshake.
  - It can push notifications and `0x02` chunks and close connections.
  - `conn.port()` gives a HostPort with no checks in between, usable by T075.
- **`mock/port.ts`:** `createMockShellHost(mockHost)`, the browser mock's `ShellAPI.host`, which plays the windowBridge.
  - It makes the window handshake itself.
  - `initialize` is answered locally with `forbidden/already_initialized`; batch arrays and broken JSON are dropped.
  - What it forwards is re-serialized.
  - Like a MessagePort, it delivers later and in order, and holds data that arrives before the first listener.
  - It reconnects automatically when the host drops the connection.
- **`contract.ts`:** `hostPortContract(setup)`, 8 tests that T076 runs against the windowBridge as well. They cover the handshake and the stripped proof, forwarding with re-serialization, the local rejection of `initialize`, dropped messages, the `0x02` frame, each chunk being its own ArrayBuffer and unsubscribing, the order on a reconnect, and `close`.

**Notes:** `notes.md` has the line counts at each step, the trade-offs, and a section on the hook points left for T087.

## Key trade-offs

- **One package for both processes.** The renderer gets its HostPort from `ShellAPI.host`; the main process wraps its socket in the same interface and calls `handshake()` then `client.attach(port, handshake, rest)`.
- **Which handshakes check `host_proof` depends on the authentication type, not on `weak_auth`.** A build with only `test-harness` has `weak_auth` false but still uses `harness` authentication.
- **Commands are marked by the caller.** The generated protocol package has no method catalog, so the caller uses `command()`. An existing `command_id` is kept; otherwise one is generated.
- **Hook points for T087** (written into `notes.md`):
  - All `stream/*` notifications arrive in order through `onNotification`.
  - `stream/subscribe` must use `client.call(method, params, done)`. `done` runs synchronously, ahead of the later notifications in the same chunk; with a `request()` Promise, the events that follow would already have been dispatched.
  - `reconnecting`, `failed` and `closed` from `onState` mean "stop and keep the cursor"; `ready` carries the new handshake, including `data_epoch`, and is the moment to resubscribe.
  - The three counts live in T087's subscription layer.
- **Removed `notify` and `sendBlob` from the client.** They had no users and no tests, so they would have been unused code. T080 and T145 can add them back when they need them.

## Verification

- `cargo xtask ci pr --record docs/tasks/T071-host-client-conn/ci/` in a clean worktree, at `4651c8d`, all pass:
  - rust 947
  - protocol 176
  - ts 272
  - license 1
  - The summary is committed (`60fb698`).
- Against the acceptance criteria:
  - [x] **UNIT-hc-conn** (`frame.test.ts`, `handshake.test.ts`, `client.test.ts`, 30 tests) covers:
    - the state machine;
    - framing across arbitrary splits, `0x02` and the 64,000-byte limit;
    - request matching in any order, error responses, and responses nobody waits for;
    - acks equal to the bytes received;
    - the same `command_id` resent after a reconnect, through `connectShell` and the mock shell host, with the request in flight failing;
    - requests failing at once and commands queueing while there is no connection;
    - `close` ending everything;
    - a broken frame;
    - handshake failures: 10 kinds of refusal, 6 protocol violations, a weak host refused, and an abort.
  - [x] **UNIT-hostport-contract-mock**: all 8 contract tests pass on the mock.
  - [x] Test titles start with the IDs verbatim; the CI summary matches both UNIT-hc-conn and UNIT-hostport-contract-mock as `pass`.
  - [x] CI passes in a clean worktree.
- Mutation checks, run and then reverted:
  - letting `initialize` through in the mock port, or pushing `reconnecting` ahead of the old data, fails the matching contract tests;
  - dropping commands on disconnect, or generating a new `command_id` on resend, fails the resend tests.
- No new packages from the npm registry. The lockfile only adds the new package's workspace links and versions already in it: zod 4.6.7, vite 8.3.3, vitest 5.0.3, @types/node 24.19.1.
- The diff has no personal information, and no commit message contains a claude.ai link.

## Size

- 1,858 lines: 1,278 code and 580 tests (the tests include the 155-line contract suite).
- That is about 6% over the ~1,750 limit and under the 1,900 hard stop from the first ruling.
- Most of the overrun is `client.ts`: about 200 lines estimated, 309 actual. The extra comes from two ordering rules: chunks are handled one at a time in arrival order and acked only once handled, and `call` callbacks run synchronously so responses keep their place among notifications. Prettier also wraps imports and object literals over many lines.
- Before writing the `client.ts` tests the count was already 1,748. What remained was only the connection tests UNIT-hc-conn requires, so I finished them and report the overrun here rather than stopping a third time.

## Commits

```
60fb698 docs(task): T071 pr CI summary
4651c8d docs(task): T071 notes: line counts, trade-offs and the hook points for T087
3677194 feat(host-client): add the connection object and connectShell
a4e7ff9 test(host-client): add the browser mock HostPort and the HostPort contract
6ba4622 test(host-client): tighten the handshake failure tables
0cf34f3 docs(task): T071 line counts so far and the second overrun warning
3ad2cb3 feat(host-client): add the handshake
08eb25b test(host-client): add the frame-level mock host
4c83658 feat(host-client): add the package and the frame codec
2345741 feat(shell-api): add the protocol members: host, HostPort, privileged actions
34bcce0 docs(task): T071 design draft and line estimate before starting
```

## Open issues and findings outside the scope

- **T076:**
  - Run `hostPortContract` against the windowBridge; the setup must supply `shell`, `host` (a `MockHost`) and `settle`.
  - The contract's request test uses `host/status` (scope `read`), so it does not depend on whether `ping`, scope `none`, is on the whitelist.
  - Its handshake comes from host-client's `handshake()`; the result must leave out `host_proof`.
  - Status changes and data have to travel on one ordered channel.
- **T075:**
  - The handshake has no built-in timeout; pass `AbortSignal.timeout(...)`, and abort the same way when the socket closes.
  - `MockConnection.port()` can be used to test `controlClient`.
- **Browser screenshot tests (T08x):** `MockHost` takes the fixtures as a constructor argument; they should import `methods.json` through Vite and pass it in.
- **T080, T145:** `HostClient` has no `notify` (e.g. for `client/presence`) or `sendBlob` yet; add them to `client.ts` when needed.
- **Main has moved since this branch was cut.** T039 was merged, but only `cupper-protocol/src/proof.rs` changed and the TS side is unaffected. I did not merge or rebase.
