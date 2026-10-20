# T115 评审

- 评审人：编排者
- 日期：2026-10-20
- 结论：**通过，合并**

## 核对

- **规模**：新增 1,189 行、删除 265 行（不含 `Cargo.lock`），在第 2 次裁定放宽的约 1,250 行之内。
- **CI**：执行者的摘要 `pr-4b831744dea8.json` 通过。合并 main 之后在本 worktree 重跑 `cargo xtask ci pr --record`，`pr-cc385902a538.json` 通过（rust 1749、protocol 185、ts 353、license、testdata-scan）。
- **合并冲突**（编排者解决）：
  - `durations.rs`：两边的条目都保留：本分支的运行时安装器三项，main 上 T008 的适配器生命周期各项。
  - `launch.rs`：保留 main 上 T009 的 `Cli`；按本分支的改法去掉 `verify` 和 `Mismatch`（统一用 `runtime::verify`）。
  - T009 的 `status.rs` 原来按 `PathBuf` 判断运行时文件在不在，改为读 `runtime::Installed` 的 `path`；测试辅助相应改为造 `Installed`。

## 逐项意见

1. **修正 T055 的错误**：拉起改为 `runtime/node/<版本>/node` 加 `runtime/claude-agent-acp/<版本>/adapter.mjs`，按 `bundle_sha256` 核对；删掉 `CLAUDE_BUNDLE`。核对函数只留 `runtime::verify` 一份。好。
2. **信任锚**：
   - 锁文件和打包脚本从包内读取，经内嵌清单核对哈希；
   - 每个 tarball 先核 SRI（只认 sha-512）再解包；
   - 打包产物核 `bundle_sha256` 后才提交。
   - 链条完整：网络上来的字节都要对上包内清单推出的哈希。
3. **锁文件过滤**：
   - 跳过 optional、dev、devOptional、link，含 Claude Code 二进制的 8 个平台包永远不下载（ADR-11）；
   - 安装路径必须在 `node_modules/` 下、只有普通路径段；
   - `resolved` 必须指向 npm 官方仓库。
4. **解包**：
   - 只写普通文件，不写符号链接和硬链接；
   - 去掉顶层目录后只允许普通路径段；
   - 目录建成 0700；
   - 不运行包脚本。
5. **HTTP**：
   - 不读本进程和系统代理，只用登录 shell 的 `HTTPS_PROXY`；
   - 不跟随重定向；
   - User-Agent 不带用户信息；
   - 限时经 `Clock`。
   - 符合 5.2.12 和 SEC-6。
6. **打包环境只有 `PATH`**，并用变异检查（多加 `HOME` 就失败）证明假 Node 的环境核对有效。好。

## 小问题（不阻塞）

- `download` 用 `response.bytes()` 把整个响应读进内存，没有大小上限；SRI 只在读完之后核对。仓库或代理恶意返回超大响应时，在 60 秒限时内可能占用大量内存。npm 包的大小都已知，以后可以按锁文件里的包或一个固定上限（例如 64 MB）截断。交给新增的 T129 一并处理。

## 交接

- 遗留第 1 条（接进宿主启动）原来没有归属任务。计划 v1.3.23 新增 T129（依赖 T059、T115），T099 改为也依赖它。接线方式和大小上限写进 `handoff-notes.md` 的 T129 一行。
- 遗留第 3 条（`cargo clean` 之后要先构建假 agent）记进交接。
