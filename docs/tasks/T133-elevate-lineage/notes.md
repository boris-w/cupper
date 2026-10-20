# T133 工作笔记

- 执行者：子 agent（Rust 工程师；T043、T055、T008、T116 的执行者）

## 开工前读过的

- 任务书；T038 任务书末尾的补充裁定；T038 分支 `notes.md` 里与 T133 的窄接口（只读查看）；
- 设计 5.8.4、5.8.5，5.2.14（CLI 在 `Cupper.app/Contents/Helpers/cupper`），10 的 bundle 布局；
- `handoff-notes.md` 里含 T038 的两行：`parent_chain` 能走到 launchd；进程名是 16 字节短名；
- 现有代码：
  - `cupper-sys` 的 `proc.rs`（`process_info`、`parent_chain`、`pid_path`）、`codesign.rs`（`AuditToken::verify`、`cdhash`、`file_cdhash`）、`peer.rs`；
  - `cupper-cli` 的 `client.rs`（`ClientAuth::Peer` 就是只读的 `cli` principal）、`verify.rs`（`HostCheck::Harness`）；
  - 协议 `methods::admin`（`AuthElevate`、`VerifiedRequester`、`ChainProcess`）；假 agent 的剧本命令。

## T038 的窄接口（照它的 notes.md）

`elevation::Requesters::examine(token: AuditToken) -> BoxFuture<'static, Examined>`，`Examined::Clear(VerifiedRequester)` 或 `Examined::Descendant { signals: Vec<String> }`（`same_session`、`same_group`、`ancestor`、`unchecked`）。注入点是 `IndexDeps.requesters`。在 T038 合并之前，本任务的实现放在新模块 `requester/` 里，不建 `elevation` 模块，避免和 T038 冲突；合并后只加一个 `impl Requesters`。

## 做法

- **`cupper-sys`**（`unsafe` 只在这里）：
  - `has_tty(pid)`：读 `PROC_PIDTBSDINFO` 的 `e_tdev`，不是 `NODEV` 就算有控制终端；
  - `AuditToken::signing()`：对令牌所指的正在运行的代码调用 `SecCodeCopySigningInformation`，取 identifier 和 Team ID，未签名时返回 `None`。用令牌而不是路径，所以 pid 复用骗不过去；
  - `setsid()`：包装，给 IT 里要自成会话的探针用。
- **血缘**（纯函数 `requester::lineage`）：
  - 输入：请求方及其祖先链（造出来的 `ProcessInfo`）、`adapters` 表里未结束的行（pid、pgid、`start_us`），以及这些 pid 现在的进程信息。
  - 某一行的 pid 现在不在、或者启动时间和 `start_us` 不符时，这行作废：适配器已经死了，pid 可能被复用了。
  - 三个信号：
    - 同一会话：请求方的 sid 等于适配器的 pid，适配器是自己会话的首进程；
    - 同一进程组：请求方的 pgid 等于适配器的 pgid；
    - 祖先：祖先链里有某个进程，pid 和启动时间都与适配器相符。
  - 取不到请求方的进程信息时返回 `unchecked`，按"是后代"处理（fail closed）。
- **核实信息**：
  - 采集：可执行文件（`pid_path`）、签名身份（经令牌）、pid、父进程链（每一层的 pid 和路径）、`has_tty`；
  - `origin_unverifiable`：没有控制终端，或者父进程就是 launchd，或者链没有走到 pid 1（中途断了），三者有一即标；
  - `bundled_cli`：请求方的路径等于宿主所在 App 的 `Contents/Helpers/cupper`（由宿主可执行文件的位置推出），并且令牌所指代码的 cdhash 等于那个文件的 cdhash。不在 App 包里运行（开发、测试）时恒为 false。
  - 关于"宿主运行期间不读取 App 包内的文件"：这里只是在有人申请提权时，读 App 包里 CLI 的签名来做比对，不加载其中的内容，所以我按允许处理。如有异议请裁定。
- **探针**：`cupper-cli` 的 `src/bin/elevate_probe.rs`，二进制名 `cupper-elevate-probe`，`required-features = ["test-harness"]`，发布构建里不会有。
  - 参数：`--data-dir`、`--out`、`--setsid`、`--scopes`。
  - 流程：按需 `setsid`；以 `ClientAuth::Peer`（`cli` principal）连上宿主；发 `auth/elevate`，把结果写进 `--out`；得到授权后再调一个 write 类方法，把结果也写进去，然后退出。
  - 路径约定同假 agent：`target/<profile>/cupper-elevate-probe`，由 `xtask ci` 在测试前构建。
- **假 agent**：加 `spawn <program> [args...]`，拉起任意程序，不等它结束，继承 stdio 之外的一切；探针是否 `setsid` 由探针自己的参数决定。

## 开工前的规模估计

**实现**

| # | 模块 | 草估 |
|---|---|---|
| 1 | `cupper-sys`：`has_tty`、`AuditToken::signing`、`setsid` | 75 |
| 2 | 血缘纯函数 | 50 |
| 3 | 核实信息的采集（`examine`：进程信息、链、终端、签名、`adapters` 表、`bundled_cli`） | 85 |
| 4 | 协议 `bundled_cli`（生成物另计） | 3 |
| 5 | 探针二进制与 Cargo 登记 | 60 |
| 6 | 假 agent `spawn`、设计 09 一行；xtask 构建探针 | 14 |
| 7 | 接上 T038 的接口（合并后） | 15 |
| | 实现小计 | **约 300**，×1.15 ≈ **345** |

**测试**

| 用例 | 草估 |
|---|---|
| `cupper-sys`：`has_tty`、`signing` | 35 |
| UNIT-lineage-signals：每个信号、pid 复用、链断在 launchd、取不到请求方 | 60 |
| 核实信息的采集与 `bundled_cli` 判定 | 40 |
| IT-elevate（合并后）：两种后代被拒；非后代未决、核实信息、授权后能写；全局一个未决、被拒 2 次后静默、2 分钟过期、断开即失效，经 `test/clock_advance` | 200 |
| 测试小计 | **约 335**，×1.1 ≈ **370** |

**合计约 715 行**，在约 800 以内。IT-elevate 的规模把握最小，写完独立部分时再推算一次。
