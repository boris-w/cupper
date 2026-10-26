# T093 修复 1 汇报

- 执行者：子 agent（T093 的执行者）
- 分支：`fix/T093-lock-test-flake`
- 完成日期：2026-10-26

**结论**：按裁定 (a) 改完了：同样负载下连跑 200 次 0 失败，修复前是 100 次 18 失败。带记录的 CI 两次都只差 license 作业，没有可提交的摘要：
- 第一次连不上 github，拉不到 RustSec 漏洞库；
- 第二次是你提醒的 RUSTSEC-2026-0382（`rustls-webpki 0.103.15`）。

T178 还没进 main：main 的 `Cargo.lock` 仍是 0.103.15。请把 main 合进来之后再跑。

## 做了什么

1. **`testutil::alone`**：xtask 的 `testutil` 里新增，做法照 `cupper-sys`。`unit_ci_lock_command_passes_its_exit_code_and_is_told_it_holds_the_lock` 改为只在单独的测试进程里跑完整的用例体，去掉主要来源：同一进程里别的测试拉起进程，会在 exec 前握着锁 fd 的副本。
2. **释放检查有上限**：
   - 这条用例和 `unit_ci_lock_released_when_holder_is_killed`，都改为调用 `freed()`。它在另一个线程里阻塞取锁，主线程用 `recv_timeout` 最多等 `RELEASE_LIMIT`（1 s）；超时才失败，信息为 "the lock outlived … by more than 1s"。
   - 不用 `Instant`、`thread::sleep`，因为 xtask 的 clippy 规则禁止这两个。
3. **注释**：`RELEASE_LIMIT` 的注释写明为什么要等：
   - 被等待回收的进程之后，单独进程里也偶有几毫秒的滞后；
   - 探针数据引自 `notes-fix1.md`：约 20,000 次里 2 次，最长 2.6 ms；不拉起进程时没有出现；
   - 机理未查清；
   - 生产上只是下一个等锁的人晚几毫秒拿到锁。
4. **`notes-fix1.md`**：补上裁定、修法、行数和修复前后的连跑数据。

## 关键取舍

- 按裁定，"短时限内等待"在这里是例外：单独进程去不掉单进程里的滞后，不是在掩盖问题。
- 上限取 1 s。实测滞后最长 2.6 ms，余量足够；真出问题时也能很快报出来。

## 规模

只改测试，增 62、删 9：`lock.rs` 33/8，`testutil.rs` 29/1。本分支没有 `cargo xtask size`，按 `git diff --numstat` 手工统计。

## 验证方式

- **连跑**：整套 xtask 单元测试 4 份同时跑，另有 2 份整套在后台一直循环，负载约 10–11。

  | 二进制 | 次数 | 失败 |
  |---|---|---|
  | 修复前，重测基线 | 100 | 18，全是这条用例 |
  | 修复后 | 200 | 0 |

- **检查**：xtask 单元测试 39 个全部通过；clippy、fmt 通过。
- **带记录的 `ci pr`**：rust 2001、protocol 185、ts 473、testdata-scan 3 都通过。ui、e2e 两个作业因为没有改到它们依赖的路径而跳过。
  - license 第一次失败：连不上 github。删掉摘要重跑一次。
  - license 第二次失败：RUSTSEC-2026-0382，`advisories FAILED`，其余 bans、licenses、sources 都正常。
  - 两份失败的摘要都已删除，没有提交。工作区干净，没有我起的进程留着。

## 提交列表

```
f18204c docs(tasks): T093 fix 1, the ruling and 200 runs without a failure
e677e07 test(xtask): the lock tests no longer fail now and then on its release
3ee5f6c docs(tasks): T093 fix 1 findings on the lock test that fails now and then
```

## 遗留问题

- 等 T178（升级 `rustls-webpki` 到 0.103.16）进 main 后，请把 main 合进本分支，再跑 `cargo xtask ci pr --record docs/tasks/T093-e2e-interaction-project/ci/`。需要我来跑就告诉我。
- 其他偶发失败：连跑里没有见到。
