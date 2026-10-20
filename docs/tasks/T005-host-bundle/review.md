# T005 Review

## 第 1 轮（2026-10-20，编排者）

- 结论：通过，合并。
- 过程：执行者在原来的 T004 worktree 里开了分支，交回后又接着做 T094，所以编排者另开一个临时 worktree 合并 main（没有冲突），重跑 `cargo xtask ci pr --record`，全部通过：rust 1167、protocol 185、ts 314、license 1、testdata-scan 2，摘要 `ci/pr-6815e820f4d7.json`。新 worktree 冷编译，rust 作业约 210 秒，其中 IT-host-bundle-layout 冷编译约 25 秒。执行者那次的 `pr-0b854172025b.json` 一并保留。
- 核对：没有新依赖，只用系统的 `codesign`、`plutil`。产物和中间编译目录都在 `target/` 下，删除输出目录只限 `target/host-bundle/<kind>`。没有个人信息。
- 审阅：
  - **签名顺序**：先核对 Node 的 cdhash，组装后按清单逐个核对运行时文件（多、少、不同都失败；路径含 `anthropic`、`claude-agent-sdk` 时失败，守 ADR-11），签宿主取 cdhash，再核对一次 Node 没被重签，最后编译并签 CLI。符合 5.10.2 和 T045 的裁定。
  - **独立的 target 目录**：用例里按自用构建的特性编译宿主，会换掉宿主集成测试经 `CARGO_BIN_EXE_cupper-host` 使用的 `target/debug/cupper-host`，所以编译到 `target/host-bundle/build/`。理由成立，代价是每个 worktree 多约 1.2 GB。
  - **离线用例**：假运行时用复制后 ad-hoc 签名的 `/usr/bin/true` 充当 Node，配一份对应的清单，不依赖下载缓存。
  - **构建种类表与设计对照**：用例直接读设计 5.7.9 的两行与代码比对。设计一改用例就失败，这是有意的；改 5.7.9 的身份时要同步 `KINDS`。
  - **esbuild-wasm 不精简**：用不到的文件约 440 KB，只占 3%，整包保留便于与 SRI 校验过的 npm 包一一对应。接受。
- 编排者裁定：开发构建关闭 hardened runtime（与 e2e 相同），已补进设计 5.7.9 的"签名"一行。
- 规模：增 553 行、删 61 行（`sign.rs` 移进共用的 `codesign.rs`），净增约 490 行，其中测试 152 行，在约 600 行的停止线以内。
- 转交事项已写进 `handoff-notes.md`：组装 `Cupper.app` 的任务与 T161（外层 Info.plist 的用途说明断言）、T075 与 T180（自用构建的 CLI 必须与同一次产出的 App 配套；发布身份重签后清单要重算）。codex-acp 内联依赖的许可证声明已在 T006 的交接里。
