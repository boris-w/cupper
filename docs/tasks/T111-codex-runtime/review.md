# T111 评审

- 评审人：编排者
- 日期：2026-11-03
- 结论：**通过，合并**（PR #41）

## 核对

- **规模**：毛计 +1016 −278；搬动提交 14df4ac 按净增计（T110 的补充裁定二），合计 848，停止线 900 以内。
- **CI**：PR #41 https://github.com/boris-w/cupper/pull/41 ，cf1c557 上 8 个检查全绿（run 41581455721）。
- **个人信息**：改动的文件过了敏感词清单，干净。执行者读过 jsdelivr 上 `@openai/codex` 的公开源码，请求用 curl 默认 UA，没有个人信息；没有安装或运行真实的 Codex。

## 逐项意见

1. **按 agent 分派的做法对**：同一个 actor 换分类器和提取层，T218 的判断留在分类器之后，对 Codex 自然生效，有两种 agent 都跑的用例守着。没有另起 actor，避免了重复的生命周期代码。
2. **回收**：`reclaimable` 的条件（在途请求、待答请求、打开中的会话、线程状态、10 s 静默、未在停止中）完整；回收与宿主的停止撞上时，停止接管回收（`stopping.done` 改为 `Some`）；回收和重启后的 `ensure_started` 只在有等待的 attach 时才起实例，不会把刚回收的适配器立刻拉起来。已知局限（K3 的命令在 2 GiB 时被回收）在汇报和设计里都写明了。
3. **未信任的项目在运行时拦住**：codex-acp 一建会话就拉起项目里的 MCP，只靠 `session/create` 不够，会话在项目变为 stale 后仍会以 `Ignore` 重新 attach。这是安全上必要的一层。
4. **测试不会碰本机的真实 codex**：进程内启动宿主的两处测试给 Codex 也注入了替身，这一点执行者自己发现并修了。
5. **只解析 arm64 的原生二进制**：与 10-release 的"只出 arm64 构建"（Q6）一致。

## 记下的点

- **设计**（本次合并时改）：06 的 `AdapterExited` 一行加 `reclaimed`；`AgentError` 分类加 `project_untrusted`，分类器按 agent 分；5.6.8 Codex 一行写明 T111 的实现和已知局限，"手动"一行写明 `agents/restart` 不计入失败；03 的第 7 条和 `FailureCode` 加 `project_untrusted`。
- **交接事项**（已写入 handoff-notes）：界面缺 `error_project_untrusted` 文案；长时间运行下 Codex 的进程数和内存要真实 Codex 核对；lifecycle 的 CLI 用例在本机要先构建带 `test-harness` 的 CLI。
