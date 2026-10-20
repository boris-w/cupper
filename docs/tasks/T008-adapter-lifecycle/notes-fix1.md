# T008 修复 1 工作笔记：K21 回收用例的偶发失败与残留进程

- 执行者：子 agent（Rust 工程师，宿主核心）

## 结论

偶发失败和成对的残留是同一个**生产缺陷**，用例只是把它碰巧暴露了出来。另有两处测试问题：检查过早，以及失败或被中断时没有清理。

### 生产缺陷：适配器先退出时，停止流程不再发信号

关闭顺序（设计 5.6.8）的实现：

1. 停止开始时，在发任何信号之前记下适配器及其全部后代的进程组；
2. 逐个 `session/close`，然后关闭 stdin（`close_stdin` 丢掉连接句柄），过 3 s 发 SIGTERM，再过 2 s 发 SIGKILL。

问题出在第二步：

- 丢掉连接句柄后，连接马上结束，`FromAgent::Closed` 立刻就到，并不等适配器的 stdout 关闭。
- 假 agent 和真实的 Claude 适配器一样，stdin 一关就自己退出。
- `Wire::Exited` 到达时，`exit` 和 `closed` 都已具备，`end()` 就结束了这个实例：取消 SIGTERM、SIGKILL 两个定时器，并回复"已停止"。
- 第一步记下的、自成进程组的后代，以及适配器组里剩下的进程，一个信号也收不到，最后被 launchd 收养。

在用例里，`Fixture::tick` 每次让出执行权就把手动时钟拨到下一个期限。所以 SIGTERM 通常抢在适配器退出之前发出，用例能通过。机器负载高时，退出先到，用例就失败，并留下一对 `/bin/sleep 3600`：`spawn_child` 那个和适配器同组，`spawn_group_child` 那个自成一组。这与编排者看到的成对残留和 CI 上那次失败（孙进程 `ppid` 为 1、仍然存活）一致。

生产环境用的是真实时钟，适配器在 stdin 关闭后的 3 s 内退出是常态。所以几乎每次停止都会漏掉自成进程组的工具命令，而这正是 K21 要防的。

### 测试问题

- **检查过早**：`process_info` 读不到僵尸进程，但刚收到 SIGKILL 的进程在消失前还能被读到一小会儿。原用例在停止返回后只看一次。
- **不清理**：用例失败、panic 或测试进程被直接杀掉时，孙进程无人回收。

## 复现

- **自然失败率**（修复前，原用例，本机）：
  - T037 修复 1 时测得：`cupper-host` 全部 lib 测试连跑 200 次，各失败 1 次（两组）；只跑 `driver::runtime::tests` 1,000 次，失败 4 次；单独循环这一条，第 436 次失败。
  - 本次复测：只跑 `it_k21` 的用例 500 次，无负载时 0 次失败；同时跑两路全部 lib 测试作负载，500 次也是 0 次。
  - 可见自然失败率很低，并且随负载形态变化，靠循环跑不足以证明修复。
- **稳定复现**：新增用例 `it_k21_reap_group_an_adapter_that_leaves_first_still_loses_its_descendants`。
  - 它用 `tick(2, stop)`，让时钟停在 SIGTERM（关 stdin 后 3 s）之前，于是适配器必然先退出。
  - 修复前连跑 10 次，10 次失败；靠新加的守卫，每次都没有留下残留。

## 修复

**生产**（`driver/runtime/mod.rs`）：

- SIGTERM 那一步抽成 `term()`。
- 停止过程中适配器退出、SIGTERM 还没发时，立即执行 `term()`：向停止开始时记下的组和此刻能找到的组发 SIGTERM，取消关 stdin 和 SIGTERM 两个定时器，2 s 后发 SIGKILL。
- `end()` 在停止过程中，只要发过信号的组还有活着的成员就不结束；SIGKILL 那一步之后才结束。
- 没有残留进程组时（常见情况），停止和原来一样立即结束。

设计 5.6.8 的四步仍然成立：仍是先枚举、后发信号，SIGTERM 之后限时再 SIGKILL。只是第 2 步"限时等适配器退出"里如果适配器提前退出，第 4 步随之提前，而不是被跳过。设计文本不用改。

