import type { KnownCommandId, ShellCommand } from "./shell-api";

export type CommandHandlers = Partial<Record<KnownCommandId, (arg: number | undefined) => void>>;

/**
 * An `onCommand` callback that runs the handler of the command's ID and
 * ignores every other command, including IDs this build does not know yet.
 * Only the handlers' own keys count, so an ID like `toString` reaches nothing.
 */
export function commandHandler(handlers: CommandHandlers): (cmd: ShellCommand) => void {
  return (cmd) => {
    if (Object.hasOwn(handlers, cmd.id)) {
      handlers[cmd.id as KnownCommandId]?.(cmd.arg);
    }
  };
}
