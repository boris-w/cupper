# T046 工作笔记

- 执行者：子 agent（Rust 工程师；T043、T055、T008、T116、T133 的执行者）

## 开工前读过的

- 任务书；设计 5.2.14 的命令表；`handoff-notes.md` 第一列含 T046 的三行（T049、T039、T100）。
- `cupper-cli` 的库：
  - `client.rs`：`Client`、`Incoming`、`CloseReason`、`ClientAuth`、`command_id`；
  - `mirror.rs`：`Mirror<IndexView>`、`Mirror<SessionView>`、`subscribe`、`fold`；
  - `verify.rs`：`HostCheck::of`、`is_weak`；
  - `harness.rs`：`HarnessKey::read_fd`；
  - `build_kind.rs`：`BUILD`、`default_data_dir`、`socket_path`、`build_info`。
- 现在的 `main.rs` 只答 `--build-info`。
- 协议：`HistoryPage`（`ops`、`next`）；`IndexView` 的 `sessions`、`elevations`；`SessionView` 的 `turns`、`items`、`requests`。
- 依赖：工作区里没有命令行参数解析库，打算手写解析，不加依赖。

## 开工前的规模估计

行数指 rustfmt 展开后的行，含文档注释。按前几个任务的实测比例，实现放大 1.15 倍、测试放大 1.1 倍。

**实现**

| # | 模块 | 内容 | 草估 |
|---|---|---|---|
| 1 | 参数与命令模型 | 全局选项 `--data-dir`、`--json`、`--harness-key-fd`（编译期门控）、`--build-info`；12 个子命令及其选项；用法说明；解析写成纯函数，`repl` 复用 | 150 |
| 2 | 连接与输出层 | `HostCheck::of(BUILD)`、认证方式、连接；"宿主未经签名校验"的标记；输出两种格式（JSON Lines 和人读）；错误按语义码输出、退出码；`Refused(PeerExited)` 与 `Eof` 都报"宿主已退出" | 110 |
| 3 | 读命令 | `status`（40）；`sessions [--watch]`，用索引镜像（70）；`watch`，用会话镜像逐条输出事件（60）；`history`，翻页（50）；`pending [<session>]`，索引加会话镜像（70） | 290 |
| 4 | 写命令 | `new`：`project/add` 再 `session/create`（40）；`send [--wait]`，等到回合结束（60）；`interrupt`（20）；`approve`/`reject`，从会话视图取选项（60）；`answer`，把 `--field k=v` 按表单 schema 转成回答（50） | 230 |
| 5 | `elevate` 与 `repl` | `elevate` 的五种回复各有输出和退出码（50）；`repl` 逐行读 stdin、切词、在同一条连接上执行（70） | 120 |
| 6 | 在场 | 不上报（索引不采信 CLI 的在场，T100） | 0 |
| | 实现小计 | | **约 900**，×1.15 ≈ **1,035** |

**测试**

| 用例 | 草估 |
|---|---|
| 参数解析的单元测试（新编号，补进 T046 段） | 60 |
| IT-cli-commands：<br>• 读命令（`status`、`sessions`、`watch`、`history`、`pending`）；<br>• 没有 `elevate` 时写被拒；<br>• `elevate` 经 control 连接批准后，`repl` 里的写操作（`new`、`send --wait`、`interrupt`）；<br>• 对假 agent 提问的 `approve`/`reject`/`answer`；<br>• 时钟经 `test/clock_advance` 推进；<br>• `cupper` 二进制的路径约定同假 agent | 300 |
| 测试小计 | **约 360**，×1.1 ≈ **400** |

**合计约 1,435 行**，超出约 900 的线约六成。按任务书，开工前停下，没有写代码。

## 拆分建议

**方案一（建议）：两份**

| 拆分 | 内容（模块编号） | 用例 | 估计 |
|---|---|---|---|
| T046（收窄）CLI 骨架与读命令 | 1、2、3 | IT-cli-commands 的读命令部分；参数解析的单元测试 | 约 750 |
| T046b 写命令、提权与 repl | 4、5 | IT-cli-commands 的写和提权部分：没有 `elevate` 时写被拒；批准后 `repl` 写成功；`approve`/`reject`/`answer`；`watch` 看到回合事件。可另起编号，或用 `parts` 把 IT-cli-commands 分成两部分 | 约 700 |

- 第一份做完，`status`、`sessions`、`watch`、`history`、`pending` 就能用于观察宿主，T090 可以开始搭 e2e 的读取部分。
- 第二份只依赖第一份的骨架。
- 如果 T090 一开始就要用 CLI 写，两份都排在它前面，松弛会从 2 减到 1 左右。

**方案二**：不拆，上限提到约 1,450，硬线 1,550。

**请一并确认**：
1. 手写参数解析、不加依赖。
2. `repl` 的切词只支持空白分隔和双引号。
3. 不上报在场。
