# T145 notes

## 开工前的估算（2026-11-04，报给编排者，等回复后再写代码）

已有、可以直接用的：
- host-client 的 `putBlob`：长文本的 `text_blob` 已经在用；
- 附件条能显示 `image`（现在只显示"图片"二字）；
- T142 的 `@` 列表和拖入；
- `keys.ts` 的 `composing()`、`sends()`，以及 `SEND_KEY_PREF`：Composer 已经读这个偏好，⌘Enter 档下 Enter 换行、⌘Enter 和 ⌘⇧Enter 发送；
- fake agent 能发 `available_commands_update`（按 persona 的数据）；
- 宿主的 `agents/capabilities` 给新会话页缓存的能力。

| 块 | 实现 | 测试 |
|---|---|---|
| A. 图片 | ~150 | ~110 |
| B. 斜杠命令菜单 | ~125 | ~95 |
| C. ↑ 恢复 | ~35 | ~25 |
| D. 发送快捷键 | ~10 | ~50 |
| 合计 | ~320 | ~280 |

各块的内容：
- **A. 图片**：
  - 新扩展 `image.tsx`：底栏的"+"（隐藏的 `input type=file accept=image/*`）；在文字区上粘贴；在输入框上拖入，带覆盖层；
  - 类型限 png、jpeg、gif、webp；大小按能力的 `prompt.max_image_bytes`；`prompt.image` 为 false 时给提示；
  - 经 `putBlob` 上传，生成 `image` 附件；
  - 附件条显示缩略图：用本地 File 的 object URL，按 hash 记下；
  - T142 的拖入处理要跳过图片；
  - 用例：组件测试（粘贴、拖入、"+"、超限和类型不符的提示、图片和普通文件分开）；E2E-image-paste（合成的 paste 和 drop 事件，"+" 用 `setInputFiles`，下一条消息带 `image` 块；不碰系统剪贴板，后台模式）。
- **B. 斜杠命令菜单**：
  - 新扩展 `slash.tsx`：文字区开头的 `/词` 打开菜单；
  - 命令来源：会话取 `capabilities.commands`；新会话页查 `agents/capabilities`，需要扩展点再带上 agent；
  - 菜单按 Codex 的样式：标题、按输入过滤、最高 320 或 10 行；↑↓、Enter 或 Tab 取用（插入 `/name `），Esc 关闭；开头先判断 `composing()`；
  - 和 `@` 列表互斥：光标前只会有一种词；
  - 用例：组件测试（键盘、组字、过滤、新会话页的来源）；E2E-slash-commands（fake agent 的命令出现在菜单里，键盘取用后随消息发出）。
- **C. ↑ 恢复**：
  - 新扩展：文字区为空、没有浮层、不在组字时按 ↑，取会话里最后一条用户消息的文本；
  - 用例：组件测试（恢复、组字不触发、有内容时不触发）。
- **D. 发送快捷键**：
  - 按键行为已有。本任务把 `keys.ts` 的结构理一下，便于 T242 加插话，约 10 行；
  - 用例：E2E-send-key-setting（经 `shell.prefs.set` 改成 ⌘Enter，Enter 换行不发送、⌘Enter 发送；改回 Enter 后 Enter 发送）；组件测试补两种发送键在输入框里的行为。

合计约 600，按以往偏差上浮后约 700，超过硬停止线 600，需要拆分。

### 拆分方案

- **T145（本任务）图片**：A，约 260，上浮后约 310。
- **新任务 斜杠命令、↑ 恢复与发送快捷键**：B、C、D，约 340，上浮后约 400。三块都是按键和文字区的事，E2E-slash-commands 和 E2E-send-key-setting 也在一起。

也可以反过来：本任务做 B、C、D，图片拆出去。图片和 T142 的拖入关系更近，放在先做的这一个里更顺。
