# T253 notes

## 开工前的估算（2026-11-10）

停止线 240，硬停止线 290。按 T151、T248 的经验，界面代码比估的多约 50%，下面的数字已经算进去了。

| 模块 | 内容 | 行 |
|---|---|---|
| ShellAPI | `PrivilegedAction` 加 `{ kind: "install_cli" }`（附加），浏览器替身不用改 | 3 |
| 主进程：安装逻辑 | 见下文 | 60 |
| 主进程：确认流程 | 见下文 | 45 |
| 主进程文案 | `privileged` 命名空间：确认框的标题、正文、按钮 | 6 |
| UNIT-install-cli | 见下文 | 85 |
| 设置页"命令行工具" | 见下文 | 45 |
| 组件用例 | 见下文 | 40 |
| 截图 | 在 `settings.spec.ts` 的分页表里加一行 | 1 |
| 合计 | | 约 285 |

**主进程：安装逻辑**（`privileged/install-cli.ts`）：
- 链接名按构建种类取。
- 位置检查：`realpath` 之后，App 必须在 `/Applications` 或 `~/Applications` 下。
- 检查同名项：没有就可以装；是指向本 App CLI 的链接，算已安装；其他一律算被占用，不覆盖。
- 建链接：`mkdir -p` 之后 `symlink`，不经 shell。并发时如果同名文件先出现，`symlink` 会报 `EEXIST`，按被占用处理，同样不覆盖。

**主进程：确认流程**：
- `actions.ts` 里的流程：先检查，再弹原生确认框，再建链接；结果带上链接位置。
- `dialogs.ts` 里的确认框。
- `index.ts` 的动作表加一项。
- `main/index.ts` 把 App 路径、主目录、构建种类传进来。

**UNIT-install-cli**：
- 在临时目录里测：位置检查、链接名、被普通文件或别处的链接占用时不覆盖、已安装的判定、目录不存在时会建、并发占用。
- 再经 `Privileged` 测：确认框的内容；取消后不建链接。

**设置页"命令行工具"**：
- `CliSection.tsx`：一行说明加 `CommandButton`。
- 结果提示：成功时显示链接位置；被拒时说明原因；其他情况沿用 `feedbackOf`。
- 分页表加一行，加上 `settings` 文案。

**组件用例**：按钮的状态，以及成功、App 不在"应用程序"文件夹、被占用、取消这几种结果的提示。

## 链接名

按 5.7.9 里 CLI 的 identifier 推出，后缀规则与数据目录、URL scheme 相同：

| 构建种类 | CLI 的 identifier | 链接名 |
|---|---|---|
| release | `dev.cupper.cli` | `cupper` |
| local | `dev.cupper.cli.local` | `cupper-local` |
| dev | `dev.cupper.cli.dev` | `cupper-dev` |
| e2e | `dev.cupper.cli.e2e` | `cupper-e2e` |

`docs/user/` 里已经在用 `cupper-local`，与此一致。

## 被拒时怎么告诉界面

主进程返回 `failed`，`error` 为 `{code: "unavailable", reason, path}`，`reason` 取 `outside_applications` 或 `occupied`。界面对认识的 `reason` 给出具体说明，其他失败沿用 `feedbackOf` 的通用提示。

不用 `cancelled` 表示被拒，因为取消会计入冷却。
