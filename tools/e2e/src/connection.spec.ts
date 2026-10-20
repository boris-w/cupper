// Security at the connection (design 5.8.1, 5.9.4): a fake host that holds
// the socket first gets no command and no secret from the app or the CLI; a
// client without the key gets at most the read-only `cli`, and one that
// offers a key without its proof is cut off.

import { randomBytes } from "node:crypto";
import { readFileSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";

import type { Cupper } from "./fixture";
import { expect, test } from "./fixture";
import { offMachine, sampleNetwork, texts, type Flow } from "./helpers";
import { signal, tree } from "./procs";
import { dial, FakeHost, initialize, keyProof, socketPath, type Message } from "./raw";

const harnessKey = (cupper: Cupper) =>
  readFileSync(join(cupper.root, "harness_key"), "utf8").trim();

/** What clients sent the fake host: `initialize` alone, its auth a proof, no key in it. */
function onlyProofs(fake: FakeHost, cupper: Cupper, type: string) {
  const sent = fake.received();
  expect(sent.length).toBeGreaterThan(0);
  for (const { method, params } of sent) {
    expect(method).toBe("initialize");
    const { auth } = params as { auth: Message };
    expect(Object.keys(auth).sort()).toEqual(["c_nonce", "proof", "type"]);
    expect(auth.type).toBe(type);
  }
  expect(JSON.stringify(sent)).not.toContain(harnessKey(cupper));
}

test(
  "E2E-SEC-fake-host: the app refuses a fake host that took over the socket, and sends it no command",
  { tag: "@smoke" },
  async ({ cupper }) => {
    await cupper.launch();
    await cupper.connected();
    // The fake takes the socket over while the host runs; then the host dies.
    const host = cupper.host();
    expect(host).toBeDefined();
    cupper.track(tree(host?.pid ?? 0));
    rmSync(socketPath(cupper.dataDir));
    const fake = await new FakeHost().listen(cupper.dataDir);
    try {
      signal(host ? [host] : [], "SIGKILL");
      // The fake cannot prove the key: the app offers to restart the host.
      const dialog = await cupper.dialogs.next();
      expect(dialog.message).toBe((await texts(cupper))("host:restart_title"));
      await cupper.dialogs.answer(dialog.index, 1);
      onlyProofs(fake, cupper, "dev_key");
    } finally {
      await fake.close();
    }
  },
);

test(
  "E2E-SEC-fake-host: a fake host on the socket before the app starts gets no command either",
  { tag: "@smoke" },
  async ({ cupper }) => {
    const fake = await new FakeHost().listen(cupper.dataDir);
    try {
      // The dialog comes while the app starts; the stand-in records it (T178).
      await cupper.launch();
      const dialog = await cupper.dialogs.next();
      expect(dialog.message).toBe((await texts(cupper))("host:restart_title"));
      await cupper.dialogs.answer(dialog.index, 1);
      onlyProofs(fake, cupper, "dev_key");
    } finally {
      await fake.close();
    }
  },
);

test(
  "E2E-SEC-cli-fake-host: the CLI refuses a fake host with exit code 4",
  { tag: "@smoke" },
  async ({ cupper }) => {
    const fake = await new FakeHost().listen(cupper.dataDir);
    try {
      const { code, stderr } = await cupper.cli.run(["status"]);
      expect(code, stderr).toBe(4);
      onlyProofs(fake, cupper, "harness");
    } finally {
      await fake.close();
    }
  },
);

test(
  "E2E-SEC-no-key-client: without the key at most the read-only cli; a key without its proof is cut off",
  { tag: "@smoke" },
  async ({ cupper }) => {
    await cupper.launch();
    await cupper.connected();
    const error = (response: Message) => (response.error as { data?: Message }).data;

    // `peer`: the read-only `cli`, and a write is refused.
    const peer = await dial(cupper.dataDir);
    const { result } = await initialize(peer, { type: "peer" });
    expect((result as Message).principal).toMatchObject({ kind: "cli", scopes: ["read"] });
    const command_id = "0192f5e0-0000-7000-8000-000000000001";
    const send = { command_id, session_id: "s_1", mode: "queue", content: [] };
    const write = await peer.request(2, "session/send", send);
    expect(error(write)).toMatchObject({ code: "forbidden", reason: "scope" });
    peer.socket.destroy();

    // Neither the window's role nor a signature it lacks makes it the desktop.
    for (const [auth, more, refusal] of [
      [{ type: "peer" }, { role: "window" }, { code: "forbidden", reason: "role" }],
      [{ type: "codesign" }, {}, { code: "unauthenticated", reason: "bad_key" }],
    ] as const) {
      const client = await dial(cupper.dataDir);
      expect(error(await initialize(client, auth, more))).toMatchObject(refusal);
      client.socket.destroy();
    }

    // A key with a wrong or no proof: `unauthenticated`, and the host hangs up.
    // `dev_key` is refused before its proof from anyone but the app (`bad_key`).
    const wrong = randomBytes(32);
    for (const [auth, reason] of [
      [(nonce: string) => keyProof("dev_key", wrong, nonce), "bad_key"],
      [() => ({ type: "dev_key" }), "bad_proof"],
      [(nonce: string) => keyProof("harness", wrong, nonce), "bad_proof"],
      [() => ({ type: "harness", c_nonce: "00" }), "bad_proof"],
    ] as const) {
      const client = await dial(cupper.dataDir);
      const refused = error(await initialize(client, auth(client.nonce)));
      expect(refused).toMatchObject({ code: "unauthenticated", reason });
      await client.closed;
    }
  },
);

// The host installs the adapter's npm tree at start, one of the two egresses
// SEC-6 allows; a registry on a closed loopback port keeps it on this machine.
test.describe(() => {
  test.use({ testConfig: { runtime: { registry_url: "http://127.0.0.1:9/" } } });

  test("E2E-SEC-egress: no connection leaves the machine while a session runs; nettop sees deep into the tree", async ({
    cupper,
  }) => {
    await cupper.launch();
    await cupper.connected();
    // The positive control (Q27): a child of the fake agent holds a UDP socket
    // connected to a documentation address (RFC 5737); connecting sends nothing.
    const control = join(cupper.root, "control.sh");
    writeFileSync(control, "exec /usr/bin/nc -u -w 20 192.0.2.1 9 < /dev/null\n");
    const script = `spawn /bin/sh ${control}; flood 200 100; sleep 3000; say done`;
    await cupper.cli.ok(["new", "--agent", "claude", "--project", cupper.repo, script]);
    const flows: Flow[] = [];
    const took: number[] = [];
    let caught: number | undefined;
    for (let n = 0; n < 15; n++) {
      const started = performance.now();
      const sample = sampleNetwork(cupper);
      took.push(Math.round(performance.now() - started));
      flows.push(...sample);
      caught ??= sample.some((flow) => flow.remote.startsWith("192.0.2.1")) ? n : undefined;
      await new Promise((done) => setTimeout(done, 200));
    }
    const pids = new Set(flows.map((flow) => flow.pid)).size;
    const tree = cupper.processes().length;
    console.log(JSON.stringify({ q27: { took, tree, pids, caught } }));
    expect(caught, "nettop sees the control's flow").toBeDefined();
    const off = offMachine(flows).filter((flow) => !flow.remote.startsWith("192.0.2.1"));
    expect(off).toEqual([]);
  });
});
