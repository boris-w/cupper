// Extract the facts used in the SP-1 notes from sanitized recordings.
//   node summarize.ts <recordings dir>  > summary.json
import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";

const root = process.argv[2];
const KEEP_NOTES = new Set([
  "initialize-latency", "session-new-latency", "time-to-initialized", "listed-empty-session", "listed-after-restart",
  "storage-check", "storage-check-after-close", "session-close", "prompt-result", "queue-order", "steer-early-result",
  "steer-running-result", "steer-idle-optin-result", "steer-idle-default-result", "cancel-latency", "kill-cli",
  "after-cli-kill", "followup-after-kill", "load-result", "resume-result", "orphans-after-exit", "list-summary",
  "list-filtered", "session-new", "process", "crash", "exit", "request-timeout", "scenario-failed",
]);

const out: Record<string, Record<string, unknown>> = {};
for (const adapter of readdirSync(root).sort()) {
  out[adapter] = {};
  for (const file of readdirSync(join(root, adapter)).filter((f) => f.endsWith(".jsonl")).sort()) {
    const frames = readFileSync(join(root, adapter, file), "utf8").split("\n").filter(Boolean).map((l) => JSON.parse(l));
    const updateKinds: Record<string, number> = {};
    const agentRequests: Record<string, number> = {};
    const notes: unknown[] = [];
    const samples: unknown[] = [];
    for (const f of frames) {
      const m = f.msg;
      if (f.dir === "recv" && m?.method === "session/update") {
        const k = m.params.update.sessionUpdate;
        updateKinds[k] = (updateKinds[k] ?? 0) + 1;
      } else if (f.dir === "recv" && m?.method) {
        agentRequests[m.method] = (agentRequests[m.method] ?? 0) + 1;
      }
      if (f.dir === "note" && f.note === "proc-sample") {
        const byRole: Record<string, number> = {};
        for (const p of f.data.procs) byRole[p.label] = (byRole[p.label] ?? 0) + Math.round(p.rssKB / 1024);
        samples.push({ t: f.t, label: f.data.label, totalMB: Math.round(f.data.totalRssKB / 1024), processes: f.data.procs.length, byRoleMB: byRole });
      } else if (f.dir === "note" && KEEP_NOTES.has(f.note)) {
        const data = f.note === "crash" ? { procs: f.data.tree?.procs?.length } : f.data;
        notes.push({ t: f.t, note: f.note, ...(data && typeof data === "object" ? data : { data }) });
      }
    }
    out[adapter][file.replace(/\.jsonl$/, "")] = { frames: frames.length, updateKinds, agentRequests, notes, samples };
  }
}
console.log(JSON.stringify(out, null, 1));
