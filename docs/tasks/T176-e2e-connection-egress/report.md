# T176 汇报（安全 e2e II：连接层与外发）

- 执行者：子 agent（测试工程师）
- 分支：`test/T176-e2e-connection-egress`
- 完成日期：2026-10-20

T176 已完成。四个编号全部通过：标为 `pr` 的三个打了 `@smoke`，egress 标 `nightly`。Q27 的结论是可行，但有局限，写在下面。上传缓冲按你的要求就地修了，附 Rust 单测。用 `cargo xtask ci e2e` 连跑 10 次，每次 39 项全绿，跑完没有残留进程。`ci pr --record` 第一次撞上 T093 的一个不稳定用例，重跑全部通过，摘要已提交。规模：新增 329 行，开工前估计约 380。这次每写完一个模块统计一次，数字记在 `notes.md`。

## 做了什么

**`tools/e2e/src/raw.ts`**（只用于测试）
- 原始客户端：自己按 `u32 长度 | u8 类型 | 负载` 分帧，直接连宿主的套接字。`initialize` 里的 `auth` 由用例给，可以是 `peer`、`codesign`，也可以是证明错误或缺失的 `dev_key`、`harness`；`keyProof` 按 5.8.1 的格式计算证明。也能发 `0x02` 分片。
- 假宿主：占住 `<数据目录>/run/host.sock`，像宿主一样先发 `host/hello`。对 `initialize` 回协议金样例里的结果，但 `host_proof` 是随机值，它不可能算对。对方发来的每条消息都记下来。

**`tools/e2e/src/connection.spec.ts`**
- **E2E-SEC-fake-host**（`@smoke`）：
  - App 连上真宿主后，删掉套接字文件，由假宿主占住同一路径，再 SIGKILL 真宿主。
  - App 重连时连到假宿主，`host_proof` 校验失败，弹出"需要重启会话宿主"，由对话框替身记下，用例点取消。
  - 假宿主收到的只有 `initialize`：`auth` 只含 `type`、`c_nonce`、`proof`，整段内容里没有 `harness_key`，之后也没有任何命令。
- **E2E-SEC-cli-fake-host**（`@smoke`）：假宿主占住套接字时，CLI（`harness`）以退出码 4 拒绝；假宿主同样只收到带证明的 `initialize`。
- **E2E-SEC-no-key-client**（`@smoke`）：结果和设计的对照见"关键取舍"。
- **E2E-SEC-egress**（`nightly`）：
  - 跑一个会话期间，用 `sampleNetwork` 对 App 的整棵进程树（App、宿主、假 agent 及其子进程）采样 15 次，断言没有一条流离开本机。
  - 正对照：假 agent 拉起 `nc -u -w 20 192.0.2.1 9`，UDP `connect` 到文档地址，不发任何包。nettop 必须看到这条流，否则用例失败，所以"什么都没采到"不会被当成通过。
  - 宿主启动时的运行时安装（SEC-6 允许的外发）用 `runtime.registry_url` 指到回环上一个关着的端口。

**上传缓冲**（`crates/cupper-host/src/gateway/blobs.rs`，单独一个 fix 提交，+37 −1）
- 每条连接最多 64 个未决的 blob。第 65 个 blob 的分片丢弃，它的 `blob/put` 会以"分片加起来不等于大小"失败。零长度分片、乱序或超限后留下的空条目都算在 64 个里面。接口没有改。
- Rust 单测：`uploads_hold_at_most_max_pending_blobs_even_without_bytes`。它和旁边原有的上传单测一样不带编号，因为 T176 段里只有四个 E2E 编号。

## 关键取舍

**握手的拒绝：实际错误码与设计 5.8 对照**

