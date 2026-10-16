# 5.5 关键流程

- 所属：`docs/02-design.md` 第 5 节
- 图中"宿主"内部的模块名见 [5.2](02-host.md)，方法和事件见 [5.3](03-protocol-v1.md)。为了可读，图里省略了每条命令的 `command_id`、账本检查和策略检查，它们对每条命令都适用
- 渲染进程发给宿主的消息都经桌面主进程转发（出方向主进程做白名单检查，入方向不解析），图中只在有安全含义的地方画出主进程

## 5.5.1 App 启动：拉起宿主、握手、首屏快照

```mermaid
sequenceDiagram
  autonumber
  actor U as 用户
  participant M as 桌面主进程
  participant H as 宿主
  participant R as 渲染进程
  U->>M: 打开 App
  M->>M: 发布构建按白名单检查命令行开关，出现不认识的开关就退出
  M->>H: 连接 run/host.sock
  alt 连接失败（宿主没有运行）
    M->>H: 经原生扩展 cupper-sys::spawn 加 SETSID 拉起 CupperHost.app，stdin 写入启动载荷后关闭
    H->>H: 读启动载荷，flock、迁移（先备份）、判定 data_epoch、崩溃恢复、加载索引流、绑定套接字
    loop 每 10 ms 重试，总预算 500 ms
      M->>H: 连接
    end
  end
  H->>H: accept：核对 uid，取审计令牌校验签名，记下固定令牌
  H-->>M: host/hello（h_nonce）
  M->>M: 原生扩展校验宿主签名，记下宿主的令牌
  M->>H: initialize（role control，auth codesign；退化模式为 dev_key 证明）
  H-->>M: principal desktop_ui、features、limits、data_epoch（退化模式另带 host_proof，主进程校验）
  par 并行
    M->>R: 创建窗口（show false；sandbox、contextIsolation、app 协议加 CSP、codeCache）
  and
    M->>H: 为窗口新建数据连接，代发 initialize（role window），先缓存入方向的数据
  end
  R->>M: ShellAPI.init()、host.open()
  M-->>R: ShellInfo；MessagePort 和握手结果
  R->>H: stream/subscribe index（经主进程转发）
  H-->>R: mode snapshot，随后 stream/snapshot 分页
  R->>R: 渲染侧边栏：项目、会话、状态
  R->>H: stream/subscribe 上次打开的会话
  H-->>R: 会话快照（最近 20 个回合加进行中的回合）
  R->>M: ready-to-show，窗口显示
  R->>R: performance.mark('cupper:ready')：可以输入，冷启动结束
  Note over H: agent 适配器不在这条路径上，按需启动（NFR-6）
```

**时间预算**（NFR-6：冷启动到可用不超过 2 秒）

| 阶段 | 预算 | 依据 |
|---|---|---|
| Electron 启动到主进程 `ready` | 700 ms | 主进程在 `ready` 之前就开始连接或拉起宿主，两者并行 |
| 宿主拉起到可连接 | 100 ms | SP-4a 实测 p95 4.2 ms；留出首次运行新二进制时系统安全检查的余量 |
| 双向签名校验与握手 | 50 ms | SP-4b：每次校验中位数约 0.9 ms |
| 索引快照 | 50 ms | SP-4a：20 个会话时 p95 4.6 ms |
| 渲染进程加载与首屏 | 800 ms | SP-6 实测确认 |
| 当前会话快照与渲染 | 300 ms | 窗口只含最近 20 个回合 |

## 5.5.2 新建会话（含新建 worktree）

