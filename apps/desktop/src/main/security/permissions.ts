import type { Session } from "electron";

/**
 * The only permission any document gets (design 5.7.3): the copy buttons
 * write to the clipboard. Reading it (`clipboard-read`), media, location,
 * notifications and devices are all refused.
 */
export const ALLOWED_PERMISSIONS: ReadonlySet<string> = new Set(["clipboard-sanitized-write"]);

export function restrictPermissions(session: Session) {
  session.setPermissionRequestHandler((_contents, permission, callback) => {
    callback(ALLOWED_PERMISSIONS.has(permission));
  });
  session.setPermissionCheckHandler((_contents, permission) => ALLOWED_PERMISSIONS.has(permission));
  session.setDevicePermissionHandler(() => false);
}
