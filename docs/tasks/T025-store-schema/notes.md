# T025 工作笔记

## 开工前的行数估计

计划估为 M（400–900 行），上限约 1,500 行。按模块估（含测试，不含 `Cargo.lock` 和 CI 摘要）：

| 模块 | 实现与文档 | 测试 |
|---|---|---|
| `migrations/0001_initial.sql`、`store/raw.sql`（5.4.2 的全部表和索引） | ~170 | — |
| `store/meta.rs`（meta 的键和读写） | ~60 | — |
| `store/migrate.rs`（迁移框架、备份、新库 meta 初值） | ~250 | ~110（表结构与 5.4.2 比对、脚本编号、备份文件名的时间） |
| `store/raw.rs`（raw.db 结构变化时重建） | ~60 | — |
| `store/upcast.rs`（事件升级器、`view_v`） | ~110 | ~130 |
| `failpoint.rs`、`store/mod.rs`、`Cargo.toml` | ~20 | — |
| `tests/migrations.rs`（IT-migrations） | — | ~300 |
| 合计 | ~670 | ~540 |

总计约 1,210 行，在上限以内，但余量不大：SQL 和集成测试是大头，写的时候要省着写。
