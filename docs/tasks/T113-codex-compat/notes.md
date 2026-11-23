# T113 notes

## 开工前的估算（2026-11-23）

### 现状

- **兼容套件只支持 Claude**：
  - 运行器 `crates/cupper-host/tests/compat.rs` 里，`session/create` 的 `agent`、等运行时装好（`installed`）、杀 CLI（`KillCli` 按进程名 `claude` 找）、报告头的默认 CLI、自我约束 `refused("claude")` 都写死了 `claude`；
  - `cargo xtask compat` 没有选 agent 的参数；
  - 剧本（`compat/scenarios.rs`）只有一套 prompt、权限模式和文件，都是给 Claude 写的；
  - `gold_b.rs` 的 `session/create` 也写死 `claude`，测试表里只有 Claude 的三份。
- **本机的 Codex**：codex-cli 0.159.2，在已验证范围 0.150.1–0.163.0 之内，所以不用扩范围，也不用重写 codex-drift 的基线。
- **sanitize 的 Codex MCP 名、AIR 的 `skillPath`**：T204 已经补上（先取名字再全局替换，四处来源）。剩下的是 Claude 的 MCP 名要不要也按取名处理，以及 `words`、`scan` 把绝对路径拼到工作目录后面的毛病（T066 交接）。
- **已入库的 Codex 录制**（T102 的 spike 录的，`source` 不是 host）只能当 GOLD-A。GOLD-B-codex-s07 要用宿主重录取消剧本。

### 分块估算（代码和测试，不含录制、金样例、报告和文档）

| 块 | 内容 | 行 |
|---|---|---|
| 1. 套件运行 | 见下文 | 约 260 |
| 2. 金样例重录 | `gold_b.rs` 按录制头里的 CLI 选 agent；测试表加 Codex 的 cancel（GOLD-B-codex-s07），以及跟着入库的 normal-turn、approval；`recordings_and_goldens_match_up` 等核对可能要跟着调 | 约 40 |
| 3. sanitize 改动 | 见下文 | 约 80 |
| 4. 其余交接 | 见下文 | 约 40 |
| 合计 | | 约 420；按以往界面和工具多出 30–50% 的经验，可能到 550 |

**1. 套件运行**：
- **xtask，约 45 行**：
  - `--agent claude|codex`；
  - 配置比对加上 `~/.codex/hooks.json`（只取摘要）；
  - `--out` 接受相对路径（T202 交接）。
- **运行器，约 60 行**：
  - agent 贯穿 `create`、`installed`、`KillCli`（进程名 `codex`）、报告头、自我约束；
  - Codex 真实模式的拉起照现有写法，T203 改过、还没真跑过，这次一并验证。
- **剧本，约 130 行**：
  - 每个剧本加一份可选的 Codex 版：prompt、权限模式（`read-only`、`workspace-write`）、文件、检查；
  - 约 16 个剧本要写，包括 Q8（concurrent-two、six-sessions）、Q25（plan，看 `revise_plan` 是否结束回合）、Q34（config-scope，`.codex/` 放在上级目录和 git 根目录之外）、未信任项目、close-running（交接里要的 Codex 版"关闭时有命令正在运行"）、thinking（换成能触发思考的题）；
  - prompt 点名使用哪个工具（T102 交接：模型会绕开 apply_patch 去用用户配置的 MCP 工具）；
  - 未信任项目对 Codex 是"拒绝启动"，要加一个检查"会话建不起来，原因是 `project_untrusted`"。
- **运行器的单元测试，约 25 行**。

**3. sanitize 改动**：
- Claude 的 MCP 名：我倾向于也按取名处理，从 `mcp_servers` 数组和 "Some MCP servers are still connecting: <名字>" 这类文本里取名，再全局替换；约 30 行加测试 20 行。
- `words`、`scan` 的绝对路径：约 10 行加测试 10 行。
- 本块和 Codex 运行无关，可以单独拆出去。

**4. 其余交接**：
- `rec-shape-diff` 过滤 `toolResponse` 下随工具不同的键：约 20 行加测试 10 行。
- MAN-compat-codex 的 `[[evidence]]`：约 5 行。
- 只核对、只记录的，不改代码：
  - codex-drift 积累的影响项：imageGeneration 和音频的 `fileId`、`moderation`、错误类别映射；
  - `notices`/`compaction` 没声明时的表现、K17 的先后、录到的新选项；
  - `file://` 的百分号编码、重连把消息拆成两条。
  - 发现需要改的，另开任务。
- **不做**：长时间运行下 Codex 的 MCP 进程数和内存（T111 交接写的是"T113 或维护者手动"）。它要跑很久，不适合这次。

### 额度（真实 Codex）

- T066 跑 Claude 用了约 150 个回合，那时运行器已经支持 Claude。
- Codex 要先调 prompt：真实模型会绕路，T102 也遇到过网关过载重连，"不确定"的要重跑。
- 一次完整套件约 70–90 个回合：并发剧本 concurrent-two 2×2×3、six-sessions、resume-procs 的多次 resume 占了大头。
- 加上调 prompt 和重跑，**估 150–250 个回合**，都用低档推理；模型需要你和作者定（见问题 2）。

### 要你定的事

1. **拆不拆**：建议拆成两份。
   - **T113a**：块 1、2 的代码，块 3、4 的工具改动，用假 agent 的 `--fake` 跑通。不用额度，约 420–550 行。
   - **T113b**：真实运行、录制和金样例入库、MAN 证据、只记录的核对项，代码改动很少。
2. **真实运行用哪个模型**：Codex 走作者的网关，`--model` 用哪个便宜的模型、推理档位 low 是否可以。
3. **`~/.codex` 的读取**：套件真实运行前后会对 `~/.codex/config.toml`（我还想加上 `hooks.json`）取 SHA-256 摘要做比对，只取摘要、不读内容、不碰 `auth.json`；T066 对 Claude 也是这样做的。这算不算任务书说的"读 `~/.codex`"？不行的话就关掉这一项，改由作者用备份比对。
