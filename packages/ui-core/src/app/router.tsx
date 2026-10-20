// A small state router: the main area shows one page, chosen by `Route`. No
// URLs: the document never navigates (design 5.7.8). Deep links and the menu
// arrive through the shell. The route lives in the UI store.

import { commandHandler, isKnownTarget } from "@cupper/shell-api";
import { useEffect, useMemo, type ReactNode } from "react";
import { useStore } from "zustand";

import { useUi } from "../state/provider";
import type { Route } from "../state/ui";
import { useShell } from "./providers";

export type { Route } from "../state/ui";

export interface RouterValue {
  route: Route;
  navigate(route: Route): void;
}

/** Follows the menu and the shell's navigation into the route. */
export function RouterProvider({ children }: { children: ReactNode }) {
  const { api } = useShell();
  const ui = useUi();
  useEffect(() => {
    const { navigate } = ui.getState();
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
  }, [api, ui]);
  return children;
}

export function useRouter(): RouterValue {
  const ui = useUi();
  const route = useStore(ui, (state) => state.route);
  const navigate = useStore(ui, (state) => state.navigate);
  return useMemo(() => ({ route, navigate }), [route, navigate]);
}
