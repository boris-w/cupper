# T089 评审

- 评审人：编排者
- 日期：2026-10-20
- 结论：**通过，合并**

## 核对

- **规模**：自分支点起新增 958 行（实现 722、测试 236），超过收窄后的上限约 900，未到硬停止线 980。执行者中途发现超出后压缩过，分模块数字在 `notes.md`；超出主要是读行与类型转换（约 120）和为模拟重启重构夹具（约 40）。开工前估约 1,320、停下给出三分方案，裁定后按收窄范围完成。编排者接受。
- **CI**：执行者的摘要通过。合并 main（含 T163、T085、T165、T168、T167）没有文本冲突，`ci pr --record` 通过：`pr-461f4de602e6.json`（rust 1909、protocol 185、ts 471、license、testdata-scan、e2e 4；ui 按路径过滤跳过）。
- **变异检查**：去掉 `ignore_for_good` 的读回、不恢复 `command_id`、邮箱不先入表，三项都会被用例抓到。

## 逐项意见

1. **按需载入**：调用到达时先把邮箱放进表、再起载入任务，同时到达的调用共用这个邮箱，一个会话只有一个 actor；载入失败时邮箱撤出、期间的调用都回这个错误，下一次调用重新载入。
2. **各核心的恢复入口**：`TurnMachine::restore`、`QueueCore::restored`（含 `command_id`、`reply_to`、发送者）、`TierGuard::restore`、交互代理经 `Input::Ended`；首个输入是 `Input::Restored`。核心仍然不做 I/O。
3. **设定者**（裁定第 1 条）：按 `mode_set_by` 用握手时的映射找回；桌面只有在 `full_access_confirmed_at` 存在时按 control 角色算；CLI 只有 read；没有测试 harness 的构建里 `pr_test` 与其他未知 ID 一样按最低上限 `normal` 算。载入时立即按找回的设定者检查一次，结论更严时当场进入 `config_unconfirmed`。这是保守的一侧，与裁定一致；代价是 CLI 提权期间设定的档位，重启后要重新确认，可以接受。
4. **`ignore_for_good` 持久化**：存进 `sessions.project_config_ignored`，与做出选择的那一步同一个事务提交，有用例证明重启后仍不加载项目配置（安全相关）。
5. **表结构改在 `0001` 而不是 `0002`**：执行者做得对。计划 P-4 规定 T160 之前表结构只改 `0001`，`store_schema_*` 用例也要求只有 `0001` 并与设计 5.4.2 一致；裁定里写的 `0002` 是编排者的笔误。设计 5.4.2 加了一行，确认。
6. **补写以视图为准**：先补写待处理请求的 `interaction.resolved{expired, host_restarted}`，再补写运行中回合的 `turn.completed{interrupted}`，随行和索引通知一起提交。视图缺内容（日志被修剪）的情况归 T169。
7. **队列一律暂停**：载入时队列里有消息又没暂停的，以 `turn_interrupted` 暂停，重启后在用户操作之前不会自己开始回合。选在了安全的一侧；设计 5.2.13 由编排者补写这条规则。
8. **两项零碎**：`StreamWriter::append` 空 `bodies` 时单独开事务执行 `with`、补了文档，actor 每一步统一经流写者提交；`session/get` 与索引共用 `index::status_of`，规则只剩一份。
9. **`canonicalize` 挪进 `spawn_blocking`**：与 T138、T163 的规则一致。

## 记下的点

- **被中断回合里没结束的条目**（例如进行中的工具调用）不补写 `item.completed`，视图里仍是进行中。交给 T169 在宿主侧补写，桌面与 CLI 看到的一致（计划 v1.3.41）。
- **`SessionPort::facts`** 仍要求会话在内存里，会话未载入时 `policy/describe` 不带所选选项的信息。重启后待处理的请求都已过期，影响很小，记交接。
- **执行者用 `git reset --soft` 合并过本分支最后一个提交**，只动了自己未推送的提交，没有 rebase、merge 或 push，可以接受；以后改提交请另加一个提交。
- **T169、T175 可直接复用** `session/load.rs` 的读行与 `Fixture::restart`；空闲退出可用管理器的 `forget`（只在邮箱匹配时撤出）。
