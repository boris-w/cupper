// T159: sums up the Chromium traces SP-6's scenarios record with SP6_TRACE=1
// (trace.ts), after the cases, when the app is gone. For each
// `results/*.trace.json`: the main document's renderer, and each stretch
// between the marks `sp6:<label>:start` and `sp6:<label>:end` it set
// (`console.timeStamp`); per
// label, and per group of labels `<group>#<n>` (one per switch, say):
//   - its main thread: how busy, and the self time of each kind of event
//     (script, style, layout, paint, GC, ...);
//   - V8's samples: the self time of each function, and the time spent in
//     each module, the function's own and what it called (inclusive).
// A function's module comes from the `//#region <path>` comments of the
// unminified build (SP6_UNMINIFIED=1) in `<assets>`. Writes
// `<name>.summary.json` and a readable `<name>.summary.txt`, then gzips the
// trace (DevTools and Perfetto open `.json.gz`).
//
//   node summarize.mjs <results dir> <renderer assets dir>

import { createReadStream, createWriteStream, readdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { pipeline } from "node:stream/promises";
import { createGzip } from "node:zlib";

const [results, assets] = process.argv.slice(2);
if (!results || !assets) {
  console.error("usage: node summarize.mjs <results dir> <renderer assets dir>");
  process.exit(2);
}

const ms = (us) => Math.round(us / 100) / 10;
const top = (map, n) =>
  [...map].sort((a, b) => b[1] - a[1]).slice(0, n).map(([key, us]) => [key, ms(us)]);
const add = (map, key, us) => map.set(key, (map.get(key) ?? 0) + us);

/** Each asset's regions, by its file name: sorted [first line (0-based), module]. */
function regions() {
  const byFile = new Map();
  for (const file of readdirSync(assets).filter((f) => f.endsWith(".js"))) {
    const lines = readFileSync(join(assets, file), "utf8").split("\n");
    const marks = [];
    lines.forEach((line, n) => {
      if (line.startsWith("//#region ")) marks.push([n, moduleName(line.slice(10).trim())]);
      else if (line.startsWith("//#endregion")) marks.push([n, null]);
    });
    byFile.set(file, marks);
  }
  return byFile;
}

/** A dependency by its package, the workspace's own code by its file. */
function moduleName(path) {
  const dep = /node_modules\/\.pnpm\/[^/]+\/node_modules\/((?:@[^/]+\/)?[^/]+)/.exec(path);
  if (dep) return dep[1];
  return path.replace(/^(\.\.\/)+/, "").replace(/^\0/, "");
}

const REGIONS = regions();

/** The module of a sampled frame. */
function moduleOf({ url, lineNumber, functionName }) {
  if (!url) return functionName.startsWith("(") ? functionName : "(no script)";
  if (!url.startsWith("app://cupper/")) return /playwright|pptr|__puppeteer/i.test(url) ? "(spike)" : url;
  const file = url.slice(url.lastIndexOf("/") + 1);
  const marks = REGIONS.get(file);
  if (!marks) return `(${file})`;
  let found = `(${file})`;
  for (const [line, name] of marks) {
    if (line > lineNumber) break;
    found = name ?? `(${file})`;
  }
  return found;
}

function summarize(file) {
  const raw = JSON.parse(readFileSync(file, "utf8"));
  const events = Array.isArray(raw) ? raw : raw.traceEvents;
  // The main document's renderer: the process that set the marks.
  const marks = events
    .filter((e) => e.name === "TimeStamp" && String(e.args?.data?.message).startsWith("sp6:"))
    .map((e) => ({ ...e, name: e.args.data.message }));
  const pid = marks[0]?.pid;
  const main = events.find(
    (e) => e.ph === "M" && e.name === "thread_name" && e.pid === pid && e.args?.name === "CrRendererMain",
  )?.tid;
  const stretches = new Map();
  for (const mark of marks) {
    const [, label, edge] = /^sp6:(.+):(start|end)$/.exec(mark.name) ?? [];
    if (!label) continue;
    const s = stretches.get(label) ?? {};
    s[edge] = mark.ts;
    stretches.set(label, s);
  }
  const windows = [...stretches].filter(([, s]) => s.start !== undefined && s.end !== undefined);

  // The main thread's events, each with its self time.
  const own = events
    .filter((e) => e.pid === pid && e.tid === main && (e.ph === "X" || e.ph === "B" || e.ph === "E"))
    .sort((a, b) => a.ts - b.ts || (a.ph === "E" ? -1 : 1));
  const spans = [];
  const open = [];
  for (const e of own) {
    if (e.ph === "E") {
      const span = open.pop();
      if (span) span.end = e.ts;
      continue;
    }
    const span = { name: e.name, start: e.ts, end: e.ph === "X" ? e.ts + (e.dur ?? 0) : undefined, children: 0 };
    if (e.ph === "B") open.push(span);
    spans.push(span);
  }
  spans.sort((a, b) => a.start - b.start || b.end - a.end);
  const stack = [];
  for (const span of spans) {
    if (span.end === undefined) continue;
    while (stack.length && stack.at(-1).end <= span.start) stack.pop();
    span.top = stack.length === 0;
    const parent = stack.at(-1);
    if (parent) parent.children += span.end - span.start;
    stack.push(span);
  }

  // V8's samples of the renderer's main isolate: [time, node] with each node's frames.
  const nodes = new Map();
  const profiles = new Map();
  for (const e of events) {
    if (e.pid !== pid) continue;
    if (e.name === "Profile") profiles.set(e.id, { at: e.args?.data?.startTime ?? e.ts, tid: e.tid, samples: [] });
    if (e.name !== "ProfileChunk") continue;
    const profile = profiles.get(e.id);
    if (!profile) continue;
    const data = e.args?.data ?? {};
    for (const node of data.cpuProfile?.nodes ?? []) nodes.set(`${e.id}:${node.id}`, { ...node, profile: e.id });
    const deltas = data.timeDeltas ?? [];
    (data.cpuProfile?.samples ?? []).forEach((id, n) => {
      profile.at += deltas[n] ?? 0;
      profile.samples.push([profile.at, `${e.id}:${id}`]);
    });
  }
  // The main thread's profile; if the trace names another thread, the one with the most samples.
  const all = [...profiles.values()];
  const chosen = all.filter((p) => p.tid === main);
  const picked = chosen.length ? chosen : all.sort((a, b) => b.samples.length - a.samples.length).slice(0, 1);
  const samples = picked.flatMap((p) => p.samples).sort((a, b) => a[0] - b[0]);
  const parentOf = (key) => {
    const node = nodes.get(key);
    return node?.parent === undefined ? undefined : `${node.profile}:${node.parent}`;
  };

  /** The raw sums of one stretch, in µs. */
  const sum = (from, to) => {
    const raw = { length: to - from, busy: 0, sampled: 0, self: new Map(), fn: new Map(), mod: new Map(), inc: new Map() };
    for (const span of spans) {
      if (span.end === undefined || span.end <= from || span.start >= to) continue;
      const length = Math.min(span.end, to) - Math.max(span.start, from);
      if (span.top) raw.busy += length;
      const share = length / Math.max(1, span.end - span.start);
      add(raw.self, span.name, Math.max(0, span.end - span.start - span.children) * share);
    }
    for (let n = 0; n < samples.length; n++) {
      const [at, key] = samples[n];
      if (at < from || at >= to) continue;
      const weight = Math.min((samples[n + 1]?.[0] ?? at) - at, 10_000);
      const node = nodes.get(key);
      if (!node) continue;
      const frame = node.callFrame;
      if (frame.functionName === "(idle)") continue;
      raw.sampled += weight;
      const where = moduleOf(frame);
      add(raw.fn, `${frame.functionName || "(anonymous)"} ${where}:${frame.lineNumber + 1}`, weight);
      add(raw.mod, where, weight);
      const seen = new Set();
      for (let k = key; k !== undefined; k = parentOf(k)) {
        const up = nodes.get(k);
        if (!up) break;
        const m = moduleOf(up.callFrame);
        if (!seen.has(m)) {
          seen.add(m);
          add(raw.inc, m, weight);
        }
      }
    }
    return raw;
  };
  const merge = (list) => {
    const out = { length: 0, busy: 0, sampled: 0, self: new Map(), fn: new Map(), mod: new Map(), inc: new Map() };
    for (const raw of list) {
      out.length += raw.length;
      out.busy += raw.busy;
      out.sampled += raw.sampled;
      for (const key of ["self", "fn", "mod", "inc"]) for (const [k, v] of raw[key]) add(out[key], k, v);
    }
    return out;
  };
  const present = (raw) => ({
    ms: ms(raw.length),
    main_thread: { busy_ms: ms(raw.busy), busy_share: Math.round((raw.busy / Math.max(1, raw.length)) * 100) / 100, self: top(raw.self, 20) },
    samples: { busy_ms: ms(raw.sampled), self_functions: top(raw.fn, 30), self_modules: top(raw.mod, 20), inclusive_modules: top(raw.inc, 30) },
  });

  const out = { file, renderer_pid: pid, samples: samples.length, windows: {}, groups: {} };
  for (const [label, { start, end }] of windows) out.windows[label] = present(sum(start, end));
  // A group (`<group>#<n>`, one per switch, say): its stretches summed, and each one's length.
  const groups = new Map();
  for (const [label, s] of windows) {
    const [, group] = /^(.+)#\d+$/.exec(label) ?? [];
    if (group) groups.set(group, [...(groups.get(group) ?? []), s]);
  }
  for (const [group, list] of groups) {
    out.groups[group] = {
      count: list.length,
      lengths_ms: list.map((s) => ms(s.end - s.start)),
      ...present(merge(list.map((s) => sum(s.start, s.end)))),
    };
  }
  return out;
}

function text(summary) {
  const lines = [`# ${summary.file}`, `renderer pid ${summary.renderer_pid}, ${summary.samples} samples`];
  const table = (title, rows) => {
    lines.push(`  ${title}`);
    for (const [k, v] of rows) lines.push(`    ${String(v).padStart(9)} ms  ${k}`);
  };
  const one = (title, w) => {
    lines.push("", `## ${title}: ${w.ms} ms, main thread busy ${w.main_thread.busy_ms} ms (${w.main_thread.busy_share}), sampled ${w.samples.busy_ms} ms`);
    table("main thread, self time by event", w.main_thread.self.slice(0, 12));
    table("JS self time by module", w.samples.self_modules.slice(0, 12));
    table("JS inclusive time by module", w.samples.inclusive_modules.slice(0, 20));
    table("JS self time by function", w.samples.self_functions.slice(0, 20));
  };
  for (const [label, w] of Object.entries(summary.windows)) one(label, w);
  for (const [group, g] of Object.entries(summary.groups)) one(`group ${group} (${g.count}: ${g.lengths_ms.join(", ")} ms)`, g);
  return `${lines.join("\n")}\n`;
}

for (const name of readdirSync(results).filter((f) => f.endsWith(".trace.json"))) {
  const file = join(results, name);
  const base = file.slice(0, -".trace.json".length);
  try {
    const summary = summarize(file);
    writeFileSync(`${base}.summary.json`, `${JSON.stringify(summary, null, 1)}\n`);
    writeFileSync(`${base}.summary.txt`, text(summary));
    console.log(`summarized ${name}`);
  } catch (error) {
    console.error(`${name}: ${error.stack ?? error}`);
  }
  await pipeline(createReadStream(file), createGzip(), createWriteStream(`${file}.gz`));
  rmSync(file);
}
