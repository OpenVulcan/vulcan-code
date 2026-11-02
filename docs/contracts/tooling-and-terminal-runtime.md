# 工具与终端运行时约定

本文仅在修改 AI 可见工具、`shell_command`、`terminal_session`、命令会话状态或其
WebUI 展示时阅读。普通业务任务不需要加载这些细节。

当前源码权威入口：

- [`shell_command.rs`](../../crates/vulcan-agent-runtime/src/tooling/native/shell_command.rs)；
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
- `shell_command` 与 `terminal_session` 的可选 `purpose` 参数、以及原生工具 `declare_intent`，只作为权限复核的
  代理理由，不影响执行；语义见 [`permission-modes-and-judge.md`](permission-modes-and-judge.md) 的“代理理由”。

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
- 内置 `terminal-session` 技能（[`SKILL.md`](../../crates/vulcan-prompt-skills/src/assets/system/terminal-session/SKILL.md)）
  向模型复述该上限并禁止用 `cat` 等方式整份读取日志；技能中的数值由
  `terminal_skill_quotes_current_output_hard_limit` 测试与常量比对，修改上限时必须同步技能文本。
- 未越过硬上限但内联渲染结果超过 `200000` 字节时，切换为项目内 `.vulcan/temp/` 分页文件；文件名
  形如 `tool_output_<exclusive-random-id>.md`，使用独占创建，写入完成后发布；并发调用不共享文件身份。
- `auto` 使用用户已配置且当前可检测的默认 Shell；没有用户配置时按平台稳定优先级选择。
- Windows 当前检测优先级为 `pwsh`、`powershell`、`cmd`、`bash`、`sh`、`zsh`，但只暴露实际
  检测成功的启动器。
- Windows 上 `bash`、`sh`、`zsh` 依赖可用的 Unix-like 环境，不能假定系统原生存在。

## 随包 RTK OSS 接入

- RTK 以独立 OSS 可执行文件接入，仅保留 Shell 命令改写与 Git 提交上下文过滤；不注册通用输出压缩
  Hook，不部署完整引擎或 `rtk-proxy`。发行标签、各平台制品名与摘要以 `scripts/build-inputs.lock.json`
  的 `tools.rtk` 为准，与 sg/rg 一同安装到 `third_party/tools/<target>/bin/`，由 `make.ps1 sync`
  复制到发行目录 `tools/bin/`。上游二进制无法在产品最低系统上运行的宿主列在 `source_targets` 中，改为
  按 `source`（固定到标签提交的源码归档及其 SHA-256，构建参数与上游 `rtk-oss` 相同）以仓库固定的 Rust
  工具链在本机编译，再执行同样的版本检查；目前为 Linux arm64，因为上游 arm64 制品需要 glibc 2.39，
  而发行版在 Ubuntu 22.04（glibc 2.35）上构建。锁文件既未声明制品也未声明源码构建的宿主（目前为
  Windows arm64）不随包提供 RTK。
- Shell 命令改写由宿主原生执行（[`rtk.rs`](../../crates/vulcan-host-tools/src/rtk.rs)），不经插件 Hook：
  `shell_command` 先以隐藏窗口子进程调用 `tools/bin` 中的 `rtk rewrite`（超时唯一定义为
  `RTK_REWRITE_TIMEOUT`），再把结果交给插件 Hook 链；报告中的 `original_command` 始终是模型原命令。
  设置保存在配置文件 `rtk` 块（`PersistedRtkConfiguration`），全部为默认值时不写出；工具目录每回合
  重建，因此设置从下一回合生效。随包缺少 RTK 时跳过改写并给出 info 诊断。
- Git 提交总结上下文同样由宿主原生压缩（`commit_summary.rs` 的 `apply_native_rtk_git_context`），在
  `git.commit_summary.context` 插件 Hook 之前执行，受 `rtk.enabled` 与 `gitCommitContextEnabled` 控制：
  依次运行 `rtk git status --short --branch`、`rtk git diff --cached --no-color`（含未暂存时再加
  `rtk git diff --no-color`），每条上限 `RTK_GIT_CONTEXT_TIMEOUT`；去除 `[rtk]` 提示行，空输出记为
  `(none)`，并保留宿主生成的未跟踪文件区块。任何一条失败都保留原始 Git 区块不变。
- `rewrite` 的上游协议归属为固定发行标签下的 `src/hooks/rewrite_cmd.rs`：退出码 `0/3` 携带改写，
  `1/2` 保持原命令；退出码不赋予 Vulcan 工具执行权限。超时或其他失败不得执行部分输出，
  多行改写必须保留完整命令并检查其中全部 RTK 路由；`selected` 模式只看 `enabledRoutes`，`all` 模式只看
  `disabledRoutes`。
