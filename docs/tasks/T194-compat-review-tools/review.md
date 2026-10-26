# T194 评审

- 评审人：编排者
- 日期：2026-10-26
- 结论：**通过，合并**

## 核对

- **规模**：+483 −15，停止线 560 以内。每写完一个模块统计了一次，逐模块的数字记在 `notes.md`，照守则第 8 条做到了。
- **CI**：执行者的摘要 `pr-65feaf1592e5.json` 一次通过：rust 2,172、protocol 188、ts 489、license 5、testdata-scan 4、ui 35、e2e 30；compat-framework 按路径跳过。
- **合 main**：main 进了 T187、T189、T190、T191，合进分支没有冲突。编排者跑了 `trace-ids check`（418 个编号，与文档一致）、两个工具的测试（rec-shape-diff 2 个、trace-ids 23 个）和 prettier，都通过。改动都在 TS 工具和 `tests/ids.toml` 里，与 main 新进的 Rust 改动不重叠，直接合并。

## 逐项意见

1. **`rec-shape-diff` 只比形态**：六类都是字符串集合；应答按 `instance`、方向和 `id` 对回它所应答的请求；文本、时间、ID 不进入形态，有用例证明。退出码区分没有差异、有差异和用法错误，评审时可以直接拿来判断。
2. **COMPAT 编号的登记是显式的**：`tests/ids.toml` 里写明剧本和 CLI，三条登记规则由 `trace-ids check` 检查，同名剧本只证明对应 CLI 的编号。
3. **只接受两种真实模式**，而不是只拦下假模式：以后加了新模式，也不会被当作真实 CLI 的结果。
4. **`not_applicable` 的口径与 T188 不同，两边都对**：
   - `compat-reports` 判断的是一份报告能不能把 CLI 版本算进已验证范围。某个剧本不适用，不说明这个版本有问题，所以不阻止计入。
   - `trace-ids --compat` 判断的是一个编号算不算完成。只有它对应的剧本真正通过才算，不适用不能代替通过。
   - 交接里"不适用按通过算"说的是前者；需要把某个 COMPAT 编号标成不适用时，照计划 1.6 在 `ids.toml` 里写 `status` 和理由。

## 记下的点

- **T192**：six-sessions、close-running、resume-procs、untrusted-project 这几个剧本还没进剧本表。T192 补上之前，对应的编号保持待完成。
- **`rec-shape-diff` 的边界**：`_meta` 里如果有以 ID 为键的映射，键路径会随 ID 变。现有录制里没有；遇到时把这类键规范成占位符。
