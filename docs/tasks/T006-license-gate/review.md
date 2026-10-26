# T006 评审

- 评审人：编排者
- 日期：2026-10-26
- 结论：**通过，合并**

## 核对

- **规模**：`cargo xtask size` 报 +753 −9，停止线 850；上游锁文件和生成的声明是数据，不计入。
- **CI**：执行者的摘要 `pr-2313a107c812.json` 通过（rust 2118、protocol 185、ts 477、license 5、testdata-scan 3、ui 35、e2e 30）。之后 main 进了 T078 与 T186（T186 改了 xtask 的 `rust` 作业，加了 `git-lfs` 步骤），编排者合进分支没有冲突，`clippy -p xtask`、`cargo test -p xtask`（44 个）、`cargo xtask ci license`（五步）都通过后合并。
- **验收**：UNIT-license-gate-negative 4 个用例通过；当前 main 上门禁通过、没有误报（主进程 2 个包、preload 0 个、渲染进程 137 个）。

## 逐项意见

1. **以 metafile 为准**：`apps/desktop` 不声明 `dependencies`、全靠打包，生产依赖只能从打包结果读。Vite 插件把每个环境打进包的模块写到 `dist/metafile/`，electron-builder 不打包这个目录，不进 App。
2. **两道检查范围不同**：禁用包查完整生产闭包（metafile 加 `pnpm-lock.yaml` 的生产边），从严，连被摇掉的声明依赖也拦得住；许可证只查真正打进包要分发的包，避免 `lightningcss`、`argparse` 的误报。出现 `@anthropic-ai/*` 时给出依赖路径。
3. **运行时组件**：清单里每个随 App 分发的组件都要有许可证文本，门禁不认识的组件也算失败；`bundle-claude-acp.mjs` 只能 import `node:` 内置模块。
4. **codex-acp 内联依赖**：上游 v2.2.2 的 `package-lock.json` 入库，声明要不多不少覆盖 23 个非 dev 包；生成声明的 `codex-notices` 要联网、由维护者运行，门禁本身离线检查。
5. **`cargo deny` 的网络失败**：拉库单独成一步、最多试 3 次，检查改为 `--offline`；3 次都失败但有缓存时记为跳过并写明用的是缓存。执行者用不通的代理模拟断网、又临时退回 `rustls-webpki 0.103.15` 验证真的公告照样失败（RUSTSEC-2026-0382）。区分了"拉取失败"与"有公告"，这正是任务书要求的。

## 记下的点

- **T182**：`codex-acp-THIRD_PARTY_NOTICES.md` 还没进 runtime-pack 的暂存与 App，manifest 的 `files` 哈希会随之变化。记交接。
- **codex-acp 升版本时**：先把新标签的 `package-lock.json` 放进 `runtime/locks/`，再跑 `node tools/license-gate/src/main.ts codex-notices`。记交接。
- **T131（审查文档）**：加第二个渲染入口时要在 `tools/license-gate/src/main.ts` 的 `ENTRIES` 里登记。记交接。
