# T045 汇报

- 执行者：子 agent（Rust 工程师，CLI）
- 分支：`feat/T045-cli-client-mirror`
- 完成日期：2026-10-19

T045 已按拆分后的范围（计划 v1.3.6）做完。`cargo xtask ci pr` 全部通过，UNIT-cli-host-verify 和 UNIT-cli-client 都在通过的用例里。Mirror 和 UNIT-mirror 归 T049，设计已经写好，代码一行没写。

## 做了什么

新建 crate `crates/cupper-cli`，包含一个库和 bin `cupper`。

1. **构建种类与 `--build-info`**（`build_kind.rs`）
   - 构建种类由特性决定：
     - 开 `test-harness` 是 e2e；
     - 开 `self-use` 是自用；
     - 开 `dev-auth` 是开发；
     - 都不开是发布。
   - 同时开了多个时，优先级依次是 `test-harness`、`self-use`、`dev-auth`。
   - 编译期常量 `BUILD` 决定本二进制的构建种类。测试可以直接构造 `BuildKind::SelfUse { host_cdhash }`，从而注入 cdhash。
   - 自用构建的 cdhash 来自编译期环境变量 `CUPPER_HOST_CDHASH`。变量缺失或格式不对，编译就失败。
   - `--build-info` 输出一行 JSON：
     - 与宿主相同的字段：`name`、`version`、`features`、`debug_assertions`；
     - 另加 `build`（构建种类）和 `host_check`（校验方式）；
     - 自用构建再带 `host_cdhash`。
   - 默认数据目录：发布 `cupper`、自用 `cupper-local`、开发 `cupper-dev`；e2e 没有默认值。套接字路径和宿主用同一个算法。
   - bin 目前只认 `--build-info`，子命令归 T046。
2. **按构建种类校验宿主**（`verify.rs`）
   - 四个分支都先要求对端与本 CLI 是同一个用户。
   - 发布：要满足签名要求，并检查动态状态字。签名要求在 T180 之前为空，所以现在不接受任何宿主。
   - 自用：对端运行中的 cdhash 要等于编进来的 cdhash。
   - 开发：对端运行中的 cdhash 要等于同目录宿主文件的 cdhash。
     - CLI 的路径先 `canonicalize`，再依次找 `CupperHost.app` 和 `cupper-host`；
     - 输出时标"宿主未经签名校验"（`is_weak`）。
   - e2e：`harness` 证明留给 T039。
   - 校验在发送任何数据之前做。通过后固定宿主的令牌，之后每次读到数据都比对。
   - 发布构建拒绝 `weak_auth` 为 true 的宿主。
3. **连接、握手、请求关联**（`client.rs`）
   - 握手：读 `host/hello`，第一条必须是它；本地用 `PROTOCOL.negotiate` 协商版本，没有共同版本就不发 `initialize`；`initialize` 用 `auth: peer`；最后核对宿主选定的版本和 `weak_auth`。
   - 读套接字一律经 `recv_closing_fds`。以下情况都断开，并记下 `CloseReason`：
     - 收到 fd；
     - 令牌变了，或对端已无法识别；
     - EOF；
     - 分帧错误；
     - 不合协议的消息；
     - 宿主发来请求。
   - 请求按 `id` 配对，通知单独分流。`stream/subscribe` 的结果按线上顺序进通知流，类型是 `Incoming::Subscribed`。
   - `command_id()` 在客户端生成 UUIDv7，用带 reason 的 `expect` 豁免了 IdGen 规则。
4. **给 T039 留的接入点**
   - `ClientAuth` 加一个 `Harness` 变体；
   - `credentials(&hello, &host)` 已经拿到 nonce 和固定下来的令牌，证明在这里算；
   - 在 `Client::handshake` 里，`check.accept` 的旁边校验 `host_proof`。
   - 细节见 `notes.md`。
5. **拆分后的收尾**
   - 去掉了文档里指向 Mirror 的断链，改成文字"T049"；顺带修掉一条指向私有项的文档链接。现在 `cargo doc` 没有警告。
   - 补了两个拒绝路径的用例：收到 fd 时断开、校验后换了一个进程在宿主那端读写时断开。
   - `notes.md` 写明了收窄后的范围和最终行数。"Mirror 的设计"一节保留给 T049。

## 关键取舍

