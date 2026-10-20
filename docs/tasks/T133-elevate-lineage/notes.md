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

## 进度（T038 未合并，独立部分已完成）

### 已完成的模块与行数

按 `git diff --numstat`（相对估计提交 d93a4b9，不含 `Cargo.lock`、生成物、schema、fixtures 和文档）：新增 549 行。

| 模块 | 新增 | 其中测试 |
|---|---|---|
| `cupper-sys`：`has_tty`、`AuditToken::signing_identity`、`setsid` | 107 | 约 20 |
| 协议 `VerifiedRequester.bundled_cli`（另有生成物、schema、fixture 各几行） | 4 | — |
| 血缘纯函数 `requester::lineage` 与 UNIT-lineage-signals | 146 | 约 70 |
| 核实信息的采集 `requester::ProcessRequesters::find` | 169 | 约 35 |
| 探针 `cupper-elevate-probe`（cupper-cli，`required-features = ["test-harness"]`） | 101 | — |
| 假 agent `spawn`；xtask 构建探针 | 21 | — |

### 剩下的（等 T038 合并）

- `impl elevation::Requesters for ProcessRequesters`：`find` 的 `Ok` 对应 `Clear`，`Err(signals)` 对应 `Descendant`；`supervisor` 里用它替换 `FailClosed`，宿主路径取 `current_exe` 的真实路径。约 15 行。
- IT-elevate：约 200–230 行。

**推算合计约 770–790 行**，贴着约 800 的线。

### 实现时定下的细节

- **`ProcessRequesters::find` 的返回**：`Result<VerifiedRequester, Vec<String>>`。T038 的接口已在它的分支上提交（`elevation.rs` 的 `Requesters`、`Examined`、`FailClosed`），形状和 notes 里写的一致，合并后只需映射一下。
- **签名身份的显示**：有 Team ID 时为 `identifier (TEAMID)`，否则只有 identifier，即 ad-hoc 或 Apple 自家的代码。未签名时没有这个字段。
- **pid 复用**：按 pid 读到的路径和祖先链，只有在令牌的 `csops` 仍然答得出 cdhash 时才算数，否则按 `unchecked` 处理。
- **探针的路径约定**：同假 agent，在 `target/<profile>/cupper-elevate-probe`。xtask 的 rust 作业在测试前构建它。

## 裁定之后：`bundled_cli` 不读 App 包

- **判定改为两条都满足**：
  - 申请方的可执行文件路径等于按宿主自己的位置推出的 `Contents/Helpers/cupper`，按路径比较；
  - 经审计令牌取申请方**运行中的代码**，它的签名 identifier 是 `dev.cupper.cli`，叶证书（DER）与宿主自己的相同。
- ad-hoc 签名没有证书，一律判为否；宿主自己是 ad-hoc 签名（开发、测试构建）时，就不设这项比对。
- **`cupper-sys` 的配合**：
  - `SigningIdentity` 加了 `leaf_certificate`，取自 `kSecCodeInfoCertificates` 的第一张，用 `SecCertificateCopyData` 取 DER；
  - 新增 `own_signing_identity()`，用 `SecCodeCopySelf` 取宿主自己的签名。
- **取证书的用例**：令牌只能拿到自己建的套接字对端的身份，拿不到别的进程的，所以改用 `/bin/cat` 的静态代码（测试里读文件无妨）。判定本身是纯函数，用造的身份表驱动。
- 协议字段的说明随之改写，并重新生成。

### 行数

相对 d93a4b9 新增 681 行（不含 lock、生成物和文档）。这一项改动本身新增约 130 行：
- `codesign.rs` +97 −43，其中静态代码的创建抽成了共用函数；
- `requester/mod.rs` +88 −22，含判定的表驱动用例。

剩下：接口接线约 15 行，最少场景的 IT-elevate 约 150–180 行。**推算合计约 850–880 行**，略超约 850 的上限，在 950 的硬线以内。

## T038 合并之后（main 合进本分支：87f8938）

### 接口与网关

- **接口**：`impl elevation::Requesters for ProcessRequesters`，`Ok` 对应 `Clear`，`Err(signals)` 对应 `Descendant`。
  - supervisor 用宿主自己的可执行文件和 `own_signing_identity()` 构造它，替换 `FailClosed`。
  - `FailClosed` 留给索引自己的用例，文档已改。
  - T038 用例里造的 `VerifiedRequester` 补了 `bundled_cli`：合并本身没有冲突，但不补编译不过。
- **网关的第二道防线**（T038 评审交给本任务）：`Granted::widen` 也按 `expires_at` 判断，时钟取连接的 `Auditor` 里的那个，`Dispatch::new` 的参数没有增加。
  - 索引 actor 卡住时，授权也不会超过 15 分钟。
  - 用例 `it_elevate_a_grant_ends_at_its_time_in_the_gateway_too`：到期前能调用 write 类方法，推进时钟到期后得到 `forbidden`（`scope`）。

### IT-elevate

用例 `it_elevate_lineage_waits_limits_expiry_grant_and_close`：真实宿主加假 agent，手动时钟，场景都只取最少的用例。

| 场景 | 做法 | 断言 |
|---|---|---|
| 两类后代被拒 | 假 agent 用 `spawn` 拉起探针：加 `--setsid` 的是工具命令；不加的留在适配器的进程组，相当于 MCP 或 hooks | 都是 `forbidden`（`lineage`） |
| 未决与限频 | 由用例自己拉起的探针申请 | 第一个进入未决，第二个 `rate_limited` |
| 过期 | `test/clock_advance` 推进 120 s | 第一个收到 `timeout` |
| 批准与写权限 | 第三个探针申请，用例读 `auth/elevation_get`、再 `grant` 写权限 | 核实信息里 pid、可执行文件正确，`bundled_cli` 为 false；探针随后调用 write 类方法得到 `not_found`，说明 scope 已生效 |
| 断开即失效 | 第四个探针申请后，用例 SIGTERM 它（自己拉起的进程） | 这个申请随之结束，为 `expired` |

- 未决和结束的申请都从 `host.db` 索引流的 `elevation.requested` 和 `elevation.resolved` 事件里轮询得到。
- 连跑 10 次全绿，每次 2–4 s；跑完没有残留的探针、假 agent 或宿主进程。

### 实际规模

不含 `Cargo.lock`、生成物、schema、fixtures 和文档，新增 915 行：合并前 681，合并后 234。超过约 850 的上限，在 950 的硬线以内，符合裁定。

合并后的 234 行：

| 部分 | 新增 |
|---|---|
| 接口与 supervisor | 约 25 |
| 网关的到期判断及其用例 | 约 65 |
| T038 用例的字段 | 2 |
| IT-elevate 与共用函数 | 141 |
