# T006 汇报

- 执行者：子 agent（前端工程师；T093 的执行者接着做）
- 分支：`ci/T006-license-gate`
- 完成日期：2026-10-26

**结论**：许可证门禁第一层做完，已接入 `xtask ci license`。当前 main 上通过，没有误报；两类负面样例都会失败。带记录的 `ci pr` 通过。`cargo deny` 拉公告库的网络失败也一并处理了：网络错误会重试、可以退回缓存，真的有公告照样失败。

## 做了什么

1. **metafile**（bed5807）：
   - `apps/desktop/vite.config.ts` 加了一个插件。每个环境打包后，把各输出含的模块写到 `dist/metafile/<环境>.json`，路径相对仓库根。
   - electron-builder 不打包这个目录，所以它不进 App。
2. **`tools/license-gate` 第一层**（21fd62f）：
   - **JS 入口**：主进程、preload、渲染进程。
     - 先从 metafile 取打进包的 npm 包和工作区包，再沿 `pnpm-lock.yaml` 的生产边扩展成完整闭包，即工作区包的 `dependencies`、快照的 `dependencies` 和 `optionalDependencies`。
     - 闭包里出现 `@anthropic-ai/*` 就失败，并给出依赖路径，例如 `apps/desktop → packages/ui-core → left-pad → @anthropic-ai/sdk`。被打包器摇掉、但锁文件里声明了的依赖也拦得住。
     - 每个打进包的 npm 包，其 `license` 必须在 `deny.toml` 的白名单里（支持 `OR`、`AND`）。
     - `lightningcss`、`argparse` 只在构建时用，不打进包，也不在生产闭包里，不会误报。
   - **运行时组件**：
     - 清单里每个随 App 分发的组件（带 `url` 的），都要在 `runtime/licenses/` 有许可证文本；门禁不认识的组件也算失败。
     - `bundle-claude-acp.mjs` 只能 import `node:` 内置模块。
   - **codex-acp 内联的依赖**：
     - 上游 v2.2.2 标签的 `package-lock.json` 放进 `runtime/locks/`。按它取非 dev、去掉 `@openai/codex` 的 23 个包，查禁用包、查许可证，并要求声明覆盖其中每个 `name@version`、不多不少。
     - `license-gate codex-notices` 负责生成声明：下载各包的 tarball，核对 integrity，取出 LICENSE，写成 `runtime/licenses/codex-acp-THIRD_PARTY_NOTICES.md`。它要联网，由维护者运行；门禁本身离线检查。
3. **接入 `xtask ci license`**（e3522c8）：依次是 `fetch-db`、`cargo-deny`、`install`、`metafile`（`vite build --mode release`）、`license-gate` 五步，都能在摘要里看到。
4. **`cargo deny` 的网络失败**（同一提交）：
   - 拉库单独成一步 `cargo deny --locked fetch db`，最多试 3 次；检查改为 `--offline`，本身不再联网。
   - 3 次都失败、但有早先拉下的缓存时，`fetch-db` 记为跳过，并写明用的是缓存；没有缓存就失败。
   - 有公告时检查照样失败。

## 关键取舍

- **以 metafile 为准**：`apps/desktop` 不声明 `dependencies`，所有依赖都靠打包，所以生产依赖只能从打包结果读。
- **两道检查范围不同**：禁用包查完整闭包，从严；许可证只查真正打进包、要分发的包，避免误报。
- **pnpm 12 的锁文件**：开头多一份包管理器自己的锁，解析时取最后一份文档。
- **重试之间不等待**：xtask 的 clippy 规则禁止 `thread::sleep`，拉取本身就要一两秒。
- **声明随 App 分发留给 T182**：要改 runtime-pack 的暂存清单，manifest 的文件哈希也会随之变化，放在 T182 的 THIRD_PARTY_NOTICES 里一起做更合适。

## 验证方式

- **UNIT-license-gate-negative**（4 个用例，通过）：
  - Anthropic 的 SDK 在闭包里，或直接打进包，都失败，并带依赖路径；
  - 只在构建时用的 lightningcss 不算；
  - `OR`、`AND` 表达式的判断；
  - 运行时组件缺许可证文本、清单里有门禁不认识的组件、codex-acp 的声明缺包或多出过时的包、内联包里有禁用包，都失败。
- **当前 main**：`cargo xtask ci license` 五步全过。主进程打进 2 个包、preload 0 个、渲染进程 137 个，许可证全在白名单里，没有误报。
- **拉库失败的处理**：
  - 用不通的代理模拟断网：`fetch-db` 试 3 次后标为跳过，其余步骤通过。
  - 把 `Cargo.lock` 临时退回 `rustls-webpki 0.103.15`：离线检查仍报 RUSTSEC-2026-0382、`advisories FAILED`。
- **带记录的 `ci pr`**（d26e0a8，摘要 `pr-2313a107c812.json`）：rust 2118、protocol 185、ts 477、license 5、testdata-scan 3、ui 35、e2e 30，全部通过。只跳过了 sensitive-words，因为本机没有词表。

## 规模

`cargo xtask size`：增 753、删 9，停止线 850，还剩 97。上游锁文件和生成的声明是数据，不计入。

| 部分 | 增 | 删 |
|---|---|---|
| license-gate 实现 | 465 | |
| license-gate 配置 | 40 | |
| license-gate 测试 | 103 | |
| Vite 插件 | 29 | 4 |
| xtask | 116 | 5 |

## 提交列表

```
d26e0a8 docs(tasks): record T006 PR CI summary
2313a10 docs(tasks): T006 notes, choices and actual size
e3522c8 ci(xtask): the license job runs the license gate, and fetches advisories apart
21fd62f feat(license-gate): the license gate's first layer
bed5807 build(desktop): write which modules each bundle holds
00d4ba0 docs(task): estimate T006
```

## 遗留问题 / 范围外发现

1. **T182**：`codex-acp-THIRD_PARTY_NOTICES.md` 还没进 runtime-pack 的暂存和 App，要在 THIRD_PARTY_NOTICES 生成时一并放进去；manifest 的 `files` 哈希会随之变化。
2. **codex-acp 升版本时**：先把新标签的 `package-lock.json` 放进 `runtime/locks/`，再运行 `node tools/license-gate/src/main.ts codex-notices`。否则门禁会因为找不到锁文件或声明不全而失败。
3. **T131（审查文档）**：加第二个渲染入口时，要在 `tools/license-gate/src/main.ts` 的 `ENTRIES` 里登记它的环境名。
