# T231 notes

## 规模估计（开工前）

停止线 550，硬停止线 650。

| 模块 | 行数 |
|---|---|
| `composer/config.ts`：从会话视图读能力里的选项和当前值；普通档和 elevated 发 `session/set_config`，完全访问走 `requestPrivileged`；失败时 toast；输入框上方的档位提示 | 60 |
| `Menu` 补受控打开、分组标题、当前项的勾 | 15 |
| `extensions/permission.tsx`：图标加短名，下拉列出各模式，elevated、完全访问各带一行说明 | 70 |
| `extensions/model.tsx`："{模型} {强度} ⌄"，下拉分"选择模型""选择推理强度"两组 | 60 |
| UI store 记下要打开的菜单；错误框 `change_model` 的"换模型"按钮 | 20 |
| 文案 | 20 |
| 组件测试 | 140 |
| E2E-full-access-native | 50 |
| 合计 | ~435 |

T141 时宿主偏高 38%，按 25–30% 上浮约 545–565，贴近停止线 550；写得紧凑，每个模块做完跑一次 `cargo xtask size`。

## 实现记录
