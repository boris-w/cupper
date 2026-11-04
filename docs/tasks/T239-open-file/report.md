# T239 汇报

- 执行者：子 agent（Rust 工程师，宿主；带一处界面改动）
- 分支：`feat/T239-open-file`
- 完成日期：2026-11-04

## 做了什么

1. **协议**：`WorkspaceOpenParams` 附加可选字段 `path: Option<WirePath>`。它是工作区内的相对路径，可以是文件也可以是目录，序列化时设了 `skip_serializing_if`。另外补了一个带 `path` 的请求样例，已重新 gen。schema-compat 结果：0 breaking，`WorkspaceOpenParams.path` 记为 additive。
2. **宿主**（`crates/cupper-host/src/files/mod.rs`）：
   - 带 `path` 时，按 target 拼命令：
     - `editor`：`open -a <编辑器> <realpath>`；
     - `terminal`：`open -a <终端> <realpath>`，path 是文件时传它所在的目录。代码注释写了原因：交给终端的文件会被当成脚本执行；
     - `finder`：`open -R <realpath>`，在 Finder 里选中它。
   - 路径核对（新函数 `inside`），以下情况一律返回 `invalid_params`，不执行任何命令：
     - 空路径、绝对路径、任何一段是 `..`；
     - 工作区路径接上 path 后做 canonicalize 失败，也就是文件不存在；
     - realpath 不在工作区的 realpath 之内，指向工作区外的符号链接因此被拒。
   - 传给 `open` 的就是核对过的 realpath。
   - 其余规则沿用 T126：应用只取内置清单，经 `cupper-sys::spawn`，不经 shell，环境为空，超时 `app_open`。不带 `path` 时行为不变。
3. **界面**：审查面板文件树右键菜单的"在外部应用中打开"带上 `path: file.path`，target 仍是 editor；失败照旧按语义码提示，菜单项不禁用。T232 的截图用例 UI-review-tree 里断言请求参数的那一条也跟着改了，截图和基线都没动。
4. **用例 IT-workspace-open-path**（`orchestrator/session/files_tests.rs`）：沿用 T126 记录参数的 `open` 替身，不拉起任何应用。
   - 合法的四种：经符号链接的文件用 editor、文件用 terminal、文件用 finder、目录用 terminal。替身记录的四条命令依次是：
     ```
     -a|TextEdit|<ws>/src/a.rs
     -a|Terminal|<ws>/src
     -R|<ws>/src/a.rs
     -a|Terminal|<ws>/src
     ```
   - 不合格的六种：绝对路径、`../secret`、`src/../../secret`、指向工作区外的符号链接、不存在的文件、空路径。都返回 `invalid_params`，替身没有收到任何命令。
5. **notes.md**：写了 Codex 的做法和 cupper 的命令表。

## 关键取舍

- **怎么核对 Codex 的做法**：用户在 F2 里允许查看本机 App 包。我只读查看了本机 ChatGPT.app（即 Codex 的 Electron 外壳）的 `Contents/Resources/app.asar`：按固定字符串搜索，读命中位置前后的字节，没有解包，也没有运行它。看到的是：
  - 文件管理器：函数 `r6` 在 darwin 下返回 `["-R", path]`；
  - 终端：`/usr/bin/open -a <终端名> <目录>`，函数 `es8` 在 path 是目录时用 path，否则取 dirname；
  - VS Code 一类编辑器：调用 App 自带的 CLI，参数是 `--goto <path>[:行:列]`。
- **editor 用 `open -a`，不调编辑器 CLI 的 `--goto`**：编辑器 CLI 不在内置清单里，调用它就要执行清单之外的程序；而且我们没有行号可用。
- terminal 打开所在目录、finder 用 `open -R`，这两项和 Codex 一致。
- `path` 指向目录也接受，和 Codex 一样。

## 验证方式

- **修复前的失败**：新用例跑在 main 的代码上，在绝对路径那一步失败（`unwrap_err()` on an `Ok` value: `{}`）。旧宿主不认识 `path`，把它丢掉，直接打开了整个工作区。
- **本机检查**（单份）：
  - fmt、clippy（host 带 test-harness，加 protocol，all-targets，`-D warnings`）通过；
  - host lib 测试 518 个通过、3 个忽略；cupper-protocol 测试 100 个通过；
  - ui-core 的 vitest 113 个通过；`tsc --build packages/protocol packages/ui-core` 通过；
  - prettier、eslint 通过；
  - 截图用例 `review.spec.ts` 在无头模式下跑了一次，16 个全过，没有弹出窗口，也没有抢焦点；
  - schema-compat 0 breaking；trace-ids 通过（450 个编号）。
- **验收标准**：
  - [x] IT-workspace-open-path 通过；runner 上 10 次 × 4 份，macos-15：https://github.com/boris-w/cupper/actions/runs/41794231117 ，40 passed, 0 failed。
  - [x] 测试不在本机拉起任何应用，也不抢焦点：`open` 是记录参数的替身；截图用例在无头模式下跑。
  - [x] `schema-compat` 0 breaking。
  - [x] PR #77 https://github.com/boris-w/cupper/pull/77 的 8 个检查都通过，运行在 https://github.com/boris-w/cupper/actions/runs/41797153031 （82c395c8）。第一次 ui 作业失败是 UI-review-tree 的断言没跟着改，补提交 82c395c8 修好。
- **规模**：+110 −10 左右（`cargo xtask size` 在加截图断言之前是 +109 −9），停止线 200。

## 提交列表

```
82c395c8 test(ui): UI-review-tree expects the file's path in workspace/open
5703a1e7 test(files): IT-workspace-open-path
ca83159f feat(ui): the file tree opens the file in the editor
764b853b feat(files): open a file or directory of the workspace
402db787 feat(protocol): workspace/open takes an optional path
```

## 设计偏差 / 补充

- **5.2.11 `workspace/open`**：补上可选的 `path`，以及以下规则：
  - 三种 target 怎么打开：editor 打开这个文件或目录，terminal 打开它所在的目录，finder 用 `open -R` 选中它；
  - 路径核对：拒绝空路径、绝对路径和 `..`；canonicalize 之后必须存在，并且在工作区之内；
  - 传给 `open` 的是 realpath。
- **03 协议文档**：`workspace/open` 的参数里加上 `path`。

## 遗留问题 / 范围外发现

- 删除了的文件仍然显示"在外部应用中打开"，点了会收到 `invalid_params` 的提示。按约定没有禁用。
- 默认编辑器是 TextEdit（T126 汇报里提过）。TextEdit 能打开文件，但打开目录没有用处；如果 path 指向目录、编辑器又是 TextEdit，就会出现这种情况。
