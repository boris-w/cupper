# T045 Review

## 第 1 轮（2026-10-19，编排者）

- 结论：通过，合并。
- 过程：
  - 执行者写完三个模块后，推算总数约 1,720 行，按守则第 8 条在 Mirror 之前停下来汇报，做法正确。
  - 编排者裁定拆分，Mirror 另立 T049（计划 v1.3.6）。
  - 编排者把 main 合入分支（`6706e5e`），带来了 T030 的 `send_with_fds`。
  - 执行者收尾：补了两条拒绝路径的用例，去掉了指向 Mirror 的断链。
  - 在干净的 worktree 里跑 `cargo xtask ci pr --record`：rust 799、protocol 176、ts 232、license 1，全部通过，摘要为 `ci/pr-c22be2f3a31b.json`。分支已包含 main 的最新提交。
- 核对：
  - 没有 `unsafe`，没有个人信息（测试里的 `/Users/example` 是占位路径）；
  - 发布构建的依赖里没有 `cupper-sys` 的 `test-util`，执行者用 `cargo tree` 确认过；
  - `command_id` 的 UUIDv7 豁免带有 reason。
- 安全审阅：
  - 宿主校验在发送任何数据之前做：
    - 先要求对端的 euid 是本用户；
    - 再按构建种类比对签名要求或 cdhash；
    - 通过后固定令牌，之后每次读取都比对。
  - 读套接字经 `recv_closing_fds`；收到 fd、令牌变化、宿主发来请求，都断开连接。
  - 自用构建缺少 cdhash 时编译失败，不会悄悄降级成弱校验。
  - 发布构建的签名要求在 T180 之前为空，所以现在发布版 CLI 不接受任何宿主。这与网关一侧的做法一致，接受。
- 规模：1,206 行（代码 741 行，测试 465 行），在上限以内。
- 裁定已写进 `02-host.md` 5.2.14 末尾的"T045 实现时的裁定"；交接事项已写进 `handoff-notes.md`（T005、T002、T039）。T180 要设置 `verify::HOST_REQUIREMENT`，另行登记。
