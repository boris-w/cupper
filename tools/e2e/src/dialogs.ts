// The native dialogs, replaced in the main process (design 5.9.9): the main
// process reads `dialog.showMessageBox` when it calls it, so the stand-in
// set here takes every call. Each call is recorded and waits until the test
// answers it; no native dialog ever shows or takes the focus.

import { expect } from "@playwright/test";
import type { ElectronApplication } from "@playwright/test";

/** What a call showed: the fields of `MessageBoxOptions` the tests look at. */
export interface MessageBox {
  index: number;
  type?: string;
  message: string;
  detail?: string;
  buttons?: string[];
  defaultId?: number;
  cancelId?: number;
  checkboxLabel?: string;
  answered: boolean;
}

type Answer = (response: number, checkboxChecked: boolean) => void;

interface Stand {
  calls: Omit<MessageBox, "index" | "answered">[];
  answers: (Answer | null)[];
}

export class Dialogs {
  readonly #app: ElectronApplication;

  private constructor(app: ElectronApplication) {
    this.#app = app;
  }

  /** Replaces `showMessageBox` and `showMessageBoxSync`; the latter answers `cancelId` at once. */
  static async install(app: ElectronApplication): Promise<Dialogs> {
    await app.evaluate(({ dialog }) => {
      const stand: Stand = { calls: [], answers: [] };
      Object.assign(globalThis, { __e2eDialogs: stand });
      const record = (args: unknown[]) => {
        const options = args.at(-1) as Electron.MessageBoxOptions;
        const { type, message, detail, buttons, defaultId, cancelId, checkboxLabel } = options;
        stand.calls.push({ type, message, detail, buttons, defaultId, cancelId, checkboxLabel });
        return options;
      };
      dialog.showMessageBox = (...args: unknown[]) => {
        record(args);
        return new Promise((resolve) =>
          stand.answers.push((response, checkboxChecked) => resolve({ response, checkboxChecked })),
        );
      };
      dialog.showMessageBoxSync = (...args: unknown[]) => {
        stand.answers.push(null);
        return record(args).cancelId ?? 0;
      };
    });
    return new Dialogs(app);
  }

  async calls(): Promise<MessageBox[]> {
    return this.#app.evaluate(() => {
      const stand = (globalThis as unknown as { __e2eDialogs: Stand }).__e2eDialogs;
      return stand.calls.map((call, index) => ({
        ...call,
        index,
        answered: stand.answers[index] === null,
      }));
    });
  }

  /** The first dialog from `from` on, once it shows. */
  async next(from = 0): Promise<MessageBox> {
    await expect
      .poll(async () => (await this.calls()).length, { message: "a native dialog" })
      .toBeGreaterThan(from);
    const [call] = (await this.calls()).slice(from);
    if (call === undefined) {
      throw new Error("the dialog went away");
    }
    return call;
  }

  /** Answers dialog `index` with button `response`, the checkbox ticked or not. */
  async answer(index: number, response: number, checkboxChecked = false): Promise<void> {
    await this.#app.evaluate(
      (_electron, [index, response, checked]) => {
        const stand = (globalThis as unknown as { __e2eDialogs: Stand }).__e2eDialogs;
        const answer = stand.answers[index];
        if (!answer) {
          throw new Error(`dialog ${index} is not waiting for an answer`);
        }
        stand.answers[index] = null;
        answer(response, checked);
      },
      [index, response, checkboxChecked] as const,
    );
  }
}
