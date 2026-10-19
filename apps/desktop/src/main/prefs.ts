import { readFile, rename, writeFile } from "node:fs/promises";

/** A whole prefs file larger than this is refused: the renderer is not trusted. */
export const MAX_PREFS_BYTES = 256 * 1024;

const KEY = /^[A-Za-z][\w.-]{0,127}$/;

export type PrefsListener = (key: string, value: unknown) => void;

/**
 * The UI preferences (design 5.7.2, 5.7.6), one JSON file in userData. They
 * are layout and habits only (widths, toggles, the send shortcut), so a
 * missing or broken file just starts over from the defaults. Values are
 * stored as JSON; writes go to a temporary file first and land in order.
 */
export class Prefs {
  readonly #file: string;
  #values: Map<string, unknown>;
  #saving: Promise<void> = Promise.resolve();
  readonly #listeners = new Set<PrefsListener>();

  private constructor(file: string, values: Map<string, unknown>) {
    this.#file = file;
    this.#values = values;
  }

  static async open(file: string): Promise<Prefs> {
    let values = new Map<string, unknown>();
    try {
      const parsed: unknown = JSON.parse(await readFile(file, "utf8"));
      if (typeof parsed === "object" && parsed !== null && !Array.isArray(parsed)) {
        values = new Map(Object.entries(parsed));
      }
    } catch {
      // Missing or broken: the defaults.
    }
    return new Prefs(file, values);
  }

  /** All preferences, for `ShellInfo.prefs`. */
  snapshot(): Record<string, unknown> {
    return structuredClone(Object.fromEntries(this.#values));
  }

  /** Stores `value` under `key`, then tells every listener. */
  async set(key: string, value: unknown): Promise<void> {
    if (!KEY.test(key)) {
      throw new TypeError(`not a preference key: ${JSON.stringify(key)}`);
    }
    const json = JSON.stringify(value) as string | undefined;
    if (json === undefined) {
      throw new TypeError(`preference ${key} is not a JSON value`);
    }
    const stored: unknown = JSON.parse(json);
    const next = new Map(this.#values).set(key, stored);
    const text = JSON.stringify(Object.fromEntries(next));
    if (Buffer.byteLength(text) > MAX_PREFS_BYTES) {
      throw new RangeError(`preferences would exceed ${MAX_PREFS_BYTES} bytes`);
    }
    this.#values = next;
    const saved = this.#saving.then(() => this.#write(text));
    this.#saving = saved.catch(() => undefined);
    await saved;
    for (const listener of this.#listeners) {
      listener(key, structuredClone(stored));
    }
  }

  onChange(listener: PrefsListener): () => void {
    this.#listeners.add(listener);
    return () => this.#listeners.delete(listener);
  }

  async #write(text: string) {
    const temporary = `${this.#file}.tmp`;
    await writeFile(temporary, text, { mode: 0o600 });
    await rename(temporary, this.#file);
  }
}
