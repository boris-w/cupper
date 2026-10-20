// One-off conversion of the SP-1 recordings to recording format v1 (design
// 5.9.3, T052). Task material, not product code; see notes.md for how it ran.
//
//   node convert-sp1.ts <SP-1 recordings dir> <out dir>
//
// SP-1 writes one file per scenario and adapter, `<adapter>/<scenario>.jsonl`.
// Each adapter process it started begins with a `header` note; `t` counts from
// the start of the scenario or of the process, and `seq` restarts per process.
// Format v1 puts the versions into a header line, makes each process a segment
// that begins with an `instance_started` note, counts `t` from the start of the
// segment and keeps `seq`. The scenario is the file name: SP-1 kept two reruns
// under their own names (`claude/s07x-*`, `codex-bundled/s10x-*`) whose header
// still names the scenario they rerun.

import { mkdirSync, readdirSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";

type Json = Record<string, unknown>;

interface Sp1Line {
  seq: number;
  t: number;
  dir: "send" | "recv" | "recv-nonjson" | "note";
  msg?: Json;
  data?: unknown;
  note?: string;
}

interface Sp1Header {
  scenario: string;
  adapter: string;
  versions: { adapter: string; cli: string };
  [rest: string]: unknown;
}

const CLI = { "claude-agent-acp": "claude", "codex-acp": "codex" } as Record<string, string>;

function header(sp1: Sp1Header, scenario: string): Json {
  const adapter = /^@agentclientprotocol\/(\S+) (\d+\.\d+\.\d+)$/.exec(sp1.versions.adapter);
  const cliVersion = /\d+\.\d+\.\d+/.exec(sp1.versions.cli);
  const cli = adapter && CLI[adapter[1]!];
  if (!adapter || !cliVersion || !cli) throw new Error(`unknown versions ${JSON.stringify(sp1.versions)}`);
  return {
    format: "cupper-rec",
    v: 1,
    source: "sp1",
    adapter: adapter[1],
    adapter_version: adapter[2],
    cli,
    cli_version: cliVersion[0],
    scenario,
    usage: ["gold-a"],
  };
}

function convert(text: string, scenario: string): string[] {
  const sp1 = text.split("\n").filter(Boolean).map((l) => JSON.parse(l) as Sp1Line);
  const first = sp1[0];
  if (first?.note !== "header") throw new Error("the first line is not a header note");
  const head = header(first.data as Sp1Header, scenario);
  const out: Json[] = [];
  let instance = -1;
  let start = 0;
  for (const line of sp1) {
    const frame: Json = { seq: line.seq, t: 0, instance, dir: line.dir };
    if (line.note === "header") {
      // The scenario and the versions are in the header line; the rest stays.
      const { scenario: _, adapter: __, versions, ...rest } = line.data as Sp1Header;
      if (JSON.stringify(versions) !== JSON.stringify((first.data as Sp1Header).versions)) {
        throw new Error(`instance ${instance + 1} has other versions`);
      }
      instance += 1;
      start = line.t;
      Object.assign(frame, { instance, note: { event: "instance_started", ...rest } });
    } else if (line.dir === "note") {
      const event = line.note === "exit" ? "instance_exited" : line.note;
      frame.note = { event, ...(line.data as Json) };
    } else if (line.dir === "recv-nonjson") {
      Object.assign(frame, { dir: "recv_nonjson", data: line.data });
    } else {
      frame.msg = line.msg;
    }
    frame.t = line.t - start;
    out.push(frame);
  }
  return [head, ...out].map((l) => JSON.stringify(l));
}

const [inDir, outDir] = process.argv.slice(2);
if (!inDir || !outDir) throw new Error("usage: node convert-sp1.ts <SP-1 recordings dir> <out dir>");
let count = 0;
for (const adapter of readdirSync(inDir).sort()) {
  for (const name of readdirSync(join(inDir, adapter)).filter((n) => n.endsWith(".jsonl")).sort()) {
    const lines = convert(readFileSync(join(inDir, adapter, name), "utf8"), name.replace(/\.jsonl$/, ""));
    const h = JSON.parse(lines[0]!) as Json;
    const dir = join(outDir, String(h.adapter), String(h.adapter_version), String(h.cli_version));
    mkdirSync(dir, { recursive: true });
    writeFileSync(join(dir, name), lines.join("\n") + "\n");
    count += 1;
  }
}
console.log(`converted ${count} recordings`);
