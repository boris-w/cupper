# T037 修复 1 汇报

- 执行者：子 agent（Rust 工程师，宿主核心）
- 分支：`fix/T037-host-lock-flake`
- 完成日期：2026-10-20

## 做了什么

锁用例的偶发失败已修好。原因就是任务书推测的那个，已经确认；这条用例现在在单独的进程里运行。修复后在并行负载下全部 lib 测试连跑 200 次，锁用例没有再失败。`cargo xtask ci pr` 在干净的 worktree 上一次通过。代码改动 36 行，全是测试：增 35、删 1，只算 `crates/`，在约 150 的上限之内。

1. **复现与确认原因**
   - 失败的样子与 CI 一致：最后一步"放开后再拿"报 `free again: AlreadyRunning(...)`。
   - 临时实验（没有提交）：主线程循环 3,000 次"拿锁、立刻放开"，另一线程持续拉 `/usr/bin/true`，三种情况各跑 3 轮，统计拿不到锁的次数：
     - 不拉进程：0、0、0；
     - `cupper_sys::spawn`（带 `POSIX_SPAWN_CLOEXEC_DEFAULT`）：188、202、201；
     - `std::process::Command`：174、171、160。
   - 结论：同一进程里别的线程拉起子进程时，子进程在 exec 之前持有本进程 fd 的副本，`flock` 随副本留在原处。这就是 T021 查明的窗口。锁文件是 `O_CLOEXEC` 打开的，用 `cupper_sys::spawn` 也照样会失败，所以生产代码没有问题。
2. **修复**
   - 在 `gateway::testkit` 加了 `alone`，做法与 `cupper-sys` 的 `testutil::alone` 相同，环境变量为 `HOST_TEST_ALONE`。
   - 锁用例开头调用它：在父进程里起一个只跑这一条、`--test-threads=1` 的测试进程，检查那个进程恰好跑了这一条并且通过。
   - 验证过用例体确实在子进程里执行：临时加一个必定失败的断言，父进程会报 `failed on its own`。
   - 没有用"放开后短时重试"。
3. **同类用例的排查**：只有这一条需要改。
   - 其他 `HostLock::acquire` 的测试都只在新目录里拿一次锁。
   - `tests/lifecycle.rs` 在宿主进程退出之后才拿锁，不受影响。
   - crate 里没有别的 `flock`、`try_lock`、查 fd 表的用例。
   - "关掉一端、再等对端读到结束"的用例都会一直等到结束为止，副本只会让结束晚到一点。

## 关键取舍

- **`alone` 复制了一份，没有挪到两边共用的位置。** `cupper-sys` 的测试工具是私有模块，里面多是只属于它的 `fork` 和 fd 辅助。要共用，就得在唯一允许 `unsafe` 的 crate 上开一个公开的测试特性，再让 `cupper-host` 以开发依赖打开它；为 20 行不值得。
- **放在 `gateway::testkit`。** 唯一的使用者在 gateway 里，而 `cupper-host` 没有全 crate 的测试工具模块。

## 验证方式

| 场景 | 修复前 | 修复后 |
|---|---|---|
| `cupper-host` 全部 lib 测试（test-harness，默认并行），连跑 200 次 | 锁用例失败 3 次，1.5% | 0 次 |
| 锁用例与一条持续拉进程的临时用例同进程跑，连跑 1,000 次 | 失败 11 次，1.1% | 0 次 |

- **负载**：修复前，200 次连跑期间同时在编译和跑其他循环。修复后，另一个进程同时循环跑那条拉进程的临时用例，覆盖了 200 次连跑的前大半段。
- **其他检查**：`cargo clippy -p cupper-host --all-targets -- -D warnings` 在默认特性和 `test-harness,dev-auth` 下都无警告。
- **CI**：`cargo xtask ci pr --record docs/tasks/T037-host-lifecycle/ci/` 一次通过（rust 1,776、protocol 185、ts 353；sensitive-words 因本机没有词表而跳过）。license 作业这次没有碰上网络问题。摘要 `pr-bfcfff446b2a.json` 已提交。
- 过程和数字都记在 `docs/tasks/T037-host-lifecycle/notes-fix1.md`。

## 提交列表

```
eef4803 docs(task): record T037 fix 1 CI summary
bfcfff4 docs(task): record the host lock flake's cause and rates
7547ace test(host): run the host lock test in a process of its own
```

（main 在我开分支之后又前进了，`git log main..HEAD` 只列出本分支的这三个提交。）

## 遗留问题 / 范围外发现

**另一条偶发失败，没有修改**：`driver::runtime::tests::it_k21_reap_group_a_stop_reaps_descendants_in_groups_of_their_own`（T008 的用例）。

- **频率**：
  - 全部 lib 测试的两组各 200 次连跑里，各失败 1 次；
  - 只跑 `driver::runtime` 的 1,000 次里失败 4 次；
  - 单独循环这一条，第 436 次失败。
- **报错**：`f.stop()` 之后，自成一组的孙进程 `sleep` 还在（`ppid` 已是 1，已被 init 进程收养，还没被收走）。断言 `[process_info(adapter.pid), process_info(child.pid)] == [None, None]` 实际得到 `[None, Some(..)]`。
- **建议**：看起来是"信号已发、进程还没消失"就去检查了。可以改成在期限内等它消失，单独派一个修复。