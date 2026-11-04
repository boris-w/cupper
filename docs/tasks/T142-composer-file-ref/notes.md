# T142 notes

## 开工前的估算（2026-11-04）

| 模块 | 实现 | 测试 |
|---|---|---|
| A. ShellAPI 附加 `pathForFile`：接口、preload（`webUtils.getPathForFile`）、浏览器 mock（按文件名给路径，测试控制） | 25 | — |
| B. 输入框扩展点补一个 `area`（文字区），让浮层扩展拿到光标和按键 | 5 | — |
| C. `composer/extensions/fileref.tsx`：识别光标前的 `@` 词，`files/search`（会话按 `session_id`，新会话页按 `project_id`），弹出层，按 UTF-16 区间切片高亮，↑↓、Enter 或 Tab 插入、Esc 关闭；空结果、搜索失败、非 git 目录的提示；插入后去掉 `@` 词，生成 `file_ref` 附件 | 130 | — |
| D. 拖入文件：经 `pathForFile` 取路径，换成相对工作区根的路径（会话取 `session.configured.workspace.path`，新会话页取项目路径），越出工作区的给提示、不生成引用 | 40 | — |
| E. 文案 | 8 | — |
| F. 用例：组件测试（弹出层、键盘、插入、拖入、越界）；E2E-file-ref（`@` 搜索、键盘、插入；拖入用 `setInputFiles` 取得有真实路径的 File，再派发合成的 drop 事件，不需要真实输入）；截图工具里弹出层一张 | — | 160 |
| 合计 | ~210 | ~160 |

合计约 370，按以往偏差上浮后约 440，接近停止线 450，做到一半再核一次。

## 实现记录

