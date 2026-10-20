# T135 工作笔记

- 执行者：子 agent（工程师；T077、T134、T136 的执行者接手）

## 估计

S，上限约 250 行（不含生成物）。草估：宿主与协议约 40 行，主进程约 50 行，用例约 80 行，合计约 170 行。

## 新建会话的项目路径：选 (a)

`policy/describe` 对 `session/create` 的 `details` 加 `project_path`。

理由：
- **改动小**：`describe` 本身仍是纯函数，路径和档位一样经 `Facts` 进来。会话端口的 `facts()` 对 `session/create` 本来就是异步的（要读能力缓存），这里再打开索引流的视图，按 `project_id` 取 `path`，和它对 `interaction/respond`、`set_config` 读会话流视图的写法相同，不查库。
- **一次往返**：路径和要确认的操作出自同一次描述。(b) 要另订阅一次整个索引流，快照里有全部会话和项目，比 `confirm_config` 订阅单个会话流还重。
- **宿主不认识这个项目时**，`project_path` 为 `null`，确认框不显示这一行；之后的 `session/create` 本来就会以 `not_found` 失败。

请编排者把 5.3.11 末尾"T031 实现时的裁定"里 `full_access`（来自 `session/create`）的 `details` 键改为 `{agent, project_id, project_path, mode}`。

## 信任项目的路径

- `project/trust_info` 的结果加 `path`（必填），取宿主记下的、添加时解析过的目录，与索引里 `project.upserted` 的 `path` 相同。
- `add_project` 不再用 `project/add` 结果里的扫描内容弹框，改为先取一次 `project/trust_info`：确认框里的路径一律来自宿主，不用窗口里选目录对话框给的原始路径。代价是多扫描一次，只在有项目配置时发生。

## 路径的显示

- 先截断，再转义：截断的边界不会切开 `\u{…}` 的转义。
- 不超过 120 个字符原样显示（不可见字符转义）。
- 超过时，保留开头 30 个字符，加 `…`，再接结尾。结尾从末尾倒数的那一段里第一个 `/` 开始，只显示完整的目录名：
  - 开头看得出在哪个卷或哪个用户目录下；
  - 结尾是项目所在的那一串目录和项目目录本身。
- 例：`/Volumes/Work/nested-directory/…/nested-directory/the-project`。

## 提权框

- `bundled_cli` 为假时，详情第一行是"【注意】申请方不是本 App 自带的命令行工具。"，在"宿主核实过的信息"之前；为真时没有这一行。
- 字都在截图夹具的字体子集里，T118 的 `fonts.test.ts` 通过。
