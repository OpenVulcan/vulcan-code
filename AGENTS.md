# Vulcan Code Agent Notes

## 工具能力实现最新约定

### 1. AI 可见的工具提示词
- 面向 AI 的 tool description / schema / 参数说明固定使用英文。
- 面向用户的 WebUI / 桌面壳文案可以继续使用中文。
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
- `owner_session_id` 必须保留在 metadata / payload / transcript attributes 中，方便 WebUI 重新绑定实时会话。
- 历史 payload 只作为无实时会话时的回退信息源，不应用来覆盖实时状态。
- 如果运行时里已经找不到对应 `command_session_id`，则当前语义直接视为 `stopped`，不能继续显示 `running`。

### 6. 工具暴露与配置原则
- “工具已实现”不等于“工具已对 AI 可见”，每次新增 Native Tool 后必须同时检查注册、请求构建、模式可见性、策略过滤链路。
- 不要在启动时静默改写用户持久化的工具策略配置。
- 如需保留系统默认策略，应优先采用运行时内存默认值，而不是偷偷回写用户配置文件。

### 7. 设计取舍
- 状态型工具要把“历史记录”和“实时状态”分层处理。
- 一次性命令结果可以主要依赖历史记录。
- 长生命周期命令会话必须始终有一条按 id 回查实时状态的路径，否则 WebUI 一定会出现“历史状态冒充当前状态”的问题。

## 本轮会话沉淀（2026-05）

### 8. ask 工具当前约定
- 模型可见的提问工具当前只保留 `ask`，不再保留 `question` 别名；注册入口在 `crates/vulcan-agent-runtime/src/tooling/bridge.rs`。
- `ask` 的 tool description / schema / 参数说明继续固定使用英文，目的不是给用户看，而是提高模型主动发起澄清与确认的概率。
- `ask` 的最终返回当前固定为 Markdown，而不是 JSON；实现位于 `crates/vulcan-agent-runtime/src/agent_runtime/question_runtime.rs` 的 `build_question_reply_markdown`。
- `ask` 运行时必须复用共享 `QuestionRuntime`，不能在 runtime refresh 时重建丢失；相关复用点在 `crates/vulcan-agent-runtime/src/agent_runtime/runtime.rs` 与 runtime stream bootstrap 链路。
- `ask` WebUI 当前通过 `apps/vulcan-code-web/src/components/ask/AskQuestionDialog.tsx` 渲染紧凑 question bar，并由 service kernel 镜像待回答请求。
- 当前交互语义：
  - 左右键切换问题标签或确认页。
  - 上下键移动当前题高亮项。
  - `Space` 只负责单选/多选切换；在确认页 `Space` 等同确认提交。
  - `Enter` 在答题页表示确认当前页并进入下一页，在确认页表示确认提交。
  - `Esc` 或右上角关闭不会直接取消，而是先弹出现有确认提示；确认放弃后会中断当前工具调用与后续对话执行。
- 纯文本题不应再显示“先选其他回答再输入”，而是进入该题后直接显示输入框并自动聚焦。
- 带选项题仍保留“其他回答”这一行，只有选中后才展开自定义输入。
- 确认页使用单独页面，不再保留底部“上一题/下一题/进入确认”大按钮；若存在未答完问题，确认页应明确列出未完成项并允许快捷跳转。
- 示例面板当前保留“工作区入口 / 补丁与骨架预览 / Git Diff 入口 / Git Diff 独立窗口”，并提供 `ask 调试` 入口专门用于样式调试。
- `ask` 调试预览当前是只读、可选择、可复制的 Markdown 预览缓冲区，不应退回为普通纯文本。
- `ask` 相关 Markdown 预览与渲染走 WebUI Markdown/文本预览链路，不再依赖旧桌面 Markdown 控件。
- overlay 顺序要注意：放弃确认提示必须渲染在 `ask` 面板之上，否则会出现提示被 ask 面板盖住的错觉。

### 9. Markdown 组件约定
- WebUI Markdown 渲染优先使用 `react-markdown`、`remark-gfm`、`rehype-sanitize` 与现有会话渲染组件。
- 需要“可选中 / 可复制 / 只读”的 Markdown 预览时，优先使用 WebUI 可选择文本或只读编辑区承载，不要退回不可复制的普通展示文本。
- 如果只是为了显示 Markdown 外观，但实际需求是“复制测试结果”，不要为了省事退回普通 `text`；这样会丢掉选择与复制能力。

