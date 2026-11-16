# T266 评审

- 评审人：编排者
- 日期：2026-11-16
- 结论：**通过，合并**（PR #127）

## 核对

- **规模**：代码 +375。开工前估 285，停止线 350、硬线 430；模块 2 写完时推算会到 355–380，执行者停下汇报，我把停止线提到 400。各模块估算与实际见汇报。
- **CI**：PR #127 https://github.com/boris-w/cupper/pull/127 ，b327e9ed 上 8 个检查全过（run 43851767086）。
- **两次合并 main**（由我做，执行者核对）：
  - T265 之后：`Shell.tsx` 一处冲突，TitleBar 开头两边的变量都保留。
  - T268 之后：`composer/config.tsx` 的 `useCached` 注释合成一段；`new-session.test.tsx` 的 import、`setup` 返回值（`shell` 与 `event`）和两边的用例都保留。
  - 生成物自动合并。合并后 `pnpm gen` 无差异，protocol 193 项、fmt、clippy、宿主 lib 517 项、ui-core 199 项、`tsc -b` 都通过。
- **截图基线**：ui-baseline 43846912651，83 张里 10 张变化，逐张说明了原因。

## 逐项意见

1. **`DefaultPermission` 可为 null**：用 `Option<String>` 时 schema 是"可省略"，宿主发出的 null 会被 zod 拒收、无法清除；照 `CliPath` 写，理由成立。
2. **设置页只读缓存**：打开设置页不拉起适配器和用户的 MCP，测试断言了没有 `agents/refresh`。
3. **失效的默认权限不发出**：不会把失效值当成 CLI 当前值显式发给 agent。
4. **Q45 一处判断**：新建会话页、输入框和 `@` 列表共用 `useSessionState().newProject`。
5. **越线前停下汇报**，按模块提交、逐个跑 size。

## 记下的点

- 交接事项：设置行在"有缓存但没有权限模式"时的提示文案（给 T154）；会话标题在顶栏、侧边栏都没有经 `escapeInvisible`（给 T267）。
