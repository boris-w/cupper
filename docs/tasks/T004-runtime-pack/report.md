# T004 汇报

- 执行者：子 agent（工程师，构建与发布工具）
- 分支：`build/T004-runtime-pack`
- 完成日期：2026-10-19

## 做了什么

1. **`cargo xtask runtime-pack`**（`crates/xtask/src/runtime_pack/`）
   - **下载**：Node 24.22.0（darwin-arm64 .tar.gz）、codex-acp 2.2.2、esbuild-wasm 0.28.2。
     - 用系统 curl 下载：`-q` 不读 `.curlrc`，只走 https，User-Agent 为 `cupper-research`。
     - 地址只允许 `https://nodejs.org/dist/` 和 `https://registry.npmjs.org/`。
     - 下载后按清单锁定的 sha-256 或 SRI 校验。缓存在 `target/runtime-pack/downloads/`，每次复用前重新核对。
   - **解包**：用 `tar` + `flate2`。只取普通文件，跳过链接；路径越界（`..`、根路径）时整个命令失败。
   - **重签 Node**：只对缓存里的副本做 ad-hoc 签名，hardened runtime，只有 `allow-jit`，identifier 为 `dev.cupper.runtime.node`。之后 `--verify --strict`，并取出 cdhash。
   - **claude-agent-acp 打包产物的哈希**：
     - 按锁文件逐包校验 SRI，下载到 `target/` 下的隔离目录（含 SDK）。8 个 optional 平台包（含 Claude Code 二进制）从不下载。
     - 用重签后的 Node 和 esbuild-wasm 跑打包脚本，算出哈希、扫描环境变量，然后删除整个目录。删除由 Drop 守卫保证；上次被杀留下的目录，下次开始时先删。
     - 算出的哈希 `a8f29655…` 与 SP-2 三天前在另一个目录下算出的值逐字节相同。
   - **暂存目录**：运行时按 `CupperHost.app/Contents` 的布局放在 `target/runtime-pack/Contents/`（`Helpers/node`、`Resources/runtime/…`，与 5.10.1 一致），供 T005 直接使用。
   - **写回**：清单的 `files` 字段记录每个暂存文件的 sha-256，Node 另记 cdhash；`claude_agent_acp.bundle_sha256`；`env.reads`。三个组件的许可证文本写入 `runtime/licenses/`。
   - **`--verify`**：什么都不改，逐项比对清单和许可证文本，有差异就失败，并列出每一处差异。
   - 全程约 20 秒。
2. **环境变量扫描**：
   - 扫描范围：claude-agent-acp 的打包产物，以及 codex-acp 的 `index.mjs`。
   - 提取规则：名为 `env` 或以 `Env` 结尾的对象上的成员读取。
   - 按清单里的 `allow`、`deny`、`harmless` 三类归类；有未归类的名字时命令失败，并列出名字和读取它的适配器。
3. **入库的内容**：
   - `runtime/manifest.json`；
   - `runtime/locks/claude-agent-acp-0.91.0.package-lock.json`（SP-2 的锁文件，根名改为中性名）；
   - `runtime/bundle-claude-acp.mjs`、`runtime/node.entitlements.plist`；
   - `runtime/licenses/`（Node、codex-acp、esbuild-wasm 的许可证原文）。
   - `.prettierignore` 加入了清单、锁文件和许可证文本。
4. **UNIT-runtime-pack-envscan**：共 4 条测试，都离线运行：
   - 提取各种读取写法；
   - 未归类的名字（含前缀不覆盖的情况）让检查失败；
   - 命令在写入任何文件之前就失败；
   - 入库的清单是规范格式，并且全部读取都已归类。
   - 另有下载校验、解包越界、锁文件解析、`--verify` 差异报告的测试。

## 关键取舍

- **用 curl，不引入 HTTP 库**：走 rustls 的 HTTP 库要带 `webpki-roots`，它的许可证是 CDLA-Permissive-2.0，不在 ADR-11 的白名单里；macOS 自带 curl。实测遇到一次 `SSL_ERROR_SYSCALL`，加了 `--retry-all-errors`。哈希不符不会重试。
- **Node 改用 .tar.gz**：新的 sha-256 `6a9724d9…` 取自 `SHASUMS256.txt`。我已用 Node 官方的 release-keys 公钥环、`gpgv` 核对过签名（Good signature）；同一文件里 .tar.xz 的哈希与 SP-2 的值一致。这样解包与 T056 用同一套 crate，不需要 xz。
- **ad-hoc 签名可复现**：实测同一文件两次签出的 sha-256 相同。所以清单里记的是重签后 Node 的 sha-256 和 cdhash，`--verify` 重算后比对。
- **esbuild-wasm 以 `node` 这个名字从 PATH 拉起服务进程**：第一次运行时报 `spawn node ENOENT`。现在打包时只给 `PATH=<Helpers 目录>`，其余环境变量全部清空。打包脚本用仓库里那一份，不在任何下载目录里执行解释器。
- **扫描规则做了收窄**：`"X" in env` 和解构两种写法在两个产物里都没有出现，为控制规模删掉了，模块文档里写明没有覆盖。不是 `process.env` 的对象，只收看起来像变量名的读取，排除了 `schemaEnv.root`、`env.name` 这类误报。
- **范围**：codex-acp 内联依赖的许可证声明按设计 5.1.2 属于 license-gate，本任务没有做（见交接）。`runtime-pack` 要联网，不进 `pr` 组合作业；5.1.6 把 `--verify` 放在每晚的 `release-config`（T161）。
- **规模**：代码 1,272 行（实现约 957，测试约 315），比估计多 11%。清单里手工维护的数据约 170 行，代码加数据约 1,440 行，在停止线以内。清单其余约 220 行是写回的计算字段。逐模块统计见 `notes.md`。

