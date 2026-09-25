# 工具与终端运行时约定

本文仅在修改 AI 可见工具、`shell_command`、`shell_session`、`terminal_session`、命令会话状态或其
WebUI 展示时阅读。普通业务任务不需要加载这些细节。

当前源码权威入口：

- [`shell_command.rs`](../../crates/vulcan-agent-runtime/src/tooling/native/shell_command.rs)；
- [`shell_session.rs`](../../crates/vulcan-agent-runtime/src/tooling/native/shell_session.rs)；
- [`terminal.rs`](../../crates/vulcan-agent-runtime/src/tooling/native/terminal.rs)；
- [`shell_host.rs`](../../crates/vulcan-host-tools/src/shell_host.rs)；
- [`command_session_runtime`](../../crates/vulcan-host-tools/src/command_session_runtime.rs)。

## 一、AI 工具暴露

- “工具已实现”不等于“工具已对 AI 可见”。新增 Native Tool 时必须同时检查注册、请求构建、模式
  可见性、策略过滤和实际执行链路。
- 面向 AI 的工具名称、description、Schema 和错误使用英文；面向用户的 WebUI 文案可以使用中文。
- 不要在工具 description 中把 `shell_command` 的返回称为 summary；它返回完整解码后的
  `stdout`、`stderr` 和退出状态（输出越过硬上限时除外，见第二节）。
- 不得在启动时静默改写用户持久化的工具策略。系统默认优先保留为运行时内存默认值。

## 二、`shell_command`

- `timeout_ms` 省略表示无限等待，直到进程自然退出；显式传入时必须 ≥ 1，传 `0` 返回参数错误。
- 省略 `stdin` 表示空输入（立即 EOF），不继承服务进程的标准输入；提供时一次写入后关闭。
- 子进程在独立进程树中启动（Unix 为新进程组，Windows 依赖 `taskkill /T`）。超时或工具调用被
  取消时终止整棵进程树；超时返回 `timed_out` 状态并附带截至超时已产生的 stdout/stderr。
- 自然退出后，输出排空最多再等待 `CHILD_OUTPUT_DRAIN_GRACE`（唯一定义在
  [`process_tree.rs`](../../crates/vulcan-host-tools/src/process_tree.rs)）；后台进程仍持有管道时停止
  捕获并在 `failure_reason` 中说明，状态仍按退出码判定，且不终止这些后台进程。
- stdout 与 stderr 的原始字节合计超过 `SHELL_COMMAND_OUTPUT_HARD_LIMIT_BYTES`（8 MiB，唯一定义在
  [`shell_command.rs`](../../crates/vulcan-agent-runtime/src/tooling/native/shell_command.rs)）时直接阻断：
  立即终止整棵进程树，丢弃全部已捕获输出（不落盘、不分页、不返回给模型），返回
  `output_limit_exceeded` 状态与提示缩小输出范围的 `failure_reason`。上限按合并字节计数，
  自然退出后才发现越限的命令同样按此处理；恰好等于上限的输出仍完整返回。
- 未越过硬上限但内联渲染结果超过 `200000` 字节时，切换为项目内 `.vulcan/temp/` 分页文件；文件名
  形如 `tool_output_overflow_page_md_<unix_ms>.md`。
- `auto` 使用用户已配置且当前可检测的默认 Shell；没有用户配置时按平台稳定优先级选择。
- Windows 当前检测优先级为 `pwsh`、`powershell`、`cmd`、`bash`、`sh`、`zsh`，但只暴露实际
  检测成功的启动器。
- Windows 上 `bash`、`sh`、`zsh` 依赖可用的 Unix-like 环境，不能假定系统原生存在。

## 三、`shell_session`

- `cwd` 只在 `start` 生效；后续 `write` 不改变工作目录。
- `read` 是轮询，不是事件推送。
- `wait_for_output_ms` 默认 `0`，表示立即返回当前缓冲。
- `max_chunks` 默认 `128`。
- chunk 边界由实现决定，不能假设严格按行或按命令返回。
- `start` 带 `stdin` 且未指定 `append_newline` 时，默认追加换行；不带 `stdin` 时默认不追加。
- `write` 未指定 `append_newline` 时默认不追加换行。
- `start` 不能把一个未以换行结束的开放输入行留给后续 `write` 隐式拼接。
- `stop` 终止会话子进程所在的整棵进程树；子进程退出后，输出排空同样受
  `CHILD_OUTPUT_DRAIN_GRACE` 约束，不会因后台进程持有管道而永久停留在运行态。

## 四、Windows 交互换行与 Shell

- 交互换行按实际交互 Shell 决定，不能只按宿主平台决定。
- `cmd`、`powershell`、`pwsh` 使用 `\r\n`。
- `bash`、`sh`、`zsh` 使用 `\n`。
- Windows 上如果启动命令直接进入 `bash`、`sh` 或 `zsh`，必须保证 launcher 与实际交互 Shell
  一致；不能让 `auto` 隐藏二者差异。

## 五、长生命周期命令状态

- 主转录卡片、工具弹层和停止按钮必须优先根据 `command_session_id` 查询实时状态。
- `owner_session_id` 必须保留在 metadata、payload 和 transcript attributes 中，以便重新绑定。
- 历史 tool result 只在实时会话不可用时作为展示回退，不能覆盖实时状态。
- 运行时已找不到对应 `command_session_id` 时，当前语义是 `stopped`，不能继续显示 `running`。
- 一次性命令主要依赖历史结果；长生命周期命令必须始终保留按 ID 回查实时状态的路径。

## 六、修改后的最低验证

- 工具描述和 Schema 与解析器默认值一致；
- Windows 与 POSIX Shell 的 launcher、换行和显式选择测试通过；
- `start/read/write/stop`、游标、等待、分页和实时状态测试通过；
- WebUI 历史回放不会冒充实时运行状态；
- 没有静默回写用户工具策略；
- `cargo fmt --all -- --check` 与目标 crate 测试通过。

