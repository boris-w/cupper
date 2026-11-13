# T145 汇报

- 执行者：子 agent（前端工程师，ui-core 与桌面）
- 分支：`feat/T145-composer-images-slash`
- 完成日期：2026-11-10
- PR：https://github.com/boris-w/cupper/pull/84
- PR 上的 CI（8d575712，8 个检查全过）：https://github.com/boris-w/cupper/actions/runs/42742951345

## 做了什么

开工前估约 700 行，超过硬线，按方案拆分：本任务只做图片；斜杠命令、↑ 恢复和发送快捷键另立 T244（计划 v1.3.88）。停止线随之改为 350，硬线 420。

1. **图片附件**：新扩展 `packages/ui-core/src/composer/extensions/image.tsx`。
   - 三个入口：
     - 底栏最左边的"+"（`order: 1`），点开一个隐藏的 `input type=file`，`accept` 限 png、jpeg、gif、webp，可以多选；
     - 文字区里粘贴：取剪贴板里 `image/*` 的文件，类型不在四种之内的给提示；
     - 输入框上拖入：只取这四种类型的图片；其他文件，包括别的图片类型，仍按 T142 的规则生成文件引用（`fileref.tsx` 改为跳过图片）。
   - 拖入时显示覆盖层"拖放到这里添加"。
   - 检查：
     - 类型限这四种；
     - 会话能力的 `prompt.image` 为 false 时提示"这个 agent 不接受图片"；
     - 超过 `prompt.max_image_bytes` 时提示"图片不能超过 … MB"；
     - 上传失败时提示，带语义码。
   - 上传：`putBlob(bytes, type)` 之后生成 `{type: "image", blob}` 附件，随下一条消息发出。
   - 文案全部走 i18n，字符都在截图字体的子集里。

2. **缩略图**（`packages/ui-core/src/composer/thumbnails.ts`）
   - 按 blob 的 hash 记下本地 File 的 object URL，附件条的图片附件显示缩略图；没有 URL 时退回"图片"二字。
   - 缩略图只用本地 File 的 object URL，不从别处加载任何东西。
   - 释放：附件变化时，不在附件里的 hash 释放；图片扩展卸载时全部释放。组件测试核对了 `URL.revokeObjectURL` 的调用。

3. **拖放覆盖层与输入框的定位**
   - 覆盖层要定位在输入框上。最初给输入框容器加了常驻的 `relative`，第一轮 ui-baseline 显示：输入框的阴影因此盖到了紧随其后的动作条、计划药丸上，出现大片 ≤8 级的色差。
   - 修正（`9c38f3f0`）：输入框容器平时不定位，只有文件拖到上面时才 `relative`：用 Tailwind 的 `data-dragging:relative`，图片扩展在拖入时设上 `data-dragging`，离开或放下时去掉。平时的画面和原来一样，只多了"+"。

4. **用例**
   - 组件测试（`composer.test.tsx` 的 "images"）：
     - 粘贴和"+"上传；附件和缩略图；移除附件时、卸载时都释放 URL；
     - 类型、大小、agent 不接受图片、上传失败，各自的提示；
     - 拖入时图片和文件分开，覆盖层出现时有 `data-dragging`，放下后消失。
   - **E2E-image-paste**（`tools/e2e/src/composer.spec.ts`，nightly）：
     - 合成的 `ClipboardEvent("paste")`，`clipboardData` 里放一个 PNG；
     - 拖入一个 PNG 和一个文本文件，文本文件仍是文件引用；
     - 用 `setInputFiles` 选"+"的输入框；
     - 断言附件条有 3 张缩略图，下一条消息带 3 个 `image` 块。
     - 不碰系统剪贴板，不需要焦点，按后台模式跑；E2E-file-ref 共用拖入的 helper。
     - 按你的决定，不另写真实 ⌘V 的 `@focus` 用例，真实粘贴留给 M1b 退出时按 MAN-S 检查表核对。

## 关键取舍

- **拆分**：图片和 T142 的拖入关系最近，放在先做的这个任务里；斜杠命令、↑ 恢复、发送快捷键都是文字区的按键，归 T244。
- **新会话页不检查能力**：新会话页取能力要带上 agent，这归 T244（扩展点要带上 agent）。所以新会话页上只查类型，大小和"agent 不接受图片"由宿主在发送时把关（`content.rs` 的映射会拒绝）。
- **缩略图的释放时机**："卸载时释放"意味着请求面板占据输入框位置、Composer 卸载时，缩略图会退回文字；附件本身不受影响。
- **覆盖层只在拖入时定位**：避免常驻定位改变阴影的绘制顺序，见上文第 3 项。

## 今天的 merge main 与基线比对

