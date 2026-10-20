import type { ShellAPI, ShellInfo } from "@cupper/shell-api";
import { useEffect, useState } from "react";
import { createRoot } from "react-dom/client";

import { LayoutStore } from "../layout/store";
import { Shell } from "../shell/Shell";
import { Page, SlotsContext, type Slots } from "./pages";
import { HostProvider, I18nProvider, ShellProvider, ThemeProvider } from "./providers";
import { RouterProvider, type Route } from "./router";

export interface AppProps {
  api: ShellAPI;
  info: ShellInfo;
  slots?: Slots;
  initialRoute?: Route;
}

/** The root of design 5.7.5 on any ShellAPI: Electron's, or the browser mock. */
export function App({ api, info, slots = {}, initialRoute }: AppProps) {
  const [store] = useState(() => new LayoutStore(api.prefs, info.prefs));
  useEffect(() => () => store.close(), [store]);
  return (
    <ShellProvider api={api} info={info}>
      <I18nProvider locale={info.locale}>
        <ThemeProvider>
          <HostProvider>
            <RouterProvider initial={initialRoute}>
              <SlotsContext value={slots}>
                <Shell store={store}>
                  <Page />
                </Shell>
              </SlotsContext>
            </RouterProvider>
          </HostProvider>
        </ThemeProvider>
      </I18nProvider>
    </ShellProvider>
  );
}

/** Asks the shell for its facts, then renders the app into the document's `#root`. */
export async function mount(api: ShellAPI, slots?: Slots): Promise<void> {
  const element = document.getElementById("root");
  if (!element) {
    throw new Error("the document has no #root");
  }
  const info = await api.init();
  document.documentElement.lang = info.locale;
  createRoot(element).render(<App api={api} info={info} slots={slots} />);
}
