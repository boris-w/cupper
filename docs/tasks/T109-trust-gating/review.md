# T109 评审

- 评审人：编排者
- 日期：2026-10-20
- 结论：**通过，合并**

## 核对

- **规模**：829 行（实现约 535、测试约 295，不含 13 行生成物），在约 950 的上限之内；开工前估 780–840，估得准。
- **CI**：执行者的摘要 `pr-cba5daeb585f.json` 通过。合并 main 之后在本 worktree 重跑 `cargo xtask ci pr --record`，`pr-7727e01e91bc.json` 通过（rust 1721、protocol 185、ts 353、license、testdata-scan）。
- **合并冲突**（编排者解决）：
  - 会话 actor 的循环：main 上是 T008 改的 `participant.idle` 加 `Next`；本分支加了 `Mail` 枚举和重扫分支。合并后 `Next` 多了 `Rescanned`，`Mail` 的两个变体在 `match` 里展开。
  - 索引邮箱：保留本分支的 `Rescanned` 和 main 上 T047 的 `Reconcile`。
  - 宿主组装：T009 合并时 `Discovery` 去掉了 `git` 字段，改为 `git_tool(&found, &data_dir)` 一次得出。合并后同一个 `GitTool` 交给索引和会话管理器。

## 逐项意见

1. **闸门是原因的集合**：队列显示第一个还在的原因，去掉一个原因时换成下一个；两种顺序的用例都有。可以。
2. **每个回合都要一次新鲜的检查**：
   - 空闲时发送的消息挂起等检查，队列弹出前也要求 `Check::Passed`，用掉即清；
   - 因此重扫结果乱序到达（例如一次较慢的回合结束重扫，晚于发 prompt 前的检查返回"可信"）最多把闸门打开一次，下一个回合仍要先过一次新的检查，不会凭旧结果放行。
   - 我核对了这条推理，认可。
3. **fail closed**：项目行、工作区、git 根目录、扫描任一步失败都按不可信处理，原因写进 notice 的 `params.reason`。符合裁定 2。
4. **"想加载"的标志**：会话起初的 `Ignore` 来自信任状态（`policy::ProjectConfig::UserOnly`），不是用户的选择。所以项目之后被信任就改为加载，符合 5.2.6。"继续"的选择用 `ignore_for_good` 永久关掉加载，也对。
5. **索引只在可信变为 `stale` 时收到扫描结果**：避免把整份扫描（含文件内容）反复发给索引。好的修正。

## 小问题（不阻塞，已记交接）

- `rescan` 里的 `std::fs::canonicalize` 在 tokio 任务里同步执行。只有一次系统调用，影响很小；以后这里要做更多文件操作时，挪进 `spawn_blocking`。
- "想加载"的标志和闸门原因只在内存里。交给 T089 从库恢复时重新得出；"继续"的选择需要持久化。

## 交接

- M1b 确认界面：协议方法直接调 `SessionManager::ignore_project_config`。notice 没有 `reason` 表示内容变了，有 `reason` 表示重扫失败。
- T089：恢复"想加载"的标志、闸门原因和"继续"的选择。