### 10. LuaSkills 当前版本与对接约定
- LuaSkills 版本判断必须先读取根 `Cargo.toml` 的 workspace dependency 与根 `Cargo.lock` 中 `name = "luaskills"` 的锁定项，不得依据本节标题、历史计划、完成文档或会话记录推断当前版本。
- 截至 2026-07-15，当前仓库实际锁定 `luaskills = 0.5.2`；后续版本升级后必须同步更新本条，但任何任务仍应以 `Cargo.toml` 与 `Cargo.lock` 的实时内容为唯一事实来源。
- 当前版本已经采用 `input_schema` 契约，不要再按早期版本的旧扁平参数推导逻辑修改宿主。
- `LuaSkills` 工具参数 schema 当前应优先直接复用 `RuntimeEntryDescriptor.input_schema`；实现位于 `crates/vulcan-agent-runtime/src/luaskills_runtime.rs` 的 `luaskills_entry_args_schema`。
- `LUASKILL_SID` 仍然是宿主管理字段，但处理方式是：
  - 面向模型的可见 schema 里隐藏该字段。
  - 调用执行前由宿主自动注入。
  - 如果 `input_schema.required` 包含该字段，隐藏时必须同步从 `required` 中移除。
- `crates/vulcan-agent-runtime/src/tooling/bridge.rs` 中判断某个 skill 是否声明 `LUASKILL_SID` 时，优先看 `input_schema.properties`，只把旧版 `parameters` 作为回退。
- 如果某个 skill 包本身还没升级到 `input_schema` / `input_schema_file`，即使宿主使用当前 LuaSkills 版本，也拿不到完整 schema 能力；此时要去更新 skill 包本身，而不是继续在宿主里手搓特例 schema。
- 当前会触发严格 schema 问题的典型 LuaSkills 技能有 `vulcan-codekit`、`vulcan-curl`、`vulcan-testkit`；排查时优先检查 skill 包是否提供完整 schema。
- LuaSkills 技能操作进度当前已经接到 WebUI / Service 链路：宿主通过 LuaSkills runtime callback 把事件写入运行时句柄队列，WebUI 按 service snapshot / event 更新展示。
- 当前 WebUI 展示链路位于 `apps/vulcan-code-web/src/components/features/skills/` 与 service kernel 对应 LuaSkills snapshot：
  - 顶部 LuaSkills 面板会显示当前操作、阶段、百分比、字节进度、来源和消息。
  - 对应技能行会显示“安装中 / 更新中”这类紧凑徽标与当前阶段。
- 当前要注意上游粒度并不完全一致：安装 / 更新能拿到细粒度 phase，卸载 / 启用 / 停用更多还是宿主侧合成的粗粒度“进行中”状态；如果后续上游补了更细 phase，优先直接接 callback，不要再加宿主特判。
- 一个很容易漏掉的坑：`LuaSkillsRuntimeHandle` 内部引擎即使已经 reload / enable / disable / update 成功，`RuntimeFoundation` 里的 `luaskills_tools` 仍然是构建时快照，不会自动跟着变。
  - 因此在 `LuaSkillsBootstrapFinished`、`LuaSkillsCatalogLoaded`、`LuaSkillsManagementFinished` 这几个节点之后，必须显式调用 `refresh_runtime_foundation()`，否则工具表会表现成“要再手动触发一次别的操作才同步”。
  - 这个问题的症状通常是：技能明明已经启用/停用/更新成功，但 AI 可见工具集合没有立刻变化。

### 11. OpenRouter / Gemini 工具 schema 兼容层
- 当前 OpenRouter 经 Google AI Studio 路由时，对函数工具 schema 的校验明显严于通用 JSON Schema；不要假设“上游 schema 合法 = Gemini 一定接受”。
- 宿主当前通过共享兼容层 `normalize_provider_tool_parameters_schema` 做正规化，位置在 `crates/vulcan-agent-runtime/src/agent_runtime/provider_protocol_tool_definition.rs`。
- 这层兼容当前已经接入：
  - `provider_transport_google_http.rs`
  - `provider_transport_openai_compatible_http.rs`
  - `provider_transport_openai_responses_http.rs`
- 当前正规化职责包括：
  - 为对象形 / 数组形分支补显式 `type`。
  - 递归处理 `properties`、`patternProperties`、`dependentSchemas`、`items`、`additionalProperties`、`unevaluatedProperties`、`unevaluatedItems`、`not`、`anyOf`、`oneOf`、`allOf` 等子 schema。
  - 对只写了 `required` 的组合分支复制父级 `properties` / `additionalProperties`，兼容 Gemini 要求“required 字段必须在同一分支声明”的限制。
  - 把 JSON Schema 的 `type: ["string","number","boolean"]` 这类多类型数组改写成 Gemini 更容易接受的 `anyOf`。
- 典型坑：
  - `required` 在 `anyOf` / `oneOf` 分支里但分支没有 `type: object`，Google 会拒。
  - `required` 引用了父级 `properties` 里才定义的字段，Google 仍会拒。
  - `items.type` / `additionalProperties.type` 用联合数组时，Google 可能报成“missing field”，本质是它不接受多类型数组写法。
- 这类问题优先在 provider 兼容层修，不要先去改 skill 原始 schema，除非确认 skill 本身就是错的。

