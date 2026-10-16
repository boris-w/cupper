# SP-2 许可证清单

- 日期：2026-10-16
- 范围：两个适配器的完整依赖树（含可选依赖），以及 Node 运行时来源方案里涉及的运行时和构建工具。
- 方法：用 `npm install --ignore-scripts` 把完整依赖树装到临时目录，再用 `spikes/SP-2-adapter-distribution/licenses.mjs` 逐个读取 `package.json` 的 `license` 字段和包里的 LICENSE/NOTICE 文件。逐包明细见 `data/licenses-*.json`。
- 本文不是法律意见。标为"需法务确认"的结论，要由人来确认。

## 1. 总表

| 组件 | 版本 | 许可证 | 能否随 App 或由 cupper 的服务器再分发 | 说明 |
|---|---|---|---|---|
| `@agentclientprotocol/claude-agent-acp` | 0.91.0 | Apache-2.0 | 可以，但没有用：离开 SDK 无法运行 | 只带自己的 LICENSE |
| **`@anthropic-ai/claude-agent-sdk`** | 0.3.300 | `SEE LICENSE IN README.md`；LICENSE.md 原文："© Anthropic PBC. All rights reserved. Use is subject to the Legal Agreements outlined here: https://code.claude.com/docs/en/legal-and-compliance." | **不能** | 包里没有任何开源授权，使用受 Anthropic 条款约束。`sdk.mjs` 还内联了它自己的依赖 |
| **`@anthropic-ai/claude-agent-sdk-darwin-arm64`**（可选依赖，原生 Claude Code 2.1.300，249.6 MB） | 0.3.300 | `SEE LICENSE IN LICENSE.md`，原文同上 | **不能** | 法律页面写明：在产品中预装 Claude Code 需要接受 Commercial Terms，且不得修改二进制、不得移除内置认证方式。PRD 已决定不再分发 |
| 其余 claude-agent-acp 依赖（105 个） | — | MIT 91、ISC 7、BSD-3-Clause 3、Apache-2.0 2（`@agentclientprotocol/sdk`、`@opentelemetry/api`）、BSD-2-Clause 1、Unlicense 1 | 可以，需附带许可证文本和版权声明 | 没有任何包带 NOTICE 文件。`standardwebhooks`（MIT）的包里没有许可证文件 |
| `@agentclientprotocol/codex-acp` | 2.2.2 | Apache-2.0（核对原文后确认，见第 2 节） | 可以，需附带内联依赖的声明 | 发布的 `dist/index.js` 是单文件，内联了 22 个包，但没有保留它们的许可证注释 |
| codex-acp 的其余依赖（22 个，全部内联进 `dist/index.js`） | — | MIT 18、ISC 2（isexe、which）、BSD-3-Clause 1（diff）、Apache-2.0 1（`@agentclientprotocol/sdk`） | 可以，需附带声明 | |
| `@openai/codex` + `@openai/codex-darwin-arm64`（codex-acp 的依赖，锁定 0.160.1，原生 333 MB） | 0.160.1 | Apache-2.0（npm 包里没有 LICENSE 文件） | 许可证上可以；但 AG-1 要求用本机 CLI，所以不需要 | 只在没有设置 `CODEX_PATH` 时才会用到 |
| Node.js `bin/node` | 24.22.0 | MIT，另含随附第三方组件：OpenSSL（Apache-2.0）、ICU（Unicode）、V8（BSD-3）、libuv、zlib、nghttp2/3、ngtcp2、llhttp、undici、simdjson、ada、brotli、zstd、LIEF（Apache-2.0）等 | 可以，需附带 Node 的 LICENSE 全文（2946 行） | LICENSE 里出现的 GPL 文字只涉及 ICU 的构建脚本（带 Autoconf 例外），不进入二进制 |
| bun | 1.4.3 | Bun 本身是 MIT；**静态链接 JavaScriptCore/WebKit（LGPL-2）和 tinycc（LGPL-2.1）** | 能再分发，但要承担 LGPL 义务 | Bun 的 LICENSE.md 原文："If you statically link against an LGPL'd library, you must also provide your application in an object (not necessarily source) format, so that a user has the opportunity to modify the library and relink the application." |
| esbuild / esbuild-wasm | 0.28.2 | MIT | 可以 | 在 CI 中做构建工具；首次运行时在本机打包则用 esbuild-wasm |
| postject | 1.0.0-alpha.6 | MIT | 只在构建 SEA 时使用，不进入产物 | |

## 2. codex-acp 的 "Other" 是怎么回事

