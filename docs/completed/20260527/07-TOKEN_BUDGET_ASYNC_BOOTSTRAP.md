# Token 预算运行时异步启动计划

## 任务目标

本轮目标是消除主界面显示前因 Token 预算计算器同步初始化造成的卡顿。Token 预算运行时应改为轻量占位先进入 `RuntimeFoundation`，再通过启动生命周期中的后台任务完成真实 tokenizer 初始化，并在完成后回填到运行时基础架构。主界面首屏展示不得等待 tokenizer warm-up。

## 详细执行步骤

1. 梳理现有启动链路
   - 确认 `RuntimeFoundation::build` 当前同步初始化 Token 预算运行时的位置。
   - 确认 `StartupLifecycle`、vldb 与 LuaSkills bootstrap 的任务调度和消息回填方式。

2. 改造 Token 预算运行时模型
   - 增加轻量占位构造方法，避免 `RuntimeFoundation::build` 同步执行 tokenizer warm-up。
   - 保留后台初始化方法，用于启动任务中创建真实 `TokenCounter`。
   - 调整状态文案，区分“初始化中 / 已初始化 / 不可用”。

3. 接入启动生命周期
   - 新增 Token 预算 bootstrap 消息与后台任务。
   - 在首屏可显示的启动流程中调度该后台初始化，完成后刷新 `RuntimeFoundation` 但复用既有 task/question/command runtime。
   - 避免初始化失败阻塞主界面，失败只进入诊断状态并继续启发式估算。

4. 验证与回归
   - 运行格式化和相关 crate 编译。
   - 运行 token/context/GUI 关键测试。
   - 检查启动链路不再同步调用重型 `TokenBudgetRuntime::initialize`。

## 技术选型

- 使用 `Task::perform` 复用 iced 现有异步任务模型，不新增专用线程池或全局阻塞锁。
- `RuntimeFoundation` 默认接收 `TokenBudgetRuntime::pending()`，保证构建基础架构是轻量操作。
- 后台任务完成后通过 `Arc<TokenBudgetRuntime>` 替换运行时基础架构中的预算运行时，所有后续预算路径继续复用同一访问接口。

## 验收标准

- 程序构建 `RuntimeFoundation` 时不再同步 warm-up tokenizer。
- 主界面 Ready 不依赖 Token 预算初始化成功。
- Token 预算初始化完成或失败后，都能在运行时摘要中看到明确状态。
- 请求预算、会话预算和压缩预算仍可在初始化成功后使用真实 token counter，初始化未完成或失败时回退启发式估算。
- 相关格式化、编译和关键测试通过。

## 执行变更总结

### 1. 核心修复与调整概述

- 已将 `RuntimeFoundation::build` 中的同步 `TokenBudgetRuntime::initialize` 改为轻量 `TokenBudgetRuntime::pending`，首屏构建不再执行 tokenizer warm-up。
- 已新增后台初始化入口 `TokenBudgetRuntime::initialize_on_background_thread`，通过 `tokio::task::spawn_blocking` 执行重型 tokenizer 加载。
- 已将 Token 预算初始化接入 LuaSkills 就绪后的启动任务批次，与工作台加载、MCP bootstrap、LuaSkills catalog 加载并行执行。
- 已新增 `TokenBudgetRuntimeInitialized` 消息，在后台初始化完成后替换运行时基础架构中的共享预算运行时。
- 已保留初始化失败回退行为：失败只写入状态与活动日志，不阻塞主界面，也不影响启发式估算兜底。

### 2. 📂文件变更清单（区分新增/修改/删除）

- 修改：`apps/vulcan-code-gui/src/agent_runtime/token_budget_runtime.rs`。
- 修改：`apps/vulcan-code-gui/src/agent_runtime/runtime.rs`。
- 修改：`apps/vulcan-code-gui/src/app_shell/update.rs`。
- 修改：`apps/vulcan-code-gui/src/app_shell/message.rs`。
- 修改：`apps/vulcan-code-gui/src/app_shell/configuration.rs`。
- 修改：`apps/vulcan-code-gui/src/app_shell/mod.rs`。
- 新增：无。
- 删除：无。

### 3. 💻关键代码调整详情（方法变动、核心逻辑更替）

- `TokenBudgetRuntime::pending` 新增轻量占位状态，只解析资源路径，不读取 tokenizer 文件。
- `TokenBudgetRuntimeState` 新增 `Pending`、`Ready`、`Failed` 三态，用于明确运行时状态展示。
- `TokenBudgetRuntime::initialize_on_background_thread` 将原本可能阻塞的 `TokenCounter::warm_up` 放到阻塞工作线程中执行。
- `RuntimeFoundation::build` 默认持有 pending runtime，配置刷新继续复用当前 runtime。
- `refresh_runtime_foundation_with_token_budget_runtime` 支持只替换预算运行时而不清空最近执行准备快照，避免后台完成事件影响正在运行的会话诊断。
- `LuaSkillsBootstrapFinished` 成功后启动 Token 预算后台初始化任务；`TokenBudgetRuntimeInitialized` 完成后回填运行时并记录活动日志。

### 4. ⚠️遗留问题与注意事项

- Token 预算初始化现在不会阻塞主界面，因此启动后极短时间内的预算可能先走启发式估算，后台初始化完成后自动切到真实 token counter。
- 如果用户在后台初始化完成前立即发起请求，本轮设计选择“不等待”，以保证交互优先；真实 API 返回用量仍会校准最终预算。
- 已验证命令：`cargo fmt --package vulcan-code-gui`、`cargo check -p vulcan-code-gui`、`cargo check -p vulcan-token-counter -p vulcan-context -p vulcan-runtime-assets -p vulcan-code-gui`、`cargo test -p vulcan-code-gui runtime_prompt_layers_for_context_compaction_use_developer_layer`、`cargo test -p vulcan-context`、`cargo test -p vulcan-token-counter`、`git diff --check`。