| 情形 | 实际 | 设计里有没有写 |
|---|---|---|
| `peer` | `pr_cli`，只有 `read`；`session/send` 被拒，`forbidden/scope` | 有（cli 只读） |
| `peer` 加 `role: window` | `forbidden/role` | 5.8.2 只说只有桌面有角色，没写这种请求怎么拒 |
| `codesign`（e2e 构建不提供） | `unauthenticated/bad_key` | 没写 |
| `dev_key` 证明错误，对端不是拉起宿主的进程 | `unauthenticated/bad_key`，在核对证明**之前**就拒 | 写了"只接受拉起宿主的进程"，没写先后 |
| `dev_key` 缺证明 | `unauthenticated/bad_proof` | 有（错误或缺失时 `unauthenticated`） |
| `harness` 证明错误或格式不对 | `unauthenticated/bad_proof` | 有 |

带密钥的四种情形都在回错误后断开连接，没有降级成只读。表里标"没写"的三行，由你决定是否补进 5.8。

**假宿主改测"接管"**
- 第一版让假宿主在 App 启动前就占住套接字。App 在 `whenReady` 时就弹出重启确认，这时 e2e 的对话框替身还没装上（`launch` 之后才装），弹出的是真的原生对话框，窗口也一直没出现，用例 60 s 超时。
- 所以改测接管：App 连上真宿主之后，再由假宿主占住套接字。"启动时套接字就被占住"这条路径 e2e 目前测不了，见遗留问题。

**上传缓冲的评估**
- 只有带 `write` 的连接能上传：`Dispatch::chunk` 会丢掉没有 `blob/put` 权限的连接发来的分片。所以只读的 cli 填不了缓冲，能填的是窗口、主进程、提权到 `write` 的 CLI 和测试身份。
- 复现：原始客户端以 `harness` 接入，发 50 万个第一片就乱序、或者零长度的分片。每片 46 字节，共 23 MB，约 1.6 s 发完。
  - 宿主 RSS 涨了约 122 MB，每个条目约 250 字节，断开后也没有还给系统。
  - 两种分片各量过一次，结果一样。
  - 渲染进程一旦被攻破，就能用这个办法耗尽宿主的内存。
- 修复后用同样的 50 万个分片再量：RSS 涨 12.6 MB（读缓冲），第二轮 1.5 MB。

**Q27：可行，但有局限**
- 做法：`nettop -L 1 -n -x` 后面跟多个 `-p`，一次采完整棵树。只有持有套接字的进程才会出现在输出里，所以靠正对照证明采样能看到树的深处。
- 实测数据（10 次连跑，每次 15 个样本）：
  - 树里 10 个进程，每次有流的是其中 2 个：App 主进程（回环上的调试端口）和正对照的 `nc`。
  - 每个样本耗时 125–225 ms，各次的中位数约 145–205 ms。
  - 10 次都在第 2 个样本抓到了正对照。第 1 个样本时它还没被拉起，从开始采样到抓到不到 0.5 s。
- 额外的实证：不设 `registry_url` 时，宿主启动后对 npm 仓库建了 8 条 HTTPS 连接（运行时安装），nettop 采到了，并且归在宿主的 pid 上。
- 局限：
  - 采样是快照。这里一个采样周期约 350 ms（采一次约 150 ms，加 200 ms 间隔），比这还短的连接可能漏掉。所以它能证明"运行期间没有持续的外发"，证明不了"一个包都没发"。要更严格，需要 pf 或抓包（要 root），或者本地代理（只管得到认代理的客户端）。
  - 本机有代理的 fake-IP（198.18.0.0/15），nettop 显示的远端是代理分配的地址，不是真实目的地。用例把一切非回环的远端都算作外发，所以代理挡不住检查；但要知道连的是谁，得另查 DNS。

## 验证方式

