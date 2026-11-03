# T173 工作笔记

- 执行者：子 agent（测试与文档；T160、T225 的执行者接手）

## 估计与实际

| 部分 | 估计 | 实际 |
|---|---|---|
| `docs/manual/README.md`：证据格式、trace-ids 的核对、现有 MAN 编号对照、MAN-S 的共同约定 | 110 | 96 |
| `docs/manual/man-s1.md` 至 `man-s4.md` | 4 × 50 | 29、28、27、25 |
| `tests/ids.toml`（表头说明、MAN-S1 至 MAN-S4 的 `versions`） | 10 | +7 −5 |
| trace-ids：精度 `any`、组件扩充（含测试） | 40 | +33 −9 |

文档合计约 220 行，停止线 400。

## 取舍

- **检查表与记录分开**：检查表放 `docs/manual/`，只写做法和通过条件；每次执行的结果写成执行任务卷宗里的记录文件，证据指向记录。这样检查表可以修订，旧记录不受影响。`man-keyboard.md` 把两者写在一起，作为先例保留，不改。
- **`any` 精度**：只有 Electron 的当前版本是 trace-ids 能读到的（`apps/desktop/package.json`）。CLI 由用户自己安装，适配器版本也会随兼容工作升级；如果按 `exact` 绑定，M1b 退出之后一升级，MAN-S1 至 MAN-S4 就会变成"问题"。所以这些组件只要求写上、不比对。
- **只改 MAN-S1 至 MAN-S4 的登记**：其他 MAN 编号的 `versions` 只在 README 里给出建议，由各自的产出任务登记。这样已有的证据（MAN-compat-claude、MAN-notification-spike 都没写版本）不会失效。
