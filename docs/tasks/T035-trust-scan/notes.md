# T035 工作笔记

- 执行者：子 agent（Rust 工程师，宿主安全）

## 开工前的规模估计

按模块估计，行数指 rustfmt 展开后的行，含文档注释和空行。估计前读过：
- 任务书；
- 5.2.6"项目信任"、5.4.2 的 `projects` 表、5.5.10、Q34；
- `cupper-protocol` 的 `project/add`、`project/trust`、`project/trust_info` 类型（`TrustScan`、`Finding`、`FindingKind`、`TrustedFileChange`）；
- 交接事项。

模块本身同步实现：文件读取是阻塞操作，调用方（T034、T044）放进阻塞线程池（5.2.3）。

### 实现（草估）

| 模块 | 内容 | 草估 |
|---|---|---|
| `trust/mod.rs` | 入口 `scan(workspace, git_root)`；`Scan`（文件清单、摘要、findings）；`ScanError` 及给确认框的语义码（`too_many_files`、`too_large`、`dangling_link`、`link_cycle`、`not_regular`、`unreadable`、`root_not_ancestor`）；模块文档 | 100 |
| `trust/walk.rs` | 工作区到 git 根目录的各级，每级的 `.claude/`、`.mcp.json`、`.codex/`；符号链接一律跟随：先 realpath，目录递归进入；按遍历栈上的目录 realpath 判定成环（ELOOP 也算）；悬空链接、非普通文件和目录都失败；2,000 个文件、16 MiB 上限，读之前按元数据累计，读时也限长；相对路径（上级目录以 `../` 开头）、目标 realpath、是否经过链接、是否在工作区外 | 170 |
| `trust/digest.rs` | 逐文件哈希：sha-256 覆盖带长度前缀的相对路径、目标 realpath、完整内容；总摘要：带版本标签，按相对路径排序后串起逐文件哈希，再做 sha-256，输出 `Digest`（`d_` 加 64 位十六进制）；逐文件清单 `Manifest`（相对路径 → 哈希，JSON 存进 `projects`）；清单差异 → `TrustedFileChange`（`add`、`update`、`delete`） | 70 |
| `trust/findings.rs` | `.claude/settings*.json` 里的 hooks（每条命令一项）、`env`（每个键一项）、`apiKeyHelper`、`statusLine`；`.mcp.json` 的 MCP 服务器；`.claude/skills/*/`、`agents/*.md`、`commands/**/*.md` 按路径各一项；`.codex/*.toml` 的顶层配置项（含 hooks、`mcp_servers`）；解析不了的文件记一项"无法解析"；链接目标在工作区外的标 `outside_workspace` | 180 |
| `trust/state.rs` | 纯函数：添加（没有文件就 `trusted`、`auto_empty`，否则 `untrusted`）；重新扫描（`trusted` 摘要变了转 `stale`，`stale` 保持到再次确认）；信任（重新扫描的摘要等于用户确认的才转 `trusted`，记下摘要、清单、谁、何时，否则 `digest_mismatch`） | 80 |
| 其余 | `0001` 与 5.4.2 的 `projects` 加一列；`FindingKind` 加一个"无法解析"的取值（见待裁定 1）与 5.3.11 的一行；TOML 依赖；`lib.rs` | 15 |
| **实现小计（草估）** | | **615** |
| **按计划 1.1 的执行经验乘 1.7** | | **约 1,045** |

### 测试（UNIT-trust-scan）

| 项 | 内容 | 估计 |
|---|---|---|
| 夹具 | 在临时目录里造目录树：工作区在 git 根目录下几层、写文件、建链接 | 50 |
| 范围 | 多级目录；不是 git 仓库；根目录不是工作区的上级时失败 | 40 |
| 链接 | 文件链接、目录链接、链接到工作区外（`target`、`outside_workspace`）、悬空、成环、指向 FIFO | 70 |
| 上限 | 2,001 个文件；合计超过 16 MiB | 30 |
| 摘要 | 对内容、相对路径、链接目标敏感；同一棵树换创建顺序结果不变；空扫描的摘要固定 | 50 |
| findings | 各类条目；`.mcp.json`、`settings.json`、`config.toml` 无法解析 | 90 |
| 状态与差异 | 添加（空项目 `auto_empty`、有文件 `untrusted`）、信任、摘要不一致、`stale`、`auto_empty` 之后出现文件转 `stale`；逐文件差异的三种变化 | 70 |
| **测试小计** | | **约 400** |

### 合计

- 草估 615 + 400 = **约 1,015**，已经接近约 1,200 的上限；
- 按任务书要求的"实现乘 1.7"计，是 1,045 + 400 = **约 1,445**，超出上限约 20%。

任务书要求推算超过约 1,200 行（包括贴线）就在开工前停下，所以停在这里，见下一节。

## 汇报与拆分建议

**方案 A：按任务书举的例子拆成两个任务（建议）。**

