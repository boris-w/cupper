# T253 汇报

- 执行者：子 agent（前端与桌面主进程工程师）
- 分支：`feat/T253-install-cli`
- 完成日期：2026-11-10
- PR：https://github.com/boris-w/cupper/pull/102
- CI：https://github.com/boris-w/cupper/actions/runs/42763045184 。`ae106ad3` 上 8 个检查全部通过。
- 基线：ui-baseline run 42761873803，核对后与测试 ID 的改名一起提交为 `ae106ad3`。

## 做了什么

**ShellAPI**：`PrivilegedAction` 附加 `{ kind: "install_cli" }`，只做附加，`frozen.ts` 的检查通过。`PrivilegedResult` 的注释写明 `install_cli` 成功时带 `{link}`。

**主进程**
- **安装逻辑**（`privileged/install-cli.ts`）：
  - 链接名按构建种类取，见"关键取舍"。
  - 位置检查：App 路径先 `realpath`，必须在 `/Applications` 或 `~/Applications` 之下。所以从 DMG、下载目录或被 macOS 转移运行时都过不了；`~/Applications.old` 也不算。
  - 链接放在 `~/.local/bin/<链接名>`，指向 `Contents/Helpers/cupper`。
  - 同名项用 `lstat` 判定：没有就可以装；是链接、而且 `realpath` 之后正是本 App 的 CLI（经由什么路径指过来都行），算已安装；其他一律算被占用，包括普通文件、指向别处的链接和悬空的链接。
  - 建链接：`mkdir -p` 之后 `symlink`，不经 shell。检查之后、建链接之前，同名文件如果先出现，`symlink` 会报 `EEXIST`，按被占用处理，不覆盖。
- **确认流程**（`actions.ts`、`dialogs.ts`）：
  - 已安装时不弹框，直接返回成功和链接位置。
  - 被拒时不弹框，返回 `failed`。
  - 可以装时弹原生确认框，写明要建什么链接、指向哪里，并提示 `~/.local/bin` 要在 PATH 里；默认按钮和 Esc 都是取消。
  - 走 `requestPrivileged` 的通用规则：真实输入、同一时间只弹一个确认框、取消后冷却。主进程照旧在调用时才取 `dialog.showMessageBox`。
- **入口**：`main/index.ts` 把 App 路径（由 `process.execPath` 上推两级）、主目录和构建种类传给 `Privileged`；动作表加了 `install_cli`。

**设置页"命令行工具"**（`CliSection.tsx`）
- 分页表最后加一行 `{ id: "cli", Body: CliSection }`，位置按设计 5.7.4 排在最后。
- 页里一行"在终端里使用 cupper"、一段说明和 `CommandButton`。说明里讲了链接放在哪里、PATH 的要求，以及命令行默认只读。
- 结果用 toast 提示：
  - 成功：说明链接在哪里，这一行随后显示"已安装：<链接>"；
  - 被拒：说明原因，见"关键取舍"第 2 条；
  - 取消：不提示；
  - 其他：沿用 `feedbackOf` 的通用提示。
- 退化模式下入口照常显示：CLI 按 uid 只读接入，不受影响。
- 主进程给的路径经 `escapeInvisible`，只作为文本显示；文案都走 `@cupper/i18n`。

**用户文档**：`docs/user/selfuse-install.md` 不再让用户手动 `ln -s`，改成指向"设置 → 命令行工具"，写明不装、不覆盖的两种情况和 PATH 的要求。"手动更新"一节也相应改了一句：设置页装的链接指向"应用程序"文件夹里的 App，替换 App 以后仍然有效。

## 关键取舍

1. **链接名**：按设计 5.7.9 里 CLI 的 identifier 推出，后缀规则与数据目录、URL scheme 相同。`docs/user/` 里原来就在用 `cupper-local`，与此一致。编排者确认过。

| 构建种类 | CLI 的 identifier | 链接名 |
|---|---|---|
| release | `dev.cupper.cli` | `cupper` |
| local | `dev.cupper.cli.local` | `cupper-local` |
| dev | `dev.cupper.cli.dev` | `cupper-dev` |
| e2e | `dev.cupper.cli.e2e` | `cupper-e2e` |

2. **被拒时的返回值**：App 不在"应用程序"文件夹、同名文件被占用这两种情况，返回 `failed`，`error` 为 `{code: "unavailable", reason, path}`，`reason` 取 `outside_applications` 或 `occupied`。界面据此给出具体说明：前者附上 App 现在的位置，后者附上被占用的路径并说明没有覆盖。不用 `cancelled` 表示被拒，因为取消会计入冷却。编排者确认过。
3. **同时接受 `~/Applications`**，都按 `realpath` 之后的路径比较。编排者确认过。
4. **已安装不弹框**：没有任何改动要做，弹框只是多问一次。

## 验证方式

