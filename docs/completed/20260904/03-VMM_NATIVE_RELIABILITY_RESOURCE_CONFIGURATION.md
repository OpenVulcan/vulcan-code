# VMM 原生可靠性与资源配置完善计划

## 1. 文档状态

- 状态：已完成并归档。
- 目标仓库：`D:\openvulcan\vulcan-code`。
- 前置归档：`docs/completed/20260904/01-VMM_NATIVE_RUNTIME_UI_REGRESSION_FIX.md`。
- 本轮范围：仅处理前置归档后由真实桌面验证新增发现的原生 VMM 问题；前置归档保持为当时源码与验证事实，不覆盖其历史记录。
- 审核规则：本计划实现完成后，从零开始连续五轮完整审核；任一轮发现问题并修复后，连续无问题轮次归零。

## 2. 任务目标

1. 修复既有会话编辑首条用户消息时，VMM 活跃会话无法获得精确 canonical user-message ID 的问题。
2. 修复原生 PostAction 分析连续失败的错误分类和宿主重试衔接，并验证结构化 JSON 输出不会混入模型思考文本。
3. 所有原生 VMM LLM 用途只允许选择明确支持关闭思考的模型，并在执行时固定为关闭思考；不得把 reasoning 流合并进 JSON。
4. 在原生 VMM 设置中补齐语言、PII 区域、Noise 组合与全部提示词资源的查看、独立覆盖、恢复和即时生效能力。
5. 用户覆盖文件保存在宿主数据目录 `.vulcan/code/vmm/config` 下，不覆盖应用内置资源；无覆盖时展示并使用内置原文。
6. 为 PII 提供使用 Core 权威正则、条件公式和替换语义的测试器，并保留现行 Noise 规则测试能力。
7. 原生画像同时保留固定单一用户画像与多个项目画像；项目画像按稳定项目 ID 切换，用户画像不要求用户选择。
8. 继续保持原生与外置 gRPC 的界面、配置、维护和能力边界分离。
9. 将共享宿主 AI 契约的任何变化主动同步到 VCE 任务 `01a05bfd-e8a2-7121-a1da-9336b71a562a`。

## 3. 技术选择

1. 既有会话编辑从权威 V1 来源投影解析位置派生的 canonical user-message ID，重建控制器状态后复用该 ID；禁止追加重复用户消息或生成替代 ID。
2. PostAction L1 仍是必需阶段。提供方或传输失败不伪装为成功降级，而是保留精确分析续体供持久队列重试；结构化契约错误继续封闭失败。
3. VMM 的 PreCheck、PostAction L1/L2、用户画像、项目画像及预留用途统一采用固定关闭思考策略。保存时过滤不支持显式关闭思考的路由，执行时强制 `ReasoningEffort::None` 并移除与该请求字段冲突的 Manifest 参数。
4. 使用宿主现有结构化输出聚合器只收集响应正文；reasoning 仅保留为独立事件，不进入 JSON 解析输入。
5. Core 内置 Prompt、PII、Noise 继续作为编译期默认资源；宿主资源仓库只保存用户覆盖与选择元数据，并在每次 Native Core 运行前激活最新有效资源快照。
6. 覆盖目录按资源类型与语言/区域分层，文件逐项独立编辑；保存使用同一配置根内的原子替换，拒绝路径逃逸、重解析点和非法规则。
7. PII 公式测试直接调用 Core 配置 crate 的同一编译器、条件求值器和替换展开器，禁止另写近似前端算法。
8. 原生管理动作通过进程内 Native VMM 客户端执行；外置管理动作继续通过其远端协议执行，二者不互相兜底。

## 4. 执行步骤

### 阶段 1：会话身份与 PostAction 可靠性

- 串联 V1 来源事件序号、Provider 投影位置、控制器 canonical ID 与工作台 prompt-turn 输入。
- 在运行态草稿提交前校验既有 ID 必须存在且对应用户文本。
- 为既有消息复用、来源 ID 解析和 PostAction L1 失败续体增加直接回归测试。
- 核对失败后的数据库状态、重试次数和最终提取状态不会被误记为成功。

### 阶段 2：统一关闭思考与结构化输出