- **T035（约 1,050，T044 的前置）：扫描、摘要与状态**
  - 实现（草估 435，乘 1.7 约 740）：`mod.rs`、`walk.rs`、`digest.rs`、`state.rs`，`projects` 表加清单列（`0001` 与 5.4.2）。
  - 测试（约 310）：UNIT-trust-scan 里的范围、链接、上限、摘要、状态与差异。
  - 交付后 T044 的门控（Attach 和 prompt 之前重新扫描、比较摘要、转 `stale`）已经能接上。T034 的 `project/add`、`project/trust` 也只需要摘要和状态，findings 可以先是空列表。
- **T035b（约 400）：findings 解析**
  - 实现（草估 180 加依赖与协议的一行，乘 1.7 约 320）：`findings.rs`，TOML 依赖，`FindingKind` 加取值。
  - 测试（约 90）：UNIT-trust-scan 里的 findings 部分（各类条目、无法解析）。
  - 只影响原生确认框里列出的内容，不影响摘要和门控，可以与 T044 并行。
- UNIT-trust-scan 由两个任务各自交付一部分。也可以给 findings 另起一个编号，比如 UNIT-trust-findings，由编排者定。

**方案 B：不拆，把上限放宽到约 1,450（硬停止线 1,550）。**

- 好处：一次交付，确认框需要的东西也一起有了。
- 代价：单个分支约 1,450 行；T044 的松弛只有 3 个单位，要多等 findings 那部分。

## 待裁定（开工前请编排者一并定）

1. **"无法解析"在协议里没有取值。**
   - `FindingKind` 现在只有 9 个取值：`hook`、`env`、`api_key_helper`、`status_line`、`mcp_server`、`skill`、`agent`、`command`、`codex_config`。
   - 建议加 `Unparsed => "unparsed"`：这是开放枚举，属于附加式改动；同时改 5.3.11 列取值的那一行，重新生成 schema 和 TS（生成物不计行数）。`detail` 写解析错误的位置和原因。
   - 另一个做法是不改协议，借用开放枚举的兜底值 `Unknown("unparsed")`。不建议，因为 schema 里看不到这个取值。
2. **`.codex/` 的 TOML 解析依赖。**
   - 建议 `toml_edit = { version = "=0.25.17", default-features = false, features = ["parse"] }`。它带来的 `toml_parser`、`toml_datetime`、`winnow` 是新依赖，`indexmap` 已在锁文件里；许可证都是 MIT OR Apache-2.0。
   - 不用 `toml` 这个 crate，因为它要多拉 `serde_spanned` 等，本机缓存里也没有。
3. **"工作区之外"以什么为界。**
   - 我按字面以工作区（会话实际使用的目录）为界：只有经过链接、且目标的 realpath 不在工作区 realpath 之下的文件，才标 `outside_workspace`，并在 `target` 里写目标。
   - 上级目录里不经过链接的文件，路径以 `../` 开头，不算"工作区之外"。
   - 后果：链接指向同一仓库里工作区之外的位置时，也会被标出来。
4. **`stale` 之后内容又改回原样**：仍保持 `stale`，直到再次确认（"按未信任处理，直到再次确认"）。这时 `diff_from_trusted` 会是空列表。
5. **16 MB 按 16 MiB**（`16 << 20`）计，与 blob 等上限的写法一致。上限只数文件，不数目录。
6. **清单列**：`projects.trust_manifest TEXT`，存 JSON 对象 `{"<相对路径>": "<sha-256 十六进制>"}`，信任时与 `trust_digest` 一起写。
7. **摘要的编码**：
   - 每个字段前加 8 字节大端长度；
   - 总摘要前加版本标签 `cupper-trust-v1`，以后改算法时，旧摘要自然对不上，项目转为 `stale`；
   - 没有任何文件时，摘要是只对标签做的 sha-256，是一个固定值。
8. **`detail` 原样输出，不截断。** 文件很多时，`project/add` 的结果可能超过 64,000 字节的帧上限。是否截断、截断到多长，留给 T034 处理协议结果时决定。
9. **git 根目录必须是工作区本身或它的上级**（两边都先取 realpath），否则扫描失败（`root_not_ancestor`），视为调用方的错误。

## 拆分之后（计划 v1.3.15）

编排者采用方案 A：本任务收窄为范围、摘要与状态，findings 解析归 T096。9 项待裁定全部认可：第 1、2 项归 T096；第 6 项补充一点，清单的键用有序 map 排序后序列化，不依赖 serde_json 的 map 键顺序。上限约 1,100 行，硬停止线 1,200。

### 收窄后的估计

| 模块 | 草估 | 乘 1.7 |
|---|---|---|
| `trust/mod.rs`（`Scan`、入口、模块文档，不含 findings） | 80 | 135 |
| `trust/walk.rs` | 170 | 290 |
| `trust/digest.rs` | 70 | 120 |
| `trust/state.rs` | 80 | 135 |
| `0001` 与 5.4.2 的清单列、`lib.rs` | 5 | 5 |
| 测试（夹具、范围、链接、上限、摘要、状态与差异） | 310 | 310 |
| **合计** | **715** | **约 995** |

