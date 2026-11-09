# MAN-keyboard：实体键盘确认（T107，Electron 45）

Electron 升了大版本（44.9.0 → 45.2.3），按 `docs/manual/README.md`，MAN-keyboard 要重做一次。

**步骤与 T099 的记录完全相同**，见 `docs/tasks/T099-m1a-exit/man-keyboard.md` 的"用哪个构建"、"构建"、"准备与启动"、"逐项"（1 至 7）、"收尾"五节。T099 记录里提到的两处缺陷，T217 已经修好，所以照"逐项"开头那段的写法做：先切到窗口、点一下空的输入框，再等请求到达。

与 T099 不同的只有两处：

- **构建**：在 T107 的分支上构建（合并前的提交，`git status` 为空）。命令照旧是 `pnpm install --frozen-lockfile`、`cargo xtask runtime-pack`、`cargo build --locked -p cupper-fake-agent`、`pnpm build:e2e`。
- **核对 Electron 版本**：在"收尾"一节，`grep -m1 '"electron"' "$R/logs/main.log"` 应得到 45.2.3。

## 结果（做完后填写）

- 日期：
- 提交：
- Electron：（取自 `main.log`）
- macOS：
- 键盘与输入法：
- 执行人：
- 判定依据：

| 项 | 结论 | 看到的现象 |
|---|---|---|
| 1 Enter 批准 | | |
| 2 Esc 拒绝（有特权选项） | | |
| 3a 数字键提交 | | |
| 3b 效果未知的编号没反应 | | |
| 4a 输入框里选词不发送 | | |
| 4b "在此输入"里选词不提交 | | |
| 4c 打拼音时来请求，选词与随后 1 秒内的 Enter 不算 | | |
| 5 打字时面板不抢焦点，Tab 后可提交 | | |
| 6 切回前台 1 秒内提示"请再按一次" | | |
| 7 特权选项的编号不直接提交 | | |

全部通过后，在 `tests/ids.toml` 末尾追加：

```toml
[[evidence]]
id = "MAN-keyboard"
date = "<日期>"
record = "docs/tasks/T107-electron-45/man-keyboard.md"
versions = { electron = "45.2.3" }
```
