# T156 汇报

- 执行者：子 agent（Rust 工程师，宿主与 cupper-sys）
- 分支：`feat/T156-power-assertion`（T054 worktree），PR https://github.com/boris-w/cupper/pull/43 ，头提交 af423cb
- 完成日期：2026-11-03

**CI 状态：交汇报时还没跑完。** 8 个检查里 license、protocol、testdata-scan 已通过，rust、ts、ui、e2e、compat-framework 还在跑；security-peer (macos-26) 也还在跑。PR 与最新 main（abf4493，含 T120）能干净合并，GitHub 显示 MERGEABLE。pmset 按进程号查断言的做法在 runner 上能不能用，要看 rust (macos-15) 和 security-peer (macos-26) 的结果。
- ci 的运行：https://github.com/boris-w/cupper/actions/runs/41586118768
- security-peer 的运行：https://github.com/boris-w/cupper/actions/runs/41586118784

## 做了什么

1. **cupper-sys `power`**：新增 `IdleSleepAssertion::take(name: &'static str)`，内部调 `IOPMAssertionCreateWithName` 取得 `PreventUserIdleSystemSleep`；对象被 drop 时调 `IOPMAssertionRelease`。
   - 名称只接受编译期常量，用户数据进不了 `pmset`。宿主用的名称是 `cupper: agents at work`。
   - 新增错误类型 `Error::IoKit`。
   - 在 `test-util` 下新增 `idle_sleep_assertions(pid)`，读 `pmset -g assertions` 里某个进程持有的这类断言。cupper-sys 自己的单测和宿主的 IT 都用它。
   - `unsafe` 只出现在 cupper-sys，每处都写了 `SAFETY:` 注释。
2. **宿主 `supervisor/power.rs`**：
   - `PowerCore` 是纯核心：不做 I/O，不 await，没有定时器。输入有两种：某个会话的计数从 from 变到 to；有序退出。输出是取得或释放。计数从 0 变 1 时取得，回到 0 时释放；退出后不再取得。
   - `Share`：每个会话 actor 持有一份。它在 `input()` 的末尾报"运行中的回合（0 或 1）加上待处理请求数"，只在数值变化时发消息；drop 时把自己那份还回去。回合开始与结束、请求出现与了结都经过 `input()`，所以计数直接来自现有的状态变化，没有另起轮询。
   - 断言由监护 actor（`Watcher`，仍然只有一个 actor）持有：它多收一路信箱，按核心的决定取得或释放，然后让索引写 `host.updated.power_assertion`。索引只在值变化时追加。取得失败时记一条 warn，并如实报 `false`。
   - 有序退出：`Host::exit` 在写线程停下之后、删除套接字之前，调 `Power::exit()` 释放断言并等确认，符合 5.2.13 规定的顺序。
3. **IT-power-assertion**（`tests/lifecycle.rs`）：
   - 回合等待权限请求时持有断言：`pmset` 在宿主进程号下能看到它，`host.updated` 变为 true。
   - 回答请求后回合继续运行，期间一直持有。中断后回合结束，断言释放。
   - 再开一个回合后对宿主发 SIGKILL：系统里这个进程号的断言消失；重启后 `host.updated` 为 false，新进程也不持有断言。
   - 用例断言的是 `host.updated.power_assertion` 的完整历史 `[0,1,0,1,0]`，所以"请求被回答、回合仍在跑"这段时间如果中途释放过，用例会失败。

## 关键取舍

- **"只有待处理请求、没有回合"由纯核心的单元测试覆盖，IT 不覆盖。** 协议 5.3"回合外的交互请求"的裁定是：请求一律属于某个回合，回合在它的请求全部了结之前不结束。所以宿主里到不了这种状态，编排者已同意。IT 覆盖的情形有四种：
  1. 回合加上待处理请求时持有；
  2. 请求被回答、回合仍在跑时持有；
  3. 回合结束时释放；
  4. SIGKILL 之后系统不残留断言，重启后 `host.updated` 为 false。

  `power_assertion` 的变化贯穿这四种情形。单元测试另外覆盖了：只有请求时也持有、多个会话的计数叠加、退出时释放且之后不再取得、没持有时退出不做任何动作。
- **断言放在监护 actor 里，而不是另起一个 actor**：设计 5.2.2 里 Supervisor 只有一个 actor，并且负责"防休眠引用计数"。因此 `Watcher` 必须活到宿主退出。原来它的循环在所有 `Watch` 都被丢掉后就退出（没有自检、也没有安装器时会这样），现在两路信箱都改用 `Some(..)` 模式，不再 break。
- **每个会话一份 `Share`，靠 drop 归还**：会话 actor 空闲离开内存、或者结束时，它那份计数会自动还回去。退出后核心不再取得断言，所以退出过程中会话的变化不会把断言重新拿回来。
- **退出时仍会给索引发一次 false**：这时写线程已经停了，追加会失败，按现有约定忽略。重启时索引的 reconcile 会把上次运行留下的 true 改回 false，IT 已经验证了这一点。

## 验证方式

本机都只跑了一份：
- `cargo test -p cupper-sys --features test-util`：全部通过，包括新增的"持有时 pmset 能看到，释放后看不到"。
- `cargo test -p cupper-host --features test-harness --lib`：463 个通过，3 个 ignored；新增的两个核心单测都通过。
- `cargo test -p cupper-host --features test-harness --test lifecycle`：20 个通过，包括 IT-power-assertion。
- clippy 在三种特性组合下都没有告警：不开特性、`test-harness`、`test-harness,dev-auth`。`cargo fmt --check` 通过。
- 测完后 `pmset -g assertions` 里没有 cupper 的断言，也没有残留的宿主或假 agent 进程。全程没有运行真实的 Claude Code 或 Codex CLI。

对照验收标准：
- IT-power-assertion 本机通过，覆盖的情形见上一节。
- `host.updated.power_assertion` 与实际持有状态一致：用例同时比对 `pmset` 和索引流。
- PR 上 8 个检查全部通过：尚未完成，见开头的 CI 状态。

规模：`cargo xtask size` 为 +436 −16。写完宿主部分时推算会越过停止线 400，当时停下请示；编排者同意不拆，并把停止线放宽到 470。

## 提交列表

```
af423cb test(host): IT-power-assertion: held while a turn runs, gone with the host
632210c feat(host): keep the Mac awake while turns run or requests wait
2a80433 feat(sys): hold a PreventUserIdleSystemSleep assertion through IOKit
```

notes.md 已随分支提交，里面有开工前的估计和实际规模。

## 遗留问题 / 范围外发现

- **设置页开关的接法建议**（请编排者补进设计）：
  - 设置键 `host.prevent_idle_sleep: bool`，默认 true，与 `host.idle_exit_minutes` 放在同一组。改它只需要 write，不算 privileged。
  - 宿主启动时从 `settings.current()` 读取初值；`SettingsPort` 在这个键变化时调 `Power::enabled(bool)`。
  - `PowerCore` 增加一个输入 `Enabled(bool)`，持有条件改为"计数 > 0、已启用、未退出"。关掉时立即释放，打开时如果计数大于 0 立即取得，`host.updated.power_assertion` 照常如实反映。预计改动约 15 行加测试。
- **runner 上的 pmset**：如果 CI 显示 runner 的 pmset 不按进程列出断言，可以改用 IOKit 的 `IOPMCopyAssertionsByProcess` 读取。那是更多只在测试里用的 `unsafe` 代码，预计 40 行左右，到时请编排者决定。
- 没有新增用例编号，也没有改协议。
