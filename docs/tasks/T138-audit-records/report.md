# T138 汇报

- 执行者：子 agent（Rust 工程师，宿主核心）
- 分支：`feat/T138-audit-records`
- 完成日期：2026-10-20

## 做了什么

按方案 A 完成。审计接收端接上了写线程，每条记录写进 `audit` 表和 host.log；全部有产生者的事件都按 5.8.6 的正式格式落盘；`conn.accepted` 带上了对端信息；`config.confirmed` 和 `project.trust.*` 有了产生者。

- **CI**：`cargo xtask ci pr` 在干净的 worktree 上一次通过（rust 1,839、protocol 185、ts 424）。
- **规模**：净增 671 行（增 738、删 67，只算 `crates/`），在约 700 的上限之内。实现约 380 行，测试约 290 行。
- **中途推算越线一次**：UNIT-audit-format 第一版 rustfmt 后有 218 行，按它推算总数会到 800 上下，越过硬停止线。我先把用例压缩（单行 JSON 比对，去掉重复的用例），推算降到 680 左右后才接着写 IT。

1. **审计写入**（`supervisor/audit.rs`，新的静止屏障种类 `audit`，已在 `Host::start` 登记）
   - 在 `Participant::idle` 里等接收端；
   - 每条先写 host.log：`tracing` 事件，字段为 `event`、`conn`、`principal`、`peer`、`detail`；
   - 再经写线程插入 `audit` 表；
   - `stream.subscribed` 只进 host.log；写表失败时记 warn。
   - 90 天清理沿用已有的 `Cleanup::Audit`，`retention.rs` 没有改。
2. **正式格式**（`gateway/audit.rs`）
   - `AuditEvent::formal()` 给出事件名、principal、`peer_json`、`detail_json`；`kept()` 说明是否进表。
   - 映射的事件：
     - `conn.accepted`、`conn.rejected`、`conn.initialized`、`conn.closed`（关闭原因写成蛇形代码，`HostExiting` 记为 `host_exiting`）；
     - `interaction.responded`；
     - `config.unconfirmed`、`config.confirmed`；
     - `elevation.requested`、`granted`、`denied`、`expired`、`lineage_refused`、`rate_limited`；
     - `project.trust.trusted`、`stale`、`scan_failed`（摘要取前 16 位）。
   - `host.*`、`git.unsafe_repo_config`、`privileged.executed` 在 `formal()` 的文档里注明由 T163 随产生者加入，没有预先加 `AuditEvent` 变体。
   - 网关夹具的审计行仍是 `Debug` 文本；文件开头"归 T038"的注释已改掉。
3. **`conn.accepted` 补对端信息**：在 `admit` 里用 `pid_path` 和 T133 的 `AuditToken::signing_identity`，记可执行文件路径、签名 identifier、Team ID；取不到时留空。
4. **`config.*`**
   - 两条记录都带实际模式与档位、设定者。
   - `config.confirmed` 在会话离开 `config_unconfirmed` 时发出，原因记离开的是哪一种。设定者取同一步的 `SetBy`，没有时取 `TierGuard` 当前的设定者（新加了一个只读访问器）。
   - 为此 T044 的 `ConfigUnconfirmed` 改成了 `{reason, config: ConfigState}`。
5. **`project.trust.*`**
   - **索引**：
     - `project/trust` 成功后记 `trusted`；
     - `project/add`（已有项目）、`trust_info`、`rescanned` 让受信任的项目变为 stale 时记 `stale`；
     - `trust`、`trust_info` 扫描失败时记 `scan_failed`。
   - **会话**：重扫时扫描本身失败，也记 `scan_failed`。
   - 新路径的 `project/add` 扫描失败时还没有项目，不记。

## 关键取舍