### 实际行数

按提交统计（`git diff --numstat`）。

| 提交 | 内容 | 增 | 估计（乘 1.7 后） |
|---|---|---|---|
| `feat(host): projects keep the per-file list …` | `0001` 与 5.4.2 各加一列 `trust_manifest` | 2 | 5 |
| `feat(host): trust scan …` | `mod.rs`、`walk.rs`、`digest.rs`、`lib.rs`；测试：范围、链接、上限、摘要 | 537（测试 202） | 545 + 230 |
| `feat(host): trust states …` | `state.rs`；测试：状态与差异 | 188（测试 73） | 135 + 70 |
| **合计** | | **725**（实现约 450，测试约 275） | 995 |

实现约 450 行，与草估的 435 相当，没有到 1.7 倍：几个文件都是平铺的同步代码，没有字段多的结构体字面量。

### 实现要点

- **入口**：`trust::scan(workspace, git_root) -> Result<Scan, ScanError>`，同步的阻塞 I/O，调用方放进阻塞线程池。`Scan.files` 按相对路径排好序，每项带：相对路径、目标 realpath、`linked`、`outside_workspace`、逐文件哈希和完整内容（内容留给 T096 解析 findings）。
- **范围**：工作区和 git 根目录都先取 realpath，根目录不是工作区或其上级时失败。从工作区逐级向上，到根目录为止，每级看 `.claude`、`.mcp.json`、`.codex` 三个名字。不存在就跳过；存在的不论是文件、目录还是链接，都按同一规则处理。
- **链接**：
  - 每个条目先用 `symlink_metadata` 判断是不是链接，再 `canonicalize` 取 realpath：`NotFound` 记为悬空，`ELOOP`（macOS 为 62）记为成环；
  - 目标是目录时，若它的 realpath 已经在遍历栈上，就是成环；
  - 目标既不是普通文件也不是目录（套接字、FIFO、设备），扫描失败，不会去读它，所以不会卡在 FIFO 上。
- **上限**：先按元数据累计；读文件时最多读到剩余额度加 1 字节，防止文件在两次查看之间变大。
- **`outside_workspace`**（裁定第 3 条）：路径上经过了链接，并且目标的 realpath 不在工作区的 realpath 之下。
- **状态**：`Trust { state, trusted: Option<Trusted { digest, manifest, at_ms, by }> }`，几个函数都是纯的：
  - `added`：扫描范围里没有文件就 `auto_empty`，否则 `untrusted`；
  - `rescanned`：`trusted` 的摘要变了就转 `stale`，转了之后不会自动回 `trusted`；
  - `confirmed`：用户确认的摘要与这次扫描不一致时返回 `DigestMismatch`；
  - `diff_from_trusted`：只在 `stale` 时给出差异。

## 给其他任务的接入点

- **T096（findings）**：
  - 遍历 `Scan.files`，按 `path` 判断文件种类，从 `content` 解析；
  - `Finding.path` 用 `ScannedFile.path`；`target` 在 `linked` 时取 `ScannedFile.target`；`outside_workspace` 直接取；
  - findings 可以挂在 `Scan` 上，也可以另给一个 `findings(&Scan)` 函数，两种都不影响摘要。
- **T034（`project/add`、`project/trust`、`project/trust_info`）**：
  - 扫描放进阻塞线程池；`ScanError` 对应 `unavailable/scan_failed`，`ScanError::code()` 给确认框说明原因；
  - 添加：`Trust::added(&scan, now)`；信任：`Trust::confirmed(&rescan, &params.trust_digest, principal, now)`，`DigestMismatch` 对应 `conflict/digest_mismatch`；
  - 写库：`trust_state`、`trust_digest`、`trust_manifest`（`Manifest::to_json`）、`trusted_at`、`trusted_by`（`TrustedBy::as_str`）；读回清单用 `Manifest::from_json`；
  - `trust_info.diff_from_trusted` 用 `Trust::diff_from_trusted(&rescan)`；
  - findings 先回空列表，等 T096。
- **T044（门控）**：
  - 每次 Attach、每次 prompt 之前、每个回合结束时调用 `scan`，用 `Trust::rescanned` 得到新状态；从 `trusted` 转为 `stale` 时，暂停队列；
  - git 根目录由 T060 取得后传入，不是 git 仓库时传 `None`。

## 范围外的发现

- **目录链接组成的 DAG 可能让扫描很慢**：上限只数文件（裁定第 5 条）。如果一个项目里的目录链接互相指向，且不构成环（例如每层两个链接都指向下一层，共 30 层），遍历次数会按指数增长；这些目录里没有文件时，文件上限拦不住。只有用户添加了这样一个恶意项目才会发生，后果是占住一个阻塞线程。是否另加一个目录访问次数的上限，请编排者定。
