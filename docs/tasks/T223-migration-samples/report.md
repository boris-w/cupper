# T223 汇报

- 执行者：子 agent（Rust 工程师，宿主存储）
- 分支：`test/T223-migration-samples`（从 main `e035a8a` 开出）
- 完成日期：2026-11-03
- PR 和 CI 运行链接：还没有。`038e9de` 已请你推送；交汇报时还没有 PR 和 CI 结果，失败的话叫我回来修。

## 做了什么

全部写在 `crates/cupper-host/tests/migrations.rs` 里，挂在 IT-migrations-samples 下；另改了 `migrate.rs` 的一段注释。

1. **已发布脚本的锁**（`it_migrations_samples_released_scripts_never_change`）：常量 `RELEASED` 按顺序列出每个脚本的名字和 sha-256，从现在的 `0001_initial` 开始。用例要求 `MIGRATIONS` 与 `RELEASED` 完全一致。
   - 改动已发布的脚本，或者新加了脚本却没有 pin，用例都会失败。
   - 失败时提示"改表结构只能加新脚本，在 `RELEASED` 里 pin 它，并加它的样例"。
2. **第二道防线**：光有哈希，改脚本时把哈希一起改就绕过去了。所以样例的用例还会核对：样例里原样保存的表结构，必须等于当前的前 N 个脚本在空库上建出来的结构。原地改脚本，就算连 pin 一起改，也会失败。要绕过就得同时改冻结的样例文件，在 diff 里很显眼。
3. **样例库**（`testdata/migrations/v1.sql`）：由宿主在测试里生成，不是手写的。
   - 生成过程：用假 agent 跑一个会话，依次是工具调用、超过内联上限转成 blob 的长输出、权限请求、排队一条消息、中断；再改一个安全相关的设置（`diagnostics.verbose`），写进设置表和审计表。宿主有序退出后，把 `host.db` 转成 SQL：`sqlite_master` 里的建表语句原样保存，每行数据一条用 `quote()` 写出的 `INSERT`，表按名字、行按全部列排序。
   - 生成时断言 16 张表每张都有数据；v1 共 62 行。
   - 本机路径都已替换：临时目录换成 `/sample`，仓库根目录换成 `/cupper`。生成时还断言不含 `HOME`，`testdata-scan` 的 pii-scan 也通过了。
   - 只在设了 `CUPPER_WRITE_MIGRATION_SAMPLE=1`、最新版本又还没有样例时才生成，不会覆盖已有的样例；缺了样例就失败，并说明怎么生成。
4. **通用用例**（`it_migrations_samples_every_version_migrates_and_a_host_starts`）：对版本 1 到最新的每个样例：
   - 载入后先核对表结构（第 2 条）；
   - 用 `store::migrate` 迁到最新，检查 `from`/`to`、`PRAGMA integrity_check` 为 ok、`pragma_foreign_key_check` 为空；
   - 检查数据保留：表结构没变的表逐行相同，`meta` 除 `schema_version` 外不变，结构变了的表行数相同；
   - 最后用假 agent 的配置在这个数据目录上启动宿主，`host/status` 能应答，有序退出、退出码为 0。
5. **`migrate.rs` 的注释**："Until T160 … changes in place" 改为：`main` 上的脚本都算已发布、永不再改，改表结构就加新脚本，同时在 IT-migrations-samples 里 pin 它的哈希、加它版本的样例。

## 关键取舍（请确认）

- **"已发布"的集合 = `main` 上的全部脚本**：用户的自用构建直接从 `main` 出，任何一次合并都可能被装上，所以没有单独的"发布"动作可挂。更新集合的方式：新脚本在加进来的同一个改动里追加到 `RELEASED`。合并前在分支上可以随意改，合并后就冻结。
- **锁放在用例里，不放在 xtask 里做 git 比较**：CI 的 checkout 是 PR 的合并提交，没有本地 `main` 分支，`git::changed_since_main` 在那里会返回"无从比较"，按 git 历史做的检查在 CI 上会悄悄不生效。所以锁做成普通的 cargo 用例，在 `rust` 作业里一定会跑，本机 `cargo test` 也能及时发现；再配上第 2 条的表结构核对，堵住连哈希一起改的情况。
- **样例用 SQL 文本**：可以审阅、可以 diff，pii-scan 也扫得到。建表语句原样保存，载入后就是当时真实的旧库，不依赖当前的脚本。

## 验证方式

- **变异检查**（验收标准第 1 条），本机临时修改、跑完已还原：
  1. 给 `0001_initial.sql` 的 `projects` 加一列，不改 pin：锁和样例两个用例都失败，提示分别是"a released script changed …"和"v1's schema"。
  2. 同样的改动，同时把 pin 换成新哈希：锁的用例通过，样例的用例仍以"v1's schema"失败。
  - 只改注释这类不影响结构的改动，由哈希拦下。
- IT-migrations-samples 两个用例都通过；migrations 测试二进制共 8 个通过、1 个忽略（原本就有的）。
- `cargo clippy --workspace --all-targets --all-features` 没有警告；`cargo xtask ci testdata-scan` 通过（trace-ids、pii-scan、sensitive-words 都通过）。
- **规模**：`cargo xtask size` 为 **+203 −1**，在停止线 300 以内；生成的样例不计。

## 提交列表

```
038e9de test(store): pin released migrations and keep a sample of every version
```

## 以后改表结构的步骤（建议写进设计 5.4.6）

1. 新加 `migrations/NNNN_<名称>.sql`（下一个编号），登记进 `MIGRATIONS`。已有的脚本一个字也不改。
2. 把它的名字和 sha-256 追加进 `tests/migrations.rs` 的 `RELEASED`。只追加，不改已有的行。
3. 生成这个版本的样例：`CUPPER_WRITE_MIGRATION_SAMPLE=1 cargo test -p cupper-host --all-features --test migrations it_migrations_samples`，会写出 `testdata/migrations/v<NNNN>.sql`，和脚本一起提交。新表在样例的剧本（`make_sample`）里没有数据时，补一步让它有数据（生成时断言每张表都有数据）。
4. 通用检查只覆盖"表结构没变的表逐行相同、变了的表行数相同"。脚本要转换数据的，在 IT-migrations-samples 旁边加针对这次转换的断言。
5. 合并之后，脚本和样例都冻结。

## 遗留问题 / 范围外发现

- 生成样例需要假 agent 和宿主的二进制。只跑 `cargo test -p cupper-host` 时 cupper-cli 不会被重编；生成样例用的是进程内的客户端，所以不受影响。
- 样例里有一处是这次测试二进制的路径（`/cupper/target/debug/deps/migrations-<哈希>`，来自审计里客户端的可执行文件）。不含个人信息，但每次生成都会不同；样例本来就冻结，不影响用例。
