# T176 工作笔记

## 开工前的核对

- `handoff-notes.md` 第一列含 T092、e2e 的行逐行读过：夹具与辅助（`fixture.ts`、`helpers.ts` 的 `sampleNetwork`、`cli.ts`、`dialogs.ts`）、`@focus` 的规矩、CLI 退出码（4 = 宿主校验失败）、上传缓冲的空条目（T029）。
- T092 分支上的 `notes.md`：第 7–12、14 项的估计（约 400 行）就是本任务。
- 宿主的握手（`gateway/handshake.rs`）：
  - `peer` 得到 `pr_cli`，只有 `read`；
  - 非桌面身份带 `role` 时以 `forbidden/role` 拒绝；
  - `dev_key` 先核对对端是不是拉起宿主的进程（不是就 `unauthenticated/bad_key`），再核对证明；
  - 证明缺失或格式不对时是 `unauthenticated/bad_proof`，不是 `invalid_params`。
- 上传缓冲（`gateway/blobs.rs`）：只收有 `blob/put` 权限（`write`）的连接发来的分片，所以只读的 cli 填不了。条目按哈希建；超限或乱序后条目不再持有字节，但条目本身一直留到 `blob/put` 或断开。零长度的分片也会建条目。字节有 64 MiB 的总上限，条目数没有上限。

## Q27 的预备实测（本机，开工前）

- `nettop -L 1 -n -x -p <pid> -p <pid>` 可以一次采多个 pid，一次约 40 ms。
- 只有持有套接字的进程才出现在输出里，所以"每个进程都采到了"不能靠"每个 pid 都出现"来证明。要用正对照：在树的深处（假 agent 拉起的子进程）开一条非回环的流，看采样能不能抓到。
- 正对照不向外网发包：UDP 套接字 `connect` 到文档地址 192.0.2.1 只设默认对端，不发任何包。nettop 能看到这条流（`bytes_out` 为 0）。注意 `nc -u` 在 stdin 结束后不会自己退出，要加 `-w`。
- 局限：`-L 1` 是快照，比采样间隔还短的连接可能漏掉。

## 规模估计（开工前）

| 用例或模块 | 内容 | 估计 |
|---|---|---|
| 原始协议客户端 `raw.ts` | 分帧（复用 host-client 的编解码）、`host/hello`、按给定 `auth` 发 `initialize`（含 `harness` 的证明）、请求、发分片 | 55 |
| 假宿主 `fake-host.ts` | 占住套接字，发 `host/hello`，记录收到的帧，对 `initialize` 回一个不对的 `host_proof` | 45 |
| E2E-SEC-fake-host | 假宿主先占住套接字再启动 App：连接失败，除 `initialize` 外什么都没发，没有密钥材料 | 40 |
| E2E-SEC-cli-fake-host | 同一个假宿主，CLI 以退出码 4 拒绝 | 20 |
| E2E-SEC-no-key-client | `peer` 得到只读的 cli，写被拒；带 `role: window` 被拒；`dev_key`、`harness` 证明错误或缺失得到 `unauthenticated` 并断开 | 55 |
| E2E-SEC-egress（`nightly`）与 Q27 | 跑一个会话期间多次采样整棵树，没有非回环的流；正对照：假 agent 拉起的子进程开一条 UDP 流，采样抓得到；记下耗时和次数 | 65 |
| 上传缓冲 | 用原始客户端复现并量内存；需要改的话，每连接条目数上限加 Rust 单测 | 40 |
| 其他 | `ids.toml` 无需改；`@smoke` 标签、依赖 | 10 |
| 合计 | | 330，按 1.15 倍约 **380** |

停止线约 430，硬停止线 480。每写完一个用例或模块统计一次，记在下面。

## 按模块统计

每个模块提交后用 `git diff --numstat` 统计（相对合并 main 后的 5c2dfc1；不含 notes 和 `pnpm-lock.yaml`）：

