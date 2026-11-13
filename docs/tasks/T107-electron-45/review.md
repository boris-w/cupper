# T107 评审

- 评审人：编排者
- 日期：2026-11-13
- 结论：**通过，合并**（PR #94）

## 核对

- **规模**：+13 −49（含 7eff08b4 删掉的 T260 临时代码；lock、docs 不计），停止线 200 以内。
- **CI**：PR #94 https://github.com/boris-w/cupper/pull/94 ，2809d1c1 上 8 个检查全过。合并 main（3f37112c）之后、7eff08b4 上的 nightly 全集 43388524911 九个作业全过；500fc45b 上的 nightly 42606099450 也通过（codex-drift 那次失败是 0.167.0 的问题，T246 已修）。
- **MAN-keyboard**：用户在 7eff08b4 的 e2e 构建上用实体键盘做完，7 项通过，记录在本目录，证据已入库（electron 45.2.3）。

## 逐项意见

1. **只升必须升的**：electron-builder、@electron/fuses 不动；fuses 9 个、名字和出厂值与 44 相同，5.7.9 和 T161 的扫描不用改。
2. **breaking changes 逐条对照**：12 条都不影响我们，沙箱 preload 去掉的垫片我们没用（构建出的 preload 只 require electron）。
3. **Q26 复测**：探针改成不抢焦点后在 45.2.3 上跑，结果与 T077 逐字相同。
4. **T260 的临时代码删掉**：上游修复经 electron/electron#54048 进了 45.2.3（我用 `gh api compare` 核对过 tag），执行者在 45.2.3 的框架里确认 `visibilityChanged` 已不存在后，把三个文件还原到 T260 之前。
5. **MAN-keyboard 的过程**：第 1 项前两次是步骤问题（15 秒的延时被我的回复耽搁，用户点进了面板第 4 行的空输入框），改成 45 秒后通过；第 2 项用的是清面板时的 Esc，焦点在第 4 行的输入框里，用户认为已验证。记录如实写了。

## 记下的点

- 合并时由编排者更新：CLAUDE.md 的技术栈表、设计 5.1.x、5.7.8、5.10.6、02-design、13 的 Q26，以及 `docs/manual/README.md` 里 MAN-keyboard 一行（重做时用 45 秒的延时）。
- T107 合并后 T103（SP-6）可以开工。
