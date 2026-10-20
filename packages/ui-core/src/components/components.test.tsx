// @vitest-environment jsdom
import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import type { ReactNode } from "react";
import { afterEach, beforeAll, describe, expect, test, vi } from "vitest";

import { I18nProvider } from "../app/providers";
import { RowRegistry, RowView, type Row } from "../rows/registry";
import { Alert } from "./Alert";
import { Menu, MenuItem } from "./Menu";
import { Switch } from "./Switch";
import { ToolbarButton } from "./ToolbarButton";
import { TooltipProvider } from "./Tooltip";

beforeAll(() => {
  // jsdom has no layout: Radix's popper measures with this.
  vi.stubGlobal(
    "ResizeObserver",
    class {
      observe = vi.fn();
      unobserve = vi.fn();
      disconnect = vi.fn();
    },
  );
});
afterEach(cleanup);

const show = (node: ReactNode) =>
  render(
    <I18nProvider locale="zh-CN">
      <TooltipProvider>{node}</TooltipProvider>
    </I18nProvider>,
  );

describe("UNIT-ui-components", () => {
  test("a toolbar button is named, pressed, clicked by keyboard and shows its tooltip", async () => {
    const user = userEvent.setup();
    const onClick = vi.fn();
    show(<ToolbarButton label="切换侧边栏" shortcut="⌘B" pressed onClick={onClick} />);
    const button = screen.getByRole("button", { name: "切换侧边栏" });
    expect(button.getAttribute("aria-pressed")).toBe("true");
    await user.tab();
    expect(await screen.findByRole("tooltip")).toHaveProperty("textContent", "切换侧边栏⌘B");
    await user.keyboard("{Enter}");
    await user.keyboard(" ");
    expect(onClick).toHaveBeenCalledTimes(2);
  });

  test("a disabled toolbar button takes no click", async () => {
    const onClick = vi.fn();
    show(<ToolbarButton label="打开" disabled onClick={onClick} />);
    await userEvent.click(screen.getByRole("button", { name: "打开" }));
    expect(onClick).not.toHaveBeenCalled();
  });

  test("a menu opens from the keyboard, selects an item, skips a disabled one and closes", async () => {
    const user = userEvent.setup();
    const [rename, archive] = [vi.fn(), vi.fn()];
    show(
      <Menu trigger={<ToolbarButton label="会话菜单" />}>
        <MenuItem onSelect={rename}>重命名</MenuItem>
        <MenuItem onSelect={archive} disabled>
          归档
        </MenuItem>
      </Menu>,
    );
    const trigger = screen.getByRole("button", { name: "会话菜单" });
    expect(trigger.getAttribute("aria-haspopup")).toBe("menu");
    trigger.focus();
    await user.keyboard("{Enter}");
    expect(trigger.getAttribute("aria-expanded")).toBe("true");
    expect(screen.getByRole("menuitem", { name: "归档" }).getAttribute("aria-disabled")).toBe(
      "true",
    );
    await user.keyboard("{Escape}");
    expect(screen.queryByRole("menu")).toBeNull();

    await user.keyboard("{Enter}");
    await user.keyboard("{ArrowDown}{ArrowDown}{Enter}");
    expect(rename).toHaveBeenCalledOnce();
    expect(archive).not.toHaveBeenCalled();
    expect(screen.queryByRole("menu")).toBeNull();
  });

  test("a switch toggles with Space unless disabled", async () => {
    const user = userEvent.setup();
    const onChange = vi.fn();
    const { rerender } = show(<Switch label="开关" checked={false} onCheckedChange={onChange} />);
    const control = screen.getByRole("switch", { name: "开关" });
    expect(control.getAttribute("aria-checked")).toBe("false");
    control.focus();
    await user.keyboard(" ");
    expect(onChange).toHaveBeenCalledWith(true);
    rerender(<Switch label="开关" checked onCheckedChange={onChange} disabled />);
    await user.click(screen.getByRole("switch", { name: "开关" }));
    expect(onChange).toHaveBeenCalledOnce();
  });

  test("an alert is a status, or an alert when it is an error, with its action", () => {
    show(
      <>
        <Alert action={<button type="button">信任</button>}>未信任</Alert>
        <Alert tone="danger">出错</Alert>
      </>,
    );
    expect(screen.getByRole("status").textContent).toBe("未信任信任");
    expect(screen.getByRole("alert").textContent).toBe("出错");
  });
});

describe("UNIT-ui-row-registry", () => {
  type TestRow = Row & { text: string };
  const Message = ({ row }: { row: TestRow }) => <p>{row.text}</p>;

  test("a registered kind draws its component; another kind shows the placeholder only", () => {
    const registry = new RowRegistry<TestRow>();
    registry.register("user_message", Message);
    show(
      <>
        <RowView registry={registry} row={{ kind: "user_message", text: "你好" }} />
        <RowView registry={registry} row={{ kind: "hologram", text: "原始数据" }} />
      </>,
    );
    expect(screen.getByText("你好")).toBeTruthy();
    expect(screen.getByText("此版本不支持显示这项内容")).toBeTruthy();
    expect(screen.queryByText("原始数据")).toBeNull();
  });

  test("a kind registers once", () => {
    const registry = new RowRegistry<TestRow>();
    registry.register("plan", Message);
    expect(() => registry.register("plan", Message)).toThrow("registered already");
    expect(registry.resolve("plan")).toBe(Message);
  });
});
