# T247 notes

## 选哪种：判定前去掉转义码

两种做法都能让 T245 那组运行判为通过，我选了在判定前去掉转义码，理由如下：

- **不依赖工具是否遵守 `NO_COLOR`**：Vitest 5.0.3 内置的 tinyrainbow 只要环境里有 `CI` 就上色，`NO_COLOR` 的优先级最高，所以关颜色现在也有效。但这是每个库各自的约定，换了 reporter、升级，或者 setup 里哪天加上 `FORCE_COLOR`、`CARGO_TERM_COLOR=always`，判定都可能再出错。去掉转义码以后，不管日志有没有颜色，判定都一样。
- **cargo 那条路的判定不变**：libtest 的输出写进文件时不上色，去掉转义码前后内容相同。
- 代价是上传的日志仍然带颜色，读的时候要自己去掉，或者用支持颜色的查看器。这属于"test-repeat 的其他改动"，不在本任务范围内。

## 写法

```bash
ran() { perl -pe 's/\e\[[0-9;?]*[A-Za-z]//g' "$1" | grep -E "$RAN" > /dev/null; }
```

- 用 perl，不用 sed：macOS runner 上的 BSD sed 不认 `\x1b`。perl 两种 runner 上都有，工作流里本来就用它做超时。
- 去掉的是所有 CSI 序列（`ESC [ 参数 字母`），不只 SGR 颜色码，以防 reporter 输出光标控制码。
- **不用 `grep -q`**：Run 步骤开着 `pipefail`。`grep -q` 一匹配就退出，perl 还在写，会收到 SIGPIPE，整条管道返回 141，通过的运行反而记成失败。日志小于管道缓冲区时碰不到，日志大了就会出现。所以让 grep 读完全部输入，输出丢掉。

## 本机检查

用 bash 照 Run 步骤的设置（`set +e -uo pipefail`）跑了新旧两种判定，日志样本是 T245 run 42589259180 下载的真实日志，加上几份手写的：

| 日志 | 新判定 | 旧判定 | perl 管道 + `grep -q` |
|---|---|---|---|
| vitest 通过（带颜色，`Tests 11 passed (11)`） | 通过 | 失败（误报） | 通过 |
| vitest 有失败（带颜色，`Tests 1 failed \| 10 passed`） | 失败 | 失败 | 失败 |
| vitest 没有匹配的用例（`Tests 11 skipped`） | 失败 | 失败 | 失败 |
| cargo 通过（`test result: ok. 5 passed`） | 通过 | 通过 | 通过 |
| cargo 全被过滤（`ok. 0 passed`） | 失败 | 失败 | 失败 |
| 13 MB 的 cargo 日志，开头就是通过行 | 通过 | 通过 | 失败（141） |

最后一行说明了为什么不用 `grep -q`。

## 规模

工作流 +5 −1。
