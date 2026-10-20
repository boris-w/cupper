// A small state router: the main area shows one page, chosen by `Route`. No
// URLs: the document never navigates (design 5.7.8). Deep links and the menu
// arrive through the shell.

import { commandHandler, isKnownTarget } from "@cupper/shell-api";
import { createContext, use, useEffect, useMemo, useState, type ReactNode } from "react";

import { required, useShell } from "./providers";

/** The page in the main area; the state layer of T080 keeps this shape. */
export type Route = { page: "new"; projectId?: string } | { page: "session"; sessionId: string };

export interface RouterValue {
  route: Route;
  navigate(route: Route): void;
}

const RouterContext = createContext<RouterValue | null>(null);

export function RouterProvider({ initial, children }: { initial?: Route; children: ReactNode }) {
  const { api } = useShell();
  const [route, navigate] = useState<Route>(initial ?? { page: "new" });
  useEffect(() => {
    const offCommand = api.onCommand(
      commandHandler({ new_session: () => navigate({ page: "new" }) }),
    );
    // Only navigates: a notification click never focuses the request panel.
    const offNavigate = api.onNavigate((target) => {
      if (isKnownTarget(target) && target.kind === "session") {
        navigate({ page: "session", sessionId: target.session_id });
      }
    });
    return () => {
      offCommand();
      offNavigate();
    };
  }, [api]);
  return (
    <RouterContext value={useMemo(() => ({ route, navigate }), [route])}>{children}</RouterContext>
  );
}

export const useRouter = () => required(use(RouterContext), "RouterProvider");
