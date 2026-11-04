# T231 notes

## 规模估计（开工前）

停止线 550，硬停止线 650。

| 模块 | 行数 |
|---|---|
| `composer/config.ts`：从会话视图读能力里的选项和当前值；普通档和 elevated 发 `session/set_config`，完全访问走 `requestPrivileged`；失败时 toast；输入框上方的档位提示 | 60 |
| `Menu` 补受控打开、分组标题、当前项的勾 | 15 |
| `extensions/permission.tsx`：图标加短名，下拉列出各模式，elevated、完全访问各带一行说明 | 70 |
| `extensions/model.tsx`："{模型} {强度} ⌄"，下拉分"选择模型""选择推理强度"两组 | 60 |
| UI store 记下要打开的菜单；错误框 `change_model` 的"换模型"按钮 | 20 |
| 文案 | 20 |
| 组件测试 | 140 |
| E2E-full-access-native | 50 |
| 合计 | ~435 |

T141 时宿主偏高 38%，按 25–30% 上浮约 545–565，贴近停止线 550；写得紧凑，每个模块做完跑一次 `cargo xtask size`。

## 实现记录

- 界面完成后 `cargo xtask size` 为 +422；加上 E2E-full-access-native 后为 +463（估算约 435）。

## 取舍

- **按钮不用 `ToolbarButton`**：`ToolbarButton` 带 Radix 的 Tooltip，需要外层有 `TooltipProvider`。底栏按钮的文字本身就是短名，用不上提示框，所以在 `composer/config.tsx` 里另写了一个 `BarButton`：高 28，文字 13，`--radius-full`，菜单打开时用次级底色。
- **只在已有会话里显示**：两个扩展拿不到会话流（新建会话页）时不显示，交给 T144。能力里没有对应选项时也不显示；mock 宿主不发 `session.capabilities`，现有截图基线因此不受影响。
- **当前值**：取 `session.configured.config`，没有时取能力里的 `current`。名字用 agent 给的 `name`。
- **完全访问**：`risk` 既不是 `normal` 也不是 `elevated` 的取值都算完全访问（与宿主规则一致，未知档位按完全访问），走 `requestPrivileged({kind: "set_config"})`；其余直接发 `session/set_config`，失败时 toast "没有改成（{code}）"。
- **提示**：
  - 完全访问时，输入框上方显示警告框"完全访问权限已开启"；
  - elevated 时显示一行状态文字"低打扰模式已开启……"（比警告框轻）；
  - 菜单里，elevated 和完全访问的取值各带一行说明。
- **`change_model` 入口**：错误框里的"换模型"按钮把 UI store 的 `menu` 设为 `model`；模型菜单改成受控打开，跟随这个值。按钮单独写成一个组件，其他错误行不需要 `UiProvider`。
- **E2E**：先在原生确认框里取消，再确认。取消后同一种特权操作要等 2 秒（主进程的冷却，设计 5.7.8），用例里等 2.1 秒再试第二次。最后核对审计里只有一条 `session/set_config` 的 `privileged.executed`（`action: full_access`），说明取消的那次没有发到宿主。
- **审阅截图**：`review-permission-menu.png`、`review-model-menu.png`，用 e2e 构建、本机系统字体和假 agent 的 Claude 画像截取，用的是一个不提交的临时 spec。只供复刻度审阅，不进基线。