| 模块 | 新增 | 删除 | 估计 | 累计新增 |
|---|---|---|---|---|
| `raw.ts`（原始客户端 + 假宿主） | 140 | 0 | 100 | 140 |
| fake-host、cli-fake-host、no-key-client 三条用例 | 115 | 0 | 115 | 255 |
| 上传缓冲的上限与 Rust 单测 | 37 | 1 | 40 | 292 |
| E2E-SEC-egress | 37 | 1 | 65 | 329 |

合计新增 329，在约 430 的停止线以内。

## 过程记录

### 假宿主

- 第一版让假宿主在 App 启动前就占住套接字。宿主校验失败后，App 在 `whenReady` 时弹出"需要重启会话宿主"，这时 e2e 的对话框替身还没装上（它在 `launch` 之后才装），弹出的是真的原生对话框，窗口也一直没出现，用例 60 s 超时。改成"接管"的情形：App 连上真宿主之后，删掉套接字文件、假宿主占住同一路径，再 SIGKILL 真宿主；App 重连时连到假宿主，校验 `host_proof` 失败，弹出的确认框由替身记下。启动时就被占住的情形 e2e 目前测不了。
- App 和 CLI 发给假宿主的都只有 `initialize`，`auth` 只有 `type`、`c_nonce`、`proof` 三个字段，整段内容里没有 `harness_key`。

### 握手的拒绝（实际的错误码）

| 情形 | 结果 |
|---|---|
| `peer` | `pr_cli`，只有 `read`；`session/send` 得到 `forbidden/scope` |
| `peer` 加 `role: window` | `forbidden/role` |
| `codesign`（e2e 构建不提供） | `unauthenticated/bad_key` |
| `dev_key` 证明错误（对端不是拉起宿主的进程） | `unauthenticated/bad_key`，在核对证明之前就拒 |
| `dev_key` 缺证明 | `unauthenticated/bad_proof` |
| `harness` 证明错误、格式不对 | `unauthenticated/bad_proof` |

带密钥的四种都在回错误后断开连接。

### 上传缓冲

- 复现（原始客户端以 `harness` 接入，有 `write`）：50 万个第一片就乱序、或者零长度的分片（每个 46 字节，共 23 MB，约 1.6 s 发完），宿主 RSS 涨了约 122 MB（每个条目约 250 字节），断开后也没有还给系统。
- 修复：每条连接最多 64 个未决的 blob，第 65 个 blob 的分片丢弃，它的 `blob/put` 会因"分片加起来不等于大小"失败。修复后同样发 50 万个分片，RSS 涨 12.6 MB（读缓冲），第二轮 1.5 MB。

### Q27

- 正对照：假 agent 拉起的 `/bin/sh` 再 `exec nc -u -w 20 192.0.2.1 9 < /dev/null`，UDP `connect` 不发包。
- 不设 `runtime.registry_url` 时，宿主启动后会对 npm 仓库建 8 条 HTTPS 连接（运行时安装，SEC-6 允许的两类外发之一）；nettop 采到了，归在宿主的 pid 上。用例把仓库指到回环上关着的端口。
- 本机有代理的 fake-IP（198.18.0.0/15），远端地址显示的是代理分配的地址，不是真实目的地；用例把一切非回环的远端都算外发，所以代理挡不住检查，但要知道连的是谁，得另查 DNS。
- 10 次连跑的数据见汇报。

### 连跑

10 次 `cargo xtask ci e2e`（持机器锁，两段），每次 39 项全过，127–134 s；每次跑完本 worktree 没有残留进程，`$TMPDIR/cupper-e2e/` 为空。

### `ci pr --record`

第一次跑 rust 作业有一项失败：T093 的 `ci::lock::tests::unit_ci_lock_command_passes_its_exit_code_and_is_told_it_holds_the_lock`（"the lock outlived its command"）。它用自己的临时锁文件，和本任务的改动无关；本机单独重跑 11 次都通过，那次运行里 xtask 的测试花了 12 s（平时 3 s）。猜测是负载下别的测试 fork 出的子进程在 exec 之前短暂持有锁文件的描述符。没有改它，在汇报里提出。重跑全部通过，摘要已提交。
