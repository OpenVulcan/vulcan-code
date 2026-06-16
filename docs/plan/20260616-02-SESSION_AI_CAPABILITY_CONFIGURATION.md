# 会话级 AI 能力配置隔离与默认同步改造计划

## 任务目标

本次任务目标是将 AI 扩展能力配置从当前偏全局的工作台配置模型，调整为“默认配置作为模板、每个会话持有独立配置、单次消息仅做临时覆盖”的三层模型。调整后，同一个工作区内不同会话的 AI 能力开关、目标顺序、目标启用状态互不污染；新会话以默认配置初始化；默认配置不会被会话内调整反向改写；快捷界面提供“使用默认配置”按钮，将当前会话配置一键同步为默认配置。

## 当前问题

1. 当前多处链路直接使用 `agent_runtime.ai_tool_configuration` 或 GUI 的 `self.ai_tool_configuration`，语义更接近全局配置，无法表达每个会话独立配置。
2. 当前会话切换主要切换 transcript/runtime state，但 AI 能力配置展示与执行容易继续引用全局快照，导致会话间顺序、开关状态串台。
3. 默认 AI 能力配置文件既承担默认模板，又承担实际工作台状态，容易被会话级调整污染。
4. 单次消息的 disabled/order override 与会话级持久配置边界不够清晰，需要明确“本次发送覆盖不回写会话配置”。
5. 之前提出的配置 hash 刷新不应放在热路径无条件读写文件，而应以默认配置内存快照为主，只有 hash 变化时刷新默认模板。

## 目标模型

### 1. 默认配置

- 默认配置由现有独立 AI 能力配置文件承载。
- 默认配置只作为新会话初始化模板，以及“使用默认配置”按钮的复制来源。
- 默认配置修改只影响未来新会话，或者用户显式点击同步按钮的当前会话。
- 默认配置不因当前会话快捷开关、目标排序、单次消息覆盖而自动变化。

### 2. 会话配置

- 每个会话在 `SessionRuntimeStateEntry` 或等价会话运行时持久结构中保存自己的 `PersistedAiToolConfiguration`。
- 新会话创建或首次补齐会话运行时状态时，从默认配置 clone 一份作为初始会话配置。
- 切换会话时，GUI/WebUI 展示、快捷开关、目标排序和执行请求都读取当前会话配置。
- 会话内调整 AI 能力开关、目标顺序、目标启用状态时，只持久化当前会话配置。

### 3. 单次消息覆盖

- composer 的临时 disabled capability、disabled target、target order override 只应用于本次提交。
- 提交执行时使用“当前会话配置 + 本次消息覆盖”得到 effective 配置。
- 本次消息覆盖不回写默认配置，也不回写会话配置。

## 技术方案

### 1. 数据结构调整

1. 在会话运行时状态结构中增加可序列化字段，例如：
   - `ai_tool_configuration: Option<PersistedAiToolConfiguration>`
2. 对旧会话兼容：
   - 如果旧会话没有该字段，则在加载/ensure runtime state 时从默认配置 clone 初始化。
   - 初始化后只更新该会话 runtime state，不修改默认配置文件。
3. 保留默认配置字段，但重命名或语义收敛为默认模板：
   - GUI 内部优先区分 `default_ai_tool_configuration` 与 `selected_session_ai_tool_configuration`。
   - Service 内部优先区分 `agent_runtime.default_ai_tool_configuration` 与每个 session runtime state 的配置。

### 2. 默认配置 hash 缓存

1. 默认配置加载时计算稳定 hash，保存在内存快照中。
2. 默认配置保存成功后，同时更新内存快照与 hash。
3. 如需检测外部文件变化：
   - 优先比较 `mtime + len`。
   - metadata 变化后再读文件并计算 hash。
   - hash 不同才 parse、normalize、替换默认配置快照。
4. 禁止在 `runtime_parts_from_state` 等热路径中无条件读写默认配置文件。
5. 禁止持有 service state lock 时执行配置文件 IO。

### 3. GUI 调整

1. AI 能力设置页与快捷界面读取当前选中会话配置。
2. 如果没有选中会话，展示默认配置或禁用会话级操作，并明确按钮状态。
3. AI 能力目标排序、开关、启用状态调整写入当前会话配置，并触发单会话 runtime state 持久化。
4. 默认配置管理入口继续允许编辑默认模板，但不影响已有会话。
5. 快捷界面增加“使用默认配置”按钮：
   - 点击后将默认配置 clone 到当前会话配置。
   - 同步当前快捷开关状态与目标顺序。
   - 持久化当前会话 runtime state。

### 4. WebUI/Service 调整

