# T037 修复 1 工作笔记：锁用例的偶发失败

- 执行者：子 agent（Rust 工程师，宿主核心）

## 失败的样子

`gateway::listen::tests::host_lock_is_held_once_and_names_its_holder` 的最后一步：释放 `host.lock` 之后立刻再拿，报 `free again: AlreadyRunning(".../host.lock")`。本地复现出的报错与 CI 上的一致。

## 原因的确认

推测是 T021 查明的窗口：同一测试进程里别的线程拉起子进程时，子进程在 exec 之前短暂持有本进程每个 fd 的副本（close-on-exec 的也一样）。`flock` 跟着打开的文件描述走，副本没关，锁就还在。

做了一个临时实验（没有提交）：在 lib 测试二进制里加一个 ignored 用例，主线程循环 3,000 次"拿锁、立刻放开"，每次都数拿不到的次数；另起一个线程按三种方式持续拉 `/usr/bin/true`。

| 另一线程 | 每轮 3,000 次中拿不到的次数（3 轮） |
|---|---|
| 不拉进程 | 0、0、0 |
| `cupper_sys::spawn`（`POSIX_SPAWN_CLOEXEC_DEFAULT`） | 188、202、201 |
| `std::process::Command` | 174、171、160 |

结论与推测一致：只要同一进程里有别的线程在拉进程，释放后立刻再拿就有约 6% 的机会失败；不拉进程时从不失败。锁文件是 `O_CLOEXEC` 打开的，用的是 `cupper_sys::spawn` 也一样会失败，所以生产代码没有问题。生产环境里宿主不会在同一个进程里释放后再拿这把锁。

## 修复

- 照 `cupper-sys` 的 `testutil::alone`，在 `gateway::testkit` 里加了同样做法的 `alone`（环境变量 `HOST_TEST_ALONE`）。这条用例开头调用它：在父进程里起一个只跑这条用例、`--test-threads=1` 的测试进程，检查它恰好跑了这一条并且通过；用例体只在那个进程里执行，那里没有别的测试拉进程。
- 选择复制而不是挪到两边共用的位置，理由如下：
  - `cupper-sys` 的测试工具是私有模块，里面多是只属于它的 `fork`、fd 辅助；
  - 共用就得在唯一允许 `unsafe` 的 crate 上加一个公开的测试特性，再让 `cupper-host` 以开发依赖打开它；
  - 只为 20 行不值得。
- 放在 `gateway::testkit`：唯一的使用者在 gateway 里，而 `cupper-host` 没有全 crate 的测试工具模块。
- 验证了子进程确实在跑用例体：临时在用例体里加一个必定失败的断言，父进程报 `failed on its own`。
- 没有用"释放后短时重试"。

## 同类用例的排查（只看测试）

- `HostLock::acquire` 的其他测试用法：`listen.rs` 另外三条用例、`gateway::testkit` 的测试网关，都是在新目录里只拿一次，不存在释放后再拿。`tests/lifecycle.rs` 在宿主进程退出之后才拿锁：锁属于宿主进程，宿主退出时它拉起的进程都已 exec，不受影响。
- crate 里没有别的 `flock`、`try_lock`、`O_EXLOCK`、查 fd 表的用例。SQLite 用的是 `fcntl` 记录锁，按进程计、子进程不继承，不受影响。
- 关闭一端、再等另一端收到结束的用例：`driver::acp`、`gateway::dispatch::tests`、`startup::payload`、`tests/startup.rs`。它们都是等到对端读到结束为止（带的是很长的期限或 `ManualClock`），副本只会让结束晚到一点，不会失败。
- 结论：只有这一条需要改。

## 修复前后的数据

本机上并行跑，作为负载。

| 场景 | 修复前 | 修复后 |
|---|---|---|
| `cupper-host` 全部 lib 测试（test-harness，默认并行线程），连跑 200 次 | 锁用例失败 3 次（第 58、113、172 次），失败率 1.5% | 锁用例 0 次 |
| 只跑锁用例和一条持续拉进程的临时用例（`spawn_noise`，拉 200 次 `/usr/bin/true`），连跑 1,000 次 | 锁用例失败 11 次（1.1%） | 0 次 |
| 只跑锁用例和 `driver::runtime` 的用例，连跑 1,000 次 | 锁用例 0 次（重合窗口太窄） | 未重跑 |

- **修复前的负载**：全部 lib 测试的 200 次连跑期间，同时在编译、跑实验，以及上面两组 1,000 次的循环。
- **修复后的负载**：200 次连跑期间，另一个进程同时循环跑 `spawn_noise` 1,500 次，前大半段时间都有这份负载。
- **修复后的结果**：锁用例 0 次失败；同一组里 `it_k21_reap_group_a_stop_reaps_descendants_in_groups_of_their_own` 失败 1 次（第 155 次），见下一节，与本修复无关。

## 范围外的偶发失败（没有修改）

`driver::runtime::tests::it_k21_reap_group_a_stop_reaps_descendants_in_groups_of_their_own`（T008）：

- 出现频率：全部 lib 测试的 200 次里失败 1 次（修复前、修复后两组各 1 次）；只跑 `driver::runtime` 的 1,000 次里失败 4 次；单独循环这一条，第 436 次失败。
- 报错：`f.stop()` 之后，自成一组的孙进程 `sleep` 还在（`ppid` 已是 1，说明已被收养给 init 进程，还没被收走），`[process_info(adapter.pid), process_info(child.pid)]` 得到 `[None, Some(..)]`，而期望是 `[None, None]`。
- 看起来是"信号已发、进程还没消失"就去检查。建议改成在期限内等它消失。
