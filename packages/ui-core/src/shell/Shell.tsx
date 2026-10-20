// The three columns of design 5.7.4: the sidebar, the session area under its
// title bar, and the side panel. The widths come from `computeLayout`; they
// reach the DOM as inline `width`, never as CSS custom properties.

import { commandHandler } from "@cupper/shell-api";
import { use, useEffect, useState, useSyncExternalStore, type ReactNode } from "react";

import { SlotsContext } from "../app/pages";
import { useHost, useShell, useT } from "../app/providers";
import { ChevronDownIcon, MoreIcon, PaneIcon } from "../components/icons";
import { Menu } from "../components/Menu";
import { ToolbarButton } from "../components/ToolbarButton";
import { ConnectionToast } from "../feedback/ConnectionToast";
import { SidebarFooter, SidebarHeader, SidebarList } from "../sidebar";
import { computeLayout, ratioOf, type Layout, type Viewport } from "../layout/layout";
import type { LayoutStore } from "../layout/store";
import { Splitter } from "./Splitter";

function useViewport(): Viewport {
  const read = () => ({ width: window.innerWidth, height: window.innerHeight });
  const [view, setView] = useState(read);
  useEffect(() => {
    const onResize = () => setView(read());
    window.addEventListener("resize", onResize);
    return () => window.removeEventListener("resize", onResize);
  }, []);
  return view;
}

export function Shell({ store, children }: { store: LayoutStore; children: ReactNode }) {
  const { api } = useShell();
  const { state } = useHost();
  const t = useT();
  const view = useViewport();
  const { prefs, overlayOpen } = useSyncExternalStore(store.subscribe, store.getState);
  const layout = computeLayout(view, prefs, overlayOpen);
  const narrow = layout.sidebar.mode === "overlay";

  useEffect(() => {
    if (!narrow) {
      store.setOverlay(false);
      return;
    }
    const onKey = (event: KeyboardEvent) => event.key === "Escape" && store.setOverlay(false);
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [narrow, store]);

  useEffect(
    () =>
      api.onCommand(
        commandHandler({
          toggle_sidebar: () => store.toggleSidebar(narrow),
          toggle_side_panel: () => store.toggleSidePanel(),
        }),
      ),
    [api, narrow, store],
  );

  const docked = layout.sidebar.visible && !narrow;
  return (
    <div className="flex h-full">
      <ConnectionToast state={state} />
      <div className="fixed top-2 left-(--layout-traffic-lights-w) z-30">
        <ToolbarButton
          label={t("shell:toggle_sidebar")}
          shortcut="⌘B"
          pressed={layout.sidebar.visible}
          onClick={() => store.toggleSidebar(narrow)}
        >
          <PaneIcon side="left" />
        </ToolbarButton>
      </div>
      {layout.sidebar.visible && narrow && (
        <div className="fixed inset-0 z-10 bg-scrim" onClick={() => store.setOverlay(false)} />
      )}
      {layout.sidebar.visible && <Sidebar layout={layout} store={store} />}
      <div className="flex min-w-0 flex-1 flex-col">
        <TitleBar
          inset={!docked}
          sidePanelOpen={prefs.sidePanelOpen}
          onSidePanel={() => store.toggleSidePanel()}
        />
        <main className="min-h-0 flex-1 overflow-y-auto">{children}</main>
      </div>
      {layout.sidePanel.visible && (
        <aside
          aria-label={t("shell:side_panel")}
          style={{ width: layout.sidePanel.width }}
          className="relative flex shrink-0 flex-col border-l border-border bg-surface"
        >
          <div
            role="tablist"
            className="app-drag flex h-(--layout-toolbar-h) shrink-0 items-center gap-1 pl-2"
          >
            <button
              type="button"
              role="tab"
              aria-selected
              className="app-no-drag h-8 min-w-22.5 rounded-lg border border-border bg-surface-elevated pr-1 pl-2.5 text-left text-sm"
            >
              {t("shell:tab_changes")}
            </button>
          </div>
          <div role="tabpanel" className="min-h-0 flex-1" />
          <Splitter
            label={t("shell:resize_side_panel")}
            edge="left"
            width={layout.sidePanel.width}
            bounds={layout.sidePanel}
            onResize={(width, done) =>
              store.set("sidePanelRatio", ratioOf(width, layout.sidePanel), done)
            }
          />
        </aside>
      )}
    </div>
  );
}

/** The 44 high title bar; `inset` leaves room for the traffic lights and the sidebar toggle. */
function TitleBar(props: { inset: boolean; sidePanelOpen: boolean; onSidePanel: () => void }) {
  const t = useT();
  const { handshake } = useHost();
  return (
    <header
      className={`app-drag flex h-(--layout-titlebar-h) shrink-0 items-center gap-1 pr-2 ${props.inset ? "pl-[calc(var(--layout-traffic-lights-w)+var(--spacing)*10)]" : "pl-2"}`}
    >
      <div className="min-w-0 flex-1" />
      {handshake?.host.weak_auth && (
        <ToolbarButton label={t("shell:weak_auth_detail")}>
          <span className="text-warning">{t("shell:weak_auth")}</span>
        </ToolbarButton>
      )}
      {/* The menus' items come with T153. */}
      <Menu
        trigger={
          <ToolbarButton label={t("shell:open")}>
            {t("shell:open")}
            <ChevronDownIcon />
          </ToolbarButton>
        }
      />
      <ToolbarButton
        label={t("shell:toggle_side_panel")}
        shortcut="⌥⌘B"
        pressed={props.sidePanelOpen}
        onClick={props.onSidePanel}
      >
        <PaneIcon side="right" />
      </ToolbarButton>
      <Menu
        trigger={
          <ToolbarButton label={t("shell:session_menu")}>
            <MoreIcon />
          </ToolbarButton>
        }
      />
    </header>
  );
}

function Sidebar({ layout, store }: { layout: Layout; store: LayoutStore }) {
  const t = useT();
  const slots = use(SlotsContext);
  const overlay = layout.sidebar.mode === "overlay";
  return (
    <aside
      aria-label={t("shell:sidebar")}
      style={{ width: layout.sidebar.width }}
      className={`flex shrink-0 flex-col bg-surface-sidebar ${overlay ? "app-no-drag fixed inset-y-0 left-0 z-20 shadow-card" : "relative"}`}
    >
      <div className="app-drag h-(--layout-titlebar-h) shrink-0" />
      <div className="flex h-8 shrink-0 items-center px-4">
        {slots.sidebarHeader ?? <SidebarHeader />}
      </div>
      <div className="min-h-0 flex-1 overflow-y-auto">{slots.sidebarList ?? <SidebarList />}</div>
      <div className="shrink-0">{slots.sidebarFooter ?? <SidebarFooter />}</div>
      <Splitter
        label={t("shell:resize_sidebar")}
        edge="right"
        width={layout.sidebar.width}
        bounds={layout.sidebar}
        onResize={(width, done) => store.set("sidebarWidth", width, done)}
      />
    </aside>
  );
}
