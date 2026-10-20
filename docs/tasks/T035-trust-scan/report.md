# T035 汇报

- 执行者：子 agent（Rust 工程师，宿主安全）
- 分支：`feat/T035-trust-scan`
- 完成日期：2026-10-20

## 做了什么

收窄后的 T035 已经完成：`trust` 模块的范围、摘要、状态，`projects` 表的清单列，以及合并前补上的目录项上限，都已提交。UNIT-trust-scan 的 6 个用例全部通过，`cargo xtask ci pr` 也全部通过。代码共约 771 行（实现约 485，测试约 300，另有 schema 改动 2 行），在约 1,100 行的上限之内。findings 按裁定留给 T096。

1. **`projects.trust_manifest`**：`0001` 与设计 5.4.2 的 SQL 各加了这一列。
2. **`trust::scan(workspace, git_root)`**（`walk.rs`、`mod.rs`）
   - **范围**：先把工作区和根目录都转成 realpath，再从工作区逐级向上直到 git 根目录，检查每一级的 `.claude`、`.mcp.json`、`.codex`。不是 git 仓库时只看工作区本身。根目录不是工作区或它的上级时，扫描失败。
   - **符号链接一律跟随**：解析 realpath 时，`NotFound` 记为悬空，`ELOOP` 记为成环；目录目标如果已经在遍历栈上，也算成环。目标不是普通文件或目录的，扫描失败。
   - **上限**：文件 2,000 个、合计 16 MiB，链接目标计入。读取时最多读到剩余额度加 1 字节，所以读的过程中文件变大也能发现。
   - **结果**：`Scan.files` 按相对路径排序，上级目录的文件以 `../` 开头。每个文件带：目标 realpath、`linked`、`outside_workspace`（经过链接且目标在工作区外）、哈希、完整内容（留给 T096 解析）。
   - **失败语义码**：`ScanError::code()` 返回其中之一：`too_many_files`、`too_large`、`too_many_entries`、`dangling_link`、`link_cycle`、`not_regular`、`unreadable`、`root_not_ancestor`。
   - 模块是同步实现，模块文档写明调用方要放进阻塞线程池。
3. **摘要与清单**（`digest.rs`）
   - 每个文件对"相对路径、目标 realpath、内容"三个字段做 sha-256，每个字段前带 8 字节大端长度。
   - 总摘要 = sha-256(`cupper-trust-v1` 标签 + 按路径排序的逐文件哈希)，输出 `Digest`（`d_` 加 64 位十六进制）。没有文件时是一个固定值。
   - `Manifest` 用 `BTreeMap` 保存，键有序，提供 `to_json`、`from_json`，不依赖 serde_json 的键顺序。
   - `Manifest::diff` 按路径排序，输出 `TrustedFileChange`（add、update、delete）。
4. **状态纯函数**（`state.rs`）
   - `Trust::added`：扫描范围里没有文件时记为 `trusted`、`auto_empty`，否则为 `untrusted`。
   - `rescanned`：已信任但摘要变了就转为 `stale`；内容改回原样也保持 `stale`。
   - `confirmed`：用户确认的摘要与重新扫描的结果不一致时，返回 `DigestMismatch`。这个函数原本叫 `trust`，clippy 不允许与类型同名，所以改了名。
   - `diff_from_trusted`：只对 `stale` 给出差异。
5. **目录项上限**（合并前的补充裁定）
   - 遍历时访问的目录项总数超过 20,000，扫描以 `too_many_entries` 失败。文件和目录都计数，经目录链接重复进入的也计数。
   - 写用例时发现，原实现每访问一个条目都要沿路径重新解析上面所有的链接。层数越深越慢：20,000 个条目要约 3.8 s，而且一条路径上链接超过 32 个时，`ELOOP` 会被误报为成环。
   - 现在进入目录后，子条目从该目录的 realpath 往下拼，同样的用例约 0.75 s，深路径也不再误报。`rel` 仍是经过链接的路径；代价是报错时显示的路径变成 realpath 下的位置。

## 关键取舍

