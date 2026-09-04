# VCE 失败关闭配置闭环实施计划

## 任务目标

将 VCE 从“固定注册空推理 Profile、项目侧直接显示并在运行时报错”修正为完整的宿主配置闭环：

1. 在插件管理范围提供 VCE 内置能力设置入口，统一复用 VulcanCode 公共 AI 的 LLM、Embedding、Rerank 路由与测试能力。
2. 建立单一、后端权威的 VCE 就绪判定；所需配置缺失、所选模型不存在、模型被禁用或能力不匹配时，VCE 一律保持关闭。
3. 只有 VCE 全局配置就绪时，项目设置才显示代码上下文入口，项目运行时才允许启动和建立索引。
4. 模型配置失效后立即停止 VCE 项目运行时、文件监听和工具执行，不保留隐式降级路径。
5. 保持 VCE 仅消费上游 HostAI 与 VLDB/LanceDB 服务，不创建独立模型、向量数据库或第二套推理配置协议。
6. 代码检索查询改写属于机器内部检索阶段，固定关闭模型推理；不向用户增加推理档位，并拒绝无法明确关闭推理的 LLM 路由。

## 详细执行步骤

1. 梳理公共 AI 推理 Profile、插件页、项目设置、VCE 服务协调、文件监听和原生工具暴露的当前调用链。
2. 在服务端增加 VCE 全局配置状态模型与权威校验函数，校验已配置路由能够解析为当前真实、启用且能力兼容的模型。
3. 增加 VCE 全局配置查询接口，并让公共 AI 配置变更触发 VCE 全项目重新协调；配置无效时停止现有运行时和监听。
4. 在插件管理页增加“内置能力”中的 VCE 设置卡，复用公共 AI 候选模型、路由保存以及 Embedding/Rerank 真实请求测试接口；LLM 使用与生产请求相同的关闭推理投影做启动前校验。
5. 调整项目设置导航，仅在后端确认 VCE 全局就绪后显示代码上下文入口；直接服务调用仍由后端再次校验并失败关闭。
6. 调整项目启用、刷新、管理查询和代码检索工具入口，确保全局配置无效时均拒绝执行且不会建立索引；工具还必须匹配当前项目修订、向量指纹、可用生命周期和已提交检查点。
7. 补充 Rust 与前端测试，覆盖配置缺失、模型不存在、能力不匹配、配置恢复、项目入口显示及运行时关闭。
8. 执行格式化、定向 Rust 检查与测试、前端测试和类型检查；核对 Git 差异并仅提交本任务文件。

## 技术选型

- 使用现有 `PersistedAiConfiguration` 的 `vce` Consumer Profile 作为唯一持久化模型配置，不新增 VCE 私有 Provider 配置。
- 使用现有 `PublicAiRuntime::resolve_internal_inference_selection` 与能力快照验证真实模型存在性和兼容性。
- 使用现有公共 AI 路由保存、候选模型和非 LLM 测试 RPC；VCE 使用独立选择器表达自身三项必需契约，不依赖 VMM 私有界面组件。
- 对查询改写请求显式发送关闭推理的标准值，并在就绪校验中验证所选 LLM 的能力声明及实际请求投影都支持关闭推理。
- 使用服务端发布的 VCE 全局状态作为项目入口与运行时门控权威，前端不自行猜测模型有效性。
- 保留项目级索引参数和启用意图，但运行态始终由全局配置状态失败关闭；不静默复制 VMM、知识库或聊天模型。

## 验收标准

1. 未配置 VCE 模型时，插件页显示未就绪，项目设置不显示代码上下文，项目运行时与监听不启动。
2. 任一启用的 VCE 路由引用不存在、禁用或能力不兼容的模型时，整体状态为关闭并返回明确诊断。
3. 配置完整且模型有效后，项目设置出现代码上下文入口，用户可显式启用并建立索引。
4. 已运行期间模型配置失效后，相关项目运行时和监听被停止，检索工具无法继续执行。
5. 模型恢复后仍通过同一公共 AI Profile 与 HostAI 契约工作，不产生新的配置格式或隐式回退。
6. VCE 与 VMM 的配置、数据命名空间和当前并行修改保持隔离。
7. 定向 Rust 测试、前端测试、类型检查及格式检查全部通过。
8. VCE 查询改写请求始终固定关闭模型推理；不能可靠关闭推理的模型不会使 VCE 进入就绪状态。

## 执行变更总结

### 1. 核心修复与调整概述

