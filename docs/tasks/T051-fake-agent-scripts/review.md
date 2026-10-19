# T051 Review

## 第 1 轮（2026-10-19，编排者）

- 结论：通过，合并。
- 过程：
  - 规模先后两次超出，执行者都按守则第 8 条停下来汇报，两次裁定见 `brief.md` 末尾。
  - 编排者把 main 合入分支（`2ca7b6a`）。`Cargo.lock` 取 main 的版本；SDK 派生宏由分支上的 3.4.0 统一回 T050 锁定的 3.3.0。
  - 执行者在合并后的分支上跑 `cargo xtask ci pr --record`，结果 rust 444、ts 64、license 1，全部通过，摘要为 `ci/pr-7b13b1c4cfa8.json`。
- 核对：
  - 没有 `unsafe`，没有个人信息，提交信息里没有会话链接；
  - persona 里录制带来的内容（模型描述、账号、组织名）已换成通用文字和 `example.invalid` 占位；
  - 计时只经 `real_sleep`、`real_interval` 两个辅助函数，都带写明理由的 `#[expect(clippy::disallowed_methods)]`，`flood_rate` 用例另有一处同样的豁免；
  - 错误类型已按要求改为 thiserror（`ConfigError`、`ServeError`），`anyhow` 只在 `main.rs`。
- 代码审阅：
  - `write_file` 只接受由普通分量组成的相对路径。它按最深的已存在祖先的真实路径判断是否仍在工作区内，目标本身是指向外面的符号链接也会被拒绝。对测试工具来说足够。
  - 原始行装进标记通知，走 SDK 的发送队列，写出时再换回原样。这样保证了它与前后消息的顺序，做法可取。
  - 请求不走 SDK 的类型化处理器，响应直接写 JSON，以保留录制里的原样形态；参数仍经 SDK 校验。接受。
  - 全局不变式：在途标记在 prompt 的响应之前清除，避免宿主紧接着发下一个 prompt 时被误判。退出码 70。
  - `spawn_child` 用 `std::process` 拉 `/bin/sleep`，模拟活得比适配器久、需要宿主按进程组回收的 CLI 子进程。`CLAUDE.md` 里"经 `cupper-sys::spawn` 拉子进程"的约束只针对宿主、CLI 和原生扩展，假 agent 不在其内。接受。
- 规模：最终 3,050 行，其中 persona JSON 404 行（prettier 排版，属于数据）、实现 1,704 行、测试 922 行。接受。
- 转交事项已写进 `docs/tasks/handoff-notes.md`：
  - T052：用 `--segment-file` 回放多实例录制；persona 与录制的自动比对（jq 命令在 `notes.md` 里）；
  - T055、T062、T090：宿主测试怎样拿到假 agent 的路径，以及只能经命令行参数配置它。