### 12. Provider 错误诊断约定
- 工具与 provider 错误文案当前要求统一英文，不走语言包；面向用户 WebUI 可继续中文。
- 标准 HTTP provider 错误格式化统一走 `crates/vulcan-agent-runtime/src/agent_runtime/provider_transport_http_support.rs` 的 `format_standard_error_response`。
- 该格式化当前会尽量保留：
  - HTTP status
  - `error.code`
  - `error.type`
  - `error.param`
  - `error.message`
  - `error.metadata` 里的 `raw` / `details` / `provider_name` / `upstream_provider` / `model` / `request_id`
  - 顶层 `user_id` / `request_id`
- 当 OpenRouter / OpenAI-compatible 上游报 `function_declarations[n]` 时，错误增强层会把索引反查为 `tool[n]=...`，实现位于 `provider_transport_openai_compatible_http.rs`。
- 调试顺序建议：
  - 先看 `tool[n]=...` 定位具体工具。
  - 再看报错路径是 `parameters.properties.xxx`、`items`、`anyOf`、`oneOf` 还是 `required`。
  - 最后判断是 skill schema 问题还是 Gemini 兼容层问题。

### 13. Provider 目录同步脚本约定
- 当前同步 OpenCode / models.dev provider 目录的脚本为 `scripts/sync_opencode_provider_catalog.ps1`。
- 常用调用方式：
  - 试运行：`pwsh -File scripts/sync_opencode_provider_catalog.ps1 -DryRun`
  - 正式写入：`pwsh -File scripts/sync_opencode_provider_catalog.ps1`
  - 如需自定义目录或报告：
    - `-ProviderDirectory <path>`
    - `-ReportPath <path>`
    - `-RemoteCatalogUrl <url>`
- 默认同步源是 `https://models.dev/api.json`。
- 默认排除 `opencode` 与 `opencode-go`，这是刻意策略，不要在脚本里误删。
- 仍需人工复核运行时支持的 provider 当前内置在 `ManualReviewProviderIds`，例如 `amazon-bedrock`、`google-vertex`、`vercel` 等；资源文件同步完成不代表运行时已经真正可用。
- 合并策略必须是“保留本地既有结构与扩展字段，只追加缺失模型”：
  - 不覆盖已有 provider 顶层自定义字段。
  - 不覆盖已有模型对象。
  - 本地独有字段如 `usageCapture`、`reasoningProfile`、`modelRuntimeOverrides`、`extraHeaders` 等必须保留。
- 同步脚本当前带结构守卫 `Assert-ProviderDefinitionShape`，用于防止把数组字段错误压成标量。
- 这轮真实踩坑就是：如果把 `credentialEnvKeys`、`fields`、`inputModalities`、`outputModalities`、`aiTools` 之类数组字段写坏成字符串或对象，运行时会在加载 provider catalog 时 panic。
- 不要直接编辑 `output/resources/providers` 作为长期来源；源资源目录以 `crates/vulcan-runtime-assets/resources/providers` 为准。

### 14. Provider 模型显示与 enabled_models 约定
- UI 里大多数模型列表看的不是“完整 catalog”，而是 `effective_model_choices`；实现位于 `crates/vulcan-provider-config/src/config.rs`。
- 只要某个 provider 配置项已经持久化了 `enabled_models`，新增同步进来的模型不会自动出现在工作台模型选择器里，除非：
  - 它被手动加入 `enabled_models`
  - 或它在刷新合并时命中“新发现且 `default_enabled = true`”的自动启用逻辑
- `current_provider_model_choices` 与 `configured_workbench_model_choices` 都依赖 `effective_model_choices`，因此“资源文件里明明有模型，但界面里看不到”通常先查 `enabled_models`。
- 如果某个旧 provider 条目完全没有保存过 `enabled_models`，当前逻辑会回退到完整目录，而不是只看默认启用模型。
- `alibaba`、`alibaba-cn`、`alibaba-coding-plan-cn` 等 provider 资源是独立文件，不要假设上游 `alibaba` 新增模型会自动出现在 `alibaba-cn`。
- 这轮实际出现过的例子：
  - `Qwen3.7 Max` 已经进了 `alibaba.json` 与 `openrouter.json`
  - 但 `alibaba-cn.json` 还没有
  - 同时用户本地 `enabled_models` 也可能没把它加入，所以 UI 依旧看不到

### 15. 参考来源与排查顺序
- provider 目录与模型节点同步，当前主要参考 `D:\projects\OpenCode\opencode` 与 `models.dev/api.json`。
- LuaSkills schema 行为与新字段支持，当前主要参考 `D:\projects\vulcan-luaskills` 与技能仓库 `D:\projects\vulcan-luaskills-skills`。
- 遇到“同一个问题到底该改宿主、改 skill 包、还是改 provider 兼容层”的判断顺序：
  - 先确认当前代码是否已经接入上游新能力，例如 `input_schema`。
  - 再确认 skill 包实际是否已经产出完整 schema。
  - 如果 skill 合法但 Gemini / OpenRouter 仍拒绝，则优先在 provider 兼容层正规化。
  - 只有确认 skill 自己输出错误时，才改 skill 包或其 schema 文件。
