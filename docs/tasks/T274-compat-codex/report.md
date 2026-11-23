# T274 汇报

- 执行者：子 agent（CI 与工具）
- 分支：`test/T274-compat-codex`（worktree T052）
- 完成日期：2026-11-23
- PR：#136 https://github.com/boris-w/cupper/pull/136 。8 个检查都通过，运行在 https://github.com/boris-w/cupper/actions/runs/45148384370

## 做了什么

- **`cargo xtask compat`**：
  - 加 `--agent claude|codex`，默认 claude，agent 写进 plan。Codex 默认不传模型，用作者自己的默认模型；推理档位 low。
  - `--out` 接受相对路径，按 xtask 运行时的目录解析。以前会落到 `crates/cupper-host` 下面。
  - 配置比对只剩 Claude 原有的几项，删掉了 `.codex/config.toml`。Codex 运行一个文件都不读：`--dry-run` 会说明要用备份比对 `~/.codex`，报告里 `config_files` 为空。
  - 报告的校验改为：真实运行时，Claude 必须有配置比对，Codex 必须没有。schema 只改了说明文字。
- **运行器（`tests/compat.rs`）**：
  - agent 贯穿建会话、等运行时装好、杀 CLI（按进程名 `claude`/`codex`）、报告头、自我约束和假 agent 配置（Codex 加 `--persona codex`）。
  - 新增两个步骤：
    - `Plan`：用 `session/set_config` 打开 Codex 的 `plan_mode`；
    - `Started`：等一个工具调用开始。
  - 新增检查 `Refused(reason)`：会话被拒时，运行器检查被拒原因，并检查没有留下标记文件。
  - 假 agent 的 Codex 人设，CLI 版本记为 `fake-codex`，录制和 Claude 的 `fake` 分开放。
- **剧本**：每个剧本都有 Codex 版。12 个各写了内容，8 个只换模式（`read-only`）。
  - prompt 点名用哪个工具：写文件只许用 apply_patch，读文件用 `cat`。
  - 计划（Q25）和提问剧本先进计划模式。Codex 选 revise_plan 后回合以 completed 结束。
  - 取消和 close-running 等工具调用开始后再动作。close-running 的命令放在单独的会话里（setsid），照 T102 的实测。
  - crash-resume 和 resume-procs 用 app-server 的 restartable（K7）。
  - 未信任项目：检查会话建不起来（`project_untrusted`），也没有 `mcp-ran`。
  - hooks-reload（Q28）和 config-scope（Q34）用 `.codex/config.toml` 加 `hooks.json`，只限真实运行。
  - Q8 两个并发剧本照用原来的 prompt。
- **`gold_b.rs`**：
  - 按录制头里的 `cli` 选 agent；`config_of` 补上 `mode` → `permission_mode`，以及 Codex 的 `reasoning_effort`。
  - 测试表加了 `gold_b_fake_codex_cancel`，录制和金样例在 `cupper-fake-agent/0.0.0/fake-codex/cancel`；这个名字仍归 GOLD-B-fake，不新增编号。
  - 金样例里有 `may_still_run: true`，说明中断不会停掉 Codex 的命令（K3）。
  - T113 录到真实的取消场景后，加一行 `gold_b_codex_...` 就行。
- **单元测试**：每个剧本的 Codex 版都过自我约束；各 agent 跑各自的版本；`--agent` 的解析；报告对 Codex 配置比对的校验。

## 关键取舍

- **Codex 版的写法**：每个剧本带一份 `Codex` 覆盖，`None` 就沿用 Claude 的写法，由 `of(agent)` 合成。没有另写一整套剧本表，免得 id 和标题重复。
- **计划模式**：`session/create` 不能带计划模式，所以计划和提问剧本先发一句 ready，再设计划模式，然后发真正的 prompt。只开放 `plan_mode` 这一个选项，避免绕过自我约束去放宽档位。
- **会话被拒**：单独一个函数处理（约 25 行），宿主仍要正常退出。不是剧本预期的拒绝，一律判失败。
- **配置比对**：Codex 运行直接关掉，报告里不写替代项，由作者用 `cmp` 比对后回报。
- **假 agent 的 Codex 录制**：CLI 版本记为 `fake-codex`。这样已入库的 Claude 假录制不用重录，只是两边命名不对称。
- **Codex 取消场景的假脚本**：用 `wait_cancel`，不用 `late_tool`。后者在回合结束后才有输出，导出录制的时机会让结果不稳定。

