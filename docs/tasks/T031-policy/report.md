# T031 汇报（规模超限，停下待裁定）

- 执行者：子 agent（Rust 工程师，宿主网关与安全）
- 分支：`feat/T031-policy`
- 完成日期：2026-10-19
- **状态：功能和测试都已完成并提交，工作区干净，不带 `--record` 的 `cargo xtask ci pr` 已通过。但改动共 1,834 行（代码 724 行，测试 1,110 行），明显超出约 1,500 行的上限。按守则第 8 条，我停在这里等编排者裁定，所以还没有跑 `--record`，也还没有提交 CI 摘要。**

## 需要编排者裁定

**超限是怎么发生的（这是我的流程失误）：**
- 我把六个模块全部写完之后才第一次统计，没有每写完一个模块统计一次。
- 第一次统计用的脚本有错：`wc` 输出里的前导空格让 `matrix.rs`、`testkit.rs` 两个文件落到了别的字段，合计时漏掉了约 345 行。
- 我按这个错误的数字压缩了测试，以为已经降到约 1,480 行，然后才把工作拆成几个提交。
- 之后用 `git diff --numstat` 复核，才发现实际是 1,834 行。全部经过写在 `notes.md` 里。

**行数分布：**

| 部分 | 代码 | 测试 |
|---|---|---|
| `describe.rs` | 244 | 201 |
| `input.rs` | 159 | 232 |
| `matrix.rs` | — | 242 |
| `tier.rs` | 67 | 160 |
| `method.rs` | 108 | 87 |
| `mod.rs` | 118 | 45 |
| `testkit.rs` | — | 103 |
| `attach.rs` | 28 | 40 |

- 测试占 60%，因为每一条拒绝路径都有用例。
- 压缩只改了写法，比如改成表驱动，没有删掉任何拒绝方向的用例。

**两个选项（提交已经按模块分开，哪种都是机械操作）：**

- **A. 按现状一个任务验收。** 我在干净的工作区跑 `cargo xtask ci pr --record docs/tasks/T031-policy/ci/`，再提交摘要。我倾向于 A：
  - 代码只有约 720 行，可以逐个提交审；
  - 超出的部分主要是表驱动的拒绝方向用例。
- **B. 把 `policy/describe` 拆成单独的任务。**
  - `052927e` 一个提交，共 445 行；另外 `matrix.rs` 里"原生确认格子必须能被 describe 解析"那一段检查要随它一起移走，约 6 行。
  - 拆完后 T031 约 1,380 行。
  - 做法：在本分支上加一个 revert 提交，再从 `052927e` 另开分支。我不做 rebase。

## 做了什么

`crates/cupper-host/src/policy/` 全部是纯函数，不做 I/O；需要时间的地方以 `MonoTime` 参数传入。`lib.rs` 只加了一行 `pub mod policy;`。principal 直接用协议里的 `Principal {id, kind, role, scopes}`。

**方法级（`method(principal, name, scope)`）**
- 检查方法的 `SCOPE`。
- CliOnly：`auth/elevate` 对 `cli` 以外的 principal 一律拒绝。
- SEC-9 名单：`project/add`、`project/remove`、`agents/refresh`、`agents/restart`、`session/rename`、`session/archive`、`session/restore`、5 个 git 写方法、`settings/update`。名单里的方法用协议常量 `X::NAME` 写，不手写字符串。

**参数级规则**
- `settings_update`：patch 里出现安全相关的键（两个 `cli_path`、`env_passthrough`、`diagnostics.verbose`）就要 privileged。
  - `SettingsPatch` 逐个键解构，以后新增设置键时，不归类就编译不过。
- `set_config`：手机只能改 model、reasoning_effort、permission_mode。permission_mode 接着按档位规则判定，所以手机只能收紧或保持。

**档位规则（`tier_change`、`ceiling`、`option_tier`）**
- 收紧或同档：所有 principal 都可以。
- 放宽到 elevated 要 approve，放宽到 full_access 要 privileged。
- 手机一律不能放宽，按 SEC-9 拒绝。
- 映射里没有的模式按 full_access 处理（5.6.5）；当前档位未知时按 normal 处理。
- 选项只有 `mode_after`、没有 `tier_after` 时，按 full_access 处理。

