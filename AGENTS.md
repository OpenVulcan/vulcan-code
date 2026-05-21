# Vulcan Code Agent Notes

## 工具能力实现最新约定

### 1. AI 可见的工具提示词
- 面向 AI 的 tool description / schema / 参数说明固定使用英文。
- 面向用户的 GUI 文案可以继续使用中文。
- 不要在工具提示词里使用“summary”描述 `shell_command` 的返回值，当前语义是返回完整解码后的 `stdout` / `stderr` 与退出状态。

### 2. shell_command 当前契约
- `timeout_ms` 省略时表示 `unlimited`，即一直等待到进程自然退出。
- 内联输出超过 `200000` 字节时会切换到分页文件模式。
- 分页文件当前落在项目内 `.vulcan/temp/`，文件名形如 `tool_output_overflow_page_md_<unix_ms>.md`。
- Windows 下 `auto` 默认优先级是 `pwsh > powershell > cmd`。
- 如果用户在设置中指定了默认 shell，则 `auto` 以用户设置为准。
- Windows 上使用 `bash` / `sh` / `zsh` 依赖兼容的 Unix-like 环境，不应假定系统原生可用。

### 3. shell_session 当前契约
- `cwd` 只在 `start` 时生效，后续 `write` 不会改变工作目录。
- `read` 是轮询模型，不是事件推送模型。
- `wait_for_output_ms` 默认值是 `0`。
- `max_chunks` 默认值是 `128`。
- `chunk` 粒度是 implementation-defined，只能保证是按宿主输出缓冲切分后的文本分片，不能假定严格按行或按命令返回。
- `start` 传入 `stdin` 时，如果未显式提供 `append_newline`，当前默认按 `true` 处理。
- `start` 若显式传入 `append_newline: false`，且 `stdin` 末尾没有换行，当前应直接拒绝，避免与后续 `write` 拼接成一条命令。

### 4. Windows 交互 Shell 约束
- Windows 下交互换行不能只按平台判断，必须按实际交互 shell 判断。
- `cmd` / `powershell` / `pwsh` 使用 `\\r\\n`。
- `bash` / `sh` / `zsh` 使用 `\\n`。
- Windows 上如果 `shell_session` 的启动命令直接进入 `bash` / `sh` / `zsh`，则不允许 `shell: auto`，必须显式指定 shell，避免外层 launcher 与内层交互 shell 不一致。

### 5. 命令会话状态展示
- `command_session` 相关 UI 不能只相信历史 tool result payload。
- 主转录卡片、工具弹层、停止按钮判定都必须优先通过 `command_session_id` 查询实时运行时状态。
- `owner_session_id` 必须保留在 metadata / payload / transcript attributes 中，方便 GUI 重新绑定实时会话。
- 历史 payload 只作为无实时会话时的回退信息源，不应用来覆盖实时状态。
- 如果运行时里已经找不到对应 `command_session_id`，则当前语义直接视为 `stopped`，不能继续显示 `running`。

### 6. 工具暴露与配置原则
- “工具已实现”不等于“工具已对 AI 可见”，每次新增 Native Tool 后必须同时检查注册、请求构建、模式可见性、策略过滤链路。
- 不要在启动时静默改写用户持久化的工具策略配置。
- 如需保留系统默认策略，应优先采用运行时内存默认值，而不是偷偷回写用户配置文件。

### 7. 设计取舍
- 状态型工具要把“历史记录”和“实时状态”分层处理。
- 一次性命令结果可以主要依赖历史记录。
- 长生命周期命令会话必须始终有一条按 id 回查实时状态的路径，否则 GUI 一定会出现“历史状态冒充当前状态”的问题。
