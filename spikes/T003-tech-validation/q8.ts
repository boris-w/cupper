// Q8 (Claude): two sessions on ONE claude-agent-acp process prompting at the
// same time (design 5.9.5): each session sends 3 prompts back to back, both
// sessions start together, all 6 must return within 5 minutes with no output
// crossing over to the other session; 3 rounds. A supplementary "hold" step
// keeps one session's permission request open while the other session works
// (upstream #830). Process-tree samples taken while commands run answer SP-4b
// leftover item 7; `--killgroup` then checks what killing the adapter's
// process group leaves running.
//
//   node q8.ts --base <tmp> --adapter 0.91.0 [--rounds 3] [--hold yes|no] [--killgroup yes|no] [--tag <name>]
//
// <base>/adapters/claude-<version> must hold an npm install of the adapter.
// Raw (unsanitized) frames go to <base>/raw/claude/; results JSON next to them.

import { randomBytes } from "node:crypto";
import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { AcpConn, CLIENT_CAPABILITIES, DEFAULT_POLICY } from "../SP-1-adapter-behavior/acp-client.ts";
import type { Frame, HeldPermission, Json } from "../SP-1-adapter-behavior/acp-client.ts";
import { adapterSpec, arg, assertSafe, boot, ConfigTracker, makeRepo, newSession, procTree, versionsOf } from "./common.ts";
import type { Proc } from "./common.ts";

const base = arg("base");
const adapterVersion = arg("adapter");
const rounds = Number(arg("rounds", "3"));
const withHold = arg("hold", "yes") === "yes";
const withKillGroup = arg("killgroup", "no") === "yes";
const tag = arg("tag", "");
const PROMPT_TIMEOUT_MS = 300_000;
const ROUND_BUDGET_MS = 300_000;

const adapterDir = join(base, "adapters", `claude-${adapterVersion}`);
const repo = join(base, "repos", `q8-${adapterVersion}`);
const rawDir = join(base, "raw", "claude");
mkdirSync(rawDir, { recursive: true });
makeRepo(repo);
const name = `q8-claude-${adapterVersion}${tag ? `-${tag}` : ""}`;
const spec = adapterSpec(adapterDir, name);
const versions = versionsOf(adapterDir, spec.env.CLAUDE_CODE_EXECUTABLE);
const c = new AcpConn(spec, join(rawDir, `${name}.jsonl`), join(rawDir, `${name}.stderr.log`), {
  scenario: name,
  versions,
  date: new Date().toISOString().slice(0, 10),
  clientCapabilities: CLIENT_CAPABILITIES,
  plan: { rounds, promptsPerSession: 3, sessions: 2, model: "haiku", mode: "default", permissionAnswers: ["allow_once"] },
});
const tr = new ConfigTracker(c);
const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

// ---------------------------------------------------------------------------
// Bookkeeping: which session is which, which tokens belong to whom

const alias = new Map<string, string>(); // sessionId -> "r1-A"
const tokensOf = new Map<string, Set<string>>(); // alias -> tokens
const inFlight = new Map<string, PromptRec>(); // sessionId -> prompt in flight
const crossTalk: Json[] = [];
const outOfTurn: Json[] = [];
let lastSessionSeen = "";
let interleavings = 0;

interface PromptRec {
  session: string;
  idx: number;
  kind: "text" | "bash" | "write";
  token: string;
  sentAt: number;
  doneAt?: number;
  elapsedMs?: number;
  ok?: boolean;
  stopReason?: string;
  timedOut?: boolean;
  error?: Json;
  text: string;
  tokenEchoed?: boolean;
  permissions: Json[];
  toolCalls: number;
  updates: number;
}

const CONTENT_UPDATES = new Set(["agent_message_chunk", "agent_thought_chunk", "tool_call", "tool_call_update", "plan"]);

c.onFrame((f: Frame) => {
  if (f.dir !== "recv" || !f.msg) return;
  const m = f.msg;
  const sid = m.params?.sessionId;
  if (!sid || !alias.has(sid)) return;
  const me = alias.get(sid)!;
  const body = JSON.stringify(m);
  for (const [other, toks] of tokensOf) {
    if (other === me) continue;
    for (const t of toks) if (body.includes(t)) crossTalk.push({ t: f.t, seq: f.seq, method: m.method, update: m.params?.update?.sessionUpdate, session: me, foreignToken: t, foreignSession: other });
  }
  if (m.method !== "session/update") return;
  const kind = m.params.update?.sessionUpdate;
  const rec = inFlight.get(sid);
  if (CONTENT_UPDATES.has(kind)) {
    if (lastSessionSeen && lastSessionSeen !== sid) interleavings++;
    lastSessionSeen = sid;
  }
  if (!rec) {
    if (CONTENT_UPDATES.has(kind)) outOfTurn.push({ t: f.t, session: me, update: kind });
    return;
  }
  rec.updates++;
  if (kind === "agent_message_chunk" && m.params.update.content?.type === "text") rec.text += m.params.update.content.text;
  if (kind === "tool_call") rec.toolCalls++;
});

