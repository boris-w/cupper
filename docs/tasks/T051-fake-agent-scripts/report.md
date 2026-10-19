# T051 汇报

- 执行者：子 agent（Rust 工程师，测试基础设施）
- 分支：`feat/T051-fake-agent-scripts`
- 完成日期：2026-10-19

## 做了什么

T051 已完成：新建 `crates/cupper-fake-agent`，36 个集成测试和 4 个单测全部通过，`cargo xtask ci pr` 在合入 main 后的提交上通过。最终 3,050 行，比你接受的 2,776 行多出约 274 行（JSON 格式化 255 行、thiserror 19 行），见"规模"一节。

在 `crates/cupper-fake-agent` 建了假 agent，基础是 SP-4a 的版本，形式是 lib + bin。它用 SDK `=3.3.0` 的 agent 角色实现，按 persona 模拟 Claude 或 Codex 的线上形态。

- **启动配置**：只经命令行传入。宿主的环境变量规则会过滤掉我们自己的变量，所以不用环境变量。参数：
  - `--persona claude|codex`；
  - `--auth logged-in|logged-out`；
  - `--fault <名称>[@<段号>]`，可选 `exit_on_start`、`initialize_hang`、`set_config_error`、`resume_error`；
  - `--segment <n>`；
  - `--segment-file <路径>`：每次启动段号加 1，重启后的 agent 接着演下一段，供 T052 回放多实例录制。

  错误类型是 `config::ConfigError`（thiserror，带原因链）。
- **persona**：静态形态放在 `personas/claude.json`、`codex.json`，用 `include_str!` 加载，都取自 SP-1 录制（claude-agent-acp 0.91.0、codex-acp 2.2.2）。包括：
  - `initialize` 的结果，`steering.supported` 在顶层 `_meta`；
  - 已登录时的 `_auth/status_update`；
  - configOptions 和默认模式（`modes` 由 `mode` 选项推出），Codex 还带旧的 `models`；
  - 斜杠命令；
  - 按工具类型区分的权限选项 ID，和计划确认的选项。

  带 ID、路径或文本的形态由 `persona.rs` 构造：表单、终端、diff、threadStatus、上游重试和几种错误。
- **agent 角色**：请求不走类型化的处理器，以保留录制里的原样形态；参数仍经 SDK 的类型解析。每个响应之后按 persona 发后续通知：
  - `initialize` 后发 `_auth/status_update`；
  - 新建、恢复或加载会话后，以及每个回合开始时，发命令列表；
  - Codex 的回合用 active/idle 两个运行状态首尾包住。

  按 persona 区分的行为：
  - 未登录时，Claude 是 prompt 返回 `-32000`，Codex 是 `session/new` 就返回 `-32000`；
  - 拒绝审批：Codex 的回合结束（#556），Claude 继续；
  - 回合取消时有审批挂起：Claude 用 `$/cancel_request` 撤回，Codex 不撤回；
  - 空闲时插话：Claude 遵守 `idleBehavior`，Codex 忽略。
- **38 条剧本命令**：SP-4a 原有的 16 条，5.9.2 新增的全部命令，再加 `echo_meta`。`echo_meta` 回显最近一次新建或恢复会话时收到的 `_meta`，供 IT-trust-settingsources 确认 `settingSources: ["user"]`。完整的命令表在 `src/script.rs` 的模块文档里。
- **原始行通道**：`raw <json>` 写的那一行先装进一个标记通知，走 SDK 的发送队列，写 stdout 时再换回原样的那一行。这样它不经过 SDK 的类型层，在消息顺序里的位置也不变。JSON 里可以带 `;`，`${session}` 会换成会话 ID。
- **全局不变式**：同一个会话收到第二个在途的 prompt 时，在 stderr 写一行说明，以退出码 70 退出。在途标记在 prompt 的响应**之前**清除，宿主紧接着发下一个 prompt 不会被误判。
- **UNIT-fake-scripts**：单测 4 个，集成测试 36 个，用一个最小 ACP 客户端经子进程管道驱动，收发的都是原样的 JSON 行。覆盖每条命令、两种 persona 在线上的形态、启动配置、原始行通道和全局不变式。

## 关键取舍

- **请求不走类型化处理器**：响应要与录制完全一致，包括 `_meta`、unstable 的 `plan_update`、带 `args` 的 authMethods 等。SDK 的类型结构会丢掉不认识的字段，所以响应直接写 JSON；参数仍用 SDK 的 `parse_message` 校验，真实适配器拒绝的请求，假 agent 也拒绝。
- **原始行搭 SDK 的发送队列**：如果绕开 SDK 直接写 stdout，原始行可能和前后的消息乱序；装进标记通知走同一个队列，顺序就有保证。
- **启动配置只用命令行**：`test_config.agents` 本来就是程序路径加参数，环境变量到不了假 agent。
- **lib + bin**：每个分步提交都不会有 dead code，集成测试也能直接引用 `EXIT_SECOND_PROMPT`。
- **计时**：只经 `real_sleep` 和 `real_interval` 两个辅助函数，都带 `#[expect(clippy::disallowed_methods, reason = "the fake agent is an external process playing a real adapter in real time")]`。`flood_rate` 的测试要量真实耗时，也加了同样的豁免。没有 `unsafe`。
- **`write_file`** 拒绝绝对路径和 `..`，也拒绝经符号链接通到工作区以外的路径（按最深的已存在祖先目录的真实路径检查）。
- **`echo_env`** 只回显环境变量名，不回显值。

## 规模

