# T039 Review

## 第 1 轮（2026-10-19，编排者）

- 结论：通过，合并。
- 过程：
  - 编排者把 main 合入分支（`e8a1f92`）。`cupper-cli/src/lib.rs` 与 T049 的 `mod mirror` 相邻冲突，两行都保留。
  - 合并后在任务 worktree 里跑 `cargo xtask ci pr --record`：rust 971、protocol 176、ts 232、license 1，全部通过，摘要为 `ci/pr-e8a1f927d058.json`。
  - 守则第 8 条：第一个模块写完后，推算约 1,545 行，略超上限。执行者认为一个模块的样本太小，没有停下，接着写。第二个模块之后推算约 1,280 行，最终 1,289 行。执行者在汇报和 `notes.md` 里如实说明了这一点。这次没有造成超限，但按守则应当先停下。已记录。
- 核对：
  - `unsafe` 只在 `cupper-sys` 的 `ExitWatch` 里，带 `SAFETY:` 注释；
  - 没有个人信息；
  - 新增依赖 `hmac =0.12.1`（MIT OR Apache-2.0），`cargo-deny` 通过；
  - `cupper-host` 对 `cupper-cli` 的依赖只在 dev-dependencies 里，用来验证两端的证明互通，不进产物。
- 安全审阅：
  - 证明是 `HMAC-SHA256(key, 方向 ‖ h_nonce ‖ c_nonce ‖ pid)`，比较用 `verify_slice`，常量时间。另有一组用 Python `hmac` 独立算出的已知答案。
  - `dev_key` 先查拉起方，再查证明。其他进程因此拿不到"证明对不对"的信号；进程级用例也能证明拒绝来自"只接受拉起方"这一条。
  - 证明缺失时回 `bad_proof`，不降级成只读的 cli。
  - `pid_c` 取固定令牌的 `pid()`；`host_proof` 由 CLI 按固定宿主令牌的 pid 校验。
  - 握手期限 10 s，经 `Clock` 计时，到期断开并记审计。
  - `dev-auth` 构建在启动时 ppid 已是 1 就以退出码 1 退出，用 kqueue 在进程级断言了退出码。
  - 变异检查：去掉拉起方检查、去掉 ppid 检查，都被用例测出。
- 编排者裁定：
  - **`is_weak(&ClientAuth)`**：e2e 构建里出示了 harness 密钥就不算弱，以 `peer` 接入仍算弱。接受。E2E-NFR1-2 的 `cli` 提权路径只能走 `peer`，一律要求 harness 会把它堵死。
  - **`--harness-key-fd` 的参数解析**：归 T046，本任务提供 `HarnessKey::read_fd`。
  - **夹具审计行**：用的是 `Debug` 文本，只供用例按子串断言，不是稳定格式；正式格式归 T037。
- 规模：1,289 行（实现约 651 行，测试约 638 行）。
- 转交事项已写进 `handoff-notes.md`：T073、T046、T037、T047。
