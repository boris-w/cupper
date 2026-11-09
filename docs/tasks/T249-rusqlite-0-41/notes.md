# T249 notes

## 升级

- `rusqlite` 从 `=0.40.2` 改为 `=0.41.1`；`cargo update -p rusqlite --precise 0.41.1` 只动了两项：`libsqlite3-sys` 0.38.2 → 0.39.1，`rusqlite` 0.40.2 → 0.41.1。
- 打包的 SQLite：`libsqlite3-sys` 0.39.1 的 `sqlite3.h` 写的是 `SQLITE_VERSION "3.54.1"`，构建出的宿主二进制里也是 `3.54.1`（source id `2026-10-16 14:45:20 555e07ef…`），在 3.54.0 以上。
- 宿主不读 `sqlite_version()`，也没有版本断言，这次没有加。

## 0.40 → 0.41 的变化里与宿主有关的（读的是 v0.41.0、v0.41.1 的发布说明）

| 变化 | 宿主的用法 | 影响 |
|---|---|---|
| `pragma_*` 先用 `pragma_list` 检查 PRAGMA 名（#1857） | `journal_mode`、`checkpoint_fullfsync`、`foreign_keys`、`synchronous`、`fullfsync`、`user_version`，以及 `raw.db` 回收时读的 `page_count`、`freelist_count`、`page_size` | 都是合法的名字；测试全过 |
| `Statement::column_names` 等返回迭代器（#1877） | 没用到 | 无 |
| bundled 构建不再开 `SQLITE_ENABLE_MEMORY_MANAGEMENT`（#1900） | 没调 `release_memory` | 无 |
| `trace_v2`、`busy_handler` 的 use-after-free 修复（#1899） | 没用到 | 无 |
| `sqlite3_wal_checkpoint_v2` 的绑定挪了位置（#1915） | 没直接调 | 无 |
| 最低 SQLite 3.45.3（#1894）、bundled 升到 3.54（#1913） | bundled | 无 |
| Edition 2024、MSRV 1.88 | 工具链更新 | 无 |

代码一行没改，编译、clippy 都没有新的告警。