- 扩展 VMM inference profile，按用途保存明确的关闭思考档位。
- UI 仅列出支持关闭思考的 LLM 路由，且只显示“关闭”档位。
- 宿主每次 VMM LLM 调用固定投影关闭思考，并移除与请求字段冲突的模型参数。
- 增加 reasoning/text 双流测试，证明 JSON 只来自最终正文并继续通过调用方 Schema 校验。
- 同步共享契约变化到 VCE 任务，但不替 VCE 决定其业务用途是否必须关闭思考。

### 阶段 3：资源覆盖与规则测试

- 提供 Native VMM 资源目录、当前语言/区域、内置原文、用户覆盖状态与有效内容的类型化 API。
- 实现逐文件保存、恢复与热加载；高风险 Prompt 编辑采用两次明确确认后解锁保存。
- 提供语言、PII 区域和 Noise 区域/组合选择界面。
- 提供 PII 当前规则集测试及自定义公式测试；保留 Noise 当前规则测试。
- 为路径安全、完整资源包、内置回退、覆盖恢复、PII 条件公式和 UI 渲染增加测试。

### 阶段 4：原生画像与维护界面复核

- 固定展示一个 Native 用户画像目标，不要求用户选择。
- 项目画像列出所有原生记忆项目并按稳定 ID 读取与修正。
- 内容页按记忆项目与宿主会话显示提取状态、各级记忆数量、失败诊断及该会话生成/写入的记忆。
- 核对原生页面不显示外置管理服务不可用、Team/Space/User 目录等远端专属语义。

### 阶段 5：验证、提交与五轮审核

- 执行相关 Rust crate 定向测试、检查与严格 Clippy。
- 执行 VMM WebUI 测试、TypeScript lint 与生产构建。
- 执行 `git diff --check`、格式检查、资源同步边界检查和脏工作树所有权复核。
- 允许按稳定边界创建中文结构化本地提交；所有本轮提交必须纳入最终累计 diff 审核。
- 完成后执行连续五轮完整审核并记录每轮覆盖与结果。

## 5. 验收标准

1. 编辑既有会话第一条消息可正常开始终端回合，VMM 获得该持久消息的精确 canonical ID，且会话历史不产生重复用户消息。
2. PostAction 提供方失败不再报 `flow state has no degradation path`；状态保留为可重试，最终成功或失败均有准确记录。
3. 所有 VMM LLM 用途只能保存支持关闭思考的模型并固定关闭思考；reasoning 不进入 JSON。
4. 原生设置可选择语言、PII 区域和 Noise 组合，可查看全部内置资源原文并逐文件覆盖或恢复。
5. Prompt 编辑默认锁定；用户经过两次高风险确认后才可编辑和保存。
6. 用户覆盖仅写入 `.vulcan/code/vmm/config`；无覆盖时使用内置资源，保存/恢复后下一次命令即时使用新快照。
7. PII 公式测试结果与生产 Core 条件求值及替换逻辑一致；非法公式返回明确错误。
8. 原生画像同时显示固定用户画像和可切换项目画像；原生内容维护不依赖外置管理服务。
9. 外置 gRPC 原有身份、前缀、远端维护和迁移边界无回归。
10. 所有本轮改动及阶段提交经连续五轮完整审核无新增问题，并在计划末尾追加执行变更总结后归档。

## 6. 当前执行记录

- 连续无问题审核轮次：5/5。
- 已完成：既有 V1 用户消息身份复用链路、PostAction L1 失败续体、VMM 全用途关闭思考策略、reasoning/JSON 隔离、Native 资源覆盖 API 与界面、PII 公式测试器、用户/项目画像分流及直接回归测试。
- 已完成：完整 VMM 范围静态审查、全量测试、并行工作边界拆分、本地提交及连续五轮无新增问题审核。
- 运行边界：自动化环境未伪造用户 Provider 凭据；真实桌面旧数据证明此前失败请求缺少 Provider 侧 JSON 模式，修复后仍需用户重启新构建进行一次最终交互确认。

## 7. 执行变更总结

### 7.1 核心修复与调整概述

