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

- **扩展点**：`OverlayContext` 补了一个 `area`，也就是文字区。弹出层要跟着光标、要在输入框之前收到按键，扩展只能拿到底栏的位置，拿不到文字区，所以必须补这一项。
  - 弹出层打开时，扩展在文字区上加原生 `keydown` 监听：↑↓、Enter、Tab 由它处理，并 `stopPropagation`，不会走到输入框的发送逻辑。
  - Esc 仍走输入框原有的顺序：先经 `setClose` 关浮层。
- **`@` 词**：光标前、空白或开头之后的 `@`，到光标为止。
  - 取用一个文件：去掉 `@` 词，加一个 `file_ref` 附件，光标回到 `@` 原来的位置。已经有同一路径的附件就不再加。
  - Esc 关掉以后，同一个 `@` 不再弹出；删掉重打会再弹出。
  - 搜索：会话用 `session_id`，新会话页用 `project_id`，`limit` 20。
  - 提示：正在搜索、没有匹配、搜索失败；项目不是 git 仓库时，在结果下面注明"文件按目录列出"。
- **高亮**：按 `ranges_utf16` 直接切片字符串（JS 的下标就是 UTF-16），命中的部分包在 span 里，不拼 HTML。
- **拖入**：
  - 监听 `[data-composer]` 的 `dragover` 和 `drop`。路径由 `api.pathForFile`（preload 里的 `webUtils.getPathForFile`）给出，再换成相对工作区根的路径：会话取 `session.configured.workspace.path`，新会话页取项目路径。
  - 渲染进程自己造的 File 没有路径，会被拒绝。越出工作区的给 toast 提示，不生成引用。宿主发送时还会再查一次路径必须是工作区内的相对路径（T126）。
  - 已知局限：工作区路径和拖入的路径按字符串前缀比较。同一个目录如果经符号链接或大小写不同的路径拖入，会被当成工作区外。
  - 图片也按文件引用处理；拖入图片作为图片附件的是 T145。
  - `webUtils` 只能在渲染进程（preload）里用，不在主进程。brief 里写的"主进程用 webUtils"，实际落在 preload：路径仍由 Electron 按 File 对象给出，页面拼不出。
- **E2E-file-ref**：
  - 拖入不用真实拖放：在页面里放一个 file input，用 Playwright 的 `setInputFiles` 取得带真实路径的 File，再派发一个合成的 `drop` 事件。不需要焦点和真实输入，本机后台单份跑过，通过。
- **复刻度审阅截图**：
  - 本机 e2e 构建、系统字体，放在 `screenshots/`：`file-ref-list.png` 是 `@` 列表，`file-ref-dropped.png` 是两个引用加越界提示。不进基线。
- **基线**：截图工具新增 `composer-file-ref.png`（UI-composer），只用 `ui-baseline` 生成。
