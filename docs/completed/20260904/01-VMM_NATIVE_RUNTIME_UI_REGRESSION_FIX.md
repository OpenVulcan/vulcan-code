# VMM 原生运行态与界面分离回归修复计划

## 1. 文档状态

- 状态：已完成。
- 目标仓库：`D:\openvulcan\vulcan-code`。
- 历史方案：`docs/completed/20260903/03-VMM_DUAL_INTEGRATION_REVISED.md`。
- 触发原因：真实桌面运行发现原生 VMM 的首页、项目设置、画像、内容维护及 AI 记忆读写仍存在外置语义泄漏或运行失败，证明历史五轮审核未覆盖这些实际入口。
- 审核规则：本计划完成全部修复后重新从零开始连续五轮完整审核；任一轮发现问题即修复并把连续无问题轮次清零。

## 2. 任务目标

1. 首页严格按全局二选一后端显示身份信息：外置 gRPC 显示默认记忆用户与项目；原生仅显示默认项目绑定模式。
2. 项目设置按当前后端显示原生或外置配置，保留原生项目级行为参数覆盖，并移除不适用的外置传输文案。
3. 原生画像只使用单层原生记忆项目目录，不要求或显示外置用户选择，并允许按稳定项目 ID 切换和维护项目画像。
4. 原生记忆内容使用进程内客户端完成搜索、详情、写入和删除；外置模式继续使用远端管理页面。
5. 修复原生 VMM 搜索、主动写入、PreCheck 与 PostAction 共用的 Embedding 请求契约，确保配置阶段即可拒绝不兼容路由。
6. 保持外置 gRPC 的用户、Team/Space/Project、管理 HTTP、迁移与查询级别边界不变。
7. 将 HostAI 契约变化同步到 VCE 任务 `01a05bfd-e8a2-7121-a1da-9336b71a562a`，明确 VMM 与 VCE 向量任务语义隔离。

## 3. 已确认源码事实

1. `NativeVmmClient` 已实现画像、搜索、详情、写入和删除接口，且通过 `process_native_vmm_client()` 暴露进程内实例。
2. `run_vmm_workbench_action` 历史实现无条件创建 `TonicVmmClient`，导致原生管理页动作错误走向外置 gRPC。
3. `MemoryArea` 历史上对原生与外置共用 `VmmDataPage`；该页面的大部分能力依赖外置管理 HTTP。
4. `VmmProfilePage` 历史上只消费外置用户/项目目录，并默认选择用户画像。
5. `NativeVmmHostDriver` 历史上固定请求 `EmbeddingTask::SemanticSimilarity`，而当前内置 Embedding 路由声明 `ProviderDefault`，因此搜索与写入在同一 Effect 链路失败。
6. 项目级 VMM 配置保存会剥离全局传输字段，但会持久化启用、PreCheck、Writeback、AI 工具、TTL、画像刷新、节点上限和注入参数；原生项目归属与连带查询绑定由独立稳定 ID 数据面管理。

## 4. 执行步骤

### 阶段 1：运行时后端分派

- 将工作台动作客户端改为按最终后端选择进程内 `NativeVmmClient` 或外置 `TonicVmmClient`。
- 为原生动作传递稳定 VulcanCode 项目 ID，由原生数据面解析唯一写入归属和连带查询范围。
- 仅对外置层级管理动作保留 ExternalGrpc 门禁；原生画像、内容与健康检查不得被该门禁拦截。
- 外置健康缓存只记录外置探测，不把原生健康结果写入外置端点缓存。

### 阶段 2：Embedding 契约修复

- VMM 写入内容与查询统一固定为对称 `ProviderDefault` 向量任务。
- 固定 Dense、Float 和路由声明的确定默认维度。
- 在 VMM 启用及能力刷新阶段验证完整请求形态，避免首次读写才失败。
- 更新共享 HostAI 契约说明并通知 VCE 任务；VCE 保持 Document/Query 语义。

### 阶段 3：WebUI 模式分离

