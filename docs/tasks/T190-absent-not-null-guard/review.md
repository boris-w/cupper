# T190 评审

- 评审人：编排者
- 日期：2026-10-26
- 结论：**通过，合并**

## 核对

- **规模**：+288 −14，在裁定后的停止线 310 以内。执行者写完守卫才统计行数，发现时已越过原来的硬停止线 240（守则第 8 条）。编排者裁定不压缩，上限改为 310/330（v1.3.61）。
- **CI**：执行者的摘要 `pr-4a0bda653ae9.json` 通过：rust 2,169、protocol 188、ts 483、license、testdata-scan 3、e2e 30；ui 按路径跳过。
  - 前两次各有一个与本任务无关的宿主测试在高负载下失败，单独重跑都通过；第三次等负载降下来后全部通过。
  - 之后 main 进了 T188（只改 xtask），编排者合进分支后，全工作区编译检查和 `cupper-protocol` 的 `fixtures` 测试都通过，直接合并。
- **负面样例**：去掉 `AuthElevateResult.expires_at` 的 `skip_serializing_if`，守卫报出具体的方法、字段和定义；改回后通过。

## 逐项意见

1. **沿导出的 schema 检查**：用的是 `schema::document()`，和 `v1.json` 同源，判定规则与设计 5.3.1 一致：非必填属性上不能有 `null`，值本身可以为 `null` 的具名定义（`CliPath`）除外。
2. **方法的类型表只有一份**：原有的往返测试和新守卫共用一张表，没有复制。
3. **覆盖面统计**：用不带 `absent_not_null` 的生成器再导出一次，找出全部带 `Option` 的属性；样例里有值或者是允许的 `null` 都算覆盖到。当前是 220 个里覆盖 135 个，测试会输出数字和没覆盖到的清单，不设下限。

## 记下的点

- **T193** 补齐没覆盖到的 85 个属性，清单在 `notes.md`。最要紧的是 `ItemUpdated`、`ItemCompleted`、`ToolCall`：宿主发得最多的事件，守卫现在还防不住它们。
- **两处高负载下的偶发失败**（新增 T196）：
  - `unit_trust_scan_forking_directory_links_hit_the_entry_limit` 用墙钟上限 10 s；
  - `it_lifecycle_host_exit_and_sigterm_exit_in_order` 没等到 `host/shutdown`。
  - T099 要求连续 20 次全绿，这两处要在那之前清掉。