```mermaid
sequenceDiagram
  autonumber
  participant R as 渲染进程
  participant H as 宿主（会话编排）
  participant G as git 服务
  participant A as 驱动层
  participant AD as 适配器和 CLI
  R->>H: agents/capabilities（新建会话页打开时）
  H-->>R: 缓存的模型、推理强度、权限模式、斜杠命令
  Note over R: 用户选 agent、项目、新建 worktree、起始分支，输入第一条消息
  R->>H: session/create（agent、project、worktree 和 base_ref、config、message）
  H->>H: 一个事务：sessions 行（含第一条消息 initial_prompt_json）、session.configured、workspace.preparing、命令结果
  H-->>R: session_id（已受理）
  R->>H: stream/subscribe 该会话
  alt 新建 worktree
    H->>G: worktree add -b cupper/短名，位置在仓库之外
    G-->>H: 检出进度
    H-->>R: workspace.preparing（进度）
    G->>G: worktree lock，等 1 秒后 update-index --refresh
    H-->>R: workspace.ready（路径、分支）
  else 本地模式
    H-->>R: workspace.ready（项目目录）
  end
  H->>H: 重新扫描项目信任（5.2.6），决定 project_config
  H->>A: Attach New（cwd 的 realpath、config、project_config）
  opt 该 agent 的适配器还没运行
    A->>A: 等运行时就绪，核对 node 的 cdhash 和 adapter.mjs 的 sha256
    A->>AD: posix_spawn（独立进程组、环境变量白名单），立即写 adapters 表
    A->>AD: initialize（clientInfo 为 cupper）
    AD-->>A: 能力、登录状态（只保留类型）
  end
  A->>AD: session/new
  AD-->>A: sessionId、configOptions
  A->>AD: set_config_option（模式、模型、推理强度）
  AD-->>A: available_commands_update
  A-->>H: AttachInfo
  H-->>R: session.capabilities
  H->>H: 进入 Preparing：一个事务写 turn.started、用户消息，并清空 initial_prompt_json
  H->>G: 起始快照（临时 index，私有对象库）
  G-->>H: tree id（超过 3 s 就不等，diff 标为不可用）
  H->>A: Prompt
  A->>AD: session/prompt
  AD-->>A: session/update 流
  A-->>H: 经提取层的 Update
  H-->>R: item 事件（写入时 40 ms 合并）
  AD-->>A: prompt 返回 end_turn
  H-->>R: turn.completed
  H->>G: 结束快照（异步）
```

失败分支：CLI 找不到、没登录、运行时安装失败时，`Attach` 返回 `unavailable` 和具体 `reason`；会话流写一条 `error` 条目（带恢复指引），会话状态为 `error`，用户处理后重发即可。worktree 创建失败时写 `workspace.failed`。

## 5.5.3 发送消息、排队、插话（Steer）

```mermaid
sequenceDiagram
  autonumber
  participant R as 客户端
  participant S as 会话 actor
  participant A as 驱动层
  participant AD as 适配器
  Note over S: 回合 t1 正在运行（Prompting）
  R->>S: session/send（mode queue，消息 B）
  S->>S: 追加到队列，写 queue.updated
  S-->>R: disposition queued
  R->>S: session/send（mode steer，expected_turn_id t1，消息 C）
  alt 能力里有 steer，且 t1 仍处于 Prompting
    S->>A: Steer（C）
    A->>AD: _session/steering（idleBehavior promptRequired）
    AD-->>A: outcome injected
    S->>S: 在 t1 里写一条 steered 的用户消息
    S-->>R: disposition steered
  else 不支持、promptRequired、超时，或 t1 已经结束
    S->>S: 自动转为排队
    S-->>R: disposition queued 和 fallback_reason
  end
  AD-->>A: t1 的 prompt 返回 end_turn
  A-->>S: PromptDone
  S->>S: turn.completed t1，异步拍结束快照
  S->>S: 队列未暂停，取出 B
  S->>S: turn.started t2（origin queued）
  S->>A: Prompt（B）
  Note over S,AD: 同一个 agent 会话同时只有一个 prompt（K1）
```

## 5.5.4 审批：先到先得

```mermaid
sequenceDiagram
  autonumber
  participant AD as 适配器
  participant A as 驱动层
  participant S as 会话 actor（交互代理）
  participant M as 桌面主进程
  participant R as 渲染进程
  participant C as cupper-cli（已提权 approve）
  AD->>A: session/request_permission
  A->>S: Permission（选项已标 effect 和 requires）
  S->>S: 刷出缓冲的文本，同一事务写 interaction.requested 和 interactions 行
  S-->>R: interaction.requested（主进程不解析，直接转发）
  S-->>C: interaction.requested
  Note over R: 卡片出现，焦点在卡片上时 Enter 批准、Esc 拒绝
  par 两端几乎同时回答
    C->>S: interaction/respond（allow_once，digest）
  and
    R->>M: interaction/respond（allow_once，digest）
    M->>M: 方法白名单；1 秒内有尚未用掉的真实输入（规则见 5.7.8），用掉它
    M->>S: 重新序列化后经 window 数据连接转发，写入 input 证据
  end
  S->>S: 先到的一条：仍是 pending、摘要一致、在 actionable_by 内；window 角色还要求带 input 证据、距创建不少于 400 ms
  S->>A: 应答 ACP（selected allow_once）
  A->>AD: 响应
  S->>S: interaction.resolved（resolved_by cupper-cli）和命令结果同一事务落盘
  S-->>C: 成功
  S-->>R: interaction.resolved，卡片收起
  S->>S: 后到的一条：已经不是 pending
  S-->>M: conflict already_resolved（resolved_by cupper-cli）
  M-->>R: 显示"已在 cupper-cli 处理"
```

