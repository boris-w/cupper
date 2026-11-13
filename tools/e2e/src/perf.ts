// The measuring helpers the bridge gate (T078, T220) and SP-6 (T103) share:
// the test's own `test_harness` connection to the host, a blob uploaded on
// it, the app's window covered, and the page's animation frames counted.

import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import { join } from "node:path";

import { expect, type Cupper } from "./fixture";
import { dial, initialize, keyProof, type Peer } from "./raw";

/** The test's own connection to the host, as `test_harness`, and a way to call on it. */
export async function harness(cupper: Cupper) {
  const peer = await dial(cupper.dataDir);
  const key = Buffer.from(readFileSync(join(cupper.root, "harness_key"), "utf8").trim(), "hex");
  const init = await initialize(peer, keyProof("harness", key, peer.nonce));
  expect(init.error).toBeUndefined();
  let next = 10;
  const call = async (method: string, params: unknown) => {
    const { result, error } = await peer.request(next++, method, params);
    expect(error, method).toBeUndefined();
    return result;
  };
  return { peer, call };
}

/** Uploads `bytes` as a blob (design 5.3.1): its chunks, then `blob/put`; its hash. */
export async function putBlob(
  peer: Peer,
  call: (m: string, p: unknown) => Promise<unknown>,
  bytes: Buffer,
) {
  const hash = createHash("sha256").update(bytes).digest("hex");
  const most = 64_000 - 5 - 41;
  for (let offset = 0; offset < bytes.length; offset += most) {
    const head = Buffer.alloc(41);
    Buffer.from(hash, "hex").copy(head);
    head.writeBigUInt64BE(BigInt(offset), 32);
    head[40] = offset + most >= bytes.length ? 1 : 0;
    peer.send(Buffer.concat([head, bytes.subarray(offset, offset + most)]), 0x02);
  }
  await call("blob/put", { hash, size: bytes.length, media_type: "application/octet-stream" });
  return hash;
}

/**
 * Covers the app's window with another one, and stops its animation frames.
 * Under Playwright, Chromium keeps the page visible and its frames running
 * even covered, minimized or hidden (T078), so the frames are stopped by
 * hand: what a hidden page loses that the store cares about (design 5.7.1).
 */
export async function cover(cupper: Cupper) {
  const bounds = await (
    await cupper.app.browserWindow(cupper.window)
  ).evaluate((w) => w.getBounds());
  await cupper.app.evaluate(({ BrowserWindow }, bounds) => {
    // Not shown on creation, which would activate the app (T195).
    const cover = new BrowserWindow({
      ...bounds,
      show: false,
      frame: false,
      alwaysOnTop: true,
      focusable: false,
    });
    cover.showInactive();
  }, bounds);
  await cupper.window.evaluate(() => {
    window.requestAnimationFrame = () => 0;
  });
}

/** The page's animation frames in one second. */
export const frameRate = (cupper: Cupper) =>
  cupper.window.evaluate(
    () =>
      new Promise<number>((done) => {
        let frames = 0;
        const tick = () => void (frames++, requestAnimationFrame(tick));
        requestAnimationFrame(tick);
        setTimeout(() => done(frames), 1000);
      }),
  );
