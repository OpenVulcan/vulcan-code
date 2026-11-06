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
- 不要在工具 description 中把 `shell_command` 的返回称为 summary；它返回按到达顺序合并的输出末尾与
  退出状态，较早的行通过 `shell_job` 读取（见第二节）。
- 不得在启动时静默改写用户持久化的工具策略。系统默认优先保留为运行时内存默认值。
- `shell_command` 的必填 `purpose`、`terminal_session` 的可选 `purpose` 以及原生工具 `declare_intent` 不影响执行；
  它们在权限复核中作为代理理由，语义见 [`permission-modes-and-judge.md`](permission-modes-and-judge.md) 的“代理理由”。
  `shell_command` 的 `purpose` 另外显示在命令卡与会话活动面板上，并随后台任务保存，供 AI 用 `shell_job list` 辨认任务。

## 二、`shell_command`

- `timeout_ms` 省略表示无限等待，直到进程自然退出；显式传入时必须 ≥ 1，传 `0` 返回参数错误。
- 省略 `stdin` 表示空输入（立即 EOF），不继承服务进程的标准输入；提供时以 UTF-8 字节一次写入后关闭。
  Windows 上的 PowerShell 默认按控制台代码页（如 936）解码 `[Console]::In`，并按 `$OutputEncoding`
  编码管道给原生程序的文本，非 ASCII 内容会乱码。因此提供 `stdin` 且 Shell 为 `pwsh`/`powershell` 时，
  宿主只在启动参数中于命令前加一条受 `try` 保护的语句，把 `[Console]::InputEncoding` 与
  `$OutputEncoding` 设为 UTF-8（`HostCommandShell::command_text_for_utf8_stdin`），不改输出编码，报告与
  权限判定仍使用原命令；以 `using`、`#requires`、`param` 或 `[CmdletBinding()]` 开头的命令不加。`$input`
  在命令的第一条语句之前已被解码，无法由此修正，工具描述要求改用 `[Console]::In.ReadToEnd()`。直接读取
  stdin 的原生程序收到的就是原始 UTF-8 字节。`terminal_session` 不受影响。
- 子进程在独立进程树中启动（Unix 为新进程组，Windows 依赖 `taskkill /T`）。超时或工具调用被
  取消时终止整棵进程树；超时返回 `timed_out` 状态并附带截至超时已产生的 stdout/stderr。
- 自然退出后，输出排空最多再等待 `CHILD_OUTPUT_DRAIN_GRACE`（唯一定义在
  [`process_tree.rs`](../../crates/vulcan-host-tools/src/process_tree.rs)）；后台进程仍持有管道时停止
  捕获并在 `failure_reason` 中说明，状态仍按退出码判定，且不终止这些后台进程。
- 每次调用都是一个命令任务（[`shell_job`](../../crates/vulcan-host-tools/src/shell_job.rs) 的 `ShellJobRuntime`）：
  stdout 与 stderr 按到达顺序合并写入会话目录 `shell-jobs/` 下的滚动日志。日志保留开头
  `SHELL_JOB_LOG_HEAD_BYTES` 与最新输出，总量不超过 `SHELL_JOB_LOG_LIMIT_BYTES`，中间被丢弃的部分以缺口行表示；
  每个会话的日志总量受 `SHELL_JOB_SESSION_LOG_BUDGET_BYTES` 约束。不因输出量终止命令，也不再有合并输出硬上限。
  这些边界值只在该文件定义。
- 结果只返回输出末尾：`SHELL_OUTPUT_TAIL_BUDGET_BYTES` 之内的完整行，超过 `SHELL_OUTPUT_LINE_WRAP_BYTES`
  的行切分为续行；输出被截断时结果注明总行数、总字节数与 `job_id`，较早的行由 `shell_job` 读取或搜索。
- 结果与日志交给模型或界面前，按与直接工具结果相同的登记值脱敏。
- 内联渲染结果超过 `200000` 字节时，切换为项目内 `.vulcan/temp/` 分页文件；文件名
  形如 `tool_output_<exclusive-random-id>.md`，使用独占创建，写入完成后发布；并发调用不共享文件身份。
- 运行中的实时尾部只用于显示（[`shell_command_live.rs`](../../crates/vulcan-agent-runtime/src/tooling/native/shell_command_live.rs)）：
  模型发起的调用在运行期间以“会话 + API 调用 + 工具调用”登记，显示行取自该任务日志的末尾，前台与后台共用
  同一条输出管线；尾部不进入转录、不交给模型。调用以任一路径返回即撤销登记，因此查不到即表示调用不在运行
  （含 Core 重启后）；转入后台的任务改由会话活动面板查看。显示行数
  `SHELL_COMMAND_LIVE_TAIL_LINES` 与尾部字节上限只在该文件定义；推送间隔
  `SHELL_COMMAND_OUTPUT_PUSH_INTERVAL` 只在服务内核
  [`shell_command_output.rs`](../../crates/vulcan-service-kernel/src/shell_command_output.rs) 定义。
  WebUI 命令卡仅在展开且尚无结果时调用 `service_attach_shell_command_output`，收到最近若干行后按
  `shell.command.output.changed` 整段替换，收起即 `service_detach_shell_command_output`，浏览器断开时订阅
  随之取消；调用结束后卡片改为显示可靠结果。工具链中的步骤不提供实时尾部显示。
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

