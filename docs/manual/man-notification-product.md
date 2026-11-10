# MAN-notification-product：自用构建上的系统通知

- 场景：PRD AP-3。窗口不在前台时，有待处理的请求、回合完成或失败，都收到系统通知；点通知跳到那个会话。
- 对应设计：5.2.9（通知身份的六条规则、通知的点击、申请时机）、5.2.13（有序退出）、5.7.8（通知没有操作按钮、只导航）、5.10.1（宿主的名称和图标）。自动化部分是 IT-notifier-record（T250）、IT-notifier-click（T251）、E2E-notification-navigate（T254），都用 record 模式，不碰系统的通知服务；本表复核它们替代不了的真实部分。
- 共同约定见 [README.md](README.md)。记录放在执行任务的卷宗里（T254），`versions` 写 `cupper`、`macos`。

## 准备

- 用要复核的提交构建自用版，按 [selfuse-install.md](../user/selfuse-install.md) 的"手动更新"替换 `/Applications/Cupper Local.app`（先退出，再删，再复制）。
- **名称和图标只在宿主第一次连上通知服务时记下**（T101），之后改了也不变。这台 Mac 上要是装过带通知的旧版 Cupper Local，系统设置里留下的是那时的名称和图标；第 2 步按当时的样子记录即可。
- 准备一个能让 agent 很快结束一个回合的项目，以及一条在默认权限档下需要批准的命令（同 MAN-S2 的准备）。
- 宿主的日志在 `~/Library/Logs/cupper-local/host.log`，数据库在 `~/Library/Application Support/cupper-local/host.db`；记录时只摘与通知有关的行，不摘路径和项目名。

## 步骤

| # | 操作 | 预期与通过条件 |
|---|---|---|
| 1 | 打开 Cupper Local | 第一次出现授权请求，来自 "Cupper Local"，图标是拿铁；点"允许"。之后再打开不再问。授权状态进 `host.updated`（`notification_auth: authorized`）。点了"不允许"时状态是 `denied`，之后不再申请 |
| 2 | 打开 系统设置 → 通知 | 有一条 "Cupper Local"，图标是拿铁，"允许通知"开着。只有这一条属于 Cupper Local |
| 3 | 让 agent 做一件需要批准的事，在请求到达前切到别的 App | 收到通知：标题是会话标题，正文是"需要批准"那一句（跟随系统语言），没有任何操作按钮，没有提示音；通知上的名称是 "Cupper Local"、图标是拿铁 |
| 4 | **Cupper Local 开着、不在前台**时点第 3 步的通知 | Cupper Local 来到前台，切到这个会话；请求面板在，但焦点不在面板上（显示"按 Tab 或点击以处理请求"）；没有 Finder 的 "…is not open anymore" 对话框 |
| 5 | 再触发一次（或让一个回合结束），这次 **Cupper Local 在前台、开着另一个会话**时从通知中心点它 | 切到通知所属的会话；没有对话框 |
| 6 | 窗口不在前台时，让一个回合正常结束；再让一个回合失败（回合进行中在终端结束 agent 的进程：`pkill -9 -f claude-agent-acp`） | 各收到一条，正文分别是回合完成、回合失败的那一句。中断（两次 Esc）的回合不发通知 |
| 7 | 照第 3 步让一个请求在**窗口不在看那个会话时**到达，收到通知后先不点；在终端只结束界面：`pkill -9 -x "Cupper Local"`（宿主因为有待处理的请求留着），再点那条通知 | Cupper Local 被启动并直接打开这个会话；没有对话框；宿主只有一个（`pgrep -x cupper-host` 只有一行） |
| 8 | 照第 7 步再留一条通知在通知中心，这次界面和宿主都结束：`pkill -9 -x "Cupper Local"; pkill -9 -x cupper-host`，再点那条通知 | 系统拉起一个宿主，它把会话交给 Cupper Local 后自己退出：Cupper Local 被启动并打开这个会话；之后宿主由界面正常拉起，只有一个 |
| 9 | 第 4、5 步时注意 Cupper Local 来到前台的过程 | 每次都停在前台，没有先闪到前台又被别的窗口盖住；不稳定时记下几次里有几次（`notifier_settle` 300 ms 够不够） |
| 10 | Cupper Local 开着、没有运行中的回合时，注销 macOS 用户再登录 | `host.log` 的最后几行是有序退出（`orderly exit`，原因 `terminated`），`host.db` 的 `meta.last_clean_exit_at` 是注销的时间；再打开时不提示"上次异常退出" |
| 11 | 正常退出 Cupper Local（⌘Q） | 通知中心里 Cupper Local 的通知都清掉了 |

## 收尾

- 第 7、8 步结束过进程，确认只剩一个宿主在运行（`pgrep -x cupper-host` 只有一行），待处理的请求答掉或拒绝。
- 强制结束（`kill -9`、活动监视器的"强制退出"）不走有序退出，留下的通知要靠第 8 步那条路兜底；普通的"退出"走有序退出，会清掉通知（第 11 步）。
