# T074 桌面主进程 I：应用骨架、构建与加固基线

- 派发日期：2026-10-19
- 执行者：子 agent（Electron 工程师，桌面主进程）
- 分支：`feat/T074-desktop-main-skeleton`
- 依赖：T070（已合并）。M1a 第 2 波，处在桌面链上：T074 → T075 → T076 → T077。

## 背景

桌面端是 Electron 应用，界面 100% 复刻 Codex。主进程持有到宿主的全部连接、命令白名单、原生确认、真实输入检查，渲染进程是沙箱里的不可信内容（ADR-03，设计 5.7）。本任务只搭主进程的应用骨架，以及加固的基线；宿主连接、windowBridge、真实输入由 T075–T077 接着做。

## 范围

**做**（计划 T074 一行）：
1. **`apps/desktop`**
   - Electron 版本取 44.x 当天的最新补丁版本。2026-10-19 是 44.9.0，开工时复核，写进 `notes.md`。
   - 运行时依赖全部由 Vite 打进 bundle，`apps/desktop` 不声明 `dependencies`（5.1.3）。
2. **Vite 构建三个入口**
   - 主进程；
   - preload：打成单个 CommonJS 文件，因为沙箱里的 preload 不支持 ESM；
   - 渲染进程的主文档。
   
   审查文档是 M1b 的事，先在配置里留好位置。
3. **`app://cupper` 协议**
   - 注册为 standard、secure、`supportFetchAPI`、`codeCache`；
   - 按 5.7.8 下发 CSP 响应头；
   - 路径先规范化，再检查前缀，防止目录穿越。
4. **加固基线**（5.7.8）
   - webPreferences：开启 `sandbox` 和 `contextIsolation`，关闭 `nodeIntegration` 和 `nodeIntegrationInSubFrames`；
   - 拦截导航和新窗口：外部链接交给 `openExternal`，只放行 http 和 https；
   - 权限处理器：放行 `clipboard-sanitized-write`，其余一律拒绝；
   - 命令行开关用白名单，出现未知开关就退出（发布配置）。
5. **BrowserWindow 参数**（5.7.4）
   - 默认 1280 × 820，最小 480 × 600；
   - `hiddenInset`，交通灯位置按设计；
   - 用 `show: false` 加 `ready-to-show`，避免白屏。
6. **单实例锁**；**userData** 显式设为数据目录下的 `ui/`（5.4.1）；**构建种类常量** `__CUPPER_FLAVOR__`（发布、自用、开发、e2e）。
7. **打包配置**：开发构建和 e2e 构建的 electron-builder 配置（5.7.9）：
   - e2e 构建：ad-hoc 签名、关闭 hardened runtime、关闭 `enableCookieEncryption`、只保留 `enableNodeCliInspectArguments`；
   - 发布构建和自用构建（T160）的配置不在本任务里。
8. **其余**：prefs（主进程侧的偏好存储）、`main.log`、菜单骨架。菜单项对应 5.7.7 的命令，经 `onCommand` 送达；具体由后续任务接线。

**不做**：
- 宿主拉起与连接（T075）；
- windowBridge 与 preload 的实际接口（T076）；
- 真实输入（T077）；
- 自用构建的配置（T160）；
- 界面（T080 起）。

## 验收标准

- [ ] 新增 UNIT-main-hardening，覆盖：
  - webPreferences 各项；
  - 导航和新窗口被拦截；
  - 权限处理器只放行白名单；
  - `app://` 路径穿越被拒；
  - CSP 响应头存在；
  - 未知的命令行开关导致退出。
- [ ] `pnpm build:e2e` 的产物能启动，显示一个空白窗口即可，单实例锁生效。这一项可以手动验证，结果写进汇报；能自动化更好。
- [ ] 在干净的 worktree 里，`cargo xtask ci pr --record docs/tasks/T074-desktop-main-skeleton/ci/` 通过，摘要随任务提交。
- [ ] 测试标题以用例编号开头（计划 1.6）。

## 交接事项

- i18n 已有 `command`、`update` 两个命名空间，菜单项的文案用 `command`；措辞日后按 Codex 的中文译文核对（T070）。
- TS 包的惯例（T070）：
  - `exports` 指向源码；
  - project references；
  - Vitest 只收 `src/**/*.test.ts`；
  - 包的 `test` 脚本必须跑 Vitest。
- 主进程要在调用时才去取 `dialog.showMessageBox` 这类函数，不要在模块加载时解构（`CLAUDE.md`）。

## 注意

- **依赖与安装脚本**：Electron 的安装要运行下载脚本，必须在 `pnpm-workspace.yaml` 的 `allowBuilds` 里放行 `electron`。只放行必需的包，并在汇报里列出。依赖取发布满约两周的版本；Electron 按设计取最新的补丁版本。
- **本机约束**：不修改全局环境；构建产物和缓存不进仓库。
- **规模**：上限约 1,500 行。明显会超出时，先停下来汇报。

## 参考资料

- 设计：`07-desktop.md` 5.7.2、5.7.4、5.7.8、5.7.9；`10-release.md` 5.10.6；`01-repo-build-ci.md` 5.1.2、5.1.3。
- ADR-03；计划 `docs/03-plan.md` T074 一行。
- R06 的 `docs/tasks/R06-codex-desktop-ui-firsthand/ui-spec.md` 第 2 节：窗口的默认尺寸与最小尺寸、交通灯位置。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为 `report.md`。不要自己创建 `report.md`。
