# T187 汇报

- 执行者：子 agent（Rust 工程师；T064 的执行者接着做）
- 分支：`feat/T187-compat-real-mode`
- 完成日期：2026-10-26

T187 做完了：真实模式、自我约束、配置前后对比、`--dry-run`、进程检查都已落地，`ci compat-framework` 和 `ci pr --record` 都通过，摘要已提交。中途规模越过了停止线和硬停止线，按第二次裁定接受，最终新增 859 行（上限 860）。全程没有运行真实的 Claude Code 或 Codex CLI，没有读写真实的 `~/.claude`、`~/.codex`。

## 做了什么

**1. 参数（xtask）**
- `--fake`、`--real` 必须恰好写一个，否则报错并说明用法。
- `--dry-run` 两种模式都能用；`--model` 只能和 `--real` 一起用，默认 haiku；`--words` 只能和 `--write` 一起用；`--dry-run` 不能和 `--write` 一起用。
- 真实模式加 `--write` 时拒绝 `--only`：T188 会把"所有剧本都通过"的真实报告计入已验证范围，只跑了部分剧本的报告看上去也是全部通过。

**2. 真实模式**
- 构建 e2e 种类的 `CupperHost.app`（`target/runtime-pack` 不在时先跑 `runtime-pack`）。执行者用 `spawn_from` 启动它，环境取自登录 shell，宿主自己的环境过滤照常生效，通知改为记录、不弹出。
- 每个剧本的临时目录权限设为 0700，项目放在其下的 `repo/` 里，并执行 `git init`。
- 每个剧本都带真实提示词和显式的权限模式；思考流剧本用 sonnet，其余用 `--model` 指定的模型。
- 适配器和 CLI 的版本取自导出录制的头行。
- 宿主库里的 `agent_session_id` 列进报告的 `cleanup`。
- `--write`：
  - 全部剧本的录制都经脱敏和敏感词清单后写入 `testdata/recordings/`；
  - 报告写到 `testdata/compat/reports/<adapter>-<ver>-<cli>-<cli-ver>.json`，报告也要过敏感词清单；
  - 报告里的 `recording` 改成指向 `testdata/` 下的录制路径。

**3. 配置前后对比（`crates/xtask/src/compat/config.rs`）**
- 照 T003 的 `config-hash.ts`：
  - `~/.claude/settings.json`、`~/.claude/settings.local.json`、`~/.codex/config.toml` 按整文件算；
  - `~/.claude.json` 只算和权限有关、会拉起程序的字段：顶层的几个固定键加名字像权限的键，以及每个项目条目的 8 个字段。
- 判定：
  - 已有的值变了，或者有项目条目消失，判失败；
  - 本次临时仓库新出现的项目条目只计数，列进 `cleanup`，不判失败；
  - 其他新增，判失败。
- 报告里只写名字和是否变化，不写值，也不写路径；凭据文件不读。
- 配置有变化时，这次运行判为不通过，`--write` 不写任何东西。

**4. 自我约束（UNIT-compat-guards）**
- 运行前（静态）：
  - 权限模式必须在允许清单里：Claude `default`、`plan`，Codex `read-only`、`workspace-write`；
  - 选项种类只能是 `allow_once`、`reject_once`；
  - 剧本声明了 `loosens` 时，可以使用清单外的模式，但必须写明是哪个模式；选项种类的限制不因声明放宽。
- 运行中：
  - 会话能力事件到达后，按宿主给的风险，所选模式必须是 `normal`；
  - 所选选项不能要求 privileged，也不能是持久的（声明了 `loosens` 时例外）；
  - 临时仓库必须在本用户自己的临时目录下。
- 剧本跑完后：适配器以下的进程不能持有宿主的管道和 Unix 套接字（按 `lsof -F` 的端点地址和对端地址比对）；宿主退出后 5 秒内这些进程都要退出，还在的记为失败，并发送 SIGTERM。

**5. 其他**
- `--dry-run`：列出每个剧本的权限模式、准备文件、是否跳过、约束检查的结果，临时目录检查的结果，以及要比较的配置文件。不读配置，也不启动任何东西。
- 报告：
  - 结果新增 `not_applicable`、`observed`、`real_only`，非通过的结果都必须写明原因；
  - 真实报告里不允许出现 `real_only`；
  - schema 已重新生成。
- `steer` 剧本记为"不适用（M1b）"，原因写明：M1a 里 steer 方式的 `session/send` 会排进队列。
- `ci compat-framework` 新增两步：执行者的单元测试，以及一次 `--real --dry-run`，让真实模式的代码路径在每次 CI 里也会跑到。

## 关键取舍（你要求写明的几点）

- **Claude 的允许清单里加了 `plan`。** 它在宿主的风险分级里是 `normal`，比 `default` 更严（只做计划、不改东西），plan 剧本要用它。运行时还会再按宿主给的风险核对一次。
- **报告 schema 有两处不是纯附加。**
  - 结果多了三种取值；
  - `config_files` 的条目从"路径、改动前、改动后"改成了"名字、是否变化"。前后哈希也算从值推出来的，比如布尔字段的哈希可以反推，所以不写。
  - 目前还没有报告入库，可以接受。