// Permission answers: allow_once only (SP-1's client never picks a persistent
// option). A session listed in `holdNext` gets its next request held open.
const holdNext = new Set<string>();
c.policy = {
  ...DEFAULT_POLICY,
  permission: (params: Json) => {
    const sid = params?.sessionId;
    const rec = inFlight.get(sid);
    const raw = JSON.stringify(params?.toolCall ?? {});
    rec?.permissions.push({
      t: c.now(),
      sessionMatches: !!rec,
      carriesOwnToken: rec ? raw.includes(rec.token) : false,
      optionKinds: (params?.options ?? []).map((o: Json) => o.kind),
    });
    if (holdNext.has(sid)) {
      holdNext.delete(sid);
      return { hold: true };
    }
    return "allow_once";
  },
};

function token(label: string) {
  return `q8${label}x${randomBytes(3).toString("hex")}`;
}

function register(sid: string, name: string) {
  alias.set(sid, name);
  tokensOf.set(name, new Set());
}

function promptText(kind: PromptRec["kind"], tok: string, file: string, seconds = 3) {
  switch (kind) {
    case "text":
      return `Reply with exactly this text and nothing else: ${tok}`;
    case "bash":
      return `Use your Bash tool to run exactly this command: sleep ${seconds}; echo ${tok}\nThen reply with only the command's output.`;
    case "write":
      return `Use your Write tool to create a file named ${file} in the current directory whose entire content is the single line ${tok}\nThen reply with exactly: done ${tok}`;
  }
}

async function prompt(sid: string, idx: number, kind: PromptRec["kind"], file = "", seconds = 3): Promise<PromptRec> {
  const name = alias.get(sid)!;
  const tok = token(`${name.replace("-", "")}p${idx}`);
  tokensOf.get(name)!.add(tok);
  assertSafe(c, tr, sid);
  const rec: PromptRec = { session: name, idx, kind, token: tok, sentAt: c.now(), text: "", permissions: [], toolCalls: 0, updates: 0 };
  inFlight.set(sid, rec);
  c.note("q8-prompt", { session: name, idx, kind });
  const r = await c.request("session/prompt", { sessionId: sid, prompt: [{ type: "text", text: promptText(kind, tok, file, seconds) }] }, PROMPT_TIMEOUT_MS);
  inFlight.delete(sid);
  Object.assign(rec, {
    doneAt: c.now(),
    elapsedMs: r.elapsedMs,
    ok: r.ok,
    stopReason: r.result?.stopReason,
    timedOut: r.timedOut ?? false,
    error: r.error,
    tokenEchoed: rec.text.includes(tok),
  });
  c.note("q8-prompt-result", { session: name, idx, ok: rec.ok, stopReason: rec.stopReason, elapsedMs: rec.elapsedMs, tokenEchoed: rec.tokenEchoed });
  return rec;
}

// ---------------------------------------------------------------------------
// Process sampling: keep samples that show a running `sleep` (a tool command)

const procSamples: Json[] = [];
let sampling = false;

function roleOf(p: Proc, root: number): string {
  if (p.pid === root) return "adapter (node)";
  if (/^claude\b/.test(p.label)) return "claude CLI";
  const m = p.label.match(/^(sleep|sh|zsh|bash|git|echo)\b/);
  return m ? m[1] : "other";
}

function takeSample(label: string, force = false) {
  const root = c.child.pid!;
  const tree = procTree(root);
  if (!force && !tree.some((p) => /^sleep\b/.test(p.label))) return;
  const adapter = tree.find((p) => p.pid === root);
  const procs = tree.map((p) => ({
    role: roleOf(p, root),
    pid: p.pid,
    ppid: p.ppid,
    pgid: p.pgid,
    sid: p.sid,
    stat: p.stat,
    rssKB: p.rssKB,
    label: p.label,
    sameSessionAsAdapter: adapter ? p.sid === adapter.sid : null,
    sameGroupAsAdapter: adapter ? p.pgid === adapter.pgid : null,
    isGroupLeader: p.pgid === p.pid,
    isSessionLeader: p.sid === p.pid,
  }));
  const sample = { t: c.now(), label, procs };
  procSamples.push(sample);
  c.note("proc-sample-sid", sample);
}

async function sampler(label: string) {
  sampling = true;
  while (sampling && !c.exited) {
    try {
      takeSample(label);
    } catch {}
    await sleep(1000);
  }
}

// ---------------------------------------------------------------------------
// Rounds

