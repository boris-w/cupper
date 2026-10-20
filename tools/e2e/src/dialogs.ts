// The native dialogs, as the e2e build's main process records them from its
// start (design 5.9.9, `apps/desktop/src/main/e2e-dialogs.ts`): each call
// waits until the test answers it here (the synchronous message box answers
// `cancelId` at once); no native dialog ever shows or takes the focus.

import { expect } from "@playwright/test";
import type { ElectronApplication } from "@playwright/test";

/** What a call showed: the fields of its options that the tests look at. */
export interface NativeDialog {
  index: number;
  /** `showMessageBox`, `showMessageBoxSync` or `showOpenDialog`. */
  kind: "message" | "message_sync" | "open";
  type?: string;
  message?: string;
  detail?: string;
  buttons?: string[];
  defaultId?: number;
  cancelId?: number;
  checkboxLabel?: string;
  /** `showOpenDialog`'s, such as `openDirectory`. */
  properties?: string[];
}

interface Stand {
  calls: Omit<NativeDialog, "index">[];
  answers: (((value: unknown) => void) | null)[];
}

export class Dialogs {
  readonly #app: ElectronApplication;

  private constructor(app: ElectronApplication) {
    this.#app = app;
  }

  /** The stand-ins that the e2e build's main process installed first thing (T178). */
  static async attach(app: ElectronApplication): Promise<Dialogs> {
    if (!(await app.evaluate(() => "__e2eDialogs" in globalThis))) {
      throw new Error("the app has no dialog stand-ins: not an e2e build");
    }
    return new Dialogs(app);
  }

  async calls(): Promise<NativeDialog[]> {
    return this.#app.evaluate(() => {
      const stand = (globalThis as unknown as { __e2eDialogs: Stand }).__e2eDialogs;
      return stand.calls.map((call, index) => ({ ...call, index }));
    });
  }

  /** The first dialog from `from` on, once it shows. */
  async next(from = 0): Promise<NativeDialog> {
    await expect
      .poll(async () => (await this.calls()).length, { message: "a native dialog" })
      .toBeGreaterThan(from);
    const [call] = (await this.calls()).slice(from);
    if (call === undefined) {
      throw new Error("the dialog went away");
    }
    return call;
  }

  /** Answers message box `index` with button `response`, the checkbox ticked or not. */
  async answer(index: number, response: number, checkboxChecked = false): Promise<void> {
    await this.#resolve(index, { response, checkboxChecked });
  }

  /** Answers open dialog `index` with `paths`, or cancels it. */
  async choose(index: number, paths: string[] | "cancel"): Promise<void> {
    const canceled = paths === "cancel";
    await this.#resolve(index, { canceled, filePaths: canceled ? [] : paths });
  }

  async #resolve(index: number, value: unknown): Promise<void> {
    await this.#app.evaluate(
      (_electron, [index, value]) => {
        const stand = (globalThis as unknown as { __e2eDialogs: Stand }).__e2eDialogs;
        const answer = stand.answers[index];
        if (!answer) {
          throw new Error(`dialog ${index} is not waiting for an answer`);
        }
        stand.answers[index] = null;
        answer(value);
      },
      [index, value] as const,
    );
  }
}
