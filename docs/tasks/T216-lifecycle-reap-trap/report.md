# T216 汇报（更新版，取代上一版）

- 执行者：子 agent（Rust 测试工程师）
- 分支：`fix/T216-lifecycle-reap-trap`
- 完成日期：2026-11-02

## 做了什么

- 只改了测试 `crates/cupper-host/tests/lifecycle.rs` 里的 `it_lifecycle_startup_ends_what_the_last_run_left_running`：
  - 起子进程的闭包把 stdout 接成 `cupper_sys::Stdio::Pipe`。
  - 忽略 SIGTERM 的 tool 脚本改成 `trap '' TERM; echo ready; /bin/sleep 60; :`。
  - 测试读到 `ready\n` 之后，才登记 adapters 表、起宿主。读完就丢掉读端；sh 和 sleep 之后都不写 stdout。
  - 写法和 `cupper-sys/src/proc.rs` 一样：先 trap，再 echo ready。
- 顶层 import 改成 `use std::io::{BufRead, BufReader, Write};`。文件后面 REPL 用例的线程闭包里有一句局部的 `use std::io::BufRead;`，有了顶层 import 就多余了，一并删掉。
- 没有改宿主的清理逻辑。
- 查同类写法：在 `crates/`、`apps/`、`packages/`、`tools/`、`testdata/` 里搜了 trap、SIG_IGN、忽略 SIGTERM 等写法，只有这一处，别处不用改。
  - `cupper-sys/src/proc.rs` 那一处本来就有就绪握手。
  - fake agent 的 `--fault linger` 先装好 SIGTERM 处理器，再开始服务 ACP。测试是 `attach` 握手完成后才 stop，这个握手本身就是就绪信号。

## 关键取舍

- **原因只有一种可能**：sh 以信号 15 退出，只能是收到 SIGTERM 时它还是默认处理，也就是还没执行到 `trap`。其他情况的结果都不是 15：
  - 组的起始时间对不上、整组被跳过：tool 会在 60 秒后正常退出，结果是 `None`。
  - SIGKILL 提前发出：结果是 9。

  所以修法定为就绪握手，不考虑加延迟或调 `reap_term_wait`。
- **adapter 不需要握手**：它在 exec 前后收到 SIGTERM 都是默认处理，结果都是 15。
- **两个子进程都接管道**：adapter 的 stdout 也接成了管道，只是不读。这样仍然共用一个闭包，改动最小。sleep 不写 stdout，不会有 SIGPIPE。
- **同步读放在 tokio 测试里没问题**：`read_line` 是阻塞读，但发生在起宿主之前，那时运行时上没有别的任务要推进。原用例里的 `wait()` 本来也是同步的。

## 验证方式

**本机（编排者放行后，单份）**

- 直接证明原因：在 trap 前人为加延迟。这只是探针，没有提交，跑完已经还原。
  - 修前写法改成 `/bin/sleep 5; trap '' TERM; /bin/sleep 60; :`：失败，`lifecycle.rs:119` 报 left `[Some(15), Some(15)]`、right `[Some(15), Some(9)]`，耗时 1.33s。和 CI 上的失败形态完全一样。
  - 修后写法改成 `/bin/sleep 5; trap '' TERM; echo ready; /bin/sleep 60; :`：通过，耗时 6.58s，测试等到了就绪才起宿主。
  - 补充：延迟先试了 `sleep 1`，修前也通过了。原因是本机 debug 构建的宿主从启动到清理大约要 1 秒多，trap 已经先设好了。改成 5 秒才稳定复现。
- 提交后的用例本身单跑一次：通过，耗时 1.67s。
- 规模：`cargo xtask size` 输出 `T216: +9 -3 (net +6); 31 left to the stop line 40, hard stop 60`。

**runner 上的对照**

公共参数：test-repeat，macos-15，`kind=cargo`，`package=cupper-host`，`args=--features test-harness --test lifecycle`。

| 运行 | 提交 | filter | 规模 | 结果 |
|---|---|---|---|---|
| 修前，单个用例 https://github.com/boris-w/cupper/actions/runs/41395611564 | main 977245c | 本用例名 | 200 × 3 | 600 过，0 败：单跑复现不了 |
| 修前，整个二进制 https://github.com/boris-w/cupper/actions/runs/41395614497 | main 977245c | `it_` | 100 × 3 | 297 过，3 败，3 次都是本断言（`lifecycle.rs:119`，`[Some(15), Some(15)]`） |
| 修后，单个用例 https://github.com/boris-w/cupper/actions/runs/41395876633 | 100ddf7 | 本用例名 | 200 × 3 | 600 过，0 败 |
| 修后，整个二进制 https://github.com/boris-w/cupper/actions/runs/41395945555 | 100ddf7 | `it_` | 100 × 3 | 299 过，1 败；败的是 recording-export，本断言 0 次 |
| 修后，整个二进制 https://github.com/boris-w/cupper/actions/runs/41399294080 | 100ddf7 | `it_` | 200 × 3 | 597 过，3 败；3 次都是 recording-export，本断言 0 次 |

- 修后整个二进制合计 900 次，本断言 0 次失败。修前是 3/300。
  - 如果修后的失败率仍是修前的 1%，900 次全不失败的概率约为 0.99^900 ≈ 1.2×10⁻⁴。
  - 修后那 4 次失败的日志里，本用例都是 `ok`。

**对照 brief 的验收标准**

- [x] **原因有证据**：runner 上修前整个二进制 3/300 失败，形态和 CI 的 41392847292、41377958324 一样；本机加延迟的探针能稳定复现，加了握手就消失。单跑 600 次都不失败，说明要靠同一二进制里其他用例并行带来的负载，和 T099 的分析一致。
- [x] **修后 runner 上 600 次全部通过**：单个用例 600/600。整个二进制合计 900 次，本断言 0 次失败。
- [x] **PR 的 8 个检查**：PR #34 https://github.com/boris-w/cupper/pull/34 ，ci 运行 https://github.com/boris-w/cupper/actions/runs/41395878435 ，8 个检查（license、protocol、testdata-scan、ts、compat-framework、e2e、rust、ui）全部通过。

## 提交列表

```
100ddf7 test(host): IT-lifecycle starts the host once the tool's trap is set
```

## 遗留问题 / 范围外发现

1. **另一个不稳定用例**：`it_recording_export_two_instances_read_back_and_sanitize`（`lifecycle.rs` 约第 371 行）在修后整个二进制 3 份并行的 900 次里失败了 4 次（41395945555 copy 3 run 10；41399294080 copy 1 run 6、copy 3 run 49、copy 1 run 184）。
   - 4 次形态相同：`common/mod.rs:265` 的 `until` 等满 PATIENCE（30 秒）后报 `SELECT status FROM turns ORDER BY turn_no: not ["completed", "failed", "completed"]`，二进制耗时约 32.9 秒。
   - 也就是说，adapter `exit 0` 之后要重启，第三个 turn 在负载下 30 秒内没有完成。
   - 修前整个二进制那 300 次里它没有失败，但样本小，看不出和本修改有关。本修改只动了另一个用例里子进程的启动顺序。
   - 和 T216 无关，我没有动它，建议另开任务查：是重启退避或首个 turn 在负载下太慢，还是 30 秒的 PATIENCE 不够。
2. runner 物料和本机探针的临时文件都已清理，没有提交到分支；失败行已摘进本汇报。runner 上的物料保留 14 天。