async function closeSession(sid: string) {
  const r = await c.request("session/close", { sessionId: sid }, 60_000);
  c.note("session-close", { session: alias.get(sid), ok: r.ok, elapsedMs: r.elapsedMs });
}

function fileHas(file: string, tok: string): boolean {
  const p = join(repo, file);
  return existsSync(p) && readFileSync(p, "utf8").trim() === tok;
}

async function worker(round: number, letter: string, sid: string): Promise<PromptRec[]> {
  const out: PromptRec[] = [];
  out.push(await prompt(sid, 1, "text"));
  out.push(await prompt(sid, 2, "bash"));
  out.push(await prompt(sid, 3, "write", `${letter.toLowerCase()}-r${round}.txt`));
  return out;
}

function overlapMs(a: PromptRec[], b: PromptRec[]): number {
  let sum = 0;
  for (const x of a) for (const y of b) {
    const lo = Math.max(x.sentAt, y.sentAt);
    const hi = Math.min(x.doneAt ?? x.sentAt, y.doneAt ?? y.sentAt);
    if (hi > lo) sum += hi - lo;
  }
  return sum;
}

async function round(r: number) {
  c.note("q8-round", { round: r });
  const crossBefore = crossTalk.length;
  const outBefore = outOfTurn.length;
  const interBefore = interleavings;
  const tNew = c.now();
  const [sa, sb] = await Promise.all([newSession(c, tr, repo), newSession(c, tr, repo)]);
  const newMs = c.now() - tNew;
  register(sa, `r${r}-A`);
  register(sb, `r${r}-B`);
  takeSample(`round ${r}: two sessions open, idle`, true);
  void sampler(`round ${r}: tool command running`);
  const t0 = c.now();
  const [ra, rb] = await Promise.all([worker(r, "A", sa), worker(r, "B", sb)]);
  const wallMs = c.now() - t0;
  sampling = false;
  const all = [...ra, ...rb];
  const files = {
    A: fileHas(`a-r${r}.txt`, ra[2].token),
    B: fileHas(`b-r${r}.txt`, rb[2].token),
  };
  await sleep(3000); // let late updates arrive before the sessions close
  await Promise.all([closeSession(sa), closeSession(sb)]);
  const allReturned = all.every((p) => p.ok && p.stopReason === "end_turn");
  const result = {
    round: r,
    newSessionsConcurrentMs: newMs,
    wallMs,
    within5min: wallMs <= ROUND_BUDGET_MS && all.every((p) => !p.timedOut),
    allReturned,
    tokensEchoed: all.filter((p) => p.tokenEchoed).length,
    permissions: all.reduce((n, p) => n + p.permissions.length, 0),
    permissionsRoutedToOwnSession: all.every((p) => p.permissions.every((q: Json) => q.sessionMatches && q.carriesOwnToken)),
    filesWritten: files,
    overlapMs: overlapMs(ra, rb),
    interleavedUpdateSwitches: interleavings - interBefore,
    crossTalk: crossTalk.slice(crossBefore),
    outOfTurnContentUpdates: outOfTurn.slice(outBefore),
    prompts: all.map(({ text, token, ...rest }) => ({ ...rest, replyChars: text.length })),
  };
  const pass = result.within5min && result.allReturned && result.crossTalk.length === 0 && result.permissionsRoutedToOwnSession && files.A && files.B;
  const echoAll = result.tokensEchoed === all.length;
  return { ...result, verdict: pass ? (echoAll ? "pass" : "pass (some replies did not echo the token; structure still fine)") : "FAIL" };
}

/**
 * Session A's permission request stays open for >= 15 s while session B
 * prompts, creates, lists and closes sessions. A Write is used because Claude
 * Code runs `sleep N; echo X` without asking (it is classified read-only).
 */
