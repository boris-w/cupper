// The native extension (T073), loaded from outside the asar: `Resources/native/
// cupper_native.node` in the app (design 5.1.3). Its build must match the
// app's: a release app talks to a release host (`signature`); local, e2e and
// dev builds prove the one-time key (`key_proof`, design 5.7.9, 5.8.1).

import type { DevAuthNative, ReleaseNative } from "@cupper/desktop-native";

import type { Flavor } from "../../shared/flavor";

export type Native =
  { keyed: false; native: ReleaseNative } | { keyed: true; native: DevAuthNative };

export function loadNative(path: string, flavor: Flavor): Native {
  const module = { exports: {} };
  process.dlopen(module, path);
  return checkNative(module.exports as ReleaseNative | DevAuthNative, flavor);
}

/** Refuses an extension built for another flavor. */
export function checkNative(native: ReleaseNative | DevAuthNative, flavor: Flavor): Native {
  const keyed = native.buildInfo().hostCheck === "key_proof";
  if (keyed !== (flavor !== "release")) {
    throw new Error(
      `the native extension (${native.buildInfo().hostCheck}) is not the ${flavor} build's`,
    );
  }
  return keyed
    ? { keyed, native: native as DevAuthNative }
    : { keyed, native: native as ReleaseNative };
}
