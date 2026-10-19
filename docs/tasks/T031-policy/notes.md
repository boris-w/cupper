# T031 工作笔记

## 行数估计（开工前）

### 模块划分

`crates/cupper-host/src/policy/` 下全是纯函数，不做 I/O；时间（`MonoTime`）作为参数传入。`lib.rs` 只加一行 `pub mod policy;`。

| 文件 | 内容 |
|---|---|
| `mod.rs` | 模块文档（两级判定、各入口由谁在哪一步调用）；拒绝结果 `Denial`（thiserror）和它到 `RpcError` 的映射；`has(principal, scope)`、`is_window` |
| `method.rs` | 方法级：scope；`CliOnly`（`auth/elevate` 只给 `cli`）；SEC-9 按方法名收窄的清单；`settings/update` 的安全键（解构 `SettingsPatch`，新增键编译不过）；`session/set_config` 的 SEC-9 收窄（手机只能改模型、推理强度，权限模式交给档位规则） |
| `tier.rs` | 档位规则：档位的序、`ceiling`（principal 最高能设定的档位）、`tier_change(from, to)`；选项的 `mode_after` / `tier_after` 换算成目标档位；未知档位按 `full_access` |
| `input.rs` | 输入证据：`InputEvidence` 的 kind / key / digit 一致性；`git/revert`（window 必须带证据）；`interaction/respond`（选项存在、`requires`、`mode_after` 的档位、window 的证据、escape 只用于拒绝类、`digit` 等于选项序号、400 ms） |
| `attach.rs` | 信任门控的钩子：按 agent 和信任状态决定加载项目配置、只加载用户配置，还是 `unavailable/project_untrusted`；何时调用归 T044 |
| `describe.rs` | `policy/describe`：解析 `{method, params}` 成可确认的命令；`Facts`（档位、待回答请求的选项、运行中的回合数，由调用方查好）；7 种 `action` 各自的 `details` 键 |

### 估计

按 T013、T024 的密度（文档注释约占代码的 30%；rustfmt 会展开 `json!` 和长参数列表）：

| 部分 | 代码 | 测试 | 说明 |
|---|---|---|---|
| `mod.rs` | 95 | — | `Denial` 7 个变体、映射、两个小助手 |
| `method.rs` | 115 | 70 | 测试：SEC-9 清单、`set_config` 的选项、安全键（含"每个设置键都已分类"） |
| `tier.rs` | 70 | 120 | UNIT-tier-rules：5.8.2 的表逐格（收紧、同档、放宽到 elevated、放宽到 full_access）× 8 种 principal，加 `mode_after` 和未知档位 |
| `input.rs` | 135 | 160 | 证据形状、escape、`digit` 与序号（特权和效果未知的选项也占序号）、400 ms 用传入的 `MonoTime` |
| `attach.rs` | 30 | 20 | |
| `describe.rs` | 165 | 100 | 7 种 action、各自的 `details`、不需要确认的情况 |
| `testkit.rs`（测试辅助） | — | 60 | 各列 principal、选项和请求的构造 |
| `matrix.rs`（UNIT-policy-matrix） | — | 190 | 宿主侧对照表（出处：5.8.3 与 `cupper-protocol` `catalog.rs` 的 `MATRIX`）、逐格判定、覆盖目录里的每个方法、表里没有单列的资源行（安全键、特权选项） |
| **合计** | **约 610** | **约 720** | **约 1,330** |

- 高于计划的 M（400–900 行），低于约 1,500 行的上限。超出的原因：测试按要求覆盖每条拒绝路径，逐格矩阵和档位表本身就长（计划 1.1 的经验：测试占 40%–55%）；`policy/describe` 的 7 种 action 各要一组用例。
- **停止线**：每完成一个模块统计一次。按已完成模块的实际偏差推算，总数预计超过约 1,450 行时，停下来汇报。
- 压缩手段：宿主侧对照表一格一个字母（`"YMYYYYY"`），一行一个 5.8.3 的行；测试用表驱动。

## 实际行数
