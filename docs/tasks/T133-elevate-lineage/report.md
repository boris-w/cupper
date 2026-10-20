# T133 汇报

- 执行者：子 agent（Rust 工程师；T043、T055、T008、T116 的执行者）
- 分支：`feat/T133-elevate-lineage`（合并提交 87f8938 已带入 T038）
- 完成日期：2026-10-20

T133 已完成。宿主现在用真实的血缘检查和核实信息代替 `FailClosed`。网关按到期时间自行收回授权，作为第二道防线。IT-elevate 连跑 10 次全绿。PR 套件在干净的 worktree 里第一次运行就通过。规模新增 915 行，在 950 的硬线以内。

## 做了什么

1. **`cupper-sys`**（`unsafe` 都在这里）：
   - `has_tty(pid)`：进程有没有控制终端；
   - `AuditToken::signing_identity()`：取令牌所指的运行中代码的签名，含 identifier、Team ID、叶证书（DER）；
   - `own_signing_identity()`：经 `SecCodeCopySelf` 取宿主自己的签名；
   - `setsid()` 包装。
2. **血缘纯函数** `requester::lineage::signals`，三个信号：
   - 同一会话：请求方的 sid 是适配器的 pid，并且请求方晚于适配器启动；
   - 同一进程组：pgid 相同，同样要求晚于适配器启动；
   - 祖先：祖先链里有 pid 和启动时间都与适配器相符的进程。

   适配器取 `adapters` 表里未结束的行。某行的 pid 现在跑着启动时间不同的进程时，这行作废（pid 被复用了）。适配器已退出但它的会话或进程组还在时，仍然算数。
3. **核实信息**：`ProcessRequesters` 实现 T038 的 `elevation::Requesters`。
   - 命中血缘信号，或无法检查时，结果为 `Descendant`，带上信号名（检查不了时是 `unchecked`）。
   - 否则结果为 `Clear`，给出：
     - 可执行文件路径；
     - 签名身份：有 Team ID 时为 `identifier (TEAMID)`，否则只有 identifier；
     - pid；
     - 父进程链，每一层都带可执行文件路径；
     - 有没有终端；
     - "无法确认来源"：没有终端、父进程就是 launchd、或者链中途断了；
     - `bundled_cli`，判定按裁定：路径是宿主旁边的 `Contents/Helpers/cupper`，并且运行中代码的签名 identifier 是 `dev.cupper.cli`、叶证书与宿主自己的相同。全程不读 App 包；ad-hoc 签名一律判为否。
   - 按 pid 读到的信息，只有在令牌仍然指着那个进程时才算数。
   - supervisor 已用它替换 `FailClosed`；`FailClosed` 留给索引自己的用例。
4. **协议**：`VerifiedRequester.bundled_cli`，已重新生成 TS 类型和 schema，fixture 同步更新。
5. **网关的第二道防线**：`Granted::widen` 也按 `expires_at` 判断。时钟取连接的 `Auditor` 里的那个，所以 `Dispatch::new` 的参数个数没变。
6. **探针与假 agent**：
   - 探针 `cupper-elevate-probe` 在 `cupper-cli` 下，需要 `test-harness` 特性，发布构建里没有。它以 `cli` 身份申请 `write`，按 JSON 行写下每一步结果；带 `--setsid` 时先自成会话；获批后调用一个 write 类方法，然后退出。
   - xtask 的 rust 作业在测试前构建探针。
   - 假 agent 加了 `spawn <program> [args...]`，设计 09 的命令表补了一行。

## 关键取舍

- **代码放在 `requester/` 模块**（已认可），合并 T038 时没有冲突。T038 的用例造 `VerifiedRequester` 时缺新字段，编译不过，我补上了 `bundled_cli: false`。
- **IT-elevate 里未决和结束的申请**：从 `host.db` 里索引流的 `elevation.requested` 和 `elevation.resolved` 事件轮询得到，不额外订阅。
- **"断开即失效"的验证方式**：用一个未决申请的连接断开来验证。已授权连接断开后撤销，T038 的 IT-elevation-grant 已经覆盖。