- 在插件管理页增加 VCE 内置能力卡，提供 Embedding、查询改写 LLM、Rerank 三条公共 AI 路由的选择、状态诊断、清除与非 LLM 真实请求测试。
- 新增后端权威全局就绪状态，基于当前公共 AI 配置、模型目录、能力快照、Embedding 物理指纹、结构化输出以及关闭推理请求投影执行全有或全无校验。
- 将三类 AI 能力均改为 VCE 启动前必需项；查询改写固定发送 `ReasoningEffort::None`，不提供用户推理档位，并复用统一宿主推理请求边界清理冲突参数。
- 公共 AI 每次成功发布后异步重协调全部 VCE 项目；配置失效时停止运行时与文件监听，配置恢复后按持久化项目意图重新启用，Embedding 指纹变化触发索引重建。
- 项目代码上下文入口仅在后端全局状态就绪时显示；AI 检索工具还要求当前项目修订、Embedding 指纹、可用生命周期与已提交检查点全部匹配，并在检索前后重复校验。
- 保留管理与恢复能力：全局模型配置无效时拒绝管理调用；配置有效但项目运行失败时仍允许查看作业、维护和执行重试。

### 2. 📂文件变更清单

- 新增：
  - `apps/vulcan-code-web/src/components/features/skills/vce-built-in-plugin-panel.tsx`
  - `apps/vulcan-code-web/src/components/features/skills/vce-built-in-plugin-panel.test.ts`
  - `apps/vulcan-code-web/src/components/project/project-settings-utils.test.tsx`
  - `crates/vulcan-service-kernel/src/vce/readiness.rs`
- 修改：
  - `apps/vulcan-code-web/src/components/features/code-context/code-context-area.test.tsx`
  - `apps/vulcan-code-web/src/components/features/skills/SkillsArea.tsx`
  - `apps/vulcan-code-web/src/components/project/project-settings-dialog.tsx`
  - `apps/vulcan-code-web/src/components/project/project-settings-utils.tsx`
  - `apps/vulcan-code-web/src/service/vce-service.ts`
  - `apps/vulcan-code-web/src/types/vce.ts`
  - `crates/vulcan-agent-runtime/src/tooling/bridge.rs`
  - `crates/vulcan-agent-runtime/src/tooling/bridge/tests.rs`
  - `crates/vulcan-context-engine/docs/vulcancode-native-integration.md`
  - `crates/vulcan-service-api/src/vce.rs`
  - `crates/vulcan-service-kernel/src/public_ai_settings/publication.rs`
  - `crates/vulcan-service-kernel/src/rpc_dispatch.rs`
  - `crates/vulcan-service-kernel/src/rpc_dispatch/vce.rs`
  - `crates/vulcan-service-kernel/src/vce/ai_port.rs`
  - `crates/vulcan-service-kernel/src/vce/manager.rs`
  - `crates/vulcan-service-kernel/src/vce/mod.rs`
  - `crates/vulcan-service-kernel/src/vce/services.rs`
  - `crates/vulcan-service-kernel/src/webui_prompt_lifecycle/precheck_continue.rs`
- 删除：无。

### 3. 💻关键代码调整详情

- `vce_configuration_readiness` 成为配置状态、项目协调、项目界面与工具准入共同使用的唯一权威校验入口。
- `VceHostAiAdapter` 注册的 LLM、Embedding、Rerank 要求全部标记为必需；查询改写请求使用严格 JSON Schema、显式关闭推理且禁止推理加密内容。
- `VceRuntimeProjectStatus` 增加 Embedding 物理指纹；公共 AI 变化时仅 Embedding 契约变化需要替换运行时和重建索引，LLM/Rerank 继续通过实时 HostAI 能力刷新消费最新路由。
- `codebase_retrieval` 在无可服务 VCE 处理器时从模型工具集合完全移除，同时保留公共工具标识以阻止 Provider 同名碰撞。
- VCE 配置卡使用公共 AI 的唯一 `vce` Consumer Profile；索引继续由宿主 VLDB Controller 的 SQLite/LanceDB 数据面持有，没有新增 VCE 私有数据库或 Provider 配置。
- 验证结果：Rust 三 crate 定向 `cargo check` 通过；VCE 内核定向测试 29 项通过；工具桥测试 9 项通过；服务 API VCE 测试 3 项通过；Web 全量 145 个测试文件、874 项测试通过；TypeScript 构建检查通过；`git diff --check` 通过。

### 4. ⚠️遗留问题与注意事项

- 未使用真实用户凭据执行付费 Provider 端到端调用；界面保留 Embedding/Rerank 真实请求测试按钮，LLM 在启动前执行与生产编码器相同的关闭推理请求投影校验，真实查询改写会在项目检索时发生。
- VCE 的显式关闭推理依赖本轮与 VMM 统一的共享 HostAI 请求契约；该共享实现由并行 VMM 任务维护，本提交不暂存或覆盖其文件。
- 未进行桌面壳人工点击验收；前端界面已通过全量组件测试与 TypeScript 构建检查。
