# T064 评审

- 评审人：编排者
- 日期：2026-10-26
- 结论：**通过，合并**

## 核对

- **规模**：+1,126 −8（不含 lock 文件、`testdata/`、`docs/`），在收窄后的停止线 1,150 以内。开工前估约 2,200，执行者在写代码前停下，编排者裁定拆出 T187、T188（v1.3.55）。中途合计到过 1,155，越线 5 行，执行者收紧写法后降回线内。
- **CI**：执行者的摘要 `pr-eb12d67291c5.json`、`compat-framework-0f4212edc59d.json` 都通过。合 main 时 `cupper-host/Cargo.toml`（T069 的 `crash_concurrency` 测试目标）与 `xtask/src/main.rs`（T186 的 `git-lfs` 子命令）有冲突，编排者取并集，在合并后的提交上重跑了带记录的 `ci pr`（`pr-0fb5bca866fe.json`，全部通过）。之后 main 又进了 T016、T117，再次合并没有冲突；编排者跑了全工作区的编译检查、xtask 的 clippy 与单测、`gold_b` 和 `ci compat-framework`，都通过。T117 那条"未实现的作业被拒绝"的用例已在 T117 里改用 `release-config`，不受 compat-framework 实现的影响。
- **GOLD-B**：normal-turn、approval 归一化后与入库的相同；cancel 剧本改成"命令执行中取消"，录制与金样例相应更新。执行者把脚本临时换回 T065 的版本跑过一次，结果与入库的相同，能说明生成器本身没有偏差。
- **结构断言**：执行者把几项检查临时改成相反的期望，都按预期失败。

## 逐项意见

1. **执行者不是 `harness = false` 目标，而是普通测试目标里一个忽略的测试 `compat_driver`**。与第一点确认有出入，认可：`harness = false` 的二进制会被 `cargo test --workspace` 不带参数地运行，rust 作业的过滤词也会被当成参数传进去。现在只有 xtask 用 `--ignored --exact compat_driver` 并设好 `CUPPER_COMPAT_PLAN` 时才跑。
2. **剧本表只在执行者里，xtask 只认报告与 `run.json`**：步骤只定义一处；T188 在 ubuntu 上做校验时不必编译宿主。
3. **报告校验分三步**（严格反序列化、语义检查、schema 文件不过期），照第二点确认。schema 的键排好序，避开了工作区里 `preserve_order` 造成的键顺序差异，符合"导出给其他语言的产物不依赖 map 键顺序"。
4. **`--write` 的边界**：只写三个 gold-b 剧本；没有敏感词清单时在跑任何东西之前报错；脱敏后有残留就失败；内容没变时不重写文件。
5. **真实模式一律报错、不留空壳**：真实提示词、权限模式、自我约束都留给 T187，没有写没人用的代码。报告的 `config_files`、`cli_sessions` 先占好位置，语义检查要求真实模式必须有配置哈希，T187 不用再改 schema。
6. **剧本 panic 与超时**：每个剧本在独立的 tokio 任务和临时目录里跑，panic 记失败、不影响后面的剧本；步骤等待 30 秒超时记"不确定"，按 `attempts` 重跑，符合 5.9.5。

## 记下的点

- **compat-framework 还没进 `ci pr`**。设计 5.1.6 要求改动兼容套件的 PR 也跑这个作业。交给 T188，照 e2e 的做法按路径判断。
- **T187**：
  - 每个剧本的真实提示词、权限模式、准备文件与自我约束；
  - `session/create` 带显式权限模式后，三份 B 层录制的 `config` 会变，用 `--fake --write` 重新产出，差异在汇报里说明；
  - cancel 剧本在真实 CLI 上是"允许后等 1 秒再中断"，`sleep 60` 没有输出，可靠性要在 T066 实际跑时确认。
- **T188**：读报告用 `report::parse` 加 `report::problems`；schema 在 `testdata/compat/report.schema.json`；`testdata_scan.rs` 的 `WORD_LIST`、`word_list` 已改成 `pub`。
- **偶发卡死**：`it_orch_trust_gating_two_reasons_hold_until_both_go` 卡了 28 分钟，已加进 T189 作为第三处。
- **`tests/ids.toml`**：以 main 为准，T064 段只有 UNIT-compat-report，UNIT-compat-guards 在 T187 段。
