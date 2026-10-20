# T135 评审

- 评审人：编排者
- 日期：2026-10-20
- 结论：**通过，合并**

## 核对

- **规模**：新增 149 行、删除 23 行（不含生成物），在约 250 的上限之内。
- **CI**：执行者的摘要 `pr-eb3b848f9f3a.json` 通过。合并 main 之后在本 worktree 重跑 `cargo xtask ci pr --record`，`pr-dfa06e8eda35.json` 通过（rust 1821、protocol 185、ts 421、license、testdata-scan、ui 11）。合并没有冲突。

## 逐项意见

1. **信任项目的确认框显示宿主给出的路径**：`ProjectTrustInfoResult.path` 必填，等于索引里 `project.upserted` 的 `path`。
   - `add_project` 也改为先取一次 `trust_info`，不用窗口选目录时给的原始路径。框里的路径一律来自宿主，代价是有项目配置时多扫描一次。这正是本任务要的："确认框里看到的路径，就是宿主将要信任的那个"。
2. **新建会话选 (a)**：`policy/describe` 的 `details` 加 `project_path`，路径经会话端口的 `facts()` 从索引视图取，`describe` 仍是纯函数。路径和要确认的操作出自同一次描述，不用为此订阅整个索引流，比 (b) 好。宿主不认识的项目给 `null`，接下来的 `session/create` 本来就会失败。
3. **提权框**：`bundled_cli` 为假时，第一行是醒目提示，排在核实信息之前。
4. **路径显示**：
   - 先截断，再转义，不会切开转义序列；
   - 超长时保留开头 30 个字符和结尾几级完整的目录名，看得出在哪个卷、哪个目录下。
5. **设计已更新**（main 上 54f67a8）：5.3.4 的 `project/trust_info` 结果加 `path`；5.3.11 的 `full_access`（来自 `session/create`）的 `details` 加 `project_path`。

## 遗留

- `bundled_cli` 为真的情况要等签名构建实测（T133 的交接，仍有效）。
