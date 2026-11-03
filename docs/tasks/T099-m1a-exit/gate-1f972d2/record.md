# `gate/m1a`：候选 1f972d2 的核对记录

- 日期：2026-11-03
- 记录人：编排者
- 结论：**通过，在 1f972d2 上打 `gate/m1a`**

## 依据

- M1a 退出由 T099 验收（候选 ec459fd），M1a 同步会（`docs/meetings/2026-11-02-m1a-sync/`）判定通过：PERF 只记录（D1），IT-sys-peer-current 不适用（D2），MAN-keyboard 不重做（D3），真实兼容到 M1b 再跑（D4），K4 通过（D5）。
- 标签要打在一个 `ci` 和 nightly 都通过、`trace-ids verify --milestone M1a` 通过的 main 提交上（T219 的评审）。

## 候选的经过

| 候选 | 结果 | 处理 |
|---|---|---|
| 5da6f1c（T219 合并后） | `ci` 通过；nightly 的 e2e 里 PERF-bridge-gate 压力用例打不开会话，`perf`、`differential` 被 fail-fast 取消 | 不打；开 T220 |
| T220 合并后 | main 上带着 T111 加的偶发失败用例（重启用例，T220、T221、T143 的 PR 都撞上过） | 不当候选；开 T224 |
| **1f972d2**（T224 合并后） | 通过 | 打标签 |

按计划 1.8，已知的偶发失败不靠重跑算作通过；两次都是修好之后换候选。

## 1f972d2 上的运行

- `ci`（push）：https://github.com/boris-w/cupper/actions/runs/41597322966 ，8 个作业全部通过。
- nightly（手动触发）：https://github.com/boris-w/cupper/actions/runs/41597345134 ，7 个作业全部通过（codex-drift、compat-framework、differential、e2e、perf、security-peer 在 macOS 15 和 26 上）。PERF 按 D1 只记录。
- 15 个摘要存在本目录的 `pr/`、`nightly/` 下。

## 核对

```
node tools/trace-ids/src/main.ts verify --milestone M1a \
  $(git ls-files 'docs/tasks/T099-m1a-exit/gate-1f972d2/*.json' | sed 's/^/--summary /') \
  --compat testdata/compat/reports/claude-agent-acp-0.91.0-claude-2.1.296.json
```

结果：`M1a: 0 of 288 IDs not done`，退出码 0。兼容场景里没有 COMPAT 编号的 16 个只列出、不判定（steer 不适用，hooks-reload、config-scope 为 observed）。
