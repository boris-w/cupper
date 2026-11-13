// Installing the command-line tool (design 5.10.1, P-6): a symbolic link in
// `~/.local/bin` to the app's own `Contents/Helpers/cupper`, made with no
// shell. The app has to run from an Applications folder: from a disk image
// or a download the link would point at a place that goes away. A file of
// the user's under the link's name is never replaced; a link there to this
// app's CLI is the tool installed already.

import { lstat, mkdir, realpath, symlink } from "node:fs/promises";
import { dirname, join, sep } from "node:path";

import type { Flavor } from "../../shared/flavor";

/** Each flavor's link name, from its CLI identifier (design 5.7.9): `dev.cupper.cli.local` → `cupper-local`. */
export const CLI_NAMES: Record<Flavor, string> = {
  release: "cupper",
  local: "cupper-local",
  dev: "cupper-dev",
  e2e: "cupper-e2e",
};

export interface CliPlace {
  /** The app bundle this process runs from. */
  app: string;
  home: string;
  flavor: Flavor;
}

/** What installing would find. */
export type CliPlan =
  | { state: "outside_applications"; path: string }
  /** Something else has the link's name. */
  | { state: "occupied"; path: string }
  | { state: "installed"; link: string }
  | { state: "ready"; link: string; target: string };

/** Where the app is, and what is at the link's place. */
export async function planCli({ app, home, flavor }: CliPlace): Promise<CliPlan> {
  const real = await realpath(app);
  const folders = ["/Applications", join(await realpath(home), "Applications")];
  if (!folders.some((folder) => real.startsWith(folder + sep))) {
    return { state: "outside_applications", path: real };
  }
  const target = join(real, "Contents", "Helpers", "cupper");
  const dir = join(home, ".local", "bin");
  const link = join(dir, CLI_NAMES[flavor]);
  const found = await lstat(link).catch((error: NodeJS.ErrnoException) => {
    if (error.code === "ENOENT") return undefined;
    throw error;
  });
  if (found === undefined) {
    return { state: "ready", link, target };
  }
  // A link, by whatever path, to this app's CLI; a dangling one is not ours.
  const reaches = found.isSymbolicLink() ? await realpath(link).catch(() => null) : null;
  return reaches === target ? { state: "installed", link } : { state: "occupied", path: link };
}

/**
 * Makes the link a `ready` plan names, and its directory when missing. A
 * file that turned up there in the meantime stays: `symlink` never replaces
 * one, and the caller gets `occupied`.
 */
export async function installCli(
  plan: Extract<CliPlan, { state: "ready" }>,
): Promise<Extract<CliPlan, { state: "installed" | "occupied" }>> {
  await mkdir(dirname(plan.link), { recursive: true });
  try {
    await symlink(plan.target, plan.link);
  } catch (error) {
    if ((error as NodeJS.ErrnoException).code === "EEXIST") {
      return { state: "occupied", path: plan.link };
    }
    throw error;
  }
  return { state: "installed", link: plan.link };
}
