# T021 cupper-sys II：对端校验与代码签名

- 派发日期：2026-10-19
- 执行者：子 agent（Rust 系统工程师，macOS 安全）
- 分支：`feat/T021-sys-peer-sign`
- 依赖：T001；T020 已合并，`cupper-sys` crate 已经存在。属于 M1a 第 1 波。

## 背景

本地认证的信任锚在宿主（ADR-05，设计 5.8.1）。宿主接受连接时，用 `LOCAL_PEERTOKEN` 取对端的审计令牌，用 SecCode 校验对端的签名要求。UI 和 CLI 校验宿主时，也用同一套接口。SP-4b 已经实测过这些接口的语义。

设计 v1.3 新增了一条规则：`dev_key` 只接受拉起宿主的那个进程。实现时用 `PROC_PIDUNIQIDENTIFIERINFO` 读出 `p_puniqueid` 和 `p_uniqueid`，这一点需要本任务实测确认。

## 范围

**做**（计划 T021 一行）：
1. 把 SP-4b 的 Security FFI 合并进 `cupper-sys`（`spikes/SP-4b-local-security/`）：
   - `LOCAL_PEERTOKEN`、`LOCAL_PEERPID`；
   - SecCode 校验，包括动态状态字；
   - 编译期写入的签名要求：要求串由调用方在编译期传入，本任务只提供校验接口；
   - csops；
   - 四类错误：对端已退出、已 exec、磁盘上的代码已变、其他（5.8.1）。
2. 实测 `proc_pidinfo` 的 `PROC_PIDUNIQIDENTIFIERINFO`：读自己的 `p_puniqueid` 和对端的 `p_uniqueid`。不可行时退回"pid 加启动时间"（T020 已经提供启动时间），并在汇报里说明。
3. `cargo xtask ci security-macos14`：在作者机器的系统上运行这些校验测试，摘要里记下系统版本。

**不做**：在网关里使用这些接口（T030、T073）；原生扩展（T072、T073）。

## 验收标准

- [ ] 新增 IT-sys-peer-current，覆盖：
  - "最后一次 I/O 的进程"语义；
  - 对端 exec 之后被拒；
  - fork 出的子进程共用 fd 时被拒；
  - 用不带控制缓冲区的 `read()` 时，`SCM_RIGHTS` 传来的 fd 被丢弃；
  - csops；
  - 四类错误码。

  以上都在作者机器的系统上验证。
- [ ] 在 `tests/ids.toml` 或计划约定的位置，把 IT-sys-peer-macos14、IT-sys-peer-macos15 登记为缺口，状态为"本机替代不了，里程碑退出时由用户确认"。如果 T002 的登记表还没合并，就在汇报里列出，由编排者转交。
- [ ] 新增 UNIT-sys-uniqueid：能读出自己的 `p_puniqueid`，对端的 `p_uniqueid` 与之吻合。不可行时，改为验证退回方案。
- [ ] `xtask ci security-macos14` 能运行，摘要里记有系统版本。
- [ ] 在干净的 worktree 里，`cargo xtask ci pr --record docs/tasks/T021-sys-peer-sign/ci/` 通过，摘要随任务提交。
- [ ] 测试名以用例编号的 snake_case 开头。`unsafe` 只出现在 `cupper-sys` 里，每处都有 `SAFETY:` 注释。

## 交接事项（来自 T020）

- `PROC_PIDTBSDINFO` 读不了其他用户的进程。`PROC_PIDT_SHORTBSDINFO` 和 `PROC_PIDUNIQIDENTIFIERINFO`（flavor 17，56 字节）对 root 进程也能读，实测 pid 1 可以。
- libc 0.2.191 没有定义 `proc_bsdinfowithuniqid`，需要自己声明。
- `cupper_sys::spawn` 不搜索 `PATH`，调用方要传入绝对路径。`Child` 必须 `wait`，否则会留下僵尸进程。

## 注意

- 签名相关的测试需要被签过名的二进制。可以在测试里用 `codesign -s -` 做 ad-hoc 签名，或者用 SP-4b 的办法。**不要**在用户的钥匙串里创建证书或身份，也不要修改系统设置；需要真实证书的正向验证属于发布流水线和 Q35（T105）。
- 测试结束后清理拉起的进程和临时文件。
- 规模上限约 1,500 行。明显会超出时，先停下来汇报。

## 参考资料

- 设计：`08-security.md` 5.8.1（含 v1.3 的构建种类表、`dev_key` 只接受拉起宿主的进程）；`01-repo-build-ci.md` 5.1.6（security-macos14）；`13-open-issues.md` Q24。
- ADR-05（含 SP-4b 修订、计划评审补充）；SP-4b 的报告与代码；`docs/tasks/T020-sys-process/report.md`。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为 `report.md`。不要自己创建 `report.md`。
