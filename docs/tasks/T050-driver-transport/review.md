# T050 Review

## 第 1 轮（2026-10-19，编排者）

- 结论：通过，补改后合并
- 核对：
  - 在任务 worktree 里重跑 `cargo xtask ci pr`，rust、ts 通过；
  - license 作业失败：SDK 引入的 `foldhash` 采用 Zlib 许可证，不在白名单里。执行者按规定没有自己改白名单，交由编排者决定。
- 裁定：白名单加入 `Zlib`（OSI 认证的宽松许可证），ADR-11 已补记。
- 接受的取舍：
  - K28 不照字面调用 `detach()`，改为不对外暴露 `SentRequest`、在请求发出时就把回调交给 SDK，效果相同。用例证实这样不会发出 `$/cancel_request`。
  - K19 由 SDK 判断取值是否已知，原值按字段名放在 `_meta.cupper.original`。
  - 凭据只在词首匹配。
  - 用两个"金丝雀"用例锁定 SDK 3.3.0 在 K27、K28 上的行为，日后升级 SDK 时，如果这两处被修掉，用例会失败提醒。
  - `agent-client-protocol-derive` 锁在 `=3.3.0`。
- 规模：约 1,490 行，在上限以内。
- 转交（记入 `docs/tasks/handoff-notes.md`）：T053 的 compaction 能力声明和 `k19::original`；T055 的邮箱顺序和关闭顺序；T028 的凭据规则；T025、T047 的 `Dir`。

## 补改（2026-10-19）

- 在 `deny.toml` 中加入 `Zlib` 并附注释（`fecf181`）。重跑 CI，三个作业全部通过，最终摘要为 `ci/pr-fecf18126ede.json`。合并前编排者又重跑了一次。
