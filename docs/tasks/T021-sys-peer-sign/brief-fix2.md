# T021 修复第 2 轮：并发拉起导致的 fd 副本，全面排查

- 派发日期：2026-10-19
- 执行者：子 agent（Rust 工程师，系统接口）
- 分支：`fix/T021-fd-copy-audit`
- 依赖：T021 与第 1 轮修复（`fix/T021-flaky-peer-test`，均已合并）

## 背景

第 1 轮修复查明了一个根因：任何 `posix_spawn`，包括带 `CLOEXEC_DEFAULT` 的 `cupper_sys::spawn`，都会在内核里先复制父进程的整张 fd 表，再关掉多余的。所以别的测试正在拉子进程时，刚关掉的 fd 可能还被短暂持有。当时把两个用例改成用 `testutil::alone` 单独进程运行，详见 `notes.md` 和 `review.md` 第 2 轮。

合并 T014 之后，main 上的 `cargo xtask ci pr` 又出现一次失败：
- 用例：`peer::tests::it_sys_peer_current_exited_peer_is_refused`（`crates/cupper-sys/src/peer.rs` 约第 600 行）；
- 现象：`drop(client)` 之后，期望 `(PeerExited, ENOTCONN=57)`，实际得到 `(PeerExited, EINVAL=22)`。也就是说，另一端仍被某个进程持有，和第 1 轮是同一个根因；
- 立即重跑，通过。

第 1 轮只修了当时失败的两个用例。本轮要把同一类断言**全部找出来**一次修好，不再一个个等它们偶发失败。

## 范围

**做**：
1. **排查 `cupper-sys` 的全部测试**：凡是断言"关掉或丢掉某个 fd 之后，对端立即看到已关闭"的，包括 `ENOTCONN`、EOF、`writers_gone`、"无人持有"之类，都要处理。可以：
   - 改用 `testutil::alone` 单独进程运行；
   - 或者改成不依赖"此刻无人持有"的写法，前提是断言的语义不变。
2. **排查其他 crate 的测试**，只看不改：`cupper-host`、`cupper-fake-agent`、`xtask`。找出同类断言，在汇报里列出位置和风险判断，由编排者决定是否另开任务。如果有明显会失败的，同样处理，并在汇报里说明理由。
3. 先复现，再修：仿照第 1 轮的做法，并发拉起子进程的情况下复现这个 `EINVAL`，用来确认根因。修复后再用同样的负载确认已经消失。

**不做**：
- 改产品代码路径；
- 改第 1 轮已经处理过的两个用例，除非排查发现它们还有问题。

## 验收标准

- [ ] 复现：说明复现的办法，以及复现前后的失败次数。
- [ ] 用 CPU 负载跑稳定性检查，做法同第 1 轮：
  - `cargo test -p cupper-sys --lib` 连跑 50 次；
  - `cargo test --workspace --all-features` 连跑 30 次；
  - 两项都要全部通过。
- [ ] 每处改动都有理由，写进 `notes.md`；同类断言的排查清单也写在那里，包括 `cupper-sys` 内外的位置、结论和处理方式。
- [ ] 在干净的 worktree 里，`cargo xtask ci pr --record docs/tasks/T021-sys-peer-sign/ci/` 通过，摘要随任务提交。
- [ ] 结束后没有残留的负载进程和临时目录。

## 参考资料

- `docs/tasks/T021-sys-peer-sign/`：`notes.md`（SCM_RIGHTS、`fd_lock` 与 `alone` 两节）、`review.md`（第 2 轮）、`report.md`（修复轮汇报）。
- `crates/cupper-sys/src/testutil.rs`（`alone`、`fd_lock`）。

## 规模

S，预计 200 行以内。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档。不要自己创建 `report.md`。`notes.md` 由执行者在任务分支上提交。