- 宿主为模型启动的进程固定注入 RTK 变量：`shell_command` 每次执行、`terminal_session` 仅在 `start` 时
  注入 `RTK_SUPPRESS_HOOK_WARNING=1`（抑制 `rtk init` 提示），遥测关闭时再加 `RTK_TELEMETRY_DISABLED=1`；
  会话环境变量与 Hook 贡献可覆盖这些值。终端环境指纹只覆盖会话变量。`NO_COLOR=1` 只用于 `rtk rewrite`
  检查进程与已改写的命令，不进入终端会话。
- 统计协议归属为上游 `src/analytics/gain.rs` 与 `src/core/tracking.rs`：汇总保留其超大输入排除口径，
  按命令明细使用加权节省率并允许负值；汇总读取 `gain --all --format json`，按命令明细解析
  `gain --text`，原始 JSON 汇总不自行重算。上游版本更新时须重新核对以上源码并同步
  [`rtk_settings.rs`](../../crates/vulcan-service-kernel/src/rtk_settings.rs) 的解析与测试。
- RTK 设置与统计页面由宿主原生提供（WebUI 插件工作区内置页、TUI 插件区页面），服务为
  `service_rtk_settings`、`service_save_rtk_settings`、`service_rtk_statistics` 与
  `service_clear_rtk_statistics`；统计每次按需读取，不做宿主缓存与失效事件。
- 清理统计仅由用户点击触发，调用官方 `gain --reset --yes`，不删除数据库文件、不修改路由配置；
  共用该统计库的终端历史也会清空。清理范围以上游 `src/analytics/gain.rs`、`src/core/tracking.rs`
  与 `src/core/retriever.rs` 为准，包括命令、Token、解析失败、Hook 与回读统计，保留可回读内容。

## 最终返回预算与分页包装

`ToolResultBudgetSnapshot.tool_result` 约束最终面向模型的文本，而非仅正文。模板、截断提示、分页指针及
分块表全部计入字节和行数上限。截断在整行边界预留模板开销；预算连有效包装都容纳不下时明确失败，
不返回超限内容。分页表仅提供有界预览与后续读取提示；原始文件保持原字节，CRLF 按原始载体核算读取
预算。只有包装成功后才发布独占分配的分页文件，失败时删除私有暂存文件，不泄漏孤立结果。

模板正文缓存按绝对候选路径和文件版本隔离，总条目及字节数有界。每次命中前仍打开文件确认访问权限，
新建高优先级模板或删除模板立即反映；不存在和读取失败不缓存。时间戳或大小变化使缓存立即失效；
即使时间戳被外部还原，也最多复用一秒，之后重新读取。优先模板损坏或不可读时明确报错，不悄悄选用
另一份模板。缓存互斥锁不跨磁盘读取持有。

## 三、`terminal_session`

旧 `shell_session` 工具已由 `terminal_session` 取代并删除，模型可见的持久命令会话只有 `terminal_session`。

- 动作为 `list`、`start`、`send`、`read`、`snapshot`、`stop`；会话只属于创建它的控制会话，跨会话访问被拒绝。
- `cwd` 只在 `start` 生效；后续 `send` 只能通过输入 shell 命令改变终端内的工作目录。
- `read` 是轮询，不是事件推送：从 AI 游标返回新输出；`snapshot` 查看当前状态但不推进游标。
- 省略 `wait_policy` 时等待预算为 `0`，立即返回当前缓冲；任何等待模式都受宿主读取等待硬上限约束。
- `max_chunks` 默认值与上限分别是 `DEFAULT_COMMAND_SESSION_READ_MAX_CHUNKS` 与 `MAX_COMMAND_SESSION_READ_CHUNKS`
  （唯一定义在 [`command_session_runtime`](../../crates/vulcan-host-tools/src/command_session_runtime.rs)）。
- chunk 边界由实现决定，不能假设严格按行或按命令返回。
- `send` 的 `text` 与 `paste` 输入由宿主把 CRLF、CR、LF 归一为当前交互 Shell 的换行；控制键用 `key`/`chord` 项，不手写 ANSI 序列。
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
- 注册表只保留有限数量的已结束会话（`COMMAND_SESSION_RETAINED_FINISHED_LIMIT`，唯一定义在
  [`command_session_runtime`](../../crates/vulcan-host-tools/src/command_session_runtime.rs)）；新会话
  启动时按准入顺序淘汰最早的已结束会话，运行中会话从不淘汰。被淘汰的会话按上一条语义视为 `stopped`。
- 一次性命令主要依赖历史结果；长生命周期命令必须始终保留按 ID 回查实时状态的路径。

## 六、修改后的最低验证

- 工具描述和 Schema 与解析器默认值一致；
- Windows 与 POSIX Shell 的 launcher、换行和显式选择测试通过；
- `start/send/read/snapshot/stop`、游标、等待、分页和实时状态测试通过；
- WebUI 历史回放不会冒充实时运行状态；
- 没有静默回写用户工具策略；
- `cargo fmt --all -- --check` 与目标 crate 测试通过。

