# ADR-11 开源许可证与第三方依赖的再分发

- 状态：提议（cupper 自身的许可证需要用户确认）
- 日期：2026-10-16
- 来源：SP-2 许可证清点（`docs/tasks/SP-2-adapter-distribution/licenses.md`）、R02、PRD 第 8 节

## 背景

cupper 是开源项目（PRD 定位）。依赖里有三类许可证问题需要处理：

- **Anthropic 的组件**：`@anthropic-ai/claude-agent-sdk` 和它自带的原生 Claude Code，标注为"All rights reserved"，使用受 Anthropic 商业条款约束。
- **宽松许可证的依赖**：两个适配器的其余依赖，共 129 个包，都是宽松许可证（MIT、ISC、BSD、Apache-2.0、Unlicense）。
- **codex-acp**：GitHub 上显示许可证是 "Other"。核对原文后确认就是 Apache-2.0，只是开头换成了 JetBrains 的版权声明、删掉了附录。它的单文件里内联了 22 个包，但没有保留这些包的许可证声明。

## 决策

**1. cupper 自身采用 Apache-2.0**（待用户确认）。理由：和 ACP 生态一致，并且带专利授权条款。

**2. 红线**

以下内容不得出现在仓库、App 包、发布页、自动更新源或任何镜像里：
- `@anthropic-ai/claude-agent-sdk` 的任何版本；
- Claude Code 的二进制；
- 任何包含以上两者的打包产物，包括 SEA、bun 编译产物和 esbuild bundle。

CI 在打包输入里发现 `@anthropic-ai/claude-agent-sdk` 时直接失败。claude-agent-acp 一律在用户机器上从 npm 安装并打包（见 ADR-08），App 里只放锁文件这类元数据。

**3. 可以随 App 分发的组件**

| 组件 | 附带的许可证文件 |
|---|---|
| 重新签名后的 Node 二进制 | Node 的 LICENSE 全文 |
| codex-acp 单文件 | Apache-2.0，加上它内联的 22 个依赖的许可证声明（由锁文件自动生成） |
| esbuild-wasm | MIT |
| Electron 和前端依赖 | 由 ADR-03 的打包流程生成第三方声明 |

不分发 bun 编译产物：它静态链接了 LGPL 组件，会带来额外的许可证义务。

**4. CI 许可证门禁**
- 白名单：MIT、ISC、BSD-2/3、Apache-2.0、0BSD、Unlicense、CC0、BlueOak。
- 不在白名单里的许可证必须人工审核。
- npm 包里缺少 LICENSE 文件的，从上游仓库补全文本。
- 第三方声明文件（THIRD_PARTY_NOTICES）由 CI 自动生成，随 App 一起分发。

**5. 需要法务确认的事项（在大范围公开分发之前）**
- "由 cupper 触发、在用户机器上自动下载 Anthropic 的 SDK"，是否属于用户自己的使用。
- Agent SDK 和"用户用自己的订阅登录"之间的政策关系。PRD 第 8 节已经列出这一项；Anthropic 法律页面上写着"不妨碍终端用户用自己的订阅登录未修改的 Claude Code 二进制"。

## 后果

- **好处**：开源分发时没有许可证风险；第三方声明可以自动生成。
- **代价**：claude-agent-acp 要在用户机器上安装和打包（见 ADR-08）；需要维护一个许可证门禁。