## 验证方式

- [x] `cargo xtask runtime-pack --verify` 通过（输出 "the runtime matches runtime/manifest.json"）。另做了反例：从分类里删掉 `CODEX_ACP_*` 和 `XDG_RUNTIME_DIR` 后，命令以退出码 1 失败，并列出 3 个名字。
- [x] UNIT-runtime-pack-envscan 新增，4 条测试全部通过；在 pr 摘要里结果为 pass。
- [x] 入库内容只有清单、锁文件、脚本、entitlements 和许可证文本。
  - grep 过分支的 diff 和提交信息，没有本机路径、用户名或邮箱。
  - `target/` 和临时目录里的 SDK 副本已全部删除。
- [x] 在干净的 worktree 里运行 `cargo xtask ci pr --record docs/tasks/T004-runtime-pack/ci/`，全部通过：
  - rust 1084 / protocol 176 / ts 291 / license 1 / testdata-scan 2；
  - ui、e2e 尚未实现；
  - 摘要为 `pr-69eb986122d1.json`，已提交。
  - `cargo deny` 对新增的 crate 通过：tar、flate2、miniz_oxide、simd-adler32、crc32fast、adler2、filetime。
- [x] 测试名以用例编号开头：`unit_runtime_pack_envscan_*`。辅助测试不带编号，与 T001 的做法相同。

## 提交列表

```
b3ce2a0 docs(task): record the T004 pr CI summary
69eb986 docs(task): record T004 sizes, trade-offs and env scan findings
bb81877 build(xtask): scan the adapters for the environment variables they read
fc0535c build(xtask): add `cargo xtask runtime-pack` and the runtime manifest
e4bc36f build(runtime): add the bundle script, Node entitlements and the claude-agent-acp lockfile
409f586 docs(task): estimate T004 by module before starting
```

## 遗留问题 / 范围外发现

1. **设计 5.6.6 的禁用清单需要补 15 项**（本任务已在清单的 `deny` 里加上，理由按组写在 `notes.md`）：
   - `CODEX_ACP_*`：codex-acp 自己的崩溃重启开关，与 `CLAUDE_AGENT_ACP_*` 同理；
   - `NO_BROWSER`、`SSH_CLIENT`、`SSH_CONNECTION`、`SSH_TTY`、`CLAUDE_CODE_REMOTE`：两个适配器据此决定提供哪些登录方式；
   - `CLAUDE_CODE_REMOTE_SESSION_ID`、`CLAUDE_CODE_SESSION_KIND`：CLI 的远程会话、后台会话模式；
   - `CLAUDE_CODE_DEBUG_LOGS_DIR`、`CLAUDE_CODE_DEBUG_LOG_LEVEL`：调试日志的位置和级别；
   - `CLAUDE_SECURESTORAGE_CONFIG_DIR`、`CLAUDE_CODE_PROJECT_DIR_NAME`：凭据存储配置和会话存放目录；
   - `CLAUDE_CODE_CUSTOM_OAUTH_URL`、`CLAUDE_CODE_OAUTH_CLIENT_ID`、`CLAUDE_LOCAL_OAUTH_*`：OAuth 端点和客户端的覆盖。

   请编排者确认归类，并改设计。建议宿主的放行过滤直接取清单的 `deny`。
2. **静态扫描对 claude 侧只能部分覆盖**：SDK 0.3.300 自带一张环境变量登记表（8 个对象，约 1,160 个名字），经 `process.env[name]` 动态读取，扫描只看到直接写死的约 100 个读取点。需要决定：
   - 接受这个局限：宿主是白名单制，禁用清单本来就是纵深防御；
   - 或者按产物特征提取登记表；
   - 或者对 `CLAUDE_CODE_*` 等前缀整体禁用，只放行逐个列出的名字。

   建议 M1 接受局限，在 T066 升级适配器时再评估。
3. **5.10.7 第 2 步的 GPG 核对没有做进 xtask**：Node 的哈希锁定在入库的清单里，我这次手工核对过 GPG。若要自动化，需要 `gpgv` 和锁定的 Node 发布公钥环，可以放进 T161 或 T180。
4. **清单里 Node 的 sha-256 和 cdhash 是 ad-hoc 签名的值**：用 Developer ID 签名后两者都会变。发布流水线（T180）要按发布身份重新生成并内嵌清单；设计里应写明每种构建内嵌哪一份清单。
5. **T056 要注意**：拉起打包脚本时，PATH 只放随包 Node 所在的目录，否则 esbuild-wasm 会找不到 node，或者用到别的 node。
6. **T006 / license-gate 要注意**：codex-acp 内联依赖的声明，应按上游 v2.2.2 标签的 `package-lock.json` 生成。SP-2 的锁文件是自己按版本范围解析出来的，与上游实际内联的版本不同（例如 `@agentclientprotocol/sdk` 1.5.0 对 1.8.0、`zod` 4.6.5 对 4.6.7，明细见 `notes.md`）。
7. 清单里还没有"已验证 CLI 范围"。按 5.10.3，它由兼容报告生成，归 T064、T066。
8. esbuild-wasm 整包随附，包括用不到的浏览器版文件，每个文件都记了哈希。如果要精简，由 T005 决定。