选择 `requires: privileged` 的选项（`allow_always` 类）时路径不同：渲染进程不能直接发送，而是调用 `ShellAPI.requestPrivileged`；主进程经 control 连接用 `interaction/get` 和 `policy/describe` 取得内容，弹出原生确认框，用户确认后由主进程经 control 连接发送 `interaction/respond`（[5.7.8](07-desktop.md#578-安全加固的落点)）。

## 5.5.5 中断

```mermaid
sequenceDiagram
  autonumber
  participant R as 客户端
  participant S as 会话 actor
  participant A as 驱动层
  participant AD as 适配器
  R->>S: session/interrupt（expected_turn_id t1）
  S->>A: Cancel
  A->>AD: session/cancel
  S->>S: 挂起的交互请求标为 cancelled
  Note over S,A: Codex 不会撤回审批，宿主替用户应答 Cancelled（K4）
  S-->>R: interaction.resolved（cancelled）
  S->>S: 进入 Cancelling，截止 10 s
  S-->>R: 已受理（turn_id t1）
  alt prompt 在截止前返回 cancelled
    AD-->>A: stopReason cancelled
    A-->>S: PromptDone（cancelled）
    S-->>R: turn.completed（cancelled）
  else 截止时间到
    S-->>R: turn.completed（cancelled，stop_reason host_timeout）
    Note over S: 之后这个 prompt 的响应直接忽略，下次发送前先对该 agent 会话 close 再 resume（K1）
  end
  Note over S: interrupt_stops_commands 为 false（Codex）时，还在运行的命令标 abandoned 加 may_still_run
  opt Codex 取消后命令仍在运行（K3）
    AD-->>A: t1 工具的输出和 completed 晚到（通常在下一个回合 t2 里）
    A-->>S: Update（toolCallId 属于已取消的 t1）
    S-->>R: 在当前回合 t2 新建 late_tool_output 条目（ref 指向 t1 的原条目），之后的输出都追加到它上面
    Note over S: t1 的回合快照不改；没有进行中的回合时开一个 autonomous 回合承载它（5.2.6）
  end
  S->>S: 队列暂停，等用户发送或 session/queue_resume
```

## 5.5.6 崩溃恢复

**宿主崩溃**（NFR-9、NFR-1 第 5 条验收）

```mermaid
sequenceDiagram
  autonumber
  participant M as 桌面主进程
  participant H as 旧宿主
  participant H2 as 新宿主
  participant AD as 适配器进程组
  Note over H: 回合 t1 运行中，审批 r1 待处理
  H--xM: 宿主被 kill -9，连接断开
  Note over AD: node 适配器退出，claude CLI 可能残留（SP-4a 实测）
  M->>M: 界面显示"正在重新连接"
  M->>H2: 连接失败，拉起新宿主
  H2->>H2: flock 成功（锁随旧进程释放）
  H2->>AD: 按 adapters 表的进程组和启动时间清理残留
  H2->>H2: t1 写 turn.completed（interrupted），r1 写 expired
  H2->>H2: 加载索引流，绑定套接字
  M->>H2: hello、initialize（control、window 两条连接）
  M->>H2: 按游标（含 data_epoch）重新订阅 index 和当前会话
  alt 进程崩溃（kill -9），data_epoch 不变
    H2-->>M: mode replay，补发断开期间的事件
  else 异常退出后系统重启过（可能断电丢了事务），data_epoch 已加 1
    H2-->>M: mode snapshot（reason epoch_changed），客户端丢弃视图和游标后重建
  end
  Note over M: 审批卡片收起，消息流提示"回合已中断"
  M->>H2: 用户再次发送
  H2->>AD: 拉起适配器，session/resume
  AD-->>H2: 恢复成功，上下文接上
```

**适配器或 CLI 崩溃**（NFR-9、MS-5）

```mermaid
sequenceDiagram
  autonumber
  participant AD as 适配器
  participant A as AgentRuntime
  participant S as 会话 actor（该 agent 的每个会话）
  participant R as 客户端
  AD--xA: 适配器进程退出
  A->>A: waitpid 拿到退出状态，adapters 行写 ended_at，清理进程组
  A-->>S: AdapterExited（instance）
  S->>S: 运行中的回合 failed（adapter_exited），属于该实例的交互请求 expired（SEC-10）
  S-->>R: notice agent_restarted
  opt 有会话正在运行回合
    A->>AD: 立即重启，退避 1、2、4 秒
  end
  Note over S: 下一次发送时 Attach Resume，即 session/resume
  opt 只是 Claude 某个会话的 CLI 崩溃（K6）
    AD-->>A: prompt 返回 -32603，之后是 Session not found
    A-->>S: PromptFailed（process_exited）
    S->>A: 下一次发送时先 Attach Resume，再 Prompt
  end
```

## 5.5.7 升级

```mermaid
sequenceDiagram
  autonumber
  actor U as 用户
  participant M as 主进程（旧版本）
  participant H as 宿主（旧版本）
  participant SQ as Squirrel.Mac（ShipIt）
  participant M2 as 主进程（新版本）
  participant H2 as 宿主（新版本）
  M->>M: electron-updater 后台下载（autoInstallOnAppQuit 为 false）
  M-->>U: 新版本已就绪
  U->>M: 重启以更新
  M->>H: host/prepare_upgrade（wait），经 control 连接
  H->>H: 进入待升级，拒绝新会话和新回合
  alt 还有回合在运行或审批待处理
    H-->>M: waiting（running_turns）
    M-->>U: 等待回合结束，或"立即升级"（中断这些回合）
  end
  H->>H: 逐个 session/close，关闭适配器，写检查点
  H-->>M: host/shutdown（upgrade）
  H->>H: 删除套接字，释放锁，退出
  M->>M: 确认连接已关闭、锁已释放；进入安装空窗：不自动拉起宿主，界面显示"正在安装更新"
  M->>SQ: quitAndInstall（before-quit-for-update，跳过 NFR-8 的退出检查）
  SQ->>SQ: 经本地代理取 zip、解包、校验签名
  alt 校验或解包失败（签名不符、磁盘满）
    SQ-->>M: 报错
    M->>M: 重新拉起旧宿主，恢复界面并提示
  else 成功
    SQ->>SQ: 新包移入、旧包移走（不原地覆盖可执行文件）
    SQ->>M2: 启动新版本
  end
  M2->>H2: 连接失败，拉起新宿主
  H2->>H2: 备份 host.db，执行迁移
  H2->>H2: 按内嵌清单安装新版本运行时（Claude 适配器需要联网）
  M2->>H2: hello、initialize，按游标重新订阅
  Note over H2: 旧版本的运行时目录在没有进程使用后回收
```

安装空窗、本地代理端口和安装位置检查的细节见 [5.10.5](10-release.md#5105-更新流程)。

其他升级路径（ADR-01 的 SP-4b 修订）：

- **用户手动把新版本拖进"应用程序"覆盖安装**：旧宿主在下一次有连接或要拉起子进程时，发现自己运行中的 cdhash 与磁盘上的不一致，进入待升级、空闲时退出。新版本的主进程校验旧宿主签名时会得到 `-67034` 或 `ENOENT`，按"旧宿主等待退出"处理：不向它发送命令，界面显示等待状态并提供"重启宿主"。
- **用户退出 App 时已有下载好的更新**：主进程按同样的顺序（待升级 → 宿主退出 → 安装）执行，只是不重新启动。M2 的后台模式下，关闭窗口不安装更新（ADR-01）。

## 5.5.8 审查、暂存、提交

```mermaid
sequenceDiagram
  autonumber
  participant R as 渲染进程
  participant H as 宿主（git 服务）
  participant GIT as git 命令行
  participant S as 会话 actor
  R->>H: git/diff（scope last_turn）
  H->>H: 从 turns 表取最近回合的起止快照（结束快照未完成时先等它）
  H->>GIT: 快照档：diff-tree -r -z --raw -M 加 --numstat（cupper 自己的 GIT_DIR）
  H-->>R: diff_token dt1、文件列表、caveats（mixed_changes 等）
  R->>H: git/file_diff（dt1，某个文件）
  H->>GIT: 快照档：固定参数生成补丁并切成 hunk
  H-->>R: hunks（hunk_id）
  Note over R: 用户在某一行写评论，点"发给 agent"
  R->>S: session/send（content 含 review_comment）
  S->>S: 把评论排成给 agent 的文本，开新回合或排队
  R->>H: git/diff（scope unstaged）得到 dt2，再 git/file_diff
  R->>H: git/stage（dt2，hunk_ids）
  H->>H: 账本写 pending_external（强制同步后才执行 git），拿 RepoLock
  H->>GIT: 仓库档：读配置、覆盖可执行类键后 apply --cached --whitespace=nowarn（用 --no-renames 生成的补丁）
  alt 应用成功
    H-->>R: 成功，客户端刷新 diff
  else 状态已变
    H-->>R: conflict stale_diff，客户端刷新后重试
  end
  R->>H: git/commit（message）
  H->>GIT: 用户档：commit -F -（会运行用户的 hooks，确认文案已提示）
  GIT-->>H: 新提交的 oid
  H-->>R: oid
  R->>H: git/push（set_upstream）
  H->>GIT: push --set-upstream origin cupper/短名
  H-->>R: remote 和 branch
```

## 5.5.9 归档与 worktree 清理（GT-5、PS-3）

```mermaid
sequenceDiagram
  autonumber
  participant R as 渲染进程
  participant H as 宿主
  participant GIT as git 命令行
  R->>H: worktree/check
  H->>GIT: 7 条规则的只读检查（ADR-10）
  H-->>R: removable、blockers、warnings（被忽略文件数、仅大小写冲突）
  Note over R: 能删就询问"同时删除 worktree？"，不能删就列出原因
  R->>H: session/archive（worktree remove 或 keep）
  H->>H: 会话 actor 对 agent 会话 session/close
  opt 删除 worktree
    H->>GIT: worktree unlock，再 worktree remove（不加 --force）
  end
  H-->>R: 结果（removed、kept 或 blocked）
  H-->>R: 索引流 session.removed（archived）
  Note over R,H: 恢复时：分支还在就 worktree add 重建（必要时先 prune），否则会话只读
```

## 5.5.10 添加项目与信任

安全 #2；PRD v1.2 的 PS-2（项目信任已定，U-R1）。规则见 [5.2.6](02-host.md#项目信任)。

```mermaid
sequenceDiagram
  autonumber
  actor U as 用户
  participant R as 渲染进程
  participant M as 桌面主进程
  participant H as 宿主
  U->>R: 点"添加项目"（真实输入）
  R->>M: ShellAPI.requestPrivileged（add_project）
  M->>U: 系统的选择目录对话框
  M->>H: project/add（control 连接）
  H->>H: realpath；扫描工作区到 git 根目录各级的 .claude/、.mcp.json、.codex/（跟随符号链接），算信任摘要
  alt 扫描范围里没有任何文件
    H->>H: 直接记为 trusted（auto_empty）
    H-->>M: project_id、trust（trusted）
  else 有项目配置
    H-->>M: project_id、trust（untrusted、findings、trust_digest）
    M->>U: 原生确认框：逐项列出 hooks、env、apiKeyHelper、MCP 服务器、skills 等，醒目标出来自工作区外的链接目标，默认按钮"不信任"
    alt 用户选择信任
      M->>H: project/trust（trust_digest）
      H->>H: 重新扫描，摘要一致才写 trusted；不一致返回 digest_mismatch，主进程重新取内容再确认
    else 不信任
      Note over H: 保持 untrusted：Claude 只加载用户级配置，Codex 不能用
    end
  end
  H-->>R: 索引流 project.upserted（trust_state）
  Note over H: 之后每次 Attach、每次发送 prompt 之前、每个回合结束时都重新扫描；摘要变了就转为 stale，暂停队列，经原生确认框展示差异
```
