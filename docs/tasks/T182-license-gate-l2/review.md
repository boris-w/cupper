# T182 评审

- 评审人：编排者
- 日期：2026-11-16
- 结论：**通过，合并**（PR #120）

## 核对

- **规模**：代码约 848 行（数据约 63 行不计）。开工前估约 570，我按"数据不计"批了停止线 550、硬线 650；写完是 842，越过硬线后才停下。执行者没有按守则第 8 条每写完一个模块统计一次，整份放在一个提交里，自己已认。功能和用例都已写完并通过，拆开要返工，我把硬线提到 850、不拆。
- **CI**：PR #120 https://github.com/boris-w/cupper/pull/120 ，6e3960b3 上 8 个检查全过（run 43810009677），`license` 里 `notices ok`。
- **nightly**：分支上 `job=release-config`（43810008408）10 passed，扫描构建出的 `Cupper.app` 输出 `license-gate: no problem`。
- **指纹清单**：在 runner 上生成（spike-run 43809511708），我看过 `fingerprints.json`，只有版本、integrity 和 15 个文件的 SRI，没有 SDK 的任何内容。

## 逐项意见

1. **ADR-11 守住了**：SDK 只在 runner 的临时目录里下载、算完即删；仓库里只有哈希和特征串。Claude Code 的签名值用 `codesign -dv` 从本机读签名元数据，没有运行它。
2. **特征串选得谨慎**：排除了会命中我们自己宿主的环境变量名和 `@anthropic-ai/claude-code`；生成命令要求每个串都在 SDK 里出现。
3. **负面样例齐全**：特征串、文件和 tarball 哈希、伪造 identifier、team 命中、universal 切片、读不出签名、锁文件改动或挪位、zip、声明缺失或过期都被报出。
4. **声明逐字节比对**：同时能发现缺声明和声明过期。

## 记下的点

- 交接事项给 T181：DMG 扫描、流水线对 zip 运行 scan、流水线装 cargo-about。
- SDK 升级后 PR 上的 `check` 会失败，维护者在 runner 上重新生成指纹清单（参数见汇报）。
- 设计 5.1.6 的 `license`、`release-config` 两行已补上 T182 的落点（合并时由编排者补）。
- 教训：估算明显偏低时，每个模块写完就统计，越线前停下；不要等全部写完。