- **四个编号**：用 `pnpm --filter @cupper/e2e run test:e2e:one` 持锁单独跑过多次，都通过。
- **连跑 10 次**：用 `cargo xtask ci e2e`（持机器锁，T167 的两段跑法）。每次 39 项全过（包括本任务的 4 项），127–134 s。每次跑完，本 worktree 的 e2e App、宿主、假 agent、`nc` 都没有残留，`$TMPDIR/cupper-e2e/` 为空。
- **上传缓冲**：新单测和原有单测都通过。复现和修复后的测量用的是临时用例，量完就删了，没有提交。
- **CI**：在干净的 worktree 里跑 `cargo xtask ci pr --record docs/tasks/T176-e2e-connection-egress/ci/`。
  - 第一次 rust 作业有一项失败：T093 的 `ci::lock::tests::unit_ci_lock_command_passes_its_exit_code_and_is_told_it_holds_the_lock`（"the lock outlived its command"）。它用自己的临时锁文件，和本任务的改动无关。本机单独重跑 11 次都通过。
  - 重跑全部通过，摘要 `pr-e63551dc6547.json` 已提交。跑的期间没有往工作区写文件。

  | 作业 | 结果 |
  |---|---|
  | rust | 1,994 通过 |
  | protocol | 185 通过 |
  | ts | 473 通过 |
  | license | 通过 |
  | testdata-scan | 3 通过（本机没有敏感词表，那一步跳过） |
  | ui | 按路径筛选跳过 |
  | e2e | 22 通过（`@smoke` 两段） |

- **试合并**：用 `git merge-tree` 试合并最新的 main，没有冲突。
- **规模**（相对合并 main 后的 5c2dfc1，不含 notes 和 lock 文件）：

  | 模块 | 新增 | 估计 |
  |---|---|---|
  | `raw.ts` | 140 | 100 |
  | 三条用例 | 115 | 115 |
  | 上传缓冲 | 37 | 40 |
  | egress | 37 | 65 |
  | 合计 | 329 | 380 |

## 提交列表

```
f24791b docs(T176): record the ci pr summary and a flaky lock test
e63551d docs(T176): record the size per module, the refusals, the buffer and Q27
1ac2fd2 test(e2e): E2E-SEC-egress samples the app's tree with nettop
4098ef8 fix(host): cap the blobs a connection may have pending uploads of
4901f54 test(e2e): a fake host and clients without the key
5c2dfc1 Merge branch 'main' into test/T176-e2e-connection-egress（编排者合并）
167386b docs(T176): record the checks, the nettop trial and the size estimate
```

## 遗留问题 / 范围外发现

- **所有 e2e 用例都会连公网的 npm 仓库**：夹具没有设 `runtime.registry_url`，每个 e2e 用例启动的宿主都会对 registry.npmjs.org 建 8 条 HTTPS 连接，去下载 claude-agent-acp 的依赖树，尽管用的是假 agent。建议在夹具里默认指向回环上关着的端口或本地夹具仓库；要先确认安装失败的状态不影响别的用例的界面。我只在 egress 用例里设了。
- **启动时的对话框接不住**：替身在 `launch` 之后才装，所以 App 启动时就弹的对话框（宿主校验失败时的重启确认）在 e2e 里是真的原生对话框，窗口要等它关掉才出现。"启动时套接字就被占住"这条路径因此测不了。可以考虑在 e2e 构建的主进程里提前装替身。
- **e2e 构建的 CLI 不带密钥时是弱校验**：只按 `peer` 连接，会连上假宿主（`verify.rs` 注明了这是弱校验）。本任务只测了带 `harness` 密钥的情形（退出码 4）。
- **设计 5.8 可补的三点**：`role` 用在非桌面身份上时的拒绝方式、e2e 构建不提供 `codesign`、`dev_key` 先核对"拉起宿主的进程"再核对证明。见"关键取舍"的表。
- **T093 的不稳定用例**：`unit_ci_lock_command_passes_its_exit_code_and_is_told_it_holds_the_lock` 在 `ci pr` 的负载下失败过一次（那次 xtask 的测试花了 12 s，平时 3 s）。猜测是同一进程里别的测试 fork 出的子进程在 exec 之前短暂持有锁文件的描述符。没有改，交给 T093 的执行者。
- **Q27 与设计 5.9.4**：等你按上面的结论更新。
