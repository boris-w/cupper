# T159 笔记

## 第一步：剖析的做法（2026-11-16）

- **trace 怎么录**：都加在 T103 的 spike 里，只在设了 `SP6_TRACE=1` 时生效；不设时场景照旧，数值可以和 T103 对照。
  - `trace.ts`：窗口的 DevTools 协议会话用 `Tracing` 录，输出 JSON，和 Performance 面板的做法相同。第一次用的是主进程的 `contentTracing`（43797367278），但 Electron 45 写出的是 Perfetto 的 protobuf，汇总脚本读不了。类别是 devtools.timeline（含 disabled-by-default 的 timeline 和 frame）、toplevel、v8、v8.execute、`disabled-by-default-v8.cpu_profiler`（V8 的采样）。
  - 每段起止由渲染进程用 `console.timeStamp("sp6:<label>:start|end")` 标出。本机试过，`performance.mark` 在 trace 里没有出现。
  - 场景 2：从开始打字算起，第 10、35、60、85 s 各录 4 s，并记下每段起止时消息的长度。
  - 场景 3：只录 4 路。1–4.5 s 只有流；10–14 s 含重新同步。
  - 场景 7：不在 LRU 和在 LRU 的切换各录 8 次，每次切换一段（`away#n`、`cached#n`）。
- **`SP6_UNMINIFIED=1`**：照 `build:e2e` 构建，只是渲染进程的 `vite build` 加 `--minify false`。这样采样里的函数保留名字，产物里的 `//#region <路径>` 注释能把行号对回模块。
  - 代价：代码体积和解析时间与发布构建不同；热点函数本身的开销不受影响。
- **`summarize.mjs`**：所有用例结束、App 退出后，在 runner 上逐个汇总 trace，结果写进 `<name>.summary.json` 和 `.summary.txt`，再把 trace 压成 `.json.gz` 随产物上传。汇总内容：
  - 主文档渲染进程主线程的忙碌时间，按事件名统计的自身时间；
  - 采样：按函数统计的自身时间，按模块统计的自身时间和包含时间。
- **本机冒烟**（截图工具的页面，headless，临时脚本，没有提交）：一条消息在 7 s 里涨到约 300 KB 的不换行文字，汇总能正确分段、对上模块，只用来验证工具。
  - Layout 2.8 s、FunctionCall 2.6 s。
  - JS 自身时间最多的一行是 StreamList 的 `useLayoutEffect` 读 `scrollHeight` 引起的强制布局（2.2 s）；其次是 micromark 解析（约 2 s，含子模块）。
  - 这只是方向，以 runner 的 trace 为准。