1. Workbench snapshot 中的 `ai_capabilities`、`ai_capability_vendors`、附件能力等与 AI 能力配置相关摘要，使用当前 selected session 的会话配置计算。
2. start/submit 执行链路使用当前会话配置，再叠加请求中的单次消息覆盖。
3. 会话切换接口完成后，返回的 workbench snapshot 必须体现目标会话配置。
4. WebUI composer 的默认 disabled/order 状态来自当前会话配置摘要，而不是全局默认配置。
5. 增加“使用默认配置”业务接口或复用现有设置保存接口，确保只覆盖当前会话配置。

### 5. 执行链路调整

1. `ExecutionRequest` / runtime launch payload 中传入的 `ai_tool_configuration` 必须来自当前会话 effective 配置。
2. pending prompt、retry、fork、resume、manual compaction 等链路如果会构造执行请求，必须统一走当前会话配置解析函数。
3. 子代理或派生会话的初始配置需要明确来源：
   - 默认采用父会话当前配置 clone。
   - 如果产品语义要求新独立会话，则采用默认配置 clone。

## 详细执行步骤

1. 梳理当前 AI 能力配置读写入口，标记默认配置、会话配置、单次消息覆盖三类职责。
2. 在会话 runtime state 中增加 AI 能力配置字段，并补齐序列化兼容逻辑。
3. 实现默认配置内存快照与 hash/metadata 校验辅助结构。
4. 移除 `runtime_parts_from_state` 热路径无条件刷新默认配置文件的逻辑，改为显式刷新点或 hash 变化刷新。
5. 实现 selected session 配置读取函数：
   - GUI: `selected_session_ai_tool_configuration`
   - Service: `session_ai_tool_configuration`
6. 修改 GUI AI 能力设置页、快捷开关、目标排序、路由策略展示，使其读写当前会话配置。
7. 增加“使用默认配置”按钮和消息处理逻辑。
8. 修改 WebUI snapshot、composer 默认状态、提交执行链路，使其使用当前会话配置。
9. 补充单元测试：
   - 新会话从默认配置初始化。
   - 会话 A 调整不会影响会话 B。
   - 默认配置调整不会覆盖已有会话。
   - 点击“使用默认配置”只覆盖当前会话。
   - 执行请求使用当前会话配置。
   - 单次消息覆盖不回写会话配置。
10. 运行相关验证：
   - `cargo check -p vulcan-agent-runtime`
   - `cargo check -p vulcan-provider-config`
   - `cargo check -p vulcan-code-gui`
   - `cargo check -p vulcan-service-kernel`
   - WebUI 如受影响则运行对应 lint/test。

## 技术选型

1. 会话配置复用现有 `PersistedAiToolConfiguration`，避免新增一套并行 schema。
2. 默认配置 hash 使用稳定序列化内容计算，避免字段顺序造成误判。
3. 会话配置持久化复用现有 `session_runtime_state` 单会话持久化通道。
4. 单次消息覆盖继续复用 `effective_ai_tool_configuration_for_prompt_overrides`，但输入基准从全局配置改为当前会话配置。
5. 外部默认配置变化检测采用 metadata 快速判断 + hash 精确判断的两阶段方案。

## 验收标准

1. 新会话创建后拥有一份从默认配置复制出的独立 AI 能力配置。
2. 切换会话时，AI 能力快捷界面、目标顺序、目标启用状态切换到对应会话配置。
3. 会话 A 修改 AI 能力顺序或开关后，会话 B 不受影响。
4. 修改默认配置不会自动改写已有会话。
5. 点击“使用默认配置”后，仅当前会话配置被默认配置覆盖。
6. 单次消息禁用能力或调整顺序只影响本次执行，不污染会话配置和默认配置。
7. 老会话没有会话级 AI 能力配置时，会自动从默认配置初始化，并在后续持久化为会话配置。
8. `understand_image` 已配置时在老会话中仍能注册为工具，不受旧版普通工具白名单误杀。
9. 默认配置文件不会在执行热路径中被无条件读写。
10. 相关 cargo check/test 通过。

## 风险与注意事项

1. 会话 runtime state schema 变更要保持旧数据兼容。
2. GUI 与 WebUI 可能同时存在配置入口，必须避免一个入口改默认、另一个入口改会话导致语义混淆。
3. 默认配置同步按钮必须文案清晰，避免用户误以为会影响所有会话。
4. 子代理和 fork 会话的配置继承语义需要在实现时统一，否则会出现派生会话与父会话能力不一致。
5. 当前工作区已有未提交修复，实施前需要先提交当前代码作为检查点，降低后续大改回滚成本。