## 后台任务与 `shell_job`

- `shell_command` 传 `background: true` 时立即返回 `running_in_background` 状态与 `job_id`；`timeout_ms` 约束整个
  后台任务。同时运行的后台任务数上限为 `MAX_RUNNING_BACKGROUND_SHELL_JOBS`。
- 只有插队把运行中的前台命令转到后台：调用返回 `running_in_background`，命令继续运行且不再受该调用的
  `timeout_ms` 约束。排队消息不影响运行中的命令。用户也可在命令卡上“转到后台”
  （`service_background_shell_command`）。工具调用被取消时仍终止前台任务的整棵进程树。
- `shell_job` 的 `list`、`status`、`read`、`search`、`wait`、`stop` 只作用于本会话的任务，权限分类为
  `Ungoverned`，不经判定模型与复核 LLM。`read` 默认与上限行数、`wait` 默认与上限时长、`search` 的匹配与上下文
  上限只在 `shell_job.rs` 定义。
- 后台任务结束时生成通知，只保存在进程内，不持久化：回合进行中在下一次 API 请求前作为宿主消息追加。
  根会话空闲（无开放回合、无活动运行或内核回合、无排队输入、未归档、未被用户停止、无待决计划）时，
  `shell_job_continuation`（Core 侧 `shell_job_continuation.rs`）开启一个宿主发起的唤醒回合
  （`TurnOrigin::ShellJobContinuation`，系统上下文键 `shell_job_continuation`，回合作用域），
  全部待送达通知在其首次请求前一并送达；同时结束的多个任务合并为一个回合，通知只送达一次。
  触发点是任务结束事件，以及回合结束且排队输入、目标续跑、子代理回流与代理任务都未接手之时；
  这些后继始终优先。尚未分发的唤醒回合会被新的用户输入取代，无法分发时以停止结束，通知继续保留到
  下一次请求。子会话不被唤醒，其通知留待自身的下一次请求；专职会话按普通根会话唤醒。Core 重启后
  通知随进程丢失，重启时仍开放的唤醒回合按停止闭合。
- 通知文本格式唯一定义在 `vulcan-session-core` 的 `v1::shell_job_notice::ShellJobNoticeFacts`（`render`/`parse`），
  WebUI 的 `shell-job-notice.ts` 镜像其解析；不新增事件类型。WebUI 把唤醒回合的系统上下文与每条通知
  （回合内送达与唤醒回合相同）都显示为上下文注入卡片的“后台命令已结束”类别：通知条目以命令用途
  （无用途时为命令首行）、任务标识、结束状态与退出码为标题，详情为模型收到的原文。TUI 以同一标题标记
  通知原文，唤醒回合显示为同文案通知。上下文压缩请求附带仍在运行的任务清单，只用于该次请求，不写入会话。
- 关闭子代理与归档会话会停止其拥有的任务；任务日志与索引保留在会话目录中，不随清理删除。每个会话在内存中
  只保留有限数量的已结束任务，其余按索引回查。
- 会话活动面板（WebUI 右侧 `activity` 页签）列出根会话的子代理与命令任务：运行中在前并显示最近若干行，
  已结束的命令默认只显示最近几条。“清除已结束”只在应用数据库 `session_activity_marks_v1` 记录时间标记，
  不删除会话、日志或索引。展开的命令日志在有界高度内贴底跟随，向上滚动按 `beforeLine` 逐页加载，内存中只保留
  有界行数。浮动会话工具面板在每个对话会话中都显示（不依赖 Git、子代理或运行中的命令），始终保留子代理与
  后台命令两行数量（为 0 时也显示），点击打开活动面板；不逐个列出子代理。
  服务调用：`service_list_shell_jobs`、`service_read_shell_job_output`、`service_stop_shell_job`、
  `service_background_shell_command`、`service_session_activity`、`service_clear_session_activity`、
  `service_running_shell_job_count`；任务开始、转入后台与结束发布 `shell.job.changed`。

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
- 后台任务：转入后台、插队释放、停止杀树、日志滚动与缺口、行切分、末尾预算与脱敏测试通过；
- WebUI 历史回放不会冒充实时运行状态；
- 没有静默回写用户工具策略；
- `cargo fmt --all -- --check` 与目标 crate 测试通过。