- **三个名字统一处理**：每一级的三个名字，不论是文件、目录还是链接，都走同一套规则。
- **保留文件内容**：`ScannedFile` 保留完整内容，T096 不必再读一次文件。内容合计最多 16 MiB。
- **按功能分提交**：扫描与摘要、状态、目录项上限分成三个提交，每个都能单独构建并通过测试。

## 验证方式

- [x] **UNIT-trust-scan**：`trust/tests.rs` 里 6 个用例，名字都以 `unit_trust_scan_` 开头，只读写用例自己创建的临时目录。
  - **范围**：工作区在 git 根目录下两层；根目录之上、工作区之下的配置不纳入；不是 git 仓库；根目录不是工作区的上级。
  - **链接**：文件链接指到工作区外；目录链接指到工作区内；悬空链接；目录成环；两个链接互指；套接字。
  - **上限**：2,000 个文件通过、第 2,001 个失败；16 MiB 通过，同一个大文件经目录链接再计一次后失败。
  - **目录项上限**：20 层、每层两个目录链接都指向下一层，不成环也没有文件。断言以 `too_many_entries` 失败，并在 10 s 内返回（实测约 0.75 s）。没有这道上限时，要访问约 2^21 个条目。
  - **摘要**：内容、路径、链接目标任一变化都会让摘要改变；按相反顺序创建同一棵树，摘要不变；空项目的摘要是固定值。
  - **状态与差异**：`auto_empty` 之后出现文件转为 `stale`；用错误的摘要确认被拒、用正确的摘要确认通过；update、add、delete 三种差异；内容改回原样仍是 `stale`；清单 JSON 键有序，读回与写入一致。
- [x] `projects` 表的新列在 `0001` 和 5.4.2 里一致，store 和 migrations 的测试都通过。
- [x] 编号已在 `tests/ids.toml` 的 T035 段，`testdata-scan` 通过。
- [x] 在干净的 worktree 里运行 `cargo xtask ci pr --record docs/tasks/T035-trust-scan/ci/`：rust 1273、protocol 185、ts 323、license、testdata-scan 全部通过。最新摘要 `ci/pr-83e127aba492.json` 已提交，之前那份 `pr-5190dd2e12d8.json` 也保留着。`sensitive-words` 这一步被跳过，原因是本机没有敏感词表。

## 提交列表

```
11a8f64 docs(task): T035 CI summary (pr at 83e127aba492)
83e127a docs(task): T035 notes: the entry limit
ff51f36 feat(host): stop a trust scan after 20,000 entries
2fdb69e docs(task): T035 CI summary (pr at 5190dd2e12d8)
5190dd2 docs(task): T035 notes: actual lines, scan and state points, hand-offs
33e17ff feat(host): trust states as pure functions of a scan
f0b62d2 feat(host): trust scan of the project configuration and its digest
6105b40 feat(host): projects keep the per-file list of what was trusted
66726fa docs(task): narrowed T035 estimate after the split
fe83e50 docs(task): estimate T035 by module and propose a split
```

## 遗留问题 / 范围外发现

1. **交给后续任务的接入点**（细节在 `notes.md`）
   - **T096**：从 `Scan.files` 的 `path`、`content` 解析 findings；`target`、`outside_workspace` 直接取 `ScannedFile` 上的字段。
   - **T034**：
     - 扫描放进阻塞线程池；
     - 错误映射：`ScanError` 对应 `unavailable/scan_failed`，`DigestMismatch` 对应 `conflict/digest_mismatch`；
     - 写库用 `Manifest::to_json` 和 `TrustedBy::as_str`；
     - findings 暂时返回空列表。
   - **T044**：每次 Attach、每次发 prompt 之前、每个回合结束时，调用 `scan` 和 `Trust::rescanned`；git 根目录由 T060 传入。
2. **分支基点**：T035 分支建在 T096 拆分合入 main 之前，所以分支里的 `tests/ids.toml` 没有 T096 那一段。分支上的文档和登记表本身是一致的，合并时 main 那边的改动会一起带进来。