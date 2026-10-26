# T188 汇报

- 执行者：子 agent（Rust 工程师）
- 分支：`feat/T188-compat-tools`
- 完成日期：2026-10-26

**结论**：`cargo xtask compat-reports [--write]` 做好了，`testdata-scan` 里加了这一步，compat-framework 也进了 `ci pr`。测试按方案二压缩后，改动为 **+452 −46**，在停止线 460 以内。在干净的 worktree 里跑了 `cargo xtask ci pr --record`，全部通过，摘要已提交，工作区干净。

### 做了什么

- **`cargo xtask compat-reports [--write]`**（`crates/xtask/src/compat/reports.rs`）
  - 不带参数时做三项检查，有问题就失败并逐条列出：
    - `testdata/compat/reports/` 下的每个 `.json`（含子目录）都经 `report::parse` 严格读入，并过 `report::problems`；目录为空或不存在时通过；
    - `testdata/compat/report.schema.json` 不过期。生成 schema 的函数从 `report.rs` 的测试里挪成了公开函数；
    - 由报告算出的范围要和清单的 `cli.*.verified` 一致，不一致时提示用 `cargo xtask compat-reports --write` 重写。
  - `--write` 只改 `cli.*.verified`，`unsupported_below` 保留；只要还有别的问题，就不写清单。
  - **范围的算法**：从 ADR-08 的起始值出发（claude 2.1.200–2.1.296，codex 0.150.1–0.163.0），扩到每份计入的报告的 CLI 版本；版本按数字比较。
  - **计入条件**照裁定：
    - 真实模式；
    - 适配器版本等于清单里锁定的版本；
    - 至少有一个剧本，而且全部通过；有 `fail` 或 `inconclusive` 时整份报告不计入。
    - 真实模式的报告，CLI 必须是 claude 或 codex，版本必须是 `x.y.z`，否则判失败。两种情况合成一条提示："`<名字> <版本>` is not claude or codex at x.y.z"。
  - **剧本结果的判断**用穷尽的 `match` 写。T187 加了新的结果取值后，这里会编译不过，合并时按你说的补分支：不适用按通过算，观察不阻止计入。
  - `manifest.rs` 里"手工维护到 T064、T066"的注释，改成了由 `compat-reports --write` 生成。
- **`testdata-scan`**：在 `trace-ids` 之后加了一步 `compat-reports`，调用的是同一个 xtask 程序。
- **compat-framework 进 `ci pr`**
  - `pr` 组合作业里加了这一项。
  - 从 main 以来没有改到那五条路径时，两步都跳过，并记下原因；单独运行这个作业或在每晚运行时总是执行。
  - "有没有改到这些路径"的判断挪进了 `ci/git.rs`，和 ui 作业共用。

### 关键取舍

- **测试压缩（方案二）**：
  - 报告夹具改用 `json!` 写，再经 `report::parse` 严格读入，字段写错照样会失败；
  - 原来两个搭建工作区的用例合成一个；
  - `compat-framework` 的路径用例合成一张表，造分支的步骤写成闭包。
- **压缩后第一次跑变异检查时，"有坏报告时仍写清单"这一条没被发现**：合格的那份报告落在基线之内，写不写清单内容都一样。改成让它把上限扩到 2.1.1000 后，这条能被发现，还省掉了一份夹具。
- 报告目录会递归查找子目录，因为文件怎么布局由 T187 决定；其他扩展名的文件忽略。

### 验证方式

- **UNIT-compat-tools**（三个用例，测试名以 `unit_compat_tools_` 开头）：
  - **已验证范围**：
    - 没有报告时等于基线；
    - 两份计入的报告把范围扩到 2.1.1000 和 0.149.0，同时验证了版本按数字比较；
    - 五份不计入的报告不改变范围：假模式、旧适配器、有失败、有不确定、没有剧本；
    - 两类错误的提示；五种不合格的版本写法都被拒。
  - **报告与清单**：
    - 没有报告时通过；
    - 多余字段和语义问题各自报错，不是 `.json` 的文件被忽略；
    - 有坏报告时 `--write` 不写清单；
    - 不一致时提示该用的命令；`--write` 后只有 claude 的上限变了，再检查通过；
    - schema 过期时报错。
  - **`ci pr` 的跳过**：路径表的包含与排除；只改了宿主文件的分支，两步都跳过；再提交一个 `testdata/compat/` 下的文件，就不再跳过。
- **变异检查**：去掉"适配器版本等于锁定版本"、把 `inconclusive` 算作通过、有问题时仍写清单，这三处各让一个用例失败。
- **其他检查**：xtask 全部测试通过；clippy 在本机和 `x86_64-unknown-linux-gnu` 两个目标上都没有警告；fmt 通过。在本仓库上运行 `cargo xtask compat-reports` 也通过：还没有报告，清单等于基线。
- **ubuntu 检查**：执行了 `rustup target add x86_64-unknown-linux-gnu`，`cargo check -p xtask --all-targets --locked --target x86_64-unknown-linux-gnu` 通过。构建脚本不需要交叉编译 C 代码，没有装别的工具链，`rust-toolchain.toml` 没改。
- **记录 CI**（`cargo xtask ci pr --record docs/tasks/T188-compat-tools/ci/`，摘要 `pr-601ed0e6392c.json`，运行期间没写工作区）：

  | 作业 | 结果 |
  |---|---|
  | rust | 2171 通过 |
  | protocol | 188 通过 |
  | ts | 483 通过 |
  | license | 通过 |
  | testdata-scan | 4 通过，含新的 `compat-reports` 一步 |
  | compat-framework | 10 通过；本分支改了兼容套件的路径，所以 `test-compat` 和 `compat-fake` 都跑了 |
  | e2e | 30 通过 |

  - `ui` 作业的截图测试跳过：截图依赖的路径没改。
  - `sensitive-words` 跳过：本机没有敏感词表。

### 提交列表

- `1195419` docs(task): T188 estimate, the planned approach and a split proposal
- `fa83318` docs(task): T188 notes take the fixed compat scenario IDs
- `4c98910` docs(task): T188 estimate for the narrowed scope
- `03ee5b0` feat(xtask): compat-reports checks the reports and the verified CLI ranges
- `54c00b1` ci: compat-framework runs in ci pr when the compat suite changed
- `d2efefe` docs(task): T188 actual line count, over the hard stop
- `eee2184` test(xtask): UNIT-compat-tools on json fixtures, the setup merged
- `601ed0e` docs(task): T188 notes after compressing the tests
- `fcd6877` docs(task): T188 recorded PR CI summary

### 遗留问题·范围外发现

- **留给 T066**：设计 5.10.3 写的是已验证范围"每个适配器版本一份"，清单里却每种 CLI 只有一个范围。适配器升级后，起始基线是否还用 ADR-08 的值，由 T066 决定。
- **和 T187 合并时**：它新加的结果取值会让 `counts` 的 `match` 编译不过，需要按裁定补上分支（不适用按通过算，观察不阻止计入）。
- **流程**：第一次实现时我没有在每个模块之后统计行数（守则第 8 条），所以超了硬停止线才发现。以后每写完一个模块就统计一次。
