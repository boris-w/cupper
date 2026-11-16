# T108 notes（浸泡，SP-6 场景 5）

## 开工前的估算（2026-11-16）

停止线 300，硬停止线 380。脚本计入，报告和数据不计。

| 文件 | 内容 | 行 |
|---|---|---|
| `spikes/T108-soak/run.sh` | 照 T103 的 `run.sh`：先构建 e2e App（`SP6_SKIP_BUILD=1` 跳过），记下机器信息，再跑浸泡用例 | 25 |
| `spikes/T108-soak/playwright.config.ts` | 照 T103：沿用 `tools/e2e` 的 global setup；单 worker；超时按浸泡时长加余量 | 15 |
| `spikes/T108-soak/soak.spec.ts` | 见下文 | 120 |
| `spikes/T108-soak/README.md` | 怎么经 `spike-run` 跑、环境变量 | 10 |
| 合计 | | 约 170 |

**`soak.spec.ts`**：
- 用 `tools/e2e` 的夹具和 `onScreen`，会话开着显示在窗口里，让渲染进程真的在渲染；回合用 `asWindow` 发 `session/send`。
- 写结果直接用 T103 `measure.ts` 的 `record`、`stats`，不复制。
- **每个回合**：假 agent 跑脚本 `tool tN execute … ; tool_done tN 4000 ; flood_rate 20 10 80 ; say done N`，含工具调用和约 10 秒的流式输出。回合结束后采样，然后等到下一个 5 分钟整点。
- **每次采样**：
  - 主进程：`getAppMetrics` 的 working set，加上 `process.memoryUsage()` 的 JS 堆；
  - 主文档的渲染进程、审查文档的渲染进程：分别按进程号对应到 `getAppMetrics` 的 working set；JS 堆用页面里的 `performance.memory`；
  - 宿主：`ps` 的 RSS；
  - 会话数、`host.db` 的事件数。
- **结束时汇总**：预热（默认 30 分钟）之后每个进程的增长量和最小二乘斜率（MB/小时），对照 5.9.6 的上限（主进程 ≤ 50 MB、渲染进程 ≤ 100 MB、宿主 ≤ 20 MB）。只记录，不判失败。
- **时长参数**：时长、间隔、预热都由环境变量给。短版是 20 分钟、5 分钟一回合、预热 5 分钟。

## 跑法

- 短版：`spike-run`，dir `spikes/T108-soak`，command `SOAK_MINUTES=20 SOAK_WARMUP_MINUTES=5 bash run.sh`，minutes 60。
- 完整版：command `bash run.sh`（默认 300 分钟、预热 30 分钟），minutes 360。构建约 15–20 分钟，加上 300 分钟浸泡，在 6 小时以内。

## 运行记录

| 版本 | run | 构建 | 浸泡 | 结果 |
|---|---|---|---|---|
| 短版 | 43797719590 | 8 分钟 | 20 分钟（预热 5 分钟），4 个回合 | 数据齐全，确认脚本可用 |
| 完整版 | 43800800252 | 6 分钟 | 300 分钟（预热 30 分钟），60 个回合 | 作业 307 分钟，详见 `report-soak.md` |

本机只做了一次 3 分钟的冒烟（1 分钟一回合）。
