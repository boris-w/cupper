// Q8 (Codex), T003's q8.ts redone for Codex (design 5.9.5): two sessions on ONE
// codex-acp process, each sending 3 prompts back to back (echo a token; `sleep 3;
// echo <token>`; create a file, approved once in read-only mode), all within 5
// minutes, no output crossing over; 3 rounds. `--hold`: one session's approval
// stays open 15 s while the other works. Process samples while `sleep` runs
// answer SP-4b leftover item 7. How to run: notes.md.
//   node q8.ts --base <tmp> --codex <native codex> [--adapter 2.2.2] [--rounds 3] [--hold yes] [--tag x]

import { randomBytes } from "node:crypto";
import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { AcpConn, CLIENT_CAPABILITIES, DEFAULT_POLICY } from "../SP-1-adapter-behavior/acp-client.ts";
import type { Frame, Json } from "../SP-1-adapter-behavior/acp-client.ts";
import { arg, boot, ConfigTracker, makeRepo } from "../T003-tech-validation/common.ts";
import { assertSafe, codexSpec, header, MODEL, newSession, sampleTree } from "./codex.ts";

const [base, version, rounds] = [arg("base"), arg("adapter", "2.2.2"), Number(arg("rounds", "3"))];
const name = `q8-codex-${version}${arg("tag", "") ? `-${arg("tag")}` : ""}`;
const BUDGET_MS = 300_000;
const [repo, rawDir] = [join(base, "repos", name), join(base, "raw", "codex")];
mkdirSync(rawDir, { recursive: true });
makeRepo(repo);
const plan = { rounds, promptsPerSession: 3, sessions: 2, model: MODEL, mode: "read-only", permissionAnswers: ["allow_once"] };
const c = new AcpConn(codexSpec(base, version, arg("codex"), name), join(rawDir, `${name}.jsonl`), join(rawDir, `${name}.stderr.log`), header(base, version, arg("codex"), name, CLIENT_CAPABILITIES, plan));
const tr = new ConfigTracker(c);
const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

// Bookkeeping: which session is which, which tokens belong to whom.
type Rec = { session: string; idx: number; kind: string; token: string; sentAt: number; doneAt?: number; text: string; permissions: Json[]; reconnects: number } & Json;
const [alias, tokensOf, inFlight] = [new Map<string, string>(), new Map<string, Set<string>>(), new Map<string, Rec>()];
const [crossTalk, outOfTurn]: Json[][] = [[], []];
const counts = { interleavings: 0, messageChunks: 0, messageChunksWithMeta: 0 };
let lastSid = "";
const CONTENT = new Set(["agent_message_chunk", "agent_thought_chunk", "tool_call", "tool_call_update", "plan"]);

c.onFrame((f: Frame) => {
  const m = f.msg;
  const sid = m?.params?.sessionId;
  if (f.dir !== "recv" || !sid || !alias.has(sid)) return;
  const me = alias.get(sid)!;
  const body = JSON.stringify(m);
  for (const [other, toks] of tokensOf) {
    if (other !== me) for (const t of toks) if (body.includes(t)) crossTalk.push({ t: f.t, seq: f.seq, method: m.method, update: m.params?.update?.sessionUpdate, session: me, foreignSession: other });
  }
  const u = m.method === "session/update" ? m.params.update : null;
  const rec = inFlight.get(sid);
  if (CONTENT.has(u?.sessionUpdate)) {
    if (lastSid && lastSid !== sid) counts.interleavings++;
    lastSid = sid;
    if (!rec) outOfTurn.push({ t: f.t, session: me, update: u.sessionUpdate, status: u.status });
  }
  if (u?.sessionUpdate === "agent_message_chunk") {
    counts.messageChunks++;
    if (u._meta) counts.messageChunksWithMeta++;
    if (rec && u.content?.type === "text") rec.text += u.content.text;
  }
  if (rec && u?.sessionUpdate === "session_info_update" && u._meta?.codex?.error) rec.reconnects++;
});

// allow_once only; a session listed in `holdNext` gets its next request held open.
const holdNext = new Set<string>();
c.policy = {
  ...DEFAULT_POLICY,
  permission: (p: Json) => {
    const rec = inFlight.get(p?.sessionId);
    const raw = JSON.stringify(p?.toolCall ?? {}); // Codex's edit approval names the file, without its content
    rec?.permissions.push({ t: c.now(), sessionMatches: !!rec, own: !!rec && (raw.includes(rec.token) || (!!rec.file && raw.includes(rec.file))), toolKind: p?.toolCall?.kind, options: (p?.options ?? []).map((o: Json) => `${o.optionId}:${o.kind}`) });
    return holdNext.delete(p?.sessionId) ? { hold: true } : "allow_once";
  },
};