**回答与输入证据（`respond`、`git_revert`）**
- 选项必须存在，否则是 invalid params。
- principal 要有选项的 `requires`。`allow_always`、kind 未知、effect 未知的选项一律按 privileged，不管对照表怎么写。
- 带 `mode_after` 的选项另按档位规则判定。
- 来自 window 的请求：
  - 必须带证据，并且 kind、key、digit 三者一致；
  - Esc 只能用于 `dismissed`，或 effect 是 reject、plan_revise 的选项；
  - `digit` 必须等于选项在 `options` 里的序号，特权选项和效果未知的选项也占序号；提问时只接受 `answer`；digit 永远不能用来 dismiss；
  - 距宿主收到请求不足 400 ms 时回 `conflict/too_early`。
- `git/revert` 来自 window 时，只接受点击、Enter、Space、⌘Enter 或原生确认。
- role 未知的 desktop 连接按 window 处理。

**信任门控钩子（`attach`）**
- 已信任：加载项目配置。
- 未信任或 stale：Claude 只加载用户配置；Codex 回 `unavailable/project_untrusted`。
- 未知的信任状态或未知的 agent 一律拒绝。

**`policy/describe`（`Confirmable::parse`、`Facts`、`describe`）**
- 主进程把 `{method, params}` 解析成 `Confirmable`。
- 按命令种类，从执行者那里查好 `Facts`：模式的档位、待回答请求里被选的选项、运行中的回合数。
- 然后得到 `{action, details}`。各 action 的 details 键如下：
  - `trust_project`：`project_id`、`trust_digest`；
  - `confirm_config`：`session_id`、`config_digest`；
  - `full_access`：
    - 来自 `session/create`：`agent`、`project_id`、`mode`；
    - 来自 `session/set_config`：`session_id`、`mode`；
    - 来自 `interaction/respond`：与 `privileged_option` 相同，另加 `mode`；
  - `privileged_option`：`session_id`、`request_id`、`option_id`、`kind`、`effect`；
  - `security_setting`：`settings`，只含安全相关的键和新值；
  - `grant_elevation`：`elevation_id`、`scopes`；
  - `interrupt_turns`：`running_turns`。只有 `host/exit` 带 `force`、`host/prepare_upgrade` 是 `mode: force`，并且确实有运行中的回合时才是这个 action。
- 这个形式下不需要确认的命令，返回 invalid params。

## 关键取舍（请写进 5.3.11）

1. **SEC-9 和 CliOnly 的拒绝码用 `forbidden/role`。**
   - 现有的 reason 里没有合适的：`scope` 会让 CLI 去申请提权，不对。
   - 我没有在协议里新增 reason，避免越出 `policy/`。
   - 档位规则拒绝时用 `forbidden/scope`，`data.scope` 填 approve 或 privileged（与 5.3.11 里 `session/create` 加 `forbidden/scope` 一致）。
2. **手机可以改 `permission_mode`，但只能收紧或保持。**
   - 依据：MB-9 写的是"切换模型、推理强度、权限模式（受 SEC-9 限制）"，5.8.2 的档位表里手机的"收紧或同档"是 ✓。
   - 冲突：协议测试里的注释写的是"只能改模型和推理强度"。
3. **400 ms 用宿主自己的 `MonoTime` 计。** 从交互代理收到请求时算起，不用线上的墙钟 `created_at`。`MIN_ANSWER_DELAY` 是安全阈值，不是定时器，所以没有登记到 `durations.rs`。
4. **不归策略管、也没做的部分：**
   - `actionable_by` 检查归 T042 的检查链；
   - 按构建门控（`Build` 格子）归 T033 注册方法时处理；
   - `ByMain` 格子由网关处理，策略只是放行；
   - 原生确认格子：control 连接持有 privileged，所以策略放行。
5. **矩阵对照表放在宿主这边另写**，注明出处为 5.8.3 和 `catalog.rs` 的 `MATRIX`；不改 `cupper-protocol`，避免和 T015 冲突。
   - 每格判定时先过方法级，所以对照表和真实的 scope 不一致时，测试会直接失败。
   - 与协议表有一格写法不同：window 的 `interaction/respond` 按 5.8.3 的"真实输入"标为 `I`，协议表里是 `Y`。

## 验证方式

- `cargo test -p cupper-host --lib policy`：25 个用例全部通过。
  - 用例名以 `unit_tier_rules_*` 或 `unit_policy_matrix_*` 开头。
  - 400 ms 用传入的 `MonoTime` 验证，不睡眠。
- clippy：`--all-features` 和不带特性两种都通过（`-D warnings`）；`cargo fmt --check` 通过。
- 每个提交都单独验证过构建、clippy 和测试。
- **变异检查：** 逐条把 15 处规则改坏，每一处都至少让一个用例失败。第一轮里"role 未知的 desktop 连接不按 window"没被抓到，已补用例。
- **`cargo xtask ci pr`（不带 `--record`）在 `c429f42` 上全部通过：**
  - rust：675 个通过，0 个失败；
  - ts：64 个通过；
  - license：通过。