- npm 包里的 `LICENSE` 和 GitHub 仓库 `HEAD` 的 `LICENSE` 逐字节相同。
- 文件开头 13 行是 Apache 官方推荐的声明样板，写着 "Copyright 2025 JetBrains s.r.o. Licensed under the Apache License, Version 2.0 ..."。后面是 Apache-2.0 全文的第 1 至 9 节和 "END OF TERMS AND CONDITIONS"，与 apache.org 的标准文本逐行一致（忽略空白），只是删掉了末尾的 APPENDIX。
- 结论：许可证就是 **Apache-2.0**，条款没有改动。GitHub 显示 "Other"，**推测**是因为文件开头多了版权样板、末尾少了 APPENDIX，导致它的自动识别工具（licensee）相似度不够。
- `package.json` 的 `license` 字段也是 `Apache-2.0`。

## 3. 打成单个二进制（`bun --compile` 或 SEA）算不算再分发

**算。** 产物里有两部分：
- 一份完整的运行时，也就是 bun 或 node 二进制；
- 每一个被打包的 JS 包的副本。打包会改动代码形态，但内容仍然是原包的副本。

把这个文件放进 .app、放到 cupper 的下载服务器、或者通过自动更新下发，都等于再分发里面的全部组件，每个组件的许可证义务都要满足。由此得出：

1. **claude-agent-acp 的任何打包产物都不能由 cupper 分发**，包括 esbuild 单文件、SEA 和 bun 二进制，因为里面有 `@anthropic-ai/claude-agent-sdk` 的代码，而 SDK 没有再分发授权。
2. 同样的打包在用户机器上完成时，原料是用户机器从 npm（Anthropic 自己的发布渠道）下载的包，cupper 没有向任何人提供这些代码的副本，**推测**不构成 cupper 的再分发。`install-runtime.mjs` 的本机 esbuild-wasm 打包按这个模型设计。**需法务确认**："由 cupper 触发、在用户机器上自动下载 SDK"是否属于用户自己的使用。
3. codex-acp 的依赖全是宽松许可证，打包产物可以分发，但要自己生成 THIRD_PARTY_NOTICES，因为上游单文件丢掉了内联依赖的声明。
4. bun 编译产物会额外带上 LGPL 义务。SEA 只带 Node 的 MIT 和第三方声明，Node 的 LICENSE 文件已经把这些声明汇总好了。

## 4. 对 ADR-11 的建议

1. **红线**：cupper 的仓库、.app、发布页、自动更新源、镜像、CI 缓存产物里，都不能出现 `@anthropic-ai/claude-agent-sdk*`、Claude Code 二进制，或任何包含它们的打包产物。CI 加一道检查：如果打包输入里出现 `@anthropic-ai/claude-agent-sdk`，构建直接失败。
2. **claude-agent-acp 走"用户机器从 npm 安装"**：App 里只放锁文件，也就是包名、版本、URL 和 SRI 这些元数据。首次运行时按锁文件从 registry.npmjs.org 下载并校验，然后在本机打包。
3. **可以随 App 分发的**（如果 ADR-08 选择这样做）：
   - 重签后的 Node 二进制，附 Node LICENSE；
   - codex-acp 单文件，附 Apache-2.0 和 22 个内联依赖的声明；
   - esbuild-wasm（MIT）。
4. **不建议分发 bun 编译的二进制**，除非愿意履行 LGPL 义务：提供 JavaScriptCore/WebKit 和 tinycc 的源码指引，并保证可以重新链接。
5. **cupper 自身的许可证**：所有将要分发的依赖都是宽松许可证（MIT、ISC、BSD、Apache-2.0），MIT 和 Apache-2.0 都兼容。建议用 Apache-2.0，与 ACP 生态（两个适配器、ACP SDK）一致，而且带专利授权。最终由 ADR-11 决定。
6. **CI 许可证门禁**：对所有进入 App 的锁文件跑清点，白名单是 MIT、ISC、BSD-2/3、Apache-2.0、0BSD、Unlicense、CC0、BlueOak；出现其他许可证就失败。THIRD_PARTY_NOTICES 由锁文件自动生成。对 npm 包里缺少 LICENSE 文件的（例如 `standardwebhooks`、`@openai/codex`），从上游仓库补全文本。
7. **范围外，提醒一下**：法律页面写明，使用 Agent SDK 的开发者应当用 API key 认证，并且不允许第三方产品代用户走 Free、Pro、Max 订阅凭据；同时又写明，这"不妨碍终端用户用自己的订阅登录**未修改的** Claude Code 二进制"。cupper 驱动的是用户自己登录的本机 `claude`，具体落在哪一边属于政策问题（R02 核心结论 6），不是许可证问题，需要单独确认。
