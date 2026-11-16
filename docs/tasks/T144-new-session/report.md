# T144 汇报

- 执行者：子 agent（前端与 e2e）
- 分支：`feat/T144-new-session`
- 完成日期：2026-11-16

## 做了什么

按拆法 A 做完了新建会话页本身。PR #122 在 4c185f68（已合入 main）上 8 个检查全部通过。两个 e2e 在 runner 上各跑 10 次，全部通过。Q44、Q45 和顶栏交给 T266，入口已经留好。

- **第一条消息前就显示能力**（AG-4，`1ed700c2`）：
  - `composer/config.tsx` 新增 `WithOptions`：在会话里取会话流，在新建会话页取 `agents/capabilities` 的缓存。
  - 模型和推理强度按钮、权限按钮、档位提示（含 `elevated` 的提示）都改用它，新建会话页上也就有了这些按钮。
  - 缓存查询抽成 `useCached`，斜杠命令也共用它，不再等输入 `/` 才查询。
  - 在新建会话页上的选择存进 UI store 的 `chosen[agent]`，会话建好后清空。
- **动作条与发送**（`fde3aba4`）：
  - agent 下拉列出 Claude 和 Codex，并显示各自的可用状态。所选 agent 不可用时禁止发送，原因写在 agent 名旁边。
  - 工作位置可选本地或新建 worktree。非 Git 项目不能选 worktree，并提示"不是 Git 仓库，或是仓库的子目录"（T234）。
  - 新建 worktree 时，起始分支从 `git/branches` 里选：默认当前分支，本地分支和远程分支分组列出。
  - `session/create` 带上 `workspace{worktree, base_ref}` 和用户选的 `config`。
  - 选了完全访问时，改走特权操作 `create_session`，弹原生确认。用户取消的话，草稿留在输入框里。
- **失败提示**（`34a2dacf`）：
  - 会话页加 worktree 失败横幅，分四种：`filter_unsupported`（用 5.2.10 的文案）、`lfs_object_missing`（提示 `git lfs fetch`）、`not_found`（提示 `git worktree prune`）和其他。
  - 队列加暂停原因 `workspace_failed` 的文案。
  - 补上 `stream:error_project_untrusted` 的文案。
- **首次使用时探测**（`0a720df6`）：缓存为空时，按设计 5.6.7 用 `agents/refresh` 探测一次（每个连接一次），然后重读缓存。
- **e2e**（`8073ad9d`）：
  - E2E-new-session-agent-codex（`@smoke`）：选 Codex 后在本地发送，会话的 agent 是 codex。
  - E2E-new-session-before-first-message（nightly）：
    - 第一条消息前，权限、模型和斜杠命令都已显示；
    - 选 Accept edits 后出现低打扰提示；选 worktree 后起始分支默认为 main；
    - 发出后，会话是 worktree（分支在 `cupper/` 下），`permission_mode` 是 `acceptEdits`。
  - E2E-new-session-agent-claude 跟着改成点 radio 项。
- **其他提交**：
  - `b3b65fe7`：动作条里"没有找到 CLI"紧挨着 agent 名，避免看成是"本地"的说明。
  - `31323240`：截图基线。

## 关键取舍

- **按拆法 A 拆**：开工前估约 890 行，超过硬线。按你的决定，本任务只做新建会话页；Q44（宿主设置键、设置页一行、页面预选）、Q45（启动时预选项目）和顶栏（标题、agent 标识、分支名）拆成 T266。
- **留给 T266 的入口**：默认权限接在 `useNewOptions(agent)` 的 `permission_mode` 解析上，取值顺序是"用户的选择 → 默认权限（在缓存的能力里时）→ agent 自己的值"。同一个值进 `chosen`，`session/create` 就会带上它；是完全访问时，同样走原生确认。notes 里专门写了一节"交给 T266 的入口"。
- **为什么要探测空缓存**：查询的 `staleTime` 是 Infinity；宿主在会话接上 agent 后写缓存，但不发 `agents.updated`。所以缓存为空时，新建会话页永远显示不出能力。探测放在一个独立的查询里，每个连接只跑一次，避免 `agents.updated` 触发失效后反复探测。你已确认这符合设计 5.6.7。
- **只发用户明确选过的值**：`config` 里只有用户选过的选项；没选的沿用 CLI 配置，宿主不另外套默认值。完全访问只按用户的选择判断。
- **本地模式不显示分支选择**：起始分支只用于 worktree 的 `base_ref`，不在创建时校验（T124）。

