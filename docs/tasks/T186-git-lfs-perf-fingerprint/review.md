# T186 评审

- 评审人：编排者
- 日期：2026-10-26
- 结论：**通过，合并**

## 核对

- **规模**：`cargo xtask size` 报 +287 −30，在停止线 320 以内；每次提交后都统计了。
- **CI**：执行者的摘要 `pr-b216537cbc39.json` 通过（rust 2127，含新增的 `git-lfs` 步骤；protocol 185、ts 473、license、testdata-scan、e2e 30）。之后 main 只进了 T078（桌面端与 e2e），编排者合进分支后编译检查通过，直接合并。
- **变异检查**：宿主不带 git-lfs 时，LFS 用例失败、`a.bin` 变成占位，证明用例确实依赖白名单配置里换成的绝对路径。

## 逐项意见

1. **`cargo xtask git-lfs`**：锁定 3.8.0，按架构选 zip，两个 sha-256 取自同一 release 的 `sha256sums.asc`；下载复用 `runtime-pack` 的 `fetch`（curl `-q`、只允许 https、占位 UA），官方地址加进白名单；先核对哈希再只解出程序本身。哈希不符的 zip 被拒绝、不解压，有单元测试。不核对 PGP 签名，pin 写死在代码里，与 SP-5 的做法相同，可以接受。
2. **IT-git-lfs-snapshot（Q32 的 LFS 部分）**：`git lfs install --local` 写入标准取值（含 `required = true`），快照档的 PATH 里没有 git-lfs，只能靠绝对路径，结果与用户的 git 写出的 tree 相同。编排者据此把 Q32 的 LFS 部分标为确认。
3. **回合前后的配置指纹**：随快照用 T097 的 `Fingerprint::take` 计算（在 `spawn_blocking` 里），前后不同时在同一批事件里、先于 `snapshot.recorded` 和 `turn.completed` 写 `notice{repo_config_changed, params: {paths}}`；下一个回合没有改动就不再提示。`paths` 用绝对路径，与仓库档的 `RepoNotice` 一致。
4. **PERF-snapshot**：1 万文件的合成仓库，没有改动约 112 ms、改 10 个文件约 115 ms（热缓存）；上限设得宽（2 s），只防严重退化。

## 记下的点

- **PERF-snapshot 还没接进 `perf` 作业**（`run: None`）：T117 实现时用 `cargo test -p cupper-host --lib -- --ignored perf_snapshot` 跑，把打印的中位数写进历史数据。记交接。
- **指纹每次快照遍历整个工作区找 `.gitattributes`**（含 `node_modules` 这类被忽略的大目录），10 万文件的仓库可能明显变慢；超预算时改为只看已跟踪和未忽略的 `.gitattributes`。记交接给 T117。
- **只锁定了 macOS 的 git-lfs**：cupper 只支持 macOS，其他系统报"no git-lfs release is pinned"。
- **界面侧**：`repo_config_changed` 的醒目提示与用户档的确认文案归 M1b。
