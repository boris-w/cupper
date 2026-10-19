# T037 Review

## 第 1 轮（2026-10-19，编排者）

- 结论：通过，合并。
- 过程：
  - 分支开出之后，main 合进了 T087、T072、T019、T043，有 4 个文件冲突，都保留了两边：
    - `Cargo.toml`：tokio 的特性取并集（T043 的 `macros`，本分支的 `rt-multi-thread`、`signal`）；
    - `gateway/conn.rs`：只有导入冲突。T019 把写循环挪到了 `send.rs`，本分支没有改写循环，只加了 `closing` 和 `Live`；
    - `gateway/mod.rs`：模块文档合并；
    - `gateway/dispatch/tests.rs`：夹具的方法表同时登记 T019 的 `routes.streams` 和本分支的 `Control::route`，`Peer` 用 T043 挪到 testkit 的那份；UNIT-unsupported-set 的允许表里 `history/page` 和 `host/exit` 两行都删掉。
  - 合并后在 worktree 里重跑 `cargo xtask ci pr --record`，全部通过：rust 1154、protocol 185、ts 314、license 1、testdata-scan 2，摘要是 `ci/pr-9f9e311a4020.json`。执行者合并前那次的 `pr-90f6c710a21a.json` 一并保留。
  - 执行者从 main 拣了 `6d214fd`（`tests/ids.toml` 的 T037、T059 拆分），改动与 main 相同，合并时没有冲突。
- 核对：
  - 新增依赖 `signal-hook-registry`、`errno`（tokio 的 `signal` 特性带进来），都是 MIT OR Apache-2.0，license 作业通过；
  - `unsafe` 只在 `cupper-sys/src/runloop.rs`，7 处都有 `SAFETY:` 注释，`cupper-host` 里没有；
  - 没有个人信息。
- 审阅：
  - **退出顺序**：停 accept → 广播 `host/shutdown` → 各部件 `stop(force)` → 清理任务和账本清理任务 → `last_clean_exit_at` → 原始帧写线程和写线程 → 关连接（先发完队列，最多等关闭宽限）→ 删套接字、放锁。清理任务比任务书提前一步停，保证 `last_clean_exit_at` 是写线程停下前的最后一个写入，接受。
  - **关连接与 T019 的写循环**：读循环收到 `closing` 后以"先发完回复"结束，T019 的 `send::write_loop` 在关闭宽限内把队列发完，两边接得上。
  - **写线程死亡**：宿主以退出码 1 有序退出，不写 `last_clean_exit_at`，下次启动按非正常退出处理。
  - **改了 T023 的用例**：宿主组装好以后会一直运行，所以 `unit_release_features_host_does_not_read_stdin` 改为等它监听、核对 stderr 为空、SIGTERM 后退出码为 0，断言的意图（发布构建不读 stdin）没有变。
- 范围外发现：T058 性质测试的反例由编排者在 T043 分支上复现，见 T043 的 review；修复任务和 `proptest-regressions/` 的处理记入交接。
- 接线的归属：本任务和 T043 都把"把会话管理器接进宿主"留给了对方。编排者指定如下，已写进 `handoff-notes.md`：
  - T059：`routes.streams(..)`，删掉 `streams` 上的 `#[expect(dead_code)]`；
  - T055：用 `Routes::sessions` 登记 `SessionManager`，它作为第一个 `Part`。`SessionDeps` 里的 `agents` 由 T055 提供；
  - T034：用 `Routes::index` 登记索引端口，作为第三个 `Part`。
- 规模：新增 1,335 行（`crates/`、`tests/`），删除 53 行，在 1,500 上限以内。
