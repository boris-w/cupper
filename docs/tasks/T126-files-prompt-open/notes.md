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
