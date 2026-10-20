# T034 汇报

- 执行者：子 agent（Rust 工程师，宿主核心）
- 分支：`feat/T034-index-actor`（编排者已在 `ea1354e` 把 main 合进来）
- 完成日期：2026-10-20

## 做了什么

收窄后的 T034 已完成。索引 actor 接进了宿主，项目、信任和帧上限的处理都已上线；IT-project-add-trust 和对账、检查点的用例都通过，`cargo xtask ci pr` 第二次全部通过。第一次失败在一个 T037 的旧用例上，属于偶发，见"遗留问题"第 1 条。

规模：净增约 1,312 行（只算 `crates/`，不含生成物），在约 1,500 的上限之内。我自己写过一份发现链，后来改用 T055 的，那 130 行写了又 revert，增删相抵。

1. **协议**：`TrustScan.findings_omitted`、`ProjectTrustInfoResult.findings_omitted`、`diff_omitted` 三个可选字段。裁定只列了两个；`trust_info` 的 findings 也可能装不下，所以多加了一个。5.3.11 已记下这个预算，`pnpm gen` 的生成物随提交。
2. **`StreamWriter::checkpoint`**：在当前 head 立即写一次检查点；head 处已有检查点时不写。
3. **`Routes` 拆分**：`index` 只登记三个项目方法；`session/mark_read`、`client/presence` 改由 `Routes::presence` 登记，留给 T100 调用，在那之前真实宿主对它们回 `unsupported`。`Routes::start` 两个都登记，所以 UNIT-unsupported-set 不受影响。
4. **`index/` 模块**：
   - **结构**：`IndexHandle` 同时实现 `Port`、`IndexPort`、`IndexNotes`、`Part`，背后是唯一的 `IndexActor`，它是索引流和 `projects` 表唯一的写者，已登记进静止屏障（`INDEX`）。
   - **启动第 7 步**：`IndexHandle::start` 在绑定套接字之前运行，每次都按 `projects`、`sessions` 两张表对账：
     - 项目与表不一致就重发 `upserted`，表里没有的发 `removed`；
     - 视图里缺的会话按行补出摘要，已有的摘要不覆盖；
     - `host.updated` 有变化才写。
   - **项目与信任**：慢操作在另起的 task 里做：取 realpath；向上找 `.git`，再经仓库档跑 `rev-parse --path-format=absolute --show-toplevel --git-common-dir`；在阻塞线程池里扫描。做完回送给 actor，由它把行、`project.upserted` 和账本结果写进同一个事务。
     - 错误映射：`ScanError` 对应 `unavailable/scan_failed`（消息里带语义码），`DigestMismatch` 对应 `conflict/digest_mismatch`，git 错误按 `GitError::code()`、`reason()` 转换。
     - 找不到 git，或往上没有 `.git` 时，`is_git = false`。
   - **帧上限**：findings 加差异最多 48,000 字节。先装差异再装 findings，`detail` 截到 2,000 字节并加 `…`，装不下的计入 omitted。
   - **`clients.updated`**：在 `opened`、`closed` 时按 `Gateway::clients()` 写，内容相同就不写。
   - **`IndexNotes::pending`**：只把待处理请求的个数和类别写进已有的会话摘要，会话状态留给 T100 计算。
   - **`Part::stop`**：写索引流的检查点。
5. **宿主组装**：
   - `Discovery` 加了 `git: Option<GitTool>` 字段，取自 T055 那份发现链的 `CliLocator`（`tool("git")`、`tool("git-lfs")`、`trusted_path()`）；
   - `last_exit_abnormal` 由 `recover::Run.abnormal`（epoch 判定得出的 `LastExit::Unclean`）给出；
   - 用 `IndexHandle` 换掉了 `NoIndex`，`NoIndex` 已删除；
   - `routes.index` 已登记，索引是第三个 `Part`；
   - `Host` 上没人用的 `streams` 字段去掉了，改为传给索引。

## 关键取舍

- **发现链**：按协调，我写的那份 revert 掉了，没有另起一套，`GitTool` 直接作为 T055 `Discovery` 的一个字段。
- **对账时不覆盖已有的会话摘要**：视图里的摘要是实时状态，归 T100；表只用来补缺和删掉已归档的。
- **先装差异再装 findings**：`stale` 时，差异是确认框里最要紧的信息。
- **IT 放进 `tests/lifecycle.rs`**：复用那里拉起宿主和连接的辅助函数，省下约 45 行。in-crate 的项目流程用例按裁定删掉了。

