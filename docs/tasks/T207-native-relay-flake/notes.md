# T207 工作笔记

- 执行者：子 agent（Rust 工程师）

## 开工前的规模估计

| 项 | 估计 |
|---|---|
| 诊断探针：用例照原样判定，另把每次的计时写进 `test-repeat/` 目录，供 runner 上修前的连跑；连跑后整个 revert，净增 0 | 0（过程中约 +120） |
| 修复（根因在测试）：`relay.rs` 的测试 | 10–30 |
| 修复（根因在中继）：`relay.rs` 本体，加一条稳定复现的用例 | 60–120 |
| **合计** | **10–120** |

在停止线 150 之内。

## 失败现场

- PR #22，run 40661950733 第 1 次运行，rust 作业的 `test-native-release` 步骤：`cargo test -p cupper-desktop-native --locked --no-fail-fast`，不带特性。
- 二进制刚链接好（`Compiling cupper-desktop-native`，16.68 s），这是它的第一次运行；步骤共 26.7 s，等于编译加 10.01 s，所以启动二进制本身没有慢。
- 7 个用例，完成的先后：`launch_without`、`launch_writes`、`fds`、`closing`、`token`、`check`，最后是 `forwards ... FAILED`。`finished in 10.01s`。
- panic：`relay.rs:182:39`（`drain` 里的 `read_to_end`），`Os { code: 35, kind: WouldBlock }`，也就是读满了 10 s 的 `SO_RCVTIMEO`。
- 同一作业第 2 次运行，整个二进制 `finished in 0.02s`。
- 所以失败的那一次，用例在二进制开始后约 10 ms 内就走到了 `drop(host)` 和 `drain`，之后 10 s 里主进程一端没有读到 EOF。
- 翻了近期失败的 19 次 CI（含 nightly）的 `--log-failed`，没有别的中继失败。
