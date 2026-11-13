# T261 汇报

- 执行者：子 agent（CI 与工具）
- 分支：`chore/T261-codex-drift-same`
- 完成日期：2026-11-13
- PR：https://github.com/boris-w/cupper/pull/112
- CI：https://github.com/boris-w/cupper/actions/runs/43382951322 。`a65d4a34` 上 8 个检查全部通过。
- 分支上的 nightly：https://github.com/boris-w/cupper/actions/runs/43382948856 ，结论 success。其中 `codex-drift (ubuntu-24.04)`（job 131744744346）是 `fetch ok`、`drift ok`、`2 passed, 0 failed`；`latest` 仍是 0.168.0，用它自己的记录通过。

## 做了什么

1. **`drift.ts`**：新增纯函数 `reviewFor(drift, baseline, version, reviews)`，返回认定这次漂移的那条记录，以及它属于哪个版本。
   - `version` 有自己的记录时，只看这一条：漂移逐行相同就通过，不同就返回空，照旧要求重审（任务书第 3 条）。
   - 没有自己的记录时，在同一基线下找漂移逐行相同的其他版本的记录，有几条时取最后记下的那条（任务书第 2 条）。
   - 原有的 `reviewed()` 不变，由 `reviewFor` 调用。
2. **`main.ts`**：`check` 改用 `reviewFor`。
   - 沿用别的版本时，打印 `the same as codex <版本>'s, reviewed on <日期> (<任务>): <结论>`，即版本、日期、任务、结论。
   - 有自己的记录时，输出和以前一样。
   - 未审（"not reviewed yet"）、要求重审（"review it again"）的提示和退出码都不变。文件头的说明补了一句。
3. **单元测试**：UNIT-codex-drift 新增 1 条，覆盖任务书第 4 条的四种情况，另加一种：
   - 与某条记录相同：通过，并指明沿用的是哪个版本；
   - 多一行、少一行：都报未审；
   - 基线不同的记录：不沿用；
   - 有自己的记录而漂移不同：不沿用别的版本的记录，即使那条正好相同。

## 关键取舍

- **自己的记录优先**：只要 `latest` 有自己的记录，就只认这一条，避免一次漂移恰好和更早的某条记录相同、把应当重审的情况放过去。
- **有几条相同时取最后记下的**：这样沿用的是最近一次审阅的结论。
- **判定写成纯函数**：`main.ts` 只负责打印，所有分支都能在单元测试里覆盖，不用跑 codex。

## 验证方式

- **PR #112 上 8 个检查全部通过**（run 43382951322）：rust、ts、ui、protocol、e2e、license、testdata-scan、compat-framework。
- **分支上的 nightly `codex-drift` 判通过**：run 43382948856，job 131744744346。
- **本机**：
  - 工具的 vitest 9 条通过；tsc、eslint、prettier 通过。
  - `cargo xtask ci codex-drift --in nightly` 通过（2 passed）；`cargo xtask ci ts` 606 条全部通过。
- **端到端演示**（本机，临时目录）：只从 npm 取了 0.168.0；codex 照旧只运行 `generate-json-schema`，`HOME`、`CODEX_HOME` 指向空目录，没有登录，没有发 prompt，没有读写 `~/.codex`。
  - 先 `check` 0.168.0：用它自己的记录通过，输出和以前一样。
  - 再把 `fetched.json` 里的版本号改成 0.168.1（包不变，相当于一个协议没变的补丁版本）：`check` 退出码 0，打印 `the same as codex 0.168.0's, reviewed on 2026-11-13 (T113): …`。
  - 临时目录已删除。
- **对照验收标准**：
  - [x] `tools/codex-drift` 的单元测试通过，覆盖四种情况。
  - [x] 本机 `cargo xtask ci codex-drift --in nightly` 通过；分支上的 nightly `codex-drift` 判通过。
  - [x] PR 上 8 个检查全部通过，链接见上。

## 规模

+77 −5（`cargo xtask size`），停止线 80。真正的改动约 55 行（函数、调用处、测试和注释）。另有约 18 行是两处 import 加了 `reviewFor` 之后超过行宽，被 prettier 拆成每行一个名字；没有为了行数改写。

## 提交列表

```
a65d4a34 feat(codex-drift): a drift the same as a reviewed one passes
```

## 遗留问题 / 范围外发现

- **新逻辑还没在 nightly 里真正用到**：`latest` 仍是 0.168.0，有自己的记录。等 npm 上出了下一个协议没变的版本，nightly 会打印 `the same as codex 0.168.0's, …` 并判通过；如果协议变了，照旧报未审，需要人工审阅。
- 任务书不做的几项没有动：改基线（`write`）、`review` 的写法、`anyOf` 的盲点、nightly 的 workflow。
- worktree 保留着。
