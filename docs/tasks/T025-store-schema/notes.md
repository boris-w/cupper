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

## 实际行数

每个提交后统计一次（相对分支起点的新增行数，不含 `Cargo.lock` 和 `docs/`）：

| 提交 | 本提交新增 | 累计 |
|---|---|---|
| 0001 迁移与表结构比对 | 325 | 325 |
| 迁移框架、备份、IT-migrations | 618 | 931 |
| raw.db 重建 | 133 | 1,061 |
| 升级器与 `view_v` | 287 | 1,348 |

按模块：SQL 206 行；实现与文档约 510 行（`migrate.rs` 238、`upcast.rs` 142、`meta.rs` 60、`raw.rs` 54，其余是 `mod.rs`、`failpoint.rs`、`Cargo.toml`）；测试 630 行（IT 357、`upcast.rs` 144、`testutil.rs` 68、`migrate.rs` 47、`raw.rs` 14）。比估计多约 140 行，主要是 rustfmt 展开后的 IT 和升级器测试。没有超过上限。

## 取舍和试错

- **脚本的版本号就是它在列表里的位置**，`Migration` 只有 `name` 和 `sql`。单元测试核对两件事：名字以 `NNNN_` 开头且等于位置；`migrations/` 目录里的文件和 `MIGRATIONS` 一一对应，防止加了脚本文件却忘了登记。
- **新库的 meta 初值写在第一个脚本的事务里**（`host_id`、`created_at`、`layout_version`、`data_epoch = 0`）。这样不存在"有 `schema_version`、没有 `host_id`"的库；如果在启动时另写，脚本提交后、初值写入前崩溃就会留下这种库。
- **备份先写到 `backups/host.partial`**，`sync_all`（Rust 在 macOS 上用 `F_FULLFSYNC`）后改名为正式文件名，再 fsync 目录。原因：已有同一版本的备份就不再备份，`VACUUM INTO` 中途崩溃留下的半截文件如果直接用正式文件名，下次启动会被当成备份。下次备份前先删掉残留的 `host.partial`（`VACUUM INTO` 要求目标不存在或为空）。
- **备份文件名的时间**用 `YYYYMMDDTHHMMSSZ`（UTC，取自 `Clock`）：人能看懂，字典序就是时间序，以后"保留最近 3 份"直接按名字排序。没有引入日期库，用 Howard Hinnant 的 `civil_from_days` 算法，单元测试覆盖了闰日和世纪边界。
- **`backups/` 不存在时以 0700 创建**。数据目录本身的权限检查归 T037。
- **表结构与设计的比对**：测试直接读取 `docs/design/04-data-model.md` 里 5.4.2 的 SQL 块，在内存库里执行，再用 `pragma_table_list`、`pragma_table_xinfo`、`pragma_foreign_key_list`、`pragma_index_list`、`pragma_index_xinfo` 和索引的 SQL（部分索引的 `WHERE`）逐条比对。改了设计没改脚本，或者反过来，测试都会失败。T160 之后有了多个脚本，比对的是"全部脚本执行完"的结果，仍然成立。
  - 变异检查（逐项临时改 `0001`，确认测试失败后还原）：默认值、`DESC`、部分索引条件、`WITHOUT ROWID`、外键、`UNIQUE` 索引、`NOT NULL`、列上的 `UNIQUE`，8 项都测出来了。失败信息只列出两边不同的行。
- **raw.db** 用 `PRAGMA user_version` 记版本。版本不同（包括更新的版本）就在一个事务里删掉所有表，再建表、写版本号。没有选择删文件重建：`open_writer` 已经打开了连接，删正在打开的文件不安全。删表之后的空闲页由之后的写入复用，空闲时 `VACUUM` 归 T027。
- **故障点 `store.migration_committed`**：每个脚本提交后命中一次，测试用 `hit` 选在第几个脚本之后崩溃。子进程就是测试二进制自己（`--exact crash_child --ignored`），和 T023 的故障点单元测试同一个做法。子进程测试名不带用例编号，不计入 IT-migrations。
- **升级器只改 `data`**，不改事件类型名。`v` 为 0 或比当前版本新都报 `UnknownVersion`。每种类型的升级器必须从 v1 开始、连续、不重复，`Upcasters::new` 检查，不满足就 panic（这是代码写错了，`Upcasters::host()` 的单元测试保证宿主自己的列表不会触发）。
- **"每个升级器都有样例"做成了测试**：测试里有一张样例表，`UPCASTERS` 里每个升级器都必须在表里有"旧样例 → 新样例"，否则失败。
- **`VIEW_VERSION` 放在宿主的 `store::upcast`**。检查点里存的是协议视图的 JSON，本分支上 T014 的视图还没有合并进来，所以没有放进 `cupper-protocol`。
- **mutation 检查**（迁移框架，逐项临时修改、确认 IT 失败后还原）：不跳过已执行的脚本、忽略已有备份、脚本不放在事务里、"过新"的判断差一、所有情况都备份，5 项都测出来了。
