// The three columns of design 5.7.4: the sidebar, the session area under its
// title bar, and the side panel. The widths come from `computeLayout`; they
// reach the DOM as inline `width`, never as CSS custom properties. The buttons
// are plain placeholders on the tokens until T128 brings the base components.

import { commandHandler } from "@cupper/shell-api";
import { use, useEffect, useState, useSyncExternalStore, type ReactNode } from "react";

import { SlotsContext } from "../app/pages";
import { useHost, useShell, useT } from "../app/providers";
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

function ToolbarButton(props: {
  label: string;
  pressed?: boolean;
  onClick?: () => void;
  children: ReactNode;
}) {
  return (
    <button
      type="button"
      aria-label={props.label}
      title={props.label}
      aria-pressed={props.pressed}
      onClick={props.onClick}
      className="app-no-drag flex h-7 items-center rounded-lg px-2 text-sm text-fg-tertiary hover:bg-hover aria-pressed:bg-btn-secondary"
    >
      {props.children}
    </button>
  );
}

/** A pane outline with one side filled: the sidebar (left) or the side panel (right). */
function PaneIcon({ side }: { side: "left" | "right" }) {
  return (
    <svg width="16" height="16" viewBox="0 0 16 16" fill="none" stroke="currentColor" aria-hidden>
      <rect x="1.5" y="2.5" width="13" height="11" rx="2.5" />
      <path d={side === "left" ? "M6 2.5v11" : "M10 2.5v11"} />
    </svg>
  );
}

export function Shell({ store, children }: { store: LayoutStore; children: ReactNode }) {
  const { api } = useShell();
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
      <div className="fixed top-2 left-(--layout-traffic-lights-w) z-30">
        <ToolbarButton
          label={t("shell:toggle_sidebar")}
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
        <span title={t("shell:weak_auth_detail")} className="px-2 text-xs text-warning">
          {t("shell:weak_auth")}
        </span>
      )}
      <ToolbarButton label={t("shell:open")}>{t("shell:open")}</ToolbarButton>
      <ToolbarButton
        label={t("shell:toggle_side_panel")}
        pressed={props.sidePanelOpen}
        onClick={props.onSidePanel}
      >
        <PaneIcon side="right" />
      </ToolbarButton>
      <ToolbarButton label={t("shell:session_menu")}>⋯</ToolbarButton>
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
      <div className="min-h-0 flex-1 overflow-y-auto">{slots.sidebarList}</div>
      <div className="shrink-0">{slots.sidebarFooter}</div>
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

const STATUS_DOT = {
  connecting: "bg-accent",
  handshaking: "bg-accent",
  reconnecting: "bg-accent",
  ready: "bg-status-connected",
  failed: "bg-status-error",
  closed: "bg-status-disconnected",
} as const;

/** The product name and the host's status dot (design 5.7.4); T081 may replace it. */
function SidebarHeader() {
  const t = useT();
  const { state } = useHost();
  return (
    <div className="flex items-center gap-2 text-base font-medium">
      {t("shell:product")}
      <span
        role="img"
        aria-label={t(`shell:host_${state.status}`)}
        title={t(`shell:host_${state.status}`)}
        className={`size-2 rounded-full ${STATUS_DOT[state.status]}`}
      />
    </div>
  );
}
