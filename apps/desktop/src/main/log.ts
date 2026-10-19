import { closeSync, mkdirSync, openSync, renameSync, statSync, writeSync } from "node:fs";
import { join } from "node:path";

export type LogLevel = "info" | "warn" | "error";
export type LogFields = Record<string, string | number | boolean | null>;

export interface MainLogOptions {
  /** The home directory, written as `~` in every line (design 5.11.2). */
  home: string;
  /** Rotate before the file would pass this size. */
  maxBytes?: number;
  /** How many files to keep, `main.log` included. */
  files?: number;
  /** Also print each line; dev builds do (design 5.11.1). */
  echo?: boolean;
}

/**
 * `main.log` (design 5.11.1): JSON Lines, rotated by size, 5 MB × 3 by
 * default. Writes are synchronous: the main process logs little, and a line
 * written just before a crash is not lost. A failed write is dropped rather
 * than taking the main process down.
 */
export class MainLog {
  readonly #file: string;
  readonly #options: Required<MainLogOptions>;
  #fd: number | null = null;
  #size = 0;

  constructor(dir: string, options: MainLogOptions) {
    this.#options = { maxBytes: 5 * 1024 * 1024, files: 3, echo: false, ...options };
    this.#file = join(dir, "main.log");
    try {
      mkdirSync(dir, { recursive: true, mode: 0o700 });
      this.#open();
    } catch {
      this.#fd = null;
    }
  }

  info(msg: string, fields?: LogFields) {
    this.write("info", msg, fields);
  }

  warn(msg: string, fields?: LogFields) {
    this.write("warn", msg, fields);
  }

  error(msg: string, fields?: LogFields) {
    this.write("error", msg, fields);
  }

  write(level: LogLevel, msg: string, fields: LogFields = {}) {
    const entry = { time: new Date().toISOString(), level, msg, ...fields };
    const home = this.#home();
    const json = JSON.stringify(entry).replaceAll(`${home}/`, "~/").replaceAll(`${home}"`, '~"');
    const line = `${json}\n`;
    if (this.#options.echo) {
      process.stdout.write(line);
    }
    if (this.#fd === null) {
      return;
    }
    const bytes = Buffer.byteLength(line);
    try {
      if (this.#size > 0 && this.#size + bytes > this.#options.maxBytes) {
        this.#rotate();
      }
      writeSync(this.#fd, line);
      this.#size += bytes;
    } catch {
      // Logging must never take the main process down.
    }
  }

  close() {
    if (this.#fd !== null) {
      closeSync(this.#fd);
      this.#fd = null;
    }
  }

  /** The home directory as it appears inside a JSON string. */
  #home(): string {
    return JSON.stringify(this.#options.home).slice(1, -1);
  }

  #open() {
    this.#fd = openSync(this.#file, "a", 0o600);
    this.#size = statSync(this.#file).size;
  }

  /** `main.log` becomes `main.log.1`, `main.log.1` becomes `main.log.2`, and so on. */
  #rotate() {
    this.close();
    for (let n = this.#options.files - 1; n > 0; n--) {
      const from = n === 1 ? this.#file : `${this.#file}.${n - 1}`;
      try {
        renameSync(from, `${this.#file}.${n}`);
      } catch {
        // Not there yet.
      }
    }
    this.#open();
  }
}