## 验证方式

- **验收标准**
  - **两个 E2E 在 runner 上各重复 10 次**：
    - E2E-new-session-agent-codex：`10 passed`，https://github.com/boris-w/cupper/actions/runs/43825547408
    - E2E-new-session-before-first-message：`10 passed`，https://github.com/boris-w/cupper/actions/runs/43825551322
    - 默认权限和启动预选的测试归 T266。
  - **协议与基线**：协议没有改动。截图基线只由 ui-baseline 生成（运行 43828557111）。
    - 14 张有变化：`new-session-*` 8 张，另有 `shell-light/dark/collapsed/narrow-overlay`、`sidebar-narrow`、`banners` 6 张，后 6 张的主区也是新建会话页。
    - 差异像素只在输入框底栏到动作条这一带，其余逐像素相同。
    - 原因是动作条多了工作位置按钮、显示了不可用原因、agent 菜单变成两个 radio 项，以及底栏显示了缓存里的模型"Opus"。
    - 第一轮基线（43825554967）看出原因文字的间距有问题，修好后重跑了一次。
  - **PR 上 8 个检查全部通过**：PR #122 https://github.com/boris-w/cupper/pull/122 ，运行 https://github.com/boris-w/cupper/actions/runs/43833736546 ，头是 4c185f68。
- **本机检查**：
  - 合并 main 后，ui-core 180 项测试全部通过，tsc、eslint、prettier、trace-ids 也都通过。
  - 三个 e2e 各跑了一次冒烟，只当冒烟，不作为结果。
- **规模**：+590 行，停止线 650、硬线 750，在线内。每写完一个模块就跑一次 `cargo xtask size`，按模块分提交：模块 2 +123、模块 3 +299、模块 6 +48、探测 +28、e2e +92。

## 提交列表

```
4c185f68 Merge branch 'main' into feat/T144-new-session
31323240 test(ui): baselines of the new session page's action bar
b3b65fe7 fix(ui): the reason an agent cannot run sits next to its name
8073ad9d test(e2e): the new session page's Codex and its options before the first message
0a720df6 feat(ui): probe an agent whose capabilities are not cached yet
34a2dacf feat(ui): say why a session's worktree could not be made
fde3aba4 feat(ui): the new session page picks the agent, the workspace and the base
1ed700c2 feat(ui): the composer's options from the cache before the first message
fb3ccec2 docs(task): T144 estimate before the work
```

## 遗留问题 / 范围外发现

- **交给 T266**：
  - Q44：宿主设置键 `sessions.default_permission.<agent>`、设置页"默认权限"一行、页面预选。入口见上文和 notes。
  - Q45：启动时预选上次的项目；那个项目已不存在时，退回列表里的第一个项目。
  - 顶栏：会话标题、agent 标识、worktree 分支名。
- **基线顺序**：T144 先合并；T265 合并 main 后，要重新生成 `new-session-*`、`shell-*`、`banners`、`sidebar-narrow`。
- **能力缓存会过期**：宿主每次会话接上 agent 都会刷新缓存，但不发 `agents.updated`。新建会话页的缓存查询只在 `agents.updated` 或重新连接时失效，所以 agent 升级后的新命令、新模型要等重连才能看到。现在只处理了首次使用时缓存为空的情况。要彻底解决，需要宿主在缓存更新时发事件。这是宿主侧的改动，需要你定是否另立任务。
- **worktree 准备中没有进度显示**：会话页还没有 `workspace.preparing` 的进度（worktree add、checkout 的百分比）。不在本任务范围，可以交给 T148 或另立任务。
