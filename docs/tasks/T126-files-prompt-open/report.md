# T126 汇报

- 执行者：子 agent（Rust 工程师，宿主文件服务与编排）
- 分支：`feat/T126-files-prompt-open`（PR https://github.com/boris-w/cupper/pull/52 ，头提交 cf2b7c3）
- 完成日期：2026-11-03

`files/search`、提示内容映射和 `workspace/open` 都做完了，三个用例本机都通过。按你的要求，没等 CI 出结果就先交汇报：交的时候 PR #52 的 8 项检查都还在运行（https://github.com/boris-w/cupper/actions/runs/41612677429 ），结果还没有。

**偏离设计**：设计 5.2.11 指定的 nucleo-matcher 是 MPL-2.0，按你的裁定换成了 MIT 的 fuzzy-matcher 0.3.7，合并时由你改设计。

## 做了什么

1. **`files/search`**（新模块 `crates/cupper-host/src/files/`）
   - **找工作区**：`session_id` 取会话的 `workspace_path`，`project_id` 取项目路径；两者恰好给一个，否则返回 INVALID_PARAMS；找不到返回 `not_found`。
   - **git 工作树**：在快照档执行 `ls-files -z --cached --others --exclude-standard`。入口是新加的 `git::snapshot::files`，复用快照的 GIT_DIR 和索引副本。
   - **其他情况走遍历**：不是 git 工作树（包括 git 仓库的子目录）、没有 git，或者快照档失败时，用 `ignore` 遍历：
     - 仓库外的 `.gitignore` 也生效，隐藏文件保留；
     - 不进入 `.git`，不跟随符号链接，只列普通文件。
   - 两条路径都最多列 20 万个文件。
   - **缓存**：按工作区缓存 5 秒，新增时长键 `file_list_ttl`，经 `Clock` 计时。会话 actor 在提交含 `turn.completed` 的那一批事件之前丢掉自己工作区的列表，所以客户端看到回合结束时，拿到的已经是新列表。会话与 `files/search` 共享这份缓存，经 `SessionDeps.lists` 传入。
   - **匹配**：SkimMatcherV2，智能大小写。
     - 排序依次按分数、路径长度、路径；
     - `limit` 默认 50、最多 100；
     - 一次回复的路径加区间不超过约 48 KB，留在单帧以内；
     - 匹配位置换算成 UTF-16 区间，连续的位置合并成一段。
2. **提示内容映射**（在 T040 已有的 `driver/runtime/content.rs` 上补齐）
   - `image`：agent 能力里 `prompt.image` 为假、或图片的实际字节数超过 `max_image_bytes` 时，这次 prompt 失败。
   - `file_ref`：只接受相对路径，每一段必须是普通名字或 `.`；`..` 和绝对路径都拒绝。
   - 失败时以 `PromptFailed`（`other`）结束，原因由 T226 的 error 条目显示出来。
   - 能力取 runtime 在 attach 时缓存的那一份；还没有缓存时不收图片。
   - `text_blob` 取回全文作为文本块，`review_comment` 排成文本（GT-2），不认识的块照旧略过。这几项是原有行为，现在由 UNIT-prompt-mapping 覆盖。
3. **`workspace/open`**
   - 内置应用清单：
     - 终端：Terminal、iTerm、Ghostty、Warp、WezTerm、kitty、Alacritty；
     - 编辑器：TextEdit、Visual Studio Code、Cursor、Zed、Sublime Text、Xcode、Nova、BBEdit；
     - `finder` 固定用 Finder。
   - `settings/update` 现在只接受清单里的应用名，`workspace/open` 打开前也再查一次。
   - 执行 `/usr/bin/open -a <应用> <工作区>`：经 `cupper-sys::spawn`（复用 `git::run::run`），参数原样传入，不经 shell，环境为空，超时为新增时长键 `app_open`（30 秒）。
   - 错误：工作区目录不在了返回 `not_found`；`open` 退出码非 0 时返回 `unavailable/app_not_found`。
4. **接线**：supervisor 和 `Routes::files` 注册两个方法；dispatch 测试的"未实现"表去掉这两项。

## 关键取舍

- **许可证**：
  - fuzzy-matcher 0.3.7（MIT）和 `ignore` 0.4.34（Unlicense OR MIT）都锁了精确版本。
  - 新增的传递依赖：thread_local、crossbeam-deque/epoch/utils、globset、walkdir、same-file、bstr、regex-automata 等。
  - `cargo deny --locked --offline check` 结果：advisories ok、bans ok、licenses ok、sources ok。
  - 需要说明的一点：本机这次没能联网更新 advisory 数据库，用的是 2026-11-02 的本机副本。这些传递依赖的已知公告都落在已修复的版本范围内，fuzzy-matcher 和 `ignore` 没有公告记录。CI 的 license 作业会先 fetch 最新数据库再检查。
