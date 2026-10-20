// The process table, for leaving nothing behind: a process is known by its
// pid and its start time, so that a pid reused by another process is never
// signalled.

import { execFileSync } from "node:child_process";

export interface Proc {
  pid: number;
  ppid: number;
  /** `ps`'s `lstart`, to tell a process from a later one with its pid. */
  started: string;
  args: string;
}

const LINE = /^\s*(\d+)\s+(\d+)\s+(\w+\s+\w+\s+\d+\s+[\d:]+\s+\d+)\s(.*)$/;

/** Every process of this user. */
export function processes(): Proc[] {
  const out = execFileSync("ps", ["-x", "-o", "pid=,ppid=,lstart=,args="], {
    encoding: "utf8",
    maxBuffer: 16 << 20,
  });
  return out
    .split("\n")
    .map((line) => LINE.exec(line))
    .filter((match) => match !== null)
    .map(([, pid, ppid, started = "", args = ""]) => ({
      pid: Number(pid),
      ppid: Number(ppid),
      started,
      args: args.trim(),
    }));
}

/** `pid` and the processes below it, while they are still its descendants. */
export function tree(pid: number, table = processes()): Proc[] {
  const found = table.filter((proc) => proc.pid === pid);
  // The iterator also visits what is pushed while it runs.
  for (const parent of found) {
    found.push(...table.filter((proc) => proc.ppid === parent.pid));
  }
  return found;
}

/** Those of `procs` that still run. */
export function alive(procs: readonly Proc[], table = processes()): Proc[] {
  return procs.filter((proc) =>
    table.some((p) => p.pid === proc.pid && p.started === proc.started),
  );
}

/** Signals those of `procs` that still run. */
export function signal(procs: readonly Proc[], name: NodeJS.Signals): void {
  for (const proc of alive(procs)) {
    try {
      process.kill(proc.pid, name);
    } catch {
      // It ended in between.
    }
  }
}

/** Waits until none of `procs` runs; the ones left after `ms`. */
export async function gone(procs: readonly Proc[], ms: number): Promise<Proc[]> {
  for (let waited = 0; ; waited += 100) {
    const left = alive(procs);
    if (left.length === 0 || waited >= ms) {
      return left;
    }
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
}