- **`cli_sessions` 改名为 `cleanup`。** 里面放 CLI 会话，以及临时仓库在 `~/.claude.json` 里的项目条目。
- **`--fake --write` 没有产生 `config` 差异。** 显式传的 `default` 与适配器当前的模式一致，宿主不发 `set_mode`；项目目录从临时根目录挪到 `repo/` 后，路径仍替换为 `/work/project`。所以三份 B 层录制归一化后与入库的相同，文件没有改写，金样例也没变。
- **越线的经过。**
  - 收窄后估计约 640 行。xtask 写完时统计过一次，是 405 行；之后执行者一侧连着写完，中间没有再统计，等发现时已经是 852 行，越过了停止线 720 和硬停止线 800。这是我没有按守则每完成一个模块就统计一次。
  - 超出主要来自四处：rustfmt 把长链式调用和 `json!`、`assert!` 展开成多行；运行时约束要先等能力事件、再按风险判断；配置比较照 T003 逐字段做；真实模式要从头行取版本、收集 `cli_sessions`。
  - 发现后停下汇报。按第二次裁定，上限改为 860 和 900，没有为了压线去改代码。
  - 之后加的 `ci compat-framework` 两步（7 行）在原估计之内。
  - 最终新增 859、删除 108，在 860 以内。
- **"只作用于临时仓库"的落实方式。** 执行者一律只在本用户临时目录下的仓库里跑，并对每个剧本强制检查，所以声明了 `loosens` 的剧本也只能作用于临时仓库。

## 验证方式

- **UNIT-compat-guards**，共 5 个用例：
  - 每个剧本都在允许的模式里、只用允许的选项种类；`acceptEdits`、`danger-full-access` 被拒，模式为空被拒，`allow_always` 即使声明了也被拒，声明后的 `acceptEdits` 放行，Codex 的 `workspace-write` 放行；
  - 临时目录：本用户的临时目录下放行，`/tmp`、临时目录本身、不存在的路径都拒绝；
  - `lsof -F` 输出的解析；
  - 配置对比：用临时目录代替 HOME，CLI 自己的状态变化和本次项目条目不判失败、只列进清理项；被改的设置文件、新出现的权限键、项目的 `allowedTools`、外来项目都判失败，报告里不出现值和路径；
  - 参数：真实模式必须显式指定；真实模式加 `--write` 时拒绝 `--only`。
- **UNIT-compat-report**：新增结果取值的规则，以及 schema 是否最新。
- **`cargo xtask compat --fake`**：基本组 10 个剧本都通过，steer 记为不适用。每个剧本都带上了管道和套接字检查、遗留进程检查。
- **`lsof` 检查的反向验证**：临时让 new-session 剧本拉起一个持有宿主管道的孙进程（`spawn_group_child`），该检查判了失败；这个改动没有提交。
- **`cargo xtask compat --real --dry-run`**：列出了完整计划，没有启动任何东西。
- **B 层录制**：`--fake --write` 的结果见上面"关键取舍"，三份都没变。之后不设 `UPDATE_GOLDEN` 跑 `gold_b`，9 个都通过。
- **`cargo xtask ci compat-framework --record`**：13 项通过，摘要 `ci/compat-framework-3dd00ac79e24.json`。
- **`cargo xtask ci pr --record`**：在干净的 worktree 里，7 个作业都通过，摘要 `ci/pr-e4548c455e2c.json`。跳过的步骤和以往一样：sensitive-words（本机没有清单）、test-ui。

## 提交列表

```
f263369 docs(task): T187 PR CI summary
e4548c4 docs(task): T187 compat-framework CI summary
3dd00ac ci: compat-framework also runs the runner's tests and a real dry run
ba26709 Merge branch 'main' into feat/T187-compat-real-mode
6fae889 docs(task): T187 notes on the size overrun
71ddf7d feat(compat): the real mode, its self-constraints and process checks
2fc41a9 docs(task): T187 estimate after the split
e21b01d Merge branch 'main' into feat/T187-compat-real-mode
3c22c99 docs(task): T187 size estimate and split proposal
```

## 遗留问题 / 范围外发现

1. **真实模式只靠单元测试和 dry-run 验证过。** T066 第一次实际运行时，请留意三点：
   - e2e 宿主包能否在登录 shell 的环境下找到 `claude`；
   - 只用 `--model` 选模型（例如 haiku）在真实适配器上是否被接受；
   - cancel 剧本"允许后等 1 秒再中断"在真实 CLI 上是否可靠，这一点 T064 时已提过。
2. **`lsof` 检查的范围。** 现在检查的是宿主直接子进程以下的所有进程。真实适配器拉起的 `claude` 进程如果继承了适配器的标准错误（宿主的管道），这项检查会判失败。这正是安全建议 #19 想抓的情况，但如果 T066 真遇到了，需要先判断是适配器的问题还是需要放宽规则。
3. **T188 的穷尽 `match`**：你说会在合并时补上，我这边没有动。
4. **T192 接着做** 其余 10 个剧本，编号以 notes.md 为准。它需要的步骤和执行者能力会在现在的 `Scenario` 类型上扩展。
