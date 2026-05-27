# Token 预算 tkz 默认启动策略计划

## 任务目标

本轮目标是根据启动卡顿反馈，调整 Token 预算初始化策略：默认使用 `.tkz` 后端，不再默认加载 `.tiktoken`；只有显式开关启用时才使用 `.tiktoken` 后端。同时将 Token 预算初始化纳入现有启动面板生命周期，在 vldb 与 LuaSkills 初始化之后、主界面 Ready 之前完成或降级，避免后台状态让用户误以为初始化已经全部结束。

## 详细执行步骤

1. 梳理当前 Token 后端选择
   - 确认 GUI 当前调用 `TokenCounter::from_assets_dir` 时默认使用 `Wordchipper`。
   - 确认 `.tkz` OpenAI 资源已经存在于 runtime-assets 的 `openai/*.tkz`。

2. 增加后端开关
   - GUI Token 预算运行时默认选择 `TokenizerBackend::Tokie`。
   - 增加显式环境开关，仅在开关为真时使用 `TokenizerBackend::Wordchipper` 加载 `.tiktoken`。
   - 状态文案中显示当前后端，方便诊断“到底是 tkz 还是 tiktoken”。

3. 调整启动生命周期
   - LuaSkills runtime 完成后进入 Token 预算初始化阶段。
   - Token 预算初始化完成后再进入 Ready，并启动工作台数据加载、MCP bootstrap 与 LuaSkills catalog 后台任务。
   - 初始化失败不阻止 Ready，只记录警告并保留启发式估算回退。

4. 验证与归档
   - 运行格式化、编译和关键测试。
   - 验证源码中 GUI 默认路径不再调用 `.tiktoken` 后端。
   - 追加执行总结并归档计划，然后中文结构化提交。

## 技术选型

- 使用环境变量 `VULCAN_TOKEN_BUDGET_ENABLE_TIKTOKEN=1` 作为临时显式开关，避免立即扩展持久化配置 schema。
- 默认后端为 `TokenizerBackend::Tokie`，使用 `.tkz` 资源。
- 启动生命周期仍通过 `Task::perform` 执行初始化，保持 UI 线程不阻塞。

## 验收标准

- GUI Token 预算初始化默认使用 `.tkz` 后端。
- 只有设置 `VULCAN_TOKEN_BUDGET_ENABLE_TIKTOKEN=1/true/yes/on` 时才使用 `.tiktoken` 后端。
- Token 预算初始化显示在启动面板阶段，不再发生在主界面 Ready 之后。
- 初始化失败不阻塞主界面，预算回退仍可工作。
- 相关格式化、编译和关键测试通过。

## 执行变更总结

### 1. 核心修复与调整概述

- 已将 GUI Token 预算运行时默认后端改为 `TokenizerBackend::Tokie`，默认加载 `.tkz` OpenAI 基础编码资源。
- 已新增临时显式开关 `VULCAN_TOKEN_BUDGET_ENABLE_TIKTOKEN`，仅在值为 `1/true/yes/on` 时启用 `.tiktoken` 后端。
- 已将 Token 预算初始化从 Ready 后后台任务调整为启动面板阶段：LuaSkills 就绪后进入“正在初始化 Token 预算运行时”，完成后再进入 Ready 并加载工作台数据、MCP 与 LuaSkills catalog。
- 已在运行时状态文案中展示资源目录和后端标签，便于确认当前使用 `tokie` 还是 `wordchipper`。

### 2. 📂文件变更清单（区分新增/修改/删除）

- 修改：`apps/vulcan-code-gui/src/agent_runtime/token_budget_runtime.rs`。
- 修改：`apps/vulcan-code-gui/src/app_shell/update.rs`。
- 新增：无。
- 删除：无。

### 3. 💻关键代码调整详情（方法变动、核心逻辑更替）

- `TokenBudgetRuntime` 新增 `backend: TokenizerBackend` 状态字段，并在 `Debug` 与 `status_row` 中输出。
- `initialize_from_resource_directory` 现在通过 `resolve_default_backend` 选择默认后端，并转发到 `initialize_from_resource_directory_with_backend`。
- `initialize_from_resource_directory_with_backend` 使用 `TokenCounter::from_assets_dir_with_backend` 构建计数器，避免 GUI 默认走 `from_assets_dir` 的 `.tiktoken` 默认路径。
- `resolve_default_backend` 默认返回 `TokenizerBackend::Tokie`，只有环境变量真值时返回 `TokenizerBackend::Wordchipper`。
- `LuaSkillsBootstrapFinished` 成功后不再立刻 Ready，而是先调度 Token 预算初始化；`TokenBudgetRuntimeInitialized` 再启动工作台数据加载、MCP bootstrap 与 LuaSkills catalog 加载。

### 4. ⚠️遗留问题与注意事项

- 当前开关先使用环境变量，尚未进入 GUI 持久化设置；后续如果用户需要界面开关，可再扩展 provider/runtime 配置 schema。
- `vulcan-token-counter` crate 自身的 `from_resources_dir` 默认仍保留 `Wordchipper`，本轮只改变 GUI 宿主策略，避免影响库级兼容性。
- 如果启用 `VULCAN_TOKEN_BUDGET_ENABLE_TIKTOKEN`，启动面板会等待 `.tiktoken` 后端初始化完成或失败，这是显式 opt-in 行为。
- 已验证命令：`cargo fmt --package vulcan-code-gui`、`cargo check -p vulcan-code-gui`、`cargo check -p vulcan-token-counter -p vulcan-context -p vulcan-runtime-assets -p vulcan-code-gui`、`cargo test -p vulcan-code-gui runtime_prompt_layers_for_context_compaction_use_developer_layer`、`cargo test -p vulcan-token-counter`、`cargo test -p vulcan-context`、`git diff --check`。
