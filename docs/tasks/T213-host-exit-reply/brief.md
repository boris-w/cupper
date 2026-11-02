# T213 `host/exit` 之后读不到应答

- 派发日期：2026-11-02
- 执行者：子 agent（Rust 工程师）
- 分支：`fix/T213-host-exit-reply`
- 依赖：无（T205 发现）。T099 依赖本任务

## 背景

`gold_b_claude_approval` 在 main 的 35c713c 上又失败了一次（https://github.com/boris-w/cupper/actions/runs/41015350073 ）：

- 位置：`gold_b.rs:394:20`；
- 错误：`Closed(Some(Refused(Refusal { kind: PeerExited, call: "getsockopt(LOCAL_PEERTOKEN)", code: 57 })))`。

T205 已经保证 `host/exit` 的应答在连接关闭之前进入发送队列（设计 5.2.13）。这次是另一回事：宿主送出应答后很快退出，客户端每次读之后都要比对宿主的令牌（5.8.1），宿主退出后比对得到 `PeerExited`，这一次读到的字节按规定不交付。

**这一条不放宽**（T196 的评审）：在 `PeerExited` 时照样交付读到的数据，会重新打开 5.8.1 要挡的攻击，也就是 fork 出的进程共用已校验的连接，在宿主退出后写入伪造数据。

所以客户端不能依赖读到 `host/exit` 的应答。T196 已经这样改过 `it_lifecycle_host_exit_and_sigterm_exit_in_order`：读到应答也行，读不到也行，关闭原因都必须是 `Eof` 或 `Refused(PeerExited)`。

## 范围

**做**：
1. **GOLD-B 的收尾**（`gold_b.rs` 的 `stop()`）照 T196 的做法判定：应答和以 `Eof` 或 `Refused(PeerExited)` 结束的连接，都算退出已受理；其他错误仍然失败；之后照旧等宿主以 `HostExit::Ok` 退出。
2. **找出别的地方**：测试里，以及产品里调用 `host/exit` 的地方（CLI、桌面主进程重启宿主、testkit），凡是要求读到应答的，都照此处理。
   - 产品里不要把这种结束当成失败报给用户；
   - 用例覆盖"应答没读到、连接以 `PeerExited` 结束"的情形；
   - 能稳定复现的话，用故障点或延迟把宿主退出提前，做修前修后对照。
3. 在 runner 上用 `test-repeat` 连跑证明：`gold_b` 修复前量一次失败率，修复后 600 次以上没有失败。要触发时把参数和提交发给 `main`。
4. 设计 5.2.13 要补的一句，写进汇报，由编排者改。

**不做**：放宽 5.8.1 的逐次比对令牌。

## 验收标准

- [ ] 测试和产品里依赖 `host/exit` 应答的地方都已列出并处理。
- [ ] 修复后 runner 上 `gold_b` 600 次以上全部通过，附运行链接。
- [ ] PR 上 GitHub CI 的 8 个检查都通过，汇报里写上 PR 和运行的链接（守则第 9 条）。

## 规模

- 停止线：120；硬停止线：160

## 参考资料

- 设计：5.2.13（有序退出，"host/exit 的回复与 host/shutdown 的先后"）；5.8.1（每次读取都比对令牌）。
- 卷宗：T196 的评审和汇报（`PeerExited` 的裁定、lifecycle 用例的改法）；T205（`reply_queued`）。

## 注意

- 不运行真实的 Claude Code、Codex CLI；本机只单份跑（守则第 10 条）。
- `gh` 只用只读命令；要推送、触发 workflow 时告诉 `main`。

## 工作方式

编排者会告诉你用哪个 worktree，以及分支是否已经开好。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为本目录的 `report.md`。不要自己创建 `report.md`。汇报里不要写本机的绝对路径。其他物料由执行者在任务分支上提交。
