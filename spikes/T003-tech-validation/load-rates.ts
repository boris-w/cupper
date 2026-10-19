// Peak output rates in recordings, to re-check the load of the main-process
// forwarding gate (design 5.7.1: per session stream 25 frames/s of ~2 KB on the
// wire, after the host's 40 ms write coalescing).
//
//   node load-rates.ts <recordings dir>...   (each dir holds *.jsonl in SP-1's format)
//
// A session stream is every `session/update` for one sessionId. Updates that
// arrive while a `session/load` request is pending are history replay (the host
// does not use replay, K8) and are reported separately. "Wire frames" model the
// host's coalescing: a batch opens at the first update and takes every update
// of the same stream that arrives within 40 ms; one batch = one frame. Sizes are
// ACP JSON bytes, a proxy for the host's own event encoding.
//
// Everything is computed twice: for all updates, and for content updates only
// (without the control updates below, which the driver turns into
// CommandsChanged / ConfigChanged / TitleChanged / Usage rather than items).

import { readdirSync, readFileSync } from "node:fs";
import { basename, join } from "node:path";

type Json = any;
const COALESCE_MS = 40;
const CONTROL_KINDS = new Set(["available_commands_update", "config_option_update", "current_mode_update", "session_info_update", "usage_update"]);

interface Upd {
  t: number;
  bytes: number;
  kind: string;
}

function percentile(xs: number[], p: number): number {
  if (!xs.length) return 0;
  const s = [...xs].sort((a, b) => a - b);
  return s[Math.min(s.length - 1, Math.ceil((p / 100) * s.length) - 1)];
}

/** Largest count / byte sum over any window of `ms` that starts at an event. */
function peakWindow(events: { t: number; bytes: number }[], ms: number) {
  let maxN = 0;
  let maxB = 0;
  let j = 0;
  let sum = 0;
  for (let i = 0; i < events.length; i++) {
    while (j < events.length && events[j].t < events[i].t + ms) sum += events[j++].bytes;
    maxN = Math.max(maxN, j - i);
    maxB = Math.max(maxB, sum);
    sum -= events[i].bytes;
  }
  return { maxN, maxB };
}

function coalesce(upds: Upd[]): { t: number; bytes: number }[] {
  const batches: { t: number; bytes: number }[] = [];
  for (const u of upds) {
    const last = batches[batches.length - 1];
    if (last && u.t < last.t + COALESCE_MS) last.bytes += u.bytes;
    else batches.push({ t: u.t, bytes: u.bytes });
  }
  return batches;
}

function streamsOf(file: string, contentOnly: boolean): Map<string, { live: Upd[]; replay: Upd[] }> {
  const streams = new Map<string, { live: Upd[]; replay: Upd[] }>();
  const loading = new Map<Json, string>(); // request id -> sessionId
  const loadingSessions = new Set<string>();
  for (const line of readFileSync(file, "utf8").split("\n")) {
    if (!line.trim()) continue;
    const f = JSON.parse(line);
    const m = f.msg;
    if (!m) continue;
    if (f.dir === "send" && m.method === "session/load") {
      loading.set(m.id, m.params.sessionId);
      loadingSessions.add(m.params.sessionId);
    }
    if (f.dir === "recv" && m.id !== undefined && loading.has(m.id)) {
      loadingSessions.delete(loading.get(m.id)!);
      loading.delete(m.id);
    }
    if (f.dir !== "recv" || m.method !== "session/update") continue;
    if (contentOnly && CONTROL_KINDS.has(m.params?.update?.sessionUpdate)) continue;
    const sid = m.params?.sessionId;
    if (!streams.has(sid)) streams.set(sid, { live: [], replay: [] });
    const u = { t: f.t, bytes: Buffer.byteLength(JSON.stringify(m)), kind: m.params?.update?.sessionUpdate };
    (loadingSessions.has(sid) ? streams.get(sid)!.replay : streams.get(sid)!.live).push(u);
  }
  return streams;
}

function analyse(dir: string, contentOnly: boolean) {
  const rows: Json[] = [];
  for (const name of readdirSync(dir).filter((n) => n.endsWith(".jsonl")).sort()) {
    let n = 0;
    for (const [, s] of streamsOf(join(dir, name), contentOnly)) {
      n++;
      for (const [mode, upds] of [["live", s.live], ["replay", s.replay]] as const) {
        if (upds.length < 2) continue;
        upds.sort((a, b) => a.t - b.t);
        const raw = peakWindow(upds, 1000);
        const batches = coalesce(upds);
        const wire = peakWindow(batches, 1000);
        rows.push({
          recording: `${basename(dir)}/${name}`,
          stream: n,
          mode,
          updates: upds.length,
          rawPeakPerSec: raw.maxN,
          rawPeakBytesPerSec: raw.maxB,
          rawMaxUpdateBytes: Math.max(...upds.map((u) => u.bytes)),
          wireFrames: batches.length,
          wirePeakFramesPerSec: wire.maxN,
          wirePeakBytesPerSec: wire.maxB,
          wireFrameBytesP50: percentile(batches.map((b) => b.bytes), 50),
          wireFrameBytesP95: percentile(batches.map((b) => b.bytes), 95),
          wireFrameBytesMax: Math.max(...batches.map((b) => b.bytes)),
          biggestKind: upds.reduce((a, b) => (b.bytes > a.bytes ? b : a)).kind,
        });
      }
    }
  }
  return rows;
}

const dirs = process.argv.slice(2);
if (!dirs.length) {
  console.error("usage: node load-rates.ts <recordings dir>...");
  process.exit(2);
}
function summarise(rows: Json[]) {
  const live = rows.filter((r) => r.mode === "live");
  const top = (key: string, k = 5) => [...live].sort((a, b) => b[key] - a[key]).slice(0, k);
  return {
    liveStreams: live.length,
    max: {
      rawPeakPerSec: Math.max(...live.map((r) => r.rawPeakPerSec)),
      rawPeakBytesPerSec: Math.max(...live.map((r) => r.rawPeakBytesPerSec)),
      wirePeakFramesPerSec: Math.max(...live.map((r) => r.wirePeakFramesPerSec)),
      wirePeakBytesPerSec: Math.max(...live.map((r) => r.wirePeakBytesPerSec)),
      wireFrameBytesMax: Math.max(...live.map((r) => r.wireFrameBytesMax)),
    },
    wireFrameBytesP50AcrossStreams: percentile(live.map((r) => r.wireFrameBytesP50), 50),
    wireFrameBytesP95AcrossStreams: percentile(live.map((r) => r.wireFrameBytesP95), 95),
    topByWireFramesPerSec: top("wirePeakFramesPerSec"),
    topByWireBytesPerSec: top("wirePeakBytesPerSec"),
    replay: rows.filter((r) => r.mode === "replay"),
  };
}

console.log(
  JSON.stringify(
    {
      coalesceMs: COALESCE_MS,
      allUpdates: summarise(dirs.flatMap((d) => analyse(d, false))),
      contentUpdates: summarise(dirs.flatMap((d) => analyse(d, true))),
    },
    null,
    1,
  ),
);