1. 既有会话编辑与排队后继现在从权威 V1 来源投影获得准确 canonical user-message ID，并复用已持久消息，修复原生 VMM 终态回合启动失败。
2. PostAction L1 提供方失败保留准确 Core 续体，由宿主持久队列重试；不再错误报告为缺少降级路径。
3. 原生 VMM 全部 LLM 用途只接受支持关闭思考的模型，配置与执行统一固定 `ReasoningEffort::None`；reasoning 流与 JSON 正文完全分离。
4. OpenAI、Gemini、Google Code Assist、Cohere 协议获得 Provider 侧 JSON Object 约束，Anthropic 保留精确 JSON Schema；宿主继续执行调用方 Schema 终验。
5. 增加 Native Prompt、PII、Noise 内置原文查看、用户覆盖、恢复、组合选择、即时激活及 PII/Noise 测试器。
6. 恢复固定单一用户画像与多项目画像切换；增加按项目、会话和回合查看提取状态、四级记忆数量、失败诊断及生成记忆详情的维护界面。
7. 完成连续五轮累计审核，并修复终审发现的测试构造缺字段及 Provider 删除后的孤立思考配置。

### 7.2 📂文件变更清单

- 新增：`crates/vulcan-service-kernel/src/native_vmm/resources.rs`，原生资源仓库、覆盖安全边界与测试。
- 新增：`apps/vulcan-code-web/src/components/features/memory/vmm-native-resources-page.tsx` 及测试，原生资源设置与规则测试界面。
- 新增：`apps/vulcan-code-web/src/components/features/memory/vmm-native-memory-records.tsx` 及测试，原生项目/会话/回合/记忆来源界面。
- 新增：`docs/reviews/20260904-vmm-native-reliability-resource-review.md`，累计审核台账与五轮结果。
- 修改：`crates/vulcan-memorymesh-core-config`、`vulcan-memorymesh-core-engine`、`vulcan-memorymesh-core`，开放内置资源读取与 PII 权威测试，修复 L1 失败续体。
- 修改：`crates/vulcan-provider-adapters`、`vulcan-service-api`、`vulcan-service-kernel`，补齐用途级思考契约、结构化调用、原生维护 API、资源激活和 canonical ID 链路。
- 修改：`apps/vulcan-code-web` 的记忆设置、画像、内容、类型与国际化文件，完成原生/外置分流及新维护能力。

### 7.3 💻关键代码调整详情

- `SharedPromptTurnBeginInput` 新增既有 canonical ID 输入；工作台归约器校验该 ID 必须存在且是用户文本，并禁止“追加新消息”和“复用既有消息”同时发生。
- Provider V1 投影按权威来源序号唯一解析位置派生消息 ID；Root 与 Queued 分发在开始回合前用同一投影重建控制器状态。
- `InferenceProfile` 增加用途级 `llm_reasoning_efforts`；VMM 保存、资格校验和实际请求均只接受 `None`，删除路由或 Provider 时同步清理附属字段。
- 受限 LLM 聚合器分别保存正文与 reasoning；结构化值只从正文或 Provider 原生结构化项生成，再按调用方 Schema 验证。
- 原生资源仓库使用清单白名单、文件大小限制、普通目录/重解析点检查、同目录原子替换和隔离 Core 预编译；Prompt 首次编辑物化完整 Bundle。
- 原生维护投影从一致 Core 快照与活动分析作业生成项目、会话、回合、记忆来源数据；迁移修改归属身份而不重建向量，清空删除全部项目级信息并保留固定个人画像。

### 7.4 ⚠️遗留问题与注意事项

- 需要用户使用新构建重启桌面后，以已配置真实 Provider 再执行一次“编辑旧会话首条消息”和“新会话画像提取”交互验收；自动化测试没有伪造真实凭据。
- L2 提供方失败时，Core 按既有可靠语义把 L1 画像候选保留为 `Pending`，不会把未经评审的候选显示为活跃画像；修复后的 JSON 模式用于显著降低进入该路径的概率。
- Provider 适配器全量测试中的既有 Anthropic 模型发现 mock 在整套运行时出现过第二页连接竞态，单项串行复跑通过；该测试及实现不在本任务改动范围。
- 本轮没有修改 `crates/vulcan-runtime-assets/resources/`，无需执行 `make.ps1 sync`。原生 Core 内置资源由 Core crate 编译，用户覆盖位于宿主 VMM 数据目录的 `config` 子目录。
