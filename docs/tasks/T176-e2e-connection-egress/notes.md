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
