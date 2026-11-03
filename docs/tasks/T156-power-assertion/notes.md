# T156 工作笔记

- 执行者：子 agent（Rust 工程师，宿主与 cupper-sys）
- 日期：2026-11-03

## 读到的现状（开工前）

- **计数从哪来**：会话 actor 在 `input()` 末尾写 `running`（`core.turn_id().is_some()`），在 `commit()` 里写 `pending`（视图里待处理请求数）。回合开始与结束、请求出现与了结都经 `input()`，`commit()` 只在 `input()` 里改 `pending`；其余直接调 `commit()` 的路径（信任重扫、快照、配置变化）不改这两个量。所以在 `input()` 末尾读这两个量、报给监护，就覆盖了全部状态变化，不需要轮询。
- **监护的观察是轮询**：`Watcher` 每 `supervisor_observe` 读一次各 `Part` 的 `activity()`，供空闲退出用。任务书要求防休眠不另起轮询，所以防休眠不走这条路。
- **只有待处理请求、没有回合**：协议 5.3 的裁定"回合外的交互请求"规定请求一律属于某个回合，回合在它的请求全部解决之前不结束。宿主里这种状态到不了，只能在纯核心的单元测试里覆盖。
- **`host.updated`**：索引的 `deps.host` 是本次运行的值，`Mail::PendingUpgrade` 改字段后追加一条。下次启动时 `reconcile` 发现视图里的与本次不同就补写，所以崩溃前写下的 `power_assertion: true` 会在重启时改回 `false`。
- **有序退出的位置**：`Host::exit` 里写线程停下之后、删套接字之前。
- **系统层面的核对**：`pmset -g assertions` 的"Listed by owning process"按 `pid <pid>(<名字>): [...] <时长> PreventUserIdleSystemSleep named: "<名称>"` 列出每个进程的断言，按进程号过滤即可。

## 做法

- `cupper-sys::power`：`IdleSleepAssertion::take(name: &'static str)` 调 `IOPMAssertionCreateWithName`，`Drop` 调 `IOPMAssertionRelease`。名称只收编译期常量，所以不可能带用户数据。`test-util` 下给出按进程号读 `pmset -g assertions` 的帮助函数，sys 自己的测试和宿主的 IT 共用。
- 宿主 `supervisor/power.rs`：
  - `PowerCore`（纯核心，无定时器）：输入是某个会话的计数从 `from` 变到 `to`，以及有序退出；输出取得 / 释放。退出后不再取得。
  - `Share`：每个会话 actor 一份，`set(回合 + 待处理请求)` 只在变化时发消息；`Drop` 时把自己那份还回去（actor 离开内存、宿主退出）。
  - 监护 actor（`Watcher`，仍是一个 actor）多收一路信箱，执行取得 / 释放，并告诉索引写 `host.updated.power_assertion`；取得失败时记日志、如实报 `false`。
  - `Host::exit` 在写线程停下之后经 `Power::exit()` 释放并等确认，然后删套接字、释放锁（5.2.13 的顺序）。

## 规模估计

| 模块 | 内容 | 实现 | 测试 |
|---|---|---|---|
| cupper-sys | IOKit 两个函数的封装、`Error::IoKit`、导出；`test-util` 的 pmset 读取 | 60 | 25（取得后 pmset 能看到本进程，释放后看不到） |
| 防休眠核心与句柄 | `PowerCore`、`Power`、`Share`、actor 侧的执行（取得、释放、告诉索引） | 95 | 45（计数、只有请求也持有、回到 0 释放、退出后不再取得） |
| 接线 | 监护 actor 的信箱、`Host::start` / `exit`、索引的 `Mail::PowerAssertion` | 40 | — |
| 会话 | `SessionDeps`、`Parts` 的字段，`input()` 末尾报数，两处构造，测试的 deps | 15 | — |
| IT-power-assertion | 等权限请求的回合持有、回答后回合仍在跑仍持有、中断后释放；SIGKILL 后系统里没有残留，重启后 `host.updated` 为 `false` | — | 80 |
| **合计** | | **约 210** | **约 150** |

合计约 360 行，在停止线 400 以内，余量不大，写的时候每个模块跑一次 `cargo xtask size`。
