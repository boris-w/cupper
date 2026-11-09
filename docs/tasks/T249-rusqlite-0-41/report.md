# T249 汇报

- 执行者：子 agent（Rust 工程师，宿主存储）
- 分支：fix/T249-rusqlite-0-41
- 完成日期：2026-11-09

## 做了什么

1. `crates/cupper-host/Cargo.toml` 里，`rusqlite` 从 `=0.40.2` 改为 `=0.41.1`，仍是精确版本，特性仍是 `bundled`、`fallible_uint`。`cargo update -p rusqlite --precise 0.41.1` 在 `Cargo.lock` 里只动了两项：
   - `libsqlite3-sys` 0.38.2 → 0.39.1
   - `rusqlite` 0.40.2 → 0.41.1
2. 读了 rusqlite v0.41.0、v0.41.1 的发布说明，把和宿主可能有关的变化逐条对照，记进了 `docs/tasks/T249-rusqlite-0-41/notes.md`。结论是代码一行都不用改：
   - **PRAGMA 名的检查**（#1857）：现在 `pragma_*` 会先用 `pragma_list` 检查名字。宿主用到的 `journal_mode`、`checkpoint_fullfsync`、`foreign_keys`、`synchronous`、`fullfsync`、`user_version`、`page_count`、`freelist_count`、`page_size` 都是合法的名字。
   - **`column_names` 等改为返回迭代器**（#1877）：宿主没用到。
   - **bundled 构建不再开 `SQLITE_ENABLE_MEMORY_MANAGEMENT`**（#1900）：宿主没调 `release_memory`。
   - **`trace_v2`、`busy_handler` 的 use-after-free 修复**（#1899）：宿主没用到。
   - **`wal_checkpoint_v2` 的绑定挪了位置**（#1915）：宿主没直接调。
   - **最低 SQLite 3.45.3、bundled 升到 3.54，Edition 2024、MSRV 1.88**：对宿主没有影响。
3. 核对了 SQLite 的实际版本：3.54.1，满足 ≥ 3.54.0。
   - libsqlite3-sys 0.39.1 打包的 `sqlite3.h` 写的是 `SQLITE_VERSION "3.54.1"`；
   - 构建出的宿主二进制里，版本串也是 `3.54.1`，source id 为 `2026-10-16 14:45:20 555e07ef…`。
   - 宿主不读 `sqlite_version()`，也没有版本断言，这次没有加。

## 关键取舍

- 只升 rusqlite 和它带的 libsqlite3-sys，不动其他依赖，也不改用系统 SQLite（任务书的"不做"）。
- 没有加 SQLite 版本断言：bundled 构建的版本由 libsqlite3-sys 锁定，cargo-deny 每个 PR 都会核对公告。

## 验证方式

- **cargo-deny**：本机 `cargo deny --locked fetch db` 后跑 `cargo deny --locked --offline check`，advisories、bans、licenses、sources 四项都通过。输出里没有 RUSTSEC-2026-0459，只剩原本就有的 4 条重复依赖警告（base64、convert_case、getrandom、syn）。
- **宿主测试**（本机单份跑，先构建了 fake agent 和 e2e 版 CLI）：
  - `cargo test -p cupper-host --all-features --locked --no-fail-fast`：lib 527 个通过，集成测试全过。
  - `cargo test -p cupper-host --features test-harness --locked --no-fail-fast`：lib 528 个通过，集成测试全过。
  - 迁移、WAL、持久性相关的用例都在里面。
- **静态检查**：clippy 在 `--workspace --all-features`、`-p cupper-host --features test-harness`、`-p cupper-host` 不带特性三种组合下都干净；fmt 干净。
- **PR #91**：https://github.com/boris-w/cupper/pull/91 ，head 为 bb7a891c。
  - CI 运行：https://github.com/boris-w/cupper/actions/runs/42599440310
  - 8 个检查全部 pass：compat-framework、e2e、license、protocol、rust、testdata-scan、ts、ui。license 作业里 cargo-deny 这一步是 ok。
- **验收标准**：
  - [x] license 作业通过，RUSTSEC-2026-0459 不再出现；
  - [x] 宿主的 lib 和集成测试都通过，clippy、fmt 干净；
  - [x] PR 的 8 个检查全部通过。
- **规模**：Cargo.toml 改 1 行，lock 文件不计；另有 notes.md。

## 提交列表

```
bb7a891c fix(host): rusqlite 0.41.1, SQLite 3.54.1 (RUSTSEC-2026-0459)
```

## 遗留问题 / 范围外发现

- `spikes/SP-4a-host-skeleton` 自成一个 workspace，里面还写着 `rusqlite = "0.40"`。它是原型实测代码，不在根 workspace 里，cargo-deny 不扫它，这次没有动。