## 验证方式

| 验收标准 | 结果 |
|---|---|
| 用假 agent，`cargo xtask compat --agent codex` 的各剧本都能跑通 | 18 个通过；steer 不适用；hooks-reload、config-scope 只限真实运行 |
| Claude 那边的行为不变 | `cargo xtask compat --fake` 全部通过；已入库的录制和金样例没有变化 |
| 单元测试覆盖 agent 参数的贯穿和 `gold_b.rs` 的选择 | compat 4 个（含新增的"每种 agent 跑各自的版本"）；xtask compat 11 个；`gold_` 13 个，含 `gold_b_fake_codex_cancel`，选错 agent 时金样例里的 `agent` 对不上 |
| PR 的 8 个检查 | 都通过：compat-framework、e2e、license、protocol、rust、testdata-scan、ts、ui |

**其他验证**：
- `--real --agent codex --dry-run`：每个剧本的 Codex 版都过了自我约束。
- `--out target/x`：落在工作树里。
- 遍历录制的 protocol、persona、gold_a 测试都通过。
- clippy `-D warnings`、fmt 通过。
- 录制入库前，`--write` 跑过 sanitize 和 scan：clean。

**T203 的拉起方式**：用假 agent 跑过一次。
- 做法：本机临时把真实模式的两个 agent 都指向假 agent，然后运行 `--real --agent codex --only new-session --only host-restart --only concurrent-two`。拉起用的是 e2e 宿主包，按 `start` 的方式、带用户的 HOME 和登录 shell。
- 结果：
  - 三个剧本都通过，回复来自假 agent，没有碰到真实的 CLI；
  - 宿主发了 mode = read-only、reasoning_effort = low，没有发模型；
  - 跑完还原了临时改动，`git status` 干净。

## 提交列表

```
46f85204 test(gold-b): replay a recording as the agent of its CLI
d143ba8c test(compat): the suite plays Codex as well as Claude
```

## 遗留问题 / 范围外发现

- **规模**：+456 −78（净 +378），越过了原硬线 420，你已把停止线放宽到 470/500。我没有每完成一块就统计一次，越线后才发现，这是我的问题。

**给 T113：Codex 崩溃方式的发现**
- Codex 的"CLI 崩溃"是 app-server 被杀（K7，`1001`、`restartable`）：
  - 适配器还在；那个回合失败，出现一个 error 条目（guidance 是 retry）；
  - 下一个请求时适配器重启 app-server，回合正常完成；
  - 所以没有 `session_resumed` 这个 notice。
- 真实运行时，`KillCli` 杀的是进程名以 `codex` 结尾的进程（app-server）。下一个 prompt 应当以 restartable 失败，再下一个正常完成。这个顺序要实测确认。

**给 T113：真实运行时要核实的假设**
1. **提问剧本**：request_user_input 应该只在计划模式下才有。答完之后，模型可能还会提出计划，回合就停不下来。
2. **计划剧本**：选 revise_plan 后回合应当以 completed 结束（T102 实测是 end_turn）。计划正文能否从 codex-acp 的确认请求里拿到，要实测。
3. **hooks**：
   - 假设要在 `[features]` 里开 `codex_hooks = true`，`hooks.json` 和 Claude 同格式；
   - 运行中改写时用的是不带 matcher 的 PreToolUse；
   - 项目级的 `.codex/` 能不能生效，可能还取决于 Codex 自己的信任表；
   - hooks-reload 的 Codex 版用 workspace-write 模式。
4. **只读命令不经审批**：`cat`、`ls`、`sleep` 在 read-only 下不经审批，相关剧本靠"等工具调用开始"来定时；假设 `cat` 会被归为 read 类工具。
5. **写文件**：apply_patch 在 read-only 下会请求审批（T102 实测过）。
6. **思考剧本**：推理档位 low 时，Codex 会不会给出思考摘要，要实测。

**其他**：注入的 agent 在录制头里的 CLI 版本是空字符串，这只影响假 agent 走真实拉起路径的情形。
