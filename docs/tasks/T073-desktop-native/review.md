# T073 Review

## 第 1 轮（2026-10-19，编排者）

- 结论：通过，合并。
- 过程：
  - 编排者把 main 合入分支（带来 T071）。`pnpm-lock.yaml` 有冲突：取 main 的版本，再补上本包的 importer，没有从 npm 新拉包。
  - 在任务 worktree 里跑 `cargo xtask ci pr --record`：rust 1073（含扩展的 Node 用例）、protocol 176、ts 291、license 1、testdata-scan 2，全部通过。摘要为 `ci/pr-54a60e320347.json`。
- 核对：
  - 没有手写 `unsafe`；napi-rs 3.14 的宏展开也不触发 `unsafe_code`，所以没有任何豁免；
  - 没有个人信息；
  - 依赖都锁了精确版本，`cargo deny` 通过。
- 安全审阅：
  - **主进程拿到的是 socketpair 的一端，不是宿主的 fd。** 设计 5.7.2 原本写"返回 fd，交给 `net.Socket` 包装"。但 Node 用普通 `read(2)` 读，会把宿主传来的 fd 装进主进程（T021 实测），这与 5.8.5 v1.8 冲突。
    - 现在扩展留着宿主的套接字：读一律经 `recv_closing_fds`，每次读完都和固定令牌比对，两项都通过才把数据转给主进程。
    - 代价是每条连接两个线程，外加一次拷贝，留给 T076 的测量闸门评估。
    - 接受，并已改设计 5.7.2。
  - **`dev_key` 的密钥不出扩展**：`HostKey` 是不透明句柄，证明由扩展计算；`verifyHostProof` 用固定令牌里的 pid 校验。
  - **按构建种类校验宿主**：
    - 发布构建要满足签名要求；签名要求在 T180 之前为空，所以一律拒绝。
    - `dev-auth` 构建要求同用户、固定令牌，再用 `host_proof` 证明身份。
    - 自用构建的 UI 按 5.8.1 的表走 `dev_key` 双向证明，不做 cdhash 校验，判断正确。
  - **拒绝路径**：`host_proof` 不符、令牌变化，在 Node 侧和 Rust 侧都有用例；收到 fd 只有 Rust 侧的用例，因为 Node 没有公开 API 能在普通 Unix 套接字上传 fd，接受。
- 规模：1,335 行（实现约 915 行，测试约 420 行），在上限以内。
- 转交事项已写进 `handoff-notes.md`：T075、T076、T161；`getrandom` 已有三个使用方，要提到 `[workspace.dependencies]`，交给下一个改依赖的任务。
