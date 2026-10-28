// Q31: is `agentMessage.phase` passed in `_meta`? codex-acp 2.2.2 adds it only for
// clients declaring the JetBrains AIR extension (`_meta.jetbrains.air`), so a plain
// and an AIR client each run a prompt with commentary, a command and an answer;
// the AIR session is then replayed with `session/load`. Q25 (initial check): the
// mode values (with their AIR `kind`), and whether answering the plan review with
// `revise_plan` (kind reject_once) ends the turn. How to run: notes.md.
//   node extra.ts --base <tmp> --codex <native codex> [--adapter 2.2.2]

import { randomBytes } from "node:crypto";
import { mkdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { AcpConn, CLIENT_CAPABILITIES, CLIENT_INFO, DEFAULT_POLICY } from "../SP-1-adapter-behavior/acp-client.ts";
import type { Frame, Json } from "../SP-1-adapter-behavior/acp-client.ts";
import { arg, ConfigTracker, makeRepo } from "../T003-tech-validation/common.ts";
import { assertSafe, codexSpec, header, newSession } from "./codex.ts";

const [base, version, codexBin] = [arg("base"), arg("adapter", "2.2.2"), arg("codex")];
const rawDir = join(base, "raw", "codex");
const repo = join(base, "repos", "extra");
mkdirSync(rawDir, { recursive: true });
makeRepo(repo);
const AIR = { ...CLIENT_CAPABILITIES, _meta: { ...CLIENT_CAPABILITIES._meta, jetbrains: { air: { version: 1, capabilities: [] } } } };
const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

/** Per messageId: chunk count, the values of `_meta.jetbrains.air.phase`, and the `_meta` keys seen. */
function phases(frames: Frame[], sid: string, from: number) {
  const out: Record<string, { chunks: number; phases: Json[]; metaKeys: string[] }> = {};
  for (const f of frames) {
    const u = f.msg?.params?.update;
    if (f.dir !== "recv" || f.seq <= from || f.msg?.params?.sessionId !== sid || u?.sessionUpdate !== "agent_message_chunk") continue;
    const e = (out[u.messageId ?? "(none)"] ??= { chunks: 0, phases: [], metaKeys: [] });
    e.chunks++;
    const ph = u._meta?.jetbrains?.air?.phase ?? null;
    if (!e.phases.includes(ph)) e.phases.push(ph);
    for (const k of Object.keys(u._meta ?? {})) if (!e.metaKeys.includes(k)) e.metaKeys.push(k);
  }
  return out;
}

async function run(tag: "plain" | "air") {
  const name = `extra-codex-${version}-${tag}`;
  const caps = tag === "air" ? AIR : CLIENT_CAPABILITIES;
  const c = new AcpConn(codexSpec(base, version, codexBin, name), join(rawDir, `${name}.jsonl`), join(rawDir, `${name}.stderr.log`), header(base, version, codexBin, name, caps));
  const tr = new ConfigTracker(c);
  const seq = () => c.frames.at(-1)?.seq ?? 0;
  const out: Json = { tag };
  const reviews: Json[] = [];
  // The plan review is answered with revise_plan (reject_once); anything else allow_once.
  c.policy = {
    ...DEFAULT_POLICY,
    permission: (p: Json) => {
      if (p?.toolCall?.kind !== "switch_mode") return "allow_once";
      reviews.push({ t: c.now(), title: p.toolCall.title, options: p.options.map((o: Json) => `${o.optionId}:${o.kind}`) });
      return "reject_once";
    },
  };
  try {
    c.start();
    const init = await c.request("initialize", { protocolVersion: 1, clientCapabilities: caps, clientInfo: CLIENT_INFO }, 60_000);
    if (!init.ok) throw new Error(`initialize failed: ${JSON.stringify(init.error)}`);
    const sid = await newSession(c, tr, repo);
    out.modes = tr.option(sid, "mode")?.options?.map((o: Json) => ({ value: o.value, name: o.name, airKind: o._meta?.jetbrains?.air?.kind ?? null }));
    const tok = `q31${tag}x${randomBytes(3).toString("hex")}`;
    assertSafe(c, tr, sid);
    const from = seq();
    const text = `First write one short sentence saying what you are about to do. Then use your shell tool to run exactly: sleep 1; echo ${tok}\nThen reply with only the command's output.`;
    const r = await c.request("session/prompt", { sessionId: sid, prompt: [{ type: "text", text }] }, 300_000);
    await sleep(2000);
    out.prompt = { ok: r.ok, stopReason: r.result?.stopReason, elapsedMs: r.elapsedMs, messages: phases(c.frames, sid, from) };
    if (tag === "air") {
      await c.request("session/close", { sessionId: sid }, 60_000);
      const at = seq();
      const l = await c.request("session/load", { sessionId: sid, cwd: repo, mcpServers: [] }, 120_000);
      await sleep(2000);
      out.replay = { ok: l.ok, messages: phases(c.frames, sid, at) };
      return out;
    }
    // Q25: plan collaboration mode in read-only; the review is answered with revise_plan.
    const ps = await newSession(c, tr, repo, "read-only", "plan");
    assertSafe(c, tr, ps, "plan");
    const ask = "Propose a short two-step plan for adding a file hello.txt that contains the word hi. Do not make any changes.";
    const pr = await c.request("session/prompt", { sessionId: ps, prompt: [{ type: "text", text: ask }] }, 300_000);
    const [doneAt, end] = [c.now(), seq()];
    await sleep(5000);
    out.planReview = {
      reviews,
      prompt: { ok: pr.ok, stopReason: pr.result?.stopReason, elapsedMs: pr.elapsedMs, msFromReviewToResponse: reviews.length ? doneAt - reviews.at(-1).t : null },
      updatesWithin5sAfterResponse: c.frames.filter((f) => f.seq > end && f.dir === "recv" && f.msg?.params?.sessionId === ps).map((f) => f.msg.params.update?.sessionUpdate ?? f.msg.method),
      collaborationModeAfter: tr.option(ps, "collaboration_mode")?.currentValue,
      modeAfter: tr.mode(ps),
    };
    return out;
  } catch (e) {
    return { ...out, error: (e as Error).message };
  } finally {
    await c.stop();
    c.close();
  }
}

const runs = [await run("plain"), await run("air")];
writeFileSync(join(rawDir, `extra-codex-${version}.results.json`), JSON.stringify({ name: `extra-codex-${version}`, date: new Date().toISOString().slice(0, 10), runs }, null, 1));
for (const r of runs) console.log(JSON.stringify(r).slice(0, 800));
process.exit(0);
