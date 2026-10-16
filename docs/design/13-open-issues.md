# 5.13 待定问题与 ADR 修订建议

- 所属：`docs/02-design.md` 第 5 节
- 本设计只细化 ADR，没有推翻任何一项。下面列出设计阶段定不下来、需要在实现中验证的事项，以及 ADR 之间或 ADR 与实测之间的不一致，每项附建议

## 5.13.1 待定问题

| # | 问题 | 涉及 | 影响 | 建议 | 何时、怎样定 |
|---|---|---|---|---|---|
| Q1 | ADR-03 写"主进程是到宿主的唯一连接，所有窗口共用"，但主进程不解析入方向的 JSON，无法把事件分给多个窗口，也认不出自己特权请求的响应 | ADR-03 | 桌面端连接结构 | 主进程持有全部连接：一条自用的 control 连接 + 每个窗口一条数据连接（[5.7.1](07-desktop.md#571-进程与连接)）。套接字不进渲染进程的安全意图不变 | 设计评审 |
| Q2 | ADR-01 规定系统通知全部由宿主 helper 弹出，但 helper 的通知身份要等 SP-10（M2 前）才验证，而 AP-3 是 M1b 的 P0 | ADR-01、SP-10、AP-3 | M1b 能否交付通知 | 把 SP-10 中"通知身份"一项提前到 M1b 开始前做（需要用户在场，会弹系统授权框）；不通过就在 M1 用技术选型讨论会定的后备方案：宿主照常写 `notification.intent`，由 UI 主进程弹 | M1b 开始前 |
| Q3 | 重签后只带 `allow-jit` 的 Node 能否通过公证、解出后首次执行有多慢（ADR-08 待验证） | ADR-08 | 首次使用的体验 | 第一次有 Developer ID 的 CI 构建时实测。后备：把重签的 `node` 作为普通 Mach-O 放进 `CupperHost.app/Contents/Helpers/`，随 App 一起公证，安装时复制（安装后多占约 100 MB，下载包增加不多） | M1b 发布任务 |
| Q4 | ADR-08 没写明 codex-acp 是随包还是首次运行时下载 | ADR-08、ADR-11 | Codex 是否需要联网安装 | 随包（ADR-11 允许）；清单记录文件哈希，CI 构建时按 npm 的 SRI 校验上游文件 | 设计评审 |
| Q5 | bundle identifier 的反向域名前缀还没定 | ADR-01、ADR-05 | 写进签名要求、TCC、通知身份；以后改动会丢失系统授权 | 由用户在 `gate/design` 时确定，之后不再改 | gate/design |
| Q6 | 只出 arm64 构建，还是同时出 x64 | 平台约束 | macOS 14–26 仍支持部分 Intel 机型 | M1 只出 arm64（自用优先）；以后需要时加一个构建目标即可，不涉及兼容代码 | gate/design，用户确认 |
| Q7 | Claude 的 `allow-with-updates` 在文件编辑上只对本会话有效，在 Bash 上写持久规则，option_id 相同 | SP-1、AP-2 | 持久选项的标注是否准确 | 按工具区分，判断不出来时按持久处理；向 claude-agent-acp 上游反馈，建议用不同的 ID 或在 `_meta` 标出作用范围 | 实现时；上游 |
| Q8 | 同一个适配器上多个会话**并发** prompt 是否可靠（ADR-09 的前提，SP-1 只测过同一会话的并发） | ADR-09 | 多会话并行（S1、NFR-11） | 在兼容剧本里加这一项，M1a 早期先测 Claude，M1b 测 Codex。不行的后备：该 agent 的 prompt 全局串行，或退回每个会话一个适配器进程（只对出问题的那种 agent） | M1a、M1b |
| Q9 | ACP Rust SDK 3.3 是否支持发送 `_session/steering` 这类扩展请求、能否收到 `_auth/status_update` 和 `$/cancel_request` | ADR-02、ADR-07 | Steer、登录检测、撤回审批 | M1a 第一个驱动任务先做技术验证；不支持的部分由线路分接层在 stdio 上直接处理（[5.6.10](06-agent-driver.md#5610-线路分接层)） | M1a |
| Q10 | Claude 没有运行状态信号，自发回合何时结束只能推断 | ADR-06 | 状态显示 | 以"带 `cost` 的 `usage_update`"或"10 秒无更新且没有运行中的工具"判定；误判只影响状态显示，不影响历史。用 SP-1 s06 的录制和兼容矩阵验证 | M1a |
| Q11 | ADR-06 写"Codex 有 `threadStatus` 时优先采用"，但 SP-1 s05 中计划确认请求发出时线程状态已是 `idle` | ADR-06、SP-1 | 回合会被提前结束 | 有 prompt 在途时以 prompt 响应为准；`threadStatus` 只用于显示运行状态和界定自发回合（K12） | 设计评审 |
| Q12 | Claude 在 SDK 会话里拿不到 TodoWrite 的计划（SP-1），haiku 上也没有思考流 | MS-4、MS-2 | MS-4 的"每一步的状态"对 Claude 可能做不到 | 兼容矩阵用 sonnet 或 opus 补测；仍拿不到时，PRD 的 MS-4 对 Claude 改为"计划模式的 Markdown 计划"，按 AG-4 不显示待办清单 | M1b 开始前，需要用户确认验收口径 |
| Q13 | GT-3 要求取消暂存，必须能看到已暂存的内容，而 PRD 把"其余审查范围"（F2，含 Staged）放在 P1 | PRD | 审查面板的范围 | M1 提供最小的 `staged` 范围，只服务于取消暂存，不算提前做 F2 | 用户确认 |
| Q14 | ADR-07 写"丢弃晚到的工具输出"，但完全丢弃会让用户不知道命令在取消后还在运行 | ADR-07 | 用户对副作用的知情 | 不进入新回合，同时在原工具条目上标 `late_output_discarded` | 设计评审 |
| Q15 | SQLite 用 `synchronous=NORMAL` 还是 `FULL`（ADR-06 要求 M1 实测） | ADR-06、NFR-9 | 断电后丢最近的事务 | 在 macOS 虚拟机里做强制断电测试；没有条件时先用 `NORMAL`，并在回合结束时做一次 WAL 检查点 | M1a |
| Q16 | 保留参数：会话视图窗口 20 个回合、日志保留 1 万条或 7 天、原始帧 14 天且不超过 512 MB | ADR-06 | 内存、磁盘、断线续传的覆盖范围 | 先按这些值实现，M1b 用实际数据调整 | M1b |
| Q17 | git 快照对象的回收 | ADR-06、ADR-10 | 磁盘占用 | M1 只累积并显示大小，M2 按"重新打包仍需要的 tree"回收（[5.4.5](04-data-model.md#545-保留与回收)） | M2 |
| Q18 | 从登录 shell 取环境：会执行用户的 rc 文件，可能很慢、可能有输出 | ADR-08、ADR-10 | 找不到 CLI、git、git-lfs | 只在宿主启动和用户刷新时执行，5 秒超时，用标记截取输出；失败时退回常见路径，设置里可以手动指定 PATH | M1a |
| Q19 | npm、Homebrew、原生安装器等安装方式下，CLI 原生二进制的位置各不相同；只有 JS 入口的安装无法使用 | ADR-08、SP-2 遗留 | CLI 发现 | 做成数据驱动的解析器，用目录树样例测试，实现时逐个核实；只有 JS 入口的安装提示改装原生版本 | M1a（Claude）、M1b（Codex） |
| Q20 | electron-updater 7 用 `autoInstallEvent` 取代 `autoInstallOnAppQuit`；Squirrel.Mac 的 ShipIt 是否"移走旧包"而不是原地覆盖还没实测；安装时 UI 进程会临时监听 127.0.0.1 | ADR-01、ADR-03 | 升级安全 | 首次正式发布前做升级演练；迁移到 7.x 时把配置改为 `manual`；临时端口写进安全说明 | M1b 发布任务 |
| Q21 | 宿主由 UI 用 `posix_spawn` 拉起时，TCC 的"负责进程"可能是 UI，影响宿主的系统授权和通知归属 | ADR-01、SP-10 | 通知、防休眠以外的系统授权 | 由 SP-10 验证；有问题时按 ADR-01 的后备方案（M1 就用 LaunchAgent），或改用 LaunchServices 拉起 helper | SP-10 |
| Q22 | 退化认证模式（开发构建、自行构建）下，一次性密钥只有拉起宿主的那个 UI 实例知道，UI 崩溃重启后连不上旧宿主，只能重启宿主 | ADR-05、NFR-8 | 这两种构建下 NFR-8 不成立 | 接受（发布构建不受影响），写进自行构建的文档。把密钥存进钥匙串或文件都会进一步削弱认证，不建议 | 设计评审，用户确认 |
| Q23 | 宿主环境下的 git 凭据：SSH agent、GPG 签名提交（终端 pinentry 没有 TTY 会失败）、credential helper | ADR-10 | GT-4 推送和提交 | 实现时逐项验证；失败时错误信息给出指引（例如改用图形版 pinentry） | M1b |
| Q24 | SP-4b 只在 macOS 26.4 上验证了对端校验的语义（`LOCAL_PEERTOKEN`、csops、错误码） | ADR-05 | 最低支持的 macOS 14、15 | CI 每晚在 macOS 14 runner 上跑 `cupper-sys` 的校验测试；macOS 15 在有 runner 时补上 | M1a |
| Q25 | Codex 的 `revise_plan` 是否结束回合、Codex 权限模式的完整取值和风险等级都没有实测 | SP-1 | 选项标注、完全访问的识别 | 兼容矩阵补测；在此之前，不认识的权限模式值一律按完全访问处理（需要原生确认） | M1b |

## 5.13.2 对 ADR 的修订建议

全部是细化或补充，`gate/design` 时随 ADR 一起交用户确认。

| ADR | 建议修订 | 出处 |
|---|---|---|
| ADR-01 | 运行时载荷放在 `CupperHost.app/Contents/Resources/runtime/`；"运行中不读包内资源"的唯一例外是自检通过后、按内嵌清单逐文件核对哈希的复制；Electron 的 userData 设到数据目录下的 `ui/`（避免与 "Cupper" 在大小写不敏感的卷上冲突）；宿主退出码约定（3、4、5）；通知的后备方案提前（Q2）；拉起方式与 TCC（Q21） | 5.2.12、5.2.13、5.4.1 |
| ADR-02 | crate 表补充：`cupper-sys`（集中全部 `unsafe`，是"只在 sys 模块"在多 crate 下的落法）、`cupper-desktop-native`（napi-rs，桌面主进程校验宿主）、`xtask`；协议的 `Method` trait 声明 `SCOPE` 和 `IDEMPOTENCY` | 5.1.3 |
| ADR-03 | 连接模型改为 control 连接 + 每窗口数据连接（Q1）；打包工具定为 electron-builder + electron-updater（5.10.4）；Electron 起始版本 44.8.0 及跟进策略；Vite 直接构建三个入口；宿主侧增加 400 ms 的最短回答间隔，作为渲染进程 500 ms 输入保护的后盾；e2e 构建的宿主同时开 `dev-auth` 和 `test-harness` | 5.7、5.10 |
| ADR-04 | 增加 `host/hello`（nonce）；`initialize` 增加 `nonce` 和 `role`；命令为"受理"语义，结果与它产生的事件同事务落盘；账本按 principal 稳定 ID；`session/create` 必须带第一条消息（与 ADR-09 一致）；`history/page` 读回合快照；`blob/put` 先发分片再发请求；hunk 的前置条件具体化为 `diff_token`；错误码增加 `git_error` 并给出 reason 清单；方法清单以 5.3.4 为准 | 5.3 |
| ADR-05 | principal 增加角色（control、window）和 `privileged`、`ui_local` 两个 scope，给出完整的权限矩阵；血缘检查的信号具体化为"适配器进程组、宿主的会话 ID、祖先链"；UI 校验宿主时之后每次读取都比对令牌；退化模式的局限（Q22） | 5.8 |
| ADR-06 | 增加回合快照表和会话视图窗口（最近 20 个回合），历史分页从回合快照读取；blob 改为按内容寻址的文件；原始帧放在单独的 `raw.db`；工具调用 ID 细化为"`toolCallId` 是 upsert 键，协议里用宿主生成的 ID"；`threadStatus` 的使用范围（Q11）；索引流不含已归档会话，另加 `session/list_archived`；交互请求表记录所属的适配器实例（SEC-10） | 5.2.8、5.4 |
| ADR-07 | 晚到的工具输出"丢弃并标记"（Q14）；增加线路分接层处理原始帧、封闭枚举和扩展消息；上一个 prompt 一直不返回时，先 `session/close` 再 `session/resume` 才发新 prompt；规避清单以 5.6.9 为准 | 5.6 |
| ADR-08 | codex-acp 随包（Q4）；清单在编译期内嵌进宿主二进制；已验证的 CLI 范围写进清单；Node 公证的后备方案（Q3） | 5.10.3 |
| ADR-09 | 多会话并发 prompt 的实测与后备（Q8）；Codex 回收阈值（常驻内存 2 GiB 或空闲 30 分钟）；适配器崩溃后的重启退避；能力探测后回收 Codex 适配器 | 5.6.7、5.6.8 |
| ADR-10 | hunk 操作用 `diff_token` 指向宿主缓存的补丁；M1 提供最小的 `staged` 范围（Q13）；按仓库加锁的范围；git 子进程的环境变量约定（`LC_ALL=C`、`GIT_TERMINAL_PROMPT=0`） | 5.2.10 |
| ADR-11 | 门禁工具具体化：`cargo deny`、cargo-about、`tools/license-gate`；CI 计算 claude-agent-acp 打包哈希时用隔离目录，不进入任何打包输入；对最终的 `.app` 再查一遍 | 5.1.6、5.10.7 |

## 5.13.3 对 PRD 的影响

不改变需求，只需在验收时明确口径：

- MS-4 对 Claude 可能只能显示计划模式的 Markdown 计划（Q12）。
- GT-3 的取消暂存依赖一个最小的"已暂存"范围（Q13）。
- UI-5 在 M1a 只做一种语言，M1b 补齐中英文。
- NFR-8 的"界面崩溃不影响会话"只对发布构建成立（Q22）。
