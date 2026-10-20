// lifecycle (design 5.7.2, NFR-8): before the app quits, the host is asked
// whether turns run or requests wait; if so, a native dialog asks, and on yes
// `host/exit` ends them first. Logging out or shutting down (`powerMonitor`'s
// `shutdown`) and installing an update quit without asking (design 5.7.1):
// the host handles the system's SIGTERM, and an update has its own flow.

import type { HostStatusResult } from "@cupper/protocol";

/** What quitting needs from the control connection. */
export interface QuitHost {
  status(): Promise<HostStatusResult>;
  /** `host/exit`; `force` interrupts running turns. */
  exit(force: boolean): Promise<unknown>;
}

export class QuitGuard {
  readonly #host: () => QuitHost | undefined;
  readonly #confirm: (status: HostStatusResult) => Promise<boolean>;
  #skipped = false;
  #decided = false;
  #deciding: Promise<"quit" | "stay"> | undefined;

  constructor(
    host: () => QuitHost | undefined,
    confirm: (status: HostStatusResult) => Promise<boolean>,
  ) {
    this.#host = host;
    this.#confirm = confirm;
  }

  /** Shutdown, logout or an update: quit without asking. */
  skip(): void {
    this.#skipped = true;
  }

  /** `before-quit` may go on; otherwise wait for {@link decide}. */
  get settled(): boolean {
    return this.#skipped || this.#decided;
  }

  /** Asks the host, and the user when it matters; once at a time. */
  decide(): Promise<"quit" | "stay"> {
    this.#deciding ??= this.#ask().finally(() => (this.#deciding = undefined));
    return this.#deciding;
  }

  async #ask(): Promise<"quit" | "stay"> {
    const host = this.#host();
    // Without an answer, nothing can be lost by quitting: the host decides.
    const status = await host?.status().catch(() => undefined);
    if (status && (status.running_turns > 0 || status.pending_interactions > 0)) {
      if (!(await this.#confirm(status))) {
        return "stay";
      }
      // The host may close the connection before it answers.
      await host?.exit(true).catch(() => undefined);
    }
    this.#decided = true;
    return "quit";
  }
}