- **最终 3,050 行**：`personas/*.json` 404 行（prettier 排版），`src/` 1,704 行，`Cargo.toml` 20 行，测试 922 行（客户端 299、用例 623）。
- **第 1 次裁定**：原定上限 1,500 行。草稿格式化后已有 1,911 行，推算总量约 2,200 行，我停下来汇报。你选方案 1，不拆分，上限放宽到约 2,300 行，条件是 persona 数据挪进 JSON、按主题分步提交。
- **第 2 次裁定**：测试补齐后是 2,776 行（测试实际 922 行，原先估约 420 行），我又停下来汇报。你选方案 A，按 2,776 行接受，并要求改用 thiserror。
- **之后的增量**：
  - 改用 thiserror：+19 行，到 2,795 行；
  - prettier：合入 main 后跑 PR 的 CI，`ts` 作业里的 prettier 检查覆盖全仓库的 JSON，标出了两个 persona 文件。用 prettier 排版后 JSON 从 149 行变成 404 行（+255），`jq -S` 比对内容不变，到 **3,050 行**。

  如果更想要紧凑的排版，也可以把 `crates/cupper-fake-agent/personas/` 加进 `.prettierignore`（根配置），JSON 能回到 149 行。我没有改根配置。

## 验证方式

- **合入 main 后**（`2ca7b6a`）在干净的 worktree 里跑 `cargo xtask ci pr --record docs/tasks/T051-fake-agent-scripts/ci/`：
  - 第一次 `ts` 作业失败，只有 prettier 一项，原因是上面的 JSON 排版，摘要没有入库；
  - 修正后在 `7b13b1c` 重跑，全部通过：rust 444/0，ts 64/0，license 1/0；protocol、testdata-scan、ui、e2e 还没实现。
  - 摘要是 `docs/tasks/T051-fake-agent-scripts/ci/pr-7b13b1c4cfa8.json`，其中 `UNIT-fake-scripts` 为 pass。
  - `cupper-sys` 的偶发失败这次没有出现。
- **分步提交逐个核对**：每个提交都用 `git archive` 导出（`tar -m` 刷新文件时间，避免 cargo 复用旧产物），单独跑 `cargo build`（0 告警）、`clippy -p cupper-fake-agent --all-targets --all-features -D warnings`、`fmt --check`、`cargo test -p cupper-fake-agent`，全部通过。中间各步提交前，还临时放入最终的集成测试，挑当时已有的命令跑过一遍（不随提交入库）。
- **对照验收标准**：
  - UNIT-fake-scripts：38 条命令每条至少一个用例；persona 的选项 ID、能力和 configOptions 有两个单测与 SP-1 比对，期望值来自 `notes.md` 里的 jq 摘取命令，另有线上集成测试；原始行通道和全局不变式各有用例。✅
  - `echo_env` 只有变量名没有值：用例里设置一个带值的变量，断言名字出现、值不出现。✅
  - `cargo xtask ci pr --record` 通过，摘要已入库。✅
  - 测试名以 `unit_fake_scripts_` 开头。✅
- 提交和入库文件都扫描过，没有本机路径、用户名或邮箱。

## 提交列表

```
d03f72b docs(task): record the T051 pr CI summary
7b13b1c chore(fake-agent): format the persona data with prettier
2ca7b6a Merge branch 'main' into feat/T051-fake-agent-scripts
c00a2b1 docs(task): record the T051 commit plan and final line counts
bea71f0 test(fake-agent): add UNIT-fake-scripts over the agent's stdio pipes
a9bcade feat(fake-agent): add the raw line channel and echo_meta
8ae6faa feat(fake-agent): add the timing, failure and notice commands
a759c15 feat(fake-agent): add the plan, form and steering commands
1d08346 feat(fake-agent): add the output, tool and file commands
4e72b4c feat(fake-agent): port the SP-4a script commands
3721b6a feat(fake-agent): serve the ACP agent role with the K1 invariant
8c870db feat(fake-agent): add the Claude and Codex personas
c4eb6a0 feat(fake-agent): add the crate with its start configuration
5b7eb5e docs(task): record the T051 line count after tests and the jq extraction commands
b4aa01d docs(task): record the T051 size estimate and the projected overrun
```

## 遗留问题 / 范围外发现

1. **根配置**：`Cargo.toml`、`deny.toml`、`.prettierignore` 都没改。依赖直接写在本 crate 的 `Cargo.toml`：`agent-client-protocol =3.3.0`、`futures`、`tokio`、`serde_json`、`thiserror`、`anyhow`（`anyhow` 只用在 `main.rs`）。`Cargo.lock` 由你合入时取了 main 的版本，SDK 的派生宏随 T050 统一为 3.3.0。
2. **persona 中替换掉的内容**：
   - 模型列表删减，描述改成通用文字（录制里的描述带有本机网关的名称）；
   - 账号信息是 `user@example.invalid` 和 `Example Org`；
   - Codex 已登录时用录制里的通用 `gateway` 形态，用 ChatGPT 登录时的真实形态没有录到。
3. **persona 与录制的自动比对**由你转给 T052。jq 摘取命令在 `notes.md` 里，可以直接用。
4. **规模估计的教训**：rustfmt 会让紧凑写法膨胀约 60%；子进程测试的客户端也比预想的大。后续任务估行数时，建议按格式化后的写法估，并把测试客户端单独列一项。
5. **宿主测试怎么拿到二进制路径**：假 agent 是独立的二进制。宿主的集成测试（T055 等）需要一种方式拿到它的路径，cargo 不支持跨 crate 的 `CARGO_BIN_EXE_*`。建议在 T055 里定一个做法，例如测试前先 `cargo build -p cupper-fake-agent`，再按 target 目录推出路径。
