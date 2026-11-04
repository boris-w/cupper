import { createContext, type ReactNode } from "react";

import type { ReviewViewComponent } from "../review/view";

/** The places later tasks fill: the sidebar's parts (T081), the composers (T086, T083), the diff view (T131). */
export interface Slots {
  sidebarHeader?: ReactNode;
  sidebarList?: ReactNode;
  sidebarFooter?: ReactNode;
  newSessionComposer?: ReactNode;
  sessionComposer?: ReactNode;
  reviewView?: ReviewViewComponent;
}

export const SlotsContext = createContext<Slots>({});