## 验证方式

- [x] **IT-project-add-trust**（`it_project_add_trust_through_the_gateway`）：真实宿主进程，`test_config.login_env` 的 PATH 设为 `/usr/bin:/bin`。在临时目录里 `git init` 一个仓库，工作区在仓库根下两层。
  - `project/add` 找到两级的配置：`../../.claude/settings.json` 的 hook 和 `.mcp.json` 的服务器；
  - `project/trust` 后状态为 `trusted`；
  - 加了 1,500 个 agent 后，`trust_info` 返回 `stale`，差异条数加 `diff_omitted` 等于 1,500，findings 条数加 `findings_omitted` 等于 1,502，结果没有超过帧上限；
  - 库里的 `trust_state` 是 `stale`，`git_common_dir` 有值；
  - SIGTERM 后退出码为 0。
- [x] **对账与检查点**（`index_start_reconciles_with_the_tables_and_checkpoints_on_stop`）：
  - 日志里多出的项目被删掉，表里的项目补齐；
  - 会话按行补出摘要（`unread`），`host.updated` 写入；
  - 第二次启动不写任何事件；
  - `Part::stop` 写的检查点正好在 head。
- [x] 另有预算的用例（`index_budget_…`，含截断与 omitted 计数）和 `StreamWriter::checkpoint` 的用例（`store_streams_checkpoint_on_demand_…`）。
- [x] 测试不睡眠：in-crate 用例用 `ManualClock`；进程级用例像 IT-lifecycle 一样，用 `SystemClock` 的定时器轮询。
- [x] 测试名以用例编号开头；分支上 `tests/ids.toml` 的 T034 段没有改。
- [x] 合并 main 之后，先在本地把 release、dev-auth、test-harness、全部特性四种组合的 clippy 和宿主测试都跑过了。
- [x] 在干净的 worktree 上跑了 `cargo xtask ci pr --record docs/tasks/T034-index-actor/ci/`：rust 1578、protocol 185、ts 353、license、testdata-scan 都通过，摘要 `ci/pr-39fa8a19ac05.json` 已提交。`sensitive-words` 一步因为本机没有敏感词表而跳过。

## 提交列表

```
fbe5cdc docs(task): T034 CI summary (pr at 39fa8a19ac05) and a flaky lock test
39fa8a1 docs(task): T034 notes: actual lines, index points, hand-offs
39438bb test(host): IT-project-add-trust through the host
fdc44da feat(host): the index in the host's assembly
ea1354e Merge branch 'main' into feat/T034-index-actor
0f266b7 feat(host): the index actor, projects and trust
8100a43 refactor(host): route read marks and presence apart from projects
2349bc7 Revert "feat(host): the discovery chain as one function"
9249de1 feat(host): the discovery chain as one function
02f155d feat(host): a stream writer checkpoints on demand
8af3579 feat(protocol): counts of trust findings and changes left out
09d146f docs(task): narrowed T034 estimate after the split
5e09368 docs(task): estimate T034 by module and propose a split
```

## 遗留问题 / 范围外发现

1. **T037 的锁用例偶发失败（没有修）。** 第一次跑 CI 时，`gateway::listen::tests::host_lock_is_held_once_and_names_its_holder` 失败了一次：释放 `host.lock` 后立刻再拿，报"已被占用"。原因应该是 T021 交接里说的那个窗口：别的测试线程正在 `posix_spawn` 时，fd 表会被短暂复制。T055 合并后，crate 内拉子进程的用例多了，更容易碰上。重跑就通过了。建议另开一个小任务：这个用例改成单独进程隔离（像 `cupper-sys` 的 `testutil::alone`），或者释放后在短时限内重试。
2. **交给后续任务的事项**（细节在 `notes.md`）：
   - **T100**：在 `Host::start` 里调用 `routes.presence(..)`；状态计算接在 `IndexActor::pending` 旁边；对账时按行补出的摘要只是起点。
   - **T059**：`HostUpdated.pending_upgrade` 改为取自检的结果。
   - **T047**：`INDEX` 已登记进静止屏障，`IndexHandle` 是第三个 `Part`。
   - **用 git 的任务**：从 `Discovery::git` 取。