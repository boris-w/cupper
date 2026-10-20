# T052 Review

## 第 1 轮（2026-10-20，编排者）

- 结论：通过，合并。
- 过程：开工前估计约 2,400 行，执行者停下汇报，按任务书预留的拆法拆分（计划 v1.3.12，回放模式与 persona 比对归 T067）。收窄后合并 main（没有冲突），在 worktree 里重跑 `cargo xtask ci pr --record`，全部通过：rust 1205、protocol 185、ts 323、license 1、testdata-scan 3（`sensitive-words` 跳过，本机还没有清单），摘要 `ci/pr-08ffba9a86bf.json`。执行者那次的 `pr-de0e404e3921.json` 一并保留。
- 核对：
  - 格式库 `cupper-protocol::recording` 不做文件 I/O，不派生 JsonSchema，不进 schema 导出；iOS 模拟器构建通过。
  - `tools/sanitize` 只用 trace-ids 已有的开发依赖。
  - 扫描命中只输出"文件:行号: 规则名或条目序号"（`scan.ts` 的 `report`），不输出命中的文字。
  - 没有个人信息。转换后的 48 份录制过了 `tools/sanitize` 的重新扫描，脱敏输出与转换结果逐字节相同，说明 SP-1 的脱敏没有漏网，规则不用改。
- 审阅：
  - **格式校验**按补充裁定第 1 项实现，`FormatError` 带行号；用例把入库的录制全部读一遍，并核对路径与头行一致、段数和 `instance_exited` 的个数。
  - **头行的 `scenario` 取文件名**：SP-1 的两次补跑（`s07x`、`s10x`）在 SP-1 自己的头里写的是原场景名，取文件名才不会重名。认可。
  - **占位域名的邮箱在通用扫描里放行**、**通用扫描不含长 base64 串**（测试图片会误报；脱敏本身仍把长串换成 `<opaque>`）：认可。
  - **跳过的可见性**：终端提示加摘要的 `skipped` 字段，字段白名单和对应用例都同步了。
- 编排者裁定：两份 Claude 录制里的 `/private/tmp/claude-<uid>/…` 路径，uid 是 macOS 第一个用户的默认值，路径其余部分已脱敏，不算个人信息，规则不改。
- 规模：代码 +1,515 −4 行（不含 `pnpm-lock.yaml`、转换后的录制和 notes），在收窄后约 1,600 的上限以内。
- 转交事项已写进 `handoff-notes.md`：T053、T067（格式库的接口）、T047（逐行写出用 `Header::to_line`、`Frame::to_line`）。敏感词清单仍待用户在 T066 之前建（计划第 6 节第 4 行），建好后 `testdata-scan` 自动启用这一步。
