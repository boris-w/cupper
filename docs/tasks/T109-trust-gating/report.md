# T109 汇报

- 执行者：子 agent（Rust 工程师，宿主安全）
- 分支：`feat/T109-trust-gating`
- 完成日期：2026-10-20

信任门控已完成：IT-orch-trust-gating 和 `cargo xtask ci pr` 都通过，规模 829 行，在约 950 的上限之内。

## 做了什么

1. **闸门改成原因的集合**（`TurnMachine`，`Input::Gate { reason, shut }`）：任一原因在就关着。去掉一个、另一个还在时，队列的暂停原因换成剩下的那个。T044 的档位接线已跟着改。
2. **开回合前先检查**：会话打开 `require_checks` 后，消息要等 `Action::Check` 通过才写 `turn.started`。
   - 空闲时发送的消息先挂起、不进队列，命令的回复也一起挂起（裁定 1 的 (b)）：通过就开回合、回复 `started`，`origin` 仍是 `user`；不通过就进被闸门暂停的队列、回复 `queued`。
   - 失败的检查会以 `project_config_changed` 关闸门，通过的检查去掉这个原因。自发回合不检查。
3. **三个时机重扫**：扫描都在独立任务里做。
   - Attach 前：attach 任务先重扫，再决定 `Load` 还是 `Ignore`。
   - 发 prompt 前：即上面的检查。
   - 回合结束后：看本步事件里有没有 `turn.completed`，有就在后台重扫。
   - 闸门因 `project_config_changed` 关着时，`session/send` 和 `session/queue_resume` 也各触发一次重扫（裁定 4）。
   - 重扫复用 `index::projects` 拆出来的 `git_of`：git 根目录经 `Discovery::git` 的仓库档取，扫描放进阻塞线程池；可写根目录取全部未删除项目的路径（裁定 5）。
4. **判定**：
   - 想加载项目配置的会话，在项目不可信时（`stale`、未信任或重扫失败）关闸门，写一次 `notice{project_config_changed}`；项目重新可信就开闸门。
   - 重扫失败一律按不可信处理（fail closed，裁定 2）。失败原因作为语义码写在 notice 的 `params.reason` 里：`workspace_missing`、`project_missing`、仓库档错误的 reason，或 `ScanError::code()`。
   - Attach 时只在"可信且想加载"时用 `Load`，否则用 `Ignore`（裁定 3）；策略变了就改写 `session.configured.project_config`。
5. **改表交给索引**：项目从可信变为 `stale` 时，经新的 `IndexNotes::rescanned(project, scan)` 交给 `IndexActor`，由它改 `projects` 表、写 `project.upserted`（裁定 3）。判定规则抽成 `trust::state_after`，`Trust::rescanned` 也用它，只有一份。
6. **"不加载项目配置继续"**：宿主内部入口 `SessionManager::ignore_project_config(&SessionId)`。会话空闲时关闭 agent 会话，下一个回合带 `Ignore` resume，同时去掉 `project_config_changed` 这个原因。
7. **协议改动**：`NoticeParams` 加了 `reason` 字段，生成物已重新生成（13 行）。

## 关键取舍

- **只在会话里打开检查**：`SessionCore::require_trust_checks()` 只由会话管理器调用，每个会话都会打开。回合状态机的单元测试和两个性质模型不打开、保持原样，检查路径由 IT 覆盖。这样省掉了改几十处既有用例。
- **"想加载"单独记一个标志**：未信任时建的会话，项目后来被信任，下一次 attach 就改为加载。"继续"的选择会永久关掉加载。
- **索引只在变为 `stale` 时收到通知**：起初每次不可信的重扫都把整份扫描结果（含文件内容）发给索引，后来改成只在可信变为 `stale` 时发（单独的 fix 提交）。
- **夹具改用真实目录**：会话夹具的项目改成临时目录里真实存在的 `app/`；IT-orch-lifecycle 里断言 `/work/app` 的两处改为 `f.project()`。另外新增了只在测试构建里存在的 `GitTool::with_test_env`。

## 验证方式

- **IT-orch-trust-gating**：6 个用例，全部在 `ManualClock` 上跑。
  - 发 prompt 前发现变化：消息留在队列，回复 `queued`，不写 `turn.started`；写了 notice、通知了索引；重新信任后 `queue_resume` 放行。
  - 回合中变化：闸门在回合结束时关上。"继续"之后，agent 会话先 Close，再以 `Ignore` resume，`project_config: ignored`。
  - 未信任的项目以 `Ignore` 运行；工作区不见了，notice 带 `reason: workspace_missing`。
  - 两个原因同时在：先确认或先重新信任，两种顺序都只在两个原因都去掉后才放行。
  - git 根目录：在临时目录里 `git init`，根目录的 `.claude/` 落在扫描范围内。不经 git 根目录扫描的话，这个用例会失败（推理结论，没有改代码实测）。
  - 索引一半（在 `index/tests.rs`）：会话的重扫让 `projects` 表和索引视图都变为 `stale`。
- `cargo test -p cupper-host`（不带特性和 `--all-features` 两种）和 `cupper-protocol` 的测试全部通过；clippy（`--all-targets`，含 `--all-features`）无警告。
- **CI**：在干净的 worktree 里跑 `cargo xtask ci pr --record docs/tasks/T109-trust-gating/ci/`，全部通过（rust 1666、protocol 185、ts 353、license、testdata-scan），IT-orch-trust-gating 为 pass，摘要 `pr-cba5daeb585f.json` 已提交。

## 规模

829 行：实现约 535，测试约 295，不含 13 行生成物。开工前估约 780–840。逐提交数字在 `notes.md` 里。

## 提交列表

```
db86aa0 docs(task): T109 pr CI summary
cba5dae docs(task): T109 line counts, choices and handoffs
11093a1 fix(host): tell the index only of a project that became stale
cb00480 test(host): IT-orch-trust-gating
b70277a feat(host): rescan the project's trust before attaches and prompts and after turns
a75025c feat(protocol): a notice may carry a reason
6314375 feat(host): the gate takes several reasons; a turn can wait for a check
f8c00c6 docs(task): T109 size estimate and open questions
```

## 遗留问题 / 范围外发现

- **交接给 M1b 的确认界面任务**：给"不加载项目配置继续"加协议方法时，直接调用 `SessionManager::ignore_project_config`。`notice{project_config_changed}` 没有 `reason` 表示内容变了，有 `reason` 表示重扫失败。
- **交接给 T089（从库恢复）**：
  - "想加载"的标志和闸门的原因都只在内存里，可以从 `session.configured.project_config` 和恢复后第一次重扫重新得出；
  - "继续"的选择需要另记下来，例如加一个会话行字段。
- **可能的合并冲突**：会话邮箱 `Mail` 改成了枚举（`Call` 装箱），`SessionDeps` 多了 `git` 字段；这一带若有并行任务改动，合并时可能冲突。

worktree 和分支都保留着。