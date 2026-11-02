# T213 评审

- 评审人：编排者
- 日期：2026-11-02
- 结论：**通过，合并**（PR #29）

## 核对

- **规模**：+89 −26，停止线 120 以内。开工后估计从约 30 改到约 65，原因是把判定收进 CLI 库、补了确定性用例。这个修订事后才写进 notes，下次要在估计变化时就记下。
- **CI**：
  - PR #29 https://github.com/boris-w/cupper/pull/29 ，4e753d2 上 8 个检查全绿（run 41379612062）。
  - 修前 https://github.com/boris-w/cupper/actions/runs/41378840292 （main d025ad4，gold_b_ 200 × 3）：600 次里失败 15 次，都在 `gold_b.rs:394`，报 `Closed(Some(Refused(PeerExited)))`。
  - 修后 https://github.com/boris-w/cupper/actions/runs/41379609751 （4e753d2，同样参数）：600 次全部通过。
- **个人信息**：改动的 6 个文件过了敏感词清单和通用扫描，都是干净的。

## 逐项意见

1. **5.8.1 没有放宽**：客户端的读取代码没改，改的是调用方不再要求读到应答。符合 T196 的裁定。
2. **判定只有一处**：`CloseReason::host_gone` 放在 CLI 库里，CLI、IT-lifecycle、GOLD-B 共用，删掉了 IT-lifecycle 里的副本。
3. **新单元测试是确定性的**：假宿主同步写出应答、关端，再等令牌查询失败，中间没有 await，读任务一定在关闭之后才读。执行者把客户端改回 T196 第一版验证过它会失败，所以它同时守着 5.8.1。
4. **`Closed(None)` 的回答成立**：只在读任务 panic 时出现，`stop()` 不接受它，碰到就是客户端的缺陷，应当失败。
5. **逐处核对了 `host/exit` 的调用方**，表格完整；桌面退出确认补了"应答始终不来"的用例。

## 记下的点

- **设计 5.2.13**：执行者建议的那一句已补进"`host/exit` 的回复与 `host/shutdown` 的先后"一条。
- **桌面确认退出后可能再拉起宿主**：读代码确认可能发生。`ControlConnection` 在 socket 关闭时立即 `connect()`，`#closed` 要到 `will-quit` 才置位，而 `host/exit` 在 `before-quit` 里发出。重启宿主的流程里也可能有同样的重复连接。另开 T214 核实并修复，T099 依赖它。