- **订阅结果不走 oneshot。** 如果响应和通知走两条通道，客户端分不清一条 `stream/events` 是旧订阅还在路上的，还是新订阅的。这条已写进设计 5.2.14 末尾的"T045 实现时的裁定"。
- **自用构建缺 cdhash 时编译失败。** 如果改成没有变量就退回开发构建，出错时会悄悄降级成弱校验。现在缺少变量时直接编译不过，是失败即关闭的做法。CI 的 `--all-features` 由 `test-harness` 优先，用不到这个变量。
- **签名要求为空时，发布版 CLI 不接受任何宿主。** 这和 T030 的 `ui_requirement` 是同一个做法。不带 Team ID 的签名要求太宽，任何持有 Developer ID 的人签的同名程序都能通过。
- **e2e 分支在 T039 之前只查同用户和固定令牌，输出标为未校验。** e2e 构建只用于测试。
- **tokio 开了用不到的 `time` 特性。** 不开的话，单独对本 crate 跑 clippy 时，`clippy.toml` 里 tokio 定时器那几条禁用条目解析不到，在 `-D warnings` 下会报错。
- **SP-4a 里有几样没有迁移**，由需要它们的任务再加：
  - `pause_reading`、`max_frame_seen`；
  - blob 分片的转交；
  - `next_timeout`，它受时间规则约束。
- **"另一个进程接手"的用例照搬了 T030 IT-pinned-token 的做法。** 测试里用 `std::process::Command` 拉起 `/bin/cat`，让它往宿主那端写一帧。`cupper_sys::spawn` 只支持 null 和管道，没法把套接字交给子进程。

## 验证方式

- 在干净的 worktree 里跑了 `cargo xtask ci pr --record docs/tasks/T045-cli-client-mirror/ci/`，提交 `c22be2f` 上全部通过，摘要文件是 `pr-c22be2f3a31b.json`：
  - rust：799 通过；
  - protocol：176 通过；
  - ts：232 通过；
  - license：通过，没有遇到网络问题。
- 另外在本 crate 上，分别以无特性、`--features dev-auth`、`--all-features` 跑了 `cargo test -p cupper-cli`（11 例）和 `cargo clippy --all-targets -- -D warnings`，都通过。`cargo doc` 没有警告。
- 手动核对过自用构建：带有效 cdhash 能构建，`--build-info` 里有 `host_cdhash`；不带变量，或者值不足 40 位，都编译失败。
- 发布构建的依赖里不带 `cupper-sys` 的 `test-util`，`cargo tree` 确认过。
- 对照验收标准：
  - [x] **UNIT-cli-host-verify**：
    - 四种构建各自的校验分支，其中开发构建在测试二进制旁边找不到宿主文件时拒绝连接；
    - 不满足签名要求时拒绝；
    - 同目录宿主文件的 cdhash 比对，覆盖相符、不符、文件缺失三种情况；
    - cdhash 不符时拒绝连接，并且一个字节都没发；
    - 发布构建拒绝 `weak_auth`；
    - `--build-info`、数据目录与套接字路径。
  - [x] **UNIT-cli-client**：
    - 握手后发出两个请求，宿主倒序回复，结果仍各自配对；
    - 订阅结果和通知保持线上顺序；
    - 宿主发来请求时断开；
    - 握手失败的四种情况：宿主直接关闭、第一条不是 hello、没有共同版本、宿主选了不对的版本；
    - 收到 fd 时断开；
    - 另一个进程在宿主那端写数据时断开；
    - `command_id` 是 UUIDv7。
  - [x] 拒绝的路径都有用例。
  - [x] 测试名都以用例编号的 snake_case 开头：`unit_cli_host_verify_…`、`unit_cli_client_…`。
  - [x] CI 摘要已随任务提交。
- **行数**：新增 1,206 行，其中代码 741 行、测试 465 行。比这几个模块开工前的估计多 32%，在约 1,500 行的上限以内。

## 提交列表

```
9e34d10 docs(task): record the T045 pr CI summary
c22be2f docs(task): record the narrowed T045 scope and its final line count
0a4a93b test(cli): end the connection on passed fds or another writer
9778f16 docs(cli): point to T049 for the stream mirror instead of linking it
6706e5e Merge branch 'main' into feat/T045-cli-client-mirror
cb92d0d docs(task): record the T045 line counts so far and stop before the mirror
3254b22 feat(cli): connect, handshake and correlate requests with the host
4a5c179 feat(cli): check the host by build kind before talking to it
86dc1a4 feat(cli): add the cupper-cli crate with its build kinds and --build-info
36081bc docs(task): estimate the T045 size by module
```

## 遗留问题 / 范围外发现

- Mirror 和 UNIT-mirror 归 T049。设计在 `notes.md` 的"Mirror 的设计"一节，等你通知后在 T049 的分支上做。
- **T005 用到的接口**：
  - 构建命令：`CUPPER_HOST_CDHASH=<cdhash> cargo build -p cupper-cli --release --features self-use`；
  - 核对产物时，可以比较 CLI 的 `cupper --build-info` 输出里的 `host_cdhash` 和宿主的 cdhash。
- **T039 要做的**（接入点见 `notes.md`）：
  - 实现 `ClientAuth::Harness` 和 `--harness-key-fd`；
  - 校验 `host_proof`；
  - 证明落地后，把 `HostCheck::Harness` 的 `is_weak` 改为 false。
- **T180 要做的**：设置 `verify::HOST_REQUIREMENT`。在那之前，发布版 CLI 不接受任何宿主。