**测试**（`driver/runtime/tests.rs`）：

- **守卫 `Groups`**：
  - 适配器还在时，按 `with_descendants()` 记下整棵进程树的组（组号加启动时间）；
  - 无论通过、失败还是 panic，drop 时向仍是原组的组发 SIGKILL，组号和启动时间都要对上；
  - 同时把记录写进临时目录下的 `cupper-test-groups/<测试进程 pid>-<序号>`；
  - 下一次有用例建守卫时，先清理那些测试进程已经不在了的记录，覆盖测试进程被直接杀掉的情况。
- **等待消失**：`gone()` 用 `SystemClock` 的定时器，给信号最多 5 s 生效，取代"只看一次"。
- **两条 K21 用例改用守卫**：
  - `a_stop` 去掉了"停止至少 3 s"的断言。它依据的前提（连接要等孙进程放开 stdout 才关闭）并不成立，修复后适配器先退出时停止只用 2 s。
  - `sampling` 也改成等待消失。
- 后面的进程级用例照这个做法：在孙进程出现之后、停止之前建守卫，用 `gone()` 断言，靠 drop 兜底。

验证被中断的情形：

- 用修复前的二进制跑复现用例，在等待期间 `kill -9` 测试进程。留下记录文件和一对 `sleep`（一个在适配器组，一个自成一组）。
- 再跑一次 `a_stop`，这对 `sleep` 被清掉，记录目录变空。

## 修复后的结果

- **并行负载下的 K21**：只跑 `it_k21` 的 4 条用例，连跑 1,000 次，0 次失败，用时 108 s。
  - 负载：同时有两路全部 lib 测试（跳过 K21）在循环跑，这期间跑完 20 轮，没有失败。
  - 此前还连跑过 300 次，也是 0 次失败。
- **全部 `cupper-host` 测试**（`cargo test -p cupper-host --features test-harness`）：跑完后，没有新增 `ppid` 为 1 的 `sleep` 或假 agent。
- **完整 CI**（`cargo xtask ci pr`）：跑完后，本 worktree 没有留下任何 `ppid` 为 1 的测试进程。

检查命令（`ppid` 为 1 的测试相关进程，附工作目录，用来区分是哪个 worktree 留下的）：

```sh
ps -A -o pid=,ppid=,pgid=,etime=,command= |
  awk '$2==1' | grep -E '/bin/sleep|cupper-(fake-agent|host|elevate-probe)' |
  while read pid ppid pgid etime cmd; do
    echo "$pid $pgid $etime $cmd cwd=$(lsof -a -d cwd -p "$pid" -Fn 2>/dev/null | sed -n 's/^n//p')"
  done
```

## 范围外的发现（没有修改）

1. **IT 留下的宿主进程**：
   - `tests/lifecycle.rs` 的用例在 `terminate(&host)` 之前失败时，拉起的 `cupper-host` 进程不会被结束，连同它的假 agent 一直运行。`spawn` 返回的是 `std::process::Child`，drop 时不结束子进程。
   - 本机现在就有：别的 worktree（T043、T054）留下的 12 个 `ppid` 为 1 的 `cupper-host`，最久的已运行半小时以上，来自 `it_recording_export_*` 等用例。本 worktree 早先失败的 `it_elevate_*` 和 `it_seed_*` 也留下过 6 个，已核对命令行（本 worktree 的二进制、测试临时数据目录）后逐个发 TERM 结束。
   - 这属于 T063 的范围，建议宿主进程也用 drop 守卫结束。
2. **`it_elevate_lineage_waits_limits_expiry_grant_and_close` 本地单独跑时的失败**：
   - 原因：它要先有带 `test-harness` 特性构建的 `cupper-elevate-probe`，CI 的 rust 作业会先构建它。我在本地只跑了 `cargo build -p cupper-cli --bins`，没带这个特性，所以失败；main 上同样如此。
   - 这不是缺陷。CI 跑完之后，本地再跑就通过了。
   - 但它失败时会留下宿主进程，见上一条。
