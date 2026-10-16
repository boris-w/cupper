# SP-2 Review

## 第 1 轮（2026-10-16）

- 结论：通过
- 审阅范围：`git diff main...spike/SP-2-adapter-distribution`，共 28 个文件：脚本、数据、锁文件、runtime-manifest.json、licenses.md、notes.md

### 存档说明

报告正文原样存档，只改了两处：
- worktree 路径里的本机用户名换成 `$HOME`；
- "本机痕迹"一节改为摘要，完整清单存放在不入库的 `.private/cleanup/SP-2-leftovers.md`。

合并前，编排者扫描了分支的 diff，只命中一句"本机只有 Apple Development 证书"的一般性描述，里面没有证书名称，也没有 Team ID。

### 验收标准核对

| 验收标准 | 结论 |
|---|---|
| 第 1、2 项实测（本机 CLI、版本不一致时的出错形态） | 达标，共 25 次运行，并附协议差异分析 |
| Node 来源对比表和推荐方案 | 达标，比较了 6 种方案，附签名矩阵和启动耗时 |
| 单文件打包 | 达标，包括升级时替换文件的实测 |
| 首次运行安装草案 | 达标，原型试跑覆盖 5 种失败情况 |
| 许可证清单和 ADR-11 建议 | 达标，共 133 个包 |
| ADR-08 建议 | 达标 |

### 意见

| # | 位置 | 问题 | 处理 |
|---|---|---|---|
| 1 | 第 3 项 | 公证没有实测，本机没有 Developer ID | 接受。放到有 Developer ID 的 CI 上验证 |
| 2 | 遗留问题第 3 条 | 宿主怎么找到用户的 CLI 还没有设计 | 写进 ADR-08 的查找顺序，细节留给详细设计 |
| 3 | 遗留问题第 4 条 | `clientInfo.name` 会出现在 codex 发往 OpenAI 的 User-Agent 里 | 写进 ADR-08：固定写成 `cupper` |
| 4 | 第 6 项 | cupper 自身的许可证建议用 Apache-2.0 | 写进 ADR-11，交用户确认 |
| 5 | 本机痕迹 | scratchpad 里还有约 5.1 GB 的测试数据，日志里有主机名和配置细节 | 编排者确认没有进程在使用后，已删除 `scratchpad/sp2/`。`~/.claude` 和 `~/.codex` 下的会话记录由用户决定是否清理 |

### 合并

- 合并提交：`d01a03b`
