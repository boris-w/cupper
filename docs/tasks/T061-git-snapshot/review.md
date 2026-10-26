# T061 评审

- 评审人：编排者
- 日期：2026-10-26
- 结论：**通过，合并**

## 核对

- **规模**：`cargo xtask size` 报 +1129 −52，在停止线 1,300 以内。开工前估约 1,140，按最近的实际偏差乘 1.2 约 1,370，执行者在写代码之前停下，编排者裁定拆出 T186（计划 v1.3.51）；实现比估计多约 40%，用例因借用仓库档的恶意夹具而少。中途按用户要求暂停过一次，停在注明"进行中"的提交上。
- **CI**：编排者把 main（含 T178）合进分支后，执行者跑 `ci pr --record` 通过：`pr-1dbbb4db9ffd.json`（rust 2091、protocol 185、ts 473、license、testdata-scan、e2e 30）。之后 main 进了 T093 修复 1（xtask 测试）与 T063（宿主的测试、假 agent、`launch` 的 stderr 读取与 K14 改判），与本任务改动的 `git/`、会话 actor 的快照接入不重叠；编排者合并前做了编译检查。
- **变异检查**：给复制的 index 新 mtime、不复制 `info/exclude`、不写占位、把 GIT_DIR 换成用户的并去掉占位，都会被用例抓到。

## 逐项意见

1. **快照档的隔离**：每仓库一个私有对象库、每工作区一个 GIT_DIR（0700，没有 `hooks/`）；`prepare` 设齐 `GIT_DIR`、`GIT_WORK_TREE`、`GIT_INDEX_FILE`、`GIT_OBJECT_DIRECTORY`、`GIT_ALTERNATE_OBJECT_DIRECTORIES`、`GIT_CONFIG_NOSYSTEM`、`GIT_CONFIG_GLOBAL`（cupper 的空文件）和 `HOME`。白名单配置是纯函数，有单元测试；git-lfs 的标准取值换成宿主的绝对路径。
2. **内容一致**：复制 index 保留 mtime（macOS 的 `std::fs::copy` 本就保留，显式设回是为了不依赖它）、复制 sharedindex 与 info 文件、源文件不在时删掉旧副本。7 种 index 形态的 tree 与 SP-5 方案相同。
3. **过滤器检测的条件**改为"配置里没有定义快照档不执行的过滤器时跳过"：没有定义的过滤器用户自己的 git 也按原样处理，`check-attr` 仍覆盖全部属性来源。编排者认可，设计 5.2.10 已改（`712c896`）。
4. **占位内容用固定的 ASCII 格式**：宿主不产生界面文案，界面按格式识别。编排者认可，设计的例句已改。
5. **接入回合**：快照任务另起时持有 `Hold`、计入在途工作；`snapshot.recorded` 的四种状态与 `turns` 两列 tree 照设计写；子进程只经 `cupper-sys::spawn`，文件操作在 `spawn_blocking` 里。
6. **Q32 快照档逐键确认**：恶意配置样例在四个仓库上拍起始与结束快照，没有任何标记。执行者如实写明防护是分层的：单独把 GIT_DIR 换成用户的 `.git` 仍通过（命令行覆盖、占位、`diff-tree` 的默认行为都挡得住），同时去掉占位才失败。编排者据此更新 Q32 的快照档部分。

## 记下的点

1. **仓库可执行配置的指纹**：设计要求在回合开始和结束时随快照计算，前后不同就写 `notice{repo_config_changed}`。不在本任务的范围清单里，没有做。编排者另行安排（见计划 v1.3.53）。
2. **`core.excludesFile`、`core.attributesFile` 是相对路径时**，git 按快照档的工作目录解析，与用户在仓库里运行 git 时不同。少见，没有处理，记交接。
3. **私有对象库不回收**（M1 的设计），sharedindex 副本也会在工作区目录里累积。
4. **超大未跟踪目录的判定是"文件数 × 目录数"的扫描**，未跟踪目录很多时会慢。
5. **T186**：LFS 用例要把 `tool(…, lfs)` 指向 xtask 下载的 git-lfs。
6. **快照档的 GIT_DIR** 以后可供 `@` 文件列表、`git/status`、diff 复用（T120 等）。
