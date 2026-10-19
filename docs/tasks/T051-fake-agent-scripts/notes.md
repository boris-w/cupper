# T051 工作笔记

## 规模估计（开工前）

上限约 1,500 行（含测试，不含 `Cargo.lock`）。按模块估计：

| 模块 | 内容 | 估计 |
|---|---|---|
| `Cargo.toml`、`src/main.rs` | 入口、stdio 行传输、原始行的解包 | 90 |
| `src/config.rs` | 启动配置：persona、登录状态、故障注入、进程分段，含单测 | 90 |
| `src/persona.rs` | 两种 persona 的形态数据（能力、configOptions、命令列表、权限选项、计划和错误的形态） | 220 |
| `src/agent.rs` | SDK 的 agent 角色：会话状态、全局不变式、steering、崩溃态、`_meta` 回显 | 230 |
| `src/script.rs` | 剧本解析和 38 条命令 | 430 |
| `tests/common/mod.rs` | 子进程管道上的最小 ACP 客户端 | 110 |
| `tests/*.rs` | UNIT-fake-scripts：每条命令至少一个用例、persona 形态、原始行、不变式 | 330 |
| 合计 | | 约 1,500 |

估计贴着上限。超出 10% 以上（约 1,650 行）时停下来汇报，拆分建议是"persona 与启动配置"和"新增剧本命令"分开。

## 进度统计

### 第 1 次（源码草稿完成，还没写集成测试）

源码一次写完，`cargo build`、`clippy -D warnings`、`fmt` 都通过，`config` 的 2 个单测通过；stdio 冒烟（initialize → session/new → 带 `raw`、`echo_meta`、`terminal` 的剧本）输出正确。没有提交，草稿留在 worktree 里。

`cargo fmt` 之后的行数：

| 文件 | 行数 | 说明 |
|---|---|---|
| `Cargo.toml` | 19 | |
| `src/main.rs` | 75 | |
| `src/config.rs` | 159 | 含 2 个单测（约 35 行） |
| `src/agent.rs` | 357 | |
| `src/persona.rs` | 590 | 其中静态形态数据约 250 行：rustfmt 把选项表的每个元组拆成多行 |
| `src/script.rs` | 711 | |
| 合计 | 1,911 | 不含集成测试 |

估计偏低的原因：写草稿时按紧凑写法估的行数，rustfmt（宽度 100）展开后膨胀了约 60%，persona 的选项表最明显。

**预计总量**：源码约 1,770 行（把 persona 的静态数据改成 `personas/*.json` 能省约 140 行）+ 集成测试约 420 行（最小客户端约 100 行，约 40 个用例）= **约 2,200 行，超出上限约 45%**。按 brief 和守则第 8 条，停下来汇报。

按"persona 与启动配置"和"新增剧本命令"拆分时的估计：

| 部分 | 内容 | 源码 | 测试 | 合计 |
|---|---|---|---|---|
| A | 迁移、启动配置、两种 persona、agent 角色（会话、全局不变式、steering 的 persona 默认行为）、原始行通道、`_meta` 回显、SP-4a 的 16 条命令、`echo_env` | 约 1,340 | 约 300 | 约 1,650 |
| B | 5.9.2 新增的其余 18 条命令：`flood_rate`、`terminal`、`diff`、`write_file`、`plan_update`、`plan_confirm`、`form`、`steer_*`、`hang`、`late_tool`、`autonomous`、`crash_cli`、`output_then_crash`、`restartable`、`upstream_retry`、`thread_status`、`auth_none` | 约 430 | 约 220 | 约 650 |

A 单独也略超 1,500，主要是两种 persona 的形态数据和 SDK 适配占了大头。