- 首页按后端隐藏或显示外置默认用户/项目。
- 原生内容页改用内部搜索、详情、写入和删除组件；外置内容页保持远端管理能力。
- 原生画像自动加载原生项目目录、强制项目画像目标、隐藏用户选择，并按稳定数字 ID 调用。
- 项目设置保留通用项目覆盖控件和原生绑定控件，按模式显示准确说明。
- 补充中英文文案，移除原生页面上的外置服务、用户或 gRPC 暗示。

### 阶段 4：定向与全量验证

- Rust：格式、service-runtime/service-kernel 定向检查、VMM/Native VMM/Agent Runtime 定向测试。
- WebUI：TypeScript lint、相关测试、全量 Vitest、生产构建。
- 静态检查：`git diff --check`、精确调用点与后端边界复核。
- 运行证据：在具备真实已配置 Provider 的桌面环境复测搜索、写入、画像与内容维护；若当前自动化环境无法使用用户凭据，明确保留为用户侧验收项，不伪造通过。

### 阶段 5：连续五轮审核

每轮覆盖：

1. 原生/外置架构边界与调用分派。
2. HostAI Embedding 任务、输出、维度、指纹与错误门禁。
3. 项目稳定 ID、唯一写入归属、连带查询范围与画像目标。
4. 首页、项目设置、画像、内容维护与 AI 工具完整暴露链。
5. 外置 gRPC 回归、管理 HTTP、远端迁移和身份配置。
6. 测试、文档、构建、同步边界、工作树与本地提交累计差异。

## 5. 技术选择

1. 继续复用 `VmmClient` 强类型接口，不增加 HTTP、FFI、Go SDK 或通用 JSON 中间层。
2. 原生工作台通过进程内注册客户端调用；外置工作台通过现有 Tonic 客户端调用。
3. 页面以权威 `experienceState.deployment` 分流，不根据错误文本、地址是否为空或目录内容猜测后端。
4. 原生画像目录复用 `native_project_catalog`，绑定与维护全部按稳定项目 ID。
5. Embedding 请求遵守模型能力声明，不做备用任务探测、不静默切换 Provider。

## 6. 验收标准

1. 原生首页不再显示默认记忆用户或外置默认项目字段，只显示固定绑定或按项目独立创建模式。
2. 原生项目设置可保存并生效项目级行为覆盖，且不显示 gRPC/API Key 继承提示；外置设置不显示原生专有字段。
3. 原生画像无需用户目录即可自动列出并切换项目，能读取和应用项目画像自然语言指令。
4. 原生内容页不调用 `management_*` 外置 HTTP 动作，搜索、详情、写入和删除均走进程内客户端。
5. `vmm_memory_search` 与 `vmm_memory_write` 不再因 `SemanticSimilarity` 能力不匹配失败；不兼容路由在启用阶段返回准确错误。
6. 外置 gRPC 原有身份目录、远端项目、管理内容页和迁移行为通过回归测试。
7. 所有本任务修改和阶段性本地提交均纳入最终累计差异审核。
8. 连续五轮完整审核无新增问题后，追加执行变更总结并将本文件迁移到 `docs/completed/20260904/01-VMM_NATIVE_RUNTIME_UI_REGRESSION_FIX.md`。

## 7. 当前执行记录

- 连续无问题审核轮次：5/5。
- 并行工作边界：VCE 任务正在修改其专属数据面、组合根、Provider 配置与工作区监听文件；本任务不编辑、不暂存、不还原这些变更，并在最终工作区门禁前等待其稳定节点。
- 已完成：运行时客户端分派、宿主项目身份传递、外置健康缓存隔离、ProviderDefault 请求与激活门禁、首页分流、原生内容页分流、原生画像项目目录、项目设置模式文案、宿主取消传播、共享契约通知、阶段性本地提交及四轮连续无问题审核。
- 待完成：真实桌面重建重启后的用户侧 Provider 验收；该项依赖用户本机已配置凭据，不以自动化环境伪造通过。

## 8. 执行变更总结

### 8.1 核心修复与调整概述