- **秘密**：令牌、密钥、登录 shell 的环境本来就不在任何审计事件里。另外握手失败只记错误码和 `data`，不记 `message`，因为参数非法时 serde 的报错可能引用客户端发来的内容。
- **IT 里验证 90 天清理的方法**
  - 没有直接把手动时钟推进 91 天，原因有两个：
    - 每小时一次的账本清理让 `test/clock_advance` 要逐步走两千多个期限，约 30 s；
    - 途中每天的审计清理会先把旧记录删掉，`test/run_maintenance` 就验证不到。
  - 改为：宿主运行时插入一条早于 90 天 1 ms、一条晚 1 ms 的记录。`run_maintenance` 后只剩后者；手动时钟推进 2 ms 后再跑一次，后者也被删，新近的连接记录留着。
- **T038 交接的边界情况**（`elevation.requested` 写入失败）：审计在索引写批次之前单独发出，不另记（你已认可）。

## 验证方式

- **UNIT-audit-format**（3 条用例）：
  - 每种映射的事件名、principal 和记录内容；
  - `conn.accepted` 的对端信息；`stream.subscribed` 不进表；
  - 写入 task 把一条没有连接的会话审计（`config.unconfirmed`）落进表，`conn_id` 为空。
- **IT-audit-retention**（真实宿主、手动时钟）：
  - 两个客户端产生的 `conn.accepted`、`conn.initialized`、`conn.closed` 按正式格式进了表，principal 为 `pr_test`；
  - host.log 里有同样顺序的 `audit` 事件；
  - 90 天清理按上一节的方法验证。
- **产生点**：
  - `config.confirmed`：会话 gating 用例确认后收到它，原因、模式、档位、设定者和进入时一致；
  - `project.trust.*`：IT-project-add-trust 末尾加了一个指向不存在目标的链接，让扫描失败，表里依次有 `trusted pr_test`、`stale`、`scan_failed`。
- **稳定性与进程卫生**：
  - 两个 IT 连跑 10 次全绿；
  - 新 IT 的宿主包在 drop 时结束的守卫里，临时让它中途 panic 也没有留下宿主进程；
  - 跑完后本 worktree 没有残留进程。
- **其他**：
  - 不睡眠：IT 只用手动时钟和已有的 `until` 等待；
  - `cargo clippy -p cupper-host --all-targets --features test-harness,dev-auth -- -D warnings` 与 `cargo clippy --workspace --all-targets -- -D warnings` 无警告；全部 `cupper-host` 测试通过；
  - 测试名以 UNIT-audit-format、IT-audit-retention 开头，`tests/ids.toml` 没有改动。
- **CI**：`cargo xtask ci pr --record docs/tasks/T138-audit-records/ci/` 通过，摘要 `pr-79a83188285d.json` 已提交。各模块的行数和取舍记在 `notes.md`。

## 提交列表

```
97d83bc docs(task): record T138 CI summary
79a8318 docs(task): record T138 line counts and choices
ec96ad2 fix(host): keep handshake messages out of conn.closed records
ec43128 test(host): IT-audit-retention, and project.trust.* on the real host
9b315ce test(host): UNIT-audit-format, each event in its formal shape
1d1eddd feat(host): store audit records in their formal shape
1de5d60 docs(task): estimate T138 by module
```

## 遗留问题 / 范围外发现

1. **有序退出时的 `conn.closed`（`host_exiting`）只进了 host.log，没进表。**
   - 原因：`Host::exit` 先停写线程，再 `close_all`，这些记录到达时表已经写不进去了。
   - 要进表，需要把顺序调成：先关连接、排空审计写入，再停写线程。
   - 这是 `Host::run` 的范围（T059）。T163 加 `host.exiting` 时也需要同样的排空，建议交接给 T163。
2. **`config.unconfirmed` 的结构变了**：T044 的 `ConfigUnconfirmed` 现在是 `{reason, config: ConfigState}`。以后读这个事件的代码以新结构为准。
3. **`project/add` 新路径扫描失败时没有审计**：因为还没有项目 ID。如果需要按路径记录，可以在 T163 一并考虑。

---

# T138 补充汇报：合并 main 后 IT-idle-exit 失败的修复