**按验收标准逐条：**
- UNIT-tier-rules ✓：放宽档位、`mode_after`、手机不能放宽，每项都有允许和拒绝两个方向的用例。
- UNIT-policy-matrix ✓：按 5.8.3 逐格断言，并断言目录里的每个方法恰好出现在一行。`Sec9`、`Input`、`CliOnly`、`Native`、`ByMain`、`Build` 六种格子都覆盖到了；`digit` 与序号的核对也在内。
- 400 ms 不睡眠 ✓。
- 拒绝的路径有覆盖 ✓。
- 测试名以用例编号开头 ✓。
- CI 摘要：**等裁定后再 `--record` 并提交**。

## 提交列表

```
c429f42 docs(task): record T031's actual line counts and how they overran
3702832 test(host): check the permission matrix cell by cell (UNIT-policy-matrix)
052927e feat(host): add policy/describe, an action and details per command
bbc304c feat(host): add the project trust gate's decision to the policy
2a8fe4b feat(host): add the policy's tier rules and the checks on answers
250f8dd feat(host): add the policy's method level and parameter rules
7ec64dc docs(task): estimate T031 policy line counts before coding
```

## 遗留问题 / 范围外发现

- **交接给 T033：**
  - 每个请求和客户端通知都调用 `method(p, M::NAME, M::SCOPE.as_ref())`。
  - 网关这一步还要调用三个只看参数的规则：`settings_update`、`set_config`、`git_revert`。
  - `policy/describe` 的处理器按 `Confirmable` 的种类去查 `Facts`。
  - `Denial` 和 `DescribeError` 都实现了 `Into<RpcError>`。
- **交接给 T042：**
  - 在检查完 pending、摘要和 `actionable_by` 之后调用 `respond`。
  - 收到请求时用 `Clock` 记下 `entered`，并传入会话当前的档位。
  - 另一个问题：5.3.6 写"所有选项都是 privileged 时 `actionable_by` 只有 `desktop_ui`"，这会把 test_harness 排除在外；而矩阵里 test_harness 对特权选项是 ✓。请 T042 留意。
- **交接给 T043、T044：**
  - `session/create` 和 `set_config` 的档位，用能力里的 `risk` 换算成 `Tier` 后调用 `tier_change`；映射里没有的模式传 `FullAccess`。
  - `config_unconfirmed` 的判定用 `ceiling`。
  - Attach 前调用 `attach`。
- **交接给 T077：** 原生确认框按上面列出的 details 键取内容。
- **建议改协议 `MATRIX`：** window 的 `interaction/respond` 那一格改成 `Input`，与 5.8.3 一致。改了之后，协议那边的 schema 测试也会检查 `input` 参数，`interaction/respond` 本来就有这个参数，不会失败。另外，协议测试里 SEC-9 的注释要按第 2 条修正。
- **已有问题（不是本任务引入的）：** `RUSTDOCFLAGS=-D warnings cargo doc` 会在 `driver/tap` 的文档链接上失败，已在交接里登记给 T055。policy 模块自己的文档链接已经核对过，没有警告。

## 追加：裁定后的收尾

已就绪，CI 摘要的文件名是 `docs/tasks/T031-policy/ci/pr-dfe4df135b84.json`。

- **两处协议测试（`1a4b925`）**：
  - `MATRIX` 里 window 那一列的 `interaction/respond` 改成了 `Input`。`Input` 格子的 schema 测试现在也会检查这个方法带 `input` 参数，测试通过。
  - SEC-9 的注释改成"手机只能改模型、推理强度，以及收紧或保持 permission_mode"。
  - 两张表现在一致了，所以宿主 `matrix.rs` 文档里"这一格写法不同"那句说明也一并删掉。
  - 这个提交 +6 / −7 行。改完后 `crates/` 合计新增 1,836 行、删除 3 行，在 1,850 行以内。统计和裁定已记进 `notes.md`（`dfe4df1`）。
- **CI**：在干净的 worktree 里跑了 `cargo xtask ci pr --record`，对应提交 `dfe4df1`：
  - rust：675 个通过，0 个失败；
  - ts：64 个通过；
  - license：通过；
  - 其余几项作业还没有实现。
- 摘要已检查过，没有个人信息，已提交（`b8d88ec`）。worktree 干净。
