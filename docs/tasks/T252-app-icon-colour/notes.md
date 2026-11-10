# T252 notes

## 开工前的估算（2026-11-10）

停止线 260，硬停止线 320，图片文件不计入。估算已按界面多出约 50% 的经验算进去。

| 模块 | 内容 | 行 |
|---|---|---|
| 共用的清单 | 见下文 | 25 |
| 主进程 `appIcon` | 见下文 | 55 |
| 生成图片 | `build/icon.sh` 顺带生成 Dock 用的 1024 PNG，和设置页用的小图标 | 12 |
| UNIT-app-icon-pref | 取值校验与回退；启动时设一次；变更时设一次；别的键和相同取值不设；选回默认时恢复包里的图标 | 45 |
| 设置 → 外观 | 见下文 | 65 |
| UI-icon-colour | 见下文 | 30 |
| 合计 | | 约 232 |

**共用的清单**：
- 放在 `@cupper/shell-api` 里（`app-icon.ts`），主进程和 ui-core 都从这里取，不写两份。
- 内容：prefs 的键 `appearance.iconColour`（照 `composer.sendKey` 的写法）、六套配色的固定清单、默认值拿铁，以及把 prefs 值归一到清单的函数。

**主进程 `appIcon`**：
- `main/app-icon.ts`：订阅 prefs，按配色名设 Dock 图标。拿铁设空图像，恢复包里的图标；其他配色从 `Resources/app-icons/<名字>.png` 读，名字取自清单，不取 prefs 里的字符串。
- `main/index.ts` 里，在建窗口之前接上。
- 四份 electron-builder 配置各加一条 `extraResources`。

**设置 → 外观**：
- `AppearanceSection.tsx`：一行"图标颜色"，六个原生 radio，每个带一个小图标。
- 分页表在"通知"之后加一行，加上文案。

**UI-icon-colour**：
- 组件用例：六个选项、当前选中、点选写入 prefs、prefs 里的值不认识时按拿铁。
- 截图规格：分页表加一行。

## 资源

- **Dock 图标**：拿铁之外的五套 1024 PNG，放在 `apps/desktop/build/app-icons/`，打进 `Resources/app-icons/`，每张约 130–150 KB。拿铁不必带，它就是包里的图标。
- **设置页的小图标**：六张 80 px 的 PNG（40 pt 的两倍），放在 `packages/ui-core/src/settings/icons/`，由 Vite 当静态资源打进渲染层，从 `app://` 加载。

## 选回默认时怎么恢复包里的图标

用 `nativeImage.createEmpty()`。Electron 把空图像交给 `NSApplication.applicationIconImage` 时，取到的 `NSImage` 是 nil，系统就用回包里的图标。

这是按 Electron 的实现推断的，本机没有实测：实测要启动 App，而启动会抢焦点。需要在自用构建上用眼睛确认一次。