- **PR #102 上 8 个检查全部通过**（run 42763045184，`ae106ad3`）：rust、ts、ui、protocol、e2e、license、testdata-scan、compat-framework。
- **本机**（单份）：
  - tsc、eslint、`prettier --check .` 通过。
  - vitest：desktop 114、ui-core 145、i18n 10、shell-api 15 条，全部通过。
  - `cargo xtask ci protocol` 通过，其中 `shell-api-m1a`（`frozen.ts` 的检查）通过。
  - 测试一律在临时目录里进行，没有往本机的 `~/.local/bin` 装任何东西。
- **对照验收标准**：
  - **UNIT-install-cli 通过**（`install-cli.test.ts` 3 条）：
    - 位置检查，包括 `~/Applications.old` 不算；
    - 链接名，以及目录不存在时会建出来；
    - 已安装的判定，包括经由被链接的目录指过来的情况；
    - 用户的文件、别处的链接、悬空的链接、检查之后才出现的文件都不覆盖。
  - **`Privileged` 层的用例通过**（UNIT-privileged 里 1 条）：确认框的标题和正文、取消后不建链接、确认后建链接、已安装时不弹框、被拒时不弹框。
  - **组件用例通过**（UI-settings-cli）：取消不提示，被占用、确认框忙、成功三种提示，成功后这一行显示链接位置。✓
  - **主进程的安全约束**：特权操作只在主进程执行，只经原生确认框；不经 shell（`fs/promises` 的 `symlink`）；不覆盖用户的文件；`dialog.showMessageBox` 在调用时才取。✓
  - **ShellAPI 只做附加**，`frozen.ts` 的检查通过；文案走 i18n。✓
  - **PR 上 8 个检查全部通过**，链接见上。✓

## 测试 ID 的分工

- **UI-settings-cli**（编排者为这一页登记，也列进了 UI-settings 的组成部分）：这一页的组件用例和截图亮暗两张。
- **UNIT-install-cli**：只用于主进程的 `install-cli.test.ts`。
- 我起初把组件用例和截图也写在 UNIT-install-cli 名下（当时只登记了这一个），登记新 ID 后在 `ae106ad3` 里改了名。

## 规模

- **最终 +369 −11**（`cargo xtask size`）。
- 停止线和硬停止线放宽过两次：
  - 开工前估约 285，超过原停止线 240，编排者把线放到 290/340；
  - 主进程部分做完时推算约 345，超过 340，编排者又把硬停止线放到 380。
- 每做完一个模块都统计了，记在 notes 里：

| 模块 | 估 | 实际 |
|---|---|---|
| ShellAPI | 3 | +7 |
| 主进程安装逻辑 | 60 | +76 |
| 主进程确认流程 | 45 | +51 |
| 主进程文案 | 6 | +5 |
| UNIT-install-cli（含 `Privileged` 层的用例） | 85 | +134 |
| 设置页（`CliSection.tsx`、分页表、文案） | 40 | +70 |
| 组件用例 | 30 | +26 |
| 截图规格 | 1 | +1 |

- **超出的原因**：主要是用例。它们覆盖的都是真实情况：悬空的链接、检查之后才出现的文件、`~/Applications.old`、经由被链接的目录指过来的链接。编排者要求保留，不靠压缩去守线。

## 基线核对

- run 42761873803 的产物下载到一个新的空目录，共 74 个文件，逐字节比对：
  - 完全相同 66 个；
  - 新增 2 个：`settings-cli-light/dark`，看过画面；
  - 变化 6 个：`settings-agents`、`settings-archived`、`settings-connections` 各亮暗两张。原因是导航末尾多了"命令行工具"一项。逐像素比对，变化只在导航第四行的位置（x 16–85、y 181–194），其余完全相同。

## 提交列表

```
ae106ad3 test(ui): baselines for the CLI page, under UI-settings-cli
7f192852 docs(user): install the CLI from the settings page
b3771a58 feat(ui): a settings page to install the command-line tool
2051239e docs(T253): size after the main process part
fecfd4fd feat(desktop): install the CLI as a privileged action
9f6c2e3e docs(T253): estimate before starting
```

## 遗留问题 / 范围外发现

- **文案里重复的键**：我在 `settings.json` 里一度又加了一个 `cli` 键，而 T151 已经用它表示 agent 卡上的"命令行工具"一行。JSON 里后写的会覆盖先写的，T151 的用例当场发现了，已改名为 `cli_section`。i18n 的用例查不出重复的键（`JSON.parse` 默默保留后一个）。编排者已把"给 i18n 加重复键检查"转进交接事项。
- **给 T147**：T253 先合并，T147 合并时要 merge main，解决分页表末尾和 `settings.json` 末尾的冲突。导航会再变，设置页那几张基线要重新生成。
- **不做**（按任务书）：卸载命令行工具、自定义安装目录。用户想卸载时手动删掉链接即可，`docs/user/` 的"卸载"一节已经提到删除命令行的链接。
- worktree 保留着。
