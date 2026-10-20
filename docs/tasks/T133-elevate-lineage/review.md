# T133 评审

- 评审人：编排者
- 日期：2026-10-20
- 结论：**通过，合并**

## 核对

- **规模**：新增 915 行（不含 lock、生成物、schema、fixtures、文档；合并 T038 前 681、合并后 234），在第 2 次裁定的硬停止线 950 以内。
- **CI**：执行者的摘要 `pr-d9f148bb7c45.json` 第一次运行就通过。合并 main 之后在本 worktree 重跑 `cargo xtask ci pr --record`，`pr-5f62847d8a26.json` 通过（rust 1821、protocol 185、ts 394、license、testdata-scan、ui 7）。合并没有冲突。
- **并行方式**：本任务提前派出，先做不依赖 T038 的部分，T038 合并后由编排者把 main 合进来（`87f8938`）再接接口。两边没有冲突，T038 定的窄接口一次也没改，这种做法可行。

## 逐项意见

1. **`cupper-sys`**：控制终端、按审计令牌取签名身份（含叶证书）、宿主自己的签名（`SecCodeCopySelf`）、`setsid` 包装。新的 `unsafe` 都在这里。
2. **血缘三信号**是纯函数：
   - 同会话、同进程组都要求"请求方晚于适配器启动"；祖先链按 pid 加启动时间匹配；
   - pid 被复用的行作废；适配器已退出、但它的会话或进程组还在时仍然算数。
   - 信号的期望按 T003 的结论。
   - UNIT-lineage-signals 覆盖每个信号、pid 复用、被 launchd 收养、适配器已退出、空链。被 launchd 收养的命令不被判为后代，这条用例把设计 5.8.4 写明的已知局限固定了下来。
3. **核实信息**：
   - 无法检查时判为 `Descendant { signals: ["unchecked"] }`，fail closed；
   - 按 pid 读到的信息，只在令牌仍指着那个进程时才算数，防 pid 复用。
4. **`bundled_cli`**：按裁定不读 App 包，只比路径字符串、运行中代码的 identifier 和叶证书。ad-hoc 构建恒为否，所以开发和测试构建里这条路径只有表驱动的纯函数用例覆盖；签名构建出来后，在 T135 或 e2e 里补一次实测。
5. **网关的第二道防线**：`Granted::widen` 按 `expires_at` 判断，时钟取连接的 `Auditor` 里那个，`Dispatch::new` 的参数不变；有用例。
6. **IT-elevate**：在真实宿主加假 agent 上覆盖两类后代被拒、未决与限频、`test/clock_advance` 推进后的过期、`elevation_get` 的核实信息、批准后写权限生效、申请方被 SIGTERM 后申请过期。连跑 10 次全绿，跑完没有残留进程。
7. **探针只在 test-harness 构建里有**，发布构建里没有；xtask 的 rust 作业会先构建它。

## 交接

- **T135**：`VerifiedRequester.bundled_cli` 已经有了，提权框可以直接加提示。
- **T090、T092**：IT-elevate 的探针（`cupper-elevate-probe`）和假 agent 的 `spawn <program> [args...]`，e2e 的 CLI 提权辅助可以照用。