- 执行者：子 agent（Rust 工程师，宿主核心）
- 分支：`feat/T138-audit-records`（在你的合并提交 c3602b8 之上）
- 日期：2026-10-20

## 结论

原因在 T138：`admit` 在异步线程上同步读取对端签名，一次约 600 ms。已在 T138 这边修好，没有动 T059、T129 的代码。修复后 IT-idle-exit 连跑 5 次全部通过，`cargo xtask ci pr` 通过。

## 原因

1. T138 在 `admit` 里新加了 `AuditToken::signing_identity()`，它在异步工作线程上同步执行。
   - 对体积大、未正式签名的程序（这里是 lifecycle 测试二进制）一次约 600 ms。临时加的日志显示，这次调用前后相差 627 ms。
2. 这段时间里宿主的定时器也停了。
   - T059 的监护本该每 20 ms 观察一次，实际只在启动时和阻塞结束后各观察了一次。
   - T138 的审计写入也一样，所有记录都到退出时才处理。
3. 客户端要等 `admit` 结束、握手完成，才算进 `gateway.clients()`。
4. 这条 IT 把空闲时长设成 300 ms，宿主看到的是一段超过 300 ms、没有客户端的空闲，就以 `idle` 退出了。

T059 的监护逻辑本身没有问题，T129 的安装器也与此无关。你列出的可疑点里，是"`admit` 里补对端信息"这一条。

## 修法

都在 T138 的代码里，修复净增 41 行（增 59、删 18）。

- **`conn.accepted`**：改为带上钉住的 `AuditToken`，路径、identifier、Team ID 一开始为空。连接不再等这次查询，异步线程也不再被阻塞。
- **审计写入 task**：收到 `conn.accepted` 时经 `spawn_blocking` 调用 `AuditEvent::read_peer()`，读出可执行文件路径和签名身份。
  - 写入 task 是串行的，记录仍按原顺序落盘。
  - 读完路径后用 `unique_id` 再核对一次令牌，对不上就不记路径，避免 pid 复用时记错进程。
- **代价**：对端如果在写入 task 读到之前就退出了，这三项为空。写入 task 通常在接受连接后几毫秒内就处理到这条记录。
- **UNIT-audit-format 跟着改**：改为对本进程取令牌，`read_peer` 后核对路径就是测试二进制本身，identifier 是字符串。

## 验证

- IT-idle-exit 连跑 5 次，全部通过。
- IT-audit-retention、全部 `cupper-host` 测试通过。
- clippy 无警告：`--all-targets` 加 `test-harness,dev-auth` 特性，以及默认特性。
- **CI**：`cargo xtask ci pr --record docs/tasks/T138-audit-records/ci/` 通过（rust 1,874、protocol 185、ts 436），摘要 `pr-ddf6762f1a6f.json` 已提交。
  - 中间重跑过一次：第一次运行时，xtask 的 `it_host_bundle_layout_of_a_self_use_build` 报 "building cupper-host failed (exit status: 101)"。当时磁盘只剩 3.8 GB，是环境问题，不是代码问题。
  - 我删掉了本 worktree 的增量编译缓存（约 50 GB，`target/debug/incremental`），再跑就通过了。
- 你指出的那份失败摘要 `pr-c3602b815eac.json` 已删掉。
- 原因、修法和验证也记进了 `notes.md`。

## 提交

```
77ad79c docs(task): record the T138 idle-exit fix and its CI summary
ddf6762 fix(host): read a peer's signature in the audit writer, not on accept
```

## 提醒

宿主里其他在连接路径或异步线程上同步调用 Security 框架的地方，也可能遇到同样的阻塞。例如 `admit` 里的 `verify(ui_requirement)`：现在只有配了 UI 的代码要求时才执行，release 构建会配。正式签名的小程序一般约 1 ms，但第一次调用或遇到大的未签名程序时会慢很多。建议在 T163 或 T180 检查一遍，必要时同样移到 `spawn_blocking`。