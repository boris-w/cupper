# T177 工作笔记

- 执行者：子 agent（Rust 工程师，宿主核心）
- 日期：2026-10-20

## 开工前的规模估计

做法照 T175 的中途汇报。

| 模块 | 内容 | 实现 | 测试 |
|---|---|---|---|
| 合并器 | 累积工具输出；超过 `max_inline_bytes` 先发缓冲的 delta，再转 `truncated`；尾部节拍（计时器键扩成枚举）；结束时交出 `Spill`，回填后写 `item.completed`；settle 时记 `abandoned`、带尾部 | 95 | — |
| 写 blob | 核心转交 `Spill`；actor await 写 blob 后交回；`blob_refs` 与事件同事务；依赖接入 blob 存储；时长 `output_tail` | 45 | — |
| IT-orch-long-output（进程内） | `Peer` 读 `0x02` 分片、夹具的 `blob/get`；转 `truncated`、节拍、blob 与读回、回合结束时 `abandoned` | — | 80 |
| 进程级检查 | 真实宿主跑 `tool_done t1 500000`，在线订阅不被重置、新订阅能拿到快照、没有帧超限 | — | 45 |
| **合计** | | **约 140** | **约 125** |

合计约 **265 行**，在约 300 的停止线以内。

## 逐模块统计

（每写完一个模块，用 `git diff --numstat` 统计一次，记在这里。）
