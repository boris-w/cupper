# T008 修复 1 汇报

- 执行者：子 agent（Rust 工程师，宿主核心）
- 分支：`fix/T008-k21-reap-flake`
- 完成日期：2026-10-20

## 做了什么

偶发失败和成对的 `sleep 3600` 残留是同一个生产缺陷，已在生产代码里修掉。另外修了两处测试问题：检查过早，以及失败或被中断时不清理。修复后，并行负载下 K21 连跑 1,000 次没有失败，跑完全部 `cupper-host` 测试和完整 CI 后本 worktree 没有留下残留。`cargo xtask ci pr` 一次通过（rust 1,825、protocol 185、ts 394）。代码改动增 135、删 27（只算 `crates/`），在约 250 的上限之内。

1. **生产缺陷**：适配器在停止期间先退出时，停止流程不再发信号（`driver/runtime/mod.rs`）。
   - `close_stdin` 丢掉连接句柄，连接马上报告已关闭，不等适配器的 stdout 关闭。
   - 适配器（假 agent 和真实的 Claude 适配器都一样）在 stdin 关闭后自己退出。`Wire::Exited` 到达时退出状态和连接关闭都已具备，`end()` 就结束了实例：SIGTERM 和 SIGKILL 的定时器被取消，停止回复"完成"。
   - 结果：停止开始时记下的后代进程组，以及适配器组里剩下的进程，一个信号也收不到，被 launchd 收养。
   - 在用例里，手动时钟让 SIGTERM 通常抢在退出之前，所以只在负载高时失败，每次留下一对 `sleep`（一个和适配器同组，一个自成一组）。这和你看到的成对残留以及 CI 上那次失败一致。
   - 生产里用的是真实时钟，适配器在关 stdin 后的 3 s 内退出是常态，所以几乎每次停止都会漏掉自成进程组的工具命令，正是 K21 要防的情况。
2. **修复**
   - SIGTERM 那一步抽成 `term()`。停止过程中适配器退出、而 SIGTERM 还没发时，立即执行它：向记下的组和此刻能找到的组发 SIGTERM，2 s 后发 SIGKILL。
   - `end()` 在停止过程中，只要发过信号的组还有活着的成员就不结束，等 SIGKILL 那一步之后再结束。
   - 没有残留进程组时（常见情况），停止和原来一样立即结束。
3. **稳定复现**：新增用例 `it_k21_reap_group_an_adapter_that_leaves_first_still_loses_its_descendants`。
   - 它让时钟停在 SIGTERM 的时刻之前，适配器必然先退出。
   - 修复前连跑 10 次，10 次失败；修复后通过。
4. **测试不留残留**（`driver/runtime/tests.rs`）
   - **守卫 `Groups`**：
     - 适配器还在时，按 `with_descendants()` 记下整棵进程树的进程组（组号加启动时间）；
     - 无论通过、失败还是 panic，drop 时都向仍是原组的组发 SIGKILL，组号和启动时间都要对上，不按名字杀；
     - 记录同时写进临时目录下的 `cupper-test-groups/<测试进程 pid>-<序号>`；
     - 测试进程被直接杀掉时 drop 不会执行，所以下一次建守卫时，先清理测试进程已不在的那些记录。
   - **`gone()`**：用 `SystemClock` 的定时器，给信号最多 5 s 生效，取代"只看一次"。
   - 三条拉起孙进程的 K21 用例都用守卫。
   - `a_stop` 去掉了"停止至少 3 s"的断言，它依据的前提（连接要等孙进程放开 stdout 才关闭）不成立。
   - 后面的进程级用例照这个做法：孙进程出现之后、停止之前建守卫，用 `gone()` 断言，靠 drop 兜底。

## 关键取舍

- **按生产缺陷修，不只在测试里加等待。** 只把检查改成等待能掩盖失败，但生产里漏发信号的问题依旧。
- **设计 5.6.8 不用改。** 仍是先枚举、后发信号，SIGTERM 之后限时再 SIGKILL。只是第 2 步"限时等适配器退出"时适配器提前退出的话，第 4 步跟着提前，而不是被跳过。

## 验证方式

- **原因**：上面的稳定复现，修复前 10/10 失败。
- **修复前的自然失败率**：
  - T037 修复 1 时测得：全部 lib 测试连跑 200 次，失败 1 次（两组各一次）；只跑 `driver::runtime::tests` 1,000 次，失败 4 次；单独循环这一条，第 436 次失败。
  - 本次只跑 K21 用例 500 次，无论有没有负载都是 0 次。可见自然失败率很低，靠循环证明不了修复，所以补了上面的稳定复现。
- **被中断的情形**：用修复前的二进制跑复现用例，期间 `kill -9` 测试进程，留下记录文件和一对 `sleep`；下一次跑 `a_stop` 时这对 `sleep` 被清掉，记录目录变空。
- **修复后**：
  - K21 的 4 条用例连跑 1,000 次，0 次失败，用时 108 s。期间同时有两路全部 lib 测试在循环跑作负载，跑完 20 轮，没有失败；此前另连跑过 300 次，也是 0 次失败。
  - 跑完全部 `cupper-host` 测试和完整 CI 后，本 worktree 没有 `ppid` 为 1 的 `sleep` 或假 agent。检查命令写在 `notes-fix1.md`，它按工作目录区分是哪个 worktree 留下的。
- **lint**：`cargo clippy -p cupper-host --all-targets --features test-harness,dev-auth -- -D warnings` 无警告。
- **CI**：`cargo xtask ci pr --record docs/tasks/T008-adapter-lifecycle/ci/` 通过，摘要 `pr-daff3aad480e.json` 已提交。这次没碰上网络问题；ui 作业的截图测试因路径没变被跳过。
- 原因、复现方法和次数、失败率都在 `docs/tasks/T008-adapter-lifecycle/notes-fix1.md`。

## 提交列表

```
7a45bca docs(task): record the K21 leak's cause, rates and CI summary
daff3aa fix(host): signal a stop's groups when the adapter exits first
c357b85 test(host): kill the K21 tests' process trees whatever the outcome
```

## 遗留问题 / 范围外发现

1. **IT 失败时留下宿主进程（建议归 T063）**
   - `tests/lifecycle.rs` 的用例如果在 `terminate(&host)` 之前失败，拉起的 `cupper-host` 连同它的假 agent 会一直运行：`std::process::Child` 在 drop 时不结束子进程。
   - 本机现在就有别的 worktree（T043、T054）留下的 12 个 `ppid` 为 1 的 `cupper-host`，最久的已运行半小时以上，来自 `it_recording_export_*` 等用例。这些不是我拉起的，我没有动。
   - 本 worktree 早先失败的 `it_elevate_*` 和 `it_seed_*` 也留下过 6 个。我核对了命令行（本 worktree 的二进制、测试的临时数据目录）后，逐个发 TERM 结束了，它们的假 agent 随宿主一起退出。
   - 建议宿主进程也用 drop 守卫结束。
2. **在本地单独跑 IT-elevate 的前提**：本地单独用 `cargo test` 跑 IT-elevate 前，要先 `cargo build -p cupper-cli --features test-harness --bin cupper-elevate-probe`（CI 的 rust 作业会做这一步），否则必然失败，并且按第 1 条留下宿主进程。可以考虑在开发文档里注明，或者让用例在探针缺失时直接报出原因。