1. 原生与外置后端从首页、项目设置、内容维护、画像、工作台动作和 AI 工具入口开始严格分流；原生不再显示或调用外置用户、层级目录和管理 HTTP。
2. 原生搜索、主动写入、PreCheck 与 PostAction 的向量请求统一改为宿主支持的对称 `ProviderDefault + Dense + Float`，并在启用阶段验证默认维度和能力声明。
3. 原生画像固定使用单层项目目录与稳定数字 ID，切换项目后加载完整画像；自然语言修正成功后重新读取全部有效节点和最终预览。
4. 每个原生命令均携带独立宿主 AI 取消信号；调用方提前结束时后台 Worker 继续接收受控取消结果并释放 Core 命令状态。
5. 外置 gRPC 的用户、Team/Space/Project、固定前缀、管理服务和远端迁移边界保持不变；VMM 与 VCE 不同的向量任务语义已经同步给并行任务。

### 8.2 📂文件变更清单

- 修改：`crates/vulcan-service-kernel/src/native_vmm/driver.rs`，修复向量请求契约、能力门禁及取消传播。
- 修改：`crates/vulcan-service-kernel/src/vmm/{config.rs,memory_actions.rs,project_bindings.rs,snapshot.rs,tests.rs}`，完成后端门禁、原生动作、稳定项目身份与状态投影。
- 修改：`crates/vulcan-service-runtime/src/vmm_management.rs`，让工作台管理动作按最终后端选择进程内或 Tonic 客户端。
- 修改：`apps/vulcan-code-web/src/components/features/memory/`、`apps/vulcan-code-web/src/components/project/project-settings-sections.tsx`、工作区记忆设置与国际化消息，完成原生首页、项目设置、画像、内容页和导航分流。
- 修改：`docs/completed/20260903/04-VMM_HOST_AI_PORT_CONTRACT_HANDOFF.md`，冻结 VMM 与 VCE 各自的向量任务语义。
- 修改：`docs/reviews/20260904-vmm-native-integration-review.md`，记录真实运行回归、修复证据和重新执行的五轮审核。
- 新增归档：本计划由 `docs/plan/` 迁移至 `docs/completed/20260904/`；无运行时代码文件删除。

### 8.3 💻关键代码调整详情

1. `NativeVmmHostDriver::execute_embedding` 固定请求 `EmbeddingTask::ProviderDefault`、Dense、Float 与能力声明的确定默认维度；`embedding_config_from_capability` 在首次 Effect 前拒绝不兼容路由。
2. `NativeVmmCoreRuntime::run` 使用请求级取消信号和后台 Worker；`effect_call_context` 将同一信号附加到 LLM、Embedding、Rerank 的宿主上下文。
3. `run_vmm_workbench_action`、原生工具连接器和 VMM Hook 均根据全局后端选择 `process_native_vmm_client()` 或 `TonicVmmClient`，不再从地址或错误文本猜测后端。
4. 原生内容维护复用内部 `VmmClient` 强类型搜索、详情、写入、删除接口；画像通过 `native_project_catalog` 和稳定 ID 读取、切换与修正。
5. 后端状态投影只在外置模式暴露默认用户/项目及远端管理能力；原生模式暴露默认绑定方式、单层项目空间和项目级行为覆盖。

### 8.4 ⚠️遗留问题与注意事项

1. 截图中的 `SemanticSimilarity` 字样只能由修复前二进制产生。Rust 源码变更必须重新构建并重启桌面应用；`make sync` 只同步运行时资源，不会更新 VMM Rust 代码。
2. 自动化环境没有用户桌面中的真实 Provider 凭据，因此未声称完成真实供应商往返；重启后应分别复测 `vmm_memory_write`、`vmm_memory_search`、自动 PostAction 与画像自然语言修正。
3. 工作区同时包含独立 VCE 任务提交；VMM 审核没有修改或暂存其文件。最终 workspace all-targets 与格式检查通过；严格生产库 Clippy 的 VMM 路径无告警，VCE 自身的新告警已单独通知其任务处理。