const PROMPTS: Record<string, (t: string, file: string) => string> = {
  text: (t) => `Reply with exactly this text and nothing else: ${t}`,
  bash: (t) => `Use your shell tool to run exactly this command: sleep 3; echo ${t}\nThen reply with only the command's output.`,
  write: (t, file) => `Create a file named ${file} in the current directory whose entire content is the single line ${t}\nUse your apply_patch tool, no other tool. Then reply with exactly: done ${t}`,
};

async function prompt(sid: string, idx: number, kind: string, file = ""): Promise<Rec> {
  const who = alias.get(sid)!;
  const tok = `q8${who.replace("-", "")}p${idx}x${randomBytes(3).toString("hex")}`;
  tokensOf.get(who)!.add(tok);
  assertSafe(c, tr, sid);
  const rec: Rec = { session: who, idx, kind, token: tok, file, sentAt: c.now(), text: "", permissions: [], reconnects: 0 };
  inFlight.set(sid, rec);
  c.note("q8-prompt", { session: who, idx, kind });
  const r = await c.request("session/prompt", { sessionId: sid, prompt: [{ type: "text", text: PROMPTS[kind](tok, file) }] }, BUDGET_MS);
  inFlight.delete(sid);
  Object.assign(rec, { doneAt: c.now(), elapsedMs: r.elapsedMs, ok: r.ok, stopReason: r.result?.stopReason, timedOut: r.timedOut ?? false, error: r.error, tokenEchoed: rec.text.includes(tok) });
  c.note("q8-prompt-result", { session: who, idx, ok: rec.ok, stopReason: rec.stopReason, elapsedMs: rec.elapsedMs, tokenEchoed: rec.tokenEchoed, reconnects: rec.reconnects });
  return rec;
}

const register = (sid: string, who: string) => tokensOf.set(who, new Set()) && alias.set(sid, who);

// Process sampling (SP-4b item 7): keep the samples that show a running `sleep`.
const [samples, privSamples]: Json[][] = [[], []];
let sampling = false;
function take(label: string, force = false) {
  const s = sampleTree(c.child.pid!, label, c.now(), privSamples);
  if (force || s.procs.some((p: Json) => p.role === "sleep")) samples.push(s);
  else privSamples.pop();
}
async function sampler(label: string) {
  for (sampling = true; sampling && !c.exited; await sleep(500)) {
    try { take(label); } catch {} // a process can exit between ps and getsid
  }
}

const close = async (sid: string) => c.note("session-close", { session: alias.get(sid), ok: (await c.request("session/close", { sessionId: sid }, 60_000)).ok });
const fileHas = (file: string, tok: string) => existsSync(join(repo, file)) && readFileSync(join(repo, file), "utf8").trim() === tok;
const worker = async (r: number, l: string, sid: string) => [await prompt(sid, 1, "text"), await prompt(sid, 2, "bash"), await prompt(sid, 3, "write", `${l}-r${r}.txt`)];

async function round(r: number) {
  c.note("q8-round", { round: r });
  const before = { cross: crossTalk.length, out: outOfTurn.length, inter: counts.interleavings };
  const tNew = c.now();
  const [sa, sb] = await Promise.all([newSession(c, tr, repo), newSession(c, tr, repo)]);
  const newMs = c.now() - tNew;
  register(sa, `r${r}-A`);
  register(sb, `r${r}-B`);
  take(`round ${r}: two sessions open, idle`, true);
  void sampler(`round ${r}: tool command running`);
  const t0 = c.now();
  const [ra, rb] = await Promise.all([worker(r, "a", sa), worker(r, "b", sb)]);
  const wallMs = c.now() - t0;
  sampling = false;
  const all = [...ra, ...rb];
  let overlapMs = 0;
  for (const x of ra) for (const y of rb) overlapMs += Math.max(0, Math.min(x.doneAt!, y.doneAt!) - Math.max(x.sentAt, y.sentAt));
  const files = { A: fileHas(`a-r${r}.txt`, ra[2].token), B: fileHas(`b-r${r}.txt`, rb[2].token) };
  await sleep(3000); // late updates arrive before the sessions close
  await Promise.all([close(sa), close(sb)]);
  const res = {
    round: r,
    newSessionsConcurrentMs: newMs,
    wallMs,
    within5min: wallMs <= BUDGET_MS && all.every((p) => !p.timedOut),
    allReturned: all.every((p) => p.ok && p.stopReason === "end_turn"),
    tokensEchoed: all.filter((p) => p.tokenEchoed).length,
    permissions: all.reduce((n, p) => n + p.permissions.length, 0),
    permissionsRoutedToOwnSession: all.every((p) => p.permissions.every((q: Json) => q.sessionMatches && q.own)),
    reconnects: all.reduce((n, p) => n + p.reconnects, 0),
    filesWritten: files,
    overlapMs,
    interleavedUpdateSwitches: counts.interleavings - before.inter,
    crossTalk: crossTalk.slice(before.cross),
    outOfTurnContentUpdates: outOfTurn.slice(before.out),
    prompts: all.map(({ text, token, ...rest }) => ({ ...rest, replyChars: text.length })),
  };
  const pass = res.within5min && res.allReturned && !res.crossTalk.length && res.permissionsRoutedToOwnSession && files.A && files.B;
  return { ...res, verdict: pass ? (res.tokensEchoed === 6 ? "pass" : "pass (some replies did not echo the token)") : "FAIL" };
}

