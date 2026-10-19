# T004 Review

## 第 1 轮（2026-10-19，编排者）

- 结论：通过，合并。
- 过程：
  - 分支开出之后，main 合进了多项任务。`.prettierignore` 冲突，两边的条目都保留；`Cargo.lock` 取 main 的版本后重新生成，新增的 8 个包（tar、flate2、miniz_oxide、zlib-rs、simd-adler32、crc32fast、adler2、filetime）与本分支锁定的版本一致。
  - 合并后在 worktree 里重跑 `cargo xtask ci pr --record`，全部通过：rust 1165、protocol 185、ts 314、license 1、testdata-scan 2，摘要是 `ci/pr-45b11cf1afa4.json`。执行者合并前那次的 `pr-69eb986122d1.json` 一并保留。
- 核对：
  - **ADR-11**：SDK 和 Claude Code 二进制没有进仓库，也没有进暂存目录。清单 `files` 里 claude 相关的只有打包脚本和锁文件；8 个含 Claude Code 二进制的 optional 平台包从不下载；SDK 只在 `target/` 下的隔离目录里用于算哈希，用完由 Drop 守卫删除。
  - **下载**：curl 带 `-q`、`--proto =https`、`--proto-redir =https`，地址限定在 `https://nodejs.org/dist/` 和 `https://registry.npmjs.org/` 两个前缀，用例覆盖了 http 和形似域名；下载后按锁定的 sha-256 或 SRI 校验，哈希不符不重试。User-Agent 是 `cupper-research`，符合守则第 7 条。
  - **解包**：只取普通文件，路径越界时整个命令失败，有用例。
  - 没有个人信息；入库的锁文件根名已改为中性名。
- 审阅：
  - **用 curl，不引入 HTTP 库**：避开 `webpki-roots` 的 CDLA 许可证，接受。
  - **Node 改用 .tar.gz**：哈希取自 `SHASUMS256.txt`，执行者手工用 release-keys 和 `gpgv` 核对过签名；同一文件里 .tar.xz 的哈希与 SP-2 一致。GPG 核对的自动化留给 T161 或 T180。
  - **ad-hoc 签名可复现**：清单记重签后的 sha-256 和 cdhash，`--verify` 重算后比对。Developer ID 签名后两者都会变，发布流水线要按发布身份重新生成清单（交给 T180）。
- 两个需要编排者裁定的事项：
  1. **禁用清单补 15 项**：同意执行者的归类。这些都是适配器或 CLI 的控制变量，会改变凭据存储、OAuth 端点、日志位置或登录方式。宿主的放行本来是白名单制，禁用清单只拦用户放行名单里的名字，多禁不会误伤正常使用。已在 main 上改设计 5.6.6，并规定宿主的放行过滤直接取清单的 `deny`。
  2. **对 SDK 动态读取的覆盖不全**：M1 接受这个局限，理由同上（白名单为主，禁用清单是纵深防御）。T066 升级适配器时再评估"按产物特征提取登记表"或"按前缀整体禁用"。已写进 5.6.6 和交接。
- 规模：代码 1,272 行（实现约 957、测试约 315），加上手工维护的清单数据约 170 行，合计约 1,440 行。计划估为 M，但任务书的停止线是约 1,500 行，没有超出，接受。
- 转交事项已写进 `handoff-notes.md`：T005、T056、T006/license-gate、T161/T180、T064/T066。
