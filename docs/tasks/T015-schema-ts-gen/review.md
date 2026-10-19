# T015 Review

## 第 1 轮（2026-10-19，编排者）

- 结论：通过，合并。
- 过程：编排者把 main 合入分支（`0e0cde2`）。
  - `xtask` 的 rust 作业起了冲突，只在模块文档上。两边的步骤都保留：T015 的 `ios-sim` 和 T026 的 `test-host-shuttle`。
  - 合并后重跑 `pnpm gen`，生成物没有变化。
  - 在任务 worktree 里跑 `cargo xtask ci pr --record`：rust 688、protocol 176、ts 232、license 1，全部通过，摘要为 `ci/pr-0e0cde2f139d.json`。前两次是 license 作业失败，原因是 GitHub 连不上，拉不到 advisory 数据库，与代码无关。
- 核对：
  - 没有 `unsafe`，没有个人信息；
  - npm 依赖都锁了精确版本，许可证列在汇报里；
  - 只有 `zod` 会随以后的 host-client 进 App，它是 MIT。
- 审阅：
  - `v1.json` 写成"加了缩进的 JCS"：键序、数值、字符串都按 RFC 8785，只多了空白，测试核对了"去掉空白即 JCS"。接受，因为契约的 diff 要能审阅；T016 冻结时也按这个形式。
  - **后处理用文本改写，每处都检查是否命中**。hey-api 升级后输出一变就会失败，不会悄悄出错。接受。
  - **执行者用样例发现了 hey-api 的四个缺陷**，并在转 OpenAPI 或后处理时修正：
    - `$ref` 旁的关键字被忽略，联合成员丢了标签；
    - `true` 子 schema 被丢掉；
    - 开放对象需要 `.loose()`；
    - tsconfig 查找。
  - **兜底成员与 Rust 的宽容模式对齐**：
    - 嵌套联合只接住不认识的标签；
    - 事件层整体降级为未知事件，用 `isKnownEvent` 判断。
    - 拿修正前的生成物跑，有 4 个样例失败，说明样例确实覆盖到了这些情况。
  - **反向验证**：改一个 Rust 类型而不重新生成，`protocol` 作业会失败。
- 规模：940 行，不含生成物和夹具。
- 编排者的后续处理：
  - 设计 5.3.10 补上实际的后处理步骤；
  - 计划 1.8 补上共用 `CARGO_TARGET_DIR` 的陷阱；
  - 交接事项写进 `handoff-notes.md`：T071、T072、T016、T006。