async function hold() {
  c.note("q8-hold", {});
  const [ha, hb] = await Promise.all([newSession(c, tr, repo), newSession(c, tr, repo)]);
  register(ha, "hold-A");
  register(hb, "hold-B");
  holdNext.add(ha);
  const pA = prompt(ha, 1, "write", "held.txt");
  const deadline = c.now() + 120_000;
  while (c.held.length === 0 && c.now() < deadline) await sleep(200);
  const held: HeldPermission | undefined = c.held.shift();
  if (!held) throw new Error("session A never asked for permission");
  const heldAt = c.now();
  const pB = await prompt(hb, 1, "text");
  const t1 = c.now();
  const extra = await c.request("session/new", { cwd: repo, mcpServers: [] }, 120_000);
  const newWhileHeldMs = c.now() - t1;
  const t2 = c.now();
  const list = await c.request("session/list", { cwd: repo }, 60_000);
  const listWhileHeldMs = c.now() - t2;
  let closeOk = false;
  if (extra.ok) {
    register(extra.result.sessionId, "hold-C");
    closeOk = (await c.request("session/close", { sessionId: extra.result.sessionId }, 60_000)).ok;
  }
  const stillHeldWhenBDone = inFlight.has(ha);
  await sleep(Math.max(0, heldAt + 15_000 - c.now()));
  const heldMs = c.now() - heldAt;
  c.answerPermission(held.id, held.params, "allow_once");
  const rA = await pA;
  await sleep(3000);
  await Promise.all([closeSession(ha), closeSession(hb)]);
  return {
    heldMs,
    sessionB: { ok: pB.ok, stopReason: pB.stopReason, elapsedMs: pB.elapsedMs, tokenEchoed: pB.tokenEchoed },
    sessionNewWhileHeld: { ok: extra.ok, elapsedMs: newWhileHeldMs },
    sessionListWhileHeld: { ok: list.ok, elapsedMs: listWhileHeldMs, sessions: list.result?.sessions?.length },
    sessionCloseWhileHeld: { ok: closeOk },
    sessionAStillWaitingWhenBFinished: stillHeldWhenBDone,
    sessionA: { ok: rA.ok, stopReason: rA.stopReason, elapsedMs: rA.elapsedMs, tokenEchoed: rA.tokenEchoed, permissions: rA.permissions.length, fileWritten: fileHas("held.txt", rA.token) },
    crossTalk: crossTalk.filter((x) => x.session.startsWith("hold")),
  };
}

function isAlive(pid: number): boolean {
  try {
    process.kill(pid, 0);
    return true;
  } catch {
    return false;
  }
}

/**
 * What item 7 means for cleanup: the host stops an adapter by killing its
 * process group (design 5.6.8). Starts `sleep 20` through the agent, sends
 * SIGKILL to the adapter's process group, and records which of the adapter's
 * former descendants are still alive a second later. Survivors are then
 * terminated (they are this test's own `sleep` and its shell).
 */
async function killGroup() {
  c.note("q8-killgroup", {});
  const sk = await newSession(c, tr, repo);
  register(sk, "kill-A");
  const p = prompt(sk, 1, "bash", "", 20);
  const root = c.child.pid!;
  const deadline = c.now() + 120_000;
  let tree: Proc[] = [];
  while (c.now() < deadline) {
    tree = procTree(root);
    if (tree.some((q) => /^sleep 20\b/.test(q.label))) break;
    await sleep(300);
  }
  takeSample("killgroup: just before SIGKILL to the adapter's process group", true);
  const adapterPgid = tree.find((q) => q.pid === root)!.pgid;
  process.kill(-adapterPgid, "SIGKILL");
  await c.waitExit(5000);
  await p;
  await sleep(1000);
  const survivors = tree
    .filter((q) => q.pid !== root && isAlive(q.pid))
    .map((q) => ({ role: roleOf(q, root), label: q.label, sameGroupAsAdapter: q.pgid === adapterPgid, sameSessionAsAdapter: q.sid === tree.find((x) => x.pid === root)!.sid }));
  for (const q of tree.filter((x) => x.pid !== root && isAlive(x.pid))) {
    try {
      process.kill(q.pid, "SIGTERM");
    } catch {}
  }
  return { processesBefore: tree.length, sleepSeen: tree.some((q) => /^sleep 20\b/.test(q.label)), survivors };
}

// ---------------------------------------------------------------------------

const results: Json = { name, versions, date: new Date().toISOString().slice(0, 10), rounds: [], hold: null };
try {
  await boot(c);
  takeSample("after initialize", true);
  for (let r = 1; r <= rounds; r++) {
    const res = await round(r);
    results.rounds.push(res);
    console.log(`round ${r}: ${res.verdict} wall=${res.wallMs}ms overlap=${res.overlapMs}ms crossTalk=${res.crossTalk.length} echoed=${res.tokensEchoed}/6`);
  }
  if (withHold) {
    results.hold = await hold();
    console.log(`hold: B ok=${results.hold.sessionB.ok} in ${results.hold.sessionB.elapsedMs}ms while A held ${results.hold.heldMs}ms; A ok=${results.hold.sessionA.ok}`);
  }
  if (withKillGroup) {
    results.killGroup = await killGroup();
    console.log(`killgroup: survivors=${JSON.stringify(results.killGroup.survivors)}`);
  }
} catch (e) {
  results.error = (e as Error).message;
  console.log(`FAILED: ${(e as Error).message}`);
} finally {
  sampling = false;
  results.outOfTurnTotal = outOfTurn.length;
  results.crossTalkTotal = crossTalk.length;
  results.procSamples = procSamples;
  await c.stop();
  c.close();
  writeFileSync(join(rawDir, `${name}.results.json`), JSON.stringify(results, null, 1));
}
process.exit(0);
