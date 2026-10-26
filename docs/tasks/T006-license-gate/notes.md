# T006 工作笔记

- 执行者：子 agent（前端工程师；T093 的执行者接着做）

## 开工前核对现状

- **桌面 App 的 JS**：
  - Vite 8（Rolldown）把主进程、preload、渲染进程各打成一个环境，`apps/desktop` 不声明任何 `dependencies`，全靠打包。所以"生产依赖"不能从 `package.json` 读，只能看打包结果里有哪些模块。
  - 实测 `vite build --mode release`：主进程打进 `i18next`、`zod`；preload 只有 `@cupper/shell-api`；渲染进程打进 137 个包（React、Radix、streamdown 一族等），另有 Vite、Rolldown 注入的虚拟模块（`\0vite/modulepreload-polyfill.js` 等）。
- **运行时组件**（`runtime/manifest.json`）：
  - 随 App 带的：Node、codex-acp 单文件、esbuild-wasm，三者的许可证文本已在 `runtime/licenses/`；以及 `bundle-claude-acp.mjs` 和 claude-agent-acp 的锁文件。
  - claude-agent-acp 本身在用户机器上安装和打包，不随 App。它的锁文件里有 `@anthropic-ai/claude-agent-sdk`，但那是元数据，ADR-11 允许。
- **codex-acp 内联的依赖**：
  - 上游 v2.2.2 标签（`202e66e`）的 `package-lock.json` 里，非 dev 的包去掉 `@openai/codex` 和它的 6 个平台包，还剩 23 个（含嵌套的 `powershell-utils@0.1.0`）。
  - 锁文件里每个包都带 `license` 字段；许可证全文要从各包的 npm tarball 取。
- **`cargo deny`**：只有一步 `cargo deny --locked check`，拉 advisory 数据库失败时整个作业失败，和"真的有公告"分不开。

## 做法

1. **metafile**：`apps/desktop/vite.config.ts` 加一个插件。每个环境打包完，把每个输出用到的模块写到 `dist/metafile/<环境>.json`，路径相对仓库根。这个目录不在 electron-builder 的 `files` 里，不进 App。
2. **闭包**：
   - **打进包的包**：metafile 里 `node_modules/.pnpm/<目录>/node_modules/<名字>/` 之下的模块所属的包（名字和版本），加上工作区包（`packages/*` 等）。
   - **完整闭包**：在上一步的基础上，按 `pnpm-lock.yaml` 的生产边扩展，即工作区包的 `dependencies`、npm 包快照里的 `dependencies` 和 `optionalDependencies`。根包 `apps/desktop` 是打包的根，它的 `devDependencies` 只在显示依赖路径时用，不算进闭包。
3. **检查**：
   - **禁用包**：完整闭包里出现 `@anthropic-ai/*` 就失败。依赖路径按锁文件的边，从入口的根包找到它。这样连打进包后又被摇掉的依赖也拦得住。
   - **npm 包的许可证**：只查真正打进包的包，读它安装目录里 `package.json` 的 `license`，必须在 `deny.toml` 的白名单里，支持 `OR`、`AND`。只在开发时用的包不打进包，所以 `lightningcss`、`argparse` 不会误报。
   - **runtime 脚本**：`bundle-claude-acp.mjs` 运行时只用 esbuild-wasm。codex-acp 的闭包就是上述 23 个内联包，同样查禁用包和许可证。
   - **运行时组件**：清单里每个带 `url` 的组件，都要在 `runtime/licenses/` 有对应的许可证文本。codex-acp 另外要有内联依赖的声明，覆盖上游锁文件里那 23 个包、版本一致。清单里出现门禁不认识的组件也失败。
4. **codex-acp 的声明**：
   - 上游锁文件复制到 `runtime/locks/codex-acp-2.2.2.package-lock.json`。
   - `license-gate codex-notices` 按它下载各包的 tarball，核对 integrity，取出 LICENSE，写成 `runtime/licenses/codex-acp-THIRD_PARTY_NOTICES.md`。这一步要联网，由维护者运行；门禁本身离线检查。
   - 声明随 App 分发属于 THIRD_PARTY_NOTICES 的打包，留给 T182。
5. **接入 `xtask ci license`**：`cargo-deny` 之后依次是 `install`、`metafile`（`vite build --mode release`）、`license-gate`。
6. **`cargo deny` 的网络失败**：
   - 先单独 `cargo deny fetch`，失败时最多重试 3 次；
   - 再 `cargo deny check --disable-fetch`，检查本身不再联网，公告照样会让它失败；
   - 3 次都拉不到、但本机有缓存时，用缓存检查，并在摘要的跳过步骤里写明"用的是缓存的公告库"；
   - 没有缓存就失败。

## 开工前的规模估计

| 模块 | 内容 | 草估 |
|---|---|---|
| `lockfile.ts` | 解析 `pnpm-lock.yaml` 的 importers 和 snapshots，得到边 | 50 |
| `bundle.ts` | 读 metafile，把模块映射到包和工作区包 | 50 |
| `gate.ts` | 闭包、禁用包和依赖路径、许可证白名单 | 90 |
| `runtime.ts` | 运行时组件的许可证、codex-acp 内联集合、声明的完整性 | 70 |
| `notices.ts` | 生成 codex-acp 的声明（下载、校验、取 LICENSE） | 60 |
| `main.ts` 与配置 | 命令行、`package.json`、`tsconfig` 等 | 80 |
| Vite 的 metafile 插件 | | 25 |
| `ci/jobs/license.rs` | 新步骤；`cargo deny` 的拉取重试与缓存 | 60 |
| **实现小计** | | **485** |
| 测试 | UNIT-license-gate-negative（禁用包带路径、运行时组件缺许可证），以及闭包、白名单、声明不全 | 170 |
| **合计** | | 约 **655** |

数据文件（上游锁文件、生成的声明）和 lock 文件不计入。在停止线 850 之内。