- **merge origin/main（0008c26a，含 T238、T245、T246、T247、T249）**：合并提交 `2ca56f14`。冲突只有 5 张共用基线：review-commit-dark、review-commit-light、review-commit-failed、stream-changes-review、stream-changes-undo。都先取 main 的版本，没有代码冲突。之前还有两次 merge main（`390aabf2` 拿到改过的任务书，`a81d05c4` 在 T131 合并之后）。
- **基线比对**（ui-baseline 42742007006，跑在 2ca56f14 上）：
  - 下载到新的空目录，逐字节比对：只变了这 5 张，每张和 main 的版本相比正好差"+"那一块（84 像素）。其余基线和 meta.json 都没变。
  - 逐张看过重叠的几张：
    - `stream-changes-undo`：保留 T245 停在底部的画面，只多了"+"；
    - `stream-changes-review`：保留 T238 的按钮，`turn` 范围只有还原；
    - review-commit 三张：保留 T238 的按钮，未暂存有暂存和还原。
  - 已提交（`8d575712`）。
- **之前几轮基线**：
  - ui-baseline 41814476367：发现阴影的副作用，没有提交，修正后重跑；
  - ui-baseline 41816884610（9c38f3f0）：40 张只多了"+"，另有零星 1/255 的抖动（你已接受），已提交（`0209db4b`）；
  - ui-baseline 42591273670（a81d05c4）：T132 的 4 张 `stream-changes-*` 和 review-commit 三张加上"+"，已提交（`68d9f660`）。

## 验证方式

本机，都是单份：
- tsc 通过（ui-core、desktop、ui-screenshots、e2e、shell-api）；
- ui-core 的 vitest 128 个通过（merge 之后）；
- 全仓 eslint、prettier 通过；`pnpm install --frozen-lockfile` 通过；
- E2E-image-paste 和 E2E-file-ref：本机构建 e2e 版本后单份跑过，都通过。按后台模式运行，不碰剪贴板。
- 截图工具：用临时配置在无头 Chromium 里跑过全部用例，功能都通过。

runner 上：
- e2e-repeat，E2E-image-paste × 10，全过：https://github.com/boris-w/cupper/actions/runs/41814469664 。这次跑在 7edc9204 上；之后的 9c38f3f0 只改了拖入时输入框的定位，不影响这个用例。
- ui-baseline：41814476367（没有提交）、41816884610、42591273670、42742007006，见上文。
- PR #84 在 8d575712 上 8 个检查全过：https://github.com/boris-w/cupper/actions/runs/42742951345

对照验收标准：

| 验收项 | 结果 |
|---|---|
| E2E-image-paste 通过，并在 runner 上连跑 10 次 | 通过（41814469664） |
| 不拼 HTML，文案走 i18n | 是 |
| 缩略图只用本地 File 的 object URL，并在移除附件时释放 | 是，移除附件和卸载时都释放，组件测试核对 |
| ShellAPI 只做附加 | 本任务没有改 ShellAPI |
| PR 上 8 个检查全过 | 是（42742951345） |
| 基线只由 ui-baseline 生成 | 是 |

## 规模

- 相对 main：+374 −31（不含文档、基线、lock 文件）。比停止线 350 多 24，在硬线 420 以内；多出的是测试，以及阴影修正和它的测试。三次 merge 不计。

## 提交列表

```
8d575712 test(ui-screenshots): the "+" in the shots T238 and T245 changed
2ca56f14 Merge remote-tracking branch 'origin/main' into feat/T145-composer-images-slash
68d9f660 test(ui-screenshots): the "+" in T132's and T131's shots
a81d05c4 Merge branch 'main' into feat/T145-composer-images-slash
0209db4b test(ui-screenshots): baselines with the composer's "+"
9c38f3f0 fix(composer): position the composer only while files are over it
7edc9204 docs(T145): notes on the images
bb45770b test(e2e): E2E-image-paste
fe198d62 feat(composer): images by paste, drop and "+"
390aabf2 Merge branch 'main' into feat/T145-composer-images-slash
f6803a01 docs(T145): the estimate before work starts, with a split
```

## 遗留问题 / 范围外发现

- **T244**：斜杠命令、↑ 恢复、发送快捷键。新会话页取能力需要扩展点带上 agent；做完之后，图片在新会话页也可以查 `prompt.image` 和大小。
- **真实 ⌘V 粘贴**：留给作者在 M1b 退出时按 MAN-S 检查表核对。
- **缩略图在 Composer 卸载后退回文字**：例如请求面板占据输入框位置时。如果要保留，可以把 URL 的生命周期挂到附件所在的 UI store 上，并在附件被移除或会话被丢弃时释放。
- 第一轮基线发现的阴影绘制顺序问题，说明"常驻定位"会影响相邻元素的画面。以后给输入框加定位类的样式，要先跑一轮 ui-baseline 比对。