## 验证方式

- **PR 套件**：`cargo xtask ci pr --record docs/tasks/T133-elevate-lineage/ci/`，第一次运行就全部通过，摘要是 `pr-d9f148bb7c45.json`：

  | 套件 | 通过 |
  |---|---|
  | rust | 1821 |
  | protocol | 185 |
  | ts | 379 |
  | license | 1 |
  | testdata-scan | 3 |
  | ui | 7 |

- **IT-elevate**（`it_elevate_lineage_waits_limits_expiry_grant_and_close`），真实宿主加假 agent，手动时钟：

  | 场景 | 结果 |
  |---|---|
  | 假 agent 拉起的探针：加 `--setsid`（工具命令）和不加（MCP 或 hooks） | 都是 `forbidden`（`lineage`） |
  | 用例自己拉起的探针 | 进入未决；第二个申请被 `rate_limited` |
  | 经 `test/clock_advance` 推进 120 s | 第一个申请 `timeout` |
  | 第三个探针 | `auth/elevation_get` 的 pid、可执行文件正确，`bundled_cli` 为 false；`grant` 写权限后，它调用 write 类方法得到 `not_found`，说明 scope 已生效 |
  | 第四个探针申请后被 SIGTERM | 申请以 `expired` 结束 |

  - **连跑 10 次全绿**，每次 1.8–4.2 s。lifecycle 整个用例文件另跑 3 次，也全绿。
  - 跑完确认没有残留的探针、假 agent 或宿主进程；只向用例自己拉起的进程发信号。
- **新增的单元用例**：
  - UNIT-lineage-signals：每个信号、pid 复用、被 launchd 收养的命令、适配器已退出、空链；
  - `examine`（用本进程）；
  - `bundled_cli` 的判定表；
  - `cupper-sys`：终端、ad-hoc 签名没有证书、`/bin/cat` 的叶证书；
  - 网关到期：`it_elevate_a_grant_ends_at_its_time_in_the_gateway_too`。
- 测试名都以已登记的 IT-elevate、UNIT-lineage-signals 开头。

## 提交列表

```
6ef9832 docs(task): T133 CI summary
d9f148b docs(task): T133 notes: wiring, the gateway's check, IT-elevate, sizes
07b941e test(host): IT-elevate on a real host with the fake agent
e82f8be fix(host): the gateway ends a grant at its time too
7cf7242 feat(host): the host examines requesters instead of failing closed
87f8938 Merge branch 'main' into feat/T133-elevate-lineage
395c28d docs(task): T133 notes after the bundled_cli ruling
d79d310 fix(host): tell the app's CLI without reading the app bundle
4b82877 feat(sys): signing certificates, and the process's own signature
36662ee docs(task): T133 progress while T038 is pending
b1ef753 test(cli): IT-elevate's probe, in test builds only
f9fbed2 feat(fake-agent): `spawn <program> [args...]`
b0c5c5e feat(host): examine an elevation's requester
0b3146b feat(host): the lineage signals of an elevation request
d983ff8 feat(protocol): `VerifiedRequester.bundled_cli`
e0be841 feat(sys): controlling terminal, signing identity, setsid
d93a4b9 docs(task): T133 size estimate and approach
```

## 遗留问题 / 范围外发现

- **规模**：不含 lock、生成物、schema、fixtures 和文档，新增 915 行，其中合并前 681、合并后 234。超过约 850 的上限，在 950 的硬线以内，符合裁定。
- **`bundled_cli` 在开发和测试构建里恒为否**：宿主是 ad-hoc 签名，没有证书可比。要等发布构建（Developer ID 签名）才会出现为真的情况；这条路径目前只有纯函数的表驱动用例覆盖。
- **已知局限**（设计 5.8.4 写明）：经 nohup、双重 fork 或 disown 脱离父进程的命令，三个血缘信号都会失效。UNIT-lineage-signals 里有一条"被 launchd 收养"的用例，断言它不被判为后代，以此记录这一点。