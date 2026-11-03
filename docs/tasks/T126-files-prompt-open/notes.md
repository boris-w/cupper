# T126 notes

## 规模估计（开工前）

停止线 600，硬停止线 750（`cargo xtask size` 只计新增行）。

| 模块 | 实现 | 测试 |
|---|---|---|
| `files` 服务：解析工作区（`session_id`、`project_id`）、端口与路由 | 45 | — |
| 文件列表：快照档 `ls-files`（git 模块里加一个入口）、非 git 目录 `ignore` 遍历（上限 20 万）、5 秒缓存与回合结束失效 | 70 | — |
| 模糊匹配与排序、UTF-16 区间 | 40 | 20（区间换算） |
| `workspace/open`：内置应用清单、`open -a` 经 `cupper-sys::spawn`、`app_not_found`；设置校验应用名在清单里 | 55 | — |
| 提示内容映射：`prompt.image`、`max_image_bytes`、`file_ref` 不越出工作区 | 30 | — |
| 接线：supervisor、`SessionDeps`（回合结束失效）、durations、dispatch 表 | 20 | — |
| IT-files-search（git 仓库、非 git 目录、缓存与失效、两种入参） | — | 100 |
| UNIT-prompt-mapping | — | 60 |
| IT-workspace-open（替身 `open`） | — | 50 |
| 合计 | ~260 | ~230 |

预计约 490，留有余量。

## 依赖与许可证

- 设计 5.2.11 写的 nucleo-matcher 是 MPL-2.0，不在 ADR-11 白名单里。编排者裁定（2026-11-03）：不引入，改用 MIT 的 fuzzy-matcher 0.3.7，条件是 cargo-deny 的 advisories 通过；合并时由编排者改设计。
- `ignore` 0.4.34：Unlicense OR MIT。
- 本机 `cargo deny check advisories licenses bans` 通过。advisory 数据库这次没能联网更新，用的是本机缓存（2026-11-02）；新增的传递依赖（thread_local 1.1.10、crossbeam-* 0.8.8/0.9.21/0.8.23 等）都在已修复的版本范围里，fuzzy-matcher、ignore 没有记录。

## 实现记录

- 全部模块完成后 `cargo xtask size` 为 +628（估计约 490）。多出来的是文档注释、rustfmt 逐行展开的两张应用清单和测试断言。越过停止线 600 时停下报告过；编排者选择照此收尾，停止线放宽到 650，硬停止线 750 不变。

## 取舍

- **files/search 的列表**：
  - 工作区是 git 工作树（工作区本身有 `.git`）时，在快照档执行 `ls-files -z --cached --others --exclude-standard`（`git::snapshot::files`，与快照共用 GIT_DIR 和索引副本）；
  - 不是 git 工作树（含 git 仓库的子目录）、没有 git，或者快照档失败时，用 `ignore` 遍历：`.gitignore` 在仓库外也生效，隐藏文件保留，不进 `.git`，不跟随符号链接，只列普通文件；
  - 两条路径都最多 20 万个文件，非 UTF-8 的路径跳过。
- **缓存**：按工作区路径缓存，有效期 5 秒（`FILE_LIST_TTL`，经 `Clock`）。会话 actor 在提交含 `turn.completed` 的那一批事件之前丢掉自己工作区的列表，所以客户端看到回合结束时，旧列表已经不在了。本地模式下会话的工作区就是项目目录，按 `project_id` 的搜索因此也跟着失效。
- **匹配**：fuzzy-matcher 的 SkimMatcherV2（智能大小写）。先只算分数，排序后只对前 `limit` 个计算匹配下标。排序依次按分数、路径长度、路径。`limit` 默认 50、最多 100，所有路径加区间不超过约 48 KB（单帧 64,000 字节）。字符下标换算成 UTF-16 区间，连续的合并成一段。
- **提示内容映射**（在 T040 的 `content::blocks` 基础上补）：
  - agent 的能力里 `prompt.image` 为假时，图片让这次 prompt 失败；超过 `max_image_bytes` 的图片同样处理（按实际字节数算）；
  - `file_ref` 只接受相对路径，路径的每一段都必须是普通名字或 `.`，`..` 和绝对路径都拒绝；
  - 失败时 prompt 以 `PromptFailed`（`other`）结束，T226 的 error 条目带上原因；
  - 不认识类型的块照旧略过；
  - 能力取 runtime 在 attach 时缓存的那一份；还没有缓存时不收图片。
- **workspace/open**：
  - 内置应用清单（终端 7 个，编辑器 8 个，见 `settings::TERMINALS`、`EDITORS`）。`settings/update` 只接受清单里的名字；`workspace/open` 也再查一次清单。
  - 执行 `/usr/bin/open -a <应用> <工作区>`：经 `cupper-sys::spawn`，参数原样传入，环境为空。`open` 退出码非 0 时视为应用不在，返回 `unavailable/app_not_found`；运行前先确认工作区目录还在，不在返回 `not_found`。
  - 实测：环境为空时，`open` 也能找到 LaunchServices；应用不存在时退出码为 1。测试里 `open` 换成替身脚本（`FilesDeps.opener`），只记录收到的参数。