/** A's file-edit approval stays open >= 15 s while B prompts, creates, lists and closes sessions. */
async function hold() {
  c.note("q8-hold", {});
  const [ha, hb] = await Promise.all([newSession(c, tr, repo), newSession(c, tr, repo)]);
  register(ha, "hold-A");
  register(hb, "hold-B");
  holdNext.add(ha);
  const pA = prompt(ha, 1, "write", "held.txt");
  for (const end = c.now() + 180_000; !c.held.length && c.now() < end; ) await sleep(200);
  const held = c.held.shift();
  if (!held) throw new Error("session A never asked for permission");
  const heldAt = c.now();
  const pB = await prompt(hb, 1, "text");
  const extra = await c.request("session/new", { cwd: repo, mcpServers: [] }, 120_000);
  const list = await c.request("session/list", { cwd: repo }, 60_000);
  if (extra.ok) register(extra.result.sessionId, "hold-C");
  const closeOk = extra.ok && (await c.request("session/close", { sessionId: extra.result.sessionId }, 60_000)).ok;
  const stillHeld = inFlight.has(ha);
  await sleep(Math.max(0, heldAt + 15_000 - c.now()));
  const heldMs = c.now() - heldAt;
  c.answerPermission(held.id, held.params, "allow_once");
  const rA = await pA;
  await sleep(3000);
  await Promise.all([close(ha), close(hb)]);
  const brief = (p: Rec) => ({ ok: p.ok, stopReason: p.stopReason, elapsedMs: p.elapsedMs, tokenEchoed: p.tokenEchoed, reconnects: p.reconnects, permissions: p.permissions.length });
  return {
    ...{ heldMs, sessionB: brief(pB), sessionAStillWaitingWhenBFinished: stillHeld, sessionCloseWhileHeld: { ok: closeOk } },
    ...{ sessionNewWhileHeld: { ok: extra.ok, elapsedMs: extra.elapsedMs }, sessionListWhileHeld: { ok: list.ok, elapsedMs: list.elapsedMs } },
    ...{ sessionA: { ...brief(rA), fileWritten: fileHas("held.txt", rA.token) }, crossTalk: crossTalk.filter((x) => x.session.startsWith("hold")) },
  };
}

const results: Json = { name, date: new Date().toISOString().slice(0, 10), rounds: [], hold: null };
try {
  await boot(c);
  take("after initialize", true);
  for (let r = 1; r <= rounds; r++) {
    results.rounds.push(await round(r));
    const x = results.rounds.at(-1);
    console.log(`round ${r}: ${x.verdict} wall=${x.wallMs}ms overlap=${x.overlapMs}ms cross=${x.crossTalk.length} echoed=${x.tokensEchoed}/6 reconnects=${x.reconnects}`);
  }
  if (arg("hold", "yes") === "yes") {
    results.hold = await hold();
    console.log(`hold: B ok=${results.hold.sessionB.ok} in ${results.hold.sessionB.elapsedMs}ms while A held ${results.hold.heldMs}ms; A ok=${results.hold.sessionA.ok}`);
  }
} catch (e) {
  results.error = (e as Error).message;
  console.log(`FAILED: ${(e as Error).message}`);
} finally {
  sampling = false;
  Object.assign(results, { outOfTurnTotal: outOfTurn.length, crossTalkTotal: crossTalk.length, ...counts, procSamples: samples });
  await c.stop();
  c.close();
  writeFileSync(join(rawDir, `${name}.results.json`), JSON.stringify(results, null, 1));
  mkdirSync(join(base, "private"), { recursive: true });
  writeFileSync(join(base, "private", `${name}.proc-labels.json`), JSON.stringify(privSamples, null, 1));
}
process.exit(0);
