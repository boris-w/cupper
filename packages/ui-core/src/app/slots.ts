import { createContext, type ReactNode } from "react";

/** The places later tasks fill: the sidebar's parts (T081) and the composers (T086, T083). */
export interface Slots {
  sidebarHeader?: ReactNode;
  sidebarList?: ReactNode;
  sidebarFooter?: ReactNode;
  newSessionComposer?: ReactNode;
  sessionComposer?: ReactNode;
}

export const SlotsContext = createContext<Slots>({});
