import {
  lstat,
  mkdir,
  mkdtemp,
  readFile,
  readlink,
  realpath,
  rm,
  symlink,
  writeFile,
} from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { describe, expect, test, onTestFinished } from "vitest";

import { installCli, planCli, type CliPlace } from "./install-cli";

/** A home in a temporary directory, with an app at `relative` that has its CLI. */
async function place(relative: string, flavor: CliPlace["flavor"] = "local"): Promise<CliPlace> {
  const home = await realpath(await mkdtemp(join(tmpdir(), "cupper-install-cli-")));
  onTestFinished(() => rm(home, { recursive: true, force: true }));
  const app = join(home, relative);
  await mkdir(join(app, "Contents", "Helpers"), { recursive: true });
  await writeFile(join(app, "Contents", "Helpers", "cupper"), "");
  return { app, home, flavor };
}

const linkOf = (cli: CliPlace, name: string) => join(cli.home, ".local", "bin", name);

describe("UNIT-install-cli", () => {
  test("an app outside the Applications folders is refused: the link would point at a place that goes away", async () => {
    const cli = await place("Downloads/Cupper Local.app");
    expect(await planCli(cli)).toEqual({ state: "outside_applications", path: cli.app });
    // `~/Applications.old` is not `~/Applications`.
    const near = await place("Applications.old/Cupper Local.app");
    expect((await planCli(near)).state).toBe("outside_applications");
  });

  test("the link's name is the flavor's; its directory is made, and then it counts as installed", async () => {
    const cli = await place("Applications/Cupper Dev.app", "dev");
    const link = linkOf(cli, "cupper-dev");
    const target = join(cli.app, "Contents", "Helpers", "cupper");
    const plan = await planCli(cli);
    expect(plan).toEqual({ state: "ready", link, target });
    expect(await installCli(plan as Extract<typeof plan, { state: "ready" }>)).toEqual({
      state: "installed",
      link,
    });
    expect(await readlink(link)).toBe(target);
    expect(await planCli(cli)).toEqual({ state: "installed", link });
    // A link to the CLI by another path, here through a linked directory, is ours too.
    await rm(link);
    await symlink(join(cli.home, "Applications"), join(cli.home, "apps"));
    await symlink(join(cli.home, "apps", "Cupper Dev.app", "Contents", "Helpers", "cupper"), link);
    expect(await planCli(cli)).toEqual({ state: "installed", link });
  });

  test("a file of the user's under the link's name is never replaced", async () => {
    const cli = await place("Applications/Cupper Local.app");
    const link = linkOf(cli, "cupper-local");
    await mkdir(join(cli.home, ".local", "bin"), { recursive: true });
    const occupied = { state: "occupied", path: link };
    // A file of the user's, a link elsewhere, a link that leads nowhere.
    await writeFile(link, "#!/bin/sh\n");
    expect(await planCli(cli)).toEqual(occupied);
    expect(await readFile(link, "utf8")).toBe("#!/bin/sh\n");
    await rm(link);
    await symlink("/bin/ls", link);
    expect(await planCli(cli)).toEqual(occupied);
    await rm(link);
    await symlink(join(cli.home, "gone"), link);
    expect(await planCli(cli)).toEqual(occupied);
    // One that turns up between the plan and the link stays.
    await rm(link);
    const plan = await planCli(cli);
    await writeFile(link, "mine");
    expect(await installCli(plan as Extract<typeof plan, { state: "ready" }>)).toEqual(occupied);
    expect([(await lstat(link)).isSymbolicLink(), await readFile(link, "utf8")]).toEqual([
      false,
      "mine",
    ]);
  });
});