- **快照档失败时也改走遍历**，不报错：搜索候选宁可多给，也不要因为仓库配置不安全之类的原因整个不可用。失败原因写一条 warn 日志。
- **怎么判断应用不在**：用 `open` 的退出码，不自己去扫 `/Applications`；`-a` 按名字交给 LaunchServices 查找。
  - 实测：环境为空时，`open` 照样能查 LaunchServices；应用名不存在时退出码为 1。
  - 打开前先确认工作区目录还在，所以退出码非 0 只剩"应用不在"这一种原因。
- **映射里不支持的块让 prompt 失败**，而不是悄悄丢掉：用户能在错误条目里看到原因。没有在 `session/send` 时提前校验，因为那属于发送路径，不在本任务范围。
- **规模**：`cargo xtask size` 为 +631 −14，开工前估计约 490。多出来的主要是文档注释，以及 rustfmt 逐行展开的应用清单和测试断言。越过停止线 600 时停下报告过；你选择不为行数压缩，停止线放宽到 650。

## 验证方式

- **IT-files-search**（`orchestrator/session/files_tests.rs`，经网关、会话夹具和 ManualClock）：
  - 普通目录：`.gitignore` 排除 `build/`，隐藏文件保留；空查询按路径长度排序；`main` 的区间是 `[[4,8]]`；路径里有 emoji 时 `é` 的区间是 `[[8,9]]`；同时给两种入参被拒绝。
  - git 工作树：已跟踪和未跟踪的文件都在，被忽略的 `*.log` 不在；列表在回合结束前保持不变，回合结束后刷新；之后再保持 5 秒，时钟前进 5 秒后刷新。
- **UNIT-prompt-mapping**（`content.rs`）：
  - 六种块逐一映射成 ACP 的 JSON，不认识的块略过；
  - 拒绝的四种情况：agent 不收图片、图片超过上限、`../x`、`/etc/hosts`。
- **IT-workspace-open**：替身 `open` 脚本只记录收到的参数，并模拟本机没有 TextEdit。
  - terminal 和 finder 成功，参数是 `-a|Terminal|<工作区>`、`-a|Finder|<工作区>`；
  - editor 返回 `unavailable app_not_found`；
  - 未知会话、工作区被删除都返回 `not_found`。
  - 没有打开任何真实应用。
- **设置**：单元用例加了一条拒绝 `apps.terminal: "Calculator"`。
- **本机**（各跑一次）：
  - `cargo test -p cupper-host --features test-harness` 全部通过。中途 lifecycle 的 `it_cli_commands…` 失败过一次，原因是本机 `target` 里的 cupper CLI 是旧版本；按 CI 的做法重建后通过，和改动无关。
  - `cargo clippy --workspace --all-targets --all-features -- -D warnings`、`cargo fmt --check`、license-gate、`trace-ids check`、prettier 都通过。
- **协议**：没有改，`schema-compat` 预计 0 breaking，等 protocol 作业确认。
- **PR CI**：运行链接 https://github.com/boris-w/cupper/actions/runs/41612677429 ，交汇报时 8 项都还在运行。

## 提交列表

```
cf2b7c3 docs(task): T126 notes
80a0dba feat(host): files/search and workspace/open
5b3b806 feat(host): map prompt content by what the agent takes
```

## 遗留问题 / 范围外发现

1. **设计需要改**：
   - 5.2.11：nucleo-matcher 改为 fuzzy-matcher（MIT，T126 裁定）；补上缓存在回合结束时失效的时点；写下快照档失败时改走遍历；列出内置应用清单。
   - 5.2.16：登记两个时长 `file_list_ttl`（5 秒）、`app_open`（30 秒）。
2. **默认编辑器 TextEdit 打不开文件夹**：`open -a TextEdit <目录>` 没有实际用处。这个默认值是 T036 定的，本任务没改。建议 T151 或 T153 改成"清单里第一个已安装的编辑器"，或者由界面引导用户选一个。
3. **`resource_link` 的 uri 没有百分号编码**：路径里有空格、`#`、`%` 时，`file://` URI 不合法。这是 T040 的原有写法，本任务没动。要不要编码，取决于两个适配器怎么解析这个 uri，建议在 COMPAT 或真机验证时确认。
4. **图片和越界引用没有在发送时提前校验**：现在是在 prompt 时失败，界面要等这个回合失败才能看到原因。如果希望在输入框里直接拦下，可以在 `session/send` 加按能力的校验，归输入框或会话的后续任务。
5. **列表只含普通文件**：遍历不列符号链接，git 路径则会列出已跟踪的符号链接，两条路径在这一点上不一致，影响很小。

notes.md（规模估计、许可证记录、取舍）已在分支上提交，位置 `docs/tasks/T126-files-prompt-open/notes.md`。
