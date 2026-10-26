# T063 评审

- 评审人：编排者
- 日期：2026-10-26
- 结论：**通过，合并**

## 核对

- **规模**：`cargo xtask size` 报新增 478、删除 34，开工前估约 560，停止线 850；每次提交后都跑了工具。
- **CI**：编排者把 main（含 T178 的 `rustls-webpki` 升级）合进分支后，执行者跑 `ci pr --record` 通过：`pr-bb477e31c81b.json`（rust 2093、protocol 185、ts 473、license、testdata-scan、e2e 30），十个编号均为 pass。之后 main 只进了 T093 修复 1（xtask 的测试），与本任务不重叠，直接合并。
- **验收**：十个编号在 `tests/driver_k.rs` 里全部通过，最后一轮连跑 10 次全绿，跑完无残留进程。

## 就地修的两处缺陷（计划 1.2 的上限内）

1. **`AdapterLog` 打不开时适配器收到 SIGPIPE**（T116 的交接，+67 −14）：只有拿到日志时才起读取任务，日志打不开就直接关管道，适配器再写 stderr 收到 SIGPIPE、原始帧也丢了。改为改用不带文件的 `AdapterLog::unwritten`，照样读出每一行写进原始帧、保留脱敏后的末尾，带用例。
2. **K14 的改判缺失**（+47 −20）：`CliTooOld` 有定义却没有任何地方产生它，旧 CLI 报"需要认证"时宿主一律当未登录，还改了登录状态。现在 prompt 以未登录失败、CLI 有版本号且低于已验证范围时改判为 `cli_too_old`，登录状态不变；新增 `test_config.agents.<agent>.cli_version` 让注入的假 agent 走同一套分类。

## 逐项意见

1. **假 agent 的 `--calls`**：让 K1、K6 直接断言收到的是 `session/resume` 还是 `session/new`、prompt 重发了几次，补上了 T091 交接的可观察性。`withdraw_after` 让撤回后剧本继续，消掉了 IT-K5 的 2/10 不稳定。设计 5.9.2 的剧本表已补这两项（`712c896`）。
2. **K5 的原因**：用户中断时宿主先以 `turn_cancelled` 结束请求，Claude 随后的撤回不起作用；`withdrawn` 只用于 agent 自己撤回。执行者的理由成立（中断由用户发起，不依赖 agent 一定撤回；与 Codex 的 K4 同一条路；界面不读原因，用户看到的都是"未回答"）。编排者采纳"改设计、不改宿主"，设计 5.6.9 K5 一行已改（`712c896`）。
3. **K6 "不重发"的口径**写得清楚：只在"prompt 在任何更新之前以 Session not found 失败"时恢复并重发；"先有输出、再遇到 Session not found"由进程内用例覆盖。
4. **Q43**：五轮"崩溃、恢复"后宿主进程树大小不变，证明宿主侧恢复不会多拉起进程；真实 `claude` 多次 resume 后会不会留下 CLI 进程，仍由 T066 观察。

## 记下的点

- **K14 只覆盖 prompt 失败**：会话打开时的认证错误还没有改判（claude 形态的假 agent 打开时不报未登录）；codex 形态归 T112。
- **Q43 的真实观察**交给 T066。
