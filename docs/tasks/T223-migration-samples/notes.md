# T223 notes

## 开工前的规模估计（2026-11-03）

停止线 300，硬停止线 400，生成的样例库不计（`testdata/` 本来就不计）。全部写在 `crates/cupper-host/tests/migrations.rs`，挂在 IT-migrations-samples 下：

| 部分 | 内容 | 行 |
|---|---|---|
| 脚本的锁 | 已发布脚本的名字和 sha-256 固定在用例里，改了就失败，提示改表结构只能加新脚本 | 20 |
| 转储 | 把 `host.db` 转成 SQL 文本：建表语句原样（来自 `sqlite_master`），每行一条 `INSERT`（`quote()`），表按名字、行按全部列排序，临时目录换成固定占位，不留本机路径 | 30 |
| 样例的生成 | 最新版本还没有样例时，在环境变量允许下由宿主生成：假 agent 跑一个会话（工具调用、长输出转 blob、权限请求、排队后中断），改一个安全相关的设置（审计），有序退出后转储；不覆盖已有样例 | 50 |
| 通用用例 | 每个样例：先核对它的表结构等于当前脚本建到该版本的结构（第二道防线，原地改脚本连哈希一起改也逃不过），再让宿主在上面启动（启动时迁移），然后检查 `schema_version`、`integrity_check`、`foreign_key_check`、数据保留（表结构没变的表逐行相同，其余行数相同） | 55 |
| `migrate.rs` 注释 | | 5 |
| 合计 | | 约 160 |

## 完成时

- `cargo xtask size`：+203 −1（不含 `testdata/migrations/v1.sql`）。
- 样例 `v1.sql` 有 62 行数据，覆盖全部 16 张表。本机路径都已替换：临时目录换成 `/sample`，仓库根目录换成 `/cupper`；生成时还断言样例里不含 `HOME`。
- 变异检查：
  1. 给 `0001` 的 `projects` 加一列、不改 pin：锁和样例两个用例都失败；
  2. 同样的改动、连 pin 一起改：锁的用例通过，样例的用例因 "v1's schema" 不符而失败。

## 以后改表结构的步骤（给设计 5.4.6 的建议）

1. 新加 `migrations/NNNN_<名称>.sql`（下一个编号），登记进 `MIGRATIONS`。已有的脚本一个字也不改：`main` 上的脚本都算已发布，自用构建就是从 `main` 出的。
2. 把新脚本的名字和 sha-256 追加进 `tests/migrations.rs` 的 `RELEASED`。只追加，不改已有的行。
3. 生成这个版本的样例：`CUPPER_WRITE_MIGRATION_SAMPLE=1 cargo test -p cupper-host --all-features --test migrations it_migrations_samples`，会写出 `testdata/migrations/v<NNNN>.sql`，连同脚本一起提交。只生成最新版本、而且没有样例时才会写，已有的样例不会被覆盖。生成时断言每张表都有数据：新表在样例的剧本（`make_sample`）里没有数据，就补一步。
4. 通用检查是：表结构没变的表逐行相同，变了的表行数相同，`meta` 除 `schema_version` 外不变。脚本要转换数据的，在 IT-migrations-samples 旁边加针对这次转换的断言。
5. 合并之后，脚本和样例都冻结。
