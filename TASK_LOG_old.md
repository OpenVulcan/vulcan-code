# 持续代码治理记录

本文件记录每轮代码治理任务的事实探索、执行流程、长期优化方案、验证结果、代码审核与自动修复情况。

治理原则：以当前需求与长期可维护性为准，不为历史兼容保留冗余分支；任何修改必须先查明真实数据来源和完整调用链。

## 第 1 轮：拆分 ComposerPanel 目标生命周期控制器（2026-07-10）

### 问题与事实依据

- `apps/vulcan-code-web/src/components/composer/composer-panel.tsx` 原始文件共 1035 行，核心 `ComposerPanel` 函数覆盖约 949 行，同时承担草稿、附件、模型、推理档位、能力配置、目标生命周期、Plan 模式与提交控制等多类职责。
- 目标功能在父组件内独占 5 组状态和多组异步回调，但视觉组件已经位于 `composer-panel/goal-dialog.tsx` 与 `composer-panel/goal-strip.tsx`，形成“视图已拆、控制逻辑仍堆积”的不完整分层。
- Web 前端当前没有测试运行器；可用的权威自动验证入口为 `npm run lint`（TypeScript 严格类型检查）和 `npm run build`（TypeScript + Vite 生产构建）。

### 已查明的执行流程

1. `WorkbenchLayout` 从 `useWorkbenchSessionActions` 取得目标保存、继续、清理和状态更新处理器。
2. 处理器依次经 `WorkspaceShell`、`ChatArea`、`ChatAreaComposerDockLayer` 和 `ComposerDock` 透传到 `ComposerPanel`。
3. `ComposerPanel` 负责目标草稿校验、Plan 模式互斥、目标持久化调用，以及新建目标成功后的首轮自动提交。
4. `useWorkbenchSessionActions` 调用 `conversation-actions-service` 中的 `saveSessionGoal`、`resumeSessionGoal`、`clearSessionGoal` 和 `updateSessionGoalStatus`。
5. 服务成功响应通过 `applySessionAddonsChangedResponse` 合并回工作台快照，新的 `sessionGoal` 再沿上述组件链返回输入区渲染。

### 长期优化调整

- 新增 `use-composer-goal-controller.ts`，集中持有目标弹窗状态、输入校验、保存/继续、暂停/恢复和清理操作。
- 保留 `ComposerPanel` 对输入区特有协作的所有权：Plan 模式互斥准备，以及新目标创建成功后的首轮提示词提交。
- 通过显式的 `UseComposerGoalControllerOptions` 与 `ComposerGoalController` 契约隔离持久化边界和视图消费边界，未增加历史兼容分支或多来源兜底。
- `ComposerPanel` 核心函数由约 949 行降至约 897 行，目标生命周期的 5 组本地状态已全部移出父组件。

### 文件变更

- 新增：`apps/vulcan-code-web/src/components/composer/composer-panel/use-composer-goal-controller.ts`
- 修改：`apps/vulcan-code-web/src/components/composer/composer-panel.tsx`
- 修改：`TASK_LOG.md`
- 删除：无

### 修改范围代码审核与自动修复

- 审核发现新控制器的保存回调若依赖整个 options 对象，会在每次父组件渲染时重建；已自动收窄为实际读取的目标、布尔状态和回调依赖。
- 审核发现父组件新增协作回调沿用了宽泛 `props` 依赖；已自动改为 `conversationId`、`onSetSessionPlanMode` 与 `onSubmitPrompt` 等真实依赖。
- 审核确认 20 个符号仅存在于 import 行，是此前拆分子组件后遗留的无效导入；已全部清理。
- 对比原实现确认：目标空文本和预算校验、编辑与继续分流、Plan 模式互斥、保存成功关闭弹窗、首次目标自动提交、失败回填草稿、暂停/恢复和清理语义均保持一致。

### 验证结果

- 基线 `npm run lint`：通过。
- 修改后 `npm run lint`：通过。
- `npm run build`：通过，Vite 成功转换 2349 个模块并生成生产产物。
- `git diff --check`：通过，未发现空白错误。
- CodeKit AST 复查：`ComposerPanel` 与新控制器均能正常解析，新文件包含 1 个控制器函数、2 个接口和 1 个模式类型。

### 遗留问题与下一轮方向

- `ComposerPanel` 仍是约 897 行的超大函数，后续应继续按已证实的职责边界拆分草稿会话同步、模型/推理选择或 slash 指令控制，禁止仅为降行数进行无语义拆分。
- 生产构建仍提示主工作台 chunk 超过 500 kB；该警告并非本轮引入，后续需单独追踪实际依赖图和加载路径后再决定代码分割方案。
- `TASK_LOG.md` 被仓库现有 `.gitignore` 明确忽略，本轮保持该既有版本控制策略不变。

## 第 2 轮：收口会话草稿槽与附件提交事务（2026-07-10）

### 问题与事实依据

- `ComposerPanel` 在调用 `useComposerAttachments` 取得附件状态后，又自行维护活动草稿键、文本镜像 ref、附件镜像 ref 和恢复标记 ref，形成两个状态所有者之间的手工同步。
- 父组件通过 1 个 `useLayoutEffect` 与 2 个 `useEffect` 负责旧槽保存、新槽恢复和状态持久化；发送流程还直接改写这些内部 ref 与草稿存储，导致持久化不变量泄漏到业务提交逻辑。
- 发送失败恢复不是普通覆盖：请求飞行期间如果用户已经输入新文本或添加新附件，必须保留新内容，只回填仍为空的状态维度。

### 已查明的执行流程

1. `composerSessionDraftKey` 对已存在会话生成 `conversation:{conversationId}`，对新会话生成 `new:{projectId}` 或工作区默认槽键。
2. `readComposerSessionMemoryDraft` 从模块内 Map 读取草稿，并克隆每个附件对象；找不到键时返回空草稿。
3. `writeComposerSessionMemoryDraft` 同样克隆附件后写入；文本与附件都为空时直接删除该槽。
4. `useComposerAttachments` 是附件状态、文件校验、暂存、粘贴和拖拽处理的唯一真实所有者。
5. 会话槽切换使用 `useLayoutEffect`：先保存旧槽，再恢复新槽，并在浏览器绘制前重置 slash 高亮、slash 关闭状态和媒体预览。
6. 普通文本与附件变化分别通过 effect 更新活动槽；发送前同步清空状态与存储，发送失败时按“保留飞行期间新编辑、只补空缺”的规则合并原提交内容。

### 长期优化调整

- 新增 `use-composer-session-draft.ts`，在一个组合 hook 内调用附件 hook，并统一管理草稿键、槽恢复、持久化镜像和提交事务。
- 向父组件暴露 `clearForSubmission`、`restoreFailedSubmission` 和 `replaceDraft` 三个具有业务语义的操作，父组件不再直接读写存储或内部 ref。
- `replaceDraft` 在编辑排队提示词时同步更新文本、附件与持久化镜像，消除原先依赖两个被动 effect 才最终一致的窗口。
- 组合 hook 的公开状态显式排除底层 `setAttachments`，避免未来调用方绕过草稿事务边界。
- `ComposerPanel` 核心函数由第 1 轮后的约 897 行继续降至约 837 行；从最初约 949 行累计减少约 112 行。

### 文件变更

- 新增：`apps/vulcan-code-web/src/components/composer/composer-panel/use-composer-session-draft.ts`
- 修改：`apps/vulcan-code-web/src/components/composer/composer-panel.tsx`
- 修改：`apps/vulcan-code-web/src/components/composer/composer-attachments.ts`
- 修改：`TASK_LOG.md`
- 删除：无

### 修改范围代码审核与自动修复

- 审核发现组合 hook 最初继承并暴露了 `setAttachments`，会破坏新建立的事务边界；已改为从公开接口排除原始 setter，仅在 hook 内部使用。
- 审核发现 `useComposerAttachments` 的文件暂存回调依赖整个 args 对象；已收窄到 `conversationId`、`projectId` 与 `onStageAttachmentBytes` 等真实依赖。
- 审核发现本轮改动涉及的发送和待发送提示词编辑回调仍依赖整个 props 对象；已收窄为实际读取的回调和状态字段。
- 使用 CodeKit 反向搜索确认：`ComposerPanel` 已不再出现草稿存储读写、草稿镜像 ref 或 `setAttachments`，底层细节只保留在附件 hook、草稿 helper 与组合 hook 内。
- 对比原实现确认：初始恢复、切槽前保存、切槽后恢复、空草稿删除、发送前清空、失败时分维度合并新旧内容的语义均保持。

### 验证结果

- 修改后首次 `npm run lint`：通过。
- 自动修复后 `npm run lint`：再次通过。
- `npm run build`：通过，Vite 成功转换 2350 个模块并生成生产产物。
- `git diff --check`：通过，未发现空白错误。
- CodeKit AST 复查：组合 hook 正常解析为 1 个函数和 2 个接口；`ComposerPanel` 核心函数边界缩减到约 837 行。

### 遗留问题与下一轮方向

- `ComposerPanel` 仍同时拥有 slash 指令、模型与推理选择、能力配置和全局键盘输入等职责；下一轮应基于调用关系选择一个完整状态域继续抽离。
- `useComposerAttachments` 内部仍直接负责用户提示与文件暂存循环，未来如继续治理，应先确认是否存在复用或独立测试需求，不能为了形式分层拆散单一流程。
- 生产构建仍存在既有主工作台大 chunk 警告，本轮未改变其加载策略。

## 第 3 轮：隔离模型与推理选择并修复过期回滚（2026-07-10）

### 问题与事实依据

- `ComposerPanel` 同时维护模型键、模型选择器打开信号、推理档位和推理选择器打开信号，并负责 Provider 快照解析、二级路由目标解析、推理选项回退、服务同步、乐观更新和失败回滚。
- `handleSelectWorkbenchModel` 与 `handleSelectWorkbenchReasoningEffort` 通过 `settings-actions-service` 请求桌面服务，成功后把新 `WorkbenchSnapshot` 合并回工作台；异常时返回 `false`，由输入区回滚本地值。
- 原失败回滚没有请求身份：多个选择请求重叠时，较早请求后失败可能覆盖较新的用户选择。
- 原同步 effect 同时依赖权威值和本地选择值；本地乐观更新会触发 effect，并立即被尚未更新的旧服务快照覆盖，实际无法稳定呈现乐观结果。

### 已查明的执行流程

1. `modelPickerChoicesFromProviders` 将 Provider 模型列表扁平化并确定性排序，模型行的 `active` 标记来自工作台服务快照。
2. 当前模型键优先取服务标记的活动模型，没有活动标记时取排序后的首个模型；二级路由控件再用该键唯一解析 Provider 与模型行。
3. 模型或推理选择通过 `WorkspaceShell`、`ChatArea`、`ComposerDock` 传入的回调进入 `useWorkbenchConversationActions`。
4. 服务层分别发送 `service_select_workbench_model` 与 `service_select_workbench_reasoning_effort` 请求，返回的新工作台快照成为后续权威值。
5. 推理选项优先使用后端 `reasoningSelection.options`；本地回退选项仅用于界面预览。只有后端选项明确包含当前值时，才把推理档位放入提示词提交载荷。

### 长期优化调整

- 新增 `use-composer-model-selection.ts`，集中管理模型与推理的权威快照同步、乐观状态、二级路由目标解析、选择器打开信号和提交安全值。
- 为模型与推理请求分别增加单调请求序号，只有最新失败请求可以回滚当前本地值，旧请求失败不会覆盖新选择。
- 模型权威同步改为只在活动模型键实际变化时执行；本地选择变化本身不再触发旧快照覆盖。
- 推理权威同步改为只在后端选中值或可选值签名变化时执行，并通过函数式状态更新保留仍然有效的本地选择。
- `ComposerPanel` 核心函数由第 2 轮后的约 837 行继续降至约 747 行；从最初约 949 行累计减少约 202 行。

### 文件变更

- 新增：`apps/vulcan-code-web/src/components/composer/composer-panel/use-composer-model-selection.ts`
- 修改：`apps/vulcan-code-web/src/components/composer/composer-panel.tsx`
- 修改：`TASK_LOG.md`
- 删除：无

### 修改范围代码审核与自动修复

- 审核复现出原同步 effect 会在乐观选择后用旧服务值立即反向覆盖；已将模型同步依赖限定为活动模型键，将推理同步依赖限定为后端选中值与可选值签名。
- 审核确认简单失败回滚存在重叠请求竞争；已增加模型和推理各自独立的请求版本 ref，仅允许最新请求回滚。
- 审核发现 slash 指令分派回调仍依赖整个 props 对象；已收窄为实际使用的 `onSlashCommand` 及控制器动作。
- CodeKit 反向搜索确认模型和推理状态 setter、权威值同步及请求版本只存在于新控制器，父组件仅消费公开值与动作。
- 复核保持了无服务运行时的预览语义：服务回调返回成功但没有新快照时，本地选择可继续保留。

### 验证结果

- 首版 `npm run lint`：通过。
- 自动修复后 `npm run lint`：再次通过。
- `npm run build`：通过，Vite 成功转换 2351 个模块并生成生产产物。
- `git diff --check`：通过，未发现空白错误。
- CodeKit AST 复查：新控制器正常解析为 1 个函数和 3 个接口；`ComposerPanel` 核心函数边界缩减到约 747 行。

### 遗留问题与下一轮方向

- `ComposerPanel` 剩余最大状态域是 slash 指令过滤、键盘导航、激活分派和全局键入重定向；该流程与 Plan 模式切换存在耦合，下一轮必须先完整确认键盘事件优先级与命令返回协议。
- 模型选择器本身会再次把 Provider 列表扁平化；是否共享派生列表需要结合组件 API 与渲染性能证据决定，不能仅因计算重复就扩大接口。
- 生产构建仍存在既有主工作台大 chunk 警告，本轮未改变加载策略。


## 第 4 轮：收口 slash 指令状态与键盘优先级（2026-07-10）

### 问题与事实依据

- `ComposerPanel` 直接维护 slash 高亮索引和被关闭输入值，并负责指令过滤、精确匹配、高亮收敛、Plan 特例、工作台路由结果、模型/推理面板打开以及文本框键盘优先级。
- slash 临时状态还被草稿槽切换回调反向重置，使草稿持久化 hook 知道上层 slash 与媒体预览 UI，形成不必要的反向依赖。
- 全局键入监听器依赖被关闭输入值，每次按 Esc 或重新输入都会重新解绑和绑定 document 级 `keydown` 监听器。

### 已查明的执行流程

1. 输入必须以 `/` 开头；去掉斜杠并规范化后，对每个命令触发词做前缀匹配。Esc 只关闭当前精确输入值，文本变化后重新启用匹配。
2. 可见面板中，上下方向键循环移动高亮；Esc 关闭；非 Shift、非输入法组合态的 Enter 优先激活精确命令，没有精确命令时激活高亮命令。
3. 只有 slash 面板未消费 Enter 时，普通输入区发送逻辑才执行；Shift+Enter 与输入法组合态继续保留换行/组合语义。
4. `/plan` 是输入区本地特例，通过持久化 Plan 模式回调切换；其余命令交给 `WorkbenchLayout` 路由。
5. 工作台路由协议包含 `handled`、可选 `replacementText` 和可选 `openTarget`；当前支持打开模型/推理选择器、插入 skill 提示词、打开技能面板和触发上下文压缩。
6. 草稿槽切换先在组合草稿 hook 的布局 effect 中恢复新槽；父组件随后在同一绘制前阶段重置 slash 临时状态与媒体预览。

### 长期优化调整

- 新增 `use-composer-slash-commands.ts`，集中管理筛选、高亮、关闭状态、激活分派、路由结果和文本框键盘优先级。
- 把 Plan 特例与普通工作台指令路由统一放入控制器，父组件只提供 `onSetPlanMode`、`onRouteCommand` 与面板打开动作。
- 将全局键入对 slash 状态的影响收口为 `appendGlobalInput`，父组件只保留工作区焦点与阻塞弹窗判断。
- 草稿 hook 不再接收 UI 重置回调，改为暴露稳定 `draftSlotKey`；父组件通过布局 effect 组合 slash 重置与媒体预览重置，消除下层对上层 UI 的认知。
- `ComposerPanel` 核心函数由第 3 轮后的约 747 行继续降至约 648 行；从最初约 949 行累计减少约 301 行。

### 文件变更

- 新增：`apps/vulcan-code-web/src/components/composer/composer-panel/use-composer-slash-commands.ts`
- 修改：`apps/vulcan-code-web/src/components/composer/composer-panel/use-composer-session-draft.ts`
- 修改：`apps/vulcan-code-web/src/components/composer/composer-panel.tsx`
- 修改：`TASK_LOG.md`
- 删除：无

### 修改范围代码审核与自动修复

- 审核发现 `appendGlobalInput` 和普通输入更新最初捕获关闭状态，会导致 document 监听器随状态反复重建；已改用函数式状态更新，使全局监听器保持稳定。
- 审核发现传入 slash 控制器的 `handlePlanModeChange` 依赖整个 props 对象；已收窄为 `conversationId`、`onSetSessionPlanMode`、翻译函数和可见目标。
- CodeKit 反向搜索确认匹配、精确触发和指令规范化仅在模型 helper 与新控制器中执行，父组件只保留控制器动作别名和 JSX 绑定。
- 对比原实现确认方向键循环、Esc 关闭、精确命令优先、高亮回退、输入法组合保护、Shift+Enter 换行、Plan 异步切换和工作台路由结果语义均保持。

### 验证结果

- 首版 `npm run lint`：通过。
- 自动修复后 `npm run lint`：再次通过。
- `npm run build`：通过，Vite 成功转换 2352 个模块并生成生产产物。
- `git diff --check`：通过，未发现空白错误。
- CodeKit AST 复查：slash 控制器正常解析为 1 个函数和 2 个接口；`ComposerPanel` 核心函数边界缩减到约 648 行。

### 遗留问题与下一轮方向

- `ComposerPanel` 剩余主要复杂度集中在能力配置同步、发送/停止编排、Plan 模式与目标互斥，以及大段 JSX 组装；下一轮应优先选择仍泄漏跨状态不变量的业务域。
- 全局键入重定向仍属于父组件，因为它依赖 DOM 焦点、阻塞弹窗和文本输入元素判断；若未来抽离，必须保持 document 监听器唯一性和焦点判定顺序。
- 生产构建仍存在既有主工作台大 chunk 警告，本轮未改变加载策略。

## 第 5 轮：统一 AI 能力配置状态与提交协议（2026-07-10）

### 问题与事实依据

- `ComposerPanel` 直接维护禁用能力类型、禁用目标选择器和目标顺序三类状态，并在服务快照同步、弹窗保存成功、普通提示词发送和目标首轮提交之间手工转换。
- 能力弹窗保存协议 `ComposerAiCapabilityConfigurationSave` 与 `ComposerSubmission` 中的三项能力字段完全一致，但父组件仍在两个提交点重复执行 Set 转数组和 Map 转 Record。
- 原同步仅依赖能力签名；当切换到签名相同的另一会话时，本地预览态修改不会重置。旧会话飞行中的保存完成后，也可能把结果写入已经切换的新会话本地状态。

### 已查明的执行流程

1. 工作台服务投影 `AiCapabilitySummary[]`，其中能力级可用/启用状态决定禁用能力集合，目标级启用状态决定禁用选择器集合，目标数组顺序决定路由顺序 Map。
2. `aiCapabilitySignature` 覆盖能力类型、可用、启用、原生/桥接标记，以及每个目标的选择器、启用状态和模型标识，用于过滤无关流式快照。
3. 能力弹窗提交完整的禁用类型数组、禁用目标数组和目标顺序 Record；`useWorkbenchSessionActions` 将其发送到 `service_save_session_ai_capability_configuration`。
4. 保存成功后的工作台快照成为后端权威状态；输入区同时镜像请求值，保证快照到达前的本地提交一致性。
5. 普通发送与目标首轮提交都把同一三字段能力配置放入 `ComposerSubmission`。

### 长期优化调整

- 新增 `use-composer-ai-capabilities.ts`，集中管理三类本地状态、权威快照签名同步、弹窗保存镜像和提交协议序列化。
- 控制器直接产出 `submissionConfiguration`，普通发送和目标首轮提交通过对象展开复用同一协议来源。
- 使用草稿槽键作为能力状态作用域；即使两个会话能力签名相同，切换会话也会按新会话快照重置。
- 保存开始时捕获作用域键，完成后只有当前仍是同一作用域才允许写回本地状态，避免旧会话异步结果污染新会话。
- `ComposerPanel` 核心函数由第 4 轮后的约 648 行继续降至约 621 行；从最初约 949 行累计减少约 328 行。

### 文件变更

- 新增：`apps/vulcan-code-web/src/components/composer/composer-panel/use-composer-ai-capabilities.ts`
- 修改：`apps/vulcan-code-web/src/components/composer/composer-panel.tsx`
- 修改：`TASK_LOG.md`
- 删除：无

### 修改范围代码审核与自动修复

- 审核发现只依赖能力签名无法区分配置相同的不同会话，预览态可能跨会话泄漏；已将稳定草稿槽键加入同步条件。
- 审核发现保存请求跨会话完成时可能写回错误的当前本地状态；已用活动作用域 ref 和请求作用域键阻断过期写回。
- CodeKit 反向搜索确认能力状态 setter、签名计算和提交序列化只存在于能力控件、模型 helper 与新控制器；父组件只消费控制器公开状态和提交配置。
- 对比原实现确认能力默认派生、签名过滤、保存失败不镜像、保存成功即时镜像以及普通/目标提交字段语义保持一致。

### 验证结果

- 首版 `npm run lint`：通过。
- 自动修复后 `npm run lint`：再次通过。
- `npm run build`：通过，Vite 成功转换 2353 个模块并生成生产产物。
- `git diff --check`：通过，未发现空白错误。
- CodeKit AST 复查：能力控制器正常解析为 1 个函数和 2 个接口；`ComposerPanel` 核心函数边界缩减到约 621 行。

### 遗留问题与下一轮方向

- `ComposerPanel` 剩余复杂度主要是发送/停止编排、Plan 模式与目标互斥、待发送提示词恢复、全局键入以及 JSX 组装；下一轮应评估发送事务是否还能形成独立边界。
- 能力签名沿用现有分隔符拼接实现；当前没有证据表明选择器或模型标识会包含冲突分隔符，因此本轮不做猜测式编码变更。
- 生产构建仍存在既有主工作台大 chunk 警告，本轮未改变加载策略。

## 第 6 轮：统一发送/停止控制器与跨槽失败恢复（2026-07-10）

### 问题与事实依据

- `ComposerPanel` 同时编排普通发送、空草稿停止、显式停止、目标首轮提交、本地重复门控、草稿清空和失败恢复，普通发送与目标发送重复拼装同一运行时配置。
- 原本地门控只使用 React state；同一渲染提交 state 前的快速重复触发可能同时通过检查。
- 原失败恢复使用当前活动草稿 ref。发送后如果用户切换会话，旧请求失败会把旧文本与附件恢复到新会话，造成跨会话污染。
- 目标保存等待期间允许用户发起普通发送；普通发送快速完成而后端运行快照尚未投影时，原目标自动提交仍可能再次发送。

### 已查明的执行流程

1. 输入区发送先计算去空白文本和附件数量；运行中且草稿为空时，发送按钮语义为停止，不消费草稿。
2. 运行中且草稿有文本或附件时，工具栏同时展示停止按钮和发送按钮；发送仍进入 `service_submit_conversation_prompt`，由后端运行时决定排队/处理。
3. 普通发送在请求前清空文本、附件和内存草稿槽；服务返回 ack 后，新建会话会切换到 ack 中的会话标识。
4. 服务不可用、拒绝或异常被 `handleSubmitConversationPrompt` 转换为 `false`；输入区必须恢复精确文本与附件，同时保留请求飞行期间产生的较新编辑。
5. 停止首先请求 `ask_if_running_subagents`；存在运行中子智能体时由用户确认停止范围，再发送最终停止模式并合并后端工作台快照。
6. 新目标保存成功后仅在没有现有后端运行和本地提交时自动发送目标文本；失败时应回填到目标所属草稿槽。

### 长期优化调整

- 新增 `use-composer-send-controller.ts`，统一普通发送、空草稿停止、显式停止、目标首轮提交、运行时配置拼装和失败恢复。
- 使用同步 `submittingRef` 与 React 可见状态双层门控，阻止 state 提交前的同 tick 重复发送。
- 引入单调提交版本；目标保存期间只要发生过任何竞争提交，即使该提交已完成，也不再执行旧目标自动提交。
- 草稿清空现在返回 `ComposerDraftSubmissionSnapshot`，携带原始草稿槽键、精确文本和克隆附件。
- 失败恢复在原槽仍活动时按维度合并当前编辑；原槽非活动时读取该槽最新持久化内容并合并回原槽，不触碰当前会话 UI。
- 目标文本失败恢复使用显式槽键；目标弹窗本身也加入槽作用域，切换会话会关闭旧弹窗并阻断旧保存结果的关闭/自动提交副作用。
- `ComposerPanel` 核心函数由第 5 轮后的约 621 行继续降至约 544 行；从最初约 949 行累计减少约 405 行。

### 文件变更

- 新增：`apps/vulcan-code-web/src/components/composer/composer-panel/use-composer-send-controller.ts`
- 修改：`apps/vulcan-code-web/src/components/composer/composer-panel/use-composer-session-draft.ts`
- 修改：`apps/vulcan-code-web/src/components/composer/composer-panel/use-composer-goal-controller.ts`
- 修改：`apps/vulcan-code-web/src/components/composer/composer-panel.tsx`
- 修改：`TASK_LOG.md`
- 删除：无

### 修改范围代码审核与自动修复

- 审核发现普通发送失败会按当前活动 ref 恢复，导致跨会话污染；已改为带槽键事务快照，并在非活动原槽中合并最新持久化内容。
- 审核发现目标首轮失败同样可能回填错误槽；已增加 `restoreInputToSlot`，按显式目标槽恢复。
- 审核发现目标弹窗异步准备和保存完成可能跨会话生效；已为目标控制器增加活动作用域校验和切槽重置。
- 审核发现 React state 无法同步拦截快速重复提交；已增加同步 ref 门控。
- 审核发现目标保存期间完成一次普通发送后仍可能自动发送目标；已增加提交版本校验，任何后发提交都会使旧自动提交资格失效。
- 审核发现发送回调依赖整个 options 对象；已收窄为实际读取的字段、事务操作和后端回调。
- CodeKit 反向搜索确认 `onSubmitPrompt`、草稿清空与失败恢复只存在于发送控制器，父组件仅消费公开发送/停止动作。

### 验证结果

- 首版 `npm run lint`：通过。
- 自动修复后 `npm run lint`：再次通过。
- `npm run build`：通过，Vite 成功转换 2354 个模块并生成生产产物。
- `git diff --check`：通过，未发现空白错误。
- CodeKit AST 复查：发送控制器正常解析为 1 个函数、1 个接口和 1 个设置类型；草稿快照与目标作用域接口正常解析；`ComposerPanel` 核心函数边界缩减到约 544 行。

### 遗留问题与下一轮方向

- `ComposerPanel` 剩余复杂度主要是 Plan 模式状态、全局键入、待发送提示词恢复和 JSX 组装；下一轮应检查 Plan 模式与目标互斥是否仍有跨会话状态泄漏。
- 停止流程的用户取消当前仍由上层返回 `false`，输入区继续沿用既有失败提示语义；是否区分取消与失败需要后端返回协议和产品文案依据，不能在输入区猜测。
- 生产构建仍存在既有主工作台大 chunk 警告，本轮未改变加载策略。

## 第 7 轮：统一 Plan 模式来源并修复跨草稿泄漏（2026-07-10）

### 问题与事实依据

- `ComposerPanel` 同时维护后端 `sessionPlanMode`、新会话 `draftPlanModeEnabled` 和独立 `executionMode`，界面开关与提交模式来自不同状态源。
- 已存在会话切换成功后，父组件立即更新 `executionMode`，但 `planModeEnabled` 仍等待后端快照，导致提交已进入新模式而界面仍显示旧模式。
- 新会话 Plan 布尔值没有草稿槽作用域；在两个没有 conversation id 的项目草稿间切换时会直接泄漏。
- 持久化切换允许重叠请求，较早响应可能在较新用户选择之后发布本地结果。

### 已查明的执行流程

1. 已存在会话通过 `service_set_session_plan_mode` 持久化开关；服务返回 `SessionAddonsChangedResponse`，再合并回工作台的 `sessionPlanMode`。
2. 新会话没有可持久化 conversation id，`handleSetSessionPlanMode` 会拒绝，因此输入区必须使用草稿本地状态直到首次提交实体化会话。
3. `executionMode` 随每条 `ComposerSubmission` 发送，后端以 `plan` 或 `build` 决定本轮执行方式。
4. Plan 模式与未完成目标互斥；启用 Plan 时必须拒绝目标冲突，打开目标编辑器时必须先成功关闭 Plan。
5. `/plan`、附件菜单开关、Plan chip 关闭和 Shift+Tab 都应进入同一个模式转换入口。

### 长期优化调整

- 新增 `use-composer-plan-mode.ts`，统一草稿本地状态、后端权威快照、成功持久化后的短期乐观覆盖、目标互斥和快捷键。
- 删除独立 `executionMode` state，提交模式直接从生效 Plan 值派生为 `plan` 或 `build`，消除界面/提交分裂。
- 新会话本地状态绑定 `draftSlotKey`；切换到另一草稿槽时默认回到 Build，不再继承上一项目的 Plan 值。
- 已存在会话的成功转换使用作用域化乐观覆盖，直到权威快照确认同一值后自动清理。
- 持久化请求使用单调版本与活动作用域双重校验，只有当前槽的最新请求可以发布乐观覆盖。
- 目标编辑准备、slash 命令、菜单开关、chip 关闭和 Shift+Tab 全部复用 `setPlanMode`。
- `ComposerPanel` 核心函数由第 6 轮后的约 544 行继续降至约 488 行；从最初约 949 行累计减少约 461 行。

### 文件变更

- 新增：`apps/vulcan-code-web/src/components/composer/composer-panel/use-composer-plan-mode.ts`
- 修改：`apps/vulcan-code-web/src/components/composer/composer-panel.tsx`
- 修改：`TASK_LOG.md`
- 删除：无

### 修改范围代码审核与自动修复

- 审核确认原本地 Plan 布尔值在不同新会话草稿间泄漏；已按稳定草稿槽键隔离并在新槽默认关闭。
- 审核确认独立 `executionMode` 会与后端 Plan 快照短暂分裂；已删除镜像 state，改为从唯一生效值直接派生。
- 审核确认重叠持久化请求没有最新结果约束；已增加请求版本和作用域校验。
- 审核发现控制器向外公开了仅供内部快捷键使用的 `togglePlanMode`；已从公共接口移除，缩小调用面。
- CodeKit 反向搜索确认父组件不再存在 `setExecutionMode`、`draftPlanModeEnabled` 或其 setter，只消费 Plan 控制器的生效值和动作。
- 对比原实现确认目标冲突提示、新会话本地切换、已存在会话持久化、目标编辑前关闭、slash 切换和 Shift+Tab 阻止冒泡语义保持。

### 验证结果

- 首版 `npm run lint`：通过。
- 自动修复后 `npm run lint`：再次通过。
- `npm run build`：通过，Vite 成功转换 2355 个模块并生成生产产物。
- `git diff --check`：通过，未发现空白错误。
- CodeKit AST 复查：Plan 控制器正常解析为 1 个函数和 3 个接口；`ComposerPanel` 核心函数边界缩减到约 488 行。

### 遗留问题与下一轮方向

- `ComposerPanel` 现主要承担控制器组合、待发送提示词恢复、全局键入、附件预览和 JSX；下一轮应判断待发送提示词恢复与全局键入是否值得抽离，或转向其他高复杂度文件获得更高收益。
- 新会话 Plan 状态目前只在当前草稿槽生命周期内保留，切换离开后按 Build 重置；若未来需要跨项目保留，必须先定义与草稿文本同等级的明确持久化协议。
- 生产构建仍存在既有主工作台大 chunk 警告，本轮未改变加载策略。

## 第 8 轮：隔离能力弹窗草稿生命周期与保存并发（2026-07-10）

### 问题与事实依据

- `ComposerAiCapabilityControl` 单函数约 479 行，内部维护打开、脏关闭确认、当前能力、三类草稿配置、基线签名、保存反馈和外部更新提示共 9 组状态。
- 会话切换时弹窗不会关闭；旧会话脏草稿可继续使用新会话的 `onSaveConfiguration` 回调保存，形成跨会话配置污染。
- `isSaving` 仅为 React state，同一 state 提交前的快速双击可能发出重复保存请求。
- “保存并关闭”使用未追踪的 220ms `setTimeout`；重新打开、切换会话、卸载或立即产生新草稿后，旧定时器仍可能关闭错误弹窗。

### 已查明的执行流程

1. 打开弹窗时从会话同步的禁用能力 Set、禁用目标 Set 与目标顺序 Map 深拷贝草稿，并捕获稳定 JSON 基线签名。
2. 能力/目标开关和拖拽排序只修改本地草稿；使用默认配置同样只暂存，不立即持久化。
3. 关闭脏弹窗必须进入确认页，可选择放弃、继续编辑或保存后关闭；保存后关闭保留 220ms 成功反馈。
4. 保存将三类草稿序列化为 `ComposerAiCapabilityConfigurationSave`，经父级能力控制器持久化并同步会话快照。
5. 弹窗打开且草稿干净时，外部会话快照变化自动刷新草稿；草稿脏时保留本地编辑并显示外部更新提示。
6. 目标拖拽通过 dnd-kit 解析活动/目标索引，再把完整选择器顺序交给草稿层替换。

### 长期优化调整

- 新增 `use-composer-capability-draft.ts`，集中管理弹窗打开/关闭、草稿状态、基线、脏检查、外部快照、保存反馈和修改操作。
- `ComposerAiCapabilityControl` 新增真实 `scopeKey`，由 `ComposerPanel` 传入 `draftSlotKey`；切换会话在布局阶段关闭旧弹窗、失效旧请求并恢复新会话快照。
- 使用同步 `savingRef` 阻止 React state 提交前的重复保存。
- 保存请求携带作用域键与单调版本，旧会话或旧版本结果不能修改当前弹窗。
- 使用单一 `closeTimerRef` 跟踪保存后关闭定时器，在重新打开、普通关闭、放弃、切槽、卸载及任何新草稿修改时统一取消。
- 视图组件只保留当前右侧能力选择、派生展示数据、拖拽事件适配和 JSX；草稿 setter 与保存副作用不再泄漏。
- `ComposerAiCapabilityControl` 核心函数由约 479 行降至约 327 行。

### 文件变更

- 新增：`apps/vulcan-code-web/src/components/composer/use-composer-capability-draft.ts`
- 修改：`apps/vulcan-code-web/src/components/composer/composer-capability-controls.tsx`
- 修改：`apps/vulcan-code-web/src/components/composer/composer-panel.tsx`
- 修改：`TASK_LOG.md`
- 删除：无

### 修改范围代码审核与自动修复

- 审核发现会话切换不会隔离弹窗草稿与保存结果；已增加作用域布局重置、活动作用域 ref 和保存版本校验。
- 审核发现 `isSaving` 无法同步阻止双击；已增加同步保存门控。
- 审核发现旧关闭定时器在重新打开、切槽和卸载后仍生效；已集中追踪并清理。
- 审核发现保存并关闭后的反馈窗口内仍可产生新修改，旧定时器会关闭新脏草稿；已让默认值、能力开关、目标开关和排序入口全部取消待执行关闭。
- CodeKit 反向搜索确认草稿 setter、保存版本、关闭定时器和外部更新状态只存在于新控制器，视图仅消费公开状态与动作。
- 对比原实现确认打开时重置、脏关闭确认、保存反馈、默认配置暂存、开关、排序、干净外部刷新和脏外部提示语义保持。

### 验证结果

- 首版 `npm run lint`：通过。
- 自动修复后 `npm run lint`：再次通过。
- `npm run build`：通过，Vite 成功转换 2356 个模块并生成生产产物。
- `git diff --check`：通过，未发现空白错误。
- CodeKit AST 复查：能力草稿控制器正常解析为 1 个函数、2 个接口、1 个状态类型和 3 个纯 helper；视图核心函数缩减到约 327 行。

### 遗留问题与下一轮方向

- 能力草稿控制器本身约 337 行但职责单一；后续只有在出现独立复用或测试边界时才继续拆分，不能机械搬移代码。
- `ComposerAiCapabilityControl` 的 JSX 仍较长，可在视觉结构稳定后按左右面板与 footer 组件边界拆分，但当前优先级低于其他跨状态问题。
- 生产构建仍存在既有主工作台大 chunk 警告，本轮未改变加载策略。

## 第 9 轮：隔离 ChatArea 思考展示与跨界面视觉过渡（2026-07-10）

### 问题与事实依据

- `ChatArea` 单函数约 972 行，包含会话派生、滚动恢复、虚拟化、Plan/ask 决策、输入区、思考展示偏好、思考浮层退场和空会话提示退场等多类职责。
- 思考展示模式采用乐观更新，但快速连续切换时，较早请求后失败会直接回滚较新的用户选择。
- 切换会话时，旧会话的思考浮层仍按 500ms 淡出、1000ms 卸载逻辑保留，可能在新会话界面短暂显示旧内容。
- 空会话提示与思考浮层各自维护状态和 timer，但都缺少明确会话/草稿作用域。

### 已查明的执行流程

1. 后端工作台快照提供全局 `thinkingDisplayMode`，输入区菜单调用 `service_select_workbench_thinking_display_mode` 持久化选择并合并新快照。
2. 展示模式决定当前流式思考块是否进入浮动面板，以及哪些思考块 id 从虚拟化主转录中隐藏。
3. 实时浮层消失后先保留 500ms，再进入淡出，1000ms 后卸载；内容签名变化时替换保留块。
4. 空会话转为非空时，提示进入 360ms 退场后卸载；重新进入空会话则立即恢复。
5. `ChatArea` 的持久化会话以 conversation id 标识；新会话草稿以选中项目 id 区分。

### 长期优化调整

- 新增 `chat-area/visual-state.ts`，分别提供 `useThinkingDisplayMode` 与 `useConversationVisualTransitions` 两个职责单一的 hook。
- 展示模式持久化使用单调请求版本；只有最新失败请求可以回滚，旧完成结果不能覆盖新选择。
- 使用 pending 模式保护：较早服务快照到达时，如果与最新乐观值不一致则暂不覆盖；服务不可用但回调成功的预览语义继续保留本地值。
- 视觉过渡使用 `conversationId` 或项目草稿键作为 `surfaceKey`；切换界面在布局阶段立即重置旧浮层与空提示状态，避免旧会话内容进入新界面绘制。
- 浮层与空提示 timer 通过 effect cleanup 在内容、空状态、作用域变化和卸载时统一清理。
- `ChatArea` 核心函数由约 972 行降至约 917 行。

### 文件变更

- 新增：`apps/vulcan-code-web/src/components/conversation/chat-area/visual-state.ts`
- 修改：`apps/vulcan-code-web/src/components/conversation/chat-area.tsx`
- 修改：`TASK_LOG.md`
- 删除：无

### 修改范围代码审核与自动修复

- 审核发现快速模式选择缺少请求身份；已增加请求版本、最新权威值 ref 和 pending 值保护。
- 审核发现旧思考浮层在切换会话后继续执行退场；已增加界面键并在绘制前重置。
- 审核发现浮层 effect 同时依赖完整保留块与其 id，造成重复触发源；已删除冗余 id 派生与依赖。
- CodeKit 反向搜索确认思考模式、浮层和空提示 setter 只存在于新 hook，`ChatArea` 仅消费生效状态和动作。
- 对比原实现确认浮层内容签名替换、500ms 淡出起点、1000ms 卸载、空提示 360ms 退场和无服务预览语义保持。

### 验证结果

- 首版 `npm run lint`：通过。
- 自动修复后 `npm run lint`：再次通过。
- `npm run build`：通过，Vite 成功转换 2357 个模块并生成生产产物。
- `git diff --check`：通过，未发现空白错误。
- CodeKit AST 复查：视觉状态文件正常解析为 2 个 hook 和 4 个接口；`ChatArea` 核心函数边界缩减到约 917 行。

### 遗留问题与下一轮方向

- `ChatArea` 最大剩余风险仍是约 500 行的滚动/布局恢复域，包含多界面快照、锚点恢复、隐藏思考布局变更与多帧修正；下一轮应把该域作为整体继续追踪。
- 新视觉 hook 只负责展示偏好与过渡，不承担转录隐藏算法；隐藏规则继续保留在已独立、纯函数化的 `thinking-display.ts`。
- 生产构建仍存在既有主工作台大 chunk 警告，本轮未改变加载策略。

## 第 10 轮：收口会话界面布局记忆并删除死版本状态（2026-07-10）

### 问题与事实依据

- `ChatArea` 直接持有每界面 `Map`，同时负责存活标识收集、过期记录清理、默认记录创建、滚动快照、虚拟列表状态和展开状态写入，与具体 DOM 恢复策略交叉。
- `ConversationSurfaceLayoutRestoreState.layoutVersion` 每次写入都自增，但经 CodeKit 与全局文本反向搜索确认没有任何读取点，不参与恢复、诊断或持久化，属于纯死状态。
- 恢复键的唯一来源已确认：普通会话使用 `conversation.id`，子代理界面使用 `parentConversationId` 路由下的任务 id；未发现多路候选字段或需要兼容的旧结构。

### 已查明的执行流程

1. `ChatArea` 从工作台全部会话 id 与当前子代理导航任务 id 构造权威存活界面集合。
2. 用户滚动、布局即将变更、`ResizeObserver`、思考行隐藏和输入区高度变化会捕获共享滚动容器的锚点或底部距离。
3. `ConversationWorkspace` 以确定的 `surfaceRestoreId` 上报进程折叠、用户提示词和工具卡展开状态；虚拟列表 hook 以同一作用域键上报行高与渲染窗口。
4. 切换界面前先把当前 DOM 视口写回旧键；React 切换到新界面但恢复流程尚未接管时，明确拒绝把旧 DOM 位置误写到新键。
5. 新界面按键读取滚动、虚拟列表和展开快照；有锚点时优先恢复锚点，否则恢复底部距离，并用多帧修正等待虚拟测量稳定。
6. 工作台删除会话或子代理任务消失后，布局记忆按权威存活集合清理过期键。

### 长期优化调整

- 新增 `chat-area/surface-layout-memory.ts`，用 `useConversationSurfaceLayoutMemory` 唯一持有运行期恢复注册表。
- 对外只暴露按维度类型化的 `read`、`writeScroll`、`writeVirtualizer` 和 `writeExpandedState`，不再向组件泄漏通用 Map 更新器。
- hook 集中创建中性记录、不可变替换和存活键清理；虚拟列表与展开状态在引用未变时跳过无效写入。
- `ChatArea` 只保留界面切换期间的业务安全守卫与具体 DOM 快照策略，布局记忆存储不再与多帧恢复编织在同一函数中。
- 直接删除无消费者的 `layoutVersion` 字段及自增逻辑，不为无实际契约的历史形状保留兼容层。
- `ChatArea` 核心函数由第 9 轮后的约 917 行继续降至约 872 行。

### 文件变更

- 新增：`apps/vulcan-code-web/src/components/conversation/chat-area/surface-layout-memory.ts`
- 修改：`apps/vulcan-code-web/src/components/conversation/chat-area.tsx`
- 修改：`apps/vulcan-code-web/src/components/conversation/conversation-layout-restore.ts`
- 修改：`TASK_LOG.md`
- 删除：无

### 修改范围代码审核与自动修复

- 审核确认恢复注册表必须只有一个所有者；已删除父组件内的重复 Map、清理 effect 和通用更新回调。
- 审核确认 `layoutVersion` 全局无读取点；已删除类型字段、默认值与写入自增，CodeKit 再搜索为零。
- 审核确认新 hook 对首次写入使用完整中性记录，不会丢失其他恢复维度。
- 自动修复了虚拟列表与展开状态相同引用的无效 Map 替换，并补齐新函数的中英文参数、返回值和关键变量注释。
- 对比原实现确认：存活键范围、切换期误写守卫、滚动锚点/底部距离恢复、虚拟行高和展开状态语义保持。

### 验证结果

- 首版 `npm run lint`：通过。
- 自动修复后 `npm run lint`：再次通过。
- `npm run build`：通过，Vite 成功转换 2358 个模块并生成生产产物。
- `git diff --check`：通过，未发现空白错误。
- CodeKit AST 复查：新布局记忆文件正常解析为 1 个 hook 和 2 个接口；`ChatArea` 核心函数边界缩减到约 872 行。

### 遗留问题与下一轮方向

- 布局记忆的存储边界已独立，但 `ChatArea` 仍持有用户滚动抑制、布局变更捕获、隐藏思考恢复、输入区高度修正和界面切换恢复等多类时序；下一轮应继续评估是否能按“视口协调器”完整边界抽离。
- 新 hook 是 WebUI 运行期内存，不承诺跨应用重启恢复；未有明确持久化协议前不应猜测后端字段。
- 生产构建仍存在既有主工作台大 chunk 警告，本轮未改变加载策略。

## 第 11 轮：删除旧虚拟化死链与重复展开状态（2026-07-10）

### 问题与事实依据

- `conversation-virtualization.tsx` 保留了 434 行虚拟窗口、行高测量、恢复窗口和用户滚动抑制实现，但全仓反向搜索确认 `useConversationTimelineVirtualWindow` 与 `ConversationTimelineVirtualRow` 均为零调用。
- `ConversationWorkspace` 声明 `initialVirtualizerState` 与 `onVirtualizerStateChange` props，却从未读取或调用；主会话仍传入完整管道，侧栏子代理还专门创建 no-op 回调只为满足死 props。
- 当前实际转录渲染将全部 `turnGroups` 直接映射为 `ConversationTurnShell`，没有虚拟窗口、前后占位或虚拟行包装。
- `ConversationSurfaceExpandedState.expandedById` 只在保存时从 `measuredCollapseById[*].expanded` 重复生成，全仓无读取点；实际恢复使用的唯一数据是 `measuredCollapseById`。

### 已查明的执行流程

1. `ConversationWorkspace` 先把可见时间线条目按用户轮次分组，然后对全部分组执行 `turnGroups.map`。
2. `ConversationTurnShell` 从模块级 `conversationTurnHeightMemory` 取初始高度，用 `ResizeObserver` 持续更新实测高度，并输出 `data-conversation-row-key` 供视口锚点恢复使用。
3. 界面切换的底部距离恢复由 `ConversationWorkspace` 内独立的 `turnRestoreFrameIdsRef` 和多帧恢复逻辑承担，不进入旧虚拟化 hook。
4. 用户滚轮、滚动条边缘按下或手动视口事件会取消当前底部恢复帧，再向 `ChatArea` 上报确定的 `up`/`down`/`unknown` 方向。
5. 折叠恢复则由 `ConversationMeasuredCollapseProvider` 直接读取 `measuredCollapseById`；通用 `expandedById` 副本从未进入消费链。

### 长期优化调整

- 删除整个 `conversation-virtualization.tsx`，不保留无调用实现、虚拟恢复类型或为“将来可能”准备的兼容壳。
- 从 `ConversationWorkspace`、`ChatArea`、布局记忆 hook 和侧栏子代理中删除虚拟恢复 props、字段、写入器与 no-op 回调。
- 将仍有两个明确消费者的 `ConversationTimelineUserScrollDirection` 迁移到 `chat-area-scroll.ts`，使类型归属真实滚动契约，而非已删除的实现。
- 删除 `expandedById`，将 `measuredCollapseById` 作为折叠展开状态与尺寸的唯一真值源。

### 文件变更

- 删除：`apps/vulcan-code-web/src/components/conversation/conversation-virtualization.tsx`
- 修改：`apps/vulcan-code-web/src/components/conversation/chat-area-scroll.ts`
- 修改：`apps/vulcan-code-web/src/components/conversation/chat-area.tsx`
- 修改：`apps/vulcan-code-web/src/components/conversation/chat-area/surface-layout-memory.ts`
- 修改：`apps/vulcan-code-web/src/components/conversation/conversation-layout-restore.ts`
- 修改：`apps/vulcan-code-web/src/components/conversation/conversation-workspace.tsx`
- 修改：`apps/vulcan-code-web/src/components/workspace/WorkspaceContextPanel.tsx`
- 修改：`TASK_LOG.md`
- 新增：无

### 修改范围代码审核与自动修复

- 审核通过“定义 → import → JSX/Hook 调用 → 状态上报 → 恢复读取”反向链证明旧虚拟化只剩类型管道，不影响当前转录渲染。
- 审核确认主会话的 `virtualizer` 快照永远不会被回调写入，侧栏明确传入 no-op；已删除整条伪持久化管道。
- 审核反向搜索发现 `expandedById` 与 `measuredCollapseById[*].expanded` 重复，且只写不读；已删除重复字段与派生构造。
- 自动修复删除虚拟化维度后遗留的两处过期注释，使布局记忆文档与当前仅有的滚动/展开维度一致。
- CodeKit 最终反向搜索确认旧文件名、两个旧虚拟化导出、虚拟恢复类型与 `expandedById` 全部归零。

### 验证结果

- 首版 `npm run lint`：通过。
- 删除重复展开状态后 `npm run lint`：再次通过。
- `npm run build`：通过，Vite 成功转换 2358 个模块并生成生产产物。
- `git diff --check`：通过，未发现空白错误。
- CodeKit AST 复查：4 个剩余滚动/恢复文件全部正常解析；旧虚拟化与重复展开状态搜索为零。

### 遗留问题与下一轮方向

- 当前 `ConversationTurnShell` 的高度记忆使用模块级 `Map<surfaceId, Map<shellKey, height>>`，暂未看到会话/轮次删除时的清理边界；下一轮应追踪其生命周期与潜在内存泄漏。
- `ConversationWorkspace` 与 `ChatArea` 仍各自维护多帧底部恢复，锚点恢复失败时的回退策略也存在不一致；需在内存边界明确后继续收口。
- 生产构建仍存在既有主工作台大 chunk 警告，本轮未改变加载策略。

## 第 12 轮：消除轮次高度全局泄漏并修复首帧恢复（2026-07-10）

### 问题与事实依据

- `conversationTurnHeightMemory` 是模块级 `Map<surfaceId, Map<shellKey, height>>`，只有 `ConversationTurnShell` 的读取与 `ResizeObserver` 写入，全仓没有会话删除、子代理消失、轮次被替换或组件卸载时的清理点。
- 工作台长时间运行并打开/删除多个会话后，模块 Map 会永久保留旧 surface 及其全部 turn key，生命周期超出任何 React 所有者。
- 代码审核进一步确认：旧实现保存了具体高度，但只用“是否为 null”决定要不要设置估算 `minHeight`；有历史高度时数值本身反而完全没有应用到首帧样式。
- 界面键已由 `conversation.id`/子代理任务 id 唯一确定，轮次键已由 `groupConversationTimelineItems` 与 `conversationTimelineItemKey` 唯一产出，无需候选字段或模糊兼容。

### 已查明的执行流程

1. `ConversationWorkspace` 从当前可见时间线投影构造 `turnGroups` 与权威 `liveTurnKeys`。
2. `ChatArea` 按当前 surface id 读取布局记忆，将 `turnHeightsByKey` 传给工作区；每个 `ConversationTurnShell` 按自身 turn key 取初始实测高度。
3. 外壳首帧优先用历史实测高度作为 `minHeight`；无历史时才使用轮次类型估算值。
4. effect 挂载后立即测量真实 DOM 高度，有变化时以 surface/turn 精确键写回布局记忆，随后解除临时 `minHeight`。
5. `ResizeObserver` 继续上报后续真实高度变化；相同数值通过 ref 与布局注册表双重跳过。
6. `liveTurnKeys` 变化后删除当前界面中已消失轮次的高度；会话或子代理从工作台移除后，布局记忆再删除整个 surface 记录。

### 长期优化调整

- 删除 `conversation-workspace.tsx` 的模块级 `conversationTurnHeightMemory`，高度数据不再超出 React 业务所有者生命周期。
- 在 `ConversationSurfaceLayoutRestoreState` 增加 `turnHeightsByKey`，复用已有存活 surface 清理边界。
- `useConversationSurfaceLayoutMemory` 增加类型化 `writeTurnHeight` 与 `retainTurnHeights`，前者只接受有限正高度，后者只保留当前权威轮次键。
- `ConversationWorkspace` 仅通过显式初始高度与可选上报/保留回调使用所有者记忆；侧栏工作区没有跨界面恢复需求，只保留组件本地测量，不创建新的全局缓存。
- 修正首帧高度策略：历史实测值现在真正进入 `minHeight`，无记录时使用估算值，完成真实测量后立即解除临时约束。

### 文件变更

- 修改：`apps/vulcan-code-web/src/components/conversation/conversation-layout-restore.ts`
- 修改：`apps/vulcan-code-web/src/components/conversation/chat-area/surface-layout-memory.ts`
- 修改：`apps/vulcan-code-web/src/components/conversation/chat-area.tsx`
- 修改：`apps/vulcan-code-web/src/components/conversation/conversation-workspace.tsx`
- 修改：`TASK_LOG.md`
- 新增：无
- 删除：无

### 修改范围代码审核与自动修复

- 审核确认原模块 Map 无任何清理路径，且同时被主会话与侧栏实例共享；已用所有者内布局记忆替代。
- 审核发现保存的高度数值从未应用到样式，只作为 null 布尔判断；已让历史值真正稳定首帧。
- 审核验证历史高度大于新内容时的收缩路径：首次测量后先解除 `minHeight`，`ResizeObserver` 会再上报更小真实高度，不会被旧最小高度永久锁住。
- 自动修复了已删除虚拟窗口后遗留的“虚拟窗口测量”过期注释，并补齐新组件、内部测量函数和保留 effect 的中英文边界说明。
- CodeKit 反向搜索确认模块级 `conversationTurnHeightMemory` 已归零，新高度数据只流经布局记忆、`ChatArea`、`ConversationWorkspace` 和 `ConversationTurnShell`。

### 验证结果

- 初版 `npm run lint`：通过。
- 首帧策略自动修复后 `npm run lint`：通过。
- 注释完整性修复后 `npm run lint`：再次通过。
- `npm run build`：通过，Vite 成功转换 2358 个模块并生成生产产物。
- `git diff --check`：通过，未发现穽白错误。
- CodeKit AST 复查：布局记忆、恢复类型、`ChatArea` 与 `ConversationWorkspace` 全部正常解析；新高度链只存在 3 个所有者文件中。

### 遗留问题与下一轮方向

- `ChatArea` 的界面切换恢复、布局事件恢复、隐藏思考恢复和 `ConversationWorkspace` 的底部距离恢复仍各自安排不同帧数，下一轮应对齐快照恢复策略与取消边界。
- 当前轮次高度记忆是 WebUI 运行期数据，跨应用重启仍使用保守估算；未有后端持久化协议前不应猜测字段。
- 生产构建仍存在既有主工作台大 chunk 警告，本轮未改变加载策略。

## 第 13 轮：统一滚动快照恢复并修复锚点回退（2026-07-10）

### 问题与事实依据

- 布局变更恢复与隐藏思考恢复都按“贴底 → 锚点 → 底部距离”处理完整快照，但会话切换恢复在锚点行暂未挂载时直接跳到转录尾部。
- `distanceFromBottom` 与锚点在同一次快照捕获中同时产生，就是锚点不可用时的确定备用位置；直接跳尾会把用户的旧阅读位置错误改写成贴底。
- `ConversationWorkspace` 另外维护 `restoreElementDistanceFromBottom`，与 `chat-area-scroll.ts` 的公共底部距离函数计算同一个 `scrollHeight - clientHeight - distance` 并各自做边界约束。

### 已查明的执行流程

1. 快照捕获先判断当前视口是否允许尾随；非贴底时同时保存首个可见轮次键、其相对偏移和底部距离。
2. 恢复贴底快照时应直接设置到转录尾部，保持后续流式尾随。
3. 非贴底且锚点行已挂载时，通过行顶部与视口顶部差值恢复相同视觉偏移。
4. 锚点行因测量未完成、投影过滤或数据变化而不可用时，应使用同快照的底部距离，不推测新位置。
5. `ConversationWorkspace` 的初始底部距离恢复不携带锚点，但可直接复用同一底层距离公式。

### 长期优化调整

- 在 `chat-area-scroll.ts` 新增 `restoreConversationScrollSnapshot`，唯一定义完整快照的贴底、锚点与底部距离优先级。
- 布局事件、隐藏思考和会话切换三处恢复全部调用该原语，删除父组件内的三套条件分支。
- 会话切换的锚点失败现在回退至已保存底部距离，不再强制跳到尾部。
- 删除 `ConversationWorkspace.restoreElementDistanceFromBottom`，工作区初始恢复直接复用 `conversationScrollToDistanceFromBottom`。
- `ChatArea` 核心函数由本轮前的约 876 行缩减到约 855 行。

### 文件变更

- 修改：`apps/vulcan-code-web/src/components/conversation/chat-area-scroll.ts`
- 修改：`apps/vulcan-code-web/src/components/conversation/chat-area.tsx`
- 修改：`apps/vulcan-code-web/src/components/conversation/conversation-workspace.tsx`
- 修改：`TASK_LOG.md`
- 新增：无
- 删除：无

### 修改范围代码审核与自动修复

- 审核确认完整快照的三个恢复入口全部进入统一原语，低层锚点恢复不再被上层分别编排。
- 审核确认剩余直接 `scrollTop = scrollHeight` 分支分别对应用户跳底、输入区变高贴底、未见界面默认贴底与流式尾随，不是快照恢复，因此不应机械替换。
- 审核确认 `conversationScrollToDistanceFromBottom` 只剩两类消费者：统一快照原语与不携带锚点的工作区初始恢复。
- CodeKit AST 复查确认新恢复原语正常解析，`ChatArea` 的三处调用与工作区距离调用均有精确结构上下文。
- 本轮修改后审核未发现需要额外自动修复的新问题。

### 验证结果

- `npm run lint`：通过。
- `npm run build`：通过，Vite 成功转换 2358 个模块并生成生产产物。
- `git diff --check`：通过，未发现空白错误。
- CodeKit AST 复查：`chat-area-scroll.ts` 正常解析为 8 个函数、2 个接口和 1 个方向类型；`ChatArea` 核心函数边界缩减到约 855 行。

### 遗留问题与下一轮方向

- 用户滚动会取消 `layoutRestoreFramesRef` 与工作区底部恢复帧，但会话切换 effect 仍在局部变量中持有两帧恢复任务；下一轮应确认真实用户输入是否能取消这组任务。
- 多处多帧调度仍重复手写递归或两/三层 `requestAnimationFrame`，后续应在取消语义明确后再抽取调度器，不能只为减少行数机械合并。
- 生产构建仍存在既有主工作台大 chunk 警告，本轮未改变加载策略。

## 第 14 轮：统一自动视口帧并修复用户取消竞态（2026-07-10）

### 问题与事实依据

- `ChatArea` 同时存在四组自动视口帧：布局事件 5 帧恢复、输入区变高 2 帧贴底、会话切换 2 帧恢复、流式内容 3 帧尾随。
- 原 `layoutRestoreFramesRef` 只追踪第一组；其余三组使用 effect 局部 `firstFrame`/`secondFrame`/`thirdFrame`，用户滚动处理器无法访问它们。
- 真实用户滚动先进入 `ConversationWorkspace`，只取消工作区自身底部恢复；上报 `ChatArea` 后又只取消布局事件帧，因此会话切换或流式尾随帧可在用户开始滚动后继续覆写 `scrollTop`。
- 跳底按钮是另一个明确用户意图，但原实现同样不取消已排期的自动恢复。

### 已查明的执行流程

1. 鼠标滚轮、滚动条边缘按下、触摸开始或手动视口事件先让工作区停止自身恢复，再把方向上报 `ChatArea`。
2. `ChatArea` 记录 260ms 原生滚动接管窗口；向上滚动另外设置 900ms 近底尾随抑制。
3. 每个自动恢复 effect 都需要一个“仅取消自身序列”的 cleanup，以免依赖变化时误伤其他序列。
4. 用户意图、跳底按钮与组件卸载则需要“取消所有序列”，并使已进入当前回调的代次无法再排下一帧。
5. 取消后的 260ms 内仍可发生宽度布局事件或输入区高度变化，因此具体校正回调还必须检查原生接管窗口，不能只取消当时已排任务。

### 长期优化调整

- 新增 `chat-area/viewport-frame-scheduler.ts`，由 `useConversationViewportFrameScheduler` 唯一持有 `ChatArea` 自动校正帧。
- `schedule` 按精确帧数顺序执行，返回序列范围幂等取消函数；`cancelAll` 取消全部待执行帧并递增全局取消代次。
- 代次检查在 `onFrame` 执行前后各进行一次，即使回调执行期间发生全量取消，也不会再排下一帧。
- 四组任务按原有 5/2/2/3 帧数迁移，布局恢复仍可单独被新布局事件替换，其他 effect cleanup 仍仅取消自身。
- 用户滚动和跳底按钮统一调用 `cancelAll`，hook 在卸载时也自动清理全部帧。
- 布局快照恢复与输入区贴底回调增加 260ms 原生滚动接管检查，防止取消后又被新事件重新排期。
- `ChatArea` 核心函数由第 13 轮后约 855 行继续降至约 822 行。

### 文件变更

- 新增：`apps/vulcan-code-web/src/components/conversation/chat-area/viewport-frame-scheduler.ts`
- 修改：`apps/vulcan-code-web/src/components/conversation/chat-area.tsx`
- 修改：`TASK_LOG.md`
- 删除：无

### 修改范围代码审核与自动修复

- 审核确认 `ChatArea` 中已无任何直接 `requestAnimationFrame` 或 `cancelAnimationFrame`，四类自动视口任务全部经统一调度器。
- 审核确认会话切换恢复的 `scrollRestoreGuardRef` 在序列完成、effect cleanup 与用户取消三条路径都能精确清理。
- 审核发现已取消任务可被接管窗口内的新布局/输入区事件重建；已在两个写 `scrollTop` 回调中增加 `userScrollSettleUntilRef` 守卫。
- 审核删除了旧虚拟化实现移除后遗留的“虚拟化行”过期注释，并用显式已完成取消常量替代匿名 no-op 返回。
- CodeKit AST 复查确认调度器正常解析为 1 个 hook 与 1 个接口；反向搜索确认所有浏览器 RAF 调用只存在新 hook 内。

### 验证结果

- 初版 `npm run lint`：通过。
- 接管窗口自动修复后 `npm run lint`：再次通过。
- `npm run build`：通过，Vite 成功转换 2359 个模块并生成生产产物。
- `git diff --check`：通过，未发现空白错误。
- CodeKit AST 复查：`ChatArea` 核心函数边界缩减到约 822 行；帧调度文件正常解析，无 AST 错误。

### 遗留问题与下一轮方向

- `ConversationWorkspace` 仍使用独立 4 帧底部距离恢复，但用户输入已会先取消该序列再上报 `ChatArea`；它与父级快照恢复是否仍重复，需要继续追踪挂载顺序。
- 帧调度器目前只服务 `ChatArea`；其他动画 RAF 具有不同业务取消语义，不应为了复用而强行迁入。
- 生产构建仍存在既有主工作台大 chunk 警告，本轮未改变加载策略。

## 第 15 轮：明确初始滚动恢复所有权并删除父子重复（2026-07-10）

### 问题与事实依据

- 主会话每次首次挂载或切换时，`ChatArea` 都覆盖三种状态：有已保存锚点/距离时恢复快照，有贴底快照时恢复尾部，无快照时将未见界面初始化到尾部。
- `ConversationWorkspace` 又以 `suppressInitialDistanceRestore` 决定是否执行自身恢复；主会话只在快照带锚点时传 `true`，纯底部距离和无快照情况会同时启动父级 2 帧与子级重复恢复。
- 侧栏子代理直接渲染 `ConversationWorkspace`，没有 `ChatArea` 父级，因此必须保留工作区自有的初始距离恢复。
- 工作区重复帧常量为 4，但原递归从 `remainingFrames=4` 一直执行到 `0` 帧，实际调度 5 帧，与常量名和注释不符。

### 已查明的执行流程

1. 主会话子树挂载时，工作区按 surface key 检查自身是否应恢复底部距离；父 `ChatArea` 另外在布局 effect 中处理完整快照或默认贴底。
2. 工作区恢复原本在首帧同步执行一次，然后递归排期重复帧；父级同样同步执行并排期 2 帧。
3. 两层使用同一滚动 DOM 元素，所以重复恢复不是互补，而是对同一 `scrollTop` 的重复写入。
4. 侧栏子代理的滚动元素由工作区自己持有，工作区必须以距离 `0` 在内容测量稳定期间保持初始贴底。

### 长期优化调整

- 将负向 `suppressInitialDistanceRestore` 替换为正向 `restoreInitialScrollDistance`，调用点直接声明工作区是否拥有恢复职责。
- 新增 `ConversationWorkspaceInitialScrollRestore` 判别联合：父级拥有时 `restoreInitialScrollDistance=false` 且禁止距离参数；工作区拥有时必须同时提供精确距离。
- 主 `ChatArea` 明确声明父级拥有，不再向工作区传入底部距离；侧栏子代理明确声明工作区拥有并传入 `0`。
- `restoredTurnSurfaceKeyRef` 只记录工作区真正执行过恢复的 surface；切换为父级拥有时清空，避免后续同 surface 切回工作区拥有时被错误短路。
- 修正工作区递归终止条件，现在常量 4 严格对应 4 个排期帧，不再包含索引 `0` 的第 5 帧。

### 文件变更

- 修改：`apps/vulcan-code-web/src/components/conversation/chat-area.tsx`
- 修改：`apps/vulcan-code-web/src/components/conversation/conversation-workspace.tsx`
- 修改：`apps/vulcan-code-web/src/components/workspace/WorkspaceContextPanel.tsx`
- 修改：`TASK_LOG.md`
- 新增：无
- 删除：无

### 修改范围代码审核与自动修复

- 审核确认主会话的有快照、无快照和首次挂载三条路径都由 `ChatArea` 完整覆盖，可安全禁用子层恢复。
- 审核确认侧栏子代理无父级恢复器，已保留工作区自有距离恢复，没有机械删除独立场景能力。
- 类型审核发现“恢复=true 但距离缺失”仍可表达；已用判别联合从编译期排除无效组合，并删除 `?? 0` 模糊回退。
- 行为审核发现所有权从父级切到工作区时会被旧已恢复键短路；已让父级模式清空该键。
- 帧数审核发现 4 帧常量实际执行 5 帧；已在调度前检查剩余帧数，使数量与契约一致。
- CodeKit 反向搜索确认旧 `suppressInitialDistanceRestore` 已归零，新判别模式只有主会话与侧栏子代理两个精确调用点。

### 验证结果

- 所有权首版调整后 `npm run lint`：通过。
- 判别联合与模式切换修复后 `npm run lint`：通过。
- 帧数自动修复后 `npm run lint`：再次通过。
- `npm run build`：通过，Vite 成功转换 2359 个模块并生成生产产物。
- `git diff --check`：通过，未发现空白错误。
- CodeKit AST 复查：初始恢复判别联合、`ConversationWorkspace`、`ChatArea` 与侧栏子代理全部正常解析，无 AST 错误。

### 遗留问题与下一轮方向

- `ChatArea` 滚动/布局域经连续收口后已有明确存储、恢复策略、帧调度和所有权边界；下一轮应转向其他高复杂度文件，避免继续对已收敛领域做机械拆分。
- `ConversationWorkspace` 的独立恢复调度仍是局部实现，但它现在只服务无父级的侧栏场景，且具有独立用户取消边界，当前无跨所有者竞态证据。
- 生产构建仍存在既有主工作台大 chunk 警告，本轮未改变加载策略。

## 第 16 轮：固定两阶段停止目标并收口决策生命周期（2026-07-10）

### 问题与事实依据

- 主会话停止是两阶段流程：第一次以 `ask_if_running_subagents` 预检；存在运行中子代理时等待用户决策，再第二次调用实际停止。
- 前端 `stopConversationRun` 原本只传 `mode`；Rust `service_stop_conversation_run` 每次进入都通过 `selected_session_id_from_state` 重新读取全局选中会话。
- 弹窗等待期间侧栏仍允许切换会话，`handleSelectConversation` 会调用后端 `service_select_conversation`；因此第二次停止可能操作新会话，而弹窗展示的智能体却来自旧会话预检。
- `WorkbenchLayout` 用一个 resolver ref 和一个 agents state 手工维护 Promise，重复请求会取消旧等待，但切会话与组件卸载都没有释放路径。

### 已查明的执行流程

1. 输入区停止按钮调用 `handleStopConversationRun`，原服务请求让 Rust 从共享状态解析当前 `selected_session_id`。
2. Rust 对该会话树查询运行中子代理；无子代理时当场停止，有子代理时只返回 `needs_confirmation` 和智能体摘要。
3. 前端将摘要存入全局弹窗状态，`handleStopConversationRun` 暂停在 Promise，直到用户取消或选择“仅主会话/主会话与子代理”。
4. 确认后的第二次 Rust 调用会再读全局选中值，并用该值移除流任务、停止 kernel generation、更新工作区、停止子代理树、暂停目标及持久化。
5. 这条后端链路的所有操作都是 session-scoped，因此必须在预检前固定唯一目标，并将同一值贯穿两次 RPC。

### 长期优化调整

- `stopConversationRun` 改为强制接收 `conversationId`，RPC 载荷同时携带 `conversationId` 与 `mode`，不再依赖快变全局选中状态。
- `handleStopConversationRun` 在流程起点从已持久化 `selectedConversation.id` 捕获不可变目标，预检、决策请求和最终停止全部使用该 id。
- Rust RPC 调度将 `conversationId` 解析为必填参数；服务处理器解析 `SessionId`、验证常规持久化会话存在，后续全链只使用 `target_session_id`。
- 新增 `use-stop-conversation-run-decision.ts`，集中管理决策作用域、agents 载荷、Promise resolver、重复请求替换、切会话取消与卸载释放。
- 决策状态同时保存不可变 `conversationId` 与智能体列表；当活动会话不同时立即以 `null` 完成等待并关闭弹窗。
- `WorkbenchLayout` 删除手写 resolver ref、agents state 和两个决策回调，只组合控制器状态与操作。

### 文件变更

- 新增：`apps/vulcan-code-web/src/components/workspace/workbench-layout/use-stop-conversation-run-decision.ts`
- 修改：`apps/vulcan-code-web/src/components/workspace/workbench-layout.tsx`
- 修改：`apps/vulcan-code-web/src/components/workspace/workbench-layout/types.ts`
- 修改：`apps/vulcan-code-web/src/components/workspace/workbench-layout/conversation-actions.ts`
- 修改：`apps/vulcan-code-web/src/service/conversation-actions-service.ts`
- 修改：`crates/vulcan-service-kernel/src/rpc_dispatch.rs`
- 修改：`crates/vulcan-service-kernel/src/conversation_run_services.rs`
- 修改：`TASK_LOG.md`
- 删除：无

### 修改范围代码审核与自动修复

- 审核确认 Rust 原处理器在两次调用首部分别执行 `selected_session_id_from_state`，已改为必填请求 id 且全函数只使用 `target_session_id`。
- 审核确认子代理预检、主流任务、kernel generation、子代理树、目标暂停和后台持久化全部改为显式目标。
- 审核发现新 resolver 结构重复保存了从未读取的 `conversationId`；已删除副本，作用域真值只保留在可见 decision 状态。
- 重复决策请求现在先脱离旧 resolver，再以 `null` 完成旧 Promise，最后安装新事务，避免重入时的双所有者。
- 卸载 cleanup 只释放 resolver，不对已卸载组件调用 state setter；切会话则通过正常 `resolve(null)` 同时关闭弹窗。
- CodeKit 反向搜索确认旧 `stopConversationChoiceResolverRef` 已归零，两次前端调用和 Rust 处理器均有精确目标结构。

### 验证结果

- `npm run lint`：通过。
- `npm run build`：通过，Vite 成功转换 2360 个模块并生成生产产物。
- `git diff --check`：通过，未发现空白错误。
- `cargo fmt --check --all`：通过。
- `cargo check -p vulcan-service-kernel`：通过。
- `cargo test -p vulcan-service-kernel stop_conversation_running_subagent_summaries_preserve_task_identity --lib`：通过，1 个测试通过，343 个被过滤。
- CodeKit AST 复查：决策 hook、会话 action、前端 service、Rust 停止处理器与 RPC 调度全部正常解析，无 AST 错误。

### 遗留问题与下一轮方向

- 本轮只修正有明确两阶段用户决策窗口的主会话停止协议；其他单次服务操作不应一律添加目标参数，需逐条查证是否依赖快变选中状态。
- `WorkbenchLayout` 仍是约 928 行的组合函数，但停止决策已从其中移出；下一轮可继续检查子代理打开请求合并与消息缓存生命周期。
- 生产构建仍存在既有主工作台大 chunk 警告，本轮未改变加载策略。

## 第 17 轮：修复子代理转录缓存自我续命与过期消息（2026-07-10）

### 问题与事实依据

- `subagentSurfaceMessageCacheRef` 按子任务会话 id 保留已加载转录，供主工作台、侧栏页签、全量快照和增量更新反复注入。
- 原 `rememberSubagentSurfaceMessages` 用“当前浮动导航任务 + `cache.keys()`”构造任务集；一个 key 只要曾进入缓存，就永久被自身视为存活。
- `injectCachedSubagentSurfaceMessages` 原本会把全部缓存 key 的消息无条件重新挂回 `workbench.messages`，因此已关闭任务、已归档/删除父会话的消息仍长期占用内存并污染本地快照。
- Rust `subagent_navigation_snapshot_for_parent_from_cached_state` 明确以常规根会话为 `parentConversationId`，`tasks` 由 `open_subagent_registry_entries_for_parent_tree` 投影该根下整棵未关闭子代理树；因此可从根导航安全推导嵌套任务存活集。

### 已查明的执行流程

1. 打开子代理面板时，服务返回根会话导航与活动子任务消息，`applySubagentPanelRouteResponse` 将导航归一化到 `subagentNavigationByConversation`，并缓存消息窗口。
2. 切换到其他根会话时，旧根导航保留在按会话缓存中，以便返回时恢复已打开子页签；这是有效跨切换记忆，不能只根据当前浮动导航清理。
3. 后端关闭任务时，新导航树不再包含该 task id；归档/移除根会话时，`workbench.conversations` 不再包含该根 id。
4. 完整快照和水合合并需要从当前消息刷新所有存活子窗口；流式增量只更新受影响任务，不应每个 token 扫描全部子转录。
5. 清理缓存 key 时必须同时从 `workbench.messages` 移除该任务的旧行，否则只删 Map 仍会留下快照污染。

### 长期优化调整

- 新增 `liveSubagentTaskIds`：先以活动常规会话限定根，再合并 `subagentNavigationByConversation` 与当前聚焦导航，得到每个根下完整嵌套任务集。
- 聚焦导航在同根下覆盖缓存导航，避免关闭任务后被旧条目重新加回存活集。
- `rememberSubagentSurfaceMessages`、消息增量同步与单窗口替换全部只接受权威存活任务，删除 `cache.has`/缓存 key 作为任务身份的回退。
- `injectCachedSubagentSurfaceMessages` 在注入前就地删除退出 task key，并保留已退出 id 直到从快照中过滤掉旧消息；最终只重新挂载存活缓存窗口。
- `deriveSelectedConversationFields` 同步过滤父会话已不在活动会话列表中的导航树，避免导航归一化缓存自身泄漏。

### 文件变更

- 修改：`apps/vulcan-code-web/src/state/workbench-state.ts`
- 修改：`TASK_LOG.md`
- 新增：无
- 删除：无

### 修改范围代码审核与自动修复

- 审核确认前端按根缓存导航是跨会话切换恢复的必需能力，未将清理范围错误缩小到当前浮动列表。
- 审核 Rust 投影确认 `tasks` 包含根下整棵树，前端不需要递归猜测子任务父子字段。
- 审核确认关闭响应和迟到打开响应都经过 `applySubagentPanelRouteResponse → deriveSelectedConversationFields → injectCachedSubagentSurfaceMessages`，无效父/任务无法重建缓存。
- 保留流式增量的单任务同步路径，没有为了清理正确性将全量消息分组引入每个 token 更新。
- 自动修复补齐了缓存注入函数“就地清理”副作用、权威输入、返回快照和关键中间集合的中英文说明。
- CodeKit 反向搜索确认 `cache.keys()` 只剩枚举/删除用途，不再参与存活身份推导。

### 验证结果

- 首版 `npm run lint`：通过。
- 审核文档补齐后 `npm run lint`：再次通过。
- `npm run build`：通过，Vite 成功转换 2360 个模块并生成生产产物。
- `git diff --check`：通过，未发现空白错误。
- CodeKit AST 复查：`workbench-state.ts`、会话 action 与 `WorkbenchLayout` 全部正常解析；缓存清理/注入函数具有完整结构上下文。

### 遗留问题与下一轮方向

- `handleOpenSubagentPanel` 仍在一个约 100 行回调中混合本地缓存快速路径、同路由请求合并、全局序列过期检查、响应合并与错误策略；下一轮可评估抽离独立路由控制器。
- 子代理转录缓存属于 WebUI 运行期加速层，后端仍是未关闭任务和转录的权威来源；不应将前端 Map 持久化为新协议。
- 生产构建仍存在既有主工作台大 chunk 警告，本轮未改变加载策略。

## 第 18 轮：修复子代理多页签加载串行失效与只读路由版本污染（2026-07-10）

### 问题与事实依据

- 右侧上下文面板会为每个 `parentConversationId + taskId` 创建独立子代理页签；页签在请求前乐观打开，请求返回 `false` 时才回滚对应页签。
- 原 `handleOpenSubagentPanel` 只保存一个全局在途请求和单调序号。先打开未缓存任务 A、再打开未缓存任务 B 后，只要 A 较晚返回，A 就会因不是“最后一次点击”而返回 `false`，即使后端已成功恢复 A 的转录，调用方仍会错误关闭 A 页签。
- 同一路由请求确实应该合并，但不同 task 路由是可并存页签，不存在互相取消的业务语义；全局 last-wins 序列与界面模型冲突。
- `applySubagentPanelRouteResponse` 原本在响应版本落后于当前全局版本时直接丢弃整个响应。子代理打开响应中的消息属于路由局部结果，若任务在最新导航中仍存活，不能因为其他页签或无关全局事件先推进版本就丢失。
- Rust `service_open_subagent_panel` 只从已水合内存和权威未关闭任务注册表读取详情；它用临时 `active_subagent_panel` 构造目标任务响应，随后恢复共享活动面板、导航与消息，因此打开页签本质上是只读路由查询。
- 原只读打开仍会通过通用投影器增加 `workbench_generation`、保留提高后的 `workbench.revision` 并额外发送导航增量，再通过 RPC 返回同一结果；这制造了不必要的全局版本竞争。

### 已查明的执行流程

1. 时间线或任务列表调用 `WorkspaceShellContent.handleOpenSubagentPanel`，前端先按父会话和 task id 创建/激活独立上下文页签。
2. 工作台 action 先检查运行期消息缓存；未命中时调用 `service_open_subagent_panel`，重复的同路由点击应共享一次后端 Promise。
3. Rust 校验父会话和 task id，沿 `open_subagent_registry_entry_for_task` 确认任务仍在权威未关闭树中，再临时选择目标 task 构建导航和转录响应。
4. RPC 响应由 `applySubagentPanelRouteResponse` 写入按父会话归一化的导航和按 task id 缓存的消息；右侧页签始终按自身 task id 读取消息，不依赖一个全局活动 task 来决定显示内容。
5. 任务停止/关闭与普通打开不同：前两者是真实生命周期变更，必须推进全局版本并发送导航事件；普通打开仅服务当前路由调用方，不应消耗全局事件序号。
6. 若普通打开响应到达时已有更高全局版本，最新导航树是任务存活性的权威来源；任务仍存活时只合并该 task 的消息，任务已退出时拒绝复活旧缓存。

### 长期优化调整

- 删除 `subagentNavigationRequestSeqRef`、单槽 `InflightSubagentOpenRequest` 及其从 `WorkbenchLayout` 向 action hook 的参数传递，移除与全局最后点击绑定的过期判定。
- 在 `useWorkbenchConversationActions` 内部使用按路由键索引的 `SubagentOpenRequestMap`；同一路由共享 Promise，不同路由并发独立完成，成功响应均返回 `true` 并保留各自乐观页签。
- 统一同路由合并与首次请求的响应应用、追踪、错误和清理流程；仅后端命令拥有者显示异常 toast，避免合并调用重复提示。
- `applySubagentPanelRouteResponse` 对当前版本继续应用完整导航投影；对较旧响应只在最新导航仍声明 task 存活时合并路由局部消息，不降级全局 revision，也不重放旧导航。
- `service_open_subagent_panel` 不再接收事件发射器，也不再发送临时导航增量；RPC 返回成为普通打开的唯一结果通道。
- 将服务端构造拆为两层：变更型构造器继续供停止/关闭使用并推进全局版本；只读包装器仅供打开使用，恢复 `workbench_generation` 与 revision，并让响应继承原全局版本。
- 扩展既有导航回归测试，验证只读详情响应仍聚焦请求 task、保留完整未关闭任务树，同时不改变共享活动面板、导航、消息、generation 和 revision。

### 文件变更

- 修改：`apps/vulcan-code-web/src/components/workspace/workbench-layout.tsx`
- 修改：`apps/vulcan-code-web/src/components/workspace/workbench-layout/conversation-actions.ts`
- 修改：`apps/vulcan-code-web/src/state/workbench-state.ts`
- 修改：`crates/vulcan-service-kernel/src/rpc_dispatch.rs`
- 修改：`crates/vulcan-service-kernel/src/subagent_panels.rs`
- 修改：`crates/vulcan-service-kernel/src/tests/subagent_navigation_tests.rs`
- 修改：`TASK_LOG.md`
- 新增：无
- 删除：无

### 修改范围代码审核与自动修复

- 审核确认右侧页签状态按 task id 独立保存，后端打开响应也使用临时 task 投影；因此不同路由没有互斥依据，按路由 Map 才与真实界面模型一致。
- 审核确认迟到响应不能直接信任自身导航恢复任务；消息降级合并严格依赖最新 `liveSubagentTaskIds`，关闭或移除后的任务不会被旧响应复活。
- 首版服务端纯化曾直接让共享构造器恢复 generation/revision；定向测试编译后继续审查调用链，发现停止/关闭也复用该构造器且需要真实版本推进，随即自动拆分变更型构造器与只读包装器，避免修复普通打开时破坏生命周期事件。
- 新增测试首次编译暴露只读构造器对同级测试模块不可见，已将其收敛为仅父模块可见，未扩大到 crate 外部 API；修正后定向测试通过。
- 审核 `subagent_navigation_snapshot_from_cached_state` 确认其唯一共享写入是无效活动面板清理，外层临时构造会恢复该字段；消息状态修复函数只修改响应局部消息切片。
- 反向搜索确认旧全局序号、旧单槽请求类型和普通打开事件慢日志均已移除；停止与关闭仍保留导航事件发射路径。

### 验证结果

- 首版前端调整后 `npm run lint`：通过。
- 服务端调整后 `npm run lint`：再次通过。
- 定向测试首次运行：发现只读构造器可见性错误，已自动修复。
- `cargo test -p vulcan-service-kernel opening_one_subagent_detail_preserves_full_unclosed_navigation`：修复后通过，1 项通过、343 项过滤。
- `cargo test -p vulcan-service-kernel`：通过，共 344 项测试、2 个测试套件全部通过。
- `npm run build`：通过，Vite 成功转换 2360 个模块并生成生产产物。
- `cargo fmt --all -- --check`：通过。
- `git diff --check`：通过，未发现空白错误。
- CodeKit AST 复查：6 个改动文件全部成功解析，102 个结构项，错误为 0。

### 遗留问题与下一轮方向

- WebUI 包当前只有 TypeScript 校验和生产构建脚本，没有现成前端单元测试运行器；本轮未为单一并发修复引入新的核心测试库，前端并发语义由纯状态边界、服务端回归测试和完整构建共同验证。
- `useWorkbenchConversationActions` 仍集中会话发送、子代理、待发送、模型、编辑与 fork 等多类职责；后续可继续按真实调用边界寻找可独立验证的控制器，而不是只做机械拆文件。
- 生产构建仍存在既有主工作台大 chunk 警告，本轮未改变加载策略。

## 第 19 轮：修复工作区文件目录与预览的跨路由异步污染（2026-07-10）

### 问题与事实依据

- `WorkspaceFilesPanelHost` 在同一挂载组件内随会话、子代理、工作区和动态页签实例切换缓存桶；目录、选择、预览与错误都复用同一组 React state。
- 原 `loadDirectory` 在请求返回后无条件合并 `listing`，没有校验请求发起时的路由、后端过滤器或同目录更新身份。切换过滤器清空目录后，旧过滤请求仍可把不兼容列表重新写回；切换会话/工作区后，旧目录也可进入新路由缓存。
- 原 `loadFilePreview` 同样无结果所有权。快速选择文件 A 再选择文件 B 时，较慢的 A 响应可覆盖 B 的预览；旧请求的 `finally` 还会提前清除新请求的加载状态。
- 目录展开可以在 state 提交前重复触发同一路径加载；原实现只看已完成的 `directories`，没有同步在途所有权，导致重复 RPC。
- 面板模块缓存上限为 12，但缓存条目只保存目录响应，不保存产生响应的后端过滤器指纹。另一个文件页签修改全局过滤偏好后，重新打开旧页签会恢复由旧过滤器生成的目录树。
- `WorkspaceFileDirectoryListing` 明确携带后端实际应用的 `filters`，服务端 `service_workspace_files_list` 直接用请求过滤器构造响应；因此过滤器是目录缓存的确定归属字段，不需要猜测路径或增加兼容分支。

### 已查明的执行流程

1. 面板从选中会话/草稿和工作台快照解析 `activeSurfaceId`、项目 id 与工作区路径，再组合动态页签实例 id 形成路由缓存键。
2. 首次挂载或路由切换时，从模块级缓存恢复目录、展开状态、选择与标题；根目录由副作用调用 `workspace.files.list`，展开目录继续懒加载子列表。
3. 用户切换树过滤器时，前端持久化全局偏好、清空当前目录和选择并重新加载根目录；RPC 响应包含实际过滤器与规范化路径。
4. 用户选择文件时立即记录 path/entry，代码和已授权媒体调用 `workspace.files.read`；恢复的已选文件也会由副作用自动重新加载预览。
5. 右侧栏折叠或动态文件页签切换时组件可能卸载/复用，面板 state 会持续写回模块级缓存；关闭动态页签时按实例 id 清理缓存。
6. 目录请求允许不同路径并行，但同一“路由 + 过滤器 + 目录”只能有一个写入者；预览则是单选界面，只允许当前作用域内最后选择的文件写入。

### 长期优化调整

- 新增 `useWorkspaceFilesRequestOwnership` 控制器，以已提交的请求作用域和单调 request id 作为唯一所有权依据，集中提供 begin/owns/finish/invalidate 操作。
- 请求作用域由路由缓存键和后端目录过滤指纹共同组成；控制器在 layout effect 提交新作用域时同步清空旧目录所有权与预览所有权。
- 目录请求按绝对目录键认领：同一作用域同一路径的重复调用直接合并，响应、错误、notice 与 loading 清理都必须持有准确 token；旧请求不能删除新请求的加载标记。
- 预览请求采用 latest-wins 所有权：新文件选择替换旧 token；普通不支持文件、媒体授权等待、关闭预览、路由或过滤器切换都会显式使旧预览失效。
- 新增 `hydratedRequestScopeKey`，记录当前 React state 真正所属的路由/过滤作用域；state 尚未水合到新作用域时，禁止文件预览、目录交互和缓存写回，消除“旧 state 发起新作用域请求”的提交时序窗口。
- 作用域变化同步清空目录/预览 loading，避免旧请求长期留下加载提示；只有仍持有所有权的 `finally` 才能清除当前请求的 loading。
- 缓存条目新增 `listingFiltersKey`，初始化与路由水合只恢复过滤指纹完全一致的条目；不再跨过滤条件复用目录和选择状态。
- 新增 `lastHydratedCacheKeyRef`，让缓存水合只由路由变化触发，同时保持 effect 依赖完整；同路由过滤变化继续走专用清空/重载流程。

### 文件变更

- 新增：`apps/vulcan-code-web/src/components/workspace/workspace-files-panel-request-ownership.ts`
- 修改：`apps/vulcan-code-web/src/components/workspace/WorkspaceFilesPanelHost.tsx`
- 修改：`TASK_LOG.md`
- 删除：无

### 修改范围代码审核与自动修复

- 审核确认工作区 Git 刷新虽然使用全局单调 id，但完成时按 `workspacePath` 比较 `refreshRequestId`，不同项目互不覆盖；该候选没有问题，未做无依据修改。
- 审核目录流程确认不同目录必须允许并行，同目录才应合并，因此没有使用一个全局 last-wins 序号串行化整棵文件树。
- 审核预览流程确认复制文件内容是一次独立用户动作，不应随预览选择变化而取消；所有权控制只覆盖可见预览 state，不影响复制结果。
- 首版所有权控制通过 TypeScript 校验后，审核发现缓存没有过滤指纹，已自动补充精确指纹并拒绝不兼容缓存。
- 第二次审核发现路由提交后的首个被动副作用仍可能读取旧 state 并启动新作用域预览，已自动加入 `hydratedRequestScopeKey` 门禁，覆盖自动恢复、手动选择、目录交互和缓存写入。
- 最终审核将路由缓存水合改为完整依赖并用 `lastHydratedCacheKeyRef` 限定只在 cache key 变化时执行，避免依赖缺失或过滤变化重复水合。
- CodeKit 反向检查确认 `setDirectories(listing)`、目录错误、`setPreview(response)` 和对应 loading 清理均位于所有权判定之后。

### 验证结果

- 首版请求所有权控制后 `npm run lint`：通过。
- 缓存指纹与水合作用域自动修复后 `npm run lint`：通过。
- 最终 effect 依赖修正后 `npm run lint`：再次通过。
- `npm run build`：最终通过，Vite 成功转换 2361 个模块并生成生产产物。
- `git diff --check`：通过，未发现空白错误。
- CodeKit AST 最终复查：`WorkspaceFilesPanelHost.tsx` 与新控制器共解析 27 个结构项，错误为 0；同时追踪的服务、协议与 Rust 文件解析错误也为 0。

### 遗留问题与下一轮方向

- WebUI 包仍没有现成单元测试运行器，本轮未为单一控制器引入新的核心测试库；通过三次 TypeScript 校验、两次生产构建、结构化源码审核与响应协议追踪完成验证。
- `WorkspaceReviewPanelHost` 也使用模块缓存、异步请求序号和路由水合 ref；下一轮可按本轮发现的提交时序事实检查其缓存写回是否真正绑定已水合状态，不能直接类推为同一问题。
- 生产构建仍存在既有主工作台大 chunk 警告，本轮未改变加载策略。

## 第 20 轮：修复 Git 审核缓存水合与刷新/详情/变更跨项目竞态（2026-07-10）

### 问题与事实依据

- `WorkspaceReviewPanelHost` 以工作区路径作为项目级缓存键，同一挂载组件在切换项目时复用 snapshot、selection、path、reviewKey、error 与 loading state。
- 原缓存水合 effect 先把 `hydratedReviewPanelCacheKeyRef.current` 改成新 key，再异步设置新缓存 state；紧随其后的缓存写回和自动刷新 effect 会在 state 尚属旧项目时看到 ref 已匹配，从而把旧快照写入新项目缓存并按旧 state 决策。
- 原完整 diff 刷新只有一个全局单调序号。正常项目切换会增加序号，但旧项目 Git 变更在切换完成后继续调用旧闭包时，可以再次取得最新序号并把旧项目响应写入共享 state。
- 原单文件详情仅用 `workspaceDetailPendingRef: Set<string>` 去重，detail key 不含项目路径。切换项目会清空 Set；旧请求的 `finally` 随后可删除新项目同名详情的 pending，旧响应也会尝试合并到新快照。
- Git 文件变更属于真实外部操作：即使用户已切换项目，也必须刷新被操作项目的共享 Git 摘要；但它不再有权清空当前面板、显示当前路由 toast、结束新项目 pending 或发起旧项目面板刷新。
- 会话消息 diff 解析已经同时使用 effect cleanup 与独立序号，输入源也随选中会话变化；该链路与项目级工作区 diff 不同，未做无依据替换。

### 已查明的执行流程

1. 面板从选中会话/草稿解析项目 id、工作区路径和活动会话 surface id，以工作区路径形成项目级审核缓存键。
2. 模块缓存恢复工作区快照、审核来源、生成路径、来源 key 与错误；`activeWorkspaceReviewSnapshot` 只有在 path 和来源 key 同时匹配当前路由时才可见。
3. 完整刷新先调用 workspace diff 服务，再异步解析返回的 raw diff；成功后更新 snapshot/path/key，并通知外层项目级 Git store。
4. 懒加载详情按来源、文件、上下文和 hunk 方向形成 detail key，经带 15 秒超时的服务请求与 worker 解析后合并进当前快照。
5. Git 文件动作先执行真实 stage/unstage/revert 等变更，再刷新外层共享 Git 摘要，最后重新加载当前审核来源。
6. 项目切换 effect 负责恢复缓存并决定是否强制刷新；来源切换负责清空基础快照与详情尝试状态，再由自动刷新 effect 加载新来源。

### 长期优化调整

- 新增 `useWorkspaceReviewRequestOwnership`，以已提交项目缓存路由和单调 request id 统一管理完整刷新、逐 key 详情和独占 Git 变更三类所有权。
- 完整刷新采用项目内 latest-wins；项目切换、来源切换或 Git 变更成功会显式使旧刷新失效，旧响应和 `finally` 均不能修改新路由 loading/state。
- 文件详情按 detail key 并行、同 key 合并；结果、错误、toast 和 `finally` 都必须持有准确 token，旧详情无法释放新项目同名详情。
- 详情的 loading/success/failure 函数式 updater 内再次校验 token，覆盖所有权检查与 React 真正执行 updater 之间的并发提交窗口。
- Git 变更增加同步独占 token。离开项目后仍调用捕获路径对应的 `onWorkspaceGitStateChanged` 更新共享项目状态，但仅当前项目 token 可以清空面板、显示结果、结束 pending 和启动当前来源刷新。
- 新增 `latestDiffReviewSelectionRef`，长生命周期 Git 变更完成后刷新用户最近提交的审核来源，而不是动作开始时的旧来源。
- 用 `hydratedReviewPanelCacheKey` state 替代提前改写的水合 ref；缓存写回、搜索和自动刷新只有在 React state 已归属当前项目 key 时执行。
- 新增 `lastHydratedReviewPanelCacheKeyRef`，保证缓存水合 effect 使用完整依赖却只在真实项目 key 变化时执行；layout effect 在项目提交时同步释放旧 loading/pending。
- 来源 key 真正变化时只由自动 effect 发起一次刷新；同一来源显式重聚焦时直接刷新一次且保留当前快照，删除原先“处理器 + effect”双 RPC 后靠序号丢弃一个的浪费。

### 文件变更

- 新增：`apps/vulcan-code-web/src/components/workspace/workspace-review-panel-request-ownership.ts`
- 修改：`apps/vulcan-code-web/src/components/workspace/WorkspaceReviewPanelHost.tsx`
- 修改：`TASK_LOG.md`
- 删除：无

### 修改范围代码审核与自动修复

- 审核确认审核缓存明确按项目路径共享，而不是按会话隔离；控制器 route key 保持项目级语义，没有错误扩大到 conversation id。
- 首版替换全局序号与 pending Set 后，审核发现来源处理器和自动 effect 会重复发起同一刷新，已自动删除变化来源的直接刷新入口。
- 审核 React updater 执行时机后，在详情 loading、成功和失败 updater 内补充二次所有权校验，避免通过检查后的排队更新污染新快照。
- 审核 Git 变更等待共享摘要的窗口后，新增完整刷新显式失效操作；变更成功立即撤销变更前 diff 请求，避免共享摘要等待期间旧快照回流。
- 审核同一来源、空 snapshot/key 的重聚焦场景后，恢复单次直接刷新分支；该分支不改变 effect 依赖 state，因此不会重新引入双 RPC。
- 保留会话 diff 的 cleanup + parse 序号，给 workspace 搜索补充水合门禁；复制 raw diff 等同步动作无需请求 token。
- CodeKit 反向搜索确认旧 `workspaceReviewRequestSeqRef`、`workspaceDetailPendingRef` 与提前匹配的 `hydratedReviewPanelCacheKeyRef` 已全部删除。

### 验证结果

- 首版项目请求所有权控制后 `npm run lint`：通过。
- 详情 updater 与重复刷新自动修复后 `npm run lint`：通过。
- 完整刷新失效与水合入口门禁补齐后 `npm run lint`：通过。
- 最终 `npm run lint`：通过。
- `npm run build`：通过，Vite 成功转换 2362 个模块并生成生产产物。
- `git diff --check`：通过，未发现空白错误。
- CodeKit AST 最终复查：4 个前端执行流/协议文件共解析 70 个结构项，错误为 0；审核面板与新控制器单独复查也无解析错误。

### 遗留问题与下一轮方向

- 懒加载详情仍会在路由失效后继续占用最多 15 秒的底层请求，但结果已被 token 隔离且超时控制器会终止请求；若服务桥后续提供统一外部 AbortSignal 所有权，可再把失效直接下推为即时取消。
- 文件面板与审核面板现在各有符合自身并发策略的控制器；在出现第三个相同模式前不急于抽象一个参数繁多的通用控制器，避免把 latest-wins、逐 key 并行和外部变更语义错误揉在一起。
- WebUI 包仍无现成单元测试运行器；本轮以多轮 TypeScript 校验、生产构建、CodeKit 调用链/AST 复查和逐异步 continuation 审核完成验证。
- 生产构建仍存在既有主工作台大 chunk 警告，本轮未改变加载策略。

## 第 21 轮：固定记忆动作会话归属并隔离删除与详情查询竞态（2026-07-10）

### 问题与事实依据

- `WorkbenchSnapshot.contextInjection` 是从 `contextInjectionByConversation` 按当前选中会话派生的投影，但 `WorkspaceMemoryPanel` 原先只接收快照和项目标识，没有接收产生该快照的会话标识。
- 原“仅本会话隐藏”请求不携带会话标识；service-kernel 在真正处理请求时调用 `selected_session_runtime_state_mut`，重新读取当时的全局选中会话。因此用户在请求等待期间切换会话时，动作可能修改新会话而不是发起动作的旧会话。
- 原删除逻辑只用 React `busyDeleteKey` 防重。状态提交前的同步双击窗口可重复提交；旧请求成功、失败或 `finally` 会无条件隐藏条目、显示通知、关闭新弹窗并清除新路由 busy。
- 原详情查询没有请求序号或路由所有权。较慢的旧查询可覆盖较新的查询，关闭弹窗后待返回结果可重新打开弹窗，切换会话后旧结果也可写入新会话面板。
- 乐观隐藏的 memory id、dedupe key、删除提示与弹窗状态在组件复用时不会随会话切换清理；相同 dedupe key 可使新会话条目被旧会话状态错误过滤。
- 后端 `suppress_context_injection_item` 明确只移除 `InjectionKind::Transient`，但静态卡片同样开放抑制按钮；前端还会无条件隐藏后端没有修改的静态条目，制造错误成功状态。

### 已查明的执行流程

1. 工作台状态通过 `deriveSelectedConversationFields` 使用 `selectedConversationId` 从 `contextInjectionByConversation` 找出唯一会话快照，再把它投影到顶层 `contextInjection`。
2. 右侧记忆页签从 `WorkspaceContextPanel` 取得该投影，并从草稿或已持久化会话解析项目 id；动态卡片代表按轮次生效的注入，静态卡片代表持续生效的注入。
3. 动态抑制请求进入 `service_run_vmm_memory_action` 的专用分支，按 dedupe key 优先、memory id 次选，从目标会话的 transient `context_injection_state` 移除条目并持久化该会话更新。
4. 长期删除走项目或全局 VMM 有效配置，解析为 `DeleteMemories` 并调用 VMM 管理服务；只有响应 `deleted_memory_ids` 含目标 id 时前端才做乐观隐藏。
5. 详情查询走同一项目或全局 VMM 上下文，解析为 `GetTurnDetails` 并按 turn id 获取完整对话详情。
6. 会话切换会替换顶层记忆投影，但旧组件实例与未完成 Promise 仍可继续存在，因此每个异步 continuation 必须同时证明发起路由和请求身份。

### 长期优化调整

- 在前后端 `VmmMemoryActionRequest` 契约增加动作级可选 `conversationId`；动态抑制调用必须传入当前已持久化会话，其他全局/项目管理动作不伪造会话依赖。
- 新增 `vmm_suppression_session_id`，严格拒绝缺失、空白、格式错误或已不存在的会话；服务端按该 id 直接取得 `session_runtime_state`，完全删除“回退当前选中会话”的错误路径。
- 抑制结果的请求审计投影增加 `conversationId`，便于后续诊断动作究竟落在哪个会话。
- 新增 `useWorkspaceMemoryRequestOwnership`：删除/抑制采用同步独占 token，补上 React state 提交前的双击窗口；详情查询采用 latest-wins token。
- token 同时包含会话与项目组成的确定性 route key 和挂载内单调 request id；成功、失败与 `finally` 都必须持有准确 token 才可更新 UI。
- 明确关闭详情弹窗会先撤销查询所有权；会话或项目路由提交时会使全部旧 token 失效，并清空仅属于旧路由的乐观隐藏、busy、提示和弹窗状态。
- 控制器返回值使用 `useMemo` 保持稳定身份；面板仅在 route key 真正变化时重置状态，避免初始挂载产生无意义的二次渲染。
- 删除目标增加 `suppressibleInSession` 能力，它只由已确认的动态卡片模式产生；静态持久化条目不再开放 transient 抑制动作。

### 文件变更

- 新增：`apps/vulcan-code-web/src/components/workspace/workspace-memory-panel-request-ownership.ts`
- 修改：`apps/vulcan-code-web/src/components/workspace/WorkspaceMemoryPanel.tsx`
- 修改：`apps/vulcan-code-web/src/components/workspace/WorkspaceContextPanel.tsx`
- 修改：`apps/vulcan-code-web/src/types/memory-vmm.ts`
- 修改：`crates/vulcan-service-api/src/vmm.rs`
- 修改：`crates/vulcan-service-kernel/src/vmm.rs`
- 修改：`crates/vulcan-service-kernel/src/vmm/memory_actions.rs`
- 修改：`crates/vulcan-service-kernel/src/vmm/tests.rs`
- 修改：`TASK_LOG.md`
- 删除：无

### 修改范围代码审核与自动修复

- 首次 Rust 定向测试发现新 helper 的 `WorkspaceSnapshot`、`SessionId` 不在子模块作用域，同时旧 selected-session helper import 已不再使用；已改为明确类型路径并删除失效 import，随后测试通过。
- 审核卡片来源后确认静态卡片也会构造相同删除目标；已把“是否支持会话抑制”编码为目标能力并禁用静态项按钮，避免以后再从展示位置猜测注入类型。
- 审核异步 continuation 后，在抑制与长期删除的成功、失败和 `finally` 三条路径全部增加 token 校验；旧路由请求仍可自然完成真实外部动作，但无权修改当前面板。
- 审核详情弹窗关闭路径后增加显式 invalidation，旧 Promise 返回后不能重新创建弹窗；同一路由连续查询也只有最新请求可更新结果。
- 审核控制器对象身份和路由清理后增加 memo 化，并用 `stateRouteKeyRef` 跳过初始空状态重置，消除不必要回调抖动与额外渲染。
- CodeKit 逐层复查请求类型、工作台会话投影、卡片 mode、service-kernel 分派、VMM action 解析和会话持久化路径；未保留候选字段或当前选中状态 fallback。

### 验证结果

- `cargo fmt --all`：通过。
- 新增定向测试 `vmm_suppression_session_id_uses_explicit_conversation`：通过，证明当前选中会话与请求会话不同时仍解析请求会话，并拒绝缺失或已失效会话。
- `cargo test -p vulcan-service-kernel`：通过，2 个测试套件共 345 项全部通过。
- 最终 `npm run lint`：通过。
- 最终 `npm run build`：通过，Vite 成功转换 2363 个模块并生成生产产物。
- `git diff --check`：通过，未发现空白错误。
- CodeKit AST 最终复查：7 个前后端实现、契约与测试文件共解析 98 个结构项，错误为 0。
- 严格 Clippy 已执行两次：workspace 依赖检查被 `vulcan-workspace-store` 8 个既有告警拦截，`--no-deps` 检查被 service-kernel 其他未修改文件 4 个既有告警拦截；诊断均不指向本轮修改文件，因此未越界改动用户已有工作。

### 遗留问题与下一轮方向

- WebUI 包仍无现成单元测试运行器；本轮以 Rust 精确单测、345 项全量 kernel 测试、多轮 TypeScript 校验、生产构建和逐 continuation 审核覆盖验证。
- VMM 长期删除和详情查询仍不能由前端 route token 中止底层请求，但结果已经完全隔离；若服务桥未来提供 AbortSignal/取消句柄，可再把 UI 失效下推为传输取消。
- 记忆管理主页面 `MemoryArea` 也包含长生命周期 VMM action 与 scope 切换，下一轮应先追踪其 action busy、结果写回和项目切换链路，再判断是否存在同类但语义不同的竞态。
- 生产构建仍存在既有主工作台大 chunk 警告，本轮未改变加载策略。

## 第 22 轮：绑定 VMM 项目草稿并串行化管理页异步操作（2026-07-10）

### 问题与事实依据

- `VmmMemorySnapshot.projectDraft` 由后端按选中会话项目构建，并与 `selectedProjectId` 一一对应；它不是任意项目都可复用的通用草稿。
- `MemoryArea` 原项目下拉框只执行 `setProjectId`，没有调用仓库已经存在的 `service_load_project_vmm_config`。用户从项目 A 切到 B 后，界面仍编辑 A 的 `projectDraft`，保存时却提交 `projectId=B`，会把 A 表单内容写入 B。
- 原 snapshot effect 每次收到任意新快照都会无条件覆盖 global/project draft、project id 与 notice；其他工作台动作刷新快照时，可直接抹掉尚未保存的配置编辑。
- 原页面只有一个 `busyAction: string | null`，但按钮仅在自身 action 名称匹配时禁用。不同动作可并发；后发动作覆盖 busy 后，先发动作的 `finally` 又会提前清空 busy，结果和 notice 也按返回顺序互相覆盖。
- 原结果、notice 与 scope/project 切换没有所有权边界；切换作用域后仍展示旧作用域结果，项目加载期间也可能短暂把旧项目草稿渲染到新项目标识下。
- 浏览器 fallback 的 `loadProjectVmmConfig(projectId)` 完全忽略参数，始终返回单个快照 `projectDraft`；fallback 保存任意项目也会覆盖当前快照项目并修改其 effective 投影。

### 已查明的执行流程

1. service-kernel 的 `vmm_memory_snapshot` 先从选中会话解析项目，读取该项目缓存 VMM layer，生成与 `selectedProjectId` 对应的 `projectDraft`、effective 和当前项目 notice。
2. 任意其他项目草稿必须通过 `service_load_project_vmm_config(projectId)`，由项目 id 解析路径、加载准确 project layer，并基于全局配置生成可编辑 fallback。
3. 配置保存按 request scope 分流：项目配置写入请求项目的项目配置文件与缓存；全局配置写入 provider 配置；随后服务刷新完整 WorkbenchSnapshot。
4. `WorkbenchLayout.applySkillWorkbenchAction` 合并保存返回的完整快照，但返回快照里的 `projectDraft` 仍只描述当前会话项目，不保证等于管理页下拉框选择项目。
5. 管理动作通过 `runVmmMemoryAction` 返回局部结果，不刷新工作台；搜索结果删除后需要保留搜索响应并移除已确认删除的 memory id。
6. 管理页同时存在完整 feature 页面与工作区内嵌设置入口，两者共用相同 `MemoryActionProps`，因此准确项目加载能力必须接入统一动作边界。

### 长期优化调整

- 新增 `useMemoryAreaController`，集中管理 scope、project route、逐项目草稿、全局草稿、脏状态、result、notice 与唯一异步操作所有权；`MemoryArea` 缩减为表单字段和渲染编排。
- `MemoryActionProps` 新增 `onLoadProjectVmmConfig(projectId)`，完整 feature 与内嵌入口都接入现有 service；项目下拉切换必须先加载准确草稿，再允许编辑或保存。
- 使用 `Map<projectId, {draft, dirty}>` 保存逐项目草稿。未保存项目切走再返回时保留本地编辑；干净项目重新选择时从服务刷新；权威项目目录删除的缓存会被清理。
- 快照只更新它明确声明的 `selectedProjectId` 缓存行；dirty global/project draft 不接受快照覆盖。保存成功也只有在请求期间未继续编辑时才清除 dirty。
- 项目加载期间 `projectDraftReady=false`，设置表单不渲染，彻底禁止旧项目草稿挂到新项目 id；加载失败恢复先前项目路由。
- 所有项目加载、VMM action 与配置保存共用同步独占 operation ref；React busy state 提交前就阻止双击或交叉提交，成功、失败与 `finally` 只允许准确 request id 更新。
- scope 与 project 控件、顶部动作、分区动作和搜索行删除入口在任一操作运行时统一禁用；当前 action 仍显示运行状态，其他按钮保持原标签但不可提交。
- scope/project 切换清空旧 result。快照 notice 只在全局作用域或本地项目等于快照项目时同步；另增中英文项目加载/选中文案，避免项目 B 表单旁展示项目 A 诊断。
- service callback 通过 ref 读取，避免上层内联 action 对象导致整个控制器回调持续重建；控制器返回值保持 memo 化。
- 浏览器 fallback 新增逐项目 override 映射：无 override 项目从 global draft 派生 `inherit` 草稿；已保存 override 按准确 project id 返回；声明有 override 却缺少载荷时显式报错。保存非当前项目不再修改当前项目 draft/effective。

### 文件变更

- 新增：`apps/vulcan-code-web/src/components/features/memory/use-memory-area-controller.ts`
- 修改：`apps/vulcan-code-web/src/components/features/memory/MemoryArea.tsx`
- 修改：`apps/vulcan-code-web/src/components/features/memory/vmm-memory-components.tsx`
- 修改：`apps/vulcan-code-web/src/components/features/memory/vmm-memory-sections.tsx`
- 修改：`apps/vulcan-code-web/src/components/workspace/workbench-layout.tsx`
- 修改：`apps/vulcan-code-web/src/service/memory-actions-service.ts`
- 修改：`apps/vulcan-code-web/src/i18n-messages.ts`
- 修改：`TASK_LOG.md`
- 删除：无

### 修改范围代码审核与自动修复

- 首轮控制器类型检查通过后，审核无项目工作区发现空 project id 会被误判为“项目已删除”并撤销全局保存；已区分稳定无项目路由与真实项目移除。
- 审核上层 `memoryActions` 内联对象后，用 `actionsRef` 提供最新服务回调，删除无意义的 callback 级联重建。
- 审核项目目录生命周期后，改为在每次权威快照进入时清理已删除项目缓存，同时只水合快照明确拥有的项目行。
- 审核 notice 来源后确认后端 notice 包含当前会话项目名称；已增加项目路由专用文案，并限制不同项目下的快照 notice 回流。
- 审核保存期间继续输入的场景后，以 draft 对象身份判断请求后是否产生新编辑；有新编辑时保持 dirty，不把尚未保存的新版本误标为干净。
- 审核项目保存的非空断言后改为显式分支收窄；项目 id、缓存 entry 或加载状态不满足时直接拒绝保存，不保留 `projectEntry!`。
- 审核 browser fallback 后新增准确项目验证与 override 存储，并自动修复“清除 override 后仍返回旧 selected projectDraft”的执行顺序问题：先以更新后的 `hasOverride=false` 决定从全局派生。
- CodeKit 反向核对 snapshot 构造、project load/save helper、service RPC、两个 UI 入口和全部 action button；未用当前快照草稿作为其他项目 fallback。

### 验证结果

- 控制器首次接线后 `npm run lint`：通过。
- 无项目路由、callback 稳定性与缓存清理自动修复后 `npm run lint`：通过。
- browser fallback 精确项目修复后 `npm run lint`：通过。
- 最终 `npm run build`：通过，Vite 成功转换 2364 个模块并生成生产产物。
- CodeKit AST 最终复查：9 个前端实现、服务与后端事实来源文件共解析 85 个结构项，错误为 0。
- 本轮文件尾随空白 `rg` 扫描无匹配。
- 全工作树 `git diff --check` 未报告任何 `file:line` 空白错误，但当前 Windows Git 对大量既有 LF 工作副本输出 `LF will be replaced by CRLF` 安全换行告警，RTK 因这些告警返回非零；本轮未批量改写用户已有文件行尾。

### 遗留问题与下一轮方向

- WebUI 包仍没有现成单元测试运行器，本轮未为单一 hook 引入新测试框架；以多轮 TypeScript 校验、生产构建、AST 检查、后端实现核对和逐异步 continuation 审核完成验证。
- 全局保存与“当前项目存在 override”时的 browser preview effective 仍是近似展示，因为 fallback snapshot 没有字段级 override layer，无法准确重算继承；真实 service 路径不受影响，后续若要增强 preview 应扩展其数据模型，不能继续猜字段归属。
- `applySkillWorkbenchAction` 对所有返回完整快照的 feature 动作统一直接合并；本轮通过记忆页面独占通道消除了同一页面的并发保存，但下一轮可先追踪 skills/agents 页面是否允许多个完整快照动作并发，再决定是否需要共享 revision 所有权。
- 生产构建仍存在既有主工作台大 chunk 警告，本轮未改变加载策略。

## 第 23 轮：拆分 Skills/Agents 领域版本并消除异步刷新倒灌（2026-07-10）

### 问题与事实依据

- LuaSkills 生命周期动作原先都在操作完成后调用通用 `refresh_workbench_snapshot`；Agent Skills 刷新、智能体目录保存/删除/恢复等动作也混用完整 `WorkbenchSnapshot`，而启停动作只返回不带版本的局部数据，返回契约没有统一的所有权边界。
- WebUI 的 `WorkbenchDelta` 已逐项核实，只包含消息、会话阶段、待发送提示词、命令会话、上下文、子智能体导航等热路径，不包含 LuaSkills、Agent Skills 或智能体目录增量。
- 轻量启停响应会推进后端 `workbench_generation`，但前端只替换目录数据而不推进任何可比较版本。若无关增量先到、轻量响应后到，前端无法区分同一领域的新旧响应；旧版整目录乐观回滚还会覆盖并发成功动作。
- 仅给轻量响应补全局 revision 仍不正确：无关增量可以拥有更高全局版本，却不携带目录内容。按全局版本拒绝较低的目录响应会丢失已成功目录变更，因此必须维护领域独立接收版本。
- 通用工作台刷新会在异步工作区加载前捕获 LuaSkills catalog、Provider 配置与智能体目录投影，异步 I/O 完成后再以更高 generation 覆盖整个 `guard.workbench`。这会产生“高版本旧数据”，前端再严格的版本门禁也无法识别。
- `mutate_provider_configuration` 原先在无事务锁的情况下执行“克隆内存配置—变更—保存文件—回存内存”；两个 Agent Skills 或 Lua 工具注册请求可同时克隆同一旧版本，后保存者覆盖先保存者。
- 智能体保存、删除、恢复、启停与刷新分别执行目录读取、文件写入和运行时重投影，没有共享目录事务；并发请求可能读取另一个请求的文件中间态。
- Agent Skills、LuaSkills、智能体页面与标题栏的忙碌状态都只依赖 React state；在 state 提交前仍存在同步重复点击窗口，同一行智能体启停也能重复提交。

### 已查明的执行流程

1. LuaSkills 安装、更新、启停、卸载先在 blocking runtime 任务中变更引擎，再把返回 catalog 写入 service cache，旧实现最后重建整个工作台。
2. Agent Skills 全局/单项启停通过 Provider 配置修改持久化状态，再调用 `agent_skills_catalog_summary` 从确定的 system/user roots 重投影目录。
3. 智能体启停写入 user sidecar override，同时修改保留的 `subagentCatalog` 和 `TaskRuntime`；保存、删除、恢复则修改 user catalog 文件后完整重载两层目录并替换运行时 catalog。
4. 后端每次上述聚焦变更都会推进统一 `workbench_generation`，但领域之间没有增量事件，因此前端只能通过响应携带的完整领域数据完成同步。
5. 前端完整快照、热路径 delta 与领域响应可能任意交错；完整快照包含全部领域，delta 只包含声明的局部字段，领域响应只拥有自身目录。因此全局 revision 与各领域已接收 revision 必须分层维护。
6. 通用刷新在开始时捕获 generation 和输入，经过异步工作区加载后再提交；若期间 generation 已变化，当前内存里的聚焦目录才是权威值，通用刷新只能提交其他重建字段。

### 长期优化调整

- 新增 `LuaSkillsCatalogUpdate`、`AgentSkillsCatalogUpdate` 与 `SubagentCatalogUpdate`，统一返回“已提交 workbench revision + 完整领域目录”；LuaSkills 响应同时携带由同一 catalog 派生的 plugin rows。
- LuaSkills 刷新、安装、更新、启停、卸载和工具注册全部改用聚焦 `refresh_luaskills_local_state`，不再为了单一目录动作执行异步完整工作台重建。
- Agent Skills 刷新与启停统一返回聚焦领域响应；智能体刷新、打开目录、保存、删除、恢复和启停统一返回完整智能体目录领域响应，删除旧的单行补丁响应协议。
- WebUI 新增 LuaSkills、Agent Skills、智能体目录三个独立 revision ref。领域响应按自身 ref 判定新旧，同时只用 `max` 推进全局 workbench revision；无关 delta 不能阻止领域响应，较新完整快照又能淘汰排队中的旧领域响应。
- 完整快照成功合并或预占时同步推进全部领域 ref；会话水合与普通 delta 不冒充目录所有者。
- 删除 Agent Skills/智能体启停的整目录乐观覆盖和回滚，改为服务确认后应用带版本完整领域，彻底消除旧 rollback 覆盖其他并发成功结果的路径。
- LuaSkills 生命周期服务新增异步事务锁，覆盖“运行时变更—catalog 存储—领域投影”全过程；工具注册也进入同一 LuaSkills 事务，响应不会投影运行时目录中间态。
- Agent Skills 与 Lua 工具注册使用的 Provider 配置修改新增进程级事务锁；配置读取与目录投影也使用同一锁顺序，消除同一技能配置域内的 clone-save lost update。
- 智能体目录的刷新、保存、删除、恢复、启停共享进程级文件事务锁；目录发现、文件变更、TaskRuntime 替换和 WebUI 投影被串成一个确定事务。打开外部文件管理器前主动释放锁，缩短临界区。
- 通用工作台刷新记录捕获 generation；提交时若发现并发 generation 已推进，调用纯函数保留当前内存中的 LuaSkills、plugins、Agent Skills 与智能体目录，阻止较早刷新以更高版本倒灌旧目录。
- 浏览器 fallback 的每次变更现在单调推进 revision，并同步重算 LuaSkills entry count 与 plugin rows；所有 Skills/Agents fallback 接口返回和真实 service 一致的领域契约。
- TypeScript `WorkbenchSnapshot` 补齐后端一直存在的 `plugins` 字段，消除前后端契约缺口。
- Agent Skills、LuaSkills 面板、Lua 安装表单、Skills 标题栏和智能体页加入同步 ref 守卫；智能体同一行重复启停被拒绝，不同行仍可并发，目录级动作与任意行级变更互斥。

### 文件变更

- 修改：`crates/vulcan-service-api/src/skills_catalog.rs`
- 修改：`crates/vulcan-service-kernel/src/lib.rs`
- 修改：`crates/vulcan-service-kernel/src/service_state_access.rs`
- 修改：`crates/vulcan-service-kernel/src/settings_runtime_projection.rs`
- 修改：`crates/vulcan-service-kernel/src/skills_services.rs`
- 修改：`crates/vulcan-service-kernel/src/workbench_refresh.rs`
- 修改：`apps/vulcan-code-web/src/types/composer-skills.ts`
- 修改：`apps/vulcan-code-web/src/types/workbench.ts`
- 修改：`apps/vulcan-code-web/src/service/browser-fallback-workbench.ts`
- 修改：`apps/vulcan-code-web/src/service/skills-actions-service.ts`
- 修改：`apps/vulcan-code-web/src/components/workspace/workbench-layout.tsx`
- 修改：`apps/vulcan-code-web/src/components/workspace/workbench-layout/project-actions.tsx`
- 修改：`apps/vulcan-code-web/src/components/workspace/workspace-shell.tsx`
- 修改：`apps/vulcan-code-web/src/components/features/feature-shell.tsx`
- 修改：`apps/vulcan-code-web/src/components/features/skills/agent-skills-panel.tsx`
- 修改：`apps/vulcan-code-web/src/components/features/skills/lua-skills-panel.tsx`
- 修改：`apps/vulcan-code-web/src/components/features/agents/AgentsArea.tsx`
- 修改：`TASK_LOG.md`
- 新增：无
- 删除：无

### 修改范围代码审核与自动修复

- 首次前端类型检查发现 `WorkbenchSnapshot.revision` 在 TypeScript 契约中仍为可选；已在 browser fallback 版本推进与领域响应投影处按现有明确类型契约归一为 0，再由首次变更推进到正版本，随后类型检查通过。
- 审核 delta 枚举后确认目录领域没有增量事件，否决了“直接复用全局 revision”的不完整方案，改用三个领域独立 revision ref。
- 审核通用刷新函数后发现异步 I/O 前捕获旧目录、最终以高 generation 全量覆盖的后端根因；新增并发提交保留函数及正反两项单元测试，而不是在前端增加候选式 fallback。
- 审核 LuaSkills `plugins_from_luaskills` 后确认 plugins 与 Lua catalog 同源；领域响应、前端 Workbench 类型和 browser fallback 都改为原子更新两者。
- 审核 Lua 生命周期 blocking continuation 后增加覆盖完整 lifecycle 的 Tokio mutex；进一步把工具注册纳入同一锁顺序，避免注册响应与安装/更新 catalog 交叉。
- 审核 Agent Skills 配置写入后确认两个请求会从相同 retained config 克隆；新增技能配置事务锁，并让聚焦目录刷新也在该锁内捕获配置。
- 审核智能体目录文件流后把刷新、保存、删除、恢复、启停纳入同一文件事务；随后缩短“打开用户目录”的持锁范围，外部 shell 动作不再阻塞目录写入。
- 审核 React 同步窗口后为页面级动作、行级启停、安装表单与两个标题栏入口增加 ref 门禁；智能体目录级按钮在任一行变更期间明确禁用，避免静默交叉请求。
- CodeKit 最终复查 8 个核心前后端文件，共解析 241 个结构项，错误为 0；锁顺序固定为 Lua 生命周期锁 → 技能配置锁 → service state 锁，未发现反向获取路径。

### 验证结果

- 新增定向测试 `focused_catalog_domains_keep_generic_refresh_without_concurrent_commit`：通过，证明无并发提交时通用刷新保留自身刚重建目录。
- 新增定向测试 `focused_catalog_domains_preserve_newer_concurrent_commit`：通过，证明 generation 变化时 LuaSkills、plugins、Agent Skills 与智能体目录全部保留较新内存提交。
- `cargo test -p vulcan-service-kernel`：通过，2 个测试套件共 347 项全部通过。
- `cargo test -p vulcan-service-api`：通过，2 个测试套件共 20 项全部通过。
- 最终 `npm run lint`：通过。
- 最终 `npm run build`：通过，Vite 成功转换 2364 个模块并生成生产产物。
- 本轮文件范围 `git diff --check`：通过，未发现空白错误。
- 严格 `cargo clippy -p vulcan-service-api -p vulcan-service-kernel --all-targets -- -D warnings` 已执行；被 `vulcan-workspace-store` 的 8 个既有告警阻断，诊断均不指向本轮文件，本轮未越界修改用户已有工作。

### 遗留问题与下一轮方向

- Provider 配置仍有 host tools、MCP、provider、Git、VMM、模型路由等多条直接 `save_configuration_file` 写入路径，它们没有共享本轮技能配置事务锁；下一轮应完整追踪所有写入者，建立单一 Provider 配置事务入口，不能继续按页面各自 clone-save。
- 通用工作台刷新目前只对本轮已明确拥有聚焦事务的 Skills/Agents 目录做并发保留；providers、MCP、VMM 等其他领域也可能存在“异步前捕获、异步后高版本覆盖”，需要在统一配置事务完成后逐领域核实所有权。
- WebUI 仍没有单元测试运行器；领域门禁通过 TypeScript、生产构建、后端正反单测、完整 Rust 测试和逐时序审核验证，后续若引入前端测试框架应优先补交错顺序用例。
- 生产构建仍存在既有主工作台大 chunk 警告，本轮未改变加载策略。

## 第 24 轮：统一全局 Provider 配置事务并消除跨页面丢更新（2026-07-10）

### 问题与事实依据

- 全仓库事实搜索确认，service-kernel 原有 14 个全局配置写入服务直接调用 `save_configuration_file`，另有上一轮技能 helper 调用同一 API；写入者横跨 Git、RTK host tools、MCP、Provider、模型路由、VMM 与 Skills。
- Git、RTK、Provider 和模型路由原先在持有 service state 锁时执行 clone-save-store，因此它们彼此之间可以串行；但这种互斥只是偶然依赖实现细节，没有覆盖锁外写入者。
- MCP 保存/移除会先从 state 克隆配置，再异步重建 MCP Runtime，最后保存并回存。在异步重建期间，其他写入者可以提交新配置；MCP 随后保存旧克隆时会把期间所有字段变更整体抹掉。
- VMM 全局保存与旧技能 helper 同样在 service state 锁外执行“克隆—变更—保存—重新加锁回存”，可以与任何设置页写入互相覆盖。
- 单纯给每个页面增加局部 mutex 仍无法形成闭环；配置文件、内存 `AgentRuntimeBridge.provider_configuration`、MCP Runtime 和 Workbench 投影必须属于同一个跨请求族事务。
- 项目级 VMM 与 Agent Skill 配置写入的是独立 `ProjectConfigurationFile`，不属于全局 Provider 配置文件；本轮明确不把不同持久化对象混入同一全局锁。

### 已查明的执行流程

1. 全局配置权威内存副本位于 `ServiceBridgeState.agent_runtime.provider_configuration`；每个写入动作都从这里克隆下一版本。
2. 普通设置动作完成校验与字段变更后保存全局配置文件，再替换运行时副本，并在同一 state 临界区更新 settings、provider、model、reasoning、VMM hook 或 session runtime control 投影。
3. MCP 是唯一需要在持久化前执行异步准备的写入者：下一份配置先用于构建新的 MCP Runtime，构建失败时禁止保存；保存成功后才替换 retained Provider 配置与 MCP Runtime。
4. 模型、OpenRouter 二级路由与 reasoning 保存还会更新选中会话 runtime state，并异步持久化 session addon；该后台持久化已经按会话和 namespace 分配版本，旧任务在真正写盘前会被拒绝。
5. Agent Skills 与 Lua 工具注册在配置提交后重新投影各自领域；领域刷新必须与全局配置事务使用同一锁顺序，不能在写入中间读取。

### 长期优化调整

- 新增进程级异步 `ProviderConfigurationTransaction`，其生命周期覆盖校验、可选异步准备、文件持久化、运行时替换与投影提交。
- 把 `save_configuration_file` 封装为事务对象的 `persist` 方法；service-kernel 全仓搜索现在只剩该方法内部唯一调用点，其他模块不再拥有绕过事务的全局配置保存权限。
- 将 `mutate_provider_configuration` 升级为异步泛型 mutate/commit helper：取得全局事务后锁定 service state，从最新 retained 配置克隆，执行字段变更与校验，持久化，替换运行时副本，最后在同一 state 临界区执行调用方投影。
- Git 提交设置、RTK 路由、供应商新增/移除、启用模型、模型能力覆盖、工作台模型、OpenRouter 二级路由、reasoning、thinking display、WebUI locale、VMM 全局配置与 Skills 配置全部迁移到统一 helper。
- MCP 保存与移除使用显式事务对象：先在事务内克隆最新配置并异步构建 Runtime，再通过事务唯一持久化入口保存，最后回存 Provider 与 MCP Runtime；新 Runtime 保存失败时仍会按原语义 shutdown。
- `refresh_agent_skills_local_state` 与 `refresh_luaskills_local_state` 改为异步并取得同一 Provider 配置事务，保证目录投影不会观察到配置写入中间态。
- VMM 配置 helper 改为异步；项目分支继续写独立项目配置文件，全局分支进入统一 Provider 配置事务，并在 commit 阶段刷新 VMM hook snapshot。
- 所有普通事务固定锁顺序为 Provider transaction → service state；Lua 工具注册额外保持 Lua lifecycle transaction → Provider transaction → service state。源码搜索未发现反向获取路径。
- 新增异步互斥测试，通过 oneshot 精确证明首个 Provider 事务释放前第二个事务无法取得所有权，不依赖 sleep 猜测调度。

### 文件变更

- 修改：`crates/vulcan-service-kernel/src/service_state_access.rs`
- 修改：`crates/vulcan-service-kernel/src/settings_runtime_projection.rs`
- 修改：`crates/vulcan-service-kernel/src/skills_services.rs`
- 修改：`crates/vulcan-service-kernel/src/git/settings.rs`
- 修改：`crates/vulcan-service-kernel/src/host_tools_settings.rs`
- 修改：`crates/vulcan-service-kernel/src/mcp_settings.rs`
- 修改：`crates/vulcan-service-kernel/src/provider_settings.rs`
- 修改：`crates/vulcan-service-kernel/src/workbench_model_routes.rs`
- 修改：`crates/vulcan-service-kernel/src/vmm/config.rs`
- 修改：`crates/vulcan-service-kernel/src/vmm/memory_actions.rs`
- 修改：`TASK_LOG.md`
- 新增：无
- 删除：无

### 修改范围代码审核与自动修复

- 首轮编译发现 Git、RTK、Provider、VMM 与模型路由子模块没有继承 root 的 helper 名称；已按真实模块边界补充明确 import，并删除 RTK 文件不再使用的 `provider_config::self` import。
- 首轮编译还发现 locale 值同时被 mutate 与 commit 闭包使用时发生借用/移动冲突；commit 改为克隆稳定字符串，保持配置文件与 startup 投影完全一致。
- 严格 Clippy 发现 VMM commit 只是原函数的冗余闭包；已直接传入 `refresh_guard_vmm_hook_snapshot` 函数项。
- 审核失败路径确认：普通 helper 在保存成功前不替换内存配置；MCP Runtime 构建失败不保存，保存失败会 shutdown 新 Runtime；校验错误不会产生文件或内存副作用。
- 审核事件顺序确认：事务在事件广播前释放，允许后续配置事务推进；即使广播网络顺序交错，既有 workbench revision 门禁会拒绝旧快照。
- 审核模型 addon 后台持久化确认已有 `next_webui_session_addon_persist_version` 与 current-version 检查，旧模型/推理任务不会倒序覆盖较新 session runtime controls。
- CodeKit 最终复查 10 个事务、写入服务、投影与 VMM 文件，共解析 197 个结构项，错误为 0。

### 验证结果

- 首轮 `cargo check -p vulcan-service-kernel` 暴露 14 个导入/所有权问题；自动修复后再次执行通过。
- 新增测试 `provider_configuration_transactions_are_exclusive`：通过，证明跨 Tokio task 的全局事务互斥。
- 最终 `cargo test -p vulcan-service-kernel`：通过，2 个测试套件共 348 项全部通过。
- 全仓 `save_configuration_file` 搜索：service-kernel 仅剩 `ProviderConfigurationTransaction::persist` 内 1 个调用点。
- 本轮文件范围 `git diff --check`：通过，未发现空白错误。
- 严格 `cargo clippy -p vulcan-service-kernel --all-targets --no-deps -- -D warnings` 已复跑；本轮冗余闭包修复后只剩其他既有文件 4 个告警，诊断不指向本轮修改文件。

### 遗留问题与下一轮方向

- `ProjectConfigurationFile` 仍由项目级 VMM 保存和项目级 Agent Skill 覆盖分别执行 load-modify-save；它们修改同一项目文件的不同字段但没有共享事务，并发时仍可能互相覆盖。下一轮应按规范项目路径建立逐项目事务入口。
- MCP Runtime 重建期间全局 Provider 事务会保持独占，保证正确性但会延迟其他配置写入；若后续有真实延迟证据，可升级为“捕获配置版本—锁外预构建—提交时比较并重试”，不能直接缩短锁而恢复丢更新。
- 普通 mutate/commit 当前在 service state 锁内执行配置文件保存，延续了多数旧写入者的阻塞模型；若测得状态热路径受影响，应把 retained 配置增加独立版本并采用两阶段提交，而不是让调用方重新各自保存。
- 本轮仅修改 Rust 后端配置事务，没有前端产物变化；上一轮前端生产构建结果仍有效。

## 第 25 轮：统一项目配置逐文件事务并阻断损坏配置覆盖（2026-07-10）

### 问题与事实依据

- 项目级 VMM 配置与项目级 Agent Skill 覆盖都写入同一个 `ProjectVulcanCodeConfigurationFile`，但原实现分别执行 load-modify-save；两个请求并发时会从同一旧版本出发，后保存者会抹掉先保存者对另一字段族的修改。
- `vmm_project_scope_path` 从保留的 `TaskProjectSummary` 取得项目的精确工作区根目录，provider-config 再通过 `project_configuration_file_path` 唯一映射到 `<workspace>/.vulcan/code/config.json`；因此配置文件规范路径是锁身份的事实来源。
- 项目配置与全局 Provider 配置是不同持久化对象。把所有项目接入上一轮的全局锁虽然能遮蔽竞态，但会让无关项目互相阻塞，不符合长期并发模型。
- 项目配置加载器对文件不存在明确返回默认配置，但对读取或 JSON 解析失败返回错误；事务层必须保持这个契约，不能把“文件损坏”误判为“文件不存在”。

### 已查明的执行流程

1. VMM 项目保存由 RPC 分发进入 `service_save_vmm_config`，项目分支先通过 project id 查询保留工作区路径，再更新项目配置中的 VMM 字段，保存成功后刷新内存 VMM 配置缓存与 hook snapshot。
2. Agent Skill 项目覆盖由 RPC 分发进入 `service_set_agent_skill_project_state`，同样按 project id 解析工作区路径，再更新项目配置中的 prompt skill 覆盖字段，并在成功后刷新 Agent Skills catalog 投影。
3. 两条链路最终调用 provider-config 的同一个 `save_project_configuration_file`，目标均为规范项目配置文件；原有互斥只覆盖各自函数内的局部 state 操作，没有覆盖完整 read-modify-save 周期。
4. 项目配置保存本身使用完整文件序列化，因此即使两条业务链修改不同字段，只要基于旧克隆保存，就会发生整文件丢更新。

### 长期优化调整

- 新增按规范配置文件路径索引的项目配置事务注册表；注册表保存 `Weak<tokio::sync::Mutex<()>>`，活跃事务共享同一锁，事务结束后条目可回收，避免长期运行中项目路径无限积累。
- 新增 `begin_project_configuration_transaction`，为同一配置文件提供 owned async guard；不同项目文件使用不同 mutex，可安全并发。
- 新增统一异步 `mutate_project_configuration` helper，事务范围完整覆盖“解析规范路径—加锁—加载最新文件—业务 mutation—整文件保存—内存 commit”。
- 将项目级 VMM 保存和项目级 Agent Skill 覆盖迁移到统一 helper；RPC 的 Agent Skill 项目状态分支同步改为 await，调用链不再存在绕过事务的保存路径。
- 保持全局 Provider 配置与项目配置使用独立事务域，不引入无关阻塞，也不改变项目路径和配置 schema。
- 新增逐文件并发测试，使用两个项目路径证明同文件等待、异文件可继续，不用 sleep 猜测 Tokio 调度。
- 新增损坏文件回归测试，使用生产路径解析器写入非法 JSON，确认事务返回加载错误、业务 mutation 不执行、原文件逐字节保持不变。

### 文件变更

- 修改：`crates/vulcan-service-kernel/src/service_state_access.rs`
- 修改：`crates/vulcan-service-kernel/src/skills_services.rs`
- 修改：`crates/vulcan-service-kernel/src/vmm/config.rs`
- 修改：`crates/vulcan-service-kernel/src/rpc_dispatch.rs`
- 修改：`TASK_LOG.md`
- 新增：无
- 删除：无

### 修改范围代码审核与自动修复

- 审核锁身份确认使用 provider-config 的规范配置文件路径，而非 project id 或未经映射的输入字符串；同一物理配置目标不会被业务入口拆成不同锁。
- 审核锁生命周期确认逐文件 guard 覆盖完整 read-modify-save-commit 周期；注册表 mutex 只保护弱引用表，不跨 await 持有，不会把不同项目重新串行化。
- 审核失败路径确认 mutation 错误和保存错误都不会执行内存 commit；VMM 缓存、hook snapshot 与 Agent Skills catalog 只在持久化成功后更新。
- 代码审核发现初版事务层沿用了只读 helper 的 `unwrap_or_default`，这会在现有项目配置 JSON 损坏时从默认值继续 mutation 并覆盖原文件。已自动修复为直接传播 `load_project_configuration_file` 错误，并补专门回归测试锁定行为。
- 全仓搜索确认 service-kernel 内 `save_project_configuration_file` 只剩统一事务 helper 内 1 个调用点，VMM 与 Skills 已无旁路保存。
- CodeKit 对项目事务、VMM、Skills 与 RPC 4 个文件进行结构复查，共解析 113 个结构项，错误为 0；新增回归测试随后通过单文件 rustfmt 与完整内核测试验证。

### 验证结果

- `rustfmt --edition 2021 --check crates/vulcan-service-kernel/src/service_state_access.rs`：通过。
- 新增测试 `project_configuration_transactions_are_keyed_by_file`：通过，证明同文件串行且异文件不互相阻塞。
- 新增测试 `project_configuration_mutation_preserves_malformed_file`：通过，证明解析失败不会被默认配置吞掉或覆盖。
- 最终 `cargo test -p vulcan-service-kernel`：通过，2 个测试套件共 350 项全部通过。
- service-kernel 全仓 `save_project_configuration_file` 搜索：仅剩事务 helper 内 1 个调用点。
- 本轮 4 个 Rust 文件范围 `git diff --check`：通过，未发现空白错误。

### 遗留问题与下一轮方向

- provider-config 的全局配置与项目配置保存仍直接使用 `fs::write` 覆盖目标文件；进程内事务解决了丢更新，但断电、磁盘错误或进程中断仍可能留下半写文件。下一轮应追踪所有配置持久化入口，设计同目录临时文件、同步与原子替换协议。
- 只读项目配置调用方中仍可能存在主动 `unwrap_or_default` 的路径；这些路径是否应该容忍损坏文件需要沿每个用户可见调用链逐一确认，不能在事务 helper 中统一猜测。
- 逐文件锁以规范路径的词法结果为 key；当前所有入口都来自同一工作区保留路径和同一 resolver。若未来允许符号链接或多种路径表达指向同一项目，应在项目注册边界建立单一规范路径，而不是在事务层增加候选路径兼容。
- 本轮只修改 Rust 后端项目配置事务，没有前端产物变化；第 23 轮前端生产构建结果仍有效。

## 第 26 轮：统一 Provider 配置原子持久化并消除截断文件风险（2026-07-10）

### 问题与事实依据

- 全局 Provider 配置、项目级 Vulcan Code 配置与独立 AI 能力配置都先生成完整 JSON 字符串，再直接调用 `fs::write` 覆盖目标文件。
- `fs::write` 会以截断方式打开已存在文件；若进程、系统或存储在截断后、完整写入前失败，下一次启动只能看到半写 JSON，并进入解析失败路径。
- 前两轮建立的进程内事务只能保证 read-modify-save 顺序，不能改变单次保存的磁盘发布语义；内存并发正确并不等于崩溃一致性。
- 仓库已有若干局部“临时文件 + rename”实现，但实现分散：部分使用固定 `.tmp` 名称，部分没有同步临时文件，无法直接作为三类配置的统一长期原语。
- 当前 Windows 工具链 Rust 1.94 标准库源码确认，`std::fs::rename` 使用 `MoveFileExW(..., MOVEFILE_REPLACE_EXISTING)`，并在拒绝访问时尝试 `FileRenameInfoEx` 的 replace 语义；因此同目录暂存后替换现有配置在当前主平台有明确实现依据。

### 已查明的执行流程

1. 全局 Provider 配置由 service-kernel 的 `ProviderConfigurationTransaction::persist` 唯一调用 `save_configuration_file`，目标为用户目录下 `.vulcan/vulcan-code.json`。
2. 项目级 VMM 与 Agent Skill 通过上一轮的逐文件事务唯一调用 `save_project_configuration_file`，目标为工作区 `.vulcan/code/config.json`。
3. AI 能力路由使用独立的 `save_ai_tool_configuration_file`，目标同样由 provider-config 解析；它不是全局 Provider JSON 的同一文件，但具有相同的完整 JSON 覆盖风险。
4. 三个保存函数都在文件系统写入前完成 serde 序列化，因此可以安全地把“完整下一版本字节”交给统一原子发布层。
5. 配置事务只在底层保存返回成功后提交内存运行时与 UI 投影；原子 helper 的错误必须继续向上传播，不能把暂存或同步失败当作成功。

### 长期优化调整

- 在 provider-config 内新增私有 `atomic_file` 模块，为三类配置提供唯一的 `write_file_atomically` 原语，不新增第三方依赖，也不扩大公共 API。
- 原子写入先在目标同目录以 `create_new` 独占创建暂存文件；名称组合原始 `OsString` 文件名、进程 id 与单调序号，既保留非 Unicode 名称，又通过文件系统原子创建解决跨线程、跨进程碰撞。
- 完整写入暂存字节后调用文件 `sync_all`，只有同步成功才执行同文件系统 rename；写入、同步或 rename 失败都会尽力清理当前调用独占的暂存文件，旧目标在发布前保持不变。
- Unix 路径在 rename 后额外打开并同步父目录，覆盖目录项持久性；Windows 使用标准库已核实的 replace-existing 实现，不虚构不可用的可移植目录同步 API。
- 三个保存函数删除重复的父目录创建与直接覆盖逻辑，统一在 JSON 序列化成功后把完整字节交给原子 helper，并保留各业务域原有错误上下文。
- helper 对已有普通目标复制原权限，避免临时文件替换把限制性配置权限恢复为默认创建权限；对符号链接目标明确拒绝，避免从旧的“跟随链接写入”静默变成“替换链接节点”。
- 新增原子替换成功测试与 rename 失败清理测试；Unix 条件下另有 `0600` 权限保持测试。

### 文件变更

- 新增：`crates/vulcan-provider-config/src/atomic_file.rs`
- 修改：`crates/vulcan-provider-config/src/lib.rs`
- 修改：`crates/vulcan-provider-config/src/config/configuration_io.rs`
- 修改：`crates/vulcan-provider-config/src/ai_tool_configuration.rs`
- 修改：`TASK_LOG.md`
- 删除：无

### 修改范围代码审核与自动修复

- 首轮严格 Clippy 发现 AI 能力配置迁移后遗留未使用的 `Path` import；已删除并复跑至零告警。
- 审核发现首版原子替换会让新临时文件权限取代旧目标权限，Unix 上可能把原有 `0600` 凭据配置恢复为默认权限。已自动修复为读取现有目标元数据、拒绝符号链接，并在同步前把权限应用到暂存文件。
- 审核确认暂存文件始终与目标同目录，不会触发跨文件系统 rename；`create_new` 而非“先 exists 再 create”是唯一碰撞判定，未引入 TOCTOU 窗口。
- 审核确认暂存文件在 rename 前已经 `sync_all` 且句柄已关闭，Windows 不会因当前进程仍持有写句柄而导致替换失败。
- 审核确认 serialization、暂存、同步或 rename 任一失败都会向上传播，service-kernel 不会提交对应内存配置和投影；成功路径继续保持既有事务顺序。
- provider-config 生产源码搜索已无直接 `fs::write`，剩余 3 处仅用于原子 helper 的文件系统测试夹具。
- CodeKit 最终复查原子模块、配置 I/O 与 AI 能力配置 3 个文件，共解析 37 个结构项，错误为 0。

### 验证结果

- `cargo clippy -p vulcan-provider-config --all-targets --no-deps -- -D warnings`：通过，零告警。
- `cargo test -p vulcan-provider-config`：通过，2 个测试套件共 76 项全部通过；Windows 主平台执行了原子替换与失败清理测试。
- `cargo test -p vulcan-service-kernel`：通过，2 个测试套件共 350 项全部通过，证明上层全局与项目事务调用仍可正常保存并提交。
- Linux 目标完整 `cargo check` 尝试在依赖 `ring` 构建阶段被环境缺少 `x86_64-linux-gnu-gcc` 阻断，诊断不指向本轮源码；随后使用已安装的 `x86_64-unknown-linux-gnu` Rust 标准库分别编译原子模块与其 `cfg(test)` 元数据，均通过，覆盖 Unix 目录同步和权限代码的编译正确性。
- 4 个 Rust 文件 `rustfmt --check`：通过。
- 本轮文件范围 `git diff --check`：通过，未发现空白错误。

### 遗留问题与下一轮方向

- 原子写入防止单文件半写，但不会解决多个应用进程同时执行 read-modify-save 时的语义丢更新；当前事务锁是进程内锁。若产品明确支持同一用户目录下多实例并发写配置，需要独立设计跨进程版本或文件锁协议，不能把原子 rename 误当事务锁。
- 进程在 rename 前崩溃可能留下带 pid/sequence 的暂存文件；当前实现不会删除其他进程可能仍在使用的暂存文件。若实际观察到堆积，应基于明确的所有权与存活判定设计启动清理，不能按文件名模糊删除。
- `vulcan-agent-runtime/src/agent_profile.rs` 仍有 Agent 状态 JSON 与用户画像 Markdown 的直接覆盖写入；两者是否共享事务、保存失败如何反馈、是否含敏感字段尚未完成调用链确认，可作为下一轮重点。
- 仓库其他固定 `.tmp` 实现仍可能存在并发暂存冲突；需要逐个区分“可重建缓存”和“权威持久状态”，不能机械地把所有 `fs::write` 一次性替换。
- 本轮只修改 Rust 持久化层，没有前端产物变化；第 23 轮前端生产构建结果仍有效。

## 第 27 轮：删除不可达 Agent 写链并确定化用户配置加载（2026-07-10）

### 问题与事实依据

- 对 `save_agent_profile_state`、`save_user_agent_profile`、`delete_user_agent_profile` 与 `migrate_legacy_provider_agents` 进行全仓搜索后确认：除同文件内部互调外没有任何调用方，service-kernel、WebUI、桌面壳与其他 crate 均未暴露 Agent profile 写入口。
- 实际产品链路只有读取：service bootstrap 调用 `load_agent_profile_catalog`，依次读取用户目录的 `agent-config.json` 与 `agents/**/*.md`；任何读取或解析错误都会包装为 Workspace bootstrap 错误并阻止服务完成启动。
- 加载完成的 `AgentProfileCatalog` 被保存在 `AgentRuntimeBridge`，运行时构建时把有效用户 profile 加入内置 Agent 集合，并用 selection state 选择 active agent；流式执行、压缩和提交摘要都只克隆这份目录。
- 原“旧 Provider Agent 迁移”函数从未被 bootstrap 或其他入口调用；`ProviderConfigurationFile.legacy_active_agent`、`legacy_agents` 及 `PersistedAgentEntry/Kind/Route` 的唯一消费者就是这个不可达函数。
- `collect_markdown_files` 依赖未排序的 `read_dir` 顺序；两个文件归一化为同一 id 时，`BTreeMap::insert` 会让后遍历文件静默覆盖先文件，最终生效配置随文件系统枚举顺序变化。
- 用户文件使用 `plan` 等保留 id 时原实现直接 `continue`，文件会静默消失；用户无法区分“文件未扫描”与“标识冲突”。
- 递归扫描使用 `Path::is_dir()`，会跟随目录符号链接；循环链接可能无限递归，指向根外的链接还会扩大未经声明的配置来源。

### 已查明的执行流程

1. `run_service_bootstrap` 在 MCP 初始化后同步调用 `load_agent_profile_catalog`；失败被标记为 Workspace 阶段错误，Agent runtime 不会创建。
2. `load_agent_profile_state` 只在 `agent-config.json` 不存在时返回默认选择，读取和 JSON 解析错误都会显式传播。
3. `load_user_agent_profiles` 递归发现 Markdown，逐文件解析 YAML frontmatter 和正文，并把 frontmatter id 或文件名 slug 作为稳定 id。
4. `AgentRuntimeConfig::from_sources` 过滤内置保留 id、合并用户 Agent，并仅在 selection id 同时存在于最终 Agent 集合时使用它，否则回退 default worker。
5. 仓库没有运行时修改 `agent-config.json` 或用户 Markdown 的调用链；用户 profile 当前事实语义是外部编写、启动时只读，而不是应用内 CRUD。

### 长期优化调整

- 删除不可达的 Agent selection 保存、用户 profile 保存/重命名、删除、旧 Provider 迁移和私有 Markdown 序列化函数，不再为不存在的产品入口维护有数据破坏风险的直接覆盖写实现。
- 删除只服务于不可达迁移的 4 个旧内建 id 常量、legacy 转换 helper、路径 helper，以及 Agent runtime 对旧 Provider 配置类型的依赖。
- 按“不考虑历史兼容”的任务约束，从 `ProviderConfigurationFile` 删除只读不写且无人消费的 `active_agent/agents` legacy 字段，并删除 `PersistedAgentEntry`、`PersistedAgentKind`、`PersistedAgentRoute` 三类死类型；当前配置保存本就通过 `skip_serializing` 丢弃这些字段，因此不改变现代配置输出。
- 抽取 `load_user_agent_profiles_from_directory`，使真实用户目录和隔离测试走同一加载逻辑。
- 在解析前按完整 `PathBuf` 排序 Markdown 文件，使跨平台加载和错误选择稳定；额外保留 id 到首个来源路径的映射，重复规范 id 直接返回同时包含两个精确路径的错误。
- 保留 id 不再静默跳过，而是明确报告冲突文件与标识，防止用户配置无声失效。
- 根路径通过 `symlink_metadata` 区分缺失、普通目录、非目录与符号链接；递归条目通过不跟随链接的 `DirEntry::file_type` 判定，并明确拒绝链接，消除越界扫描和循环递归。
- 新增重复 id 确定性错误、保留 id 错误、非目录根错误三项测试。

### 文件变更

- 修改：`crates/vulcan-agent-runtime/src/agent_profile.rs`
- 修改：`crates/vulcan-provider-config/src/config/configuration_io.rs`
- 修改：`crates/vulcan-provider-config/src/config/runtime_persistence.rs`
- 修改：`TASK_LOG.md`
- 新增：无
- 删除：无

### 修改范围代码审核与自动修复

- 首轮修改后 `cargo check -p vulcan-agent-runtime` 通过，确认删除的 public 函数和 Provider legacy 类型在工作区内确实没有隐藏 Rust 调用方。
- 审核确认现代 Agent profile 的读取模型仍完整保留：selection state、Markdown frontmatter、正文、模型、温度、轮次、skills、native/MCP tools、tool policy 与 runtime policy 均未改字段语义。
- 审核发现首版确定性扫描仍会通过 `Path::is_dir` 跟随符号链接；已自动修复为根与子项均读取不跟随链接的元数据，并补非目录根回归测试。链接现在显式失败，不再静默越界或递归。
- 审核重复 id 逻辑确认冲突发生在任何 `BTreeMap::insert` 覆盖前，失败目录不会产生部分运行时目录；排序保证重复执行返回相同错误文本。
- 审核 reserved id 逻辑确认文件名派生 id 与 frontmatter 显式 id 使用同一校验，不存在通过省略 frontmatter 绕过的第二路径。
- 全仓最终搜索确认被删除的 CRUD、迁移、legacy 字段和 legacy 类型名称均为 0 个残留引用。
- CodeKit 最终复查 Agent profile、Provider 配置 I/O 与 runtime persistence 3 个文件，共解析 118 个结构项，错误为 0。

### 验证结果

- `cargo check -p vulcan-agent-runtime`：通过。
- 定向 `cargo test -p vulcan-agent-runtime agent_profile`：最终 6 项全部通过，包含 3 项新增目录校验测试。
- 最终 `cargo test -p vulcan-agent-runtime`：通过，2 个测试套件共 580 项全部通过。
- `cargo test -p vulcan-provider-config`：通过，2 个测试套件共 76 项全部通过。
- `cargo test -p vulcan-service-kernel`：通过，2 个测试套件共 350 项全部通过。
- 严格 `cargo clippy -p vulcan-agent-runtime -p vulcan-provider-config --all-targets --no-deps -- -D warnings` 已执行；5 个错误全部位于本轮未修改的 task-runtime 文件，修改范围没有新增 Clippy 诊断。
- 3 个修改文件 `rustfmt --check`：通过。
- 本轮文件范围 `git diff --check`：通过，未发现空白错误。

### 遗留问题与下一轮方向

- 当前 Agent profile 是启动时快照，运行期间外部修改 Markdown 不会自动刷新；若产品要支持热更新，应接入明确的 watcher、目录版本与 runtime foundation refresh，而不是恢复孤立的写函数。
- 任意一个格式错误的用户 Agent 文件仍会阻止整个服务启动；这是当前 bootstrap 的严格配置语义。本轮只把重复、保留标识和链接错误显式化，未擅自改成“跳过坏文件继续启动”。若后续要容错，需要设计可见诊断集合，不能静默忽略。
- `agent-config.json` 仍允许手工选择用户 Agent，但应用内没有选择保存入口；需要下一轮确认这是否是刻意的文件驱动配置，还是 UI 已移除后遗留的无效 selection state。
- `attachment_asset_index/storage.rs` 使用固定 `assets.json.tmp`，多个写入者可能共享暂存文件；应先追踪其上层锁和写入频率，再判断是增加事务还是复用独占临时文件协议。
- 本轮只修改 Rust 后端加载与旧 schema，没有前端产物变化；第 23 轮前端生产构建结果仍有效。

## 第 28 轮：移除孤立 Agent selection state 并明确默认运行时回退（2026-07-10）

### 问题与事实依据

- 全仓源码、应用、脚本与常规文档搜索确认，`agent-config.json`、`selectedAgentId`、`AgentProfileState` 和 `__system_default__` 只有 agent-profile 模块、运行时投影与单元测试引用；没有 UI、RPC、命令、自动化或写入服务。
- 唯一给 `selected_agent_id` 赋值的代码是 `runtime_config_preserves_selected_plan_agent` 单元测试直接修改内存对象，不是产品入口。
- `AgentRuntimeConfig::from_sources` 仅把 selection state 用作请求未指定 Agent 时的 foundation 默认项；真实执行准备仍优先解析请求级 Agent 定义，子代理和会话执行选择链不依赖该状态文件。
- `docs/completed/20260510/01-AGENT_ARCHITECTURE_RESTRUCTURE.md` 提到 `~/.vulcan/code/agent-config.json`，但文档开头和执行总结明确说明其任务仅为策划、不进入代码重构，且正式实现前仍有待确认决策；它不是已落地 UI 或持久化契约。
- 现状属于半实现状态：有磁盘 reader、数据结构和 fallback 投影，没有任何 writer、管理面或用户文档。损坏这个无入口文件反而会让 Workspace bootstrap 整体失败。

### 已查明的执行流程

1. bootstrap 通过 `load_agent_profile_catalog` 读取 selection state 与用户 Markdown；state 文件不存在时返回默认空状态，解析失败则中止启动。
2. `AgentRuntimeConfig::from_sources` 始终构建 default worker、plan、explore、Lua、memory、coordinator 与用户 Agent 集合。
3. 旧逻辑从 selection state 读取 id，经过保留 id 和实际集合两层过滤后写入 `AgentRuntimeConfig.active_agent_id`，无效时回退 `DEFAULT_WORKER_AGENT_ID`。
4. 执行准备优先按请求显式 Agent id 选择定义；只有请求未指定时才调用 `RuntimeFoundation::active_agent_definition` 使用 foundation fallback。
5. session runtime state 的 `active_agent_id` 是执行准备后的结果投影，与全局 `agent-config.json` reader 不是同一来源。

### 长期优化调整

- 删除 `AgentProfileState`、`AGENT_PROFILE_STATE_FILE_NAME`、`agent_profile_state_file_path` 与 `load_agent_profile_state`，bootstrap 不再因一个没有产品入口的孤立 JSON 文件失败。
- `AgentProfileCatalog` 收敛为纯 Markdown profile 目录，只保留 `agents` 权威映射；加载函数不再拼装无写入来源的 selection state。
- 删除仅服务于不存在 picker 的 `SYSTEM_DEFAULT_AGENT_CHOICE_ID`；保留 id 校验只覆盖真实内建运行时 Agent。
- 将 `AgentRuntimeConfig.active_agent_id` 的无请求默认值明确固定为 `DEFAULT_WORKER_AGENT_ID`，删除两层 selection filter；请求级 Agent 选择与用户 Agent 定义仍保持原逻辑。
- 把原“持久化 plan 选择”测试替换为“无请求覆盖时默认 worker”测试，锁定真实产品语义。
- `UserAgentFrontmatter` 改为只派生 `Deserialize`，删除只对已移除 Markdown writer 有意义的 `skip_serializing_if` 与空 runtime policy 序列化 helper。
- 更新 agent-profile 模块、目录和 catalog 文档，去除“状态文件/picker”误导表述。

### 文件变更

- 修改：`crates/vulcan-agent-runtime/src/agent_profile.rs`
- 修改：`crates/vulcan-agent-runtime/src/agent_runtime/config.rs`
- 修改：`TASK_LOG.md`
- 新增：无
- 删除：无

### 修改范围代码审核与自动修复

- 全域审查最初发现历史策划文档仍提及 `agent-config.json`；已读取目标章节、计划状态和执行总结，确认该文档明确标注“未进入代码重构”“不代表运行时代码已完成”，没有把历史方案当作不存在的事实证据。
- 首轮格式化误用 Rust 2021 edition，而 `config.rs` 已使用 Rust 2024 let-chain；已按根 `Cargo.toml` 的 `edition = "2024"` 重跑，未改写现有语法。
- 首轮测试发现新默认回退断言漏导入 `DEFAULT_WORKER_AGENT_ID`；已补明确 import，并复跑定向与全量测试。
- 审核确认 `active_agent_id` 字段本身仍被 foundation summary、默认执行准备与 Agent registry 使用，因此只移除无来源的 selection 计算，没有误删运行时 active Agent 概念。
- 审核确认请求级 `agent_id` 解析仍优先于 `active_agent_definition`；删除全局 state 不影响显式 plan、explore、用户 profile 或子代理选择。
- 最终跨源码、应用与文档关键词搜索只剩历史策划文档 1 处，代码中的 selection state 标识全部为 0。
- CodeKit 最终复查 Agent profile 与 runtime config 2 个文件，共解析 65 个结构项，错误为 0。

### 验证结果

- `cargo test -p vulcan-agent-runtime agent_profile`：6 项全部通过。
- `cargo test -p vulcan-agent-runtime runtime_config_`：5 项全部通过，包含新的 default worker 回退测试。
- 最终 `cargo test -p vulcan-agent-runtime`：通过，2 个测试套件共 580 项全部通过。
- 最终 `cargo test -p vulcan-service-kernel`：通过，2 个测试套件共 350 项全部通过。
- 严格 `cargo clippy -p vulcan-agent-runtime --all-targets --no-deps -- -D warnings` 已复跑；5 个错误仍全部位于本轮未修改的 task-runtime 文件，两个本轮文件没有新增诊断。
- 2 个修改文件 `rustfmt --edition 2024 --check`：通过。
- 本轮文件范围 `git diff --check`：通过，未发现空白错误。

### 遗留问题与下一轮方向

- 历史 Agent 架构策划仍描述未落地的 selection state 和 picker 目标，但其正文已明确标注仅为方案。若未来重新实现主 Agent 管理，应从明确 UI/RPC/事务设计开始，而不是恢复孤立 reader。
- `AgentRuntimeConfig.active_agent_id` 目前固定为 default worker，但请求级显式选择仍可覆盖；后续若产品引入全局默认 Agent，应把它放进有真实设置入口和统一配置事务的现代配置域。
- Agent Markdown 仍是启动时只读快照，热更新与坏文件诊断集合问题沿用上一轮结论。
- `attachment_asset_index/storage.rs` 的固定 `assets.json.tmp` 仍是下一个明确正确性候选：必须确认其所有写调用、上层锁与多会话并发后再调整。
- 本轮只修改 Rust 后端 Agent 配置模型，没有前端产物变化；第 23 轮前端生产构建结果仍有效。

## 第 29 轮：事务化附件资产索引并修复并发丢记录与孤儿文件（2026-07-10）

### 问题与事实依据

- 附件索引的 `record_staged_attachment`、引用提升、租约增减、未引用删除和项目清理全部各自执行 load-modify-save，没有共享锁。
- 同一项目可同时接收多个浏览器内存上传、路径暂存和媒体生成结果；service staging 的异步 RPC 只短暂读取 service state 来解析工作区，随后在锁外同步暂存，因此不同请求可以并发修改同一个 `assets.json`。
- 项目附件清理是独立异步 RPC，同样在 service state 锁外调用 `execute_attachment_cleanup`，可以与上传、媒体生成和其他清理同时执行。
- 生产写入口中，`stage_attachment_bytes` 负责新建托管文件并调用 `record_staged_attachment`；`execute_attachment_cleanup` 负责先写 purge 标记、再删除文件、必要时二次保存失败原因。二者确实会竞争同一索引，而非仅测试代码存在并发可能。
- acquire/release/promote/discard 当前主要由模块测试覆盖，工作区暂未发现外部生产调用，但它们是公开生命周期 API，必须与 record/cleanup 使用同一事务域，不能留下未来旁路。
- 原保存函数使用固定 `index/assets.json.tmp`：并发保存会共同截断和重命名同一个临时文件；即使没有 load-modify-save 丢更新，也可能出现内容串写、临时文件被另一请求提前 rename 或删除。
- 固定临时文件通过 `fs::write` 后立即 rename，没有 `sync_all`；进程或系统故障仍可能发布未完整落盘的索引。
- `stage_attachment_bytes` 在写出托管文件后才设置时间并提交索引；时间设置或索引保存失败时原实现直接返回错误，已经写出的文件成为永远无法由索引管理的孤儿资产。

### 已查明的执行流程

1. 浏览器 bytes 或本地路径进入 service staging，解析项目工作区后调用 agent-runtime `stage_attachment_bytes`。
2. staging 生成 UUID asset id，以时间前缀和净化文件名写入工作区 `.vulcan/code/attachments/<kind>/`，设置文件时间，构造 `ConversationAttachment`，最后更新索引。
3. record 从规范 managed attachment root 解析 `index/assets.json`，加载完整文档，按 asset id 更新或追加记录，再整文件保存。
4. cleanup 从相同索引读取候选，先把匹配记录标记 purged 并保存，随后删除托管文件和 preview sidecar；删除失败时更新原因并再次保存。
5. replay/preview 是只读快照路径；原子 rename 可保证它们读取旧版或新版完整文档，因此不需要与写事务强制串行。
6. managed attachment root 已通过工作区规范路径解析，最终 `index/assets.json` 是逐索引锁身份的唯一事实来源。

### 长期优化调整

- 新增按精确 `assets.json` 路径索引的事务锁注册表；注册表保存 `Weak<Mutex<()>>`，同一索引共享锁，不同工作区可并发，空闲锁不会永久积累。
- record、promote、discard、update refs 和 destructive cleanup 全部在加载前取得同一逐索引锁，并保持到校验、文件副作用与最终索引发布完成。
- cleanup 的事务锁覆盖候选选择、purge 标记首次保存、托管文件删除和失败原因二次保存，防止清理在引用检查后与新引用/新暂存交错。
- 固定 `assets.json.tmp` 改为同目录 UUID 暂存文件，并用 `OpenOptions::create_new` 作为跨进程碰撞的权威检查；不再共享临时路径。
- 保存前先生成完整 JSON bytes；暂存文件完整写入、继承既有目标权限并 `sync_all` 后才 rename，失败时清理当前调用独占暂存文件。
- Unix 在 rename 后同步索引目录项；符号链接索引被明确拒绝，避免替换链接节点。
- `stage_attachment_bytes` 增加失败回滚：文件时间设置或索引提交失败时删除刚写出的托管文件；若回滚自身失败，返回错误会同时包含首要失败与孤儿文件路径。
- 新增 24 线程同工作区并发暂存测试，比较所有返回 asset id 与最终持久索引 id 集合；新增索引目录被普通文件阻塞的失败测试，确认错误后分类目录文件数为 0。

### 文件变更

- 修改：`crates/vulcan-agent-runtime/src/attachment_asset_index.rs`
- 修改：`crates/vulcan-agent-runtime/src/attachment_asset_index/storage.rs`
- 修改：`crates/vulcan-agent-runtime/src/attachment_asset_index/lifecycle.rs`
- 修改：`crates/vulcan-agent-runtime/src/attachment_asset_index/cleanup.rs`
- 修改：`crates/vulcan-agent-runtime/src/attachment_asset_index/tests.rs`
- 修改：`crates/vulcan-agent-runtime/src/attachment_assets.rs`
- 修改：`TASK_LOG.md`
- 新增：无
- 删除：无

### 修改范围代码审核与自动修复

- 审核锁身份确认使用最终索引文件路径而非 session id、project id 或附件 id；所有修改同一整文件文档的业务族进入同一个锁。
- 审核注册表确认 mutex 只保护弱引用映射，不跨文件 I/O 持有；逐索引 mutex 才覆盖事务，因此无关项目清理不会阻塞其他项目上传。
- 审核生命周期确认 discard 在“无引用检查—文件删除—索引保存”全程持锁，update refs 在“记录存在校验—租约变更—可能删除—保存”全程持锁，关闭检查后竞态。
- 审核发现首版事务改造仍保留 staging 写文件后失败不回滚的旧断裂；已自动增加 `rollback_failed_managed_attachment`，并补真实索引失败夹具验证。
- 审核原子保存确认使用 `create_new` 而非先 exists 再 create；暂存句柄在 Windows rename 前显式关闭，目标权限在同步前应用。
- 最终源码搜索确认 `assets.json.tmp` 与固定 `temporary_file` 保存模式在附件索引模块中均为 0 个残留。
- CodeKit 最终复查附件索引入口、storage、lifecycle、cleanup、测试与 staging 6 个文件，共解析 85 个结构项，错误为 0。

### 验证结果

- 定向 `cargo test -p vulcan-agent-runtime attachment_asset_index`：13 项全部通过。
- 定向 `cargo test -p vulcan-agent-runtime attachment_asset`：23 项全部通过，包含并发保全和失败回滚测试。
- 最终 `cargo test -p vulcan-agent-runtime`：通过，2 个测试套件共 582 项全部通过。
- 最终 `cargo test -p vulcan-service-kernel`：通过，2 个测试套件共 350 项全部通过。
- 严格 `cargo clippy -p vulcan-agent-runtime --all-targets --no-deps -- -D warnings` 已复跑；仍只有本轮未修改的 task-runtime 5 个既有错误，本轮附件文件没有新增诊断。
- 6 个修改文件 `rustfmt --edition 2024 --check`：通过。
- 本轮文件范围 `git diff --check`：通过，未发现空白错误。

### 遗留问题与下一轮方向

- 逐索引锁是进程内事务；两个应用进程同时修改同一工作区索引时，唯一临时文件可防止内容串写，但 read-modify-save 仍可能最后写入者覆盖。多实例支持需要明确跨进程锁或版本协议。
- 进程在 rename 前崩溃可能留下 UUID 暂存文件；不能在无所有权与存活证据时模糊清理其他进程的文件。
- discard 需要同时删除托管文件并移除索引记录，两种持久介质无法由普通文件系统 rename 组成单一原子事务；当前锁解决并发但不解决两步之间进程崩溃。后续可沿 cleanup 已采用的 tombstone 模型统一设计可恢复删除。
- Unix 目录同步失败发生在 rename 已发布之后，错误路径无法回滚目录项；若要向调用方精确区分“未发布失败”与“已发布但持久性不确定”，需要为内部保存结果建立阶段化错误类型。
- 本轮只修改 Rust 后端附件持久化，没有前端产物变化；第 23 轮前端生产构建结果仍有效。

## 第 30 轮：清零 task-runtime 严格 Clippy 债务并保持执行语义（2026-07-10）

### 问题与事实依据

- 第 24 至 29 轮多次执行 `cargo clippy -p vulcan-agent-runtime --all-targets --no-deps -- -D warnings`，始终被同一组 5 个错误阻断；这些错误已经成为后续修改范围无法获得严格静态审查绿灯的持续工程债务。
- `build_task_subagent_prompt_context` 使用无副作用闭包为 `Option<&str>` 提供 description 回退，Clippy 明确判定无需惰性求值。
- `child_forbidden_tool_prompt_list` 把静态 `&[&str]` 执行 `iter().copied().collect::<Vec<_>>()` 后才 join，产生无意义中间 Vec；同一 slice 已直接供 child tool policy 使用。
- preflight compaction 先判断请求上限启用且已达到，再嵌套判断异步压缩是否返回 snapshot；两层条件属于同一单次决策，没有 else 或中间副作用。
- `status_events.rs` 和 `tool_surface.rs` 都在生产函数前插入了 `#[cfg(test)] mod tests`，导致 production item 位于 test module 之后；Clippy 的 `items_after_test_module` 对两个文件都报错。

### 已查明的执行流程

1. 子代理 prompt identity 优先取 `invocation.subagent`，缺失时取 description，再裁剪空白并回退 `subagent`；回退表达式本身不执行 I/O 或状态变更。
2. child forbidden tool ids 来源是 `vulcan_session_core::child_agent_forbidden_tool_ids()` 静态 slice，既用于请求 hide/reject，也用于隐藏 delegation prompt 文本。
3. preflight compaction 只在 auto compaction 启用、provider request limit 为正且自最近压缩以来请求数达到阈值时调用；压缩返回 `Some` 才替换 invocation snapshot，返回 `None` 保持原快照。
4. `task_result_summary` 被持久化方法用于生成最多 240 字符的 metadata 摘要，与其下方测试模块无运行时依赖。
5. wait_agent result-payload 判定和 user-steer result builder 被 tool surface 与 invocation methods 调用；status-update 测试只验证 actionability，不参与生产注册。

### 长期优化调整

- identity 回退从 `.or_else(|| invocation.description.as_deref())` 改为 `.or(invocation.description.as_deref())`，保持优先级与裁剪语义不变。
- child forbidden tool prompt 直接在静态 slice 上 `.join(", ")`，删除中间 Vec 分配。
- 使用 Rust 2024 let-chain 合并“请求上限达到”与“压缩返回 Some”两层判断；异步压缩仍只在前置条件成立时执行，snapshot 更新条件不变。
- 将 `status_events` 测试模块移到 `task_result_summary` 之后，使全部生产 item 位于 test module 前。
- 将 `tool_surface` 测试模块移到 result-payload helper 与 user-steer builder 之后，使测试模块成为文件末项。

### 文件变更

- 修改：`crates/vulcan-agent-runtime/src/agent_runtime/task_runtime/runtime_launch.rs`
- 修改：`crates/vulcan-agent-runtime/src/agent_runtime/task_runtime/status_events.rs`
- 修改：`crates/vulcan-agent-runtime/src/agent_runtime/task_runtime/tool_surface.rs`
- 修改：`TASK_LOG.md`
- 新增：无
- 删除：无

### 修改范围代码审核与自动修复

- 审核 `Option::or` 确认 description 只是现有借用值，不含函数调用或副作用；从惰性到立即求值不会改变结果或成本边界。
- 审核 forbidden ids 返回类型确认是可直接 join 的字符串 slice，输出顺序、分隔符和内容不变。
- 审核 Rust 2024 let-chain 短路顺序确认：请求数读取只在 limit 启用后发生，异步 compaction 只在 reached 为 true 后发生；与原嵌套 if 一致。
- 审核测试模块移动确认没有改变可见性、cfg 条件、测试名称或生产函数顺序之间的调用依赖。
- CodeKit 最终复查 runtime launch、status events 与 tool surface 3 个文件，共解析 92 个结构项，错误为 0。

### 验证结果

- `cargo clippy -p vulcan-agent-runtime --all-targets --no-deps -- -D warnings`：通过，零告警；连续多轮存在的 5 个严格错误已全部清零。
- `cargo test -p vulcan-agent-runtime`：通过，2 个测试套件共 582 项全部通过。
- `cargo test -p vulcan-service-kernel`：通过，2 个测试套件共 350 项全部通过。
- 3 个修改文件 `rustfmt --edition 2024 --check`：通过。
- 本轮文件范围 `git diff --check`：通过，未发现空白错误。

### 遗留问题与下一轮方向

- 本轮只修复已经由执行流证明为语义等价的 Clippy 债务，没有借 lint 之名改写 task request limit 或 mailbox 协议。
- 附件 discard 的“先删文件、后删索引记录”仍缺少可恢复 tombstone；可作为下一轮正确性候选。
- 全工作区其他 crate 尚未统一执行 `-D warnings`，后续可按真实模块逐个清理，避免一次性机械改动过大。
- 本轮只修改 Rust 后端 task-runtime，没有前端产物变化；第 23 轮前端生产构建结果仍有效。

## 第 31 轮：为附件 discard 建立可恢复两阶段删除状态机（2026-07-10）

### 问题与事实依据

- 单条 `discard_unreferenced_staged_attachment` 和 release refs 的批量删除都在确认无引用后先删除托管文件，最后才从 `assets.json` 移除记录并保存。
- 第 29 轮逐索引锁关闭了并发检查窗口，但无法覆盖两个不同持久介质之间的进程崩溃：文件删除成功后、索引保存前中断，会留下指向缺失文件的普通未 purge 记录。
- replay 状态优先检查 `purged_at_unix_millis`；已有 cleanup 正是先保存 purge tombstone、再删文件，因此即使删除失败，回放也不会错误回退到已经明确清理的原始来源。
- 现有 `purge_reason` 无法可靠区分“历史清理审计记录”和“删除尚未完成”；按字符串前缀猜状态会违反唯一事实来源原则。
- cleanup 会永久保留历史 purged 记录用于回放审计，而 discard 希望最终移除从未进入消息历史的暂存记录；两类 tombstone 生命周期不同，必须有显式状态字段。

### 已查明的执行流程

1. direct discard 在逐索引锁内加载文档，定位 asset id，拒绝仍有 session/message/staging refs 的记录，然后删除 managed file/preview，移除记录并保存。
2. release refs 先批量移除指定 staging scope，再收集完全无引用的 record indices，逆序调用同一个文件删除 helper，最后统一保存文档。
3. cleanup 先给历史候选写入 purged 时间与原因并保存，再逐文件删除；记录保留，即使文件不存在或删除失败也保持 `Purged` 回放状态。
4. replay 只读路径通过完整原子索引快照解析 record，存在 purged timestamp 时直接返回 `Purged`，不会使用 managed file 或 source fallback。
5. 所有 mutation 已在第 29 轮进入逐索引事务锁，因此可以在每次 load 后安全恢复 pending deletion，再执行当前业务变更。

### 长期优化调整

- 为 `AttachmentAssetRecord` 新增 `deletion_pending: bool`，serde default 为 false 且 false 时不写出；已有索引无需迁移即可加载，状态含义不依赖 reason 文本。
- discard 改为两阶段持久状态机：先设置 pending、purged 时间和稳定原因并原子保存；再删除托管文件和 preview sidecar；最后移除记录并再次原子保存。
- 删除失败时保留 pending tombstone，更新 `delete-failed` 诊断并保存；首要删除错误仍向上传播，若诊断保存也失败则组合报告两项错误。
- 新增 `load_attachment_asset_index_document_for_mutation`：读取最新索引后先扫描全部 pending indices，并按逆序完成文件删除与记录移除；record、promote、discard、update refs 与 destructive cleanup 统一使用。
- read-only replay 与 cleanup preview 继续使用纯加载，不产生隐藏文件删除副作用；pending 记录已有 purged marker，读路径仍保持安全状态。
- release refs 若发生实际 discard，helper 的 phase-one/phase-two 保存已经覆盖同一内存文档里的全部租约变更，最终不再执行冗余第三次保存；无 discard 的普通租约变更仍保存一次。
- record 更新同 asset id 时显式清除 `deletion_pending`，新 record 初始化为 false；历史 cleanup 标记 purge 时也显式保持 false，防止两类状态混淆。
- 新增中断恢复测试：构造“pending tombstone 已保存但文件仍存在”的精确崩溃状态，再执行另一条 acquire mutation，验证旧文件和 pending record 被恢复删除，同时新租约正常提交。

### 文件变更

- 修改：`crates/vulcan-agent-runtime/src/attachment_asset_index/types.rs`
- 修改：`crates/vulcan-agent-runtime/src/attachment_asset_index/lifecycle.rs`
- 修改：`crates/vulcan-agent-runtime/src/attachment_asset_index/cleanup.rs`
- 修改：`crates/vulcan-agent-runtime/src/attachment_asset_index/tests.rs`
- 修改：`TASK_LOG.md`
- 新增：无
- 删除：无

### 修改范围代码审核与自动修复

- 审核 schema 确认 `deletion_pending` 使用独立布尔事实字段，旧索引缺失字段时明确为 false，没有使用 purge reason 候选匹配。
- 审核发布顺序确认 phase-one tombstone 必须保存成功后才允许删除文件；因此 phase-one 保存失败不会产生不可恢复文件副作用。
- 审核 phase-two 保存失败确认磁盘上仍至少保留 pending tombstone；若 rename 已发布但目录同步报错，记录可能已经移除，两种可见状态都不会把缺失文件误判为可回放。
- 审核恢复路径确认所有生产 mutation 都使用 mutation loader；只读 replay/preview 不做恢复，避免查询触发隐藏破坏性动作。
- 审核批量逆序删除确认每次记录移除不会使尚未处理的低位 index 失效；每个完成步骤都独立持久化，不依赖循环末尾一次性提交。
- 审核 acquire/promote 与 pending 竞态确认恢复先于“记录存在”校验；已计划删除的资产会先完成删除，随后引用操作明确得到缺失记录错误，不能复活 tombstone。
- CodeKit 最终复查附件 record schema、lifecycle、cleanup 与测试 4 个文件，共解析 55 个结构项，错误为 0。

### 验证结果

- 定向 `cargo test -p vulcan-agent-runtime attachment_asset_index`：14 项全部通过，包含新增 pending deletion 恢复测试。
- `cargo clippy -p vulcan-agent-runtime --all-targets --no-deps -- -D warnings`：通过，零告警。
- 最终 `cargo test -p vulcan-agent-runtime`：通过，2 个测试套件共 583 项全部通过。
- 最终 `cargo test -p vulcan-service-kernel`：通过，2 个测试套件共 350 项全部通过。
- 4 个修改文件 `rustfmt --edition 2024 --check`：通过。
- 本轮文件范围 `git diff --check`：通过，未发现空白错误。

### 遗留问题与下一轮方向

- pending deletion 只在下一次 mutation 时恢复；纯只读进程会保留文件但回放状态已安全标记为 Purged。若需要启动即回收，应在明确的维护/bootstrap 阶段调用恢复，不能让普通查询隐式删除。
- 跨进程并发仍需要跨进程事务协议；两个进程各自恢复/修改同一索引时，进程内锁不足。
- cleanup 的历史 tombstone 会长期保留并增长；需要基于审计保留策略设计压缩或归档，不能与 pending deletion 机械混删。
- attachment index 已形成独立状态机，后续可检查其他“文件副作用 + JSON 状态”模块是否仍采用不可恢复顺序。
- 本轮只修改 Rust 后端附件删除状态机，没有前端产物变化；第 23 轮前端生产构建结果仍有效。

## 第 32 轮：事务化 TaskSession state.json 并替换固定 JSON 临时文件（2026-07-10）

### 问题与事实依据

- `vulcan-task-session` 的通用 `write_json_file` 固定使用 `path.with_extension("json.tmp")`，同一路径并发保存会共享、截断和 rename 同一个临时文件。
- 该 helper 写入的是 TaskSession 权威文档：根/任务 metadata、task_metadata 和 state.json，不是可随时重建的纯缓存。
- `state.json` 同时存放 `task_context`、runtime/current context 投影与 `runtime_checkpoint`；`save_context` 和 `save_runtime_checkpoint` 分别先加载整份 state、修改不同字段、再整文件保存。
- agent-runtime 的 task persistence 在一次状态提交中会依次保存 context、task metadata 和 runtime checkpoint；不同状态事件或异步执行仍可让两个 store clone 并发写同一 task state。
- 两条 state 保存原先没有上层锁，因此即使临时文件名唯一，两个请求也可能从同一旧 state 出发，后提交者抹掉另一字段族。
- append task segment、mirror 和 repair 路径已经使用 `vulcan_session_core::segmented_append_file_io_lock` 覆盖 metadata/JSONL 读改写；不能在不了解重入关系时机械地给所有 helper 再套同一锁。

### 已查明的执行流程

1. `save_context` 通过 task id 定位目录，读取 `state.json` 或创建默认文档，更新 runtime_state、current_context 与 task_context，再整文件保存。
2. `save_runtime_checkpoint` 定位同一目录，读取同一 state 文档，仅更新 runtime_checkpoint 与 updated_at，再整文件保存。
3. `load_context` 与 `load_runtime_checkpoint` 只读取各自字段；底层原子 rename 可让它们观察旧版或新版完整 JSON，无需读锁。
4. `write_json_file` 还被 create/save task session、append metadata repair 与 root mirror 使用；其中多数 append/mirror 路径已持有共享 append lock，但 state 路径需要自己的逐文档事务。
5. `SegmentedTaskSessionStore` 可 Clone，生产 task runtime 与测试均通过共享根目录构造多个 handle，不能依赖单对象独占规避并发。

### 长期优化调整

- 在 task-session segmented I/O 层新增按最终 JSON 路径划分的弱引用事务锁注册表；相同 state path 共享 mutex，不同任务文档可并发，空闲路径锁可回收。
- `save_context` 与 `save_runtime_checkpoint` 在任何读取前取得同一 state path 锁，并保持到合并后的完整文档原子发布完成，形成统一 read-modify-write 事务。
- `write_json_file` 在接触文件系统前生成完整 pretty JSON bytes；序列化失败不创建目录或临时文件。
- 固定 `json.tmp` 改为保留原始 `OsString` 文件名、进程 id 与单调序号的同级暂存文件，使用 `OpenOptions::create_new` 进行权威跨线程/跨进程碰撞检查。
- 暂存内容完整写入、继承已有目标权限并 `sync_all` 后才 rename；失败时清理当前尝试独占的暂存文件，符号链接目标被明确拒绝。
- Unix rename 后同步父目录项；Windows 沿用已核实的 Rust replace-existing 实现。
- 读 helper 继续无锁，原子发布保证不会读取半写 JSON；事务锁只覆盖真正需要合并旧状态的写路径。
- 新增两个 store clone、两个线程并发测试：context/checkpoint 生产者同时启动并各写 32 次，最终同时读取到 provider context 与最后 checkpoint sequence。

### 文件变更

- 修改：`crates/vulcan-task-session/src/segmented_store/io.rs`
- 修改：`crates/vulcan-task-session/src/segmented_store/traits.rs`
- 修改：`crates/vulcan-task-session/src/segmented_store/tests.rs`
- 修改：`TASK_LOG.md`
- 新增：无
- 删除：无

### 修改范围代码审核与自动修复

- 审核锁身份确认使用最终 `state.json` 路径，context 与 checkpoint 不会因业务方法不同取得不同锁。
- 审核锁范围确认 state load 在加锁后发生、atomic write 在释放前完成，关闭“锁只包 write、旧 clone 仍覆盖新值”的伪事务。
- 审核无重入确认 `write_json_file` 本身不获取事务锁；显式 state 事务可安全调用，已持 append lock 的 metadata 路径也不会发生双锁自死锁。
- 审核 staging name 确认不进行有损 UTF-8 转换，`create_new` 是唯一碰撞判定；进程 id/sequence 只减少重试概率。
- 审核测试确认两个生产者不仅都返回成功，最终 state 还同时保留不同字段族，直接覆盖丢更新语义。
- 最终源码搜索确认固定 `json.tmp` 为 0；剩余 `with_extension` 仅位于 JSONL repair 的时间戳临时文件路径，不属于本轮固定名称。
- CodeKit 最终复查 segmented I/O、traits 与测试 3 个文件，共解析 66 个结构项，错误为 0。

### 验证结果

- 新增 `concurrent_state_saves_preserve_context_and_checkpoint`：通过，两个并发生产者各 32 次写入后字段完整。
- `cargo clippy -p vulcan-task-session --all-targets --no-deps -- -D warnings`：通过，零告警。
- `cargo test -p vulcan-task-session`：通过，2 个测试套件共 41 项全部通过。
- `cargo test -p vulcan-agent-runtime`：通过，2 个测试套件共 583 项全部通过。
- `cargo test -p vulcan-service-kernel`：通过，2 个测试套件共 350 项全部通过。
- 3 个修改文件 `rustfmt --edition 2024 --check`：通过。
- 本轮文件范围 `git diff --check`：通过，未发现空白错误。

### 遗留问题与下一轮方向

- `save_task_session` 会读改写 task metadata 与 main metadata，但尚未显式取得共享 append lock；可能与 `append_task_segment_event` 竞争相同文档，下一轮应沿真实锁顺序确认并统一事务。
- `create_task_session` 的“find 不存在—创建目录—写多文件—追加父事件”不是单一事务，同 task id 并发创建和中途失败仍可能留下部分布局。
- JSONL repair 的 `current_unix_millis` 临时名在同毫秒并发修复时仍可能碰撞；多数调用已有全局 append lock，但跨进程仍需唯一 `create_new` 协议。
- 通用 atomic writer 在 Unix 目录同步失败后同样存在“rename 已发布但返回错误”的阶段区分问题，后续可统一阶段化内部错误模型。
- 本轮只修改 Rust 后端 TaskSession 持久化，没有前端产物变化；第 23 轮前端生产构建结果仍有效。

## 第 33 轮：统一 TaskSession 元数据保存与事件追加事务（2026-07-10）

### 问题与事实依据

- `save_task_session` 原先先读取并改写子任务 `task_metadata.json` 与子任务主 `metadata.json`，两次发布完成后才刷新根会话 `metadata.json` 中的任务镜像，全程没有覆盖子任务文档的共享事务锁。
- `append_task_segment_event` 已在 `segmented_append_file_io_lock` 内读取、修复和改写同一份子任务主 metadata，再同步写回 task metadata；两条路径会竞争 `segments`、`active_segment`、`next_global_seq`、`updated_at` 与业务状态投影。
- 通用 JSON 原子 rename 只能保证单个文件不会半写，不能阻止两个 read-modify-write 事务从同一旧版本出发后互相覆盖。
- 原根镜像 helper `refresh_main_session_task_mirror_from_metadata` 会自行取得同一把非重入 append lock；若只在 `save_task_session` 外层机械加锁，会在刷新根镜像时发生递归锁死。
- 全仓调用点核实后，生产 `save_task_session` 调用均从 agent-runtime task persistence 独立进入，没有调用方已持有 append lock 的隐藏前提；该根镜像 helper 也只有这一个保存调用点。

### 已查明的执行流程

1. agent-runtime 根据运行状态构造完整 `TaskSessionMetadata`，调用 store 的 `save_task_session`。
2. 保存路径定位固定 task 目录，读取 `task_metadata.json`，替换权威业务 metadata、状态、更新时间与 agent 投影。
3. 保存路径读取同目录主 `metadata.json`，更新标题与更新时间，并把当前 segment 索引同步到 task metadata。
4. 两份子任务 JSON 依次原子发布，然后根镜像路径读取根 `metadata.json`，以子任务投影构造 checkpoint 事件形态并更新 `tasks` 镜像，但不追加 JSONL 父事件。
5. 事件追加路径在共享 append lock 内修复 JSONL、重新计算序号、追加事件、改写主 metadata，再同步改写 task metadata；根父链接路径也使用同一共享锁更新根 JSONL 与根 metadata。
6. 因此三份 metadata 的正确一致性边界不是单文件 writer，而是既有的 append transaction；镜像刷新必须提供调用方已持锁的内部入口。

### 长期优化调整

- `save_task_session` 在定位和读取任何子任务文档前取得 `segmented_append_file_io_lock`，并保持到两份子任务 metadata 与根任务镜像全部发布完成。
- 根镜像刷新函数重命名为 `refresh_main_session_task_mirror_from_metadata_with_owned_append_lock`，删除内部重复加锁，名称与双语契约明确要求调用方持有共享 append lock。
- 保存事务现在与 task segment append、JSONL repair、父链接追加串行化，消除保存旧 segment 投影覆盖新追加元数据的窗口。
- 保留读取路径无锁与单文件原子发布；只序列化共享 read-modify-write 集合，不额外阻塞不同的 state.json 事务锁。
- 新增锁边界回归测试：测试线程先持有共享 append lock，工作线程到达保存调用后必须保持阻塞；释放后必须在限定时间内完成并持久化 Archived 状态，用同一测试同时验证“确实共锁”和“没有递归死锁”。

### 文件变更

- 修改：`crates/vulcan-task-session/src/segmented_store/traits.rs`
- 修改：`crates/vulcan-task-session/src/segmented_store/mirror.rs`
- 修改：`crates/vulcan-task-session/src/segmented_store/tests.rs`
- 修改：`TASK_LOG.md`
- 新增：无
- 删除：无

### 修改范围代码审核与自动修复

- 审核确认锁在 `indexed_task_dir` 与首次 metadata read 之前取得，避免只保护写入而继续允许旧快照覆盖。
- 审核确认锁跨越子任务主 metadata、task metadata 与根镜像三次发布，事件追加无法插入事务中间观察或覆盖混合版本。
- 审核确认根镜像内部不再重复取得非重入锁；回归测试在释放外部锁后成功完成保存，直接排除自死锁。
- 全仓搜索确认 renamed helper 只有 `save_task_session` 一个调用点，旧 helper 名称为 0；生产调用点没有外部持锁嵌套。
- 审核确认 state.json 仍走第 32 轮独立逐文档事务锁，不与全局 append lock 形成反向锁序；本轮没有引入双锁循环。
- CodeKit 最终复查 traits、mirror 与 tests 3 个文件，共解析 61 个结构项，错误为 0。

### 验证结果

- 新增 `save_task_session_waits_for_segment_transaction_lock`：通过，持锁期间保存阻塞，释放后完成且状态正确落盘。
- `cargo clippy -p vulcan-task-session --all-targets --no-deps -- -D warnings`：通过，零告警。
- `cargo test -p vulcan-task-session`：通过，2 个测试套件共 42 项全部通过。
- `cargo test -p vulcan-agent-runtime`：通过，2 个测试套件共 583 项全部通过。
- `cargo test -p vulcan-service-kernel`：通过，2 个测试套件共 350 项全部通过。
- 3 个修改文件 `rustfmt --edition 2024 --check`：通过。
- 本轮文件范围 `git diff --check`：通过，未发现空白错误。

### 遗留问题与下一轮方向

- `create_task_session` 的“检查不存在—创建目录—追加初始 JSONL—写三份 JSON—追加父事件”仍不是单一事务；同 task id 并发创建和中途失败会产生部分布局，下一轮应完整追踪创建失败的可恢复语义。
- `save_task_session` 的三次原子 rename 仍可能在中途 I/O 失败时留下跨文档混合版本；现有读取/repair 能修复 segment 投影，但业务 metadata 与根镜像需要明确权威源与恢复机制后才能继续收敛。
- JSONL repair 仍用毫秒时间戳派生临时文件名，进程内有全局锁但跨进程仍可能碰撞；后续应替换为独占 `create_new` 暂存协议。
- 本轮只修改 Rust 后端持久化锁边界，没有前端产物变化；第 23 轮前端生产构建结果仍有效。

## 第 34 轮：原子发布 TaskSession 创建布局并拒绝并发复用（2026-07-10）

### 问题与事实依据

- `create_task_session` 原先先用 `find_task_dir` 检查 `task_metadata.json` 是否存在，再对最终 `task/<task_id>` 目录执行 `create_dir_all`，随后依次追加初始 JSONL、写主 metadata、task metadata、state，整个子任务布局会逐文件暴露。
- `find_task_dir` 只承认包含有效 task metadata 的目录；空目录或只写入部分文件的异常最终目录会被视为“不存在”，下一次创建会直接复用并继续追加，可能产生重复初始行或混合布局。
- 同一 task id 的两个线程可同时通过旧“未找到”检查，共享最终目录和 `s001.jsonl`，然后各自写 metadata 并分别追加父级创建事件。
- 父级调用追加路径原先自行取得 append lock；若创建事务直接外层加锁，主会话父级会在 root helper 重复加锁，嵌套任务父级还会在 task segment helper 重复加锁。
- `load_all_task_documents` 会扫描 `task/` 下所有目录，不校验目录名是否等于 metadata 中 task id 的规范文件名；因此即使使用同级暂存目录，只要 task metadata 已写入，批量发现仍可能在 rename 前提前看到它。

### 已查明的执行流程

1. 创建入口创建 task 根目录，通过 task id 派生最终扁平目录，并按 task metadata 是否存在判断重复。
2. 初始 `subagent_task_meta` 直接追加到最终活动 segment，随后 segment 统计同步到主 metadata 与 task metadata，最后创建 state.json。
3. 三份 JSON 和初始 JSONL 完成后，创建入口构造稳定 `task-created-<task_id>` 事件并调用父级追加。
4. 主会话父级只更新根 JSONL 与根 metadata；任务父级先更新父任务 JSONL/两份 metadata，再更新根 JSONL 与根 metadata。
5. task segment、根链接和第 33 轮保存路径已经共享进程级 `segmented_append_file_io_lock`，创建必须加入同一事务并调用“已持锁”内部入口。
6. 单 ID 读取使用规范 `task/<task_id>` 直达路径，但批量读取原先不验证目录身份，所以暂存不可见性必须由发现规则明确保证。

### 长期优化调整

- `create_task_session` 在任何存在性检查前取得共享 append lock，并保持到完整子任务布局发布与父级创建关联追加结束。
- 重复检查从“是否找到有效 task metadata”改为对规范最终目录执行 `symlink_metadata`；任何文件、目录或符号链接占用最终身份都显式拒绝，不再复用异常残缺布局。
- 新增以原始 `OsString` task 目录名、进程 id 和单调序号构造的同级暂存目录，使用 `create_dir` 作为独占碰撞判定。
- 初始 JSONL、主 metadata、task metadata 与 state.json 全部先写入暂存目录；任一发布前写入失败会删除整个暂存目录，完成后以同文件系统目录 rename 一次发布。
- 根 metadata 读取与所有纯内存文档构造提前到暂存目录创建之前，避免前置读取失败遗留空暂存目录。
- task segment append、父级 invocation append 与根 task link 分别拆出 caller-owned append lock 内部入口；外部入口只负责一次加锁，创建事务直接调用 owned 入口，消除递归锁死。
- 嵌套任务父链接的直接父级更新与根索引更新现在处于同一锁持有期，不再允许其他 append 在两者之间交错。
- 批量任务发现新增规范目录身份校验：目录名必须等于 task metadata 中稳定 task id 的 sanitize 结果；非规范同级目录（包括创建暂存目录）永不进入已发布任务集合。
- 新增三条回归测试：并发同 ID 只有一次成功且只产生一份完整布局/一个父链接；预存空残缺最终目录被拒绝且不被补写；写有有效 metadata 的暂存目录仍不会被批量发现。

### 文件变更

- 修改：`crates/vulcan-task-session/src/segmented_store/io.rs`
- 修改：`crates/vulcan-task-session/src/segmented_store/traits.rs`
- 修改：`crates/vulcan-task-session/src/segmented_store/store_impl.rs`
- 修改：`crates/vulcan-task-session/src/segmented_store/mirror.rs`
- 修改：`crates/vulcan-task-session/src/segmented_store/tests.rs`
- 修改：`TASK_LOG.md`
- 新增：无
- 删除：无

### 修改范围代码审核与自动修复

- 首轮审核发现暂存目录位于 `task/` 下时仍会被旧批量扫描读取；已自动增加规范目录名校验，并以“有效 metadata 位于非规范暂存名”回归测试锁定不可见性。
- 审核发现若先创建暂存目录、再读取根 metadata，根读取失败会绕过清理；已把全部前置读取和纯内存构造移到创建暂存目录之前。
- 审核确认 staging 与 final 始终为同级目录，rename 不跨文件系统；进程内并发由共享锁串行，另一个完整发布的非空最终目录会使竞争 rename 显式失败。
- 审核确认最终身份检查使用 `symlink_metadata`，不会跟随符号链接，也不会把空目录误判为可复用任务。
- 审核确认所有外部 append 入口只取得一次共享锁，owned helper 内部不重复加锁；创建、保存、事件追加之间不存在新反向锁序。
- 审核确认发布失败只清理仍归当前尝试所有的暂存目录，不会删除已经存在或由其他创建者发布的最终目录。
- 审核纠正“完整布局等于内容已持久化”的过度承诺：当前保证完整目录的原子可见性，初始 JSONL 仍只执行 flush，未声称崩溃级持久性。
- CodeKit 最终复查 io、traits、store_impl、mirror 与 tests 5 个文件，共解析 122 个结构项，错误为 0。

### 验证结果

- 新增 `concurrent_task_creation_publishes_one_complete_layout`：通过，两个并发创建仅一个成功，子任务初始行 1 条、根会话总行数 2 条且无暂存残留。
- 新增 `task_creation_rejects_existing_partial_final_directory`：通过，残缺最终目录未被写入任何生产产物。
- 新增 `task_discovery_ignores_unpublished_staging_layouts`：通过，有效暂存 metadata 未进入批量发现结果。
- `cargo clippy -p vulcan-task-session --all-targets --no-deps -- -D warnings`：通过，零告警。
- `cargo test -p vulcan-task-session`：通过，2 个测试套件共 45 项全部通过。
- `cargo test -p vulcan-agent-runtime`：通过，2 个测试套件共 583 项全部通过。
- `cargo test -p vulcan-service-kernel`：通过，2 个测试套件共 350 项全部通过。
- 5 个修改文件 `rustfmt --edition 2024 --check`：通过。
- 本轮文件范围 `git diff --check`：通过，未发现空白错误。

### 遗留问题与下一轮方向

- 子任务目录 rename 成功后，父级事件或 POSIX 父目录 sync 若失败，最终目录已经发布但 create 返回错误；下一轮应设计可识别发布阶段、可幂等补写父链接的恢复协议。
- 嵌套任务父链接虽已处于同一锁持有期，但直接父任务 JSONL/metadata 与根 JSONL/metadata 仍是多文件发布，后半段失败会留下单边事件，需要稳定 event_id 去重与重放。
- `append_encoded_json_line` 只 flush 不 `sync_all`；如果要承诺崩溃后一致性，需要统一 JSONL durability 策略并评估每事件 fsync 的性能成本。
- JSONL repair 仍使用毫秒时间戳临时文件名，跨进程碰撞风险尚未处理。
- 本轮只修改 Rust 后端 TaskSession 创建与发现，没有前端产物变化；第 23 轮前端生产构建结果仍有效。

## 第 35 轮：统一 JSON 与 JSONL Repair 原子文件发布器（2026-07-10）

### 问题与事实依据

- TaskSession JSONL repair 的 `write_json_lines_file_atomic` 原先通过 `current_unix_millis_u64()` 构造 `s001.jsonl.tmp.<millis>`；同一路径的线程或进程在同一毫秒重写时会命中相同临时文件。
- 旧 repair writer 使用 `File::create`，碰撞时会直接截断另一尝试正在写入的暂存内容，不具备独占所有权判定。
- 旧路径逐行写入后只调用 `flush`，没有 `sync_all`；也没有继承目标权限、拒绝符号链接或在 rename 后同步 POSIX 父目录项。
- 第 32 轮普通 JSON writer 已经具备原始 `OsString` 文件名、进程 id、单调序号、`create_new`、权限继承、文件同步与父目录同步，但实现仍以 JSON document 命名，repair 另行维护了一套较弱逻辑。
- JSONL repair 已把保留行加载到内存，继续构建一份完整编码字节不会引入新的数据规模等级，却能保证序列化失败发生在任何文件系统变更之前。

### 已查明的执行流程

1. task segment repair 在共享 append lock 内逐个读取 metadata 已索引的 JSONL，截断首条损坏记录后的尾部，并重建 segment metadata。
2. 物理 JSONL 需要修复时调用 `write_json_lines_file_atomic`，空 segment 初始化也走同一 helper。
3. 旧 helper 创建毫秒临时路径、逐行序列化写入、flush 后 rename；碰撞、权限与 durability 行为均独立于普通 JSON writer。
4. repair 随后写回主 metadata 与 task metadata；因此 JSONL 文件发布必须先完成完整字节的独占原子替换，不能把并发写坏的暂存文件交给后续 metadata 同步。
5. 同轮探索还确认：创建发布后父链接失败不会被生产 `persist_task_record_state` 自动重试；根流 rollover 后 metadata 发布前失败时，简单 event_id 查重无法发现未索引 segment，完整恢复需要持久化阶段/重放协议，属于待确认的存储协议调整，本轮不擅自实现。

### 长期优化调整

- 将 `JSON_STAGING_FILE_SEQUENCE` 泛化为统一 `FILE_STAGING_SEQUENCE`，普通 JSON 与 repair JSONL 共用同一进程内后缀分配器和跨进程 `create_new` 碰撞判定。
- 将 JSON 专用 staging helper 泛化为 `create_unique_staging_file` 与 `remove_staging_file`，保留原始非 Unicode 文件名身份。
- 抽取 `write_file_bytes_atomic(path, encoded, artifact_kind)`：创建父目录、拒绝符号链接目标、继承既有权限、写入完整字节、`sync_all`、rename，并在 POSIX 上同步父目录项。
- `write_json_file` 仅负责在文件系统变更前生成 pretty JSON bytes，然后调用共享发布器。
- `write_json_lines_file_atomic` 先在内存中序列化全部有序行与换行符，序列化成功后再以 `JSONL segment` 标签调用共享发布器；旧毫秒临时名与 `File::create` 路径全部删除。
- repair writer 提升为父模块可见的 crate 内 helper，仅用于真实生产逻辑的并发回归测试，没有扩大 crate 公共 API。
- 新增两个线程、每轮 barrier 同步、各 16 次重写同一路径的回归测试；两个 writer 必须全部成功，最终文件必须严格等于某一完整候选行集合，且同级目录不能遗留 `.vulcan-stage-*` 文件。

### 文件变更

- 修改：`crates/vulcan-task-session/src/segmented_store/io.rs`
- 修改：`crates/vulcan-task-session/src/segmented_store/tests.rs`
- 修改：`TASK_LOG.md`
- 新增：无
- 删除：无

### 修改范围代码审核与自动修复

- 审核确认 JSON 与 JSONL 的全部序列化均在创建暂存文件之前完成；自定义 Serialize 失败不会创建目录项或触碰目标文件。
- 审核确认 staging filename 使用目标精确 `OsString`、进程 id 与原子序号，`create_new` 是真实碰撞权威，序号不被误作唯一性保证。
- 审核确认两个并发 writer 允许 last-writer-wins，但每次 rename 的输入都来自独占且已同步的完整暂存文件，不会出现混合、截断或交叉删除。
- 审核确认权限只从最终目标读取并应用到当前尝试的暂存文件，符号链接目标在创建暂存文件前即被拒绝。
- 审核确认失败清理只删除当前尝试持有的 unique staged path，不再可能删除另一个 writer 的共享毫秒临时文件。
- 最终源码搜索确认 `with_extension`、`.tmp.`、`File::create(&temp_path)` 与旧 JSON staging helper 名称均为 0。
- CodeKit 最终复查 io 与 tests 2 个文件，共解析 56 个结构项，错误为 0。

### 验证结果

- 新增 `concurrent_jsonl_atomic_rewrites_publish_complete_files`：通过，32 次成对竞争发布全部成功，最终内容完整且无 staging 泄漏。
- `cargo clippy -p vulcan-task-session --all-targets --no-deps -- -D warnings`：通过，零告警。
- `cargo test -p vulcan-task-session`：通过，2 个测试套件共 46 项全部通过。
- `cargo test -p vulcan-agent-runtime`：通过，2 个测试套件共 583 项全部通过。
- `cargo test -p vulcan-service-kernel`：通过，2 个测试套件共 350 项全部通过。
- 2 个修改文件 `rustfmt --edition 2024 --check`：通过。
- 本轮文件范围 `git diff --check`：通过，未发现空白错误。

### 遗留问题与下一轮方向

- 普通事件追加 `append_encoded_json_line` 仍只 flush 不 `sync_all`；repair 重写已经耐久化，但增量追加在崩溃一致性上仍弱于 metadata 原子发布。
- 父链接发布后恢复需要持久化 pending/replay 协议，或先补齐根流 orphan segment 发现与 event_id 幂等规则；这是存储协议级重大决策，必须在方案确认后实施。
- 共享原子发布器在 POSIX rename 已成功、父目录 sync 失败时仍返回错误但目标已发布；阶段化错误结果尚未统一。
- 本轮只修改 Rust 后端文件发布与测试，没有前端产物变化；第 23 轮前端生产构建结果仍有效。

## 第 36 轮：在 Metadata 发布前耐久化增量 JSONL 事件（2026-07-10）

### 问题与事实依据

- `append_encoded_json_line` 原先在写入修复边界换行与新 JSONL 记录后只调用 `File::flush`；`File` 没有用户态缓冲器，flush 不能建立数据已到达稳定存储的保证。
- task segment append 在 JSONL 返回成功后会原子写主 metadata 与 task metadata，根 task link append 会原子写根 metadata；这些 metadata 文件已经 `sync_all` 并在 POSIX 上同步父目录。
- 旧顺序允许 metadata 的 `line_count`、`byte_len`、`next_global_seq` 与 active segment 先耐久化，而新增 JSONL 数据仍只停留在操作系统缓存；崩溃后会形成“metadata 宣称存在、物理行丢失”的反向不一致。
- `append_json_line` 的生产用途只有新建子任务初始行，随后会写三份子任务 JSON；`append_segment_event` 的两个生产调用分别是 task event 与根 task link，随后都会提交同目录 metadata。
- 因此 JSONL 追加只需在返回前耐久化内容与文件长度，目录项与其余文件 metadata 会由紧随其后的原子 metadata 发布完成同步。

### 已查明的执行流程

1. `append_segment_event` 序列化完整事件，决定是否 rollover，调用 `append_encoded_json_line` 写入活动 JSONL，再更新内存 segment 统计与 next sequence。
2. task event 调用方随后写主 metadata，读取并同步 task metadata，再写 task metadata。
3. 根 task link 调用方随后更新根 segments、active segment、next sequence、updated_at 与 task mirror，再写根 metadata。
4. 新建 task 初始行写入暂存目录后，随后写主 metadata、task metadata 与 state，最后才 rename 发布整个目录。
5. 所有生产路径均处于第 34 轮统一的共享 append transaction 内，不存在另一个 writer 在 JSONL data sync 与 metadata publication 之间插入。

### 长期优化调整

- 将 `append_encoded_json_line` 契约明确为“在 metadata 发布前追加并同步一条预编码 JSONL 记录”，补全参数与返回值双语文档。
- 写入尾部边界修复和新记录后，将无耐久保证的 `flush` 替换为 `sync_data`；只有同步成功才返回新增 byte count，调用方才会继续发布 segment metadata。
- 选择 `sync_data` 而非 `sync_all`：事件内容与文件长度是本阶段的权威数据，权限等文件元数据不在增量追加中变化；同目录 entry durability 由后续原子 metadata writer 的父目录同步承担。
- 保留错误显式传播：data sync 失败时不更新内存外的 metadata，后续 repair 仍以物理 JSONL 为事实源进行重建。

### 文件变更

- 修改：`crates/vulcan-task-session/src/segmented_store/io.rs`
- 修改：`TASK_LOG.md`
- 新增：无
- 删除：无

### 修改范围代码审核与自动修复

- 全仓 CodeKit 搜索确认 TaskSession 生产追加入口只有 task create、task segment append 与根 task link 三类，且每类都已查明后续 metadata 发布点。
- 审核确认 `ensure_jsonl_append_boundary` 可能补写的换行与新事件在同一个 file handle 上由一次 `sync_data` 共同覆盖，不存在只同步新行而漏掉边界修复。
- 审核确认 `sync_data` 位于 byte count 返回之前；同步失败不会让调用方用未耐久的字节数更新 segment metadata。
- 审核确认本轮不额外调用 `sync_all`，避免对每条事件重复同步不变的权限 metadata，同时不削弱已确定的数据顺序。
- 最终源码搜索确认增量 append 函数内 `file.flush` 为 0，唯一落盘调用为 `file.sync_data()`。
- CodeKit 最终复查 io 文件，共解析 32 个结构项，错误为 0。

### 验证结果

- `cargo clippy -p vulcan-task-session --all-targets --no-deps -- -D warnings`：通过，零告警。
- `cargo test -p vulcan-task-session`：通过，2 个测试套件共 46 项全部通过，覆盖 task create、task/root event append、rollover 与 repair。
- `cargo test -p vulcan-agent-runtime`：通过，2 个测试套件共 583 项全部通过。
- `cargo test -p vulcan-service-kernel`：通过，2 个测试套件共 350 项全部通过。
- 修改文件 `rustfmt --edition 2024 --check`：通过。
- 本轮文件范围 `git diff --check`：通过，未发现空白错误。

### 遗留问题与下一轮方向

- `sync_data` 会为每条 TaskSession 事件引入真实存储同步开销；当前优先保证权威 JSONL 与 metadata 的崩溃顺序，后续若需要批量提交必须先设计显式事务边界，不能退回无保证 flush。
- 根流 append 若在 data sync 成功后、root metadata 发布前失败，物理行已耐久但 metadata 可能陈旧；当前 next-sequence 会扫描已索引 segment 行，但 rollover 产生的未索引新 segment 仍需 orphan 发现机制。
- 父链接发布后恢复与原子文件发布阶段化错误仍属于后续存储协议治理范围。
- 本轮只修改 Rust 后端 JSONL 追加耐久顺序，没有前端产物变化；第 23 轮前端生产构建结果仍有效。

## 第 37 轮：恢复全工作区 Rust 严格 Clippy 质量门（2026-07-10）

### 问题与事实依据

- `cargo clippy --workspace --all-targets --no-deps -- -D warnings` 首轮发现 10 个错误：workbench 测试无说明 `unwrap`、workspace-store 6 处多余借用、一个 `&mut Vec` 过宽参数、hydration 用 `map` 执行副作用，以及生产函数位于测试模块之后。
- 修复首批错误后，完整编译链继续暴露此前被遮蔽的 4 个 service-kernel 错误：多余 let-and-return、手写可派生 Default、可折叠嵌套 if，以及 run-services 测试模块夹在生产函数中间。
- 8 个最终修改文件在本轮开始前均经 `git status` 核实；其中 `conversation_run_services.rs` 已包含此前精确会话停止目标调整，本轮必须保留该现有变更，只移动测试与收敛 Default。
- 严格 lint 指向的不全是格式问题：hydration 的 Option 链确实用 `map` 执行展示统计修复副作用后原样返回 runtime state；duplicate segment repair 只改变内层 lines，不会改变外层容器长度；这些事实决定了正确类型与组合子。

### 已查明的执行流程

1. workspace-store session fork 使用已解析的 `&Path` 源/目标重建 state、runtime log 与 fork source marker，调用点再次取引用只制造 `&&Path`，编译器立即解引用。
2. segmented duplicate repair 先扫描全部 segment 建立最新 message-index 行，再逐个 `take` 并替换每个内层 lines；外层 segment 列表不 push、remove 或重排，所以只需要可变切片。
3. hydration 加载 runtime state 后克隆 provider conversation state，修复 `messages` 中展示统计，再把同一个 runtime state 存入 snapshot；副作用是目标，变换不是目标。
4. provider usage 来自对 `message.payload` 的引用模式匹配，`usage` 已经是引用，再传 `&usage` 形成多余双重借用。
5. duplicate record-id repair 是生产 hydration 恢复函数，却被定义在内联 tests 模块之后；run-services 也在 plan/VMM/addon 生产 RPC 之前插入 tests 模块。
6. service history 的 else 分支只需直接返回带 slow-trace `inspect` 的 workspace index Future 结果；临时 snapshot 绑定没有二次用途。
7. stop mode 的默认值稳定等于 `AskIfRunningSubagents`，可由枚举 `#[default]` 明确表达；父回合 payload 字段只在 parent id 和 host payload 同时存在时写入。

### 长期优化调整

- workbench feedback 测试把 `unwrap` 替换为带业务语义的 `expect("stats entry should contain metadata")`，失败时直接指出契约缺失。
- 清除 edit/fork 与 segmented fork 调用点共 5 处 `&&Path` 多余借用，以及 hydration usage 的一处双重借用。
- `repair_duplicate_segmented_message_indexes` 参数从 `&mut Vec<...>` 收窄为 `&mut [...]`，在类型层明确函数不能改变外层 segment 集合形状。
- hydration runtime-state 链从副作用式 `map` 改为 `inspect`，保留原 Option 值并显式表达只观察/修复外部 messages。
- 将 `repair_duplicate_conversation_record_ids` 移到 hydration tests 之前，将 run-services tests 移到文件末尾，恢复“生产定义在前、测试在后”的源码组织。
- `StopConversationRunMode` 使用派生 `Default` 并在 `AskIfRunningSubagents` 上标记 `#[default]`，删除可漂移的手写实现。
- history index load 直接返回 `.await.inspect(...)?` 结果，删除只为立即返回而存在的 snapshot 绑定。
- 父回合 metadata 写入改为 Rust 2024 let-chain，同时要求 parent_turn_id 与 payload 存在，保持原有双条件语义。

### 文件变更

- 修改：`crates/vulcan-workbench-model/src/subagent/tests/metadata.rs`
- 修改：`crates/vulcan-workspace-store/src/workspace_store/persistence/edit_fork.rs`
- 修改：`crates/vulcan-workspace-store/src/workspace_store/segmented_session_store/segment_io.rs`
- 修改：`crates/vulcan-workspace-store/src/workspace_store/segmented_session_store/session_fork.rs`
- 修改：`crates/vulcan-workspace-store/src/workspace_store/session_load/hydration.rs`
- 修改：`crates/vulcan-service-kernel/src/conversation_history.rs`
- 修改：`crates/vulcan-service-kernel/src/conversation_run_services.rs`
- 修改：`crates/vulcan-service-kernel/src/subagent_event_ingest.rs`
- 修改：`TASK_LOG.md`
- 新增：无
- 删除：无

### 修改范围代码审核与自动修复

- 审核确认 slice 收窄后函数仍可 `iter`、`iter_mut` 并 `take` 每个内层 Vec，所有调用方 `&mut Vec` 可无损强制转换，业务行为不变。
- 审核确认 `Option::inspect` 接收 runtime state 引用，clone conversation state 与修复外部 `messages` 的顺序和原 `map` 完全一致，Option 的 Some/None 形状不变。
- 审核确认 Default 派生仍返回 AskIfRunningSubagents，现有精确 `conversation_id` 停止路径与此前工作区改动未被覆盖。
- 审核确认两个测试模块移动后成为各自文件最后的结构项，生产函数可在非 test 编译中保持连续定义。
- 审核确认所有多余借用删除仅恢复函数签名已有的 `&Path`/引用层级，没有改变所有权或生命周期。
- 最终全工作区严格 Clippy 二次完整执行为零问题，不再有被前置错误遮蔽的新诊断。
- CodeKit 最终复查 8 个文件，共解析 198 个结构项，错误为 0。

### 验证结果

- `cargo clippy --workspace --all-targets --no-deps -- -D warnings`：通过，全工作区零告警。
- `cargo test -p vulcan-workbench-model`：通过，2 个测试套件共 75 项全部通过。
- `cargo test -p vulcan-workspace-store`：通过，2 个测试套件共 109 项全部通过。
- `cargo test -p vulcan-service-kernel`：通过，2 个测试套件共 350 项全部通过。
- `cargo test -p vulcan-agent-runtime`：通过，2 个测试套件共 583 项全部通过。
- 8 个修改文件 `rustfmt --edition 2024 --check`：通过。
- 本轮文件范围 `git diff --check`：通过，未发现空白错误。

### 遗留问题与下一轮方向

- 本轮恢复的是全工作区静态质量门，未执行全部 workspace tests；已按受影响包边界运行 4 个核心包的 1117 项测试。
- workspace-store 的 `write_json_file_atomic` / `write_json_lines_file_atomic` 仍维护独立临时文件实现，与第 35 轮 task-session 共享原子发布器的强契约可能存在差距；下一轮可沿其真实调用链比较碰撞、权限与 durability。
- hydration 内联测试仍有较多未逐变量补充的旧测试代码，但本轮没有扩写这些逻辑；后续治理应以功能域测试抽取为单位，避免纯注释噪音。
- 本轮只修改 Rust 后端与 Rust 测试，没有前端产物变化；第 23 轮前端生产构建结果仍有效。

## 第 38 轮：统一 WorkspaceStore 原子文件发布与 JSONL 耐久追加（2026-07-10）

### 问题与事实依据

- workspace-store segmented I/O 的 JSON writer、JSONL writer 与“写事件同时重建 segment metadata”路径各自使用 `unique_json_temp_path + File::create + flush + rename`；session addon store 又复制了第四套近似实现。
- 临时路径虽然包含 UUID，但 `File::create` 仍会在真实碰撞时截断已有暂存文件；UUID 只降低概率，不能替代独占所有权判定。
- 旧 `unique_json_temp_path` 通过 `to_str().unwrap_or(...)` 构造名称，非 Unicode 目标会丢失原始文件名身份；不同目标可能退化到同一 fallback 名称族。
- 四条覆盖写路径都在序列化完成前创建父目录/暂存文件，序列化中途失败会遗留文件系统副作用；同时不继承既有权限、不拒绝符号链接、不 `sync_all` 暂存内容，也不在 POSIX rename 后同步父目录项。
- 单行 display JSONL append 与 runtime batch append 都只 `flush`；其中 `append_runtime_line_to_active_segment` 明确设计为“不刷新 metadata”，不能依赖后续 metadata writer 间接提供 durability。
- segment/runtime repair、message rewrite、fork identity rewrite、state、runtime metadata/index、addon 等调用最终都汇入这些 helper，属于 workspace-store 的共享事实持久化边界。

### 已查明的执行流程

1. display event append 在共享 append lock 内写活动 JSONL，更新内存 segment 统计，随后由上层写主 metadata；rollover、repair 和 transcript rewrite 也复用同一 segment I/O 层。
2. runtime tail append 在共享锁内追加批量 JSONL，重新读取全部 runtime lines，更新 runtime metadata 并重建 index。
3. runtime single-line append 只校验活动 segment 存在并追加一行，刻意不刷新 metadata，所以 append helper 自身必须保证数据落盘。
4. `write_segment_event_lines_atomic_with_metadata` 在同一轮序列化中同时构建完整字节与 segment 统计，原先自行创建/rename 临时文件。
5. session addon state 位于同一 segmented session 目录，但维护独立 atomic JSON helper；它和主 metadata/state 对原子发布有相同需求，无需另起协议。
6. workspace-store 模块内部已经允许 segmented helper 以 `pub(in crate::workspace_store)` 暴露给 sibling，addon 可直接复用而不增加新 crate 依赖。

### 长期优化调整

- 在 workspace-store segmented I/O 内抽取 `write_file_bytes_atomic`：序列化完成后创建父目录，读取并继承既有权限，拒绝符号链接目标，独占创建同级暂存文件，完整写入并 `sync_all`，rename 后在 POSIX 同步父目录。
- 新增 `create_unique_staging_file`：使用原始 `OsString` 目标文件名与 UUID 诊断后缀，并以 `OpenOptions::create_new` 作为跨线程/跨进程真实碰撞判定；碰撞会生成新候选继续尝试。
- JSON 与 JSONL writer 都先在内存中生成完整 bytes，序列化失败不创建父目录、目标或 staging 文件，再调用共享字节发布器。
- `write_segment_event_lines_atomic_with_metadata` 保留一次序列化同时计算 metadata 的效率，但最终 bytes 改走共享发布器，删除专用临时文件逻辑。
- display 单行 append 与 runtime batch append 将无耐久保证的 `flush` 替换为 `sync_data`；边界补换行与完整事件批次在返回前共同落盘。
- session addon store 删除本地 atomic writer、UUID temp helper 与重复 filesystem error adapter，直接复用 workspace-store 内部共享 JSON writer。
- 新增 `atomic_io.rs` 测试模块：自定义确定性失败 serializer 验证失败前零文件系统副作用；两个线程各 16 次 barrier 同步重写同一 JSONL，验证全部成功、最终内容为某一完整候选且无 staging 泄漏。

### 文件变更

- 修改：`crates/vulcan-workspace-store/src/workspace_store/segmented_session_store.rs`
- 修改：`crates/vulcan-workspace-store/src/workspace_store/segmented_session_store/segment_io.rs`
- 修改：`crates/vulcan-workspace-store/src/workspace_store/session_addon_store.rs`
- 修改：`crates/vulcan-workspace-store/src/workspace_store/segmented_session_store/tests/mod.rs`
- 新增：`crates/vulcan-workspace-store/src/workspace_store/segmented_session_store/tests/atomic_io.rs`
- 修改：`TASK_LOG.md`
- 删除：无

### 修改范围代码审核与自动修复

- 审核确认普通 JSON、普通 JSONL、带 metadata 的 display JSONL 与 addon JSON 四条覆盖写路径都只在完整序列化成功后进入共享发布器。
- 审核确认 staging 使用目标精确 `OsString`，UUID 只作为诊断/减碰撞后缀，`create_new` 才是权威所有权判定；失败清理只删除当前尝试独占的路径。
- 审核确认目标权限在 staging 发布前继承，符号链接在创建 staging 前显式拒绝，避免 rename 穿透或访问策略漂移。
- 审核确认 runtime 单行 append 没有后续 metadata 提交，因此本轮 `sync_data` 是独立必要保证；runtime tail 和 display append 也由相同顺序受益。
- 审核确认 addon 对 segmented 内部 writer 的复用只发生在同一 workspace_store 模块可见域，没有公共 API 或 crate 依赖变化。
- 最终源码搜索确认目标范围内 `unique_json_temp_path`、`File::create(&temp...)` 与追加路径 `file.flush` 为 0；新发布路径仅保留 `.vulcan-stage-*` 独占暂存名。
- CodeKit 最终复查 5 个文件，共解析 65 个结构项，错误为 0。

### 验证结果

- 新增 `atomic_json_serialization_failure_does_not_touch_filesystem`：通过，失败后父目录与目标均不存在。
- 新增 `concurrent_atomic_jsonl_rewrites_publish_complete_candidates`：通过，32 次成对竞争替换全部成功、内容完整且无 staging 泄漏。
- `cargo clippy -p vulcan-workspace-store --all-targets --no-deps -- -D warnings`：通过，零告警。
- `cargo clippy --workspace --all-targets --no-deps -- -D warnings`：通过，全工作区零告警。
- `cargo test -p vulcan-workspace-store`：通过，2 个测试套件共 111 项全部通过。
- `cargo test -p vulcan-service-kernel`：通过，2 个测试套件共 350 项全部通过。
- `cargo test -p vulcan-agent-runtime`：通过，2 个测试套件共 583 项全部通过。
- 5 个修改文件 `rustfmt --edition 2024 --check`：通过。
- 本轮文件范围 `git diff --check`：通过，未发现空白错误。

### 遗留问题与下一轮方向

- workspace-store 与 task-session 现在各自有等价原子字节发布器；若后续继续演进阶段化错误或 Windows 替换语义，应评估下沉到 session-core，但这属于跨 crate 架构决策，不能仅为去重擅自迁移。
- 共享发布器在 rename 已成功、POSIX 父目录 sync 失败时仍以普通错误返回，调用方无法区分“未发布”与“已发布但目录同步失败”。
- `move_segmented_session_directory` 仍采用 exists-check 后 rename，且没有发布后父目录 sync；目录 fork/archive 的跨进程竞争与耐久语义尚未统一。
- 本轮只修改 Rust 后端持久化与 Rust 测试，没有前端产物变化；第 23 轮前端生产构建结果仍有效。

## 第 39 轮：事务化 Segmented Session 目录归档与恢复（2026-07-10）

### 问题与事实依据

- workspace-store 的 `move_segmented_session_directory` 只执行 `destination.exists()` 检查、创建父目录和 `fs::rename`；它跟随符号链接判断，不验证源是真实目录，也不同步源/目标父目录。
- 按 ID 归档使用该 helper，但按日期归档与恢复直接调用 `fs::rename`，同一生命周期操作存在两套无覆盖与错误语义。
- 所有归档/恢复路径都是“先移动目录，再调用 `patch_segmented_session_metadata`”；metadata 解析失败、atomic writer 拒绝符号链接、rename 后目录 sync 失败等情况都会让 API 返回错误，但目录已经从活动树切到归档树或反向切换。
- persistence 生命周期调用对 `Result<Option<PathBuf>>` 使用 `?`，仅忽略“目录不存在”的 Option，不会吞掉 Err；因此底层一旦正确返回失败，上层数据库 archive flag 更新会停止，适合把一致性保证放在目录事务层。
- 活动树与归档树父目录不同；目录 rename 的持久性既需要同步源父目录的删除项，也需要同步目标父目录的新增项。

### 已查明的执行流程

1. `archive_segmented_session_directory_if_present` 从持久化索引读取 project tag/created_at，按精确日期定位目录，完成文件系统归档后才更新 SQLite session archive flag。
2. project archive 与按 summary 归档通过同步 helper 先移动目录并 patch `archived_at`，上层随后更新 session/project 索引。
3. restore project 流程先把归档目录移回活动树并清除 `archived_at`，再恢复索引状态。
4. `archive_segmented_session_by_id` 会扫描活动项目目录定位 session，再构造归档目标；按日期路径已经持有确定源/目标，不需要扫描。
5. metadata patch 使用第 38 轮强化后的原子 JSON writer，可能在发布前失败，也可能在 rename 已发布但 POSIX 目录 sync 失败后返回错误；回滚必须保留移动前原 metadata 以覆盖两种阶段。

### 长期优化调整

- 强化 `move_segmented_session_directory`：通过 `symlink_metadata` 要求源为非符号链接真实目录，任何文件/目录/符号链接占用目标都显式拒绝，只有目标缺失时才创建父目录并 rename。
- 新增 `synchronize_segmented_session_directory_move`，在 Unix 上同步源父目录，并在父目录不同的情况下同步目标父目录；非 Unix 平台保持显式 no-op 契约。
- 新增 `move_and_patch_segmented_session_directory` 事务：任何 rename 前先读取权威原 metadata，随后统一执行 move、lifecycle metadata patch 与双父目录同步。
- commit 后半段失败时，事务尝试把目录从目标移回源、用预读原文档覆盖恢复 metadata，再同步回滚移动的两个父目录。
- 回滚每个阶段都保留首要 commit error，并在目录回移、metadata 恢复或回滚同步再次失败时返回组合错误，不用模糊 fallback 掩盖实际磁盘状态。
- 按日期归档、按 ID 归档与恢复三条生产路径全部改用同一事务 helper，删除 session management 中直接 `fs::rename` 与独立父目录创建逻辑。
- 新增三条回归：损坏活动 metadata 时归档在 rename 前失败且活动目录保留；损坏归档 metadata 时恢复在 rename 前失败且归档目录保留；已有空归档目标时拒绝覆盖且源目录不移动。

### 文件变更

- 修改：`crates/vulcan-workspace-store/src/workspace_store/segmented_session_store/segment_io.rs`
- 修改：`crates/vulcan-workspace-store/src/workspace_store/segmented_session_store/session_management.rs`
- 修改：`crates/vulcan-workspace-store/src/workspace_store/segmented_session_store/tests/metadata_archive.rs`
- 修改：`TASK_LOG.md`
- 新增：无
- 删除：无

### 修改范围代码审核与自动修复

- 审核确认原 metadata 在任何 rename 前读取；损坏文档、缺失 metadata 或解析失败不会再改变目录树位置。
- 审核确认 patch 已发布但返回 sync 错误时，回滚仍会移回目录并用预读文档覆盖原 metadata，不依赖错误阶段猜测。
- 审核确认 rollback move 使用同一无覆盖 helper，只有原 source 路径仍为空时才恢复；若被外部占用会显式报告 rollback move failure，绝不覆盖新目录项。
- 审核确认成功 commit 与成功 rollback 都会同步活动/归档两个不同父目录；同父目录移动只同步一次。
- 审核确认 persistence 上层在文件系统事务 Err 时不会继续更新数据库 archive flag，目录与索引不会因新错误传播顺序产生进一步漂移。
- 最终源码搜索确认 session management 生产路径中直接 `fs::rename` 为 0，三条 archive/restore 入口全部调用 `move_and_patch_segmented_session_directory`。
- CodeKit 最终复查 3 个文件，共解析 57 个结构项，错误为 0。

### 验证结果

- 新增 `archive_with_invalid_metadata_keeps_active_directory`：通过，归档失败后活动目录存在、归档目录不存在。
- 新增 `restore_with_invalid_metadata_keeps_archived_directory`：通过，恢复失败后活动目录不存在、归档目录保留。
- 新增 `archive_rejects_existing_empty_destination_without_moving_source`：通过，空目标未被覆盖或填充，源目录保留。
- metadata archive 定向测试：7 项全部通过。
- `cargo clippy -p vulcan-workspace-store --all-targets --no-deps -- -D warnings`：通过，零告警。
- `cargo clippy --workspace --all-targets --no-deps -- -D warnings`：通过，全工作区零告警。
- `cargo test -p vulcan-workspace-store`：通过，2 个测试套件共 114 项全部通过。
- `cargo test -p vulcan-service-kernel`：通过，2 个测试套件共 350 项全部通过。
- `cargo test -p vulcan-agent-runtime`：通过，2 个测试套件共 583 项全部通过。
- 3 个修改文件 `rustfmt --edition 2024 --check`：通过。
- 本轮文件范围 `git diff --check`：通过，未发现空白错误。

### 遗留问题与下一轮方向

- 标准库 `fs::rename` 在部分 Unix 语义下可能替换竞态出现的空目标目录；当前 preflight 与“并发发布目录必为非空”约束覆盖正常 writer 竞争，但真正跨进程 no-replace 仍需要平台 API 或新依赖，属于需确认方案。
- post-move rollback 路径已实现并完成代码审核，但当前测试环境没有安全的 I/O 故障注入点来强制命中“rename 成功、patch writer 失败”阶段；新增测试覆盖可确定的 rename 前失败与目标冲突。
- `delete_segmented_session_by_created_date` 仍使用 exists-check 后 `remove_dir_all`，没有 tombstone、父目录同步或失败恢复；下一轮可沿永久删除与索引删除顺序继续治理。
- 本轮只修改 Rust 后端生命周期持久化与 Rust 测试，没有前端产物变化；第 23 轮前端生产构建结果仍有效。

## 第 40 轮：可恢复并耐久化 Segmented Session 永久删除（2026-07-10）

### 问题与事实依据

- `delete_segmented_session_by_created_date` 原先对规范会话目录执行 `exists` 检查后直接 `remove_dir_all`；递归删除中途失败会把不完整目录继续留在活动或归档目录扫描根内，重试无法区分“尚未开始”与“部分删除”。
- 单会话永久删除先按 `archived = true` 查询持久化 locator，再删除底层目录，最后删除 `workspace_session_index` 行；项目永久删除对每个 session 重复同一顺序，全部完成后才删除 project 行。
- 上层使用 `?` 传播文件系统错误，因此只要目录删除协议在完全成功前返回错误，locator/index 会保留并为下一次重试提供确定身份；正确的一致性边界应建立在目录层，而不是用兼容性兜底跳过错误。
- 墓碑初稿已经把规范目录移出 catalog 后再递归删除，但修改范围审核发现两处耐久缺口：目标多级父目录由普通 `create_dir_all` 创建却未逐级同步，目录 rename 后又先同步源父目录再同步目标父目录。
- 在 POSIX 崩溃语义下，先同步源删除再同步目标发布会留下“源删除已耐久、目标插入尚未耐久”的丢失窗口；目标父路径自身尚未耐久时，仅同步最终父目录也不足以证明完整墓碑路径可恢复。

### 已查明的执行流程

1. `permanently_delete_archived_session` 查询归档 locator；locator 唯一提供 project id、created_at 与 archived 状态，随后进入精确目录删除入口。
2. `delete_segmented_session_directory_for_locator_if_present` 将 created_at 转换为目录日期，并由 project tag、日期、session id 与 archived 状态构造唯一规范路径。
3. 删除入口在 `.segmented_session_deletions` 内镜像规范路径作为确定性墓碑，并通过 `symlink_metadata` 验证规范目录与墓碑只能处于四种状态：两者均缺失、仅规范目录、仅墓碑、两者同时存在。
4. 仅规范目录存在时，先从最近现有真实祖先向下创建墓碑父层级；每个新目录项在 Unix 上立即同步其父目录，然后才把规范目录 rename 到墓碑。
5. rename 后先同步目标父目录发布墓碑，再同步源父目录提交规范目录删除；若同步阶段返回错误，locator 仍保留，下一次删除会从仅墓碑状态续删。
6. 仅墓碑存在时直接续删；两者同时存在时显式报告歧义并保持两边不变；两者均缺失时返回未找到。
7. 墓碑递归删除成功后同步其父目录，目录 helper 才返回成功；随后上层删除 session index，项目删除则在全部 session 完成后删除 project 行。

### 长期优化调整

- 新增独立的 `.segmented_session_deletions` 内部根，墓碑路径保留活动/归档根、项目与会话叶子身份，同时天然位于 catalog 只扫描的 `sessions` / `archived_sessions` 两棵树之外。
- 将永久删除改为确定性两阶段协议：先把完整规范目录原子移动成不可见墓碑，再递归清理墓碑；中断后以同一路径幂等续删，不再暴露部分删除会话。
- 对规范路径和墓碑路径使用 `symlink_metadata`，拒绝符号链接、文件及其他非真实目录项；同时存在规范目录与墓碑时停止，禁止猜测权威副本。
- 新增 `create_segmented_session_move_parent_durably`，从最近现有祖先向下独立创建缺失目录，验证并发创建者产出的目录项，并在 Unix 上逐级同步父目录。
- 修正所有 segmented session 目录移动的同步顺序：目标父目录先于源父目录完成同步，使 archive、restore、rollback 与 deletion 都遵循“替代身份先耐久、旧身份后移除”的一致协议。
- 新增目录删除后的父目录同步 helper，确保 POSIX 上墓碑删除目录项在返回成功前达到耐久存储。
- 增加“仅墓碑续删且不进入 catalog”与“规范目录/墓碑同时存在时拒绝删除”回归；既有 archive/restore/delete 生命周期回归继续覆盖正常规范目录删除与重复删除。

### 文件变更

- 修改：`crates/vulcan-workspace-store/src/workspace_store/segmented_session_store.rs`
- 修改：`crates/vulcan-workspace-store/src/workspace_store/segmented_session_store/segment_io.rs`
- 修改：`crates/vulcan-workspace-store/src/workspace_store/segmented_session_store/session_management.rs`
- 修改：`crates/vulcan-workspace-store/src/workspace_store/segmented_session_store/tests/metadata_archive.rs`
- 修改：`TASK_LOG.md`
- 新增：无
- 删除：无

### 修改范围代码审核与自动修复

- CodeKit 先建立 workspace-store 结构图，再确认永久删除的唯一生产顺序为“归档 locator 查询 → 精确目录删除 → session index 删除”，项目删除在 session 循环后才删除 project 行。
- 审核确认 catalog 根只可能选择 `SEGMENTED_SESSIONS_DIR` 或 `SEGMENTED_ARCHIVED_SESSIONS_DIR`，内部墓碑树不会被活动/归档列表、水合或重放路径发现。
- 审核发现墓碑初稿沿用的同步顺序会先耐久化源删除；已自动修正为目标发布优先，并补充目标多级父路径的逐级耐久创建，消除已识别的崩溃丢失窗口。
- 审核确认父层级创建对既有非目录或符号链接显式失败；并发方在预检后创建同名目录时会重新读取类型，只有真实目录才能继续。
- 审核确认目录移动或同步失败时不会执行 index 删除；墓碑递归删除成功但最终父目录同步失败时也保留 locator，重试看到两边均缺失后可完成 index 清理。
- 最终源码搜索确认永久删除入口不再直接对规范 `session_dir` 执行 `remove_dir_all`，递归删除只作用于 catalog 外的确定性墓碑。
- CodeKit 最终复查 4 个文件，共解析 69 个结构项，错误为 0。

### 验证结果

- 删除定向回归：3 项全部通过，覆盖正常删除、既有墓碑续删与双目录歧义拒绝。
- `cargo test -p vulcan-workspace-store`：通过，2 个测试套件共 116 项全部通过。
- `cargo test -p vulcan-service-kernel`：通过，2 个测试套件共 350 项全部通过。
- `cargo test -p vulcan-agent-runtime`：通过，2 个测试套件共 583 项全部通过。
- `cargo clippy -p vulcan-workspace-store --all-targets --no-deps -- -D warnings`：通过，零告警。
- `cargo clippy --workspace --all-targets --no-deps -- -D warnings`：通过，全工作区零告警。
- 4 个直接修改 Rust 文件 `rustfmt --edition 2024 --check`：通过。
- 本轮文件范围 `git diff --check`：通过，未发现空白错误。

### 遗留问题与下一轮方向

- `fs::rename` 的“目标不存在”预检与实际 rename 之间仍存在跨进程竞态；真正的 no-replace 需要 Linux `renameat2(RENAME_NOREPLACE)`、Windows 对应平台 API 或经过确认的新依赖，属于后续需要方案确认的平台抽象决策。
- 永久删除项目仍信任调用方传入的 `session_ids` 列表；若调用方快照遗漏归档 session，当前实现可能删除 project index 而遗留 session locator 与目录，下一轮应沿真实调用入口核实并把子会话枚举收归持久化边界。
- 新增耐久同步在非 Unix 平台保持显式 no-op，与现有标准库能力一致；Windows 目录项刷新若要提供同等级保证，需要单独的平台句柄实现与故障注入验证。
- 本轮只修改 Rust 后端生命周期持久化与 Rust 测试，没有前端产物变化；第 23 轮前端生产构建结果仍有效。

## 第 41 轮：收归归档永久删除的权威身份边界（2026-07-10）

### 问题与事实依据

- `permanently_delete_archived_session` 原先查询 `archived = true` locator；查询为空时却继续执行不带 archive 条件的 `delete_session_index_row`。传入活动会话 ID 会直接删除活动索引、保留活动目录，制造不可见孤儿。
- `permanently_delete_archived_project` 原先接受调用方提供的 `Vec<SessionId>`，既不证明列表完整，也不校验每个 ID 属于目标项目；任意其他项目或活动会话 ID 都可能走到同一个无条件索引删除入口。
- `workspace_session_index` 只有普通 project/archive 索引，没有关联 `projects` 的外键或级联约束，删除 project 行不会自动约束、清理或拒绝孤儿 session locator。
- 工作区加载从 SQLite 读取全部 locator 后，会对底层 session 目录不存在的行执行 `continue`；因此 WebUI snapshot 明确不是完整 locator 集合，不能作为永久删除项目的权威子会话参数。
- 当前真实 WebUI“移除项目”链路是 `service_remove_project → remove_project_after_archiving_sessions`，与这两个尚未接入 RPC 的永久删除公共 API 不同；全仓字面量与结构化搜索确认永久删除 API 目前只有定义和公共导出。
- 即使项目行已经归档，旧的不完整归档流程仍可能留下 `archived = false` 的子 locator；若永久删除只查归档子集，仍会删除 project 行并遗留活动孤儿。

### 已查明的执行流程

1. WebUI 项目菜单的“移除项目”先从保留 snapshot 收集活动会话，归档这些会话后删除活动 project 行，再刷新工作台；这不是物理永久删除。
2. 永久删除单会话的权威输入只有 `session_id`，SQLite locator 同时提供 archive 状态、project id、project tag 与 created_at，足以唯一定位目录或第 40 轮墓碑。
3. 永久删除项目的权威输入只有 `project_id`；`projects.archived` 可证明目标生命周期，`workspace_session_index.project_id + archived` 可在持久化边界枚举完整子集。
4. 新流程先拒绝空 project id，再用精确 SQL 聚合验证项目行确实处于归档状态；验证失败时不触碰任何子会话。
5. 项目预检随后查询同 project 的全部活动 locator；只要非空就显式报告混合生命周期状态并在任何破坏性操作前停止。
6. 通过预检后，按 `project_id` 与 `archived = true` 查询完整 locator 集合，以 sort order、更新时间和 ID 确定删除顺序，并在持久化边界解析每个 UUID。
7. 单会话与项目子会话统一调用共享删除提交函数：先用权威 locator 完成目录/墓碑删除，再删除对应索引行；任一步失败都会保留尚未提交的后续身份。
8. 项目只有在全部归档子会话工件及索引删除成功后，才执行带 `archived != 0` 条件的 project 行删除；中途失败可按剩余 SQLite locator 重试。

### 长期优化调整

- 单会话永久删除将归档 locator 从可选分支改为强制前置条件；活动、缺失或状态不匹配的 session id 直接返回错误，不再触碰任何索引。
- 项目永久删除 API 移除外部 `session_ids` 参数，只接受 controller 与精确 project id；未来调用方无法再传入遗漏、跨项目或状态错误的会话列表。
- 新增 `archived_project_exists` 精确查询，在删除任何子工件前证明目标 project 行存在且已归档。
- 新增 `query_session_index_locators_for_project`，按项目归属与归档状态从 SQLite 查询确定顺序的权威子集，不扫描目录、不依赖 UI snapshot。
- 抽取共享 `SessionLocatorRow`、`PersistedSessionIndexEntry` 与严格 UUID 转换器；单行查询和项目批量查询复用同一字段解释与损坏诊断。
- 项目删除增加活动子 locator 预检；发现混合状态时停止并报告数量，既不猜测删除活动数据，也不删除仍被引用的项目。
- 抽取 `delete_archived_session_artifacts_and_index`，把“工件/墓碑先删、索引后删”的提交顺序集中到一个函数，避免单会话与项目路径再次漂移。
- 为原始 locator 转换新增有效身份/归档状态保留与无效 UUID 拒绝两条回归，证明持久化边界不会构造替代身份。

### 文件变更

- 修改：`crates/vulcan-workspace-store/src/workspace_store/persistence/deletion.rs`
- 修改：`crates/vulcan-workspace-store/src/workspace_store/persistence/index.rs`
- 修改：`crates/vulcan-workspace-store/src/workspace_store/persistence/projects.rs`
- 修改：`TASK_LOG.md`
- 新增：无
- 删除：无

### 修改范围代码审核与自动修复

- CodeKit 与全仓字面量搜索共同确认永久删除 API 当前没有生产调用点；本轮可以直接收窄签名，不需要保留错误的三参数历史接口。
- 审核确认单会话路径只有在 `query_session_index_locator(..., true)` 返回 locator 后才会进入共享删除函数，旧的“None 后继续删索引”控制流已不存在。
- 审核确认项目批量查询的 SQL 同时约束精确 `project_id` 和 archive 状态，其他项目 ID 无法进入删除集合；旧 `session_ids: Vec<SessionId>` 签名搜索结果为 0。
- 审核确认 locator UUID 在任何文件系统路径构造前严格解析，损坏行会返回包含原始 ID 的显式错误，不使用 nil、随机 UUID 或候选式 fallback。
- 修改范围复查发现仅查询归档子集仍会遗漏旧混合状态中的活动 locator；已自动增加活动子集前置查询与非空拒绝，确保 project 行不会在已知孤儿风险下删除。
- 审核确认项目归档预检、活动子集预检、归档子集查询都发生在首个破坏性操作之前；子会话循环失败时 project 行保持不变。
- CodeKit 最终复查 3 个文件，共解析 36 个结构项，错误为 0。

### 验证结果

- 新增 locator 转换定向回归：2 项全部通过，覆盖有效 UUID/字段/归档状态保留与无效 UUID 显式拒绝。
- `cargo test -p vulcan-workspace-store`：通过，2 个测试套件共 118 项全部通过。
- `cargo test -p vulcan-service-kernel`：通过，2 个测试套件共 350 项全部通过。
- `cargo test -p vulcan-agent-runtime`：通过，2 个测试套件共 583 项全部通过。
- `cargo clippy -p vulcan-workspace-store --all-targets --no-deps -- -D warnings`：通过，零告警。
- `cargo clippy --workspace --all-targets --no-deps -- -D warnings`：通过，全工作区零告警。
- 3 个直接修改 Rust 文件 `rustfmt --edition 2024 --check`：通过。
- 本轮文件范围 `git diff --check`：通过，未发现空白错误。

### 遗留问题与下一轮方向

- 永久删除 API 尚未接入 Service RPC，因此本轮 controller 查询路径主要由静态类型、严格 Clippy、消费者编译与 locator 纯转换回归验证；后续接入 RPC 时应增加真实 controller 数据平面集成测试。
- 项目归档状态预检、活动/归档 locator 查询、文件系统删除与最终 project 删除不是跨 SQLite/文件系统事务；同一项目若允许并发 restore/archive/delete，仍需显式生命周期租约或持久化删除状态机。
- 当前真实 `service_remove_project` 仍从 retained snapshot 传入活动会话列表，而加载层会跳过目录缺失 locator；下一轮应继续沿活动项目移除路径确认是否也需收归 SQLite 枚举，或至少在不一致状态下拒绝删除 project 行。
- 本轮只修改 Rust 后端持久化边界与 Rust 测试，没有前端产物变化；第 23 轮前端生产构建结果仍有效。

## 第 42 轮：统一项目级归档与移除的 SQLite 权威边界（2026-07-10）

### 问题与事实依据

- WebUI `removeProject` 通过 `service_remove_project` 进入 Service Kernel，原流程从 retained `WorkspaceSnapshot` 收集会话摘要，再把该列表传给 `remove_project_after_archiving_sessions`；持久化层处理完传入列表后直接删除 active project 行。
- 工作区加载会跳过底层 session 目录不存在的 locator，项目成员列表与 session 自身 project id 也可能暂时不一致；因此 snapshot 会话集合无法证明 SQLite 中该项目的 active locator 已全部处理。
- `service_archive_project_conversations` 在 snapshot 会话列表为空时直接返回，SQLite 中被 snapshot 过滤的 active locator 不会归档；非空时又只手工迁移旧列表对应的内存投影。
- 旧 `archive_project_sessions` 依赖 `SessionSummary` 的 created_at 与 project id 重新构造路径，并通过 upsert 覆盖索引；它没有使用 locator 中的 project tag、created_at 与现有排序字段作为权威身份。
- 未使用的 `archive_project_with_sessions → archive_project_session_ids → archive_segmented_session_by_id` 旧链路按调用方 ID 扫描目录，只移动目录而不更新 session archive flag，能够直接制造“归档项目 + 活动 locator”的混合状态。
- 手工 `archive_project_sessions_in_retained_state` 复制了会话、消息、运行态、选中项与 workbench generation 的迁移逻辑；共享 `refresh_workbench_snapshot` 已实现权威索引重载、活动运行态保护、选中会话修复与并发 generation 合并，两套投影协议会持续漂移。
- 修改范围审核还发现 `persistence_segmented_session_date_from_unix_millis_text` 使用 `unwrap_or_default()`；损坏 created_at 会静默定位到 1970 年并继续更新索引，可能把 locator 标为归档却遗留真实活动目录。

### 已查明的执行流程

1. WebUI 项目菜单提交 `service_remove_project(projectId)`；Service Kernel 先从 retained snapshot 验证目标活动项目与默认项目移除策略。
2. 原持久化流程只遍历 snapshot 派生的 `SessionSummary`，按摘要日期移动目录并 upsert archive 状态，随后执行 `DELETE FROM projects WHERE archived = 0`。
3. `workspace_session_index` 已持久化 session id、project id、project tag、archive 状态、sort order、created_at 与 updated_at；这些字段足以在不读取 snapshot、不扫描目录的情况下唯一枚举并定位项目子会话。
4. 新 `archive_project_sessions` 先验证 project 行仍为 active，再按 `project_id + archived = false` 查询完整 locator 集合；UUID 与 created_at 在首次破坏性操作前严格解析。
5. 每个 locator 通过同一个 helper 按精确 project tag/date/session id 发布 archived 目录，只有目录阶段成功后才更新该 locator 的 archive 状态；已经移走但索引未提交的重试也会完成状态修复。
6. 会话循环结束后再次查询 active locator；若并发或局部写入留下任何活动行，流程显式失败并保留 project 行。
7. “仅归档会话”到此返回并保持 project active；“移除项目”随后删除 active project 行；“归档整个项目”随后更新 project archive flag。
8. Service Kernel 在持久化成功后统一调用 `refresh_workbench_snapshot`，从 SQLite 与分段目录重建返回投影，不再使用旧 snapshot 列表手工修改 conversations/messages/runtime state。

### 长期优化调整

- `archive_project_sessions` 移除 `sessions: &[SessionSummary]`，只接受 controller 与 project id；完整子会话范围由持久化边界内部查询。
- `remove_project_after_archiving_sessions` 同步移除 snapshot 参数，必须在 authoritative archive helper 证明 active 子集为空后才能删除 project 行。
- 将未使用且错误的 `archive_project_with_sessions` 改为无外部 ID 的 `archive_project`；删除 `archive_project_session_ids` 旧调用层。
- 将 `archived_project_exists` 泛化为 `project_archive_state_exists(project_id, archived)`，项目级归档、移除与永久删除复用同一精确生命周期预检。
- 抽取 locator 驱动的 `archive_segmented_session_directory_for_locator`；单会话与项目批量归档共享“目录先发布、索引状态后提交”顺序。
- 删除无生产调用的 `archive_segmented_session_by_id` 扫描路径，并将两条生命周期测试切换到精确 created-date 定位。
- Service Kernel 项目归档改为持久化成功后完整刷新；删除 snapshot 会话收集器、no-op retained 返回器、约百行手工归档投影器及其只验证旧列表补偿逻辑的测试。
- 将时间戳到目录日期转换改为 `Result`：空白、非数字与 i64 溢出全部显式失败，所有 7 个生产调用点使用 `?` 传播，不再生成 1970 fallback。
- 新增无效时间戳回归，证明空白与非数字 locator 身份不会被替换为零毫秒。

### 文件变更

- 修改：`crates/vulcan-service-kernel/src/project_conversation_services/project_services.rs`
- 修改：`crates/vulcan-workspace-store/src/workspace_store/mod.rs`
- 修改：`crates/vulcan-workspace-store/src/workspace_store/persistence.rs`
- 修改：`crates/vulcan-workspace-store/src/workspace_store/persistence/deletion.rs`
- 修改：`crates/vulcan-workspace-store/src/workspace_store/persistence/edit_fork.rs`
- 修改：`crates/vulcan-workspace-store/src/workspace_store/persistence/index.rs`
- 修改：`crates/vulcan-workspace-store/src/workspace_store/persistence/locators.rs`
- 修改：`crates/vulcan-workspace-store/src/workspace_store/persistence/projects.rs`
- 修改：`crates/vulcan-workspace-store/src/workspace_store/persistence/session_lifecycle.rs`
- 修改：`crates/vulcan-workspace-store/src/workspace_store/persistence/tests.rs`
- 修改：`crates/vulcan-workspace-store/src/workspace_store/segmented_session_store/session_management.rs`
- 修改：`crates/vulcan-workspace-store/src/workspace_store/segmented_session_store/tests/metadata_archive.rs`
- 修改：`crates/vulcan-workspace-store/src/workspace_store/session_load/tests.rs`
- 修改：`TASK_LOG.md`
- 新增：无
- 删除：无

### 修改范围代码审核与自动修复

- 审核确认 Service Kernel retained snapshot 现在只验证用户选中的 active project 与默认项目策略，不再决定持久化子会话范围；两个项目动作均在成功后走共享权威刷新。
- 审核确认 active project 预检、首次 active locator 查询、全部目录/索引归档与二次 active locator 查询均发生在 project 行变更前；失败重试只需处理剩余 active locator。
- 审核确认批量查询携带 locator 原始 project tag 与 created_at，删除了使用 project id 猜测 tag 或按目录扫描寻找日期的项目级生产路径。
- 严格 Clippy 首轮发现 `archive_segmented_session_by_id` 已成为 dead code；已删除函数并将测试改为确定日期 API，没有添加 lint allow。
- 审核确认旧 `archive_project_with_sessions`、`archive_project_session_ids`、`archive_segmented_session_by_id`、`active_sessions_for_project_action`、`archive_project_sessions_in_retained_state` 与 retained no-op helper 的最终源码搜索结果均为 0。
- 审核确认时间戳转换的 `unwrap_or_default` 已删除；fork、locator 初始化、rename/archive/restore/delete 等全部调用方显式传播解析错误。
- CodeKit 最终复查 8 个核心源码文件，共解析 112 个结构项，错误为 0。

### 验证结果

- created-date archive/restore/delete 定向生命周期测试：1 项通过。
- session-boundary archive/restore 展示投影测试：1 项通过。
- Service Kernel project removal 策略定向测试：3 项通过。
- 时间戳目录日期定向测试：2 项通过，覆盖有效日期与无效输入拒绝。
- `cargo test -p vulcan-workspace-store`：通过，2 个测试套件共 119 项全部通过。
- `cargo test -p vulcan-service-kernel`：通过，2 个测试套件共 349 项全部通过；删除 1 项只服务旧 snapshot 会话收集器的测试。
- `cargo test -p vulcan-agent-runtime`：通过，2 个测试套件共 583 项全部通过。
- `cargo clippy -p vulcan-workspace-store --all-targets --no-deps -- -D warnings`：通过，零告警。
- `cargo clippy -p vulcan-service-kernel --all-targets --no-deps -- -D warnings`：通过，零告警。
- `cargo clippy --workspace --all-targets --no-deps -- -D warnings`：通过，全工作区零告警。
- 13 个直接修改 Rust 文件 `rustfmt --edition 2024 --check`：通过。
- 本轮文件范围 `git diff --check`：通过，未发现空白错误。

### 遗留问题与下一轮方向

- 项目 active 状态预检、locator 查询、文件系统 rename、索引逐行更新与 project 行变更仍不是跨 SQLite/文件系统事务；二次查询可阻止已观察到的残留，但跨进程竞争仍需要项目级持久化生命周期租约或状态机。
- WorkspaceStore 尚无可在测试内启动的 controller 数据平面 fixture；本轮权威 SQL 路径由严格类型、全消费者编译与现有端到端服务结构验证，后续应建设 in-memory/controller 集成测试支架。
- `archive_single_session_for_project` 与 `restore_project_with_sessions` 仍依赖调用方 `SessionSummary` 构造路径和 upsert 字段；下一轮可继续核实单会话/恢复链是否也应完全收归 locator。
- 加载层仍会静默跳过目录缺失 locator；本轮已让项目级动作不再依赖该过滤结果，但诊断、修复或展示这些损坏 locator 仍需独立治理。
- 本轮只修改 Rust 后端生命周期持久化、服务投影与 Rust 测试，没有前端产物变化；第 23 轮前端生产构建结果仍有效。

## 第 43 轮：统一单会话归档与项目恢复的权威定位及中断续作（2026-07-10）

### 问题与事实依据

- `service_archive_conversation` 原先从 retained `WorkspaceSnapshot` 取得 `SessionSummary` 与 project id，再调用 `archive_single_session_for_project`；持久化层按摘要中的 created_at 与调用方 project id 重建目录路径，并通过 upsert 重写索引字段，而不是使用 SQLite 已持久化的 locator。
- 单会话归档成功后，Service Kernel 通过 `archive_conversation_in_retained_state` 手工迁移 sessions、archived sessions、messages、runtime state、selected session 与 workbench generation；这复制了共享 `refresh_workbench_snapshot` 的投影职责，且无法呈现 SQLite 中 snapshot 先前遗漏的 locator。
- `service_add_project_from_path` 恢复归档项目时，从 retained snapshot 的 `project.session_ids` 与 `archived_sessions` 交集构造 `restored_sessions`；加载层会跳过底层目录缺失的 locator，因此该交集不能证明项目的完整归档子集。
- `restore_project_with_sessions` 只处理调用方提供的摘要列表，按摘要重新构造目录并 upsert session/project 行；遗漏的归档 locator 会继续留在归档状态，但项目可能已经恢复为活动状态。
- `workspace_session_index` 的 locator 已包含 session id、project id、project tag、archive 状态与 created_at；`projects` 行已保留完整项目元数据，归档和恢复不需要从 UI 摘要重建身份或覆盖现有记录。
- 修改范围首轮审核发现，精确日期归档/恢复在源目录缺失时返回 `Ok(None)`；上层随后仍会更新 index。这个语义无法区分“rename 已完成但索引尚未提交”和“活动/归档目录均已丢失”，会把真实损坏隐藏成成功状态。

### 已查明的执行流程

1. 单会话归档从 WebUI 精确 conversation id 解析 UUID；retained snapshot 只验证用户仍选择了一个可见会话，不再提供 project/date/目录身份。
2. `archive_single_session` 按 `session_id + archived = false` 查询唯一 SQLite locator；活动 locator 不存在时立即拒绝，不扫描目录，也不创建替代索引行。
3. locator 的 project tag、created_at 与 session id 构成唯一活动/归档目录身份；底层在目录阶段成功后才更新该 locator 的 archive flag。
4. 持久化归档成功后，Service Kernel 调用共享 `refresh_workbench_snapshot`，由 SQLite 与分段目录重建 sessions、messages、runtime state、archived conversations 与选中项。
5. 项目恢复先验证 project 行确实归档，再按 `project_id + archived = true` 查询完整 locator 集合；逐个恢复目录并更新 locator，二次查询确认归档子集为空后才把既有 project 行标为活动。
6. 精确日期归档/恢复现在对活动目录和归档目录执行真实目录检查，并处理完整四状态：仅源存在时正常移动，仅目标存在时续作中断提交，两边都不存在时报告损坏，两边同时存在时报告权威身份歧义。
7. 中断续作会在已发布目标目录重新提交 archived_at/updated_at 元数据，并按“目标父目录先耐久、源父目录后耐久”的现有协议补齐目录同步；完成后上层才提交 SQLite 状态。
8. 缺失、双目录、符号链接或非目录项都会在 index 更新前返回错误；重试只能沿同一 locator 恢复，不存在候选路径轮询或模糊字段兼容。

### 长期优化调整

- `archive_single_session` 移除 `SessionSummary` 与外部 project id 依赖，改为只接受 controller 与精确 session id，并强制查询活动 locator。
- 删除 `archive_single_session_for_project` 及按 project/summary 构造归档目录的旧 helper，单会话与项目批量归档统一进入 locator 驱动实现。
- 将 `restore_project_with_sessions` 替换为 `restore_project(controller, project_id)`；恢复范围完全由 SQLite 枚举，项目元数据通过更新既有 archive flag 保留，不再按 snapshot upsert。
- 删除按 `SessionSummary` 恢复目录的旧 helper，以及 Service Kernel 中 archived session 列表交集构造逻辑。
- 单会话归档成功后统一刷新权威工作台；删除约百行 retained state 手工归档投影器、归档 conversation 摘要构造器及仅服务该旧投影的测试。
- 精确日期归档/恢复返回类型由 `Result<Option<PathBuf>, String>` 收紧为 `Result<PathBuf, String>`；两边缺失不再是可忽略 no-op。
- 抽取通用 `segmented_session_directory_exists`，archive、restore 与 tombstone deletion 统一使用 `symlink_metadata`，拒绝符号链接和非目录生命周期参与项。
- 增加中断 archive/restore 续作、两边缺失拒绝、活动/归档双目录歧义拒绝三组回归，并更新既有生命周期与 session-boundary 测试以匹配非可选返回契约。

### 文件变更

- 修改：`crates/vulcan-service-kernel/src/project_conversation_services/conversation_services.rs`
- 修改：`crates/vulcan-service-kernel/src/project_directories.rs`
- 修改：`crates/vulcan-workspace-store/src/workspace_store/mod.rs`
- 修改：`crates/vulcan-workspace-store/src/workspace_store/persistence.rs`
- 修改：`crates/vulcan-workspace-store/src/workspace_store/persistence/session_lifecycle.rs`
- 修改：`crates/vulcan-workspace-store/src/workspace_store/segmented_session_store/session_management.rs`
- 修改：`crates/vulcan-workspace-store/src/workspace_store/segmented_session_store/tests/metadata_archive.rs`
- 修改：`crates/vulcan-workspace-store/src/workspace_store/session_load/tests.rs`
- 修改：`TASK_LOG.md`
- 新增：无
- 删除：无

### 修改范围代码审核与自动修复

- CodeKit 先定位单会话 archive 与项目 restore 的 Service Kernel、持久化、索引、分段目录调用点，确认旧 API 只有已追踪的生产消费者，可以直接移除错误签名而无需历史兼容层。
- 审核确认 `service_archive_conversation` 仍以 retained snapshot 做交互可见性校验，但 project tag、created_at 与目录身份全部来自 SQLite locator；snapshot 不再决定持久化路径。
- 审核确认 `restore_project` 的首次 locator 查询、逐目录恢复、逐索引提交、归档子集二次查询均发生在 project archive flag 更新之前；中途失败不会提前发布活动项目。
- 修改范围首轮审核识别出 `Ok(None)` 会把“两边均缺失”和“目录已移动”合并为同一结果；已自动改为四状态协议，并将缺失与双目录状态提升为显式错误。
- 审核确认仅目标目录存在的恢复分支会重新提交生命周期元数据和父目录同步；因此 directory rename 后、metadata/index 提交前的崩溃可以按原 locator 幂等续作。
- 最终源码搜索确认 `archive_single_session_for_project`、`restore_project_with_sessions`、summary 驱动的 archive/restore helpers、`archive_conversation_in_retained_state` 与 `archived_conversation_summary_from_session` 均为 0 个匹配。
- CodeKit 最终复查 4 个核心源码文件，共解析 45 个结构项，错误为 0。

### 验证结果

- archive/restore 定向回归：4 项全部通过，覆盖正常生命周期组合、中断移动续作、两边缺失拒绝与双目录歧义拒绝。
- `cargo test -p vulcan-workspace-store`：通过，2 个测试套件共 122 项全部通过；新增 3 项生命周期回归。
- `cargo test -p vulcan-service-kernel`：通过，2 个测试套件共 348 项全部通过；删除 1 项只服务旧手工归档投影的测试。
- `cargo test --workspace`：通过，63 个测试套件共 1544 项全部通过。
- `cargo clippy -p vulcan-workspace-store --all-targets --no-deps -- -D warnings`：通过，零告警。
- `cargo clippy -p vulcan-service-kernel --all-targets --no-deps -- -D warnings`：通过，零告警。
- `cargo clippy --workspace --all-targets -- -D warnings`：通过，全工作区零告警。
- 8 个直接修改 Rust 文件 `rustfmt --edition 2024 --check`：通过。
- 本轮文件范围 `git diff --check`：通过，未发现空白错误。

### 遗留问题与下一轮方向

- SQLite locator 更新、目录 rename/同步与 project archive flag 仍不属于同一跨介质事务；本轮补齐了确定性重试，但跨进程并发 archive/restore 仍需要持久化生命周期租约或状态机。
- WorkspaceStore 仍缺少可在测试内启动的 controller 数据平面 fixture；本轮 controller 查询与 project 行更新主要由全消费者编译、现有服务测试和底层四状态回归验证，后续应建设真实 SQLite/controller 集成支架。
- `rename_single_session_metadata` 仍接收调用方 project id 与 `SessionSummary`，并用摘要 created_at 定位目录；下一轮可继续追踪 rename 的真实生产链并收归 SQLite locator。
- 加载层仍会静默跳过底层目录缺失的 locator；本轮生命周期动作会显式拒绝此状态，但工作台尚未提供损坏 locator 的诊断或修复视图。
- 本轮只修改 Rust 后端生命周期持久化、服务投影与 Rust 测试，没有前端产物变化；第 23 轮前端生产构建结果仍有效。

## 第 44 轮：收归会话重命名的 locator 权威边界与提交确认（2026-07-10）

### 问题与事实依据

- WebUI `service_rename_conversation` 原先从 retained `WorkspaceSnapshot` 取得 `SessionSummary` 与 project id，再把两者传给 `rename_single_session_metadata`；因此 UI 摘要参与了持久化目录身份决策。
- 旧持久化函数克隆摘要、修改 title/title source/updated_at，再用调用方 project id 与摘要 created_at 构造活动目录；它没有查询 SQLite 中已经存在的 project tag、archive 状态与 created_at locator。
- 旧目录逻辑使用 `session_dir.exists()`；目录缺失时静默跳过 `metadata.json`，随后仍 upsert SQLite 并返回成功，能够制造“索引标题已更新、分段元数据未更新”的分裂状态。
- 重命名通过通用 `upsert_session_index_row` 提交时，会额外把 project id、project tag、archive flag、sort order 与 updated_at 一起覆盖；其中 sort order 固定传入 0，单纯改标题可能悄悄改变会话顺序和定位身份。
- Service Kernel 成功后通过 `rename_conversation_in_retained_state` 手工修改 workspace session 与 workbench conversation；共享 `refresh_workbench_snapshot` 已能从 SQLite 重建标题、标题来源、更新时间和相对时间标签，手工投影形成第二套协议。
- `boundary_store::rename_segmented_session_metadata` 通过 session id 扫描活动/归档目录，只存在定义与单元测试，没有生产调用；它会绕过 SQLite lifecycle 状态和 project/date locator。
- SQLite 加载路径明确以 `workspace_session_index.title/title_source/updated_at` 构造工作台摘要，而分段 `metadata.json` 保留同一展示字段供分段存储使用；两份文档必须按同一次重命名提交同步。

### 已查明的执行流程

1. RPC `service_rename_conversation` 接收 conversation id 与标题；Service Kernel 规范化非空标题、严格解析 UUID，并确认该 session 仍在 retained snapshot 的活动会话集合中。
2. retained snapshot 现在只承担交互可见性校验，不再提供 project id、created_at、title source 或目录路径。
3. `rename_single_session` 按 `session_id + archived = false` 查询唯一 SQLite locator；活动 locator 缺失、UUID 损坏、project id 缺失或 project tag 缺失都会在文件系统变更前显式失败。
4. 目录日期严格从 locator.created_at 解析，目录项目身份只使用 locator.project_tag；不再回退到 project id，也不从 `SessionSummary` 重建路径。
5. `rename_segmented_session_by_created_date` 构造唯一活动目录，通过 `symlink_metadata` 证明它是真实目录；目录缺失、普通文件或符号链接均拒绝，然后只修补 title、manual title source 与 updated_at。
6. 目录元数据提交成功后，`update_active_session_index_title` 只更新 SQLite 的 title、title_source 与 updated_at，不触碰 project、project tag、archive flag、sort order 或 created_at。
7. 标题 SQL 使用 `WHERE id = ? AND archived = 0 UPDATE ... RETURNING id`；必须返回唯一且匹配的 session id，否则把并发归档、删除、0 行更新或异常响应视为失败。
8. 持久化成功后 Service Kernel 调用 `refresh_workbench_snapshot`，从 SQLite 和分段存储重建权威工作台，不再手工修改 retained session/conversation 字段。

### 长期优化调整

- 将公共 API `rename_single_session_metadata(controller, project_id, summary, ...)` 替换为 `rename_single_session(controller, session_id, ...)`，彻底移除外部 project id 与 `SessionSummary` 参数。
- 增加精确日期目录重命名 helper，并复用生命周期真实目录校验；缺失目录不再是可忽略 no-op。
- 新增 `update_active_session_index_title` 专用 SQL，代替会覆盖无关 locator 字段的通用 upsert。
- 使用仓库现有且已验证的 SQLite `UPDATE ... RETURNING` 能力，为受保护标题更新增加唯一行提交确认。
- locator 解码新增 project id 与 project tag 非空校验；`PersistedSessionIndexLocator` 删除不参与文件系统寻址的 project id 字段，archive/restore/delete 同步移除 project-tag-to-project-id 历史 fallback。
- Service Kernel 重命名成功后统一执行权威工作台刷新；删除 `rename_conversation_in_retained_state` 与只为摘要提取 project id 的 controller helper。
- 删除无生产调用的扫描式 `rename_segmented_session_metadata` API，并把其旧测试迁移为精确 locator 测试。
- 新增精确重命名不改写 JSONL、缺失/非目录身份拒绝、locator 归属字段缺失拒绝、`UPDATE ... RETURNING` 唯一身份校验四类回归。

### 文件变更

- 修改：`crates/vulcan-service-kernel/src/project_conversation_services/conversation_services.rs`
- 修改：`crates/vulcan-workspace-store/src/workspace_store/mod.rs`
- 修改：`crates/vulcan-workspace-store/src/workspace_store/persistence.rs`
- 修改：`crates/vulcan-workspace-store/src/workspace_store/persistence/index.rs`
- 修改：`crates/vulcan-workspace-store/src/workspace_store/persistence/session_lifecycle.rs`
- 修改：`crates/vulcan-workspace-store/src/workspace_store/boundary_store.rs`
- 修改：`crates/vulcan-workspace-store/src/workspace_store/segmented_session_store.rs`
- 修改：`crates/vulcan-workspace-store/src/workspace_store/segmented_session_store/session_management.rs`
- 修改：`crates/vulcan-workspace-store/src/workspace_store/segmented_session_store/tests/metadata_archive.rs`
- 修改：`crates/vulcan-workspace-store/src/workspace_store/tests.rs`
- 修改：`TASK_LOG.md`
- 新增：无
- 删除：无

### 修改范围代码审核与自动修复

- CodeKit 先追踪 `service_rename_conversation → rename_single_session_metadata → metadata patch + index upsert → retained projection`，并核实工作台加载最终使用 SQLite 标题字段，而非 `metadata.json` 标题覆盖索引。
- 审核确认旧 API 只有一个生产调用点，扫描式 boundary API 只有一个测试调用点；可以直接删除错误签名和扫描入口，无需历史兼容包装。
- 首轮严格 Clippy 发现扫描 API 删除后遗留的元数据补丁导入，以及 locator 中已不再使用的 project id 字段；已自动删除残留，没有添加 lint allow。
- 修改范围审核进一步发现普通 SQLite execute 无法证明 `WHERE archived = 0` 实际命中；已自动改成 `UPDATE ... RETURNING id`，并增加 0 行、匹配行与异常身份的纯逻辑验证。
- 审核确认目录元数据先于 SQLite 标题提交：索引提交失败时，旧 locator 仍可驱动相同精确目录的幂等重试；不会先让工作台展示一个尚未写入分段元数据的标题。
- 审核确认专用 SQL 更新列集合只有 title、title_source、updated_at；旧 upsert 对 project tag、archive flag、sort order 与 created_at 的覆盖风险已经消失。
- 最终结构化搜索确认 `rename_single_session_metadata`、`rename_segmented_session_metadata`、`rename_conversation_in_retained_state` 与 `controller_runtime_and_visible_session_project_from_state` 均为 0 个匹配。
- CodeKit 最终复查 4 个核心源码文件，共解析 51 个结构项，错误为 0。

### 验证结果

- 重命名目录定向回归：2 项全部通过，覆盖元数据更新且 JSONL 字节保持不变，以及缺失/非目录活动身份拒绝。
- locator 归属字段定向回归：1 项通过，覆盖 project id 与 project tag 缺失的显式拒绝。
- guarded title update 定向回归：1 项通过，覆盖唯一匹配、0 行状态变化与异常返回身份。
- `cargo test -p vulcan-workspace-store`：通过，2 个测试套件共 125 项全部通过。
- `cargo test -p vulcan-service-kernel`：通过，2 个测试套件共 348 项全部通过。
- `cargo test --workspace`：通过，63 个测试套件共 1547 项全部通过。
- `cargo clippy -p vulcan-workspace-store --all-targets --no-deps -- -D warnings`：通过，零告警。
- `cargo clippy -p vulcan-service-kernel --all-targets --no-deps -- -D warnings`：通过，零告警。
- `cargo clippy --workspace --all-targets -- -D warnings`：通过，全工作区零告警。
- 10 个直接修改 Rust 文件 `rustfmt --edition 2024 --check`：通过。
- 本轮文件范围 `git diff --check`：通过，未发现空白错误。

### 遗留问题与下一轮方向

- 分段元数据文件与 SQLite 标题更新仍不是跨介质原子事务；本轮通过精确 locator、提交顺序与幂等重试消除静默成功，但并发 rename/archive 的完全串行化仍需要持久化 lifecycle lease 或 mutation journal。
- WorkspaceStore 尚无可启动真实 Controller SQLite binding 的测试 fixture；`UPDATE ... RETURNING` 已有 goal store 生产先例，本轮通过全消费者编译与返回行纯逻辑回归验证，后续仍应补 controller 数据平面集成测试。
- `load_workspace_snapshot_with_runtime_state` 仍用非严格时间戳转换定位目录，并对 `!session_dir.exists()` 直接 `continue`；损坏 locator 会从工作台静默消失，下一轮应优先追踪并收紧加载诊断边界。
- runtime checkpoint、transcript suffix 与 runtime log 等若干路径仍按 session id 扫描目录；需要逐条确认其调用方是否已有 SQLite locator，不能在未查清事实来源前批量替换。
- 持久化重命名成功但随后工作台刷新失败时，RPC 会报告错误而标题事实上已经提交；未来可通过明确的提交回执与独立刷新状态区分持久化失败和投影刷新失败。
- 本轮只修改 Rust 后端持久化、服务投影与 Rust 测试，没有前端产物变化；第 23 轮前端生产构建结果仍有效。

## 第 45 轮：消除工作区加载的静默会话丢失与时间身份兜底（2026-07-10）

### 问题与事实依据

- `load_workspace_snapshot_with_runtime_state` 是 Service Bootstrap、工作台刷新、会话提交加载与历史水合共同使用的索引加载核心；它的任何容错都会直接决定 WebUI 能否看见持久化会话。
- SQLite 查询返回 session id、project id、project tag、title、archive flag、created_at 与 updated_at 原始标量；旧加载流程在这些字段尚未验证时就开始构造目录和工作台投影。
- project id 为空时旧代码回退到 project tag，project tag 为空时又回退到 project id；两者均不可靠时仍会继续构造路径，违反唯一归属原则。
- archive flag 只判断 `!= 0`，损坏值 2、负数等会被无条件当成归档状态；缺失 project 行时又用 project id 伪造项目名称及项目摘要。
- created_at 通过宽松转换器解析，空白或非数字使用 `unwrap_or_default()` 落到 1970-01-01；updated_at 无效时，相对时间标签回退当前时间，排序值则回退 0，同一损坏字段产生两种替代事实。
- 旧加载流程构造 expected session directory 后执行 `if !session_dir.exists() { continue; }`；目录缺失 locator 会从项目成员、会话列表、归档列表和工作台中完全消失，调用方得不到任何诊断。
- expected 目录缺失并非单一语义：opposite-only 表示 archive/restore 已移动目录但索引未提交，both-missing 可能是删除残留或数据丢失，both-existing 则存在权威副本歧义；不能用同一个 `continue` 或自动索引改写处理。
- 旧降序排序通过对 updated_at 执行一元负号实现；严格解析允许完整 i64 范围后，`i64::MIN` 取负会溢出。

### 已查明的执行流程

1. Service Bootstrap、refresh、submission 或 history hydration 调用 `load_workspace_index_snapshot`，进入共享 `load_workspace_snapshot_with_runtime_state`。
2. Loader 先查询 project rows 和 session rows；session row 原先直接进行 project fallback、archive 布尔转换、日期构造与目录 `exists` 判断。
3. 新流程先把每条原始行交给 `validate_session_index_row`：严格解析 UUID，要求 project id/project tag 非空，只接受 archive 0/1，并分别严格解析 created_at 与 updated_at。
4. created_at 通过统一 `session_directory_date_from_unix_millis_text` 生成唯一 UTC 日期；空白、非数字或 i64 溢出直接返回包含 session id 的损坏诊断，不再生成 1970 路径。
5. Loader 要求 locator 引用的 project row 真实存在；活动 session 若仍属于 archived project，则报告项目恢复状态未提交，不再同时伪造活动/归档项目投影。
6. `required_session_directory_for_index_row` 同时构造 expected 与 opposite 生命周期路径，并通过 `symlink_metadata` 验证真实目录类型。
7. 只有 expected-only 状态进入 runtime state、addon state 与 session summary 加载；opposite-only 报告未提交生命周期转换，both-missing 报告双树缺失，both-existing 报告权威副本歧义，链接或普通文件也显式失败。
8. updated_at 只解析一次，解析值同时用于相对时间标签与排序；降序排序使用 `Reverse<i64>`，不再对有符号极值取负。
9. 加载成功后才将 session 加入 project、visible/archived session、runtime state 与 run phase 投影，因此任何损坏行都不会被部分投影后继续运行。

### 长期优化调整

- 新增内部 `ValidatedSessionIndexRow`，把原始 SQLite 标量与可用于寻址/排序的验证后身份分层。
- 删除 loader 中 project id/project tag 双向 fallback、archive `!= 0` 宽松判定、updated_at 两套 fallback 和缺失目录 `continue`。
- 增加 project 引用存在性与“活动 session + archived project”状态校验，停止伪造项目摘要。
- 新增四状态目录解析器，统一检查 expected/opposite 两棵树及真实目录类型，并输出包含 session id 和两侧路径的诊断。
- 将 `segmented_session_directory_exists` 提升为 workspace-store 内共享的真实目录检查，加载与生命周期变更使用同一链接/非目录拒绝规则。
- 把日期转换、Unix 毫秒解析与 Gregorian 日期算法集中到 `serialization.rs`；删除 boundary store 与 persistence index 中两套重复实现。
- boundary session 创建、fork、locator 初始化、rename/archive/restore/delete 与 workspace load 全部改用同一严格日期转换器。
- 可选 task-store locator 保持 `Option` 契约，但无效 created_at 现在返回 `None`，不再错误命中 1970-01-01 目录。
- 抽取 `descending_session_updated_at_key` 并使用 `Reverse<i64>`，覆盖 `i64::MIN..=i64::MAX` 完整范围。

### 文件变更

- 修改：`crates/vulcan-workspace-store/src/workspace_store/serialization.rs`
- 修改：`crates/vulcan-workspace-store/src/workspace_store/boundary_store.rs`
- 修改：`crates/vulcan-workspace-store/src/workspace_store/mod.rs`
- 修改：`crates/vulcan-workspace-store/src/workspace_store/loading.rs`
- 修改：`crates/vulcan-workspace-store/src/workspace_store/persistence/index.rs`
- 修改：`crates/vulcan-workspace-store/src/workspace_store/persistence/edit_fork.rs`
- 修改：`crates/vulcan-workspace-store/src/workspace_store/persistence/locators.rs`
- 修改：`crates/vulcan-workspace-store/src/workspace_store/persistence/session_lifecycle.rs`
- 修改：`crates/vulcan-workspace-store/src/workspace_store/persistence/tests.rs`
- 修改：`crates/vulcan-workspace-store/src/workspace_store/segmented_session_store.rs`
- 修改：`crates/vulcan-workspace-store/src/workspace_store/segmented_session_store/session_management.rs`
- 修改：`crates/vulcan-workspace-store/src/workspace_store/segmented_session_store/tests/session_lifecycle.rs`
- 修改：`TASK_LOG.md`
- 新增：无
- 删除：无

### 修改范围代码审核与自动修复

- CodeKit 先确认共享 loader 的四类生产消费者，再沿 `query_session_index_rows → raw row decode → path construction → runtime/summary projection` 查明静默过滤的真实影响范围。
- 审核确认 opposite-only、both-missing 与 both-existing 的业务含义不同；在没有持久化问题列表、恢复租约或明确删除意图的情况下，没有足够证据允许加载期自动改写 index，因此采用显式诊断而非投机修复。
- 首轮严格 Clippy 只发现测试错误分支需要 `ValidatedSessionIndexRow: Debug`；已补标准派生，没有改写断言或添加 lint allow。
- 修改范围复查发现严格 updated_at 接受 `i64::MIN` 后，旧负号降序键会溢出；已自动改为 `Reverse<i64>` 并增加完整有符号范围回归。
- 审核确认 session row 在 project 引用、目录状态与时间戳全部验证完成前不会写入任何工作台集合；不存在“部分投影后继续”的新路径。
- 最终结构化搜索确认旧宽松日期函数、persistence 专用重复日期函数、相对时间文本 fallback、loader `session_dir.exists()` 与 `row.updated_at.parse().unwrap_or_default()` 均为 0 个匹配。
- CodeKit 最终复查 6 个核心源码文件，共解析 102 个结构项，错误为 0。

### 验证结果

- session row 严格解码回归：1 项通过，覆盖 project id/tag 缺失、archive flag 非 0/1、created_at 与 updated_at 损坏。
- session directory 四状态回归：1 项通过，覆盖 expected-only、opposite-only、both-missing、both-existing 与普通文件占位。
- 统一日期转换回归：2 项通过，覆盖 Unix 日期边界、两侧空白与无效输入拒绝。
- task-store locator 回归：1 项通过，证明无效时间戳不再命中 1970 目录。
- 完整 i64 降序排序回归：1 项通过，覆盖 `i64::MAX` 与 `i64::MIN`。
- `cargo test -p vulcan-workspace-store`：通过，2 个测试套件共 128 项全部通过。
- `cargo test -p vulcan-service-kernel`：通过，2 个测试套件共 348 项全部通过。
- `cargo test --workspace`：通过，63 个测试套件共 1550 项全部通过。
- `cargo clippy -p vulcan-workspace-store --all-targets --no-deps -- -D warnings`：通过，零告警。
- `cargo clippy --workspace --all-targets -- -D warnings`：通过，全工作区零告警。
- 12 个直接修改 Rust 文件 `rustfmt --edition 2024 --check`：通过。
- 本轮文件范围 `git diff --check`：通过，未发现空白错误。

### 遗留问题与下一轮方向

- 当前严格错误会阻止整个 workspace snapshot 完成，正确性优先但可用性有限；若要隔离单条损坏 locator 并继续启动，需要新增持久化健康问题模型、Service API 与 WebUI 修复入口，属于后续需确认的架构决策。
- opposite-only 状态现在能被精确诊断，但仍需用户重新触发生命周期动作或外部修复；长期应设计带状态机/租约的恢复命令，而不是让 loader 在竞态下自动改写 archive flag。
- both-missing 状态仍无法区分“墓碑删除已完成但 index 未提交”和真实数据丢失；后续恢复工具应同时检查确定性 tombstone 与删除意图记录。
- project row 本身仍把任意非零 archived 值视为归档、把负 sort order 钳制到 0；下一轮可继续追踪 project 加载、默认项目播种与排序投影的严格协议。
- title/title_source 仍按可空 Schema 采用 `Untitled`/manual fallback；是否改为强制字段需要先核实所有新建、迁移和 seed 写入路径。
- 可选 task-store locator 仍以 `Option` 隐去无效 created_at 诊断；若要彻底显式化，需要沿 Service Kernel 同步投影调用链调整返回类型，适合作为独立后续任务。
- 本轮只修改 Rust 后端工作区存储、加载与测试，没有前端产物变化；第 23 轮前端生产构建结果仍有效。

## 第 46 轮：严格化项目索引加载与默认项目播种身份（2026-07-10）

### 问题与事实依据

- `query_project_rows` 会返回 projects 表中的全部活动及归档行，但旧注释称其只查询“可见项目”；共享 loader、默认项目保证入口和 session 归属验证都依赖这组原始行。
- `load_workspace_snapshot_with_runtime_state` 在校验 project row 之前先执行 `project_rows.iter().all(|row| row.archived != 0)`；损坏的 archived=2、负数等会先触发默认项目写入，随后才可能在其他路径暴露问题。
- project 投影把任意非零 pinned/archive 转成 true，把负 sort order 通过 `max(0)` 钳制成 0；不同损坏行可能被投影成相同分区顺序，SQLite 原始事实被悄悄替换。
- project id、name 与 root_path 都是 Schema 的非空文本，但旧加载未拒绝空白内容；相对 root_path 也会直接进入工作台，后续文件系统操作会依赖进程 cwd。
- projects Schema 只有 `NOT NULL`，没有 pinned/archive 0/1、sort_order 非负或 root_path 绝对路径约束；因此读取边界必须在这些标量触发工作台决策前显式验证。
- `ensure_default_workspace_project` 与完整 loader 使用两套原始行判断；播种成功后都没有重新查询并证明活动项目实际存在。
- 默认项目播种在创建逻辑路径 `<home>/.vulcan/projects/vulcan_project` 后，把 canonicalized 实路径写入 project.path 和稳定 ID；`is_default_workspace_project` 却按逻辑路径比较。home 经过 junction/symlink 时，唯一默认项目可能失去移除保护。
- 默认项目写入通过 `persist_new_project → upsert_projects`；同 ID 的 archived 默认项目会被明确重新激活，因此“没有任何活动项目时恢复默认项目”是现有确定语义，不需要构造新兼容分支。

### 已查明的执行流程

1. Service Bootstrap 或工作台刷新进入共享 loader；loader 首先查询 projects 表，SQL 按 pinned、sort order 与 updated_at 返回活动及归档行。
2. 新 `query_validated_project_rows` 在任何默认播种判断或投影前逐行调用 `validate_project_row`。
3. project id、name、root_path 必须非空白，root_path 必须是当前宿主上的绝对路径；pinned/archive 只接受 0/1，sort order 必须能无损转换为非负 usize。
4. 只有全部 project rows 验证成功后，`workspace_requires_default_project` 才判断是否不存在活动行；空表与全归档表返回 true，任意活动行返回 false。
5. 需要播种时，loader 创建逻辑默认目录，并要求 `fs::canonicalize` 成功以证明路径真实可解析；持久化 path 与 ID仍使用逻辑默认路径，保证身份不随 junction/symlink 实路径变化。
6. `persist_new_project` 插入或重新激活稳定默认 ID 后，loader 重新查询并严格验证全部 project rows；若仍无活动项目则显式失败。
7. `ensure_default_workspace_project` 复用同一验证、判断、播种与二次确认流程，不再形成独立宽松协议。
8. 工作台 `TaskProjectSummary` 只使用验证后的 pinned/archive/order，session locator 查找项目归属时也只查询 `ValidatedProjectRow`。

### 长期优化调整

- 新增内部 `ValidatedProjectRow`，把原始 SQLite 展示字段与可用于决策的严格 pinned/archive/order 分层。
- 新增 `validate_project_row`，拒绝空白 id/name/root path、相对 root path、非 0/1 标记及负数/平台溢出顺序。
- 新增 `query_validated_project_rows`，让完整 workspace load 与 default-project ensure 共享唯一项目读取边界。
- 新增 `workspace_requires_default_project`，默认播种只消费已验证 archive 状态，不再直接解释原始 i64。
- 删除 project 投影中的 `pinned != 0`、`archived != 0` 与 `sort_order.max(0)` 强制转换。
- 默认项目播种与保证入口在写入后重新查询；未真正发布活动项目时不再返回假成功。
- canonicalize 从身份来源调整为存在性证明；默认项目 path、稳定 ID 与删除保护统一使用逻辑默认路径。
- 修正 project query 文档，明确返回活动与归档全量行，避免后续调用方误把结果理解为可见子集。

### 文件变更

- 修改：`crates/vulcan-workspace-store/src/workspace_store/loading.rs`
- 修改：`crates/vulcan-workspace-store/src/workspace_store/queries.rs`
- 修改：`TASK_LOG.md`
- 新增：无
- 删除：无

### 修改范围代码审核与自动修复

- CodeKit 追踪 `query_project_rows → default seed decision → TaskProjectSummary projection → session project lookup`，并核实 Service Bootstrap、refresh 与 default ensure 都会经过该边界。
- 审核确认 projects 表没有 CHECK 约束，本轮没有擅自执行数据库 Schema 变更；严格协议集中在读取边界，后续若增加 CHECK 与数据迁移需按重大决策单独确认。
- 审核确认默认播种前所有行已完成验证；损坏 archive flag 不会再触发任何目录创建或 SQLite upsert。
- 审核确认播种后的二次查询同样经过严格验证，并显式证明至少一个活动行存在。
- 修改范围复查发现 canonicalized 实路径与逻辑默认路径会导致默认身份漂移；已自动改为“canonicalize 仅验证、逻辑路径负责持久化身份”。
- 审核确认 session project lookup 使用验证后 project row；活动 session 与归档 project 的未提交恢复状态仍会被第 45 轮规则显式拒绝。
- 最终结构化搜索确认 loader 中 `row.archived != 0`、`row.pinned != 0`、`sort_order.max(0)`、canonicalize fallback 与直接 `query_project_rows(&controller)` 均为 0 个匹配。
- CodeKit 最终复查 5 个核心源码文件，共解析 94 个结构项，错误为 0。

### 验证结果

- project row 严格解码回归：1 项通过，覆盖空白 id/name/root、相对路径、无效 pinned/archive 与负顺序。
- 默认项目需求判定回归：1 项通过，覆盖空表、全归档和存在活动项目三种状态。
- 默认项目路径、摘要、稳定 ID 与分类回归：3 项全部通过。
- `cargo test -p vulcan-workspace-store`：通过，2 个测试套件共 130 项全部通过。
- `cargo test -p vulcan-service-kernel`：通过，2 个测试套件共 348 项全部通过。
- `cargo test --workspace`：通过，63 个测试套件共 1552 项全部通过。
- `cargo clippy -p vulcan-workspace-store --all-targets --no-deps -- -D warnings`：通过，零告警。
- `cargo clippy --workspace --all-targets -- -D warnings`：通过，全工作区零告警。
- 2 个直接修改 Rust 文件 `rustfmt --edition 2024 --check`：通过。
- 本轮文件范围 `git diff --check`：通过，未发现空白错误。

### 遗留问题与下一轮方向

- projects 表仍没有 pinned/archive 0/1、sort_order 非负与绝对路径 CHECK；增加约束需要处理既有数据并修改 Schema，属于需确认的数据库重大决策。
- Service Kernel 的置顶与重排服务仍直接执行 SQL，而 WorkspaceStore 中存在未接入生产调用的同类 API；两套写入协议会继续漂移，下一轮应追踪并统一到受保护的持久化入口。
- project archive、delete、rename、pin 与 reorder 多数使用不返回受影响行数的 execute API；目标行缺失或状态已变化时仍可能返回假成功，可复用第 44 轮 `UPDATE ... RETURNING` 提交确认模式。
- project section 内重复 sort order 尚未被拒绝或正规化；需要先确认 pinned/regular 分区重排是否保证提交完整成员集合。
- `default_workspace_project_id` 使用标准库 `DefaultHasher`，其算法不属于稳定持久化协议保证；替换 ID 算法涉及现有默认 project/session 引用迁移，需要独立方案与确认。
- Service Kernel 的 `normalize_task_project_path` 仍在 canonicalize 失败时回退原路径，且项目新增顺序使用 `order + 1`，存在路径不确定与 usize 溢出风险，适合后续独立治理。
- 本轮只修改 Rust 后端项目加载、默认播种与测试，没有前端产物变化；第 23 轮前端生产构建结果仍有效。

## 第 47 轮：统一项目布局原子写入并消除零行更新假成功（2026-07-10）

### 问题与事实依据

- WebUI `ProjectSidebar` 拖拽时使用 `arrayMove(sectionIds, ...)`，提交的是一个 pinned 或 regular 分区的完整身份顺序；Service Kernel 旧实现只验证请求中的项目属于同一分区，没有证明请求未遗漏该分区其他项目。
- 项目加载 SQL 按 `pinned DESC, sort_order ASC, updated_at DESC` 排序；旧置顶服务只翻转 pinned，不重新分配两个分区的 sort order，项目跨分区后会携带旧分区顺序并产生重复排序号。
- `service_toggle_project_pinned` 与 `service_reorder_projects` 在 Service Kernel 内直接执行 projects SQL；WorkspaceStore 同时存在未被生产调用的 `toggle_project_pinned`、`persist_project_order_update` 与三段式 `persist_project_pinned_and_order_update`，形成两套会漂移的写入协议。
- WorkspaceStore 旧三段式置顶写入先更新 pinned，再分别重排 pinned/regular；任一步失败都会留下部分提交，且三个 execute 调用都无法证明目标行实际命中。
- 旧排序把 `usize` 通过 `index as i64` 写入 SQLite；平台范围超出时会静默截断，不符合第 46 轮建立的严格顺序协议。
- 重命名、图标设置/清除、archive flag 更新与活动项目删除都只检查 execute 调用是否报错；目标项目已归档、删除或并发改变状态时，SQLite 零行变更仍会向上返回成功。
- 图标 SQL 也直接位于 Service Kernel，字段不变量与持久化提交确认无法由 WorkspaceStore 统一维护。
- 项目新增服务用保留 `TaskProjectSummary.order` 的最大值计算下一顺序；若置顶后只修改内存 pinned 而不刷新全部 order，随后新增项目会基于陈旧顺序生成错误编号。

### 已查明的执行流程

1. WebUI 先应用置顶或拖拽乐观预览，再通过 RPC 调用 `service_toggle_project_pinned` 或 `service_reorder_projects`；失败时回滚/刷新，成功时沿用结果。
2. Service Kernel 现在从同一份保留 workspace snapshot 抽取活动项目的最小 `{id, pinned}` 布局，避免分别读取工作台投影与运行时造成跨快照拼接。
3. 置顶路径按当前展示顺序只改变目标项目的结果分区，生成完整 pinned/regular 两个列表；拖拽路径要求请求集合与所选分区的完整保留成员集合严格相等，同时保留另一分区顺序。
4. WorkspaceStore 把两个完整分区转换为每个活动项目的 `id / expected_pinned / next_pinned / sort_order`；置顶时只有目标项目的 expected 与 next 相反，普通重排时所有项目的 expected 与 next 相同。
5. 单条 SQLite 语句用 `requested` CTE 承载类型化请求，用 materialized `current_active` 固定语句开始时的活动身份与置顶状态。
6. `layout_guard` 同时要求活动项目总数等于请求总数，并要求每个活动 id 的当前 pinned 精确匹配 expected；任一新增、删除、归档或并发置顶变化都会让整条 UPDATE 更新零行，不会部分提交。
7. 保护通过后，同一 UPDATE 一次写入每个活动项目的结果 pinned、分区内零基 sort order 与共享 updated_at，并通过 `RETURNING id, pinned, sort_order` 返回完整结果。
8. 返回行按身份逐项验证：数量、唯一性、合法 0/1 pinned 与精确 sort order 必须全部匹配请求；不能再把“SQL 没报错”等同于“布局提交成功”。
9. 置顶成功后重新加载权威 workspace/workbench snapshot，使 pinned、数组顺序与 `TaskProjectSummary.order` 同步；拖拽仍返回刷新后的完整快照。
10. 重命名、图标更新、archive 状态转换与活动项目删除改用 DML `RETURNING id`；archive 转换还要求精确 expected_archived，错误生命周期状态不再被无条件覆盖。

### 长期优化调整

- 新增内部完整布局转换模型与唯一执行器，置顶和同分区拖拽共享同一条“全量活动集合 + 预期状态 + 原子结果”协议。
- 删除未使用的 WorkspaceStore 单行 toggle API，并让 Service Kernel 完全移除 projects 表直接 SQL；项目写入职责回到 WorkspaceStore。
- 将拖拽契约从“请求里看起来同分区”提升为“请求必须等于该分区完整成员集合”，从源头阻止遗漏项目后产生重复 sort order。
- 所有布局 sort order 与动态参数索引都使用 checked arithmetic/`i64::try_from`，不再使用截断转换。
- 新增 `persist_project_icon_update`，强制 icon pack/key 同时存在或同时清除、拒绝空白，并使用 `RETURNING` 确认精确活动项目。
- 重命名、archive flag 转换与活动项目删除全部改为受保护 DML 返回确认；archive/restore 分别要求 false→true 与 true→false。
- Service Kernel 的置顶成功路径删除局部 pinned 翻转辅助函数，改为刷新权威快照，确保后续新增项目读取到重新编号后的 order。
- 更新 WorkspaceStore 公共导出，移除旧 toggle 入口并公开统一图标写入入口；没有保留旧调用兼容层。

### 文件变更

- 修改：`crates/vulcan-workspace-store/src/workspace_store/persistence/projects.rs`
- 修改：`crates/vulcan-workspace-store/src/workspace_store/persistence/session_lifecycle.rs`
- 修改：`crates/vulcan-workspace-store/src/workspace_store/mod.rs`
- 修改：`crates/vulcan-service-kernel/src/project_conversation_services/project_services.rs`
- 修改：`crates/vulcan-service-kernel/src/service_state_access.rs`
- 修改：`crates/vulcan-service-kernel/src/lib.rs`
- 修改：`TASK_LOG.md`
- 新增：无
- 删除：无

### 修改范围代码审核与自动修复

- CodeKit 从 WebUI `ProjectSidebar → project-actions-service → RPC dispatch → Service Kernel → WorkspaceStore → projects 查询排序` 追踪完整执行链，确认前端确实提交完整分区列表，而旧后端没有执行完整性证明。
- 审核确认旧 `persist_project_pinned_and_order_update` 是三个独立 execute 调用且没有生产调用点；本轮直接替换为单语句全布局协议，没有继续包装旧三段式实现。
- 使用真实 SQLite 3.51.1 执行新 CTE：三条 expected state 全匹配时三行同时返回并提交；故意让一条 expected pinned 不匹配时 RETURNING 为空，三条原始数据与 updated_at 全部保持不变。
- 首轮严格 Clippy 发现一个可折叠嵌套条件；已按语言原生 let-chain 修复，没有添加 lint allow。
- 修改范围复查发现置顶成功后若只更新本地 pinned，`TaskProjectSummary.order` 会保持旧值并污染紧随其后的项目新增顺序；已自动删除局部置顶更新捷径，改为刷新权威完整快照。
- 审核确认布局返回验证按 id 建立预期映射，不依赖 SQLite RETURNING 行顺序，并拒绝缺行、多行、未知 id、重复 id、非法 pinned 与错误 sort order。
- 结构化搜索确认 Service Kernel 中 projects UPDATE/DELETE SQL、`ControllerSqliteValue` 与 `update_project_pinned_local_state` 均为 0 个匹配；置顶、重排和图标写入分别只有一个 WorkspaceStore 生产调用点。
- CodeKit 最终复查 6 个直接修改 Rust 文件，共解析 103 个结构项，错误为 0。

### 验证结果

- 单行项目变更身份回归：1 项通过，覆盖精确命中、零行返回与意外身份。
- WorkspaceStore 项目布局回归：3 项通过，覆盖完整转换、重复/错分区/缺失目标、返回乱序、缺行、非法 pinned、错误 sort order 与 SQL guard 结构。
- 图标对不变量回归：1 项通过，覆盖规范化、完整清除、半对输入与空白值拒绝。
- Service Kernel 布局回归：2 项通过，覆盖置顶跨分区顺序、完整同分区重排、局部集合、跨分区与重复身份拒绝。
- 真实 SQLite 成功与 guard-failure 两条执行验证：全部通过，失败分支确认零行部分写入。
- `cargo test -p vulcan-workspace-store`：通过，2 个测试套件共 135 项全部通过。
- `cargo test -p vulcan-service-kernel`：通过，2 个测试套件共 349 项全部通过。
- `cargo test --workspace`：通过，63 个测试套件共 1558 项全部通过。
- `cargo clippy -p vulcan-workspace-store -p vulcan-service-kernel --all-targets --no-deps -- -D warnings`：通过，零告警。
- `cargo clippy --workspace --all-targets -- -D warnings`：通过，全工作区零告警。
- 6 个直接修改 Rust 文件 `rustfmt --edition 2024 --check`：通过。
- 本轮 Rust 文件范围 `git diff --check`：通过，未发现空白错误。

### 遗留问题与下一轮方向

- `permanently_delete_archived_project` 仍使用 `archived != 0` 与无返回行的 DELETE，并且先删除归档会话目录、后删除 project row；下一轮应追踪永久删除的真实调用入口、文件/索引提交顺序与假成功风险。
- `archive_missing_projects`、session index archive/delete 等内部写入仍使用 execute API；需要按每个调用方的幂等语义分别判断哪些必须改为 `RETURNING`，不能机械替换。
- Service Kernel 的图标本地投影辅助仍以“至少命中一份投影”为成功条件；workspace/workbench 两份投影缺一时可能掩盖状态分裂，适合下一轮独立收紧。
- 多个“数据库已提交后刷新快照”的服务在 refresh 失败时会向客户端返回错误，而持久化变更已生效；长期应设计带提交结果与权威 revision 的 mutation response，而不是让客户端把后置刷新错误解释为提交失败。
- projects 表仍没有 pinned/archive 0/1、sort_order 非负与绝对路径 CHECK；增加约束与既有数据迁移属于需用户确认的数据库重大决策。
- `default_workspace_project_id` 的 `DefaultHasher` 稳定性、`normalize_task_project_path` 的 canonicalize fallback 与项目新增 `order + 1` 溢出仍未处理，后续继续按独立事实链治理。
- 本轮未修改前端；现有前端完整分区提交与乐观预览契约保持不变，第 23 轮前端生产构建结果仍有效。

## 第 48 轮：删除不可达永久删除协议与孤立墓碑实现（2026-07-10）

### 问题与事实依据

- `permanently_delete_archived_session` 与 `permanently_delete_archived_project` 虽由 WorkspaceStore 公开导出，但全仓库没有任何生产调用；Service Kernel、RPC dispatch、WebUI service 与组件层都不存在永久删除项目或会话入口。
- 两个公开函数只在 `persistence/deletion.rs` 内组成自包含孤立链；Rust 函数不存在动态字符串调用，因此仓库内调用点为零可以确定其当前不可达。
- 永久删除会话先按 archived locator 永久移除分段会话目录，再使用无返回行的 execute 删除 session index；进程在两步之间失败会留下 SQLite locator 指向缺失目录，第 45 轮严格 loader 会在下次启动显式失败。
- 底层 tombstone 只能保证目录从 catalog tree 移出后可续删，但代码会在提交 SQLite index 删除之前彻底清除 tombstone；文件系统与 SQLite 之间没有持久化删除意图或可恢复提交点。
- 永久删除项目先验证 archived project、拒绝活动子会话，再逐个执行“删目录→删 locator”；中途失败会形成部分子会话永久删除，最后 project row 仍存在。
- 子会话全部处理后，项目删除使用 `DELETE ... WHERE archived != 0` 和无返回行 execute；它既把非法 archive 标记当成归档，也无法证明 project row 仍然存在。
- `delete_session_index_row`、locator 目录删除包装、date-based tombstone 删除、删除目录同步函数与 `.segmented_session_deletions` 常量都只服务这条不可达链；删除专用测试是在验证没有产品入口的实现细节。
- 若要把永久删除真正做成产品能力，需要 SQLite 删除意图、文件墓碑阶段、恢复扫描、幂等重试、提交确认、Service API 与用户确认界面，属于新的跨资源生命周期设计；继续修补当前死 API 只会扩大无消费者复杂度。

### 已查明的执行流程

1. 旧单会话入口查询 `workspace_session_index` 中 archived=true 的精确 locator，不存在时拒绝。
2. locator 包含 project tag、created_at 与 archive 状态；包装函数据此定位 archived session directory。
3. date-based delete 将规范目录移动到 data root 下的确定性 tombstone，执行目录 move 同步，然后递归删除 tombstone 并同步父目录。
4. 目录已不可恢复后，`delete_session_index_row` 才执行无返回行的 SQLite DELETE；文件删除与索引删除之间没有事务桥接。
5. 旧项目入口先查询 archived project，再查询并拒绝活动子 locator，随后查询全部归档子 locator。
6. 项目入口对每个归档子 locator 串行复用上述会话永久删除；任何后续子项失败都不会恢复先前已删除子项。
7. 循环完成后才用宽松 `archived != 0` 删除 project row，零行删除仍返回成功。
8. CodeKit 与全仓库结构化搜索确认上述两个入口没有上层消费者；本轮因此从公共导出向下删除整条链，而不是为不可达路径设计新状态机。
9. 可达的 archive/restore 仍使用 active/archived 双树移动、元数据修补、目录同步与 SQLite archive flag 转换；这些路径及其回归测试不依赖永久删除墓碑。

### 长期优化调整

- 删除整个 `persistence/deletion.rs` 模块以及两个永久删除公共 API，不保留兼容包装。
- 从 WorkspaceStore 与 persistence 公共导出中移除永久删除符号，缩小真实能力表面。
- 删除只被永久删除链调用的 session index DELETE、locator 目录删除包装与相关 wildcard 私有导入。
- 删除 `delete_segmented_session_by_created_date`、确定性删除 tombstone 路径函数、删除目录同步函数及专用目录常量。
- 删除两项纯永久删除 tombstone 回归；保留原组合测试中的 archive/restore 部分，并重命名为 `archive_and_restore_by_created_date`。
- 更新 `segmented_session_directory_exists` 文档，只描述当前真实消费者使用的 active/archived 生命周期目录。
- 不引入新的删除状态机、数据库字段或 UI；若未来确认产品需要永久删除，应按新的跨资源协议单独设计，而不是复活旧实现。

### 文件变更

- 删除：`crates/vulcan-workspace-store/src/workspace_store/persistence/deletion.rs`
- 修改：`crates/vulcan-workspace-store/src/workspace_store/persistence.rs`
- 修改：`crates/vulcan-workspace-store/src/workspace_store/mod.rs`
- 修改：`crates/vulcan-workspace-store/src/workspace_store/persistence/index.rs`
- 修改：`crates/vulcan-workspace-store/src/workspace_store/persistence/session_lifecycle.rs`
- 修改：`crates/vulcan-workspace-store/src/workspace_store/segmented_session_store.rs`
- 修改：`crates/vulcan-workspace-store/src/workspace_store/segmented_session_store/session_management.rs`
- 修改：`crates/vulcan-workspace-store/src/workspace_store/segmented_session_store/segment_io.rs`
- 修改：`crates/vulcan-workspace-store/src/workspace_store/segmented_session_store/tests/metadata_archive.rs`
- 修改：`TASK_LOG.md`
- 新增：无

### 修改范围代码审核与自动修复

- CodeKit 先从两个公开函数向上确认 Service/RPC/WebUI 调用点为零，再向下确认 `delete_session_index_row`、locator 删除包装、date-based delete、tombstone path 与 removal sync 都只有该死链消费者。
- 审核区分了仍在生产使用的 archive/restore move/sync 逻辑与永久删除独占逻辑；保留 `move_segmented_session_directory`、`synchronize_segmented_session_directory_move` 和真实目录检查，没有误删活动生命周期基础设施。
- 删除链后首次 `cargo check` 发现 `use session_lifecycle::*` 已无消费者；已自动移除，不使用 lint allow。
- 修改范围复查发现原组合测试不再通过永久删除清空 session directory，会把完整恢复夹具留在系统临时目录；已自动增加显式 test-root 清理。
- 审核确认测试总数减少两项，恰好对应删除的 tombstone-only recovery 与 canonical/tombstone ambiguity 测试；archive/restore 组合测试仍保留且通过。
- 最终结构化搜索确认永久删除 API、删除 artifacts helper、session index delete、date-based delete、tombstone path/常量、removal sync 与 `archived != 0` 在 Rust 源码中全部为 0 个匹配。
- CodeKit 最终复查 8 个现存修改 Rust 文件，共解析 99 个结构项，错误为 0；被删除模块不再参与 AST 扫描。

### 验证结果

- `archive_and_restore_by_created_date` 定向回归：1 项通过，证明删除专用逻辑移除后确定性 archive/restore 组合仍正常。
- archive/restore 相关定向回归：5 项全部通过，覆盖正常转换、已完成目录移动续作与双树歧义拒绝。
- `cargo test -p vulcan-workspace-store`：通过，2 个测试套件共 133 项全部通过。
- `cargo test -p vulcan-service-kernel`：通过，2 个测试套件共 349 项全部通过。
- `cargo test --workspace`：通过，63 个测试套件共 1556 项全部通过。
- `cargo clippy -p vulcan-workspace-store --all-targets --no-deps -- -D warnings`：通过，零告警。
- `cargo clippy --workspace --all-targets -- -D warnings`：通过，全工作区零告警。
- 8 个现存修改 Rust 文件 `rustfmt --edition 2024 --check`：通过。
- 9 个本轮 Rust 文件（含 1 个删除文件）`git diff --check`：通过，未发现空白错误。

### 遗留问题与下一轮方向

- 当前产品没有永久删除能力；若未来需要，必须先确认跨 SQLite/文件系统的 durable delete intent、恢复阶段与用户交互，这是重大架构决策，不能直接恢复本轮删除的旧 API。
- 旧版本可能已经在 data root 留下 `.segmented_session_deletions` 目录；本轮遵循不保留历史兼容，不增加自动迁移或清理。若用户需要磁盘治理，应提供显式审计/清理命令并先确认范围。
- `archive_missing_projects` 与 `update_session_index_archive_state` 仍使用无返回行 execute；下一轮应沿真实生产调用确认哪些状态转换必须使用 expected-state `RETURNING`。
- Service Kernel 的图标本地投影辅助仍以“至少命中一份投影”为成功条件，可能掩盖 workspace/workbench 投影分裂。
- 多个数据库提交后的 snapshot refresh 失败仍会被客户端解释为整个变更失败，需要长期 mutation result/revision 协议。
- projects Schema CHECK、默认项目 `DefaultHasher`、项目路径 canonicalize fallback 与新增顺序溢出仍按前轮记录保留。
- 本轮没有修改前端功能；只是删除后端不可达能力与独占测试，前端生产构建结果不受影响。

## 第 49 轮：删除死全量快照链并收紧会话生命周期提交（2026-07-10）

### 问题与事实依据

- `archive_missing_projects` 只由 `persist_workspace_snapshot` 调用，而 `persist_workspace_snapshot` 在全仓库没有任何生产调用点；它连同批量 session index、分段 snapshot group、transcript-loaded 判定、active project id 查询和项目 owner 强制解析组成一条不可达的旧全量快照链。
- 旧全量快照链通过无返回行 execute 把“当前快照缺失”的项目直接标记为 archived；它既没有 expected-state 保护，也没有真实产品入口，继续保留只会让未来调用者误用一个宽松的批量软归档协议。
- 可达的会话 archive/restore 路径会先按 locator 移动 active/archived 分段目录，再调用 `update_session_index_archive_state` 更新 SQLite；旧更新只按 session id 写 archive flag，目标行缺失、locator 已变化或 archive 状态已改变时都可能零行成功或覆盖新状态。
- locator 查询已经提供 project id、project tag、created_at 与期望 archive 树；这些字段共同构成文件位置身份，因此 SQLite 状态提交必须验证同一份完整 locator，不能只检查 session id。
- `persist_single_session_update` 是生产可达入口，但仍保留死全量链遗留的 dummy session/archived session/compaction 参数与 warning outcome；唯一调用方丢弃 outcome，分段文件写入错误还会被降级为 warning。
- 旧单会话更新先 upsert SQLite locator，再写分段文件；文件写入失败时数据库会公开一个没有可靠 backing data 的 locator。
- 后台完整会话、transcript 与 addon 写入虽然共享 `webui_workspace_persist_lock` 并分别使用版本域淘汰陈旧任务，但 archive/restore 服务此前绕过该 gate；已排队的旧快照可能在生命周期迁移后重新创建 active 目录或覆盖 index。
- 项目新增/恢复路径同样会改变项目与会话生命周期，但此前没有与后台会话持久化统一串行，刷新快照也可能和旧写任务交错。

### 已查明的执行流程

1. 会话归档从当前 workspace snapshot 解析 session id 和 controller，随后按 active locator 定位分段目录并移动到 archived tree。
2. 项目会话归档先查询该 project 的全部 active locator，再逐个复用相同目录移动与 index archive 转换。
3. 项目恢复先把 project row 从 archived=true 转为 false，再查询其 archived locator，逐个把分段目录恢复到 active tree并转换 index 状态。
4. locator 的 project id、project tag 与 created_at 同时决定分段目录位置；index archive 转换现在把这三个字段和 expected archive flag 一并放入 UPDATE WHERE。
5. UPDATE 通过 `RETURNING id, archived` 返回提交事实；返回验证要求恰好一行、身份精确、archive 必须为严格 0/1 且等于目标状态，零行与异常多行都显式失败。
6. Service Kernel 在 archive/remove 开始前提升目标 session 的完整写入版本栅栏，再获取共享 workspace persist gate，确保先前获取版本但尚未入 gate 的任务在进入后被判定陈旧。
7. 存储尝试结束后再次提升栅栏，覆盖在第一次提升与 gate 获取之间新分配的版本；该步骤在存储失败时也执行，不会因提前 `?` 跳过。
8. 权威 workspace/workbench 刷新仍在同一 gate 内完成；刷新尝试后第三次提升栅栏，防止刷新期间新分配的旧生命周期写入在 gate 释放后提交。
9. 生命周期栅栏一次锁定版本状态，同时推进完整 session、transcript 及目标 session 当前已分配的所有 addon namespace；其他 session 的版本保持不变。
10. 项目新增或恢复现在也在共享 gate 内完成“判断→持久化/恢复→权威刷新”，不再让项目生命周期切换和后台会话写入并发提交。
11. 单会话完整更新先计算有效 transcript 并持久化分段文件；任何文件错误直接返回，只有 backing data 成功后才 upsert session index locator。
12. CodeKit 证明旧全量 snapshot API、项目缺失归档及其专属 helpers 没有生产消费者；本轮从导出和实现两端删除整条链，不保留兼容包装。

### 长期优化调整

- 删除 `persist_workspace_snapshot` 及其批量 project/session/segmented snapshot 依赖链，移除不可达的 `archive_missing_projects`、`query_active_project_ids`、owner 强制解析与旧 outcome/warning 模型。
- 将项目新建写入收敛为私有单项目 `upsert_active_project`，只接受活动项目并使用 checked `usize → i64` 顺序转换。
- 将会话 archive flag 写入升级为“完整 locator + expected state + RETURNING”状态转换，杜绝零行假成功、陈旧 locator 覆盖和宽松 archive 值。
- `PersistedSessionIndexLocator` 明确保留 project id，使文件系统定位事实和 SQLite 提交保护使用同一身份来源。
- 简化 `persist_single_session_update` 为 `Result<(), String>`，删除无消费者 outcome 与 compaction 参数；文件写入错误不再吞为 warning。
- 调整单会话完整写入顺序为 backing file first、locator publication second，避免 index 提前公开失败文件状态。
- 新增 session 生命周期版本栅栏，覆盖完整 session、transcript 和所有已分配 addon 通道，并用单锁保证目标通道原子推进。
- archive conversation、archive project conversations、remove project 与 add/restore project 全部接入共享持久化 gate；成功和失败路径都执行必要的版本淘汰。
- 删除另一个无生产调用的 `archive_project` 公共包装，保留产品实际使用的 archive sessions、remove-after-archive 与 restore 明确入口。

### 文件变更

- 修改：`crates/vulcan-workspace-store/src/workspace_store/persistence/snapshot.rs`
- 修改：`crates/vulcan-workspace-store/src/workspace_store/persistence.rs`
- 修改：`crates/vulcan-workspace-store/src/workspace_store/mod.rs`
- 修改：`crates/vulcan-workspace-store/src/workspace_store/persistence/tests.rs`
- 修改：`crates/vulcan-workspace-store/src/workspace_store/persistence/index.rs`
- 修改：`crates/vulcan-workspace-store/src/workspace_store/persistence/projects.rs`
- 修改：`crates/vulcan-workspace-store/src/workspace_store/queries.rs`
- 修改：`crates/vulcan-workspace-store/src/workspace_store/persistence/session_lifecycle.rs`
- 修改：`crates/vulcan-workspace-store/src/workspace_store/models.rs`
- 修改：`crates/vulcan-service-kernel/src/workspace_session_persistence.rs`
- 修改：`crates/vulcan-service-kernel/src/workspace_persistence_versions.rs`
- 修改：`crates/vulcan-service-kernel/src/project_conversation_services/conversation_services.rs`
- 修改：`crates/vulcan-service-kernel/src/project_conversation_services/project_services.rs`
- 修改：`crates/vulcan-service-kernel/src/project_directories.rs`
- 修改：`TASK_LOG.md`
- 新增：无
- 删除：无

### 关键代码调整详情

- `update_session_index_archive_state` 新增完整 locator 参数，SQL 同时校验 session id、project id、project tag、created_at 与 expected archived，并通过类型化 RETURNING 结果确认提交。
- `validate_session_archive_state_update_rows` 集中验证返回行数量、身份和严格布尔状态；新增单元测试覆盖精确命中、零行、错身份、非法 archive 与错误目标状态。
- `persist_single_session_update` 删除 compactions 与 outcome，分段持久化错误直接向 Service Kernel 传播；调用方不再执行无意义的 `.map(|_| ())`。
- `invalidate_webui_session_lifecycle_persist_versions` 在同一个版本状态锁内推进 session/transcript/addon 版本；新增测试证明目标所有通道失效且其他 session 不受影响。
- `service_archive_conversation`、`service_archive_project_conversations` 与 `service_remove_project` 使用“前置栅栏→共享 gate→存储尝试→后置栅栏→权威刷新→最终栅栏”顺序，错误分支不会跳过后置淘汰。
- `service_add_project_from_path` 把现有项目判断、新增或 restore 和刷新放入同一 shared persist gate，避免 add/restore 与旧 session snapshot 交叉提交。

### 修改范围代码审核与自动修复

- 首次审查删除了只服务死全量 snapshot 的三项单元测试，并同时移除对应 helpers，避免为不可达实现保留虚假覆盖率。
- 审核发现 `required_session_project_owner` 删除批量链后只剩自身测试，已连同测试一起移除；没有把测试当成生产消费者。
- 审核发现 `WorkspaceSnapshotPersistOutcome`、warning builder、dummy sessions/archived sessions/compactions 仍残留在可达单会话路径，已进一步删除并改为显式错误传播。
- 审核发现单会话写入先发布 locator 后写文件，已自动交换提交顺序，使失败不会制造新悬空 locator。
- 审核发现 archive/restore 服务绕过后台持久化 gate 与版本域，已增加生命周期栅栏和统一 gate；随后又发现存储错误上的提前 `?` 会跳过第二次栅栏，已改为先捕获结果、执行栅栏，再传播错误。
- 审核发现 `archive_project` 是没有调用点的公共包装，已从实现和导出删除，避免继续扩大生命周期 API 表面。
- 真实 SQLite 3.51.1 验证：完整 locator 与 expected state 匹配时返回 `s1|1` 并提交；故意使用陈旧 project tag 时 RETURNING 为空，archive flag 与 updated_at 均保持原值。
- 最终结构化搜索确认旧全量 snapshot、项目缺失归档、active project id 查询、旧 outcome/warning、旧批量类型、`archive_project` 与 `upsert_projects` 在 Rust 源码中均为 0 个匹配。
- CodeKit 最终复查 14 个直接修改 Rust 文件，共解析 187 个结构项，错误为 0；生命周期相关搜索得到 5 个文件、16 个匹配，确认 archive state 更新只有定义与两个精确调用点，项目生命周期栅栏在两个服务中各执行三次。

### 验证结果

- 会话 archive RETURNING 验证回归：1 项通过。
- locator 身份与 archive 状态回归：3 项通过。
- archive/restore 目录与 locator 过滤回归：5 项通过。
- 默认 workspace project 过滤回归：3 项通过。
- 单会话有效消息持久化回归：1 项通过。
- Service Kernel 生命周期版本栅栏回归：1 项通过。
- 真实 SQLite 成功提交与陈旧 locator 拒绝两条执行验证：全部通过。
- `cargo test -p vulcan-workspace-store`：通过，2 个测试套件共 131 项全部通过。
- `cargo test -p vulcan-service-kernel`：通过，2 个测试套件共 350 项全部通过。
- `cargo test --workspace`：通过，63 个测试套件共 1555 项全部通过。
- `cargo clippy -p vulcan-workspace-store -p vulcan-service-kernel --all-targets --no-deps -- -D warnings`：通过，零告警。
- `cargo clippy --workspace --all-targets -- -D warnings`：通过，全工作区零告警。
- 14 个直接修改 Rust 文件 `rustfmt --edition 2024 --check`：通过。
- 本轮 14 个 Rust 文件 `git diff --check`：通过，未发现空白错误。
- 测试总数相对第 48 轮净减少 1：删除 3 项死批量链测试，新增 1 项 archive 返回验证和 1 项生命周期版本栅栏测试。

### 遗留问题与下一轮方向

- `append_single_session_transcript_suffix` 当前看起来只有定义和导出、没有生产调用；下一轮应沿 transcript 增量写入入口再次确认，若确属不可达则连同专属实现删除。
- `upsert_session_index_row` 仍使用无返回行 execute，且 ON CONFLICT 可以更新 locator 与 archive flag；已知生产路径现由共享 gate 保护，但 WorkspaceStore API 自身仍没有 RETURNING 提交证明或 expected-locator 保护。
- `persist_single_session_runtime_state_update` 在目标 session 目录不存在时可能沿底层 helper 返回成功；需要追踪调用者是否把“目录尚未创建”定义为合法无操作，不能先行猜测。
- Service Kernel 图标本地投影仍以至少命中一份投影为成功，可能掩盖 workspace/workbench 状态分裂。
- 多个数据库提交后的 snapshot refresh 失败仍会被客户端解释为整个 mutation 失败，长期需要带提交结果和 revision 的响应协议。
- projects Schema CHECK、默认项目 `DefaultHasher`、项目路径 canonicalize fallback 与新增顺序 `order + 1` 溢出仍按前轮记录保留；Schema 与持久 ID 迁移属于需用户确认的重大变更。
- 本轮没有修改前端；后台版本栅栏和共享 gate 不改变现有 RPC/API 形状。

## 第 50 轮：删除无保护转录尾部协议并原子化完整转录更新（2026-07-10）

### 问题与事实依据

- `append_single_session_transcript_suffix` 虽被 WorkspaceStore 两层公开导出，但全仓库没有任何生产调用点；精确搜索只找到定义与两处导出。
- 该高层入口调用 `append_segmented_session_message_suffix`，后者只被该死入口和一项专属测试使用；配套 `advance_segmented_message_checkpoint_with_suffix` 也只有该底层函数一个消费者。
- suffix API 只接收“待追加消息切片”，没有已持久化尾部身份、完整 transcript 或预期 message count，无法在自身协议内证明调用方切片确实紧随当前尾部。
- 底层 suffix 实现直接把消息编号从 `metadata.message_count` 开始；它只检查新切片内部生成身份是否重复，不能验证既有尾部身份/内容，也不能阻止调用方误把完整 transcript 当成 suffix 后重复追加。
- 真实生产路径始终提交完整可见 transcript：流式投影和执行报告检查点通过 `spawn_workspace_session_transcript_persist_task` 调用，context compaction 直接调用 `persist_single_session_transcript_messages`，最终都进入 `append_segmented_session_message_update`。
- 旧 `append_segmented_session_message_update` 先调用严格 tail append，再用 `error.contains("non-tail transcript change")` 判断是否物理重写；错误文案实际承担控制协议，文案变更或意外同词错误都可能改变执行分支。
- 旧流程的 tail 检测持有追加锁，但返回错误时先释放锁，随后 rewrite 再次获取锁并重新读取元数据；另一写入可在检测与重写之间插入，使重写决策基于已经失效的边界事实。
- 完整会话写入 `persist_segmented_session_tail_update` 同样解析上述错误字符串，但只把非尾部分歧延迟到 runtime state 写完后再报错；完整 transcript 发生插入、缩短或已有行更新时，文件已部分更新而 locator 发布失败。
- 旧边界验证对缺失 `last_message_id` 或 `last_message_fingerprint` 使用 `is_some_and`；message count 大于零但检查点字段缺失时会被当成匹配，允许在没有边界证明的情况下继续追加。

### 已查明的执行流程

1. 实时 assistant/display 变化先更新保留 workspace snapshot，再由 Service Kernel 为该 session 分配 transcript persist version。
2. 后台 transcript task 获取共享 workspace persist gate，确认版本仍为当前值，从 snapshot 抽取完整 loaded messages 后调用 WorkspaceStore。
3. context compaction 在完整 session version gate 下构建压缩后必须保留的完整 display window，再调用相同的完整 transcript 持久化入口。
4. `persist_single_session_transcript_messages` 解析既有 segmented session directory，先修复 JSONL 尾损坏，再提交完整消息切片给统一 update 协议。
5. 完整会话持久化先确保 locator 对应的 active/archived session directory 存在，现在也把完整有效 transcript 交给同一 update 协议，然后写 state/runtime log，最后由上层发布 SQLite locator。
6. 统一 update 只获取一次进程内追加锁，在锁内修复分段文件并读取唯一一份 metadata checkpoint。
7. `segmented_message_tail_matches` 把 `message_count` 作为持久化前缀长度；零消息检查点要求 id/fingerprint 同时为空，非零检查点要求传入 transcript 足够长、边界 stable id 和序列化 fingerprint 全部精确匹配。
8. 边界完整匹配时，locked tail helper 只生成 `message_count..messages.len()` 的新事件，检查新增切片身份，分配 turn index 与 JSONL sequence，并更新 checkpoint。
9. 任一 count、尾部身份、尾部内容或检查点完整性不匹配时，同一把锁下直接进入 physical rewrite；不会通过错误字符串跳转，也不会在检测与重写之间释放锁。
10. rewrite 保留非消息事件，按 message index 重建现存消息行，删除超出新 transcript 的行，补齐缺失行，并用完整输入恢复 message count、tail id 与 fingerprint。
11. 旧 suffix 公共入口、底层函数、专属 checkpoint helper、导出和自证测试均被删除；全部测试夹具改用产品真实的完整 update 协议。

### 长期优化调整

- 删除 `append_single_session_transcript_suffix` 及 WorkspaceStore/persistence 两层公共导出，不保留无消费者兼容包装。
- 删除 `append_segmented_session_message_suffix` 与 `advance_segmented_message_checkpoint_with_suffix`，彻底移除无法自证边界的裸 suffix 写入协议。
- 把 `append_segmented_session_message_update` 定义为唯一完整 transcript 提交入口，明确调用方必须提供完整权威快照。
- 将边界判断从错误字符串解析改为 `Result<bool, String>` 的类型化事实：`true` 表示可以增量追加，`false` 表示必须物理重写，真实存储错误继续显式传播。
- 把 repair、metadata read、边界判断、append/rewrite 决策和选定写入路径放在同一把 append lock 下，消除检测后重新加锁的竞态窗口。
- 将 append 与 rewrite 拆成只能在持锁上下文调用的私有 helper，并共享同一份 metadata snapshot，避免二次读取覆盖决策事实。
- 完整会话写入由 strict-tail-with-error 改为完整 update/reconcile；插入、缩短、编辑和缺失 checkpoint 不再导致部分 runtime state 后报错。
- 将误导性的 `persist_segmented_session_tail_update` 重命名为 `persist_segmented_session_update`，同步补齐完整双语参数、返回值和关键变量文档。
- 全部低层测试夹具改用真实 update 入口，删除只证明死 suffix 行为的测试；新增缺失 checkpoint 物理重写与修复回归。

### 文件变更

- 修改：`crates/vulcan-workspace-store/src/workspace_store/persistence/snapshot.rs`
- 修改：`crates/vulcan-workspace-store/src/workspace_store/persistence.rs`
- 修改：`crates/vulcan-workspace-store/src/workspace_store/mod.rs`
- 修改：`crates/vulcan-workspace-store/src/workspace_store/persistence/runtime_tail.rs`
- 修改：`crates/vulcan-workspace-store/src/workspace_store/segmented_session_store/message_events.rs`
- 修改：`crates/vulcan-workspace-store/src/workspace_store/segmented_session_store/tests/append_events.rs`
- 修改：`crates/vulcan-workspace-store/src/workspace_store/segmented_session_store/tests/rollback_updates.rs`
- 修改：`crates/vulcan-workspace-store/src/workspace_store/segmented_session_store/tests/runtime_rebuild.rs`
- 修改：`crates/vulcan-workspace-store/src/workspace_store/segmented_session_store/tests/runtime_sync.rs`
- 修改：`crates/vulcan-workspace-store/src/workspace_store/segmented_session_store/tests/session_lifecycle.rs`
- 修改：`crates/vulcan-workspace-store/src/workspace_store/segmented_session_store/tests/window_and_state.rs`
- 修改：`TASK_LOG.md`
- 新增：无
- 删除：无

### 关键代码调整详情

- `append_segmented_session_message_update` 现在在单锁范围内读取 metadata，并根据 `segmented_message_tail_matches` 的布尔事实直接选择 locked append 或 locked rewrite。
- `append_segmented_session_message_tail_locked` 只接受已证明前缀匹配的完整 transcript，计算首个新增 index、拒绝新增切片重复身份、追加事件并更新完整 checkpoint。
- `rewrite_segmented_session_message_records_locked` 不再自行获取锁或重新读取 metadata，避免检查与提交使用两份状态快照。
- `segmented_message_tail_matches` 对非零 checkpoint 强制要求 stable tail id 和 fingerprint 同时存在且精确匹配；对零 count 强制要求两个 tail 字段同时为空。
- `persist_segmented_session_update` 对完整会话 transcript 复用统一 reconcile 入口，不再解析错误文案或在 transcript 分歧后继续写 runtime state 再返回错误。
- `append_message_update_physically_rewrites_inserted_history` 将旧“strict tail 必须报错”测试改为长期产品语义：插入消息必须物理重写并可完整 replay。
- `append_message_update_rewrites_incomplete_tail_checkpoint` 人为清空非零 checkpoint 的 id/fingerprint，验证更新会重写变更行、恢复两个 checkpoint 字段并得到权威 replay。

### 修改范围代码审核与自动修复

- 初次删除死 suffix 链后，审核发现 `append_segmented_session_message_update` 与完整会话写入仍有两处 `error.contains` 控制流；已继续重构为类型化边界判断，没有把文案协议留到下一轮。
- 审核确认旧 update 在 tail 检测错误与 rewrite 之间释放追加锁；已把检测与两条提交分支收敛到单锁，避免基于过期 metadata 重写。
- 审核发现完整会话路径仍名为 `tail_update` 且文档声称只写 append-only stream；在它已支持物理重写后已自动重命名并修正文档。
- 审核发现 `message_count == 0` 时边界判断仍可能忽略残留 tail 字段；已收紧为 id/fingerprint 必须同时为空才允许 append。
- 审核发现测试函数、test-root 名称和变量仍残留 `message_tail` 旧术语；已统一为完整 message update/verified tail 语义。
- 所有原 strict-tail 测试夹具改用产品真实入口后仍覆盖幂等追加、事件类型映射、tool detail sequence、分段窗口、runtime rebuild、fork 和生命周期行为。
- 最终结构化搜索确认高层 suffix、底层 suffix、suffix checkpoint helper、非尾部分歧错误字符串与旧 `persist_segmented_session_tail_update` 在 Rust 源码中均为 0 个匹配。
- CodeKit 最终复查 11 个直接修改 Rust 文件，共解析 60 个结构项，错误为 0。

### 验证结果

- transcript update 定向回归：5 项通过，覆盖插入行重写、缺失 checkpoint 修复、缩短历史重写、重复更新与 retained event gap。
- segmented session store 回归：59 项通过；新增 checkpoint 测试后由完整包回归覆盖 60 项相关测试。
- `cargo test -p vulcan-workspace-store`：通过，2 个测试套件共 131 项全部通过。
- `cargo test -p vulcan-service-kernel`：通过，2 个测试套件共 350 项全部通过。
- `cargo test --workspace`：通过，63 个测试套件共 1555 项全部通过。
- `cargo clippy -p vulcan-workspace-store --all-targets --no-deps -- -D warnings`：通过，零告警。
- `cargo clippy --workspace --all-targets -- -D warnings`：通过，全工作区零告警。
- 11 个直接修改 Rust 文件 `rustfmt --edition 2024 --check`：通过。
- 本轮 11 个 Rust 文件 `git diff --check`：通过，未发现空白错误。
- 测试总数与第 49 轮相同：删除 1 项死 suffix 自证测试，新增 1 项不完整 checkpoint 修复测试。

### 遗留问题与下一轮方向

- `persist_single_session_transcript_messages` 与 `persist_segmented_session_runtime_state_checkpoint` 在目标 session directory 不存在时仍返回成功；下一轮必须结合新会话初始落盘顺序和后台版本 gate 确认“尚未创建”是否允许无操作。
- `persist_single_session_transcript_messages` 对空消息直接返回成功，因此 transcript-only 路径不能清空既有转录，而完整 session update 会把空 transcript 物理重写为空；两条入口的空快照语义需要沿真实调用者统一。
- physical rewrite 对每个 segment 使用单文件原子替换，但多个 segment 与 metadata 之间仍不是目录级原子提交；中途 I/O 失败可能留下部分新分段与旧 metadata，需要评估 staged directory swap 或持久化 rewrite intent。
- `upsert_session_index_row` 仍使用无返回行 execute，WorkspaceStore API 自身没有 expected-locator/RETURNING 提交证明。
- Service Kernel 图标双投影、提交后 refresh 失败语义、projects Schema CHECK、默认项目 `DefaultHasher`、路径 canonicalize fallback 与新增顺序溢出仍按前轮记录保留。
- 本轮只修改 WorkspaceStore Rust 后端与测试，没有前端产物变化。

## 第 51 轮：修复项目删除墓碑导致的 Workspace 启动失败（2026-07-10）

### 问题与事实依据

- 桌面应用长期停留在 Workspace 加载 83%，界面显示“正在加载项目与会话索引”，但原启动失败路径没有向终端输出错误，导致一次真实失败表现为无反馈卡住。
- `run_service_bootstrap` 在该阶段同步等待 `load_or_seed_workspace_index`；当前未提交差异把会话所属项目缺失从旧宽松投影改成了致命校验错误。
- 用户真实 `app-workspace.sqlite3` 中有 1 条项目记录、111 条会话索引，其中 62 条会话索引所属项目已经不存在，49 条仍属于当前项目。
- 源码调用链确认 `remove_project_after_archiving_sessions` 会先归档项目会话，再调用 `delete_active_project_row` 删除项目行；因此“归档会话仍在、项目行已删除”是当前删除协议的合法墓碑状态，不是数据库损坏。
- 62 条墓碑记录中有 61 条生命周期目录与索引一致；另有 1 条索引仍标记活动、目录已位于归档树，属于跨 SQLite 与文件系统操作中断留下的状态差异。

### 核心修复与调整概述

- Workspace 加载器不再把已删除项目的会话墓碑当成启动错误，也不重新构造缺失项目摘要；这些墓碑从实时工作区投影中排除并按总数输出诊断。
- 对仍有项目行的会话，加载器同时检查活动树与归档树；仅相反目录存在时，自动把唯一副本恢复到 SQLite 声明的生命周期位置并同步 metadata。
- 会话归档或恢复在目录移动成功、SQLite 索引提交失败时，使用 locator 中原始 `updated_at` 执行反向目录与 metadata 回滚，避免留下跨资源半提交。
- 会话 locator 查询补充 `updated_at`，并在持久化边界拒绝空白回滚时间戳。
- Service Kernel 在 Workspace 索引加载前后输出阶段日志，并在任何启动失败时直接向终端输出完整失败原因。
- 诊断期逐会话日志已经删除，最终只保留阶段、汇总、异常恢复和失败输出。

### 文件变更

- 修改：`crates/vulcan-service-kernel/src/service_bootstrap.rs`
- 修改：`crates/vulcan-service-kernel/src/service_bootstrap/startup_updates.rs`
- 修改：`crates/vulcan-workspace-store/src/workspace_store/loading.rs`
- 修改：`crates/vulcan-workspace-store/src/workspace_store/persistence/index.rs`
- 修改：`crates/vulcan-workspace-store/src/workspace_store/persistence/session_lifecycle.rs`
- 修改：`TASK_LOG.md`
- 新增：无
- 删除：无

### 关键代码调整详情

- `retained_project_row_for_session` 只允许仍存在的项目授权会话进入工作区投影；项目删除后的会话索引被识别为墓碑并计数跳过。
- `required_session_directory_for_index_row` 对活动/归档双树执行四态判定：唯一预期目录直接使用、唯一相反目录自动恢复、双缺失和双存在均显式报错。
- `PersistedSessionIndexLocator` 新增原始更新时间戳，单条与项目级 locator 查询都同步选择 `updated_at`。
- `archive_segmented_session_directory_for_locator` 与 `restore_segmented_session_directory_for_locator` 在 SQLite 提交失败时执行对称文件系统回滚，并把提交错误与回滚错误一起返回。
- `update_service_failure` 现在把启动阶段和失败原因写入 stderr，不再只更新用户看不到的内存快照。

### 验证结果与测试方式

- `make br` 真实启动通过，终端输出：项目 1 条、会话索引 111 条、加载 49 条、项目删除墓碑 62 条，随后出现 `workspace index load complete`；桌面服务保持稳定运行。
- `cargo test -p vulcan-workspace-store`：通过，2 个测试套件共 133 项全部通过。
- `cargo test -p vulcan-service-kernel`：通过，2 个测试套件共 350 项全部通过。
- `cargo test --workspace`：通过，63 个测试套件共 1557 项全部通过。
- `cargo clippy --workspace --all-targets -- -D warnings`：通过，全工作区零告警。
- `cargo fmt --all -- --check`：通过。
- `git diff --check`：通过，仅报告现有 Windows 行尾转换提示，没有空白错误。
- CodeKit 复核 5 个直接相关 Rust 文件，共识别 79 个结构项，解析错误为 0；关键加载、locator 与生命周期回滚函数边界正确。

### 遗留问题与注意事项

- 项目删除墓碑目前保留在 SQLite 与归档目录中，但不进入实时 Workspace 投影；这是与当前“删除项目但保留归档会话数据”协议一致的行为。
- 真实数据中的 1 条已删除项目活动索引不会在启动时自动改写，因为项目墓碑先于会话目录恢复被排除；它不再阻塞启动，也不会被错误挂载到当前项目。后续如需清理墓碑数据，应单独设计带审计和恢复策略的维护流程。
- `make br` 当前仍在运行，便于直接进行桌面端人工验收；本轮没有提交、暂存或改写用户其他未提交变更。

## 第 52 轮：从领域版本根因修复分叉会话归档不收敛（2026-07-10）

### 问题与事实依据

- 用户对刚分叉的会话 `8b1f2f6c-0904-43f8-aea9-928cc08b341e` 执行归档时收到“活动会话不存在，拒绝归档”。
- 真实 SQLite 行存在且已经是 `archived = 1`，创建时间为 `1783669156063`，归档更新时间为 `1783669158974`；对应唯一目录已经位于 `archived_sessions/vulcan-code-f75456bf/2026-07-10-8b1f2f6c-0904-43f8-aea9-928cc08b341e`。
- 因此分叉持久化与第一次归档均已提交；随后反复点击仍可收到成功提示，但活动列表直到重启才消失，证明数据库和目录正确，失效点位于运行中客户端状态收敛链。
- 第 23 轮已经确定：完整快照、热路径增量与聚焦领域响应可以任意交错；无关的更高全局 revision 不能拒绝较低但尚未应用的领域变更，必须维护领域独立接收版本。
- 第 49 轮为收紧持久化 gate，删除了归档后直接修改保留态的 `archive_conversation_in_retained_state`，改为 gate 内异步 `refresh_workbench_snapshot` 并通过 RPC 返回完整 `WorkbenchSnapshot`；这一改动重新把会话生命周期提交绑回全局快照门禁。
- 归档刷新提交后，只要后续局部增量先到达 WebUI，`acceptedWorkbenchRevisionRef` 就会拒绝较低 revision 的归档完整快照；局部增量不拥有会话集合，无法携带“从活动列表移除”这一事实，因此列表持续保留旧行，重启加载数据库后才恢复。
- `service_archive_conversation` 还在进入 WorkspaceStore 前用可变活动快照验证会话；即使存储层能识别“已经归档”，较晚的重复请求仍可能在权威 SQLite 查询前被陈旧入口条件拒绝。
- WebUI 会话归档动作没有同步重入保护，React 状态提交前的重复点击可以发出多个相同 RPC。

### 已查明的根因链

1. 第一次请求在共享持久化 gate 内完成活动目录到归档目录的移动和 SQLite `archived = 1` 提交。
2. `refresh_workbench_snapshot` 从持久化层得到正确活动/归档集合，推进全局 generation 并返回完整快照。
3. 另一个热路径增量可能在归档 RPC 响应到达前推进并占用更高的客户端全局 revision。
4. 项目动作入口按全局 revision 丢弃归档完整快照；该增量只更新自身字段，不会移除活动会话。
5. 运行中列表因此与已提交 SQLite 分裂；重启绕过丢失的运行时响应，直接从持久化层重建后恢复正确。
6. 若用户在分裂窗口再次点击，服务入口仍先查询活动快照，可能在 WorkspaceStore 的“活动 / 已归档 / 不存在”权威判定前报错。

### 核心修复与调整概述

- 新增正式协议 `ConversationArchivedDelta`，只拥有会话归档生命周期字段：被移除的活动会话标识、可选的权威归档摘要和归档后的选中会话标识。
- 归档刷新在共享 gate 内提交完成后，以刷新提交的同一 generation 发送 `workbench.delta` 事件，再返回原完整快照；事件进入现有 generation 缺口缓冲，不再依赖 RPC 全量快照的到达顺序。
- WebUI 新增独立 `acceptedConversationLifecycleRevisionRef`；完整快照可以推进该领域版本，但无关领域或局部增量不能拒绝尚未应用的归档生命周期转换，重新落实第 23 轮的领域版本原则。
- 生命周期 reducer 即使面对更高的混合工作台 revision，也只应用协议明确拥有的归档字段，不覆盖无关较新状态；同时清理目标父会话的消息、水合、待发送提示词、ask、Addon、VMM、上下文注入与子代理缓存投影。
- 侧栏本地选择新增通用活动集合不变量：草稿选择保留，仍活动的当前选择保留，否则使用后端生命周期选择或首个活动会话。
- 删除本轮中途加入的“完整快照被拒绝后本地强制归档”症状补丁，`applyProjectActionSnapshot` 恢复无返回值语义，浏览器 fallback 预览也恢复原有仅移除活动行的职责。
- `service_archive_conversation` 不再用可变活动快照抢先判定会话存在性，只读取控制器；WorkspaceStore 统一依据 SQLite 查询结果区分活动定位行、已归档定位行和完全不存在。
- WorkspaceStore 通过 `SessionArchiveTransition::Required` 与 `AlreadyCommitted` 实现明确幂等状态机：活动定位行执行物理转换，已归档定位行直接成功，双侧均不存在才显式失败。
- WebUI 保留按会话标识的同步归档进行中集合，同一会话首个 RPC 完成前拒绝重复发送，无论成功或失败均在 `finally` 中释放。

### 文件变更

- 修改：`crates/vulcan-service-api/src/conversation.rs`
- 修改：`crates/vulcan-service-kernel/src/lib.rs`
- 修改：`crates/vulcan-service-kernel/src/rpc_dispatch.rs`
- 修改：`crates/vulcan-service-kernel/src/project_conversation_services/conversation_services.rs`
- 修改：`crates/vulcan-workspace-store/src/workspace_store/persistence/session_lifecycle.rs`
- 修改：`apps/vulcan-code-web/src/types/conversation.ts`
- 修改：`apps/vulcan-code-web/src/state/workbench-message-merge.ts`
- 修改：`apps/vulcan-code-web/src/state/workbench-state.ts`
- 修改：`apps/vulcan-code-web/src/components/workspace/workbench-layout.tsx`
- 修改：`TASK_LOG.md`
- 新增：无
- 删除：无

### 关键代码调整详情

- `conversation_archived_delta_envelope` 从归档后权威 workbench 构建同 generation 的生命周期事件；归档摘要保持协议级可选，因为不可恢复的归档墓碑不会进入 WebUI 归档集合。
- `applyConversationArchivedDelta` 原子移除活动行及其按会话缓存，并以权威归档摘要做去重写入；通过 `deriveSelectedConversationFields` 与子代理缓存保留边界重算选择投影。
- `applyWorkbenchDeltaWithSubagentSurfaceMessageCache` 对会话生命周期领域使用独立版本接收，同时保持混合工作台 revision 单调不回退。
- `workbenchDeltaConversationId` 识别归档事件的目标会话，使现有增量诊断与合并链完整覆盖新协议。
- 归档 RPC 调度新增事件发射器参数；事件在刷新成功和 gate 释放后发送，失败的持久化或刷新不会伪造生命周期提交事件。
- 服务入口改用 `controller_runtime_from_state`，未知会话仍由 WorkspaceStore 的活动/归档双查询显式拒绝，不增加候选路径或模糊兜底。

### 验证结果与测试方式

- `cargo test -p vulcan-service-kernel conversation_archive_delta_uses_committed_workbench_projection`：通过，1 项生命周期 generation 与权威摘要定向回归通过。
- `cargo test -p vulcan-workspace-store session_archive_transition_is_idempotent_after_commit`：通过，1 项活动、已提交和完全缺失三态回归通过。
- `cargo test -p vulcan-service-api`：通过，2 个测试套件共 20 项全部通过。
- `npm run lint`：通过，TypeScript 项目检查无错误。
- `npm run build`：通过，2364 个模块完成生产构建；仅保留既有大 chunk 提示。
- `cargo test --workspace`：第二次完整运行通过，63 个测试套件共 1559 项全部通过。
- 第一次全工作区运行仅有未修改模块中的并发时序测试 `streaming_tool_executor_yields_tool_started_before_tool_body_runs` 抖动失败；该测试立即单独复跑通过，随后完整工作区复跑全部通过。
- `cargo clippy --workspace --all-targets -- -D warnings`：通过，全工作区零告警。
- 5 个直接修改 Rust 文件 `rustfmt --edition 2024 --check`：通过。
- 11 个直接相关代码文件 `git diff --check`：通过，未发现空白错误。
- CodeKit 复核 8 个核心协议、服务、存储与前端状态文件，共识别 153 个结构项，解析错误为 0。

### 遗留问题与注意事项

- 当前真实会话已经归档完成，不需要修改 SQLite 或移动目录；本轮修复的是运行时领域收敛协议与重复请求入口，而非修补该条数据。
- 后续 `make br` 与桌面端手工验收由用户执行；Codex 仅负责自动化测试与静态检查，不主动启动桌面应用。
- 本轮没有提交、暂存、清理或覆盖用户其他未提交变更。

## 第 53 轮：纠正归档根因并阻断已归档会话运行态复活（2026-07-10）

### 前轮结论纠正

- 第 52 轮修复了重复归档的存储幂等性和生命周期事件的 revision 接收，但没有解决用户可见问题；此前宣布收尾是错误的。
- 第 52 轮假定归档后的 `refresh_workbench_snapshot` 已经生成正确会话集合，实际没有验证运行中服务的保留快照。
- 新增生命周期事件从错误快照构建，因此只是稳定传播了错误状态；存储幂等又使同一错误活动行可以反复归档并持续显示成功提示。

### 根因事实链

- 用户本轮新归档会话 `48c6b57f-5d20-4159-8c79-76a10f9b4651` 在真实 SQLite 中只有 `archived = 1`，唯一目录位于归档树。
- 直接调用仍在运行的 Core Service `service_workbench_snapshot` 得到 revision 16，服务端保留快照对同一 ID 同时返回 `activeContains = true` 与 `archivedContains = true`；问题已经发生在后端投影层，而非前端事件接收层。
- `refresh_workbench_snapshot_with_selected_session` 在读取 SQLite 前调用 `capture_preserved_workspace_run_state` 捕获旧活动快照。
- 捕获协议不仅保留仍有活动 kernel turn 的会话，还会保留已水合窗口、待发送提示词、已提交会话和包含具体上下文的 idle 会话；当前选中分叉会话因此被捕获。
- SQLite 刷新正确地把该 ID 放入 `archived_sessions` 并从 `sessions` 移除。
- `preserve_active_session_run_state` 原先只判断刷新后的活动 `sessions` 是否缺少旧行，没有检查同一 ID 是否已经存在于权威 `archived_sessions`。
- 对缺失活动行调用 `preserve_missing_visible_session_summary` 后，旧摘要被重新插入 `sessions`、项目会话成员和选中态，随后 `workbench_from_runtime` 同时投影活动与归档集合。
- 重启后旧水合和运行态不再参与捕获，SQLite 归档事实重新成为唯一来源，所以列表才会消失。

### 核心修复与调整概述

- `preserve_active_session_run_state` 在恢复任何摘要、phase、runtime state 或消息前，先检查刷新快照的 `archived_sessions`。
- 一旦存在同 ID 权威归档墓碑，立即跳过该保留项；内存水合态、上下文和 kernel 活动状态均不能覆盖已经提交的生命周期转换。
- 新增真实形态回归：旧快照中会话处于选中且已水合状态，刷新快照中仅存在归档行；旧实现稳定失败并重新插入活动行，修复后保持活动集合为空、归档集合唯一、选中态为空，且不恢复活动消息与水合标记。
- 前端归档请求锁从“RPC 结束即释放”调整为“失败时释放，成功后等待活动行真正消失再释放”。
- 成功响应与生命周期事件之间即使存在短暂调度窗口，同一会话也不能再次发送归档 RPC 或堆叠成功 toast；恢复会话重新进入活动列表前，旧锁已随先前活动行消失而清理。

### 文件变更

- 修改：`crates/vulcan-service-kernel/src/session_hydration/run_state_preservation.rs`
- 修改：`crates/vulcan-service-kernel/src/tests/active_session_state_tests.rs`
- 修改：`apps/vulcan-code-web/src/components/workspace/workbench-layout.tsx`
- 修改：`TASK_LOG.md`
- 新增：无
- 删除：无

### 验证结果与测试方式

- 新回归在修复前运行：稳定失败，`0 passed, 1 failed`，证明旧逻辑确实复活权威归档会话。
- 同一回归在修复后运行：通过，`1 passed`。
- `cargo test -p vulcan-service-kernel preserve_active_session_run_state`：7 项保留态定向测试全部通过。
- `npm run lint`：通过，TypeScript 项目检查无错误。
- `npm run build`：通过，2364 个模块完成生产构建；仅保留既有大 chunk 提示。
- `cargo test --workspace`：通过，63 个测试套件共 1560 项全部通过。
- `cargo clippy --workspace --all-targets -- -D warnings`：通过，全工作区零告警。
- 3 个本轮直接修改代码文件 `git diff --check`：通过，未发现空白错误。
- CodeKit 复核 3 个直接修改代码文件，共识别 27 个结构项，解析错误为 0。

### 遗留问题与注意事项

- 当前正在运行的旧进程已经持有双重活动/归档投影，必须由用户执行后续 `make br` 启动新构建后才能验证运行态清理；Codex 不执行该命令。
- 已归档真实数据不需要手工改库或移动目录，新进程会直接依据 SQLite 和归档目录构建唯一归档投影。
- 本轮没有提交、暂存或清理用户其他未提交变更。

## 第 54 轮：建立归档生命周期硬边界并修复项目级收敛（2026-07-10）

### 问题与事实依据

- 第 53 轮只阻止了“刷新结果已经包含 `archived_sessions` 墓碑”时的水合态复活，代码审核继续确认了四条未闭合路径：运行中会话仍可从右键菜单归档、项目删除墓碑不会进入 `archived_sessions`、批量归档与项目删除仍依赖全局 revision 快照、成功归档锁可能无限等待事件。
- 普通会话持久化原先会把调用方快照中的 `archived` 直接写回 `workspace_session_index`；活动任务在归档后创建的新持久化版本仍可能用旧快照把墓碑恢复为活动行。
- 项目删除后，加载器会跳过所属项目不存在的会话；若保留态只检查 `archived_sessions`，该会话仍会被重新插入成没有项目成员关系的孤儿活动行。
- Workbench 事件缓冲按 generation 只保留一条信封，因此项目移除的项目变化与全部子会话归档必须合并为单一领域事件，不能发送同 generation 的多条同级事件。

### 核心修复与调整概述

- 服务状态新增会话与项目生命周期预留集合；归档、批量归档和项目移除会在同一状态锁内检查活动 kernel turn 并预留目标，运行内核与提示词提交入口均拒绝跨越预留边界的新工作。
- 单会话归档的预留守卫放在真实归档服务入口；运行中会话归档会明确失败，右键菜单同步禁用运行中会话的归档项。
- WorkspaceStore 普通快照写入在文件写入前查询活动/归档定位行并校验生命周期；SQLite 冲突更新不再修改 `archived`，只有显式 archive/restore 生命周期接口可以改变该字段。
- 保留态恢复除检查归档墓碑外，还要求非空所属项目继续存在于权威活动项目集合；项目已经删除时不再恢复摘要、消息、运行态或选中态。
- 服务协议新增 `conversationsArchived` 与 `projectRemoved` 增量；批量归档使用一条包含全部子转换的事件，项目移除使用一条同时包含权威项目集合与全部子归档的事件。
- 前端会话与项目生命周期按目标标识独立记录 revision；全量 RPC 快照被全局 revision 拒绝时，只应用响应中目标自有的归档/项目字段，不覆盖无关的新状态。
- 归档成功锁现在可由 RPC 生命周期响应直接收敛；已接受但尚未完成 React 渲染的事件也会被识别为已调度，避免误报失败或永久锁死。

### 文件变更

- 修改：`crates/vulcan-service-api/src/conversation.rs`
- 修改：`crates/vulcan-service-kernel/src/project_conversation_services.rs`
- 修改：`crates/vulcan-service-kernel/src/project_conversation_services/conversation_services.rs`
- 修改：`crates/vulcan-service-kernel/src/project_conversation_services/project_services.rs`
- 修改：`crates/vulcan-service-kernel/src/conversation_submission.rs`
- 修改：`crates/vulcan-service-kernel/src/session_runtime_kernel.rs`
- 修改：`crates/vulcan-service-kernel/src/session_hydration/run_state_preservation.rs`
- 修改：`crates/vulcan-service-kernel/src/state.rs`、`lib.rs`、`rpc_dispatch.rs` 及状态测试构造器
- 修改：`crates/vulcan-workspace-store/src/workspace_store/persistence/index.rs`
- 修改：`crates/vulcan-workspace-store/src/workspace_store/persistence/snapshot.rs`
- 修改：`apps/vulcan-code-web/src/types/conversation.ts`
- 修改：`apps/vulcan-code-web/src/state/workbench-state.ts`
- 修改：`apps/vulcan-code-web/src/state/workbench-message-merge.ts`
- 修改：`apps/vulcan-code-web/src/components/workspace/workbench-layout.tsx`
- 修改：`apps/vulcan-code-web/src/components/workspace/workbench-layout/project-actions.tsx`
- 修改：`apps/vulcan-code-web/src/components/project/project-sidebar-menus.tsx`
- 修改：`TASK_LOG.md`
- 新增：无
- 删除：无

### 关键测试覆盖

- 新增项目删除保留态回归，验证所属项目消失后不会复活孤儿活动会话。
- 新增生命周期预留守卫回归，验证项目及子会话预留会阻止新 kernel turn，并在所有退出路径释放。
- 新增普通持久化生命周期矩阵回归，覆盖新活动会话、活动更新、归档更新以及活动/归档相互转换拒绝。
- 新增批量会话归档单信封回归和项目移除单 generation 回归，验证传输协议不会丢失同批子转换。
- 新增运行内核生命周期预留回归，验证任何新用户回合都不能跨越归档或移除事务。

### 验证结果与测试方式

- `cargo test -p vulcan-service-kernel -p vulcan-workspace-store`：通过，492 项全部通过。
- `cargo test --workspace --quiet`：最终完整复跑通过，共 1566 项测试全部通过。
- 完整工作区第一次复跑出现未修改模块的并发时序测试 `streaming_tool_executor_yields_tool_started_before_tool_body_runs` 抖动失败；该测试单独复跑通过，随后全工作区复跑 1566 项全部通过。
- `cargo clippy --workspace --all-targets -- -D warnings`：通过，全工作区零告警。
- `npm run lint`：通过，TypeScript 项目检查无错误。
- `npm run build`：通过，2364 个模块完成生产构建；仅保留既有大 chunk 提示。
- `cargo fmt --all --check`：通过。
- 24 个本轮直接相关代码文件 `git diff --check`：通过；全仓检查仍会报告用户既有 `MemoryArea.tsx` 文件末尾空行，与本轮无关，未擅自修改。
- CodeKit 复核 12 个核心协议、服务、存储与前端状态文件，共识别 253 个结构项，解析错误为 0。

### 遗留问题与注意事项

- 自动化测试、静态检查和结构复核已经闭环；桌面端真实交互仍需用户执行后续 `make br` 后验收。
- 本轮没有执行 `make br`，没有提交、暂存、清理或覆盖用户其他未提交变更。

## 第 55 轮：修复新会话首次提交后选择回跳（2026-07-10）

### 根因事实链

- 新建会话在首次提交前只是前端本地草稿；提交请求把 `conversation_id` 置空，后端随后生成真实会话标识。
- 后端会先提交工作区快照并发送 `conversationSummaryChanged`，最后返回包含真实会话标识的提交 ACK；但 RPC 返回、父级全量快照和前端增量进入 React 的渲染先后并不固定。
- 前端收到 ACK 后原先立即清空草稿并切换到真实标识；若该时刻侧栏摘要尚未进入本地会话集合，后续快照选择校验会把“不在列表中的真实标识”判为失效并回退到上一次会话。
- `conversationSummaryChanged` 协议已经携带 `selectedConversationId`，但前端归并函数只写入会话摘要，完全忽略该选择字段，导致后端已确认的新选择不能参与状态收敛。
- 原提交回调还会无条件清空当前草稿；请求等待期间若用户已经创建另一条草稿，较早请求的完成结果会误删较新的草稿。

### 核心修复与调整概述

- 为草稿首次提交增加显式的“已持久化、等待侧栏摘要”过渡标识；在真实摘要可见前，全量快照和增量选择校验都不会把该会话回退到旧会话。
- 后端增量先到达时立即结束过渡；ACK 先到达时则在权威侧栏行出现后结束过渡，避免保护标识残留并干扰后续归档或选择切换。
- 提交完成后的选择更新改为函数式状态校验，仅当用户仍停留在发起提交的草稿上时才切换到真实会话，避免异步完成结果抢回用户焦点。
- 草稿清理改为按提交时的精确草稿标识执行，不再删除请求期间新建的其他草稿。
- `conversationSummaryChanged` 归并现在应用协议自有的 `selectedConversationId`，并重新派生选中会话相关投影，补齐后端到前端的选择状态契约。

### 文件变更

- 修改：`apps/vulcan-code-web/src/components/workspace/workbench-layout.tsx`
- 修改：`apps/vulcan-code-web/src/components/workspace/workbench-layout/conversation-actions.ts`
- 修改：`apps/vulcan-code-web/src/state/workbench-state.ts`
- 修改：`TASK_LOG.md`
- 新增：无
- 删除：无

### 验证结果与测试方式

- `npm run lint`：通过，TypeScript 严格项目检查无错误。
- `npm run build`：通过，2364 个模块完成生产构建；仅保留既有大 chunk 提示。
- 3 个本轮直接修改代码文件 `git diff --check`：通过，未发现空白错误，仅有仓库既有的 Windows 行尾转换提示。
- CodeKit 对提交动作、工作台选择状态与增量归并 3 个文件进行结构复核，解析错误为 0。

### 遗留问题与注意事项

- 桌面端需要用户执行后续 `make br` 后进行真实时序验收；建议覆盖首次提交、快速切换、连续新建以及新会话后续归档四种场景。
- 本轮没有执行 `make br`，没有提交、暂存、清理或覆盖用户其他未提交变更。

## 第 56 轮：分离新会话实时创建与老会话历史水合（2026-07-10）

### 根因事实链

- 新会话首次提交后已经进入实时运行，用户提示词和后续模型输出通过增量转录直接进入前端；该会话在创建时不存在需要读取的历史窗口。
- 前端原先只把本地草稿排除在水合流程外；草稿转成真实会话标识后，会因为缺少 `hydratedConversationWindows` 记录而立即调用 `service_session_messages`，错误进入老会话历史加载流程。
- 后端 `apply_hydration_to_selected_workspace_snapshot` 又把显示层正文加载与 Provider 提交上下文共用同一个 `ensure_hydrated_session_dialogue_context` 强校验。
- 新会话首次运行期间，显示转录可以合法可用，但 Provider 对话状态可能尚未完成持久化；显示水合因此返回 `ERR_SESSION_DIALOGUE_CONTEXT_NOT_READY`，前端将错误投影成 SYSTEM 消息并短暂显示加载界面。

### 核心修复与调整概述

- 前端新增当前生命周期内的本地创建会话集合；草稿提交成功后，真实会话标识立即登记为实时创建会话。
- 实时创建会话的正文被视为已经由当前流提供，不再触发 `service_session_messages` 历史水合，因此首次对话直接进入运行界面，不显示老会话加载页。
- 老会话、重启后重新进入的会话仍使用原历史水合流程；本地创建标记不会持久化到下一次启动。
- 会话归档、项目移除等活动生命周期结束后会清理本地创建标记；会话恢复后会重新作为老会话执行历史水合，避免跳过真实历史读取。
- 后端显示层水合不再要求 Provider 对话上下文已经就绪，只同步严格提交就绪标记；提交和运行时水合路径仍保留原有强校验，不降低后续模型请求的上下文完整性要求。

### 文件变更

- 修改：`apps/vulcan-code-web/src/components/workspace/workbench-layout.tsx`
- 修改：`apps/vulcan-code-web/src/components/workspace/workbench-layout/conversation-actions.ts`
- 修改：`crates/vulcan-service-kernel/src/session_hydration/loading.rs`
- 修改：`crates/vulcan-service-kernel/src/session_hydration/tests.rs`
- 修改：`TASK_LOG.md`
- 新增：无
- 删除：无

### 验证结果与测试方式

- `cargo test -p vulcan-service-kernel session_hydration`：通过，3 项水合定向测试全部通过。
- `cargo test -p vulcan-service-kernel`：通过，357 项服务内核测试全部通过。
- `cargo clippy -p vulcan-service-kernel --all-targets -- -D warnings`：通过，零告警。
- `cargo fmt --all --check`：通过。
- `npm run lint`：通过，TypeScript 严格项目检查无错误。
- `npm run build`：通过，2364 个模块完成生产构建；仅保留既有大 chunk 提示。
- CodeKit 复核 4 个新旧会话状态与水合核心文件，共识别 24 个结构项，解析错误为 0。

### 遗留问题与注意事项

- 桌面端真实交互需用户执行后续 `make br` 后验收：新会话首次提交应直接显示实时对话；老会话首次打开仍应显示正常加载过程；归档后恢复的会话应重新加载历史。
- 本轮没有执行 `make br`，没有提交、暂存、清理或覆盖用户其他未提交变更。

## 第 57 轮：消除新会话转正帧的空会话占位闪烁（2026-07-10）

### 问题与根因

- 草稿提交 ACK 已返回真实会话标识，但权威会话摘要尚未完成 React 渲染时，当前界面短暂处于“有选中标识、无会话摘要、无消息”的转正帧。
- `ConversationWorkspace` 将该帧误判为普通未选中状态，渲染“尚未选择会话，等待工作区数据载入。”SYSTEM 占位卡片。
- 会话摘要和首条转录到达后条件自然消失，因此该卡片只闪现一帧，常规截图难以捕获。

### 核心修复与调整概述

- `ChatArea` 新增精确的转正帧识别：存在真实选中标识、尚无对应摘要且当前不是本地草稿。
- 该帧直接复用 `ConversationWorkspace` 已有的 `suppressEmptyPlaceholder` 能力，只隐藏默认 SYSTEM 空占位卡片。
- 不修改顶部标题、正常草稿空状态、老会话水合加载页和真实无选择状态的既有行为。

### 文件变更

- 修改：`apps/vulcan-code-web/src/components/conversation/chat-area.tsx`
- 修改：`TASK_LOG.md`
- 新增：无
- 删除：无

### 验证结果与测试方式

- `npm run lint`：通过，TypeScript 严格项目检查无错误。
- `npm run build`：通过，2364 个模块完成生产构建；仅保留既有大 chunk 提示。
- CodeKit 复核 `ChatArea`，识别 5 个结构项，解析错误为 0。

### 遗留问题与注意事项

- 桌面端需用户执行后续 `make br` 并通过录像逐帧验收；转正间隙应保持内容区域空白，直到首条实时转录出现。
- 本轮没有执行 `make br`，没有提交、暂存、清理或覆盖用户其他未提交变更。

## 第 58 轮（失败实验，已撤销）：尝试让输入区位移动画避开超长转录提交阻塞（2026-07-10）

### 问题与根因

- 录像逐帧对照确认：新会话与短会话之间双向切换均能看到输入区上下移动，但新会话与超长会话之间双向切换都会直接跳到终点。
- 输入区原样式使用固定 `bottom 360ms` 过渡；会话正文没有有效虚拟化，超长转录切换会同步提交大量节点并执行滚动位置测量与恢复。
- 动画时长没有根据 `scrollHeight` 计算；真实问题是 CSS 过渡在长转录提交时已经开始计时，但主线程在首帧绘制前持续阻塞，浏览器获得绘制机会时 360ms 窗口已经结束。
- 已撤销此前在布局 effect 内立即启动 Web Animations 的无效尝试：它虽然改用 FLIP 坐标，但动画时钟仍与长转录后续布局工作重叠，不能改变首帧被阻塞的根因。

### 核心修复与调整概述

- `ComposerDock` 仅在 `emptyMode` 发生真实变化时记录上一个已绘制位置与新布局终点，不受普通转录增量渲染影响。
- 新布局提交后先用同步 `transform` 把输入区固定在旧的可见坐标，再通过两级 `requestAnimationFrame` 保证该起点至少完成一次绘制。
- 360ms Web Animations 位移动画从下一绘制帧开始计时，因此超长转录的 DOM 提交和布局阻塞不会预先消耗动画窗口。
- 中途反向切换会从当前可见坐标继续；组件卸载、方向反转和待启动帧均显式取消，避免遗留内联变换或孤立动画。
- CSS 移除与新机制冲突的 `bottom` 过渡，只保留工作区水平偏移所需的 `transform` 过渡；减少动态属性范围，不引入内容高度或候选路径兜底。

### 文件变更

- 修改：`apps/vulcan-code-web/src/components/composer/composer-dock.tsx`
- 修改：`apps/vulcan-code-web/src/styles/composer-layout.css`
- 修改：`TASK_LOG.md`
- 新增：无
- 删除：无

### 验证结果与测试方式

- `npm run lint`：通过，TypeScript 严格项目检查无错误。
- `npm run build`：通过，2364 个模块完成生产构建；仅保留既有大 chunk 提示。
- 2 个本轮代码文件 `git diff --check`：通过，未发现空白错误，仅有仓库既有的 Windows 行尾转换提示。
- CodeKit 复核 `ComposerDock` 及对应 CSS 定位点，组件 AST 解析错误为 0；确认动画时长与会话 `scrollHeight` 没有数据依赖。

### 遗留问题与注意事项

- 桌面端录像实测确认本方案无效：新会话与超长会话双向切换仍然直接跳变，没有恢复可见位移动画。
- 所有动画无反应的切换都伴随浏览器性能告警 `Forced reflow while executing JavaScript took 40ms`；该一致性证据说明同步布局读取链路尚未查明，不能继续把问题归结为动画计时启动过早。
- 本轮 `ComposerDock` 双帧延迟 Web Animations 与 CSS 调整已经按实验前 Git 基线完整撤销，代码中不保留无实际效果的实现。
- 后续应先增加纯文本浏览器诊断日志，单独记录会话切换、输入区模式变化、布局读取、帧回调和 CSS transition 生命周期，再依据同一次切换的时间线定位唯一阻塞点。
- 本轮没有执行 `make br`，没有提交、暂存、清理或覆盖用户其他未提交变更。

## 第 59 轮：增加会话位移动画 Forced Reflow 纯文本诊断（2026-07-10）

### 诊断目标与事实依据

- 第 58 轮双帧延迟动画经桌面录像实测无效并已完整撤销，不能继续修改动画实现后再凭视觉结果猜测。
- 所有无动画切换均伴随浏览器 `Forced reflow while executing JavaScript took 40ms` 告警，需要把同一次用户切换、React 提交、浏览器 transition 生命周期与既有同步布局读取关联到一条时间线。
- 当前已确认的同步布局读取来源包括：滚动视口距离与锚点恢复、用户提示词溢出测量、轮次外壳高度测量、折叠内容高度测量和输入区高度测量。

### 核心调整概述

- 新增会话动画诊断模块；每次选择会话或新建会话分配单调递增的 `switch` 编号，并开启 6 秒诊断窗口。
- 所有日志使用固定 `[conversation-motion]` 前缀和 `key=value` 纯文本格式，仅向浏览器控制台传入一个字符串，不使用 JSON 序列化。
- `ChatArea` 记录界面提交、消息数量、空会话状态、第一绘制帧与第二绘制帧；不为诊断额外读取任何布局属性。
- `ComposerDock` 记录空会话模式提交以及 `transitionrun`、`transitionstart`、`transitionend`、`transitioncancel` 原生事件，从浏览器事实判断 `bottom` 过渡是否真正启动和完成。
- 既有同步布局操作通过统一计时器记录；只有耗时达到 1ms 才输出，避免大量廉价行测量和控制台写入反过来干扰性能。
- 计时点覆盖滚动锚点、尾部锁定、用户提示词溢出、轮次外壳、折叠内容和输入区高度；包装器只包围原操作，不新增 `scrollHeight`、`getBoundingClientRect` 或 `getComputedStyle` 调用。

### 实测诊断结论

- 正常短会话双向切换均出现 `composer-mode-commit`，随后浏览器依次发出 `composer-transitionrun`、`composer-transitionstart` 与 `composer-transitionend`，且 `bottom` 的实际 elapsed 为 360ms。
- 异常超长会话双向切换均完全缺失 `composer-mode-commit` 和全部 transition 事件；这证明输入区不是在原节点上切换模式，而是随父级布局子树卸载后以终点模式重新挂载。
- 超长会话进入时还记录到用户提示词溢出测量 17.9ms、视口锚点捕获 14.1ms和浏览器 Forced Reflow 56ms；这些是长转录挂载成本，但没有 transition 事件已经先行证明它们不是动画消失的直接机制。
- 结构追踪确认 `WorkspaceResizableBody` 会过滤 `panel=null` 的工具面板描述：超长会话存在 Git、终端或子代理工具面板时，`ChatArea` 位于 `AdaptiveToolPanel` 内；切到没有工具面板的新会话时该包装层被移除，导致整个主内容子树改变父级身份。

### 文件变更

- 新增：`apps/vulcan-code-web/src/components/conversation/conversation-motion-diagnostics.ts`
- 修改：`apps/vulcan-code-web/src/components/composer/composer-dock.tsx`
- 修改：`apps/vulcan-code-web/src/components/conversation/chat-area-scroll.ts`
- 修改：`apps/vulcan-code-web/src/components/conversation/chat-area.tsx`
- 修改：`apps/vulcan-code-web/src/components/conversation/conversation-measured-collapse.tsx`
- 修改：`apps/vulcan-code-web/src/components/conversation/conversation-render-items.tsx`
- 修改：`apps/vulcan-code-web/src/components/conversation/conversation-workspace.tsx`
- 修改：`apps/vulcan-code-web/src/components/workspace/workbench-layout.tsx`
- 修改：`apps/vulcan-code-web/src/components/workspace/workbench-layout/project-actions.tsx`
- 修改：`TASK_LOG.md`
- 删除：无

### 验证结果与测试方式

- `npm run lint`：通过，TypeScript 严格项目检查无错误。
- `npm run build`：通过，2365 个模块完成生产构建；仅保留既有大 chunk 提示。
- 9 个诊断相关代码文件 `git diff --check`：通过，未发现空白错误，仅有仓库既有的 Windows 行尾转换提示。
- CodeKit 复核 9 个诊断、切换、输入区和会话布局文件，共识别 75 个结构项，解析错误为 0。
- 桌面测试时在浏览器控制台过滤 `conversation-motion`，清空旧日志后只执行一次失败切换；复制同一 `switch` 从 `switch-request` 到最后一个 `chat-*frame` 或 `composer-transition*` 的全部纯文本行，并保留相邻 Forced Reflow 告警。

### 遗留问题与注意事项

- 本轮只增加诊断，没有修改 `bottom` 动画时长、缓动、定位方式或会话滚动行为；必须依据实测日志确定唯一慢布局读取后再实施根因修复。
- 定位完成后应删除本轮临时诊断代码，但保留本轮日志结论和最终命中的调用链，避免以后重复排查。
- 本轮没有执行 `make br`，没有提交、暂存、清理或覆盖用户其他未提交变更。

## 第 60 轮：稳定会话工具面板外壳并保留输入区组件身份（2026-07-10）

### 根因修复

- `WorkspaceShell` 已经始终为聊天页面提供一个会话工具面板描述，只在没有 Git、终端或子代理内容时把 `panel` 设为 `null`。
- `WorkspaceResizableBody` 原先再次过滤空面板描述，使主内容在“直接挂载”和“挂载于 AdaptiveToolPanel”两种父级拓扑之间切换；React 因此无法保留 `ChatArea` 与 `ComposerDock` 身份。
- 移除该过滤步骤后，描述槽外壳始终挂载，`panel` 内容仍按现有条件为空或出现；新会话与带工具面板的超长会话之间切换时，主内容保持同一 React 父级链。
- 本轮没有修改输入区 `bottom` 过渡、动画时长、缓动或滚动恢复逻辑；修复点是造成动画根本无法创建的组件身份断点。

### 文件变更

- 修改：`apps/vulcan-code-web/src/components/workspace/workspace-resizable-body.tsx`
- 修改：`TASK_LOG.md`
- 新增：无
- 删除：无

### 验证结果与测试方式

- `npm run lint`：通过，TypeScript 严格项目检查无错误。
- `npm run build`：通过，2365 个模块完成生产构建；仅保留既有大 chunk 提示。
- CodeKit 复核 `WorkspaceResizableBody`、`WorkspaceShell` 与 `AdaptiveToolPanel`，共识别 17 个结构项，解析错误为 0。
- 桌面复测时异常路径应首次出现 `composer-mode-commit fromEmpty=false toEmpty=true` 或反向记录，并产生完整的 `bottom` transitionrun/start/end 事件；这同时证明输入区实例没有重挂载。

### 遗留问题与注意事项

- 桌面实测确认超长会话双向切换动画已经恢复，第 59 轮临时 `conversation-motion` 模块、计时包装器、切换日志和 transition 监听均已删除；任务日志保留诊断证据与根因结论。
- Forced Reflow 属于超长转录挂载的独立性能问题；若组件身份修复后动画恢复，应在后续单独治理用户提示词逐行测量和视口锚点扫描，不能再与动画缺失混为同一个问题。
- 本轮没有执行 `make br`，没有提交、暂存、清理或覆盖用户其他未提交变更。

## 第 61 轮：清理临时动画诊断与工作区加载阶段日志（2026-07-10）

### 清理范围

- 删除前端 `conversation-motion` 诊断模块及其所有调用点，包括切换编号、布局耗时计时、绘制帧记录和输入区 transition 事件监听。
- 删除服务启动时 `[webui-service] workspace index load begin/complete` 两条阶段进度输出。
- 删除工作区存储加载中的 begin、项目查询、默认项目播种、会话索引查询、会话投影、折叠项目查询和快照组装完成等阶段进度输出。
- 随会话投影进度日志删除只为输出墓碑数量而存在的计数变量；缺失项目会话仍按原逻辑直接跳过。
- 保留中断会话生命周期恢复等具有实际运维意义的异常日志，没有静默删除错误或恢复信息。
- 保留第 60 轮 `AdaptiveToolPanel` 描述槽稳定外壳修复，动画恢复所依赖的组件身份边界不变。

### 文件变更

- 修改：`crates/vulcan-service-kernel/src/service_bootstrap.rs`
- 修改：`crates/vulcan-workspace-store/src/workspace_store/loading.rs`
- 修改：`TASK_LOG.md`
- 删除：`apps/vulcan-code-web/src/components/conversation/conversation-motion-diagnostics.ts`

### 验证结果

- CodeKit 全前端反向搜索 `conversation-motion`、诊断计时器和切换追踪符号：零命中。
- CodeKit 全仓反向搜索指定工作区加载阶段文本：零命中。
- `npm run lint`：通过，TypeScript 严格项目检查无错误。
- `npm run build`：通过，生产构建恢复为 2364 个模块；仅保留既有大 chunk 提示。
- `cargo check -p vulcan-service-kernel -p vulcan-workspace-store`：通过。
- `cargo fmt --all --check`：通过。

### 遗留问题与注意事项

- Forced Reflow 性能治理尚未在本轮展开；本轮只按用户要求清除已完成定位使命的输出，不混入新的性能优化。
- 本轮没有执行 `make br`，没有提交、暂存、清理或覆盖用户其他未提交变更。

## 第 62 轮：让右侧上下文面板首帧直接使用预判布局模式（2026-07-10）

### 问题与根因

- `WorkspaceShell.openRightContextPanel` 在点击打开前已经根据当前外壳宽度预判 `rightPanelFloating`，该结果与 `rightPanelOpen` 在同一用户动作中提交。
- `WorkspaceResizableBody` 又维护独立的 `rightPanelTransition.floating` 缓存，并通过 effect 与动画帧异步同步父级目标。
- 打开期间的停靠占位、浮动类名和面板 `floating` 属性原先全部读取内部缓存，因此首次可见帧可能先沿用旧的停靠模式压缩主会话，再切换到父级已经预判出的浮动模式。

### 核心修复

- 新增 `renderedRightPanelFloating`：面板打开期间直接使用 `props.rightPanelFloating`，关闭退出期间才使用内部过渡缓存保留原方向。
- 停靠宽度占位、停靠挂载判断、`has-floating-context`、`is-floating` 和 `WorkspaceContextPanel.floating` 统一读取同一个最终模式。
- 内部 `rightPanelTransition` 继续负责 mounted/visible 的进入退出动画，不再决定打开期间的布局类型。
- 未修改面板宽度阈值、主会话最小宽度和动画时长。

### 文件变更

- 修改：`apps/vulcan-code-web/src/components/workspace/workspace-resizable-body.tsx`
- 修改：`TASK_LOG.md`
- 新增：无
- 删除：无

### 验证结果与测试方式

- `npm run lint`：通过，TypeScript 严格项目检查无错误。
- `npm run build`：通过，2364 个模块完成生产构建；仅保留既有大 chunk 提示。
- CodeKit 复核 `WorkspaceResizableBody` 与预判来源 `WorkspaceShell`，共识别 12 个结构项，解析错误为 0。
- `git diff --check`：通过，未发现空白错误，仅有仓库既有的 Windows 行尾转换提示。
- 桌面复测应覆盖空间充足与空间不足两种宽度：空间充足时直接停靠展开；空间不足时第一帧直接浮动，不再先压缩主会话再弹回。

### 遗留问题与注意事项

- 桌面真实动画仍需用户执行后续 `make br` 后验收。
- 本轮没有执行 `make br`，没有提交、暂存、清理或覆盖用户其他未提交变更。

## 第 63 轮：联动 Git 工具框与浮动上下文面板（2026-07-10）

### 问题与根因

- 主会话 Git 工具框由 `ConversationSessionTodolistPanel` 渲染，但布局模式由外层通用 `AdaptiveToolPanel` 根据自身宽度独立判断。
- 右侧上下文面板已经打开代码审核等页签并处于浮动模式时，该状态没有进入会话工具面板描述，因此 Git 工具框仍可能先按内联占位展开，再因空间变化切换为浮动。
- Git 工具框展开宽度原先固定为 320px，短分支名和紧凑操作仍占用完整宽度。

### 核心修复

- `WorkspacePagePanelDescriptor` 与 `AdaptiveToolPanel` 新增 `forceOverlay` 契约；外部布局可以明确禁止面板占用主内容宽度。
- 会话工具面板在 `effectiveRightPanelOpen && rightPanelFloating` 时预先进入强制浮动态，无论工具框此前折叠、展开、内联还是普通浮动。
- `WorkspaceResizableBody` 在上下文面板挂载动画开始前发布 `has-floating-context`，使 Git 工具框首帧就定位到浮动上下文面板左侧。
- 浮动上下文面板宽度收口到共享 CSS 变量；Git 工具框右边缘使用该真实宽度加 12px 间距，避免相互覆盖。
- Git 工具框改为内容驱动宽度，默认最小 260px、最大保持原 320px；文本统一保持单行，过长内容沿既有行样式省略。

### 文件变更

- 修改：`apps/vulcan-code-web/src/components/workspace/workspace-page-descriptor.ts`
- 修改：`apps/vulcan-code-web/src/components/workspace/adaptive-tool-panel.tsx`
- 修改：`apps/vulcan-code-web/src/components/workspace/workspace-resizable-body.tsx`
- 修改：`apps/vulcan-code-web/src/components/workspace/workspace-shell.tsx`
- 修改：`apps/vulcan-code-web/src/styles/workspace-layout.css`
- 修改：`apps/vulcan-code-web/src/styles/skill-management.css`
- 修改：`apps/vulcan-code-web/src/styles/session-tools.css`
- 修改：`TASK_LOG.md`
- 新增：无
- 删除：无

### 验证结果与测试方式

- `npm run lint`：通过，TypeScript 严格项目检查无错误。
- `npm run build`：通过，2364 个模块完成生产构建；仅保留既有大 chunk 提示。
- CodeKit 复核 4 个布局契约与状态核心文件，共识别 19 个结构项，解析错误为 0。
- 7 个本轮代码与样式文件 `git diff --check`：通过，未发现空白错误，仅有仓库既有的 Windows 行尾转换提示。
- 桌面复测需覆盖：Git 工具框独立展开、右侧浮动窗口已打开代码审核后展开 Git 工具框、Git 工具框已展开后打开浮动窗口，以及右侧窗口关闭后的模式恢复。

### 遗留问题与注意事项

- 当前 Git 工具框最大宽度仍为 320px，未扩大既有占用；极窄窗口下会受浮动上下文面板剩余空间约束并使用既有省略显示。
- 本轮没有执行 `make br`，没有提交、暂存、清理或覆盖用户其他未提交变更。

## 第 64 轮（失败，已回退）：尝试在绘制前重测 Git 工具框布局（2026-07-10）

### 问题与根因

- 第 63 轮只在右侧上下文面板已经判定为浮动时通过 `forceOverlay` 固定 Git 工具框模式，没有覆盖右侧面板带选项卡但仍处于停靠状态的情况。
- `AdaptiveToolPanel` 原先在普通 `useEffect` 中测量是否具备内联占位空间；右侧停靠面板改变主区域宽度后，点击展开的首次绘制仍可能沿用折叠阶段缓存的 `reservesContent`，先拉开主会话，再由 effect 或 `ResizeObserver` 切回浮动。
- 第 63 轮为贴靠浮动上下文面板增加的 `max-width` 规则会额外压缩 Git 工具框，与“贴边只改变位置、不改变宽度”的最终要求不一致。

### 核心修复

- 将占位模式测量切换为 `useLayoutEffect`，并把 `expanded` 纳入依赖；每次展开或折叠都在浏览器绘制前按当前真实根节点宽度重新确认模式。
- 保留第 63 轮已经有意义的 `forceOverlay` 与贴靠定位契约，未继续叠加新的模式分支。
- 删除贴靠场景对工具插槽和菜单的额外宽度限制；贴靠规则现在只更新 `right`，Git 工具框继续使用自身内容驱动宽度、260px 默认下限和 320px 原有上限。

### 文件变更

- 修改：`apps/vulcan-code-web/src/components/workspace/adaptive-tool-panel.tsx`
- 修改：`apps/vulcan-code-web/src/styles/skill-management.css`
- 修改：`apps/vulcan-code-web/src/styles/session-tools.css`
- 修改：`TASK_LOG.md`
- 新增：无
- 删除：无

### 验证结果与测试方式

- `npm run lint`：通过，TypeScript 严格项目检查无错误。
- `npm run build`：通过，2364 个模块完成生产构建；仅保留既有大 chunk 提示。
- CodeKit 复核 `AdaptiveToolPanel`，识别 5 个结构项，解析错误为 0。
- 3 个目标文件 `git diff --check`：通过，未发现空白错误，仅有仓库既有的 Windows 行尾转换提示。
- 桌面复测重点：右侧停靠面板已有代码审核等选项卡时展开 Git 工具框，首帧应直接选择最终布局；右侧浮动面板时工具框应贴靠其左侧且宽度与独立展开时一致。

### 遗留问题与注意事项

- 用户桌面实测确认：右侧存在审核、记忆、打开文件等任意选项卡时，仍会先压缩主会话，再切换浮动并让主会话弹回；因此绘制前重新测量不是根因。
- 已撤销本轮 `useLayoutEffect`、`expanded` 依赖与对应说明注释，避免保留无效时序改动。
- “贴边不改变 Git 工具框宽度”是独立且明确的最终需求，相关宽度约束删除继续保留，不随失败实验回退。
- 本轮没有执行 `make br`，没有提交、暂存、清理或覆盖用户其他未提交变更。

## 第 65 轮：右侧选项卡面板拉弹流程的状态链诊断（2026-07-10）

### 当前事实与诊断目标

- 第 64 轮实测失败并回退后，源码确认右侧上下文面板的浮动目标由 `WorkspaceShell` 计算，实际停靠或浮动类由 `WorkspaceResizableBody` 渲染，两者之间还存在独立的进入退出过渡状态。
- 仅凭视觉无法区分“点击预判先给出停靠、随后响应式观察器改成浮动”和“预判始终为浮动、但过渡渲染仍短暂输出停靠”这两条互斥路径。
- 本轮只增加纯文本诊断输出，不修改宽度阈值、浮动规则、动画时长或布局状态。

### 诊断输出

- `open-preflight`：记录点击展开前的外壳宽度、主面板真实宽度、左栏状态、选项卡数量、Git 工具框展开状态及预判目标。
- `responsive-mode-change`：仅在外壳响应式计算准备改变停靠或浮动状态时输出，记录改变前后模式和对应宽度。
- `body-commit`：记录右侧主体每次相关提交时的属性目标、内部过渡模式、最终渲染模式、挂载/可见状态以及选项卡数量。
- 所有输出均使用 `[context-layout]` 前缀和单行文本格式，不使用 JSON。

### 文件变更

- 修改：`apps/vulcan-code-web/src/components/workspace/workspace-shell.tsx`
- 修改：`apps/vulcan-code-web/src/components/workspace/workspace-resizable-body.tsx`
- 修改：`TASK_LOG.md`
- 新增：无
- 删除：无

### 验证结果与下一步

- `npm run lint`：通过，TypeScript 严格项目检查无错误。
- `npm run build`：通过，2364 个模块完成生产构建；仅保留既有大 chunk 提示。
- 下一次桌面复现只需提供从 `open-preflight` 开始到最后一个 `body-commit` 的连续日志，即可唯一判断错误发生在预判状态还是过渡渲染。
- 根因确认并完成修复后必须删除本轮临时输出，但保留本日志中的诊断过程与结论。

### 遗留问题与注意事项

- 用户提供的运行时日志证明：`open-preflight` 首次目标就是 `floating`；第一次主体提交已经满足 `propMode=floating`、`renderedMode=floating`、`dockedMounted=false`、`dockedOpen=false`。右侧上下文面板从未进入停靠布局。
- 日志同时确认复现时 `gitExpanded=true`。结合 `.adaptive-tool-panel-content-shell` 固定存在的 260ms padding 过渡，根因确定为 Git 工具框从内嵌占位切换到强制浮动时，主会话 padding 仍继续动画，而不是右侧面板状态计算或过渡渲染错误。
- 本轮临时 `[context-layout]` 输出已在第 66 轮修复时完整删除；CodeKit 反向搜索确认零残留。
- 本轮没有执行 `make br`，没有提交、暂存、清理或覆盖用户其他未提交变更。

## 第 66 轮（失败，已回退）：切断 Git 工具框强制浮动时的主会话 padding 回弹（2026-07-10）

### 根因

- 右侧上下文面板打开前已经正确预判为浮动，且首次提交没有挂载任何停靠面板。
- 当 Git 工具框此前处于展开内嵌状态时，右侧上下文面板打开会同步给 Git 工具框施加 `is-forced-overlay`，布局类在第一次提交即已正确切换。
- 通用 `.adaptive-tool-panel-content-shell` 对 `padding-left` 和 `padding-right` 固定设置了 260ms transition；因此即使状态已经是浮动，主会话仍会从旧的内嵌 padding 缓慢回到 14px，形成“先被压缩、随后弹回”的假象。

### 核心修复

- 仅在右侧上下文浮动且会话 Git 工具框处于强制浮动时，禁用主内容外壳的 padding transition。
- Git 工具框自身的宽高过渡继续保留，右侧上下文面板的滑入滑出动画继续保留。
- 不调整 Git 工具框宽度、最大宽度、贴边距离、右侧面板浮动阈值或任何 React 状态时序。
- 删除第 65 轮全部 `[context-layout]` 临时诊断输出和仅为日志增加的宽度读取。

### 文件变更

- 修改：`apps/vulcan-code-web/src/styles/skill-management.css`
- 修改：`apps/vulcan-code-web/src/components/workspace/workspace-shell.tsx`（删除临时日志）
- 修改：`apps/vulcan-code-web/src/components/workspace/workspace-resizable-body.tsx`（删除临时日志）
- 修改：`TASK_LOG.md`
- 新增：无
- 删除：无

### 验证结果与测试方式

- `npm run lint`：通过，TypeScript 严格项目检查无错误。
- `npm run build`：通过，2364 个模块完成生产构建；仅保留既有大 chunk 提示。
- CodeKit 反向搜索 `context-layout`、`open-preflight`、`responsive-mode-change`、`body-commit`：零命中。
- 桌面重点复测：先展开 Git 工具框，再让右侧面板保留审核、记忆或文件任意选项卡并点击展开；右侧面板应直接浮动滑入，主会话不再经历 260ms 的压缩后回弹。

### 遗留问题与注意事项

- 用户补充实测：不存在 Git 的会话同样复现，而右侧面板没有任何选项卡时，无论是否存在 Git 工具框都正常。这证明 Git 工具框不是必要条件，第 66 轮只命中了伴随现象而非根因。
- 已撤销强制浮动时禁用主会话 padding transition 的 CSS，避免保留无效行为修改；第 63 轮贴边和宽度调整不受影响。
- 稳定触发条件已收敛为“右侧面板存在活动选项卡”，后续定位转向选项卡挂载副作用和实际 DOM 几何变化。
- 本轮没有执行 `make br`，没有提交、暂存、清理或覆盖用户其他未提交变更。

## 第 67 轮：限制活动页签自动滚动的作用域（2026-07-10）

### 根因

- 右侧面板没有选项卡时，`WorkspaceContextPanel` 不执行活动页签定位 effect；存在审核、记忆、文件或子代理任意活动页签时，该 effect 都会在面板挂载后调用 `scrollIntoView()`。
- 第 65 轮日志证明：活动页签组件挂载所在的第二次提交中，浮动面板为 `mounted=true`、`visible=false`。此时 `.workspace-context-panel.is-floating.is-closed` 正通过 transform 将整个面板平移到窗口右侧之外。
- 对屏幕外活动页签调用 `scrollIntoView()` 会请求浏览器滚动所有可滚动祖先，而不仅是页签条自身；它因此与浮动面板进入动画争抢外层滚动和布局位置，产生主会话先被挤压再弹回的视觉过程。
- 这条路径只依赖是否存在活动页签，与 Git 工具框无关，完整解释了用户补充的正反场景。

### 核心修复

- 删除活动页签元素上的 `scrollIntoView()`。
- 通过活动页签与 `.context-dynamic-tab-strip` 的矩形差值计算局部目标位置，只调用页签条自身的 `scrollTo()`。
- 保留平滑居中活动页签的原有体验，同时阻止滚动传播到主会话、工作区外壳或窗口。
- 不改右侧面板浮动预判、挂载状态、动画时长、Git 工具框宽度及贴边规则。

### 文件变更

- 修改：`apps/vulcan-code-web/src/components/workspace/WorkspaceContextPanel.tsx`
- 修改：`TASK_LOG.md`
- 新增：无
- 删除：无

### 验证结果与测试方式

- `npm run lint`：通过，TypeScript 严格项目检查无错误。
- `npm run build`：通过，2364 个模块完成生产构建；仅保留既有大 chunk 提示。
- CodeKit 结构复核 `WorkspaceContextPanel.tsx`：识别 25 个结构项，解析错误为 0。
- 桌面复测矩阵：右侧无选项卡、审核页签、记忆页签、文件页签分别关闭后重新打开；同时覆盖存在 Git 工具框与不存在 Git 工具框的会话。

### 遗留问题与注意事项

- 桌面真实动画仍需用户执行后续 `make br` 后验收；如果本轮仍无效，应撤销局部 `scrollTo()` 替换并保留本日志结论，再增加 DOM 几何帧采样。
- 本轮没有执行 `make br`，没有提交、暂存、清理或覆盖用户其他未提交变更。

## 第 68 轮：收尾清理与调试残留审计（2026-07-10）

### 清理结论

- 用户桌面确认第 67 轮修复有效，活动页签局部 `scrollTo()` 作为最终根因修复保留。
- 第 64 轮 `useLayoutEffect` 重测实验和第 66 轮强制浮动 padding transition 覆盖均已从代码中完全回退，只保留失败记录，避免重复试错。
- 第 65 轮 `[context-layout]` 运行时诊断、此前 `[conversation-motion]` 动画诊断和工作区加载阶段进度输出均已清除。
- 全仓仍存在一条 `[workspace-load] recovering interrupted session lifecycle`，该输出只在真实的中断会话生命周期恢复时触发，属于异常恢复运维信息，不是临时调试输出，因此保留。

### 保留变更及依据

- `WorkspaceContextPanel` 的局部页签条滚动：用户已实测解决有活动选项卡时的拉弹问题。
- `forceOverlay` 描述契约和右侧浮动目标首帧传递：用于保证右侧上下文面板浮动时 Git 工具框同步浮动并在首次可见帧使用最终模式。
- Git 工具框内容驱动宽度、260px 默认下限、320px 原有上限及不换行：对应用户明确提出的独立界面要求。
- 共享浮动上下文宽度变量和贴靠定位：只复用同一宽度值确定相邻位置，不额外压缩 Git 工具框。

### 文件审计

- 保留修改：`apps/vulcan-code-web/src/components/workspace/WorkspaceContextPanel.tsx`
- 保留修改：`apps/vulcan-code-web/src/components/workspace/adaptive-tool-panel.tsx`
- 保留修改：`apps/vulcan-code-web/src/components/workspace/workspace-page-descriptor.ts`
- 保留修改：`apps/vulcan-code-web/src/components/workspace/workspace-resizable-body.tsx`
- 保留修改：`apps/vulcan-code-web/src/components/workspace/workspace-shell.tsx`
- 保留修改：`apps/vulcan-code-web/src/styles/session-tools.css`
- 保留修改：`apps/vulcan-code-web/src/styles/skill-management.css`
- 保留修改：`apps/vulcan-code-web/src/styles/workspace-layout.css`
- 新增：无
- 删除：无

### 验证与注意事项

- CodeKit 全仓反向搜索临时日志前缀：`context-layout`、`conversation-motion` 和工作区加载阶段进度输出均为零命中。
- CodeKit 确认剩余 `[workspace-load]` 唯一命中属于 `required_session_directory_for_index_row` 的中断生命周期恢复分支。
- 本轮没有执行 `make br`，没有提交、暂存、清理或覆盖用户其他未提交变更。

## 第 69 轮：补齐 Git 工具框贴靠浮动面板的位移动画（2026-07-10）

### 问题与根因

- Git 工具框进入强制浮动状态时，通过 `right` 从工作区右缘移动到浮动上下文面板左侧。
- 会话工具插槽原先只对 `width` 和 `height` 声明 transition，`right` 变化因此直接跳到终点，表现为瞬移。

### 核心修复

- 为会话工具插槽补充 `right 220ms cubic-bezier(0.2, 0.72, 0.22, 1)`。
- 动画时长和缓动与右侧上下文面板的滑入滑出保持一致。
- 不调整 Git 工具框自身宽度、最大宽度、内容布局或最终贴靠位置。

### 文件变更

- 修改：`apps/vulcan-code-web/src/styles/skill-management.css`
- 修改：`TASK_LOG.md`
- 新增：无
- 删除：无

### 验证与注意事项

- 桌面复测需覆盖右侧浮动面板打开和关闭两个方向，Git 工具框应平滑移动到贴靠位置并平滑返回。
- 本轮没有执行 `make br`，没有提交、暂存、清理或覆盖用户其他未提交变更。

## 第 70 轮：连续工具调用显示组合与流式顺序修复（2026-07-10）

### 任务目标

- 对显示层面连续出现的多个工具调用生成一个默认折叠、可点击展开的纯展示组合器。
- 思考显示策略已经隐藏的思考记录不应打断组合；可见思考、Assistant 正文、用户或系统记录必须结束当前工具序列并保持在折叠外。
- 修复首次思考已经先开始，但工具边界补丁短暂删除思考，导致工具显示在思考之前的顺序错误。

### 根因与核心修复

- `ConversationProcessGroup` 原先直接渲染思考策略过滤后的过程行，没有对相邻工具执行行做二次显示组合。
- 新增显示层 `groupVisibleProcessBodyEntries`：仅把至少两条连续、且全部块都属于真实工具执行证据的记录组合；单条工具与所有非工具记录保持原顺序。
- 新增 `ConversationToolSequenceGroup`：默认折叠，复用虚拟滚动安全的 `ConversationMeasuredCollapse` 与展开记忆，摘要显示工具调用数量。
- 运行时思考 delta 直接进入 WebUI 工作台消息，但稳定会话记录在工具事件投影阶段有意排除流式草稿；旧逻辑用稳定记录重建窗口时会先删掉当前思考，再追加工具，后续检查点恢复思考后顺序倒置。
- 新增 `merge_visible_streaming_rows_with_runtime_projection`：在工具事件补丁中原位替换稳定记录，同时保留已经可见但尚未进入稳定存储的 thinking-output 与 assistant-output，最后再追加新工具记录。

### 文件变更

- 修改：`apps/vulcan-code-web/src/components/conversation/conversation-render-items.tsx`
- 修改：`apps/vulcan-code-web/src/styles/special-blocks.css`
- 修改：`apps/vulcan-code-web/src/i18n-messages.ts`
- 修改：`crates/vulcan-service-kernel/src/agent_stream/projection.rs`
- 修改：`TASK_LOG.md`
- 新增：无
- 删除：无

### 验证结果与测试方式

- `npm run lint`：通过。
- `npm run build`：通过，2364 个模块完成生产构建；仅保留既有大 chunk 提示。
- `cargo test -p vulcan-service-kernel runtime_tool_projection_ --lib`：3 项相关测试全部通过，包含新增的可见流顺序回归测试。
- `cargo fmt --all -- --check`：通过。
- CodeKit 复核前端组合器与后端投影文件：识别 46 个结构项，解析错误为 0。
- `git diff --check`：通过，未发现空白错误，仅有仓库既有的 Windows 行尾转换提示。
- 桌面复测应覆盖：纯连续工具、被隐藏思考隔开的工具、可见思考隔开的工具、Assistant 正文隔开的工具，以及首段思考后立即调用第一个工具的实时顺序。

### 遗留问题与注意事项

- 工具组合器按可见工具执行“行数”计数；同一工具调用的 call/result 已在更早投影阶段合并为一行，不会重复计数。
- 本轮没有执行 `make br`，没有提交、暂存、清理或覆盖用户其他未提交变更。

## 第 71 轮：连续工具组合器的运行态分层（2026-07-10）

### 任务目标

- 连续工具序列执行期间，摘要同时显示已经结束的调用数量和仍在运行的工具数量。
- 已结束的工具继续位于折叠区；运行中工具始终在摘要下方展开显示，结束后自动收入折叠区。

### 根因与核心修复

- 原组合器只按“是否为工具行”分组，将运行中和已结束工具一起放进默认折叠区，导致用户看不到正在执行的工具。
- 直接复用已有统一状态判定 `toolExecutionDisplayState(blocks, sessionRunning)`，不按文案或单一 `status` 字段推测；该判定覆盖工具结果、命令、子智能体以及失败、停止等终态。
- `groupVisibleProcessBodyEntries` 现在把每段连续工具序列拆为终态记录和运行中记录：终态记录进入可测量折叠区，运行中记录保持在摘要下方。
- 运行态摘要新增“已执行 {completed} 次调用，正在调用 {running} 个工具”；全部完成后自动恢复“已执行 {count} 次工具调用”。
- 当序列尚无已完成记录时，摘要仅承担状态展示，不出现无意义的折叠箭头；首条工具完成后折叠入口自动生效。

### 文件变更

- 修改：`apps/vulcan-code-web/src/components/conversation/conversation-render-items.tsx`
- 修改：`apps/vulcan-code-web/src/styles/special-blocks.css`
- 修改：`apps/vulcan-code-web/src/i18n-messages.ts`
- 修改：`TASK_LOG.md`
- 新增：无
- 删除：无

### 验证结果与测试方式

- `npm run lint`：通过。
- `npm run build`：通过，2364 个模块完成生产构建；仅保留既有大 chunk 提示。
- CodeKit 复核组合器与语言包结构：识别 15 个结构项，解析错误为 0。
- `git diff --check`：通过，未发现空白错误，仅有仓库既有的 Windows 行尾转换提示。
- 桌面复测应连续启动至少两个工具，确认运行期间摘要计数实时变化、活动工具保持可见，完成后对应行自动收入折叠区；随后展开折叠区确认原始工具顺序和内容完整。

### 遗留问题与注意事项

- 单条孤立工具调用继续沿用普通工具行展示，不额外制造组合摘要。
- 本轮没有执行 `make br`，没有提交、暂存、清理或覆盖用户其他未提交变更。

## 第 72 轮：移除底栏中央工具调用提示区域（2026-07-10）

### 任务目标

- 删除底部状态栏中央的“工具调用中”提示胶囊。
- 删除该提示专用的中央布局区域，不保留隐藏占位。

### 根因与核心修复

- 原底栏使用左、中、右三列，中央列由 `StatusbarRuntimeActivities` 渲染普通工具运行徽标。
- 完整删除底栏工具状态的派生函数、专用数据类型、渲染组件、语言键和样式，不采用仅隐藏元素的方式。
- 底栏改为左右两列，版本信息继续靠左，仓库链接继续靠右，中央区域不再参与布局。
- 同步删除工作台中仅供该徽标使用的会话消息投影计算，避免无界面消费者的重复解析。

### 文件变更

- 修改：`apps/vulcan-code-web/src/components/workspace/workbench-layout.tsx`
- 修改：`apps/vulcan-code-web/src/components/conversation/conversation-runtime.ts`
- 修改：`apps/vulcan-code-web/src/components/conversation/conversation-model.ts`
- 修改：`apps/vulcan-code-web/src/components/shell/app-chrome.tsx`
- 修改：`apps/vulcan-code-web/src/styles/special-blocks.css`
- 修改：`apps/vulcan-code-web/src/i18n-messages.ts`
- 修改：`TASK_LOG.md`
- 新增：无
- 删除：无

### 验证结果与测试方式

- 全仓反向搜索 `statusbar-runtime-activit`、`runtime.toolActivity`、`StatusbarRuntimeActivities`、`buildStatusbarRuntimeActivities`、`StatusbarRuntimeActivity`：零命中。
- `npm run lint`：通过。
- `npm run build`：通过，2364 个模块完成生产构建；仅保留既有大 chunk 提示。
- 桌面复测时启动工具调用，底栏应始终只显示左侧版本信息和右侧仓库入口，不再出现中央胶囊或中央留白列。

### 遗留问题与注意事项

- 输入区上方的其他运行状态提示不属于本轮删除范围，继续保留既有语义。
- 本轮没有执行 `make br`，没有提交、暂存、清理或覆盖用户其他未提交变更。

## 第 73 轮：全局运行态文字统一迁移至 shadcn shimmer（2026-07-10）

### 任务目标

- 删除项目自建的 `ShinyText` 闪光实现，统一使用 shadcn/ui 提供的 `shimmer` utility class。
- 覆盖正在请求、会话加载、正在思考、运行中工具和含运行中工具的组合摘要。
- 工具进入完成、失败或停止等终态后立即取消 shimmer。
- 审计 `runtime.toolRunning` 是否存在真实运行时调用。

### 当轮处理与后续判定

- 当轮确认项目在全局样式入口导入了 `shadcn/tailwind.css`，但遗漏了对已安装 `shadcn` 版本中是否真实包含 `shimmer` 的检查。
- 删除 `ShinyText` 组件、`.shiny-text` 样式、自定义文字渐变和 `shiny-text` keyframes，避免两套闪光效果并存。
- 正在请求、会话水合提示与浮动思考标题直接使用 `className="shimmer"`。
- 普通工具行、命令执行、Markdown 工具、ask、文件变更和子智能体展示均以现有 `running` 状态条件添加 shimmer；终态渲染不包含该 class。
- 工具组合摘要仅在 `runningCount > 0` 时添加 shimmer，最后一个运行中工具完成后自动取消。
- `runtime.toolRunning` 全仓只有语言键类型声明及中英文翻译值，没有任何组件或运行时函数调用；本轮按审计要求保留该键，未擅自删除。

### 文件变更

- 修改：`apps/vulcan-code-web/src/components/composer/composer-dock.tsx`
- 修改：`apps/vulcan-code-web/src/components/conversation/conversation-blocks.tsx`
- 修改：`apps/vulcan-code-web/src/components/conversation/conversation-chrome.tsx`
- 修改：`apps/vulcan-code-web/src/components/conversation/conversation-render-items.tsx`
- 修改：`apps/vulcan-code-web/src/components/conversation/conversation-tool-execution.tsx`
- 修改：`apps/vulcan-code-web/src/components/conversation/conversation-tool-presentation-card.tsx`
- 修改：`apps/vulcan-code-web/src/styles/composer-layout.css`
- 修改：`apps/vulcan-code-web/src/styles/special-blocks.css`
- 删除：`apps/vulcan-code-web/src/components/ui/shiny-text.tsx`
- 修改：`TASK_LOG.md`
- 新增：无

### 当轮验证结果与失效原因

- 全局反向搜索 `ShinyText`、`shiny-text` 与旧 keyframes：源代码零命中。
- 当轮“构建产物包含 `.shimmer`”的检查实际只命中了项目局部选择器 `.streaming-thinking-title .shimmer` 等，未验证 `.shimmer` 本体是否包含 `animation: tw-shimmer`；这是一次误判。
- `npm run lint`：通过。
- `npm run build`：通过，2363 个模块完成生产构建；仅保留既有大 chunk 提示。
- CodeKit 复核 6 个主要渲染文件：识别 84 个结构项，解析错误为 0。
- `git diff --check`：通过，仅有仓库既有的 Windows 行尾转换提示。
- 桌面复测失败：请求提示、运行中工具和工具组合摘要均没有光效；后续第 74 轮确认根因是项目仍使用不包含 shimmer utility 的 `shadcn@4.9.0`。

### 遗留问题与注意事项

- `runtime.toolRunning` 当轮确认是未被消费的孤立语言键，并在后续第 74 轮按用户要求删除。
- 工具图标原有的运行态脉冲不属于文字 shimmer，本轮予以保留。
- 本轮没有执行 `make br`，没有提交、暂存、清理或覆盖用户其他未提交变更。

## 第 74 轮：升级 shadcn 并修复 shimmer utility 缺失（2026-07-10）

### 问题与根因

- UI 已正确渲染 `class="shimmer"`，但请求提示、运行中工具和组合摘要均没有动画。
- 项目安装的 `shadcn@4.9.0` 中，`dist/tailwind.css` 全文不存在 `shimmer`、`tw-shimmer` 或相关 utility 定义。
- 上一轮构建验证只匹配到项目局部 `.shimmer` 选择器，未要求规则正文包含 `animation: tw-shimmer`，因此产生了错误通过结论。
- 当前 npm 最新版 `shadcn@4.13.0` 已包含完整 shimmer utility。

### 核心修复

- 使用与现有依赖树一致的 Corepack pnpm v11.11.0，将 `shadcn` 从 `4.9.0` 升级到 `4.13.0`，同步更新 `package.json`、`pnpm-lock.yaml` 和本地依赖链接。
- 新版 `dist/tailwind.css` 已确认包含 `@utility shimmer`、`@keyframes tw-shimmer`、RTL 反向动画以及 `prefers-reduced-motion` 降级。
- 通过最新版 CLI `shadcn info` 复核 Vite、Tailwind v4、radix-nova preset、路径别名和全局 CSS 配置均能正确解析。
- 对 17 个已安装 shadcn 组件执行全量 dry-run：9 个与上游完全一致；其余差异包含桌面端 `sonner`、tooltip 焦点恢复抑制、sidebar 等项目定制，未使用 `--overwrite` 破坏本地功能。
- 按用户要求删除无消费者的 `runtime.toolRunning` 类型声明及中英文翻译值。

### 文件变更

- 修改：`apps/vulcan-code-web/package.json`
- 修改：`apps/vulcan-code-web/pnpm-lock.yaml`
- 修改：`apps/vulcan-code-web/src/i18n-messages.ts`
- 修改：`TASK_LOG.md`
- 新增：无
- 删除：无

### 验证结果与测试方式

- `corepack pnpm exec shadcn --version`：输出 `4.13.0`。
- `corepack pnpm exec shadcn info`：项目配置、preset、alias、全局 CSS 与 17 个已安装组件均成功识别。
- `npm run lint`：通过。
- `npm run build`：通过，2363 个模块完成生产构建；仅保留既有大 chunk 提示。
- 最终构建 CSS 精确匹配 `.shimmer{...animation:tw-shimmer var(--shimmer-duration,2s) linear infinite...}`。
- 最终构建 CSS 精确匹配 `@keyframes tw-shimmer{0%{background-position:100% 0}to{background-position:0 0}}`。
- 全仓反向搜索 `runtime.toolRunning`：零命中。
- `git diff --check`：通过，仅有仓库既有的 Windows 行尾转换提示。

### 遗留问题与注意事项

- 系统启用“减少动态效果”时，shadcn 官方 utility 会按设计关闭动画并恢复静态文字。
- 本地 open-code 组件不能用 `shadcn add --overwrite` 全量覆盖，否则会删除已确认的桌面端定制并引入不适用于 Vite 的 `next-themes`。
- 本轮没有执行 `make br`，没有提交、暂存、清理或覆盖用户其他未提交变更。

## 第 75 轮：修复 checkpoint 将思考重新追加到工具之后（2026-07-10）

### 问题与根因

- 实时过程中首段思考最初位于工具之前，但 checkpoint 或完成态到达后，思考重新移动到工具组合之后，并被持久化为错误顺序。
- 工具事件会先进入 `workspace.session_messages`；思考流记录位于 `runtime_state.conversation_state.records`。
- 第 70 轮的 `merge_visible_streaming_rows_with_runtime_projection` 只修复实时 Workbench 消息补丁，不改变保留层 `session_messages` 的顺序。
- checkpoint 调用共享函数 `merge_session_messages_with_conversation_state_records` 时，旧实现复制已有工具消息，再把尚不存在的思考记录统一追加到末尾，造成“工具 → 思考”。
- 原测试只覆盖“保留历史并追加全新运行态片段”，没有覆盖“部分工具已经存在、运行态补入更早思考”的交错合并场景。

### 核心修复

- 将运行态 `state.records` 明确作为当前运行片段的规范顺序。
- 共享合并函数现在以第一个现有/运行态共享记录为锚点：保留锚点前的加载历史，随后写入完整规范运行态顺序，再保留运行态未拥有的显示层尾部记录。
- 不按时间戳、文案或记录种类猜测顺序；唯一依据是运行态记录数组的既有顺序和稳定消息标识。
- checkpoint、终态持久化、运行中快照持久化及重启水合都复用该共享函数，因此修复覆盖完整生命周期。
- 新增回归测试，模拟显示层先出现两个工具，而规范运行态为“思考 1 → 工具 1 → 思考 2 → 工具 2”，并验证工具内容会更新、历史前缀和显示尾部不会丢失。

### 文件变更

- 修改：`crates/vulcan-context/src/protocol/projection.rs`
- 修改：`crates/vulcan-context/src/protocol/tests.rs`
- 修改：`TASK_LOG.md`
- 新增：无
- 删除：无

### 验证结果与测试方式

- 首次定向单测编译失败：`projected_ids` 借用了 `projected_messages`，后续无法移动规范记录数组；已改为拥有字符串的 `BTreeSet<String>`，没有保留无效借用方案。
- `cargo test -p vulcan-context merge_session_messages_with_conversation_state_records --lib`：2 项通过。
- `cargo test -p vulcan-context --lib`：31 项全部通过。
- `cargo test -p vulcan-service-kernel runtime_tool_projection_ --lib`：3 项全部通过。
- `cargo test -p vulcan-service-kernel checkpoint --lib`：1 项通过。
- `cargo test -p vulcan-workspace-store effective_session_messages_for_persistence_preserves_display_history --lib`：1 项通过。
- `cargo fmt --all -- --check`：通过。
- CodeKit 复核共享投影与测试文件：识别 48 个结构项，解析错误为 0。
- `git diff --check`：通过，仅有仓库既有的 Windows 行尾转换提示。

### 遗留问题与注意事项

- 第 70 轮实时 Workbench 顺序修复仍有独立价值：它负责 checkpoint 到达前的即时显示；本轮共享合并修复负责 checkpoint、持久化和重启后的规范顺序，两者覆盖不同阶段。
- 桌面复测必须新建一轮包含至少两段思考和多次工具调用的会话，并在执行完成、切换会话及重启后分别确认顺序保持一致。
- 本轮没有执行 `make br`，没有提交、暂存、清理或覆盖用户其他未提交变更。

## 第 76 轮：删除用户智能体目录的旧版平铺扫描（2026-07-10）

### 问题与事实依据

- `crates/vulcan-task-session/src/catalog.rs` 同时负责智能体定义模型、系统/用户/包目录扫描、Markdown 与 sidecar 解析、写回、路径安全、兼容转换和单元测试；结构扫描识别到 3463 行、126 个符号，是尚未治理的高复杂度模块。
- 当前用户目录契约已经明确分为 `definitions/`、`packages/` 和 `overrides/`，但加载主流程仍额外调用 `scan_legacy_flat_subagent_definition_root`，把应用数据 `agents/` 根目录直下的任意 Markdown 当作智能体定义。
- 全仓调用追踪确认该旧扫描器只有加载主流程一个调用点；当前保存入口统一通过 `user_subagent_definition_path` 写入 `definitions/<id>.md`，没有任何生产写入者继续生成根目录平铺定义。
- 系统内置资源位于 `crates/vulcan-runtime-assets/resources/subagents/<category>/`，下载包位于用户目录 `packages/`，当前资源布局同样不依赖旧版平铺用户文件。
- 旧扫描会让用户放在 `agents/` 根目录的说明文档或 README 被错误解析、参与标识冲突并产生加载错误，扩大了目录输入边界。

### 已查明的执行流程

1. `RuntimeFoundation::build_with_task_runtime` 使用系统资源目录和 `<application_data>/agents` 构建智能体目录，再交给 `TaskRuntime::replace_subagent_catalog` 作为运行时定义来源。
2. 服务端目录摘要、保存、启停等入口通过 `load_current_subagent_catalog` 复用相同的系统根和用户根，因此加载器的目录边界同时影响运行时和管理界面。
3. `load_subagent_catalog_from_roots` 先递归扫描系统根，再扫描用户 `packages/`，随后扫描用户 `definitions/`，旧实现还会扫描用户根目录平铺 Markdown，最后应用 `overrides/` sidecar。
4. `service_save_subagent_definition` 最终调用 `write_user_subagent_definition`；该函数通过 `user_subagent_definition_path` 和安全相对路径校验固定写入 `definitions/`，同时写入配套 sidecar。
5. 因此根目录平铺扫描只有历史读取价值，不属于任何当前创建、编辑、启停或运行时刷新流程。

### 长期优化调整

- 从 `load_subagent_catalog_from_roots` 删除用户根目录平铺扫描步骤，当前目录只接受系统根、用户 `packages/`、用户 `definitions/` 和用户 `overrides/` 四个明确来源。
- 完整删除私有函数 `scan_legacy_flat_subagent_definition_root` 及其专用错误文案，不保留迁移、候选路径或双读兜底。
- 将原用户/系统标识冲突测试迁移到正式 `definitions/` 布局，继续覆盖用户定义不能遮蔽系统定义的规则。
- 新增独立回归测试 `ignores_markdown_files_outside_structured_user_directories`，确认用户根目录 Markdown 既不会产出定义，也不会产出加载错误。

### 文件变更

- 修改：`crates/vulcan-task-session/src/catalog.rs`
- 修改：`TASK_LOG.md`
- 新增：无
- 删除：无

### 修改范围代码审核与自动修复

- 首版回归验证把“正式用户定义与系统定义冲突”和“根目录 Markdown 被忽略”合并在同一测试中，失败时无法直接定位破坏的契约；审核后已自动拆成两个单一职责测试。
- 审核确认运行时初始化、服务目录加载和用户定义保存使用同一 `<application_data>/agents` 根，且保存路径唯一落在 `definitions/`，不存在需要保留平铺读取的当前数据来源。
- CodeKit 全仓反向搜索确认 `scan_legacy_flat_subagent_definition_root` 与 `Failed to read legacy subagent catalog root` 均为零命中，新回归测试存在唯一结构化归属。
- Clippy 以 `-D warnings` 审核修改 crate 的库和测试目标，未发现警告或可自动修复项。

### 验证结果

- 修改前基线 `cargo test -p vulcan-task-session --lib`：46 项全部通过。
- 自动修复后 `cargo test -p vulcan-task-session --lib`：47 项全部通过。
- `cargo clippy -p vulcan-task-session --lib --tests -- -D warnings`：通过，零告警。
- `cargo fmt --all -- --check`：通过。
- `git diff --check`：通过，未发现空白错误。
- CodeKit AST 复查：`catalog.rs` 识别 126 个结构项，解析错误为 0；旧扫描函数已从结构树消失，两个目录边界测试均能独立解析。

### 遗留问题与下一轮方向

- `catalog.rs` 仍把解析、扫描、sidecar 转换和路径规则集中在单文件内；后续应沿已证实的职责边界继续拆分，不能只为降低行数移动代码。
- 文件内仍有 `policy_snapshot_from_legacy_lists`、`legacy_tool_policy_lists_from_exposure` 等旧策略兼容链；下一轮需先核对当前 sidecar 写入格式、已有系统资源和服务请求结构，再决定是否删除，不能仅凭命名推断为死代码。
- 本轮没有提交、暂存或覆盖用户其他未提交变更。

## 第 77 轮：收口智能体能力策略并隔离分族工具覆盖（2026-07-10）

### 问题与事实依据

- `catalog.rs` 同时保留统一 `CapabilityPolicySnapshot`、旧 `allowed/disabled` 列表、旧逐能力三态 map，以及从 `luaskills_overrides`、`mcp_server_overrides` 反推全局工具策略的四套解释路径。
- `normalize_tool_exposure` 在统一 policy 为默认值时，会把 LuaSkills/MCP 分族 override 中的选择器收集进全局 allow/deny 列表，再生成统一策略；这会把“只允许某个 LuaSkill”错误提升成全局 `allow_only`，从而让无关 Native Tool 被全局过滤。
- 运行时 `ToolExposureSelectorState` 已分别解析统一全局 policy、LuaSkills override 和 MCP override，分族载荷本来就有独立消费路径；目录层再次合并不仅冗余，而且改变语义。
- WebUI `saveRequestFromDraft` 当前始终发送 `toolPolicyMode + toolPolicyTools` 和 `promptSkillPolicyMode + promptSkillPolicySkills`，并把旧 allow/deny 列表置空；服务层再将显式 mode 与 selectors 归一为 `CapabilityPolicySnapshot`。
- 当前 sidecar 写入器固定输出现代 `toolPolicy: { mode, tools }` 与 `promptSkillPolicy: { mode, skills }`；系统资源目录全量搜索未发现旧策略 map、旧列表或分族 override 策略字段，因此不存在当前资源依赖。

### 已查明的执行流程

1. Agents 管理界面从目录摘要中的统一 policy 构建编辑草稿，保存时显式提交 mode 与 selectors，旧列表字段为空。
2. `service_save_subagent_definition` 通过 `apply_request_tool_policy` 和 `apply_request_prompt_skill_policy` 把请求归一为共享 policy，同时生成运行时镜像列表。
3. `write_user_subagent_definition` 将 Markdown 与 YAML sidecar 分开写入；全局工具策略写入 `toolPolicy`，提示词技能策略写入 `promptSkillPolicy`，LuaSkills/MCP 分族载荷写入 `toolExposure`。
4. 目录重载时，`parse_subagent_definition_text_with_metadata` 先取得 sidecar 分族载荷，再应用独立全局 policy，最后执行子代理禁止工具清理。
5. 旧实现会在最后规范化阶段重新读取分族 JSON，并把其中的 allow/deny 选择器提升为全局 policy；运行时随后又按分族规则解析一次，造成目录层与执行层职责重叠。

### 长期优化调整

- `normalize_tool_exposure` 与 `normalize_prompt_skills` 现在只信任显式 `CapabilityPolicySnapshot`，不再从旧镜像列表或分族 JSON 推导策略。
- 删除 `policy_snapshot_from_legacy_lists`、`policy_snapshot_from_state_map`、`legacy_tool_policy_lists_from_exposure` 及四个旧 JSON selector 解析函数。
- 删除旧逐能力 `SubagentPolicyState`、untagged policy 枚举和 `selectors/tools` 字段别名；`toolPolicy` 与 `promptSkillPolicy` 只接受当前 mode-plus-list 对象。
- 删除 sidecar 中旧 `promptSkills` 快照输入；Markdown frontmatter 仍可通过共享快照中的显式 policy 声明当前策略。
- 新增专用 `SubagentToolExposureSidecar`，持久化层只允许 AI 能力字段、LuaSkills override 和 MCP override，类型上禁止嵌套全局 policy 与旧 allowed/disabled 字段绕过 `toolPolicy`。
- 更新两项旧策略测试为现代 YAML 对象，并新增分族 override 往返测试与旧格式拒绝测试。
- `catalog.rs` 从第 76 轮后的 3458 行、126 个结构项降到 3363 行、120 个结构项；行数以 Git 基线与当前文件逐行计数为准。

### 文件变更

- 修改：`crates/vulcan-task-session/src/catalog.rs`
- 修改：`TASK_LOG.md`
- 新增：无
- 删除：无

### 修改范围代码审核与自动修复

- 首轮实现虽删除了旧推导，但 sidecar 仍直接复用运行时 `ToolExposureSnapshot`，类型上还能接受嵌套全局 policy；审核后新增专用 sidecar DTO，彻底隔离持久化与运行时结构。
- 审核发现提示词技能策略只有旧格式拒绝测试，没有现代 `{mode, skills}` 正向往返证据；已扩展现有持久化测试，同时验证空 `allow_only` 工具策略和 `deny_only` 提示词技能策略。
- 删除旧 `promptSkills` 字段后，完整 sidecar 初始化器中的 `..SubagentDefinitionSidecar::default()` 失去作用；Clippy 以 `-D warnings` 捕获后已自动移除。
- CodeKit 反向搜索确认旧策略转换函数、旧三态类型、旧对象类型、旧 sidecar 辅助函数与旧测试名称全部零命中。
- 审核确认专用 DTO 使用 `deny_unknown_fields`，旧 `toolExposure.policy`、`allowed_tools` 与 `disabled_tools` 会被明确拒绝，而不是静默改变全局策略。

### 验证结果

- 修改前基线 `cargo test -p vulcan-task-session --lib`：47 项全部通过。
- 最终 `cargo test -p vulcan-task-session --lib`：49 项全部通过。
- `cargo test -p vulcan-agent-runtime execution_tool_exposure_ --lib`：7 项通过，覆盖全局选择器与 LuaSkills/MCP 分族选择器运行时语义。
- `cargo test -p vulcan-service-kernel apply_request_ --lib`：1 项通过，覆盖保存请求统一策略与子代理禁止工具清理。
- `cargo clippy -p vulcan-task-session --lib --tests -- -D warnings`：通过，零告警。
- `cargo fmt --all -- --check`：通过。
- `git diff --check`：通过，未发现空白错误。
- CodeKit AST 复查：`catalog.rs` 识别 120 个结构项，解析错误为 0；专用 sidecar DTO、两个现代 policy 对象和 49 项测试均可正常解析。

### 遗留问题与下一轮方向

- 共享 `ToolExposureSnapshot` 和服务 API 仍保留 allowed/disabled 镜像字段，运行时也保留 policy 缺失时的列表回退；它们已不再进入当前目录 sidecar，但是否能在跨 crate 范围彻底删除需要单独追踪任务调用参数、会话配置叠加与外部 API 消费后再决定。
- `catalog.rs` 仍同时承担解析、目录扫描、路径安全、sidecar DTO 与测试；下一轮可优先评估把 sidecar 协议与转换逻辑拆成独立模块，前提是保持清晰的公开 API 边界，而不是机械拆文件。
- 本轮没有提交、暂存或覆盖用户其他未提交变更。

## 第 78 轮：抽离智能体能力策略 sidecar 与规范化模块（2026-07-10）

### 问题与事实依据

- 第 77 轮已形成清晰的能力策略边界，但三个 sidecar DTO、DTO/运行时转换、工具与提示词技能规范化仍全部位于 `catalog.rs`，与目录扫描、Markdown 解析、路径安全和类别元数据交错。
- CodeKit 调用追踪确认 `SubagentToolExposureSidecar`、`SubagentToolPolicySidecar`、`SubagentPromptSkillPolicySidecar` 以及两个规范化函数只由 `catalog.rs` 的定义解析、sidecar 写入和覆盖应用流程消费，不属于 crate 公开 API。
- `catalog.rs` 在本轮开始时实际为 3363 行、120 个结构项；文件行数通过当前文件逐行计数确认，未沿用 CodeKit 目录树的旧缓存行数。
- 跨 crate 删除 `ToolExposureSnapshot` 与服务 API 的镜像字段会改变核心配置和公开 Schema，属于需要独立决策的范围；本轮只处理已经证实的私有职责边界。

### 已查明的执行流程

1. `parse_subagent_definition_text_with_metadata` 从 Markdown frontmatter 与 YAML sidecar 取得显式 policy 和分族工具载荷，组合为运行时快照后执行统一规范化。
2. `write_user_subagent_metadata_override` 与 `write_subagent_sidecar_to_path` 从运行时快照提取三个 sidecar DTO，分别序列化分族工具载荷、全局工具 policy 与提示词技能 policy。
3. `apply_sidecar_override` 将用户 override DTO 应用到已有定义，并在函数末端统一规范化工具和提示词技能快照。
4. 运行时后续只消费规范化后的 `ToolExposureSnapshot` 与 `PromptSkillSnapshot`，不依赖 DTO 的源码所在模块。
5. 因此 DTO、转换和规范化可以收口到 `catalog` 私有子模块，父模块只需要组合它们，不需要改变公开结构或 YAML 字段。

### 长期优化调整

- 新增私有模块 `catalog/capability_policy.rs`，集中持有三个 sidecar DTO、DTO/运行时转换、工具策略规范化、提示词技能策略规范化和字符串列表清理。
- `catalog.rs` 通过 `mod capability_policy` 和窄化 import 使用该模块；类型与函数均限制为 `pub(super)`，没有扩大 crate 公共 API。
- `CapabilityPolicyMode` 与 `CapabilityPolicySnapshot` 在父模块仅供测试使用，改为 `#[cfg(test)]` 导入，生产编译不再依赖无关符号。
- `catalog.rs` 从 3363 行降至 3177 行；新模块 192 行，总代码为 3369 行，新增的 6 行主要来自模块声明和双语模块契约。
- 结构复查中父文件降至 101 个结构项，新模块承载 17 个结构项，总结构项从 120 降至 118。

### 文件变更

- 新增：`crates/vulcan-task-session/src/catalog/capability_policy.rs`
- 修改：`crates/vulcan-task-session/src/catalog.rs`
- 修改：`TASK_LOG.md`
- 删除：无

### 修改范围代码审核与自动修复

- 初版抽取保留了两个只调用共享函数的 `apply_*` 包装器；审核进一步追踪发现 sidecar policy 会先应用一次，随后规范化再次应用，override 流程末端还会重复规范化。
- 已自动改为父流程只替换显式 `policy` 字段，解析流程规范化一次，override 流程继续在既有末端规范化一次；两个透传包装器随之删除。
- 新模块完整保留严格 sidecar DTO、分族载荷隔离、AI 能力列表去空去重以及子代理禁止工具清理，未复制第二套业务规则。
- `cargo fmt` 自动修正模块声明、crate import 与私有模块 import 的顺序。
- CodeKit 反向搜索确认三个 DTO 和两个规范化函数只在新模块定义；`catalog.rs` 仅保留真实调用点，两个旧 `apply_*` 包装函数零命中。

### 验证结果

- 修改前基线 `cargo test -p vulcan-task-session --lib`：49 项全部通过。
- 修改后 `cargo test -p vulcan-task-session --lib`：49 项全部通过。
- `cargo test -p vulcan-agent-runtime execution_tool_exposure_ --lib`：7 项通过。
- `cargo test -p vulcan-service-kernel apply_request_ --lib`：1 项通过。
- `cargo clippy -p vulcan-task-session --lib --tests -- -D warnings`：通过，零告警。
- `cargo fmt --all -- --check`：通过。
- `git diff --check`：通过，已跟踪文件无空白错误；新增模块同时通过 rustfmt、编译与 Clippy 检查。
- CodeKit AST 复查两个文件共识别 118 个结构项，解析错误为 0；新模块识别 3 个 DTO、3 个实现块、2 个规范化函数和 1 个私有列表清理函数。

### 遗留问题与下一轮方向

- `catalog.rs` 仍有 3177 行，下一轮应在 sidecar 读取/写入、目录扫描或路径标识规则中继续寻找具有独立不变量和可验证调用边界的职责域。
- 跨 crate 的 allowed/disabled 镜像字段涉及核心配置与服务 API Schema，不应与内部模块抽取混为一轮；若后续确认要彻底删除，应先形成影响分析并等待重大决策确认。
- 本轮没有提交、暂存或覆盖用户其他未提交变更。

## 第 79 轮：统一智能体目录路径规则并阻断不可移植标识（2026-07-10）

### 问题与事实依据

- `catalog.rs` 同时保存类别标识、定义标识、相对路径、安全写入路径、文件扩展名与顶层类别判断等多组路径函数；类别路径与定义路径各自重复拆分和校验，规则容易继续分叉。
- 原安全函数只拒绝 `.`、`..`、反斜杠和冒号，并会静默裁掉路径段首尾空白；它仍允许 `*`、`?`、双引号、尖括号、竖线、尾随点以及 Windows 设备保留名，不能保证目录在支持平台间可移植。
- 定义写入最终通过 `set_extension` 设置 Markdown 扩展名；原规则允许最终标识自带点号，因此输入中的最后一段可能被当作旧扩展名改写，而不是保持唯一、可逆的定义标识。
- 包扫描此前直接把磁盘目录名作为 package id，绕过了用户定义路径的安全边界。WebUI 对定义文件名只允许 ASCII 字母、数字、点、下划线、斜杠和连字符，对 package id 只允许 ASCII 字母、数字、点、下划线和连字符，服务层负责分隔符与扩展名规范化，目录层仍必须承担最终文件系统校验。
- 对 `crates/vulcan-runtime-assets/resources/subagents` 的完整文件名审计确认：现有内置资源没有超出 `[A-Za-z0-9_./-]` 的字符、没有多点号最终文件名，也没有 Windows 设备保留名；收紧规则不会破坏当前资源事实。

### 已查明的执行流程

1. WebUI 先规范化用户输入，服务请求再拆分目录分隔符并移除最终扩展名，随后调用任务会话目录写入接口。
2. 用户定义、sidecar 与用户覆盖的写入路径分别通过类别路径或定义路径安全函数生成相对路径，再拼接到受控根目录；这些函数是阻止路径越界和不可移植文件名进入磁盘的最终边界。
3. 目录加载流程扫描系统资源、用户资源与包资源，通过文件扩展名、相对目录和文件 stem 生成 canonical id；用户覆盖和类别镜像均依赖同一标识还原到定义。
4. `display_subagent_category_id` 是唯一跨模块使用的显示转换入口，由服务内核消费；其余路径函数只在目录实现内部使用，可以收口为私有子模块而不改变公开 API。
5. 包扫描以包根目录的直接子目录为 package id；因此包目录必须先通过同一可移植路径段规则，失败时记录明确加载错误并跳过，不能继续把原始目录名传播到 canonical id。

### 长期优化调整

- 新增私有模块 `catalog/path_rules.rs`，集中管理扩展名常量、类别与包目录标识、canonical id、相对路径转换、安全写入路径、文件 stem 和扩展名判断。
- `catalog.rs` 保留 `display_subagent_category_id` 的原公开路径，通过 re-export 维持调用方接口；其余函数限制为 `pub(super)` 或模块私有，路径不变量不再散落在父模块。
- 新增统一的可移植路径段规则：只允许 ASCII 字母、数字、连字符、下划线和点；拒绝空段、`.`、`..`、尾随点，以及不区分大小写的 `CON`、`PRN`、`AUX`、`NUL`、`COM1` 至 `COM9`、`LPT1` 至 `LPT9` 设备名及其带扩展名形式。
- 类别和定义安全路径现在复用同一解析入口，并显式拒绝首尾空白，不再静默修剪成另一个标识；定义最终段禁止包含点号，避免 `set_extension` 隐式改写用户标识。
- 包扫描新增 `package_id_for_directory` 校验；非法包目录会生成 `SubagentCatalogLoadError` 并被跳过，错误不会再被伪装成空目录或合法 package id。
- 新增 4 项路径模块单元测试，覆盖合法嵌套路径、非法与保留标识、类别/文件/包 canonical id 以及扩展名大小写；新增 1 项目录集成测试，验证非法包目录会被报告并跳过。
- `catalog.rs` 从 3177 行降至 3038 行；新路径模块 370 行。父模块减少 139 行，并把路径职责与测试移到具有单一安全契约的边界内。

### 文件变更

- 新增：`crates/vulcan-task-session/src/catalog/path_rules.rs`
- 修改：`crates/vulcan-task-session/src/catalog.rs`
- 修改：`TASK_LOG.md`
- 删除：无

### 修改范围代码审核与自动修复

- 初版抽取只覆盖用户定义、类别与 canonical id 路径；审核完整扫描链后发现 package id 仍直接来自磁盘目录名，会形成同类规则旁路，已自动补入包目录校验与集成回归测试。
- 审核逐一反向检查路径函数调用点，确认父模块只保留调用，所有路径规则定义均位于新模块；公开显示函数通过 re-export 保持原调用路径。
- 审核确认显示用类别转换只影响 UI 展示，不参与文件系统路径构造；其防御性空白清理不会掩盖写入标识，因为实际写入仍经过严格安全路径函数。
- `cargo fmt` 自动修正新模块、导入和测试代码格式；Clippy、编译与完整相关测试未发现需要继续修正的问题。
- CodeKit AST 复查三个目录实现文件共识别 128 个结构项，解析错误为 0；路径模块中的安全入口、内部公共校验和 4 项单元测试均可正常解析。

### 验证结果

- 修改前基线 `cargo test -p vulcan-task-session --lib`：49 项全部通过。
- 最终 `cargo test -p vulcan-task-session --lib`：54 项全部通过。
- `cargo test -p vulcan-service-kernel --lib`：358 项全部通过，验证公开类别显示接口的跨 crate 调用未回归。
- `cargo clippy -p vulcan-task-session --lib --tests -- -D warnings`：通过，零告警。
- `cargo fmt --all -- --check`：通过。
- `git diff --check`：通过，已跟踪文件无空白错误；新增模块同时通过 rustfmt、编译与 Clippy 检查。
- 内置智能体资源文件名审计：非法字符、多点号最终文件名与 Windows 设备保留名均为零命中。
- 当前文件逐行计数：`catalog.rs` 3038 行、`capability_policy.rs` 192 行、`path_rules.rs` 370 行。

### 遗留问题与下一轮方向

- `catalog.rs` 仍有 3038 行，后续应继续追踪 sidecar 读写或目录扫描流程，寻找下一个具有独立数据契约和完整测试边界的职责域。
- 共享能力快照的 allowed/disabled 镜像字段仍涉及跨 crate 核心配置与服务 API Schema；这是重大接口决策，不能在内部清理轮次中顺手删除。
- 本轮没有提交、暂存或覆盖用户其他未提交变更。

## 第 80 轮：让 sidecar schemaVersion 成为强制协议（2026-07-10）

### 问题与事实依据

- 定义 sidecar 虽然一直写出 `schemaVersion: 1`，对应 DTO 却把它声明为可选字段；读取完成后没有任何版本检查，缺失、零值或未来版本都会继续进入定义合并流程。
- 分类 sidecar 的内置文件同样声明 `schemaVersion: 1`，但 `SubagentCategorySidecar` 原本根本没有该字段；Serde 会把它当作未知字段静默丢弃，因此分类版本号完全是装饰。
- 定义、分类、来源和本地化 DTO 的外层均未启用 `deny_unknown_fields`。字段拼写错误或过期字段不会报错，而会在加载时无声消失，导致用户看到“文件加载成功但设置不生效”。
- 完整资源审计确认 `crates/vulcan-runtime-assets/resources/subagents` 下共有 293 个 YAML/YML/JSON sidecar，293 个均明确声明 `schemaVersion: 1`，没有缺失或其他版本；当前资源已经满足严格协议，不需要保留旧版无版本输入。
- 调用点追踪确认 `schema_version` 原先只出现在三个写入构造器和 DTO 字段中，从未被读取逻辑消费；分类 DTO 更是零定义命中。这证明问题不是测试缺口推测，而是确定的执行链断点。

### 已查明的执行流程

1. 系统、用户和包目录扫描先找到 Markdown 定义，再按 YAML、YML、JSON 顺序定位同名 sidecar；`read_definition_sidecar` 解析成功后把 DTO 交给定义组合流程。
2. `parse_subagent_definition_text_with_metadata` 将 sidecar 元数据覆盖到 Markdown frontmatter，并组合来源、能力策略、运行时策略和更新信息。
3. 用户 override 扫描递归收集 sidecar 文件，通过同一个定义读取入口解析，然后由 `apply_sidecar_override` 覆盖已有系统定义。
4. 分类注册定位 `_category` sidecar，经 `read_category_sidecar` 解析后生成分类显示元数据；原流程在这里直接忽略 `schemaVersion`。
5. 用户定义、完整元数据 override 和启用状态 override 都由目录写入 API 生成版本一 sidecar。因此读取端强制当前版本与现有写入端拥有唯一、闭合的协议事实来源。

### 长期优化调整

- 新增 `SUBAGENT_SIDECAR_SCHEMA_VERSION` 常量，三个定义 sidecar 写入构造器统一引用该常量，不再散落数字字面量。
- `SubagentDefinitionSidecar.schema_version` 从 `Option<u32>` 改为必需 `u32`；`SubagentCategorySidecar` 新增同样的必需字段。缺失版本现在会在反序列化阶段明确失败。
- 新增统一泛型读取函数 `read_sidecar_document`，集中处理 YAML/YML/JSON 读取、反序列化和路径诊断，删除定义与分类读取中的重复格式分支。
- 新增 `validate_sidecar_schema_version`，定义和分类读取都必须在 DTO 进入业务流程前验证版本恰好为当前版本一；未来版本会返回包含文件路径和期望版本的明确错误。
- 定义、分类、来源及两类本地化 DTO 全部启用 `deny_unknown_fields`；协议外字段不再静默丢弃。扩展数据仍只能通过已经声明的 `extra` 字段进入，不再依赖任意顶层键。
- 现有测试夹具统一迁移到 `schemaVersion: 1`，旧策略形状测试也显式携带当前版本，确保失败原因仍是旧字段本身，而不是缺少版本。
- 新增任务会话真实文件读取测试，覆盖缺失版本、定义与分类的不支持版本、未知字段、当前 YAML、当前 JSON 和当前分类 sidecar。
- 新增服务内核资源回归测试，通过生产运行时使用的同一目录入口加载全部内置子代理，并断言定义非空且 `catalog.errors` 为空，防止严格协议与资源目录未来再次脱节。

### 文件变更

- 修改：`crates/vulcan-task-session/src/catalog.rs`
- 修改：`crates/vulcan-service-kernel/src/skill_catalog_projection.rs`
- 修改：`TASK_LOG.md`
- 新增：无
- 删除：无

### 修改范围代码审核与自动修复

- 首次聚焦编译发现泛型读取返回值无法仅凭后续字段访问推导 DTO 类型；已按编译器证据为定义与分类入口补充显式类型标注，避免模糊推断或候选式兜底。
- 审核发现新增协议测试只覆盖 YAML，而生产查找链仍明确支持 JSON；已自动增加版本一 JSON 文件的真实读取断言，验证共享版本检查不会遗漏格式分支。
- 审核发现现有跨 crate 测试只读取内置定义标识，没有断言 `catalog.errors`；严格解析失败时 Markdown 仍可能产生定义，旧测试因此会假绿。已增加生产资源零加载错误断言。
- CodeKit 反向搜索确认三个 sidecar 写入构造器全部使用统一版本常量，定义与分类读取全部经过共享反序列化函数和版本校验函数。
- CodeKit AST 复查 `catalog.rs` 与服务投影文件共识别 119 个结构项，解析错误为 0；新增两个协议函数和两项回归测试均有明确结构归属。

### 验证结果

- 修改前基线 `cargo test -p vulcan-task-session --lib`：54 项全部通过。
- 最终 `cargo test -p vulcan-task-session --lib`：55 项全部通过。
- `cargo test -p vulcan-service-kernel --lib`：359 项全部通过。
- 新增内置资源聚焦测试：1 项通过，覆盖生产资源目录与完整目录错误集合。
- `cargo clippy -p vulcan-task-session -p vulcan-service-kernel --lib --tests -- -D warnings`：通过，零告警。
- `cargo fmt --all -- --check`：通过。
- `git diff --check`：通过；相关文件尾随空白搜索零命中。
- 内置 sidecar 版本审计：293 个文件，版本一 293 个，缺失或非版本一 0 个。
- 当前文件逐行计数：`catalog.rs` 3185 行，`skill_catalog_projection.rs` 678 行；CodeKit 报告的历史总行数缓存未作为计量依据。

### 遗留问题与下一轮方向

- 定义 sidecar 的三个写入入口仍重复执行建目录、序列化、创建文件和写入步骤，并且使用直接截断写入；进程崩溃或写入失败可能留下半个 YAML。下一轮应完整追踪并收口为抗崩溃的同目录原子替换流程。
- `catalog.rs` 的 sidecar DTO、转换与文件 I/O 仍位于父模块；在原子写入边界查清后，可评估把完整私有协议层抽入子模块，而不是只为缩短文件机械移动代码。
- 本轮严格协议会明确拒绝缺少 `schemaVersion`、版本不是一或含未知字段的外部用户 sidecar；这是按“不考虑历史兼容”要求实施的长期行为，不提供旧格式回退。
- 本轮没有提交、暂存或覆盖用户其他未提交变更。

## 第 81 轮：统一目录文档的抗崩溃原子写入（2026-07-10）

### 问题与事实依据

- `write_user_subagent_metadata_override`、`write_user_subagent_enabled_override` 与 `write_subagent_sidecar_to_path` 都重复执行建目录、YAML 序列化、`File::create` 和 `write_all`；`File::create` 会先截断旧文件，进程崩溃或中途写入失败会留下空文件或半个 YAML。
- 完整调用追踪发现风险不只在 sidecar：`write_user_subagent_definition` 的 Markdown 主文件也使用相同的直接截断写入，而该函数随后还会写完整 sidecar。只修三个 YAML 入口仍会让同一个保存操作保留半写主文件风险。
- 服务内核的启停、系统定义元数据保存和用户定义保存分别调用上述公开入口；损坏文件会在下一次目录刷新中进入严格 sidecar/Markdown 解析并形成持久加载错误。
- 同一 crate 的 `segmented_store::io` 已经拥有成熟的原子字节发布算法：同目录唯一暂存、`create_new` 碰撞保护、完整写入、权限继承、文件同步、原子重命名、失败清理以及 Unix 父目录同步。
- 该算法原本被锁在分段存储私有模块，只供 JSON 与 JSONL 使用；在 `catalog.rs` 复制第三套实现会继续制造规则分叉，提升为 crate 私有共享边界才是长期方案。

### 已查明的执行流程

1. 服务保存用户定义时先调用 `write_user_subagent_definition`；目录层生成安全 Markdown 路径、序列化 frontmatter 与正文、写入主文件，然后生成并写入同名 YAML sidecar。
2. 服务编辑系统定义元数据时调用 `write_user_subagent_metadata_override`，启停时调用 `write_user_subagent_enabled_override`；两者生成 `overrides/<id>.yaml` 并直接截断写入。
3. 完整覆盖保存通过 `write_user_subagent_override` 转入 `write_subagent_sidecar_to_path`，用户定义的完整 sidecar 也经过同一函数。
4. 分段任务存储的 JSON 文档和 JSONL repair 先在内存完成编码，再调用原 `write_file_bytes_atomic`；并发 JSONL 测试已经验证最终文件只能是某一个完整候选内容且不会遗留 `.vulcan-stage-*`。
5. 原子算法的发布边界是单个文件：暂存文件与目标位于同一目录，完成同步后才重命名。Markdown 与 sidecar 是两个分别原子的完整文件，不是伪造的跨文件事务。

### 长期优化调整

- 新增 crate 私有模块 `atomic_file.rs`，集中提供 `write_file_atomically`、唯一暂存文件创建、失败清理、发布和 Unix 父目录同步。
- 原子写入会先在内存获得完整字节，再通过同级独占暂存文件写入并 `sync_all`；已有普通文件的权限会继承，符号链接与非普通目标会被明确拒绝。
- 暂存名称保留精确目标文件名，并组合进程号和进程内单调序号；`create_new` 是跨线程和跨进程碰撞的权威检查，不会让并发写入共享暂存文件。
- `segmented_store::io` 改为复用共享模块，删除本地文件暂存序号、原子字节写入、唯一暂存文件、失败清理与父目录同步副本；JSON、JSONL 和任务目录发布保持原有语义。
- `catalog.rs` 新增 `write_catalog_document_atomically` 和泛型 `write_yaml_sidecar_atomically`；YAML 必须在任何文件系统修改前完整序列化，再交给共享原子发布器。
- 用户 Markdown、完整 sidecar、元数据 override 和启停 override 四类目录文档全部接入共享实现，`catalog.rs` 的生产代码中不再存在 `File::create` 或 `write_all` 直接截断路径。
- 公共 API 文档明确说明用户定义的 Markdown 与 YAML sidecar 是“分别作为完整文件原子写入”，避免把逐文件保证误解为跨文件事务。

### 文件变更

- 新增：`crates/vulcan-task-session/src/atomic_file.rs`
- 修改：`crates/vulcan-task-session/src/lib.rs`
- 修改：`crates/vulcan-task-session/src/catalog.rs`
- 修改：`crates/vulcan-task-session/src/segmented_store/io.rs`
- 修改：`TASK_LOG.md`
- 删除：无

### 修改范围代码审核与自动修复

- 首版共享实现新增了成功替换、缺失父目录创建和非普通目标拒绝测试，但重命名失败后的清理分支仍只有代码推理，没有可重复故障证据。
- 审核后提取 `publish_staged_file` 为唯一发布函数，并用手工完整暂存文件替换目标目录，确定性触发跨平台重命名失败；测试直接证明目标内容保留且独占暂存文件被删除。
- 审核确认共享函数仍保留分段存储已有的权限继承、文件同步、同文件系统重命名与 Unix 父目录同步语义；错误通过 `SessionStoreError` 或 `SubagentCatalogLoadError` 的既有边界映射，没有引入新的公开错误类型。
- CodeKit 反向搜索确认旧 `write_file_bytes_atomic`、旧文件暂存序号和 `catalog.rs` 的 `File::create` 均为零命中；全部目录写入口与 JSON/JSONL 写入口只调用共享原子函数。
- CodeKit AST 复查四个任务会话文件共识别 143 个结构项，解析错误为 0；共享模块中的 5 个生产函数和 5 个测试函数均有明确结构归属。

### 验证结果

- 修改前基线 `cargo test -p vulcan-task-session --lib`：55 项全部通过。
- 最终 `cargo test -p vulcan-task-session --lib`：59 项全部通过。
- Windows 原子模块聚焦测试：成功替换、父目录创建、非普通目标拒绝和发布失败清理共 4 项通过；Unix 权限继承测试按平台条件保留。
- 既有并发 JSONL 原子重写测试随完整任务会话测试通过，证明共享迁移没有破坏并发完整文件语义。
- `cargo test -p vulcan-service-kernel --lib`：359 项全部通过，覆盖服务保存入口及内置目录加载链。
- `cargo clippy -p vulcan-task-session -p vulcan-service-kernel --lib --tests -- -D warnings`：通过，零告警。
- `cargo fmt --all -- --check`：通过。
- `git diff --check`：通过；相关文件尾随空白搜索零命中。
- 当前文件逐行计数：`atomic_file.rs` 443 行、`catalog.rs` 3180 行、`segmented_store/io.rs` 742 行、`lib.rs` 483 行；分段 I/O 文件从本轮开始时的 916 行降至 742 行。

### 遗留问题与下一轮方向

- 用户定义保存包含 Markdown 与 YAML 两个文件；本轮保证每个文件永远以完整镜像发布，但没有承诺两个文件跨文件同时提交。真正事务化需要改变目录布局或引入版本指针，属于架构决策，后续若推进必须先提供方案并等待确认。
- `vulcan-provider-config` 等其他 crate 仍有各自的原子文件实现；提升为工作区公共文件系统库会改变跨 crate 架构边界，不应在当前私有收口中顺手进行。
- `catalog.rs` 的 sidecar DTO、转换、读写和覆盖应用仍位于父模块；下一轮可基于已经稳定的原子 I/O 边界抽离完整私有 sidecar 协议模块。
- 本轮没有提交、暂存或覆盖用户其他未提交变更。

## 第 82 轮：抽离严格 sidecar 持久化协议模块（2026-07-10）

### 问题与事实依据

- 第 80、81 轮已经建立严格版本校验和原子文件写入，但五个 sidecar DTO、版本常量、YAML/JSON 解析、文件发现、override 收集与 YAML 写入仍散落在 `catalog.rs`，与扫描编排、定义合并、分类注册和测试交错。
- CodeKit 调用追踪确认这些元素共同描述“磁盘 sidecar 长什么样、如何找到、如何读写、哪些版本合法”，而不是目录业务规则；它们只由 `catalog.rs` 消费，不属于 crate 公开 API。
- `SubagentDefinitionSidecar` 与 `SubagentCategorySidecar` 的字段由定义解析、分类转换和覆盖应用读取，因此协议 DTO 必须对父目录模块可见；但无需暴露到 crate 或跨 crate 边界。
- `SubagentDefinitionFrontmatter` 与 `SubagentMarkdownFrontmatter` 描述 Markdown 文档而不是 sidecar，定义合并、分类注册和 `apply_sidecar_override` 描述业务映射而不是持久化格式；把这些一并搬走会形成按文件长度机械拆分。
- 抽取前 `catalog.rs` 实际为 3180 行、95 个结构项；协议相关定义和函数只有一个调用域，具备独立不变量、独立测试和稳定私有接口。

### 已查明的执行流程

1. 定义扫描和用户 override 扫描分别通过 `find_definition_sidecar` 或 `collect_sidecar_files` 获得候选路径，再调用严格定义 sidecar 读取器。
2. 分类注册通过 `find_category_sidecar` 定位 `_category` YAML/YML/JSON，再调用严格分类读取器。
3. 两个读取入口共享格式反序列化和版本一校验，生成 DTO 后才交给父模块的定义合并、分类转换或覆盖应用。
4. 用户定义、完整覆盖、元数据覆盖和启停覆盖在父模块构造 DTO，再调用统一 YAML 原子写入入口；文件完整性由 crate 级 `atomic_file` 保证。
5. 能力策略 DTO 仍由相邻 `capability_policy` 模块拥有，sidecar 协议只组合它们；扩展名和路径判定仍由 `path_rules` 提供，避免复制能力或路径规则。

### 长期优化调整

- 新增私有模块 `catalog/sidecar_protocol.rs`，集中持有当前 sidecar schema 版本、来源 DTO、定义本地化 DTO、定义 DTO、分类 DTO 和分类本地化 DTO。
- 模块统一持有 YAML/YML/JSON 严格读取、格式错误诊断、版本校验、定义 sidecar 发现、分类 sidecar 发现、override 递归收集和 YAML 原子写入。
- DTO 与模块入口全部限制为 `pub(super)`，父 `catalog` 可以执行业务映射，但 crate 公共 API、服务请求 Schema 和磁盘字段名称均未改变。
- `catalog.rs` 只导入协议类型和窄化函数，不再定义第二份版本常量、DTO、反序列化器或文件发现规则；两个严格协议测试同步迁入新模块，与被测私有实现保持同一所有权边界。
- 协议模块直接依赖 crate 级 `atomic_file` 完成持久化错误适配，不再反向调用父模块的通用 Markdown 写入函数；依赖方向保持“目录业务 → 协议 → 原子文件/路径/能力 DTO”。
- `catalog.rs` 从 3180 行降至 2693 行，减少 487 行和 15 个结构项；新协议模块为 629 行、16 个结构项。新增行主要来自完整双语接口文档、显式 `pub(super)` 字段边界和新增故障测试。

### 文件变更

- 新增：`crates/vulcan-task-session/src/catalog/sidecar_protocol.rs`
- 修改：`crates/vulcan-task-session/src/catalog.rs`
- 修改：`TASK_LOG.md`
- 删除：无

### 修改范围代码审核与自动修复

- 首版抽取让新协议模块反向调用父模块 `write_catalog_document_atomically`；审核确认这会形成不必要的子模块到业务父层依赖，已改为直接调用 crate 级原子文件能力并在协议模块内适配目录错误。
- 审核发现 `collect_sidecar_files` 虽接收错误集合，却使用 `entries.flatten()` 静默丢弃单项目录错误，根目录读取失败也不保留操作系统原因；这会把 override 读取失败伪装成“没有覆盖文件”。
- 已自动改为显式处理根目录和每个目录项错误，完整记录路径及操作系统原因，并继续处理可读取的同级项。
- 新增真实文件故障测试：把普通文件作为 override 根传入收集器，断言没有伪造 sidecar、错误路径准确且消息保留底层原因。
- CodeKit 反向搜索和 AST 复查确认五个协议 DTO、共享反序列化和版本校验只在新模块定义；父模块只剩三个版本常量使用点和真实业务调用。

### 验证结果

- 修改前基线 `cargo test -p vulcan-task-session --lib`：59 项全部通过。
- 最终 `cargo test -p vulcan-task-session --lib`：60 项全部通过。
- `cargo test -p vulcan-task-session sidecar_protocol --lib`：3 项通过，覆盖旧协议拒绝、严格版本/字段/格式和 override 根错误诊断。
- `cargo test -p vulcan-service-kernel --lib`：359 项全部通过，验证公开目录 API、服务保存链和内置资源严格加载未回归。
- `cargo clippy -p vulcan-task-session -p vulcan-service-kernel --lib --tests -- -D warnings`：通过，零告警。
- `cargo fmt --all -- --check`：通过。
- `git diff --check`：通过；相关文件尾随空白搜索零命中。
- CodeKit AST 复查四个目录实现文件共识别 134 个结构项，解析错误为 0；父模块 80 个、新协议模块 16 个、能力协议模块 17 个、路径模块 21 个。
- 当前文件逐行计数：`catalog.rs` 2693 行、`sidecar_protocol.rs` 629 行、`capability_policy.rs` 192 行、`path_rules.rs` 370 行；CodeKit 的历史总行数缓存未用于计量。

### 遗留问题与下一轮方向

- 父模块仍负责将协议 DTO 映射为领域定义并应用覆盖，这是有意保留的业务边界；若继续拆分，应围绕“文档解析与领域映射”形成完整接口，而不是暴露更多 DTO 字段。
- `scan_subagent_definition_root`、`mirror_system_top_level_category_directories` 和 `scan_package_roots` 仍使用 `entries.flatten()`，可能像本轮旧 override 收集器一样吞掉目录项错误；下一轮应沿三条扫描链逐一核实并统一显式诊断。
- 用户 Markdown 与 sidecar 仍是分别原子的两个文件，不是跨文件事务；该架构限制与第 81 轮结论不变。
- 本轮没有提交、暂存或覆盖用户其他未提交变更。

## 第 83 轮：消除目录扫描中的静默错误吞噬（2026-07-10）

### 问题与事实依据

- `scan_subagent_definition_root`、`mirror_system_top_level_category_directories` 与 `scan_package_roots` 都使用 `entries.flatten()`；`ReadDir` 某个目录项读取失败时会被当成不存在，目录继续加载但没有任何错误记录。
- 三条扫描链都已经接收同一个 `Vec<SubagentCatalogLoadError>`，并由 `load_subagent_catalog_from_roots` 汇入 `SubagentCatalog.errors`；服务刷新再把该错误集合投影到工作台目录摘要与错误数量通知。因此静默跳过会直接让用户误判目录完整。
- 三个根目录的 `read_dir` 建立失败中，系统定义根和分类镜像会保留操作系统原因，包根原先只写固定消息；同一目录系统存在不一致诊断策略。
- 首轮替换迭代逻辑后继续审核前置条件，发现 `scan_package_roots` 与 `apply_user_overrides` 使用 `!path.is_dir()` 同时表示“路径缺失”和“路径已存在但类型错误”；`packages` 或 `overrides` 若是普通文件，真实运行仍会无声忽略。
- 代码搜索确认任务会话源码共有且仅有这三处 `.flatten()`，没有其他需要保留该语义的多层迭代器使用。

### 已查明的执行流程

1. 目录加载首先扫描系统根，递归读取每个子目录和 Markdown 文件，解析定义并注册顶层分类。
2. 用户根存在时，包扫描读取 `packages` 的直接子目录，校验 package id，再把每个包目录交回相同的递归定义扫描器。
3. 用户定义扫描完成后，override 流程读取 `overrides` 根并递归收集 sidecar，最后应用到已有定义。
4. `ensure_user_subagent_category_directories` 通过分类镜像扫描系统根的直接子目录，并在用户定义根创建对应分类目录。
5. 四条路径的错误最终进入同一个目录对象；迭代错误应记录后继续可读同级项，缺失的可选用户根则应保持无错误，已存在但不是目录或无法检查的根必须明确上报。

### 长期优化调整

- 新增统一 `directory_entry_or_record_error`，接收真实 `io::Result<DirEntry>`、所属目录和扫描上下文；成功返回目录项，失败追加包含路径与底层原因的目录错误并返回 `None`。
- 系统定义递归扫描、系统分类镜像和包根扫描全部改为显式遍历 `ReadDir` 结果，通过共享函数处理目录项错误后继续可读同级项。
- 包根 `read_dir` 建立失败改为保留操作系统错误原因，不再只返回无信息量的固定文案。
- 新增 `scannable_catalog_directory`，用 `fs::metadata` 区分空路径、真正缺失、有效目录、非目录类型与检查失败；系统定义根、包根和 override 根共用同一根路径分类策略。
- 缺失的可选包根和 override 根仍然正常跳过；已存在但不是目录的路径会生成稳定类型错误，元数据检查失败会保留底层错误原因。
- 任务会话源码中的 `.flatten()` 已全部清零，扫描错误不再通过迭代器语法被静默丢弃。

### 文件变更

- 修改：`crates/vulcan-task-session/src/catalog.rs`
- 修改：`TASK_LOG.md`
- 新增：无
- 删除：无

### 修改范围代码审核与自动修复

- 首版实现只替换了三处 `entries.flatten()` 并新增合成目录项错误测试；审核发现包根和 override 根在进入迭代器前仍可能把非目录路径静默视为缺失。
- 已自动把根路径分类提升为共享函数，并让包扫描、override 应用和递归目录扫描全部复用，消除迭代前旁路。
- 初版共享根判断若继续使用 `exists()`/`is_dir()` 会吞掉元数据读取错误；审核后改为直接匹配 `fs::metadata`，只对 `NotFound` 安静返回，其余检查错误全部可观察。
- 新增完整目录加载集成测试，同时把 `packages` 与 `overrides` 构造为普通文件，断言两个精确路径和稳定类型错误都进入目录结果。
- CodeKit 反向搜索与普通文本复核均确认任务会话源码 `.flatten()` 零命中；三条扫描链只调用共享目录项处理函数。

### 验证结果

- 修改前基线 `cargo test -p vulcan-task-session --lib`：60 项全部通过。
- 最终 `cargo test -p vulcan-task-session --lib`：62 项全部通过。
- 目录项错误聚焦测试：1 项通过，验证底层原因、扫描上下文和所属目录路径。
- 非目录包根/覆盖根集成测试：1 项通过，验证完整生产加载顺序不会把类型错误当作缺失路径。
- `cargo test -p vulcan-service-kernel --lib`：359 项全部通过，验证目录错误投影、服务刷新与内置资源加载未回归。
- `cargo clippy -p vulcan-task-session -p vulcan-service-kernel --lib --tests -- -D warnings`：通过，零告警。
- `cargo fmt --all -- --check`：通过。
- `git diff --check`：通过；相关文件尾随空白搜索零命中。
- CodeKit AST 复查四个目录实现文件共识别 138 个结构项，解析错误为 0；`catalog.rs` 新增两个共享函数和两个回归测试。
- 当前文件逐行计数：`catalog.rs` 2878 行、`sidecar_protocol.rs` 629 行、`capability_policy.rs` 192 行、`path_rules.rs` 370 行；CodeKit 的历史总行数缓存未用于计量。

### 遗留问题与下一轮方向

- 递归定义扫描仍通过 `metadata/is_dir` 跟随目录符号链接，且没有已访问目录集合；指向父目录或自身的链接可能造成循环扫描。下一轮应先确认支持平台、链接语义和现有资源事实，再设计确定性的循环阻断。
- 包根中的普通非目录条目当前按设计跳过；它们不是 package id 候选，也不等同于包根本身类型错误。
- 用户 Markdown 与 sidecar 仍是分别原子的两个文件，不是跨文件事务；该架构限制保持不变。
- 本轮没有提交、暂存或覆盖用户其他未提交变更。

## 第 84 轮：拒绝目录链接并阻断递归扫描逃逸（2026-07-10）

### 问题与事实依据

- 第 83 轮消除 `ReadDir` 错误吞噬后，递归定义扫描、分类镜像、包扫描和 override 收集仍通过 `Path::is_dir()` / `Path::is_file()` 判断目录项；这些接口会跟随符号链接，指回祖先的目录链接可形成无限递归，指向目录树外部的链接还会让目录边界失效。
- 源码搜索确认风险集中在四条目录枚举链以及定义/分类 sidecar 的约定文件名发现；原子文件写入器已经用 `symlink_metadata` 拒绝最终目标链接，但这不能保护读取扫描链。
- 现有内置子代理资源、task-session 源码和测试目录中的重解析点数量均为 0，因此收紧后代目录项策略不会破坏当前仓库资源。
- 本机 Windows 未启用创建符号链接所需权限，无法运行真实 Windows 链接夹具；已继续查阅当前 Rust 1.94 标准库 Windows 实现，确认 `DirEntry::file_type().is_symlink()` 会识别带名称代理标记的重解析点，覆盖符号链接以及 junction/mount-point 这类可改变路径解析目标的目录项。
- 最终策略明确为：调用方显式提供的系统根和用户根仍是受信任边界，可以自身解析为链接；所有从这些根枚举出的后代符号链接或 Windows 名称代理重解析项一律报告并跳过，不引入兼容白名单、canonicalize 猜测或已访问路径补丁。

### 已查明的执行流程

1. `load_subagent_catalog_from_roots` 先递归扫描系统根，再扫描用户 `packages` 和 `definitions`，最后收集并应用 `overrides` sidecar；系统分类镜像也会单独枚举系统根的顶层目录。
2. 三条定义/分类/包扫描链原先从 `ReadDir` 取得路径后再次调用跟随链接的路径类型判断；override 收集器也用相同方式决定是否递归。
3. Markdown 定义和分类目录被接受后，sidecar 发现会按 YAML、YML、JSON 优先级检查约定路径；原先的 `Path::is_file()` 会把指向外部普通文件的链接当成合法 sidecar。
4. 本轮统一在 `DirEntry` 边界读取不跟随链接的 `FileType`；链接在任何递归或协议解析前进入 `SubagentCatalog.errors`，普通目录和普通文件才继续原有业务流程。
5. 目录错误随后沿现有 catalog 投影进入服务层工作台摘要；合法同级项继续加载，因此严格拒绝链接不会把单个恶意或错误条目扩大为整个目录不可用。

### 长期优化调整

- 新增 `catalog/directory_scan.rs`，集中承载可扫描根路径分类、`ReadDir` 单项错误处理、不跟随链接的文件类型读取以及严格链接拒绝诊断。
- `scan_subagent_definition_root`、`mirror_system_top_level_category_directories`、`scan_package_roots` 和 `collect_sidecar_files` 全部复用共享目录项分类器，不再根据会跟随链接的 `Path::is_dir()` / `Path::is_file()` 决定后代递归或文件加载。
- 每个被拒绝链接都保留精确目录项路径和扫描上下文；读取失败、类型检查失败与链接策略失败继续使用不同诊断，避免把权限问题误报为链接。
- 定义和分类 sidecar 发现改用 `fs::symlink_metadata`，仅接受直接存储的普通文件；链接、目录、缺失候选和检查失败都不会进入 sidecar 解析器。
- 新增平台无关的纯策略测试，验证链接拒绝结果、错误路径和上下文文案；新增 Unix 条件集成测试，覆盖指回祖先的递归目录链接与约定 sidecar 文件链接。

### 文件变更

- 新增：`crates/vulcan-task-session/src/catalog/directory_scan.rs`
- 修改：`crates/vulcan-task-session/src/catalog.rs`
- 修改：`crates/vulcan-task-session/src/catalog/sidecar_protocol.rs`
- 修改：`TASK_LOG.md`
- 删除：无

### 修改范围代码审核与自动修复

- 首版目录扫描调整只覆盖递归入口；审核调用链时发现 `find_definition_sidecar` 与 `find_category_sidecar` 仍通过 `Path::is_file()` 跟随链接，外部文件可绕过目录项递归策略进入协议解析。
- 已自动把两处约定候选发现统一改为不跟随链接的普通文件检查，并补充真实 Unix 文件链接测试，确保链接占用最高优先级扩展名时不会被读取。
- 审核确认共享分类器先处理 `ReadDir` 单项失败，再读取 `DirEntry::file_type()`，只有完成类型检查后才判断链接；因此不会用模糊 fallback 掩盖底层 I/O 失败。
- CodeKit 反向搜索确认目录枚举链只使用共享分类器返回的 `FileType`；剩余 `system_root.is_dir()` 位于显式根信任边界，测试中的 `is_dir` / `is_file` 仅用于断言，分段存储中的同名调用不属于本轮目录协议。
- 当前平台无法创建真实链接夹具，审核没有伪造 Windows 条件分支；以标准库实现证据确认名称代理重解析点语义，同时保留 Unix 真实循环测试供对应平台执行。

### 验证结果

- 修改前基线 `cargo test -p vulcan-task-session --lib`：62 项全部通过。
- `cargo test -p vulcan-task-session directory_scan --lib`：2 项通过，验证目录项错误可观察性和严格链接诊断。
- 最终 `cargo test -p vulcan-task-session --lib`：63 项全部通过；当前 Windows 构建按预期不执行两个 `#[cfg(unix)]` 真实链接测试。
- `cargo test -p vulcan-service-kernel --lib`：359 项全部通过，验证目录错误投影、服务刷新和内置资源加载未回归。
- `cargo clippy -p vulcan-task-session -p vulcan-service-kernel --all-targets -- -D warnings`：通过，零告警。
- `cargo fmt --all -- --check`：通过。
- `git diff --check`：通过；相关文件尾随空白搜索零命中。
- CodeKit AST 复查父模块与四个目录子模块共识别 143 个结构项，解析错误为 0；文本到结构反查未发现扫描链残留的跟随式目录项判断。
- 当前文件逐行计数：`catalog.rs` 2821 行、`directory_scan.rs` 201 行、`sidecar_protocol.rs` 682 行、`capability_policy.rs` 192 行、`path_rules.rs` 370 行；CodeKit 的历史行数元数据未用于计量。

### 遗留问题与下一轮方向

- 显式系统根和用户根目前按既定信任边界允许自身为链接；若产品要求根路径也必须物理落在配置位置，需要先调整配置协议并明确现有安装布局，不能把根与后代语义混为一谈。
- `DirEntry::file_type()` 与随后按路径 `read_dir`/读取文件之间仍存在替换竞态；彻底消除 TOCTOU 需要基于已打开目录句柄的遍历以及 Windows/Unix 不同底层 API，属于后续需要单独取证的跨平台文件系统架构工作。
- 原子写入器会拒绝最终目标本身是链接，但尚未证明所有父路径组件都不经过链接；下一轮可沿用户目录写入链确认父目录归属和创建顺序，再决定是否需要句柄化写入边界。
- 用户 Markdown 与 sidecar 仍是分别原子的两个文件，不是跨文件事务；该架构限制保持不变。
- 本轮没有提交、暂存或覆盖用户其他未提交变更。

## 第 85 轮：消除分段存储的存在性预检与错误伪装（2026-07-11）

### 问题与事实依据

- 沿第 84 轮遗留的父路径链接风险追踪原子写入链时，确认共享写入器只接收最终路径，不知道系统路径与用户可控路径之间的可信根边界；仅在函数内部遍历路径组件会误伤合法系统重解析点，而且检查后仍可被替换，不能作为长期安全方案。
- 同一持久化执行链存在一个可以确定性闭环的问题：`read_json_file` 先调用 `Path::exists()`，再调用 `File::open()`；Rust 的布尔存在性接口会把检查错误折叠为 `false`，因此权限、非法路径和其他非 `NotFound` 错误会被伪装成可选文件缺失。
- 根元数据调用、任务目录定位、根镜像读取、JSONL 测试读取以及分段修复删除都重复使用“先 `exists()`、再打开/枚举/删除”的模式；即使预检成功，真实 I/O 前路径状态也可能改变。
- CodeKit 调用点反查确认可选元数据缺失的业务语义是返回空事件、空镜像、`false` 或 `None`；这些语义只应对应真实 `NotFound`，其他错误必须继续沿现有 `Result` 返回。
- `root_session_metadata_exists()` 和 `task_session_directory_exists()` 是只返回瞬时布尔快照的查询，不会根据结果紧接着执行 I/O；本轮不把这类纯查询与错误的 I/O 前置预检混为一谈。

### 已查明的执行流程

1. `SegmentedTaskSessionStore` 以主会话目录为根，根元数据中的分段索引和任务镜像驱动主会话事件读取、镜像加载与任务关联更新。
2. 可选根元数据原先先检查 `metadata.json` 是否存在，再通过 `read_required_json` 重新打开；检查失败会返回空结果，检查与打开之间删除文件则会变成意外的必需文件错误。
3. `find_task_dir` 先检查任务元数据路径，再通过 `read_task_metadata_document` 打开解析；`load_all_task_documents` 也先检查 `task` 根，再调用 `read_dir`。
4. JSONL 修复先判断分段路径存在，再打开读取；清除空分段或损坏分段后缀时又先检查存在，再删除，读取后是否重写还会第三次检查同一路径。
5. 所有这些函数本身已经返回 `Result`，因此实际 `open`、`read_dir` 和 `remove_file` 可以直接成为唯一事实来源：仅 `ErrorKind::NotFound` 映射为可选缺失，其余错误原样转为路径感知存储错误。

### 长期优化调整

- 新增内部 `open_optional_file`，直接打开目标文件；成功返回句柄，只有 `NotFound` 返回 `None`，其他失败返回包含精确路径的英文诊断。
- 新增内部 `remove_optional_file`，以实际删除结果区分已删除、真正缺失和路径感知删除失败，不再执行过期存在性预检。
- `read_json_file`、测试 JSONL 读取和分段修复读取统一复用权威打开结果；缺失分段的修复状态明确记录为未打开，不再通过后续 `exists()` 猜测。
- 根事件读取、根任务镜像追加/刷新/加载、镜像树加载和 `root_session_task_mirror_exists` 全部直接使用可选 JSON 读取契约，保留原有真实缺失返回值并暴露其他错误。
- `find_task_dir` 直接读取一次任务元数据；`load_all_task_documents` 直接建立 `read_dir`，仅对 `NotFound` 返回空集合，其他根目录错误包含精确路径。
- 分段清理改为直接执行可选删除；JSON/JSONL 生产路径中所有“存在性预检后执行 I/O”的组合均已移除。

### 文件变更

- 修改：`crates/vulcan-task-session/src/segmented_store/io.rs`
- 修改：`crates/vulcan-task-session/src/segmented_store/invocations.rs`
- 修改：`crates/vulcan-task-session/src/segmented_store/mirror.rs`
- 修改：`crates/vulcan-task-session/src/segmented_store/store_impl.rs`
- 修改：`crates/vulcan-task-session/src/segmented_store/tests.rs`
- 修改：`TASK_LOG.md`
- 新增：无
- 删除：无

### 修改范围代码审核与自动修复

- 初版修复用 `source_file_exists` 保存分段读取时的打开结果；审核发现该名称暗示重写时的实时状态，实际只能证明观察时点曾成功打开，已自动重命名为 `source_file_was_opened` 并修正文档，避免后续误用为实时保证。
- 初版共享 helper 仍把非缺失错误交给通用 `to_store_error`，只保留裸操作系统消息；审核后改为打开、删除和任务根枚举三类路径感知诊断，错误不再脱离产物位置。
- 新增确定性跨平台测试：真实缺失路径必须返回 `None`，包含内嵌 NUL 的非法路径必须返回以 `Failed to open optional file` 开头的错误；旧 `Path::exists()` 实现会把后者错误折叠为 `None`。
- CodeKit 反向搜索确认分段存储生产代码中的 `.exists()` 仅剩纯布尔查询；测试中的 `.exists()` 只用于最终状态断言，未参与业务 I/O 决策。
- 审核确认真实缺失根元数据仍分别返回空事件、空镜像、空镜像树或 `false`，本轮没有把可选文件协议错误收紧为必需文件协议。

### 验证结果

- 修改前基线 `cargo test -p vulcan-task-session --lib`：63 项全部通过。
- 聚焦回归 `cargo test -p vulcan-task-session optional_json_read_distinguishes_missing_path_from_open_failure --lib`：1 项通过。
- 最终 `cargo test -p vulcan-task-session --lib`：64 项全部通过。
- `cargo test -p vulcan-workbench-model --lib`：75 项全部通过，验证面板状态与分段存储读取集成未回归。
- `cargo test -p vulcan-service-kernel --lib`：359 项全部通过，验证服务层任务镜像、导航和目录投影未回归。
- `cargo clippy -p vulcan-task-session -p vulcan-service-kernel -p vulcan-workbench-model --all-targets -- -D warnings`：通过，零告警。
- `cargo fmt --all -- --check`：通过。
- `git diff --check`：通过；五个修改范围源码文件尾随空白搜索零命中。
- CodeKit AST 复查五个修改文件共识别 110 个结构项，解析错误为 0；结构反查确认所有执行 I/O 的生产调用链均不再使用 `.exists()` 预检。
- 当前文件逐行计数：`io.rs` 794 行、`invocations.rs` 118 行、`mirror.rs` 707 行、`store_impl.rs` 543 行、`tests.rs` 1809 行；CodeKit 的历史行数元数据未用于计量。

### 父路径链接方案分析与重大决策边界

- 方案一是在每次写入前用 `symlink_metadata` 静态检查父路径组件；该方案能拒绝静态链接，但无法消除检查与创建、打开、重命名之间的替换竞态，不满足长期安全边界，因此没有实施。
- 长期方案需要把“可信根”显式纳入持久化 API，并基于已打开目录句柄执行相对创建、打开与发布；目录扫描、原子文件发布和任务目录发布都必须共享同一个句柄化边界。
- 该方案会改变 `SegmentedTaskSessionStore::new`、目录写入 API 和 catalog 用户根写入契约，并需要 Windows 与 Unix 不同的底层实现或引入能力型文件系统核心库，属于架构变更、核心库/平台 API 决策与安全协议调整。
- 按项目重大决策规则，本轮只完成事实链和方案边界，不擅自引入依赖或用不完整预检替代；实施前需要用户明确确认句柄化跨平台持久化方向。

### 遗留问题与下一轮方向

- 原子写入、追加 JSONL 和任务目录发布仍可能跟随可信根以下的父目录链接；完整修复等待上述重大方案确认。
- `load_all_task_documents` 仍通过 `task_dir.is_dir()` 跟随枚举出的目录链接；它与父路径写入属于同一存储信任边界，不能只修读取侧而保留写入侧旁路。
- 两个纯布尔存在性查询会按接口设计折叠检查错误；下一轮可先追踪全部调用者，判断它们是否应迁移为返回 `Result<bool, SessionStoreError>` 的显式状态查询。
- `source_file_was_opened` 只记录读取时点，不能保证原子重写时路径未被外部替换；彻底消除该竞态同样依赖句柄化发布。
- 用户 Markdown 与 sidecar 仍是分别原子的两个文件，不是跨文件事务；该架构限制保持不变。
- 本轮没有提交、暂存或覆盖用户其他未提交变更。

## 第 86 轮：删除分段存储的零调用状态探测 API（2026-07-11）

### 问题与事实依据

- 第 85 轮消除执行 I/O 前的 `exists()` 预检后，`SegmentedTaskSessionStore` 仍公开 `root_session_metadata_exists()` 与 `task_session_directory_exists()` 两个布尔查询；它们分别通过 `Path::exists()` 和 `Path::is_dir()` 把检查错误折叠为 `false`。
- CodeKit 全仓调用点搜索确认两个方法都只在 `store_impl.rs` 定义一次，生产代码、测试和文档调用数均为 0；仓库级纯文本复核得到相同结果。
- 同组的 `root_session_task_mirror_exists()` 虽然返回 `Result<bool, SessionStoreError>`，但同样零调用，只读取根元数据后判断 `tasks` 是否为数组；保留它会鼓励调用者先探测、再调用真实镜像加载 API。
- 三个方法都是公开固有方法，但当前仓库没有任何消费者；按照“不考虑历史兼容”的长期目标，不需要为潜在外部调用者保留死接口或兼容别名。
- 实际镜像加载、任务元数据加载和任务目录解析 API 已经能够一次返回数据、真实缺失或明确错误，因此状态探测器没有独立业务能力。

### 已查明的执行流程

1. 根元数据存在性方法只读取一个瞬时布尔值，不参与任何仓库内启动、恢复或 UI 分支。
2. 任务目录存在性方法同样没有调用者，真实任务定位通过 `task_session_directory`、`find_task_dir` 和元数据读取完成。
3. 根任务镜像探测器单独读取 `metadata.json` 并检查 `tasks` 数组；真实消费者直接调用 `load_task_session_mirror_summaries` 或按父级加载 API。
4. 若未来先调用探测器再调用加载器，会产生两次文件系统观察和状态切换窗口；删除探测器可强制消费者以一次权威加载结果决策。
5. 三个定义删除后，仓库内不存在名称引用、迁移调用点或条件编译旁路。

### 长期优化调整

- 删除 `root_session_metadata_exists()`，不再暴露会吞掉文件系统检查错误的根元数据布尔探测器。
- 删除 `task_session_directory_exists()`，不再把不可读、非目录和真正缺失折叠为同一个 `false`。
- 删除零调用的 `root_session_task_mirror_exists()`；需要镜像的调用者必须使用真实加载 API 并处理完整 `Result`。
- 没有新增替代方法、deprecated 包装器或兼容转发层，公开 API 与实际使用能力保持一致。

### 文件变更

- 修改：`crates/vulcan-task-session/src/segmented_store/store_impl.rs`
- 修改：`TASK_LOG.md`
- 新增：无
- 删除：无

### 修改范围代码审核与自动修复

- 初始审计目标是两个布尔方法；修改范围复核发现相邻的 `root_session_task_mirror_exists()` 虽有正确错误类型，但同样零调用且属于同一探测模式，已自动扩展为完整删除三件套，避免只清理一半。
- 独立只读审计再次执行删除前与删除后的全仓结构搜索：删除前三个名称各只有定义、调用数均为 0；删除后匹配数为 0，解析错误为 0。
- 独立审计继续检查 `SegmentedTaskSessionStore` 的公开 `exists`、`status` 和布尔状态包装器，没有发现遗漏；`SubagentCatalog::is_enabled` 等谓词具有真实业务语义，不属于文件系统状态探测。
- AST 复查确认 `store_impl.rs` 从 27 个结构项减少为 24 个，后续公开加载方法直接衔接，没有遗留空分组、无用实现或结构错误。

### 验证结果

- 修改前基线 `cargo test -p vulcan-task-session --lib`：64 项全部通过。
- 最终 `cargo test -p vulcan-task-session --lib`：64 项全部通过。
- `cargo test -p vulcan-workbench-model --lib`：75 项全部通过，验证工作台状态消费者没有依赖已删除 API。
- `cargo test -p vulcan-service-kernel --lib`：359 项全部通过，验证服务层导航、恢复和任务镜像调用链未回归。
- `cargo clippy -p vulcan-task-session -p vulcan-service-kernel -p vulcan-workbench-model -p vulcan-agent-runtime --all-targets -- -D warnings`：通过，零告警，覆盖主要公开 API 消费者及条件编译目标。
- `cargo fmt --all -- --check`：通过。
- `git diff --check`：通过；`store_impl.rs` 尾随空白搜索零命中。
- CodeKit 删除后全仓精确反查：三个方法名称匹配数为 0，错误为 0。
- CodeKit AST 复查 `store_impl.rs` 识别 24 个结构项，解析错误为 0；权威逐行计数为 519 行。

### 遗留问题与下一轮方向

- 原子写入、追加 JSONL、任务目录发布和任务目录枚举的父路径链接问题仍需要句柄化跨平台持久化方案；该重大架构与安全协议调整保持等待确认。
- `SegmentedTaskSessionStore` 的其余公开方法均有仓库内调用或明确加载/写入语义，本轮没有以“清理死 API”为名扩大删除范围。
- `source_file_was_opened` 仍只代表读取时点，不能消除读取与原子重写之间的外部替换竞态。
- 用户 Markdown 与 sidecar 仍是分别原子的两个文件，不是跨文件事务；该架构限制保持不变。
- 本轮没有提交、暂存或覆盖用户其他未提交变更。

## 第 87 轮：严格化运行时日志完整性与锁内事务（2026-07-11）

### 问题与事实依据

- `vulcan-workspace-store` 的 Provider 可见运行时日志由 `runtime_metadata.json`、`runtime/r001.jsonl` 与可重建的 `runtime.index.json` 共同组成；修改前多个读取入口先用 `Path::exists()` 判断，再把打开、解析或分段读取错误降级成“缺失”。
- 普通同步、压缩态同步、hydration 与审计追加分别存在 `Err(_) -> rebuild`、`Err(_) -> Ok(None)` 或 `Err(_) -> Ok(false)`，会让已持久化损坏伪装成合法缺失，并可能覆盖唯一故障证据。
- 容错 JSONL 读取会跳过任意损坏行并保留其后的有效行，能够把“有效—损坏—有效”拼成缺少中间事件的会话；完全损坏时还会先把源分段重写为空，再返回错误，导致第二次读取错误地成功。
- 元数据缺失时，旧实现没有检查仍存在的 `runtime/` 或 runtime index；同步会把崩溃中断留下的孤儿运行记录当成首次创建并清理旧目录。
- `append_runtime_log_event_if_present` 在锁外读取 `next_seq`，`append_runtime_log_tail` 又在持锁后调用会重复取同一非重入锁的读取修复函数；并发审计可能分配重复序号，修复路径存在死锁，读—决策—写也不是同一锁周期。
- `RuntimeLogMetadata` 虽预留 `Vec<RuntimeSegmentMetadata>` 与 `sealed`，但 CodeKit 全仓追踪确认生产代码只有一个构造点，始终创建 `runtime/r001.jsonl`，没有 rollover、`segments.push`、`r002` 或封存旧段的实现；旧读取器却擅自接受多段，能够在前段尾损坏、后段有效时跨缺口拼接。
- 元数据没有统一校验 `schema_version`、活动段归属、起始序号、行数、结束序号、字节数与 `next_seq`；合法 JSON 但陈旧的元数据可以造成重复/跳号，活动段不属于 `segments` 时甚至会写入一个索引永远看不到的文件。
- 分叉路径复制 `runtime/` 与 index 后，在 runtime metadata 缺失时直接返回成功；无截断分叉不会进入后续 replay rebuild，因此会发布一个包含孤儿运行产物且身份未改写的坏分叉。
- `runtime_log_line_matches` 通过两个 `serde_json::to_value(...).ok()` 比较事件；双方序列化都失败时会错误地把两个 `None` 判为相等。

### 已查明的执行流程

1. hydration 链路为 `ensure_runtime_log_from_segmented_context -> read_runtime_conversation_state -> read_runtime_log_lines`，再进入会话提交、mailbox、goal continuation、压缩与工作台恢复；除明确的 display 分段缺失回退外，runtime 错误会沿 workspace-store 与 service-kernel 的 `Result` 传播。
2. 持久化链路为 `sync_segmented_runtime_log_for_state -> sync_runtime_log_from_conversation_state / sync_runtime_log_from_compacted_conversation_state`；TaskSession 也通过公开 runtime 同步门面进入同一实现。
3. 审计链路为 injection applied/cancelled/merged、provider checkpoint、lowering repair 与压缩备份等调用 `append_runtime_log_event_if_present`；最终调用方多为 best-effort 日志处理，但“调用方选择记录后继续”不等于存储层可以吞掉损坏。
4. 分叉链路为 `fork_persisted_session -> fork_segmented_session_directory_from_source -> copy_segmented_session_tree_without_tasks -> rewrite_forked_persisted_identity_artifacts`；真正截断 anchor 才会在更高层从 replay 重建 runtime，无 anchor 或末尾 anchor 会直接依赖低层复制与身份改写。
5. CodeKit 生产调用点反查确认 runtime 只有单段写入能力；因此长期契约选择明确拒绝伪多段元数据，而不是维护“能读但不能正确创建、封存和追加”的虚假兼容层。
6. 锁边界最终统一为：读取只在快照验证/修复期间持锁；普通/压缩同步从首次读取持续持锁到 append 或 rebuild 完成；审计从快照读取、序号分配持续持锁到 segment、metadata、index 发布；分叉源复制与 display 转换持同一锁，目标身份改写另起一个完整锁事务。

### 长期优化调整

- 新增权威 `read_optional_json`：只有 `File::open` 真正返回 `NotFound` 才产生 `None`，打开与解析失败均带路径传播；`read_required_json` 与必需 JSONL 读取不再通过预检折叠错误。
- `append_json_lines_file` 只允许追加已有文件，不再创建父目录或缺失活动段；缺失 segment 会保持为明确完整性错误。
- 新增 `RuntimeLogSnapshot<'guard>`，把已验证 metadata、连续 runtime lines 与追加锁生命周期绑定；快照不可克隆，持锁追加辅助器消费快照，避免未来误用锁外陈旧状态。
- 普通同步、压缩同步、重建、审计序号分配、追加与修复全部改成同一 read-decision-write 锁事务；移除会二次取锁的旧辅助路径。
- 建立统一单段元数据契约：仅接受 schema 1、恰好一个 `runtime/r001.jsonl`、活动段精确匹配、`start_seq = 1`、未封存、行数/结束序号/字节数/更新时间形态一致以及受检 `next_seq`。
- 所有运行时行在落盘前校验 schema 1 与从序号 1 开始的严格连续序列；尾部追加验证每一行而非只验证第一行，所有序号推进改用 `checked_add`，拒绝饱和后重复序号。
- JSONL 容错只允许单个活动分段的物理损坏后缀；内部损坏、全损坏、缺失分段和伪多段均在任何备份或重写前失败。
- 修复流程先完成全量扫描与验证，再构造“有效前缀 + `RuntimeRepair` 事件”，备份原文件后通过一次原子替换发布，不再先截断、再追加 repair 行。
- 重建流程在覆盖前完成 conversation 投影、保留审计事件重新分配序号、完整序列验证、JSONL 序列化和长度转换；不再删除整个 `runtime/` 目录，而是原子覆盖固定 `r001`，从而保留旧段直到新候选准备完成并保留损坏备份证据。
- 新增无文件系统副作用的 `encode_json_lines`，让重建在接触现有持久化产物前证明序列化成功。
- metadata 缺失时统一检查 runtime 目录与 index：三者一致缺失才是合法 `Absent`，任何孤儿产物都返回包含精确路径的错误。
- 分叉源树复制与源 display 转换共享追加锁；目标 runtime 从验证快照开始持锁到身份 token 改写、metadata 刷新和 index 重建完成。失败清理不再吞掉 `remove_dir_all` 错误，而是同时报告原始分叉错误与清理错误。
- `runtime_log_line_matches` 改成仅在双方事件都成功序列化且值相等时匹配，不再把双侧失败折叠成相等。
- 公开 runtime 门面文档统一明确：会话缺失或运行时产物一致缺失/为空时返回 `false` 或空列表；持久化损坏与 I/O 失败返回 `Err`。

### 文件变更

- 修改：`crates/vulcan-workspace-store/src/workspace_store/runtime_logs.rs`
- 修改：`crates/vulcan-workspace-store/src/workspace_store/segmented_session_store/segment_types.rs`
- 修改：`crates/vulcan-workspace-store/src/workspace_store/segmented_session_store/segment_io.rs`
- 修改：`crates/vulcan-workspace-store/src/workspace_store/segmented_session_store/runtime_io.rs`
- 修改：`crates/vulcan-workspace-store/src/workspace_store/segmented_session_store/runtime_log.rs`
- 修改：`crates/vulcan-workspace-store/src/workspace_store/segmented_session_store/session_fork.rs`
- 修改：`crates/vulcan-workspace-store/src/workspace_store/segmented_session_store/tests/runtime_rebuild.rs`
- 修改：`crates/vulcan-workspace-store/src/workspace_store/segmented_session_store/tests/runtime_sync.rs`
- 修改：`crates/vulcan-workspace-store/src/workspace_store/segmented_session_store/tests/session_lifecycle.rs`
- 修改：`TASK_LOG.md`
- 新增：无
- 删除：无

### 修改范围代码审核与自动修复

- 第一轮审查发现初始“传播读取错误”修复仍会在全损坏时先重写为空、把缺失 metadata 与孤儿产物混同，并允许内部坏行后的有效行；已自动改为两阶段扫描、持续错误与孤儿一致性检查。
- 第二轮审查发现修复读取在持锁追加中重复取锁、审计序号在锁外分配、同步重建仍可能覆盖并发审计；已引入锁生命周期快照并把完整读—决策—写收进同一事务。
- 审查确认 metadata 的多段字段只是未落地设计预留；已删除索引参数化的 runtime 分段文件名生成能力并显式拒绝多段，避免跨段中间损坏拼接。
- 审查发现活动段不属于唯一 segment、陈旧 `next_seq`、行序重复/跳号以及尾部第二行跳号均可能在首个写字节后才暴露；已统一改成写前全量不变量校验。
- 审查发现分叉 metadata 缺失分支绕过孤儿检查，且源复制与目标改写不在稳定锁快照中；已补齐源/目标两个锁边界、严格错误传播与失败目录清理诊断。
- 审查发现修复原先分两次修改同一 segment、重建先删除 runtime 目录后才投影/序列化；已自动收敛为单次原子修复替换，并把重建全部可失败的内存工作前置。
- 审查根据 `RuntimeLogMetadata.updated_at` 的类型定义纠正了错误假设：该字段表示 metadata 最后更新时间，不强制等于最后事件时间；只有 `RuntimeSegmentMetadata.updated_at` 与尾行时间保持严格一致。
- 新增并发审计、并发重建与审计、损坏 metadata 的四类入口传播、孤儿产物、内部/全损坏、陈旧下一序号、未索引活动段、伪多段、尾部内部跳号、缺失活动段及分叉孤儿清理测试。
- “零写入”测试最终同时快照并比对 segment、metadata 与 index；并发测试同时验证 JSONL 连续序号、事件不丢失及 runtime index offset/line_count/last_seq 一致性。
- 最终 CodeKit 结构复查扫描 6 个生产文件、识别 133 个结构项、解析错误为 0；旧持锁读取辅助名、旧尾部追加入口、带索引参数的 runtime 分段生成器及关键 metadata/active-segment `exists()` 预检均为 0 命中。

### 验证结果

- 修改前基线 `cargo test -p vulcan-workspace-store --lib`：135 项全部通过。
- 最终 `cargo test -p vulcan-workspace-store --lib`：145 项全部通过。
- `cargo test -p vulcan-service-kernel --lib`：359 项全部通过，覆盖 hydration、会话持久化、压缩与审计调用链。
- `cargo test -p vulcan-task-session --lib`：64 项全部通过，覆盖 TaskSession runtime 同步消费者。
- `cargo test -p vulcan-workbench-model --lib`：75 项全部通过，覆盖工作台恢复与状态消费。
- `cargo clippy -p vulcan-workspace-store -p vulcan-service-kernel -p vulcan-task-session -p vulcan-agent-runtime --all-targets -- -D warnings`：通过，零告警。
- `cargo fmt --all -- --check`：通过。
- `git diff --check`：通过。
- 最终缺失活动段专项测试：1 项通过，144 项被过滤。

### 遗留问题与下一轮方向

- segment、metadata 与 index 仍是分别原子的文件。彻底消除进程崩溃或后续写失败造成的跨文件提交窗口，需要 WAL/commit manifest、代际目录加单指针切换，或明确的 roll-forward/rollback 恢复协议；现有辅助只支持单文件 staging + rename。这属于重大持久化架构决策，必须先取得用户确认，不能用猜测式自动恢复掩盖。
- `runtime_backup.rs` 仍在追加锁外复制 metadata、index 与 runtime 目录，之后才调用会自行取锁的审计追加；其中还保留 `read_runtime_metadata(...).ok()?`、活动段找不到时取最后段和多个 `exists()` 预检。下一轮应设计 owned-lock 审计路径并整体重构备份事务，不能简单套外层锁造成非重入死锁。
- `vulcan-context` 的 runtime projection builder 仍使用 `saturating_add`；本轮 workspace-store 写前验证已阻止重复序号落盘，但上游公共投影 API 最终应改成可失败的 checked 设计。
- 分叉的 `state.json` 与 runtime 使用不同锁，尚不构成跨产物原子快照；若要保证完整 fork snapshot，需要与跨文件提交协议一起设计。
- 损坏分段备份的复制结果尚未形成完整 fsync/目录同步协议，应与后续持久化事务方案统一处理。
- 本轮没有提交、暂存或覆盖用户其他未提交变更。

## 第 88 轮：收紧运行时压缩备份的一致性事务（2026-07-11）

### 问题与事实依据

- `runtime_backup.rs` 在修改前先以三个 `Path::exists()` 判断 metadata、index 与 runtime 目录，再在没有追加锁保护的情况下逐个复制；源状态可能在复制不同文件之间或复制与审计追加之间变化，备份并不代表任一真实时间点。
- 备份目录名称会重新读取 metadata，并通过 `.ok()?` 把读取失败折叠成未知范围；活动段找不到时回退到最后一个段，结束序号缺失时又以 `next_seq.saturating_sub(1)` 猜测，损坏状态因此会生成看似有效的备份名。
- 旧实现递归复制整个 `runtime/` 目录，但第 87 轮已经用源码构造点、校验器与测试确认当前长期契约只有 `runtime/r001.jsonl`；递归复制会把无关文件、旧垃圾或未知布局一并提升为审计备份内容。
- metadata、index 与 segment 的复制使用存在性预检和 `fs::copy`；预检与真实打开之间有竞态，必需 metadata 也会被静默当成可选文件跳过，复制完成后没有显式同步目标文件数据。
- 旧流程先清理历史备份，再调用会自行获取非重入追加锁的 `append_runtime_log_event_if_present`。直接在外层补锁会死锁，而保持原状则让备份与审计使用不同快照；清理先于审计还会在审计失败时删除旧证据。
- 清理逻辑用 `exists()` 预检根目录，并把备份目录修改时间读取失败回退成 `UNIX_EPOCH`；文件系统错误会被伪装成“最旧备份”并触发错误删除。
- 服务调用链经 `finish_webui_context_compaction_execution -> backup_session_runtime_log_before_compaction -> backup_runtime_log_before_compaction` 进入备份；服务层会记录 `context_compaction_runtime_backup_failed` 后继续压缩，因此存储层必须返回准确的部分成功诊断，不能吞掉损坏或 I/O 失败。

### 已查明的执行流程

1. 上下文压缩执行结束前，服务内核调用 workspace-store 公共备份门面；公共门面进入 segmented session store 的备份实现。
2. 第 87 轮的 `read_runtime_log_snapshot_with_owned_lock` 会在追加锁内严格分类三种状态：完整缺失返回 `None`，metadata/segment 一致且序号连续时返回生命周期绑定的 `RuntimeLogSnapshot`，损坏、孤儿或未实现布局返回 `Err`。
3. 合法 runtime 的权威持久化集合是必需 `runtime_metadata.json`、必需 `runtime/r001.jsonl` 和可选可重建 `runtime.index.json`；index 缺失不影响快照有效性，后续审计追加会重建 index。
4. 备份成功后必须向同一 runtime 追加 `ContextCompactionApplied`，其中包含实际复制路径与基于 compaction id、时间戳和路径集合计算的指纹；该事件的序号必须来自刚才验证的同一快照。
5. 审计追加会同时发布 segment、metadata 与重建后的 index，因此备份复制必须在追加前完成，并且完整过程要持有同一追加锁，才能保证备份字节严格等于审计事件之前的状态。
6. 历史备份上限为五份；只有新备份复制与审计追加都完成后才允许清理。若审计或清理失败，已经完整复制的备份仍是有效证据，应保留并返回“备份已完成但后续步骤失败”的明确错误。

### 长期优化调整

- 将备份入口改为从获取共享追加锁开始，在同一锁生命周期内完成严格快照读取、命名、精确复制、审计追加和历史清理；一致缺失直接返回空列表，损坏与孤儿状态在创建备份根目录前失败。
- 抽出 `append_runtime_log_event_to_snapshot_with_owned_lock`，让普通审计入口与备份事务共用“消费已验证快照并追加”的唯一实现，避免重复取锁、重复读取和锁外序号分配。
- 备份名称只从已验证快照的唯一 segment 与精确 `start_seq/end_seq` 派生；删除重新读取 metadata、未知范围 fallback、最后段 fallback 与 `saturating_sub` 猜测。
- 备份目录使用 `fs::create_dir` 原子创建；只有权威创建返回 `AlreadyExists` 时才生成 UUID 后缀并再次独占创建，不再执行 check-then-create。
- 删除 runtime 目录递归复制，改为固定复制 metadata、可选 index 与 `runtime/r001.jsonl`。必需文件的任何打开失败都返回错误；可选 index 只有 `File::open` 明确返回 `NotFound` 时才跳过。
- 每个目标文件都通过 `create_new` 独占创建、`std::io::copy` 流式写入并 `sync_all`；复制路径必须严格属于 session 目录，不再用原路径 fallback 掩盖归属错误。
- 复制中途失败时删除不完整备份并同时保留复制与清理错误；若目录已被并发清除导致 `NotFound`，按清理目标已经达成处理。审计或清理失败时保留完整备份，并在错误中报告其精确位置。
- 把历史清理移动到审计追加之后；根目录缺失只以 `read_dir` 的 `NotFound` 判定，目录项分类、metadata 和修改时间读取失败全部带路径传播，不再回退到 `UNIX_EPOCH`。
- 仅供测试使用的 `read_runtime_metadata` 在失去最后一个生产调用点后加上 `#[cfg(test)]`，生产构建不再保留死代码。

### 文件变更

- 修改：`crates/vulcan-workspace-store/src/workspace_store/segmented_session_store/runtime_backup.rs`
- 修改：`crates/vulcan-workspace-store/src/workspace_store/segmented_session_store/runtime_log.rs`
- 修改：`crates/vulcan-workspace-store/src/workspace_store/segmented_session_store/runtime_io.rs`
- 修改：`crates/vulcan-workspace-store/src/workspace_store/segmented_session_store/tests/runtime_sync.rs`
- 修改：`TASK_LOG.md`
- 新增：无
- 删除：无

### 修改范围代码审核与自动修复

- 首轮编译发现 `read_runtime_metadata` 已无生产调用并产生 dead-code 告警；确认全部剩余调用均位于测试后，自动限制为测试构建，而不是保留无意义生产 API。
- 首轮测试新增了三类事务证据：每轮备份的 metadata/index/segment 字节必须等于审计追加前快照；runtime 目录中的无关文件不得进入备份；缺失可重建 index 时备份仍成功且审计会重新生成 index。
- 新增损坏 metadata 与 metadata 缺失但 index/runtime 仍存在的孤儿测试，确认两类错误都在创建 `runtime_backup/` 前返回，且原 segment 字节保持不变。
- 代码审核发现备份命名对“已验证路径必有 UTF-8 stem”仍使用 `expect`；已自动改为可传播的严格错误，避免将不变量检查退化成生产 panic。
- 代码审核发现不完整目录清理若被并发操作提前完成，`remove_dir_all` 的 `NotFound` 会被误报为二次清理失败；已根据权威删除结果把该状态归类为清理目标已达成，其他错误继续同时报告。
- 精确反查确认 `runtime_backup.rs` 已无 `exists()` 预检、递归 runtime 复制、metadata 读取 fallback、旧审计入口或 `fs::copy`；仅保留明确语义的 `ok_or_else` 与受控目录删除。
- CodeKit AST 最终复查 4 个相关文件，识别 72 个结构项、解析错误为 0；新的备份模块包含 12 个职责明确的函数，锁内审计辅助函数只有一个实现入口。

### 验证结果

- 修改前基线 `cargo test -p vulcan-workspace-store --lib`：145 项全部通过。
- 最终 `cargo test -p vulcan-workspace-store --lib`：148 项全部通过。
- `cargo test -p vulcan-service-kernel --lib`：359 项全部通过，覆盖压缩调用方和错误记录链路。
- `cargo test -p vulcan-task-session --lib`：64 项全部通过。
- `cargo test -p vulcan-workbench-model --lib`：75 项全部通过。
- `cargo check -p vulcan-workspace-store`：通过，零告警。
- `cargo clippy -p vulcan-workspace-store --all-targets -- -D warnings`：通过，零告警。
- `cargo clippy -p vulcan-service-kernel -p vulcan-task-session -p vulcan-workbench-model --all-targets -- -D warnings`：通过，零告警。
- `cargo fmt --all -- --check`：通过。
- `git diff --check`：通过。

### 遗留问题与下一轮方向

- 当前追加锁是进程内全局锁；备份期间复制文件会短暂阻塞所有会话的 runtime 写入。改成严格的 per-session 锁需要统一调整全部 runtime 读写、分叉与恢复入口的锁键和生命周期，属于后续独立锁架构工作，不能在单个备份函数中局部替换。
- 备份文件已经逐个 `sync_all`，但目录项持久化以及 segment/metadata/index 的跨文件原子提交仍没有 WAL、commit manifest、代际目录指针或恢复协议；该重大持久化架构决策继续等待用户确认。
- `vulcan-context` 的 runtime projection builder 仍使用 `saturating_add`；workspace-store 会拒绝溢出后重复序号落盘，但上游公共投影 API 仍应在后续非重大范围内梳理为可失败的 checked 设计。
- 服务层当前明确选择在 runtime 备份失败后记录日志并继续压缩；本轮保持该产品语义，只保证存储层错误不再被伪装或吞掉。是否把备份失败提升为压缩阻断属于产品行为决策，需要单独确认。
- 本轮没有提交、暂存或覆盖用户其他未提交变更。

## 第 89 轮：严格化会话到运行时日志的投影状态机（2026-07-11）

### 问题与事实依据

- `vulcan-context::runtime_log_lines_from_conversation_state` 原先宣称永不失败并直接返回 `Vec<RuntimeLogEventLine>`；内部 `append` 用 `u64::saturating_add` 推进序号，到达 `u64::MAX` 后会继续生成相同序号，直到 workspace-store 的落盘前校验才发现重复序号。
- 生产调用追踪确认该投影函数只有 workspace-store 的三个消费者：锁内 runtime 重建、普通同步和压缩后同步；三者本身都返回 `Result<RuntimeLogMetadata, String>`，因此不存在必须保留“永不失败”API 的真实调用约束。
- 工具批次状态按每个 tool-call id 保存一份包含批次总调用数的克隆，关闭批次时再以 `max(tool_call_count)` 和 `saturating_add(tool_result_count)` 拼回汇总；这是重复状态，不是单一事实源。
- `open_batches` 在首次工具结果匹配后不会消费或标记该调用；同一个 call id 的第二条、第三条结果仍会被标为 `Completed` 并反复增加结果数，最终可能形成 `tool_result_count > tool_call_count` 的伪批次状态。
- 同一个批次内或跨批次出现重复 runtime tool-call id 时，`BTreeMap::insert` 会静默覆盖旧归属；重复 assistant message id 会生成相同 `tool-batch-<message-id>` 并在关闭阶段静默合并。
- 工具调用的 Provider id 会 trim 后用于 runtime id 回退判断，但事件中的 `provider_tool_call_id` 原样保留；空白 Provider id 因而可能与规范化本地 runtime id 同时落盘，形成同一调用的两套身份表达。
- `SessionConversationToolCall` 类型只声明稳定标识字段，没有构造时验证；Provider 消息转换和历史会话投影均可直接构造该类型，因此 projection 是 runtime 持久化前最后一个能够拒绝空白、重复或冲突身份的统一边界。

### 已查明的执行流程

1. 会话事实先经 `conversation_state_from_session_messages` 或 Provider/core 消息转换形成 `SessionConversationState`；该结构保留助手消息、tool calls 和后续 tool results，但不强制验证调用标识唯一性。
2. workspace-store 的重建、普通同步与压缩后同步均在持有 runtime 追加锁时调用 `runtime_log_lines_from_conversation_state(..., starting_seq = 1)`，随后才进行前缀比较、额外审计事件重排、JSONL 编码和文件发布。
3. projection 对每条用户/助手/代理消息发出消息事件；助手存在工具调用时再发出 batch-start 与每个 tool-call 事件；工具结果根据 runtime call id 查询批次并发出 completed/orphan 状态。
4. 全部消息处理后，projection 汇总仍保留的调用状态并发出 batch-completed；旧实现的结果数来自重复批次克隆求和，而不是“每个唯一调用是否已经消费一条结果”。
5. workspace-store 取得投影结果后才触碰旧 runtime 快照或发布文件，因此只要 projection 改为在错误时返回 `Err`，三个消费者都能在任何 metadata/index/segment 修改前中止。
6. runtime 读取方向经 `conversation_state_from_runtime_log_lines` 重建 Provider 会话；重复/空白身份一旦落盘会继续影响工具调用归属和历史结果降级，因此写入边界必须拒绝冲突，而不是为未知身份制造 fallback。

### 长期优化调整

- 新增公开类型 `RuntimeLogProjectionError`，明确表示序号耗尽、工具调用标识缺失、工具调用标识重复、工具批次标识重复和批次基数溢出；实现稳定英文 `Display` 与标准 `Error`，无需引入新依赖。
- `runtime_log_lines_from_conversation_state` 改为返回 `Result<Vec<RuntimeLogEventLine>, RuntimeLogProjectionError>`；runtime 模块和 crate 根继续明确重导出函数，同时导出其真实错误契约，不保留旧 infallible 包装层。
- `append` 在把事件加入结果前使用 `checked_add` 证明仍存在可表示的下一序号；若当前序号已经是 `u64::MAX`，完整投影失败，绝不返回含重复序号的部分结果。
- 工具调用 runtime id 只允许“trim 后非空的 Provider call id”或“trim 后非空的稳定本地 item id”两种源码明确来源；二者都缺失时返回类型化错误，不创建候选标识。
- 打开工具批次前先全量解析本批次调用身份，并同时检查批次内重复、历史批次重复和 tool-batch id 重复；验证完成前不发出 batch-start 事件。
- 每个唯一 tool-call 状态只保留所属 batch id 与 `matched` 布尔值；首次结果把它标为已消费并生成 `Completed`，后续同 id 结果明确生成 `Orphan`，不再重复累计完成数。
- 批次关闭时从“唯一调用条目数”和“其中 matched 为 true 的条目数”精确计算 call/result count；两个计数都使用 checked 推进，删除 runtime projection 内全部 `saturating_add`。
- 可选外部标识统一通过 `normalized_optional_identifier` 处理；空白 Provider id 持久化为 `None`，使用本地稳定 id 的 runtime 身份保持单一，工具结果的查找 id 与事件字段使用同一规范化值。
- workspace-store 三个生产调用点显式把 typed projection error 映射到现有字符串存储边界并立即传播；没有增加兼容回退或自动修复异常会话。

### 文件变更

- 修改：`crates/vulcan-context/src/runtime/projection.rs`
- 修改：`crates/vulcan-context/src/runtime/tests.rs`
- 修改：`crates/vulcan-context/src/runtime.rs`
- 修改：`crates/vulcan-context/src/lib.rs`
- 修改：`crates/vulcan-workspace-store/src/workspace_store/segmented_session_store/runtime_log.rs`
- 修改：`crates/vulcan-workspace-store/src/workspace_store/segmented_session_store/tests/runtime_sync.rs`
- 修改：`TASK_LOG.md`
- 新增：无
- 删除：无

### 修改范围代码审核与自动修复

- 首版状态机重构已删除序号和结果数的饱和推进，但 Clippy 发现一个无需闭包的 `ok_or_else`；已自动收敛为直接 `ok_or`，最终全目标零告警。
- 审核发现首次身份收紧只规范化 runtime 查找键，仍会把原始空白 Provider id 写进事件；已新增统一可选标识规范化入口，并增加“空白 Provider id + 带空白本地 id + 带空白结果 id”的完整匹配测试。
- 审核把旧批次克隆模型替换为每调用一次匹配状态与独立关闭汇总，确认平行两调用/两结果仍得到 `2/2 complete`，重复结果则严格得到 `Completed + Orphan` 和 `1/1 complete`。
- 新增序号边界测试，从 `u64::MAX - 1` 投影两条消息时精确返回 `SequenceExhausted { sequence: u64::MAX }`，不再通过 Vec 返回部分或重复序号。
- 新增同批次重复 call id、空白双重 call id、重复 tool-batch id 三类类型化错误测试，分别断言精确冲突标识和错误变体。
- workspace-store 集成测试先建立有效 runtime，再传入重复 tool-call id 的异常会话；断言错误诊断穿透字符串边界，并逐字节比较 metadata、index 与 segment，证明三类文件均零写入。
- 删除重构后不再需要的 `RuntimeOpenToolBatch: Clone` 能力，避免为旧复制模型保留无实际消费者的 trait 表面。
- CodeKit 最终反查确认 runtime projection 目录内 `saturating_add`、旧本地 id `unwrap_or_else` 和结果数饱和累计均为零命中；公开函数的全部 3 个生产调用点与 8 个测试调用点均已处理新 `Result`。
- CodeKit AST 最终复查 4 个核心实现/测试文件，识别 81 个结构项、解析错误为 0；projection 文件明确识别 typed error、5 个 builder 方法、2 个批次状态类型和2个身份解析函数。

### 验证结果

- 修改前基线 `cargo test -p vulcan-context --lib`：31 项全部通过。
- 最终 `cargo test -p vulcan-context --lib`：37 项全部通过。
- 最终 `cargo test -p vulcan-workspace-store --lib`：149 项全部通过，较第 88 轮增加 1 项零写入集成测试。
- `cargo test -p vulcan-service-kernel --lib`：359 项全部通过。
- `cargo test -p vulcan-task-session --lib`：64 项全部通过。
- `cargo test -p vulcan-workbench-model --lib`：75 项全部通过。
- `cargo clippy -p vulcan-context -p vulcan-workspace-store -p vulcan-service-kernel -p vulcan-task-session -p vulcan-workbench-model --all-targets -- -D warnings`：通过，零告警。
- `cargo fmt --all -- --check`：通过。
- `git diff --check`：通过。

### 遗留问题与下一轮方向

- 反向 `RuntimeConversationProjection::attach_tool_call` 先把工具调用写入全局 map，再在所属 assistant 不存在、索引失效或目标 payload 不是 Assistant 时静默返回；这些状态究竟应严格失败还是被定义为历史降级，必须沿 hydration、repair 与公开重放调用点继续查明，不能直接照搬正向写入规则。
- `RuntimeConversationProjection::push_tool_result` 当前不区分 `Completed` 与 `Orphan`，会先把两者都重建为 ToolResult，再交给 `normalize_replayed_protocol_messages` 二次降级；下一轮应确认这条双阶段语义是否会错误关联孤儿结果或丢失审计状态。
- `conversation_state_from_runtime_log_lines` 仍是 infallible 公共 API，自身不验证 schema、序号连续性或结构引用；workspace-store 调用前已有严格落盘验证，但其他未来调用者没有类型保障。下一轮应先完成全仓调用追踪，再决定重放层自身需要承担的最小严格不变量。
- workspace-store 仍以 `String` 作为内部存储错误边界，typed projection error 在此被格式化；若要保留跨 crate 错误 source 链，需要整体设计 workspace-store 错误类型，不能只为一个调用点局部引入混合错误模型。
- 本轮没有提交、暂存或覆盖用户其他未提交变更。

## 第 90 轮：严格化运行时日志到规范会话的反向重放（2026-07-11）

### 问题与事实依据

- `conversation_state_from_runtime_log_lines` 修改前是 infallible 公共 API：不检查 line schema、序号连续性或事件间引用，任何输入都会返回 `SessionConversationState`。
- 全仓调用追踪确认生产消费只有 workspace-store 的 `read_runtime_conversation_state`；其上游先完成物理 JSONL、metadata、schema 与序号校验，下游进入 hydration、会话加载和公开 runtime 读取门面。另有两个 vulcan-context 测试直接调用该公共 API。
- `RuntimeConversationProjection::attach_tool_call` 先把调用写入 `tool_calls` map，随后在所属 assistant 不存在、索引失效或目标 payload 不匹配时直接 `return`；调用事件从最终会话中完全消失且没有错误。
- `tool_calls` map 在旧实现中只有写入、没有任何读取；重复 runtime tool-call id 会静默覆盖，说明原本预留的关系索引没有真正承担唯一性或结果匹配职责。
- `assistant_indexes.insert` 同样会静默覆盖重复 assistant message id；后续 tool-call 会被附加到最后一条同 id 消息，使重放结果取决于覆盖顺序。
- `push_tool_result` 不区分 `Completed`、`Failed` 和 `Orphan`，三者都先重建为普通 `SessionConversationPayload::ToolResult`；后续 `normalize_replayed_protocol_messages` 再按 call id 尝试匹配。
- 因此显式标记为 `Orphan` 的结果若恰好携带一个存在的 call id，会被协议归一化重新认成合法匹配；状态语义发生反转。
- Runtime ToolResult 类型明确规定：`Completed` 与 `Failed` 是已执行且模型可见的结果，`Orphan` 是无法匹配前置调用的结果；batch/runtime/provider id 分别具有明确可选性，因此可以依据类型定义建立唯一关系，不需要候选式猜测。
- 物理 JSONL 尾部修复可能保留 batch-start/tool-call 前缀并删除损坏的后续结果或 completion；现有协议归一化会把不完整工具批次降级为历史文本。该降级是恢复语义，不能把“缺少结果或 completion”一概升级为错误。

### 已查明的执行流程

1. workspace-store 在追加锁内读取 metadata 与固定 `runtime/r001.jsonl`，验证 schema、连续序号、行数、字节数、end_seq 与 next_seq；物理损坏尾部会被裁剪并追加 `RuntimeRepair` 事件。
2. `read_runtime_conversation_state` 取得经过物理验证或修复的 lines 后调用 vulcan-context 反向重放；旧重放逐事件填充消息 Vec、assistant 索引和只写 tool-call map。
3. Assistant 事件创建规范助手消息并记录 message id 到 Vec 索引；ToolCall 通过 `assistant_message_id` 查找并附加到该消息。
4. ToolResult 旧流程不查询 tool-call map，直接生成规范 ToolResult；控制与审计事件全部忽略。
5. `finish` 调用 `normalize_replayed_protocol_messages`：相邻 assistant tool calls 只有在后续连续结果完整匹配时保留 Provider 结构，否则调用与孤儿结果会分别降级为历史 UserText。
6. 因此反向重放的严格边界应是“构建规范会话所必需且不可降级的引用必须有效”；显式 Orphan 和结果不完整批次继续走已有历史文本降级，而不是被当成存储损坏。

### 长期优化调整

- 新增公开 `RuntimeLogReplayError`，覆盖不支持 schema、序号不连续/耗尽、重复 assistant id、缺失 assistant、空白/重复/跨命名空间歧义 tool-call id、空 batch id、已匹配结果身份缺失/未知/冲突/重复以及 batch 归属冲突。
- `conversation_state_from_runtime_log_lines` 改为返回 `Result<SessionConversationState, RuntimeLogReplayError>`；以首条输入序号作为切片锚点，逐行验证当前 schema 和严格连续性，非末尾 `u64::MAX` 返回明确耗尽错误。
- `RuntimeConversationProjection::apply_event`、`push_assistant_message`、`attach_tool_call` 与 `push_tool_result` 全部改为可失败路径，删除工具关系链内的静默 `return`。
- assistant id 在正向与反向两端统一要求唯一：正向 builder 新增 assistant id 集合，即使消息没有 tool calls 也会拒绝重复；删除只在批次层检测冲突的冗余设计。
- ToolCall 重放会规范化并要求非空 runtime id 与 batch id，严格确认所属 assistant 存在；runtime id、provider id 各自唯一，且任一标识不得占用另一命名空间的已有键。
- 原只写 `tool_calls` map 改为真正的 `RuntimeReplayToolCallState` 索引，记录权威 batch 与是否已消费 matched result；同时新增 provider id 到 runtime id 的唯一反向索引。
- `Completed` 与 `Failed` 结果必须通过 runtime/provider 两个明确字段独立解析到同一前置调用；二者都缺失、任一未知、解析冲突、重复消费或显式 batch 不匹配均返回类型化错误。
- Result 的可选 batch id 会先 trim；缺失仍按类型契约允许，存在时必须与所属 ToolCall 的 batch 精确一致。
- `Orphan` 不参与任何 matched-call 消费，并在 runtime 重放阶段立即调用共享 `historical_tool_result_text_message` 转为 UserText；保留 result id、可选 call id 与正文，同时彻底阻止协议归一化再次匹配。
- 将协议层历史结果降级器提升为 `pub(crate)` 并补齐完整双语接口文档，runtime 与协议归一化共用唯一文本格式，不复制第二套降级模板。
- 不完整工具批次继续由 `normalize_replayed_protocol_messages` 降级；本轮没有强制要求 batch-completed，以保留物理尾部修复后的明确恢复能力。
- workspace-store 唯一生产调用点显式格式化并传播 typed replay error；公开存储读取仍维持既有 `Result<Option<_>, String>` 外部形状，不增加吞错包装。

### 文件变更

- 修改：`crates/vulcan-context/src/runtime/projection.rs`
- 修改：`crates/vulcan-context/src/runtime/tests.rs`
- 修改：`crates/vulcan-context/src/runtime.rs`
- 修改：`crates/vulcan-context/src/lib.rs`
- 修改：`crates/vulcan-context/src/protocol/projection.rs`
- 修改：`crates/vulcan-context/src/protocol.rs`
- 修改：`crates/vulcan-workspace-store/src/workspace_store/segmented_session_store/runtime_log.rs`
- 修改：`crates/vulcan-workspace-store/src/workspace_store/segmented_session_store/tests/runtime_rebuild.rs`
- 修改：`TASK_LOG.md`
- 新增：无
- 删除：无

### 修改范围代码审核与自动修复

- 首版严格重放已让只写 tool-call map 承担结果匹配，但审核发现正向 projection 仍允许无工具调用的重复 assistant id，能够生成反向重放必然拒绝的日志；已把 assistant 唯一性前移到正向消息入口并更新测试。
- 审核发现 runtime id 与 provider id 虽分别唯一，仍可能跨命名空间占用同一个字符串并让结果解析顺序决定归属；已新增双向冲突检查及 `AmbiguousToolCallIdentifier` 测试。
- 审核发现 ToolCall 的 batch id 仍可为空、ToolResult batch 比较未 trim；已要求调用 batch 非空，并统一规范化可选结果 batch 后再比较。
- 第二轮审核发现显式 Orphan 仍先进入普通 ToolResult，若其 ID 匹配前置调用会被协议归一化重新匹配；已改成重放阶段立即复用统一历史文本降级器，并新增“存在前置同 id 调用”的反向测试，最终只产生 UserText。
- 新增九项反向重放测试，覆盖 schema、序号缺口/耗尽、空身份、缺失 assistant、重复 assistant/runtime/provider id、跨命名空间歧义、空 batch、未知/缺失 matched identity、provider-only Failed、重复 matched result、双身份冲突、batch 冲突与显式 Orphan 降级。
- workspace-store 集成测试先由生产 projection 写出有效日志，再只篡改 ToolCall 的 assistant owner，同时重建 metadata 字节数与 index；物理验证成功后，公开读取精确传播“missing assistant”语义错误，证明关系错误不会再次被丢弃。
- CodeKit 全仓调用反查确认新 Result 只有一个生产消费者且已经显式传播；runtime projection 的工具关系路径中旧裸 `return;` 为零命中。
- CodeKit 最终 AST 复查 5 个核心文件，识别 127 个结构项、解析错误为 0；runtime projection 明确识别 2 个 typed error、正向 5 个 builder 方法、反向 7 个关系方法与3个状态/身份辅助结构。

### 验证结果

- 修改前基线 `cargo test -p vulcan-context --lib`：37 项全部通过。
- 最终 `cargo test -p vulcan-context --lib`：46 项全部通过。
- 最终 `cargo test -p vulcan-workspace-store --lib`：150 项全部通过，较第 89 轮增加 1 项语义损坏集成测试。
- `cargo test -p vulcan-service-kernel --lib`：359 项全部通过。
- `cargo test -p vulcan-task-session --lib`：64 项全部通过。
- `cargo test -p vulcan-workbench-model --lib`：75 项全部通过。
- `cargo clippy -p vulcan-context -p vulcan-workspace-store -p vulcan-service-kernel -p vulcan-task-session -p vulcan-workbench-model --all-targets -- -D warnings`：通过，零告警。
- `cargo fmt --all -- --check`：通过。
- `git diff --check`：通过。

### 遗留问题与下一轮方向

- `RuntimeToolBatchStartedEvent` 与 `RuntimeToolBatchCompletedEvent` 在反向重放中仍被完全忽略，批次开始归属、重复 batch id、completion 计数及 complete 标志尚未与 ToolCall/ToolResult 关系交叉验证。
- 物理尾部修复允许缺失 completion，因此批次审计不能简单要求“每个 start 必须有 completed”；下一轮应把“存在的控制事件必须精确一致”与“尾部缺失允许降级”分开建模。
- `RuntimeRepairEvent.downgraded_tool_segments` 当前唯一生产构造恒为 0；协议归一化实际可能降级不完整批次，但日志审计并未记录数量。若要写回真实降级计数，需要先明确只读 replay 与持久化 repair 的职责边界。
- User、Assistant 和 ToolResult 的规范消息 id 仍未建立跨事件类型的全局唯一集合；本轮只处理会影响工具归属的 assistant id。下一轮或后续应沿 merge/compaction 对 message id 的使用继续追踪后再收紧。
- workspace-store 仍以 String 作为跨模块错误边界，typed replay error 在此格式化；整体错误类型设计保持后续独立任务。
- 本轮没有提交、暂存或覆盖用户其他未提交变更。

## 第 91 轮：校验运行时工具批次控制事件与阶段顺序（2026-07-11）

### 问题与事实依据

- 第 90 轮已经严格化 ToolCall/ToolResult 关系，但 `RuntimeToolBatchStartedEvent` 与 `RuntimeToolBatchCompletedEvent` 在 `apply_event` 中仍与审计事件一起被完全忽略；它们的 assistant 归属、批次身份、调用数、结果数和 complete 标志没有任何消费者验证。
- CodeKit 全仓追踪确认 batch-start/completed 只有一个生产器：`RuntimeLogProjectionBuilder::open_tool_batch` 与 `close_remaining_batches`；workspace-store index 和 conversation-event 分类只把两者视为不投影到 Provider 会话的控制事件。
- 唯一生产顺序明确为：Assistant → BatchStarted → 该批次全部 ToolCall；所有会话消息和 ToolResult 处理完毕后，按 batch id 稳定顺序发出全部 BatchCompleted；额外 repair、compaction、injection、provider checkpoint 等审计事件再追加到其后。
- 旧反向重放即使收到重复 BatchStarted、未知 BatchCompleted、虚假 call/result count 或 complete 标志，也会生成与未损坏日志相同的会话状态；控制事件因此只是无法审计的冗余载荷。
- 物理 JSONL 尾部修复只允许损坏后缀，并会保留有效前缀后追加 RuntimeRepair；有效前缀可能包含 BatchStarted/ToolCall/Result 但缺少位于尾部的 BatchCompleted。
- `conversation_state_from_runtime_log_lines` 公共 API 也明确允许有序日志切片，因此“没有 completion”不能单独证明损坏；但一旦 completion 存在，其内容必须精确匹配前置观察状态。
- 生产器发出第一条 BatchCompleted 后不再产生任何 User、Assistant、BatchStarted、ToolCall 或 ToolResult；完成序列结束后才进入审计事件阶段，阶段顺序不可逆。

### 已查明的执行流程

1. 正向 projection 遇到含 tool calls 的 Assistant 时先发出助手消息，再生成 `tool-batch-<assistant-id>` 的 BatchStarted，并逐个发出 ToolCall。
2. 每个 ToolResult 在正向状态机中首次匹配唯一 call 时计入该 batch；重复结果变为 Orphan，不增加 matched result count。
3. 所有规范会话消息处理完成后，`close_remaining_batches` 从唯一 call 状态汇总每个 batch 的精确 call/result 数，并发出 `complete = call_count == result_count` 的 BatchCompleted。
4. workspace-store 重建会把完整 projection 放在前面，再重新分配并追加已有审计事件；普通审计追加只发生在已持久化 projection 尾部。
5. 物理后缀修复可能裁掉一部分或全部 completion，并在有效前缀后写 RuntimeRepair；它不会移除更早 BatchStarted 后继续保留内部后续有效行。
6. 反向重放完成工具关系构建后仍调用协议归一化；未完成批次会安全降级成历史文本，因此缺失 completion 无需制造兼容 fallback 或强制失败。

### 长期优化调整

- `RuntimeConversationProjection` 新增 `tool_batches`，按规范化 batch id 保存开始事件声明的 assistant、精确观察 call 数、精确 matched result 数和 completion 状态。
- BatchStarted 现在要求非空稳定 batch id、全局唯一且引用已经重放的 Assistant；空标识、重复开始和缺失 assistant 均返回新的类型化 replay error。
- ToolCall 除第 90 轮身份校验外，现在还必须引用已经开始的 batch，且自身 assistant 必须与 BatchStarted 声明一致；观察 call count 使用 checked 推进。
- Completed/Failed ToolResult 在完成唯一 call 匹配后，同时增加所属 batch 的精确 result count；Orphan 不消费 call，也不进入 batch matched 计数。
- BatchCompleted 要求非空 id、引用已开始且尚未关闭的 batch，并逐项验证 tool_call_count、tool_result_count 和 complete；完成但没有任何 call 的 batch 被明确拒绝。
- 不要求每个 started batch 最终一定有 completion：有序切片或物理损坏尾部仍可由协议归一化降级；该例外只针对缺失事件，不放宽任何已经存在的控制事件内容。
- 第一条有效 BatchCompleted 会关闭全部会话事件阶段；之后出现 User、Assistant、BatchStarted、ToolCall 或 ToolResult（包括 Orphan）统一返回 `ConversationEventAfterToolBatchCompletion`，避免关闭 A 后继续修改 B。
- 增加 audit 阶段封口：任一 repair/compaction/injection/provider checkpoint 等仅审计事件出现后，projection 与 completion 均不得重新出现，形成 conversation → completion → audit 的单向状态推进。
- 保留多个 BatchCompleted 连续出现的能力，以支持生产器按稳定 batch id 关闭多个批次；同 batch 第二条 completion 仍以精确重复关闭错误拒绝。
- 新增稳定事件类型映射 `runtime_replay_event_kind`，阶段顺序错误不依赖 Debug 文本或 serde 名称猜测。

### 文件变更

- 修改：`crates/vulcan-context/src/runtime/projection.rs`
- 修改：`crates/vulcan-context/src/runtime/tests.rs`
- 修改：`crates/vulcan-workspace-store/src/workspace_store/segmented_session_store/tests/runtime_rebuild.rs`
- 修改：`TASK_LOG.md`
- 新增：无
- 删除：无

### 修改范围代码审核与自动修复

- 首版实现只按单 batch 检查“完成后 call/result”，审核发现它会漏掉关闭 batch A 后继续修改仍开放 batch B，也会允许完成后出现 Orphan；已提升为全局 completion phase 并删除被完全覆盖的局部死分支。
- 第二轮顺序审核发现状态机仍允许审计事件后重新出现 BatchCompleted；已新增 audit phase，使 projection、completion 和 audit 只能单向推进，RuntimeRepair 后缺失 completion 继续允许，但 repair 后补 completion 明确失败。
- Clippy 捕获 matched result 计数上的不必要 `ok_or_else`；已自动改为直接 `ok_or`，最终所有目标零告警。
- 新增 batch-start 控制测试，覆盖空 id、缺失 assistant、重复 start、ToolCall 缺少 start 及 call/batch assistant 冲突。
- 新增 completion 汇总测试，覆盖完整 1/1/true 成功、未知 batch、零调用完成、虚假 call/result/complete 汇总以及 audit 后 completion。
- 新增关闭后事件测试，覆盖重复 completion、关闭后 ToolCall、关闭后 matched result 和关闭后 Orphan；缺失 completion 的相邻多批次重放测试继续通过，证明尾部降级语义未被误伤。
- workspace-store 集成测试使用生产 projection 生成有效一调用/零结果日志，仅把 BatchCompleted 的 call count 从 1 篡改为 2，再同步刷新 JSONL byte_len 与 index；物理校验通过后，公开读取精确返回 summary mismatch。
- CodeKit 反查确认反向 `apply_event` 不再存在忽略 BatchStarted/Completed 的联合分支，runtime projection 目录内饱和计数与裸静默 `return;` 均为零命中。
- CodeKit 最终 AST 复查 4 个核心文件，识别 106 个结构项、解析错误为 0；反向 projection 明确识别 9 个关系/阶段方法、2 个重放状态结构和稳定事件类型辅助函数。

### 验证结果

- 修改前基线 `cargo test -p vulcan-context --lib`：46 项全部通过。
- 最终 `cargo test -p vulcan-context --lib`：49 项全部通过。
- 最终 `cargo test -p vulcan-workspace-store --lib`：151 项全部通过，较第 90 轮增加 1 项控制汇总语义损坏集成测试。
- `cargo test -p vulcan-service-kernel --lib`：359 项全部通过。
- `cargo test -p vulcan-task-session --lib`：64 项全部通过。
- `cargo test -p vulcan-workbench-model --lib`：75 项全部通过。
- `cargo clippy -p vulcan-context -p vulcan-workspace-store -p vulcan-service-kernel -p vulcan-task-session -p vulcan-workbench-model --all-targets -- -D warnings`：通过，零告警。
- `cargo fmt --all -- --check`：通过。
- `git diff --check`：通过。

### 遗留问题与下一轮方向

- `RuntimeRepairEvent.downgraded_tool_segments` 当前唯一生产构造仍恒为 0；实际协议归一化可能降级一个或多个未完成批次，但只读 replay 只返回 `SessionConversationState`，没有把降级诊断反馈给持久化 repair。
- 若要准确记录降级数量，长期接口应返回“会话状态 + 结构化 replay diagnostics”，再由 workspace-store 在物理 repair 事务中决定是否写入；不能让纯重放函数暗中修改日志。
- User、Assistant、ToolResult 的规范消息 id 仍未建立跨事件类型的全局唯一约束；merge 与 compaction 使用 message id 作为集合键，后续应完整追踪后收紧。
- `runtime/projection.rs` 目前同时承载正向 projection、反向 replay、两类 typed error 与两套状态机，结构已经明显增长；下一轮应先用 AST 与依赖追踪确认私有边界，再将 forward/replay 分离为职责明确的模块，避免继续把新语义堆入单文件。
- workspace-store 的 String 错误边界与跨文件提交架构问题保持不变，仍需独立设计。
- 本轮没有提交、暂存或覆盖用户其他未提交变更。

## 第 92 轮：拆分运行时日志双向投影状态机与错误边界（2026-07-11）

### 问题与事实依据

- 第 91 轮结束后，`crates/vulcan-context/src/runtime/projection.rs` 已增长到 1648 行、36 个结构项，同时承载正向投影、反向重放、两类错误定义、两套状态机和共享标识规范化逻辑。
- CodeKit AST 与调用追踪确认，正向入口只依赖 `RuntimeLogProjectionBuilder`、正向批次状态和 `RuntimeLogProjectionError`；反向入口只依赖 `RuntimeConversationProjection`、反向工具关系状态和 `RuntimeLogReplayError`。
- 两条路径之间唯一实际共享的私有逻辑是 `normalized_optional_identifier`；没有跨状态机方法调用，也没有必须共享的可变状态。
- 公共 API 统一由 `crates/vulcan-context/src/runtime.rs` 导出，workspace-store 等调用方只引用 `runtime_log_lines_from_conversation_state`、`conversation_state_from_runtime_log_lines` 及两类公开错误，不依赖原文件布局。
- 因此该单文件聚合不是协议需求，而是持续叠加形成的职责混杂；继续在其中添加诊断或消息标识约束会进一步放大审核范围和修改冲突。

### 已查明的执行流程

1. 规范会话状态经 `runtime_log_lines_from_conversation_state` 进入正向 builder，按序生成 User、Assistant、BatchStarted、ToolCall、ToolResult 和 BatchCompleted 事件。
2. 正向状态机只在投影期间维护序号、助手消息身份、开放/关闭批次及工具调用身份，失败通过 `RuntimeLogProjectionError` 返回。
3. 持久化层读取 JSONL 后调用 `conversation_state_from_runtime_log_lines`，反向状态机按事件顺序验证 schema、序号、阶段、批次和工具关系，再生成规范会话状态。
4. 反向状态机只维护重放消息、工具调用索引、批次控制状态和阶段标记，失败通过 `RuntimeLogReplayError` 返回。
5. 两条路径都对可选外部标识执行“去空白、空值归无”的同一规则；该规则不发明候选来源，适合作为唯一共享私有模块。
6. `runtime.rs` 是稳定门面；内部模块重排后继续从同一公开路径重导出函数、事件和错误类型，依赖方无需改动。

### 长期优化调整

- 将正向投影实现保留在 `runtime/projection.rs`，文件只包含正向入口、builder、批次状态与工具调用标识解析。
- 新建 `runtime/replay.rs`，独立承载反向入口、重放状态机、工具调用/批次状态与事件类型诊断。
- 新建 `runtime/identity.rs`，只保存两条路径确实共享的标识规范化函数，避免建立含糊的通用工具模块。
- 代码审核发现首轮拆分后的 `replay.rs` 仍被约 443 行错误枚举与格式化实现遮蔽；进一步新建 `projection_error.rs` 和 `replay_error.rs`，使状态机实现与错误展示职责分离。
- `runtime.rs` 显式声明五个私有模块，并分别重导出两个函数和两个错误类型；公共名称、参数、返回类型与事件协议完全不变。
- 不保留旧单文件兼容层或转发别名；内部结构直接迁移到长期职责边界，避免形成第二套历史入口。

### 文件变更

- 修改：`crates/vulcan-context/src/runtime.rs`
- 修改：`crates/vulcan-context/src/runtime/projection.rs`
- 新增：`crates/vulcan-context/src/runtime/replay.rs`
- 新增：`crates/vulcan-context/src/runtime/identity.rs`
- 新增：`crates/vulcan-context/src/runtime/projection_error.rs`
- 新增：`crates/vulcan-context/src/runtime/replay_error.rs`
- 修改：`TASK_LOG.md`
- 删除：无

### 修改范围代码审核与自动修复

- 首轮按执行方向拆分后立即运行核心测试，49 项全部通过，证明事件生成与重放语义未因文件迁移改变。
- 审核模块尺寸时发现反向错误定义仍占据实现文件近四成；已自动进行第二层拆分，避免只解决文件名而没有解决职责密度。
- CodeKit 精确搜索确认 `RuntimeLogProjectionError` 与 `RuntimeLogReplayError` 各自只有一个定义，状态机模块通过明确的同级私有模块导入，不存在重复类型或旧定义残留。
- CodeKit 最终 AST 复查运行时目录 7 个文件、96 个结构项；核心 5 个拆分文件共识别 36 个结构项，解析错误为 0。
- 最终实现边界为：正向状态机约 390 行、反向状态机约 687 行、正向错误 85 行、反向错误 443 行、共享 identity 22 行；任何一类修改都不再要求扫描完整 1600 余行双向实现。
- `cargo clippy --all-targets -- -D warnings` 对五个相关 crate 零告警，无需额外兼容性修补。
- `git diff --check` 与格式检查均通过；未触碰工作区中其他轮次的未提交修改。

### 验证结果

- `cargo test -p vulcan-context --lib`：49 项全部通过。
- `cargo test -p vulcan-workspace-store --lib`：151 项全部通过。
- `cargo test -p vulcan-service-kernel --lib`：359 项全部通过。
- `cargo test -p vulcan-task-session --lib`：64 项全部通过。
- `cargo test -p vulcan-workbench-model --lib`：75 项全部通过。
- `cargo clippy -p vulcan-context -p vulcan-workspace-store -p vulcan-service-kernel -p vulcan-task-session -p vulcan-workbench-model --all-targets -- -D warnings`：通过，零告警。
- `cargo fmt --all -- --check`：通过。
- `git diff --check`：通过。

### 遗留问题与下一轮方向

- `RuntimeRepairEvent.downgraded_tool_segments` 的生产值仍恒为 0；准确诊断需要先设计“重放结果 + 结构化诊断”的纯返回边界，再由持久化事务决定写入。
- User、Assistant、ToolResult 的规范消息 id 尚未建立跨事件类型的全局唯一约束；merge/compaction 对 message id 的集合语义需要继续完整追踪。
- `RuntimeLogReplayError` 已独立但仍有 443 行，说明错误变体本身较多；后续只有在确认调用方需要结构化分类时才考虑按身份、批次、阶段细分，不能为缩短文件制造无消费方的抽象。
- workspace-store 仍把类型化 replay/projection 错误格式化为 String，跨 crate 错误边界仍需独立设计。
- 本轮没有提交、暂存或覆盖用户其他未提交变更。

## 第 93 轮：统一规范消息身份并修复 ToolResult 静默改名（2026-07-11）

### 问题与事实依据

- 正向状态机只维护 `assistant_message_ids`，因此 User、AgentMessage 和 ToolResult 可以复用已有消息 id；反向状态机也只通过 `assistant_indexes` 拒绝重复 Assistant。
- runtime index 将 User、Assistant 和 ToolResult 写入同一个 `message_offsets: BTreeMap<String, u64>`；重复 id 会让后写事件静默覆盖更早序号。
- compaction source 直接保留 `SessionConversationMessage.id`，规划与 rewrite 均通过 `position(|message| message.id == ...)` 解析边界；重复 id 会把边界确定为第一个同名消息。
- CodeKit 追踪确认 transcript merge 使用的是 `SessionConversationRecord.id`，不是本轮规范消息 id；本轮没有把两个身份域混为一谈。
- `SessionConversationMessage::ToolResult` 真实存在两个不同所有者的身份：外层 `message.id` 是本地工作台持有的协议消息 id，payload 内 `tool_result_id` 是 Provider 发出或本地合成的结果 id。
- 旧 `RuntimeToolResultEvent` 只保存 `tool_result_id`；正向投影丢弃外层 `message.id`，反向重放再用 `tool_result_id` 构造外层 id，因此两者不同时会发生无错误、无诊断的静默改名。
- transcript 恢复入口明确可能从 metadata 提取独立 `tool_result_id`，同时保留显示消息自身 id；这证明两个字段不是可以择一回退的候选路径。
- runtime 事件 schema 的唯一常量位于 `RUNTIME_LOG_SCHEMA_VERSION`，但 workspace-store 的重建、快照和尾部校验各自把事件版本硬编码成 `1`，生产者与消费者没有形成单一版本事实源。

### 已查明的执行流程

1. User、Assistant、AgentMessage 和 ToolResult 以 `SessionConversationMessage.id` 进入规范会话窗口。
2. 正向 projection 将 User/AgentMessage 写成 UserMessage，将 Assistant 写成 AssistantMessage，将 ToolResult 写成 ToolResult 事件。
3. 旧 ToolResult 事件把外层 id 仅放入语义不同且可选的 `turn_id`，把 payload 的 `tool_result_id` 当作 message index 键；正式事件结构没有本地消息 id 字段。
4. workspace-store 重建 JSONL 后调用 `rebuild_runtime_log_index`，User/Assistant 用 `message_id`，ToolResult 却用 `tool_result_id` 写同一个 message map。
5. 反向 replay 从 User/Assistant 的 `message_id` 恢复外层 id，但从 ToolResult 的 `tool_result_id` 恢复外层 id，导致双向投影不保真。
6. Provider 请求桥仍分别消费外层消息身份和结果身份；修复必须同时保留两者，不能删除 payload id 或把 `turn_id` 重新解释成消息 id。
7. runtime JSONL 的生产、重建校验、快照校验和尾部追加校验都必须接受同一事件 schema 版本；metadata/index 自身的 schema version 是独立契约，不应随事件版本一起盲目修改。

### 长期优化调整

- runtime 事件 schema 升级为 v2；`RuntimeToolResultEvent` 新增必填 `message_id`，明确保存本地协议消息身份，不提供 v1 兼容迁移或候选字段回退。
- ToolResult 正向投影同时写入外层 `message.id` 和 payload `tool_result_id`；反向重放分别恢复两个字段，双向转换不再静默改名。
- runtime index 的 ToolResult 分支改用 `event.message_id` 写入 `message_offsets`，Provider/本地结果 id 不再冒充协议消息索引键。
- 在共享 `runtime/identity.rs` 中新增单一注册器，要求规范消息 id 非空、没有首尾空白、且在整个会话窗口全局唯一；不对损坏身份执行 trim 或重新生成。
- 正向 builder 将 Assistant 专用集合替换为会话全局 `message_ids`，在任何事件写入前统一验证 User、Assistant、ToolResult 和 AgentMessage。
- 反向 projection 同样新增全局 `message_ids`；User、Assistant 和 ToolResult 在进入会话向量或助手索引前统一验证。
- 两类 typed error 删除 Assistant 专用重复变体，新增缺失、非规范和跨类型重复消息身份错误，并保留精确消息类型与原始 id。
- workspace-store 的三处事件版本检查全部改为引用 `RUNTIME_LOG_SCHEMA_VERSION`；runtime metadata schema 仍保持自己的 v1，避免错误耦合。

### 文件变更

- 修改：`crates/vulcan-context/src/runtime/events.rs`
- 修改：`crates/vulcan-context/src/runtime/identity.rs`
- 修改：`crates/vulcan-context/src/runtime/projection.rs`
- 修改：`crates/vulcan-context/src/runtime/projection_error.rs`
- 修改：`crates/vulcan-context/src/runtime/replay.rs`
- 修改：`crates/vulcan-context/src/runtime/replay_error.rs`
- 修改：`crates/vulcan-context/src/runtime/tests.rs`
- 修改：`crates/vulcan-workspace-store/src/workspace_store/segmented_session_store.rs`
- 修改：`crates/vulcan-workspace-store/src/workspace_store/segmented_session_store/runtime_io.rs`
- 修改：`crates/vulcan-workspace-store/src/workspace_store/segmented_session_store/runtime_log.rs`
- 修改：`crates/vulcan-workspace-store/src/workspace_store/segmented_session_store/tests/runtime_rebuild.rs`
- 修改：`crates/vulcan-workspace-store/src/workspace_store/session_load/tool_detail_hydration.rs`
- 修改：`TASK_LOG.md`
- 新增：无
- 删除：无

### 修改范围代码审核与自动修复

- 首轮存储测试出现 28 项失败，全部指向“schema version 2 不受支持”；沿调用链定位到重建、快照和尾部校验中的三处硬编码 `!= 1`，已统一改为事件 schema 常量，152 项存储测试随后全部通过。
- 精确搜索确认 metadata 的 `schema_version != 1` 属于独立 metadata 文件契约，未被错误替换。
- CodeKit 搜索确认旧 `DuplicateAssistantMessageIdentifier` 在全仓零残留；前向与反向都只通过共享注册器建立消息唯一性。
- 新增跨 Assistant/User 类型重复测试，证明 id 唯一性不是分角色命名空间；新增空白与首尾空白测试，证明不会静默 trim 身份。
- 新增 ToolResult 双身份 round-trip 测试，使用 `message-result-1` 与 `provider-result-1` 两个真实独立值，验证事件及重放状态均保真。
- 新增持久化索引测试，验证 `message_offsets` 包含本地 ToolResult 消息 id 且不包含 Provider 结果 id。
- CodeKit 最终复查 7 个核心文件、95 个结构项，解析错误为 0；全仓只存在 4 个 ToolResult 事件构造点，均已提供必填 `message_id`。
- schema 检索确认 workspace-store 的三条事件版本验证链均引用 `RUNTIME_LOG_SCHEMA_VERSION`，旧事件版本硬编码为零。

### 验证结果

- `cargo test -p vulcan-context --lib`：52 项全部通过，较第 92 轮增加 3 项身份与双向保真测试。
- `cargo test -p vulcan-workspace-store --lib`：152 项全部通过，较第 92 轮增加 1 项持久化索引测试。
- `cargo test -p vulcan-context-compaction --lib`：21 项全部通过。
- `cargo test -p vulcan-agent-runtime --lib`：583 项全部通过。
- `cargo test -p vulcan-service-kernel --lib`：359 项全部通过。
- `cargo test -p vulcan-task-session --lib`：64 项全部通过。
- `cargo test -p vulcan-workbench-model --lib`：75 项全部通过。
- `cargo clippy -p vulcan-context -p vulcan-context-compaction -p vulcan-workspace-store -p vulcan-agent-runtime -p vulcan-service-kernel -p vulcan-task-session -p vulcan-workbench-model --all-targets -- -D warnings`：通过，零告警。
- `cargo fmt --all -- --check`：通过。
- `git diff --check`：通过。

### 遗留问题与下一轮方向

- runtime schema v2 有意不读取 v1 ToolResult 事件；本任务明确不考虑历史兼容，因此未增加迁移器、默认字段或 `turn_id` 回退。
- payload `tool_result_id` 已与协议消息 id 分离，但其自身“stable”契约尚未验证非空、规范形式或重复语义；下一轮应沿 Provider 桥、display metadata 和工具执行生产器确认唯一所有者后再决定约束。
- `SessionConversationRecord.id` 是 transcript merge 与 compaction checkpoint 使用的另一身份域，本轮只查明边界，未把规范消息不变量错误套用到记录。
- `RuntimeRepairEvent.downgraded_tool_segments` 仍恒为 0，结构化 replay diagnostics 设计保持待办。
- workspace-store 仍把类型化 replay/projection 错误格式化为 String，跨 crate 错误边界仍需独立设计。
- 本轮没有提交、暂存或覆盖用户其他未提交变更。

## 第 94 轮：删除规范 ToolResult 伪身份并统一显示结果所有权（2026-07-11）

### 问题与事实依据

- 第 93 轮把 ToolResult 外层 `message.id` 与 payload `tool_result_id` 正确分离后，继续追踪发现 payload 字段本身没有独立业务实体。
- `VulcanCoreController::append_tool_result_message` 为同一条结果连续消耗 `next_local_id("tool-result-message")`、`next_local_id("tool-result")` 和 `next_local_id("tool-result-record")`；其中第二个 id 没有进入索引、匹配或 Provider 协议。
- Provider 工具结果关联只使用 `tool_call_id`；OpenAI、Anthropic、Google、Cohere 等传输实现均忽略规范 payload 的 `tool_result_id`。
- Provider history projection 只在孤儿降级文本、部分批次文本和请求预览中打印该 id；这些位置已经拥有更稳定的外层 `message.id`。
- WebUI 中的 `tool_result_id` 属于另一个显示身份域：它按 `tool-result-{tool_call_id}` 派生，用于工具卡片合并与详情展示，不等于 Core 通过本地计数器生成的 payload id。
- 正常显示投影从 ToolFinished metadata 读取或回退生成 `tool_result_id`；开放工具补全路径又独立写了一次相同 format，形成两个生产器和一个可被外部 metadata 覆盖的身份。
- workspace-store 在缺失详情水合时把 runtime payload id 写回显示 metadata，会把 Core 本地计数 id 误当成 `tool-result-{tool_call_id}`，与 WebUI 的派生规则产生语义错位。
- `build_tool_result_runtime_metadata` 声明返回 `Option<Value>`，但所有路径恒定返回 `Some`，调用方被迫处理一个永远不存在的空状态。

### 已查明的执行流程

1. Provider Assistant 发出工具调用后，Core 通过 `ToolInvocationRequest.tool_call_id` 执行工具。
2. Core 收到 `ToolInvocationResult` 后，规范消息使用外层 `message.id` 标识结果实体，并以 `tool_call_id` 关联所属调用。
3. 旧 Core 额外生成 payload `tool_result_id`，随后 session/core 转换、runtime projection 和 replay 只做无条件复制。
4. Provider request bridge 匹配结果时只检查 `tool_call_id`；payload id 仅被拼入诊断文字，不参与拓扑判定。
5. StreamingRuntimeToolExecutor 同时生成显示 `tool-result-{tool_call_id}`、写入 RuntimeEvent metadata，并把它重复打印进 Markdown 详情。
6. session event reducer再次从 metadata 读取该值；缺失时又自行格式化同一字符串，然后写入可见结果 metadata 与隐藏详情 metadata。
7. WebUI 优先使用 `tool_call_id`，仅把显示 `tool_result_id` 当作工具执行卡片身份回退；因此规范 payload id 与 UI 显示 id 没有同一所有者。
8. runtime JSONL 与 workspace hydration 已有本地 `message_id` 和调用 id，删除 payload id 后仍能完整重放、索引和合成详情。

### 长期优化调整

- 从 `vulcan-core::ConversationPayload::ToolResult` 和 `SessionConversationPayload::ToolResult` 删除 `tool_result_id`；规范结果实体只由外层 `message.id` 标识。
- Core 不再为每次工具结果额外消耗 `next_local_id("tool-result")`，本地 id 计数只服务真实消息和记录。
- runtime schema 升级为 v3，并从 `RuntimeToolResultEvent` 删除 payload id；事件只保留 `message_id`、调用关系、批次、文本和状态。
- replay 的五类工具结果关系错误将诊断字段统一改为 `message_id`，不再把不存在的结果实体 id 写进错误模型。
- 历史降级、compaction source、Provider orphan/partial-batch 文本和请求预览全部改用外层消息 id；转换层不再复制伪字段。
- workspace 缺失详情水合只写可靠的 `tool_call_id`，不再把 runtime 消息 id 伪装成显示 `tool_result_id`。
- RuntimeEvent metadata 不再携带结果 id，工具详情 Markdown 也删除冗余 Result ID 标题；ToolFinished 标题和 metadata 中的 `tool_call_id` 是调用身份来源。
- 显示层 `tool_result_id` 仍保留在可见/隐藏记录 metadata，但只能通过共享 `build_host_tool_result_id(tool_call_id)` 生成；开放工具补全与正常完成路径均调用该函数。
- reducer 不再信任或读取传入 metadata 的结果 id；新增冲突输入测试，证明外部值无法覆盖 call-derived 显示身份。
- `build_tool_result_runtime_metadata` 改为确定返回 `Value`，删除恒定 `Some` 的伪可选性。

### 文件变更

- 修改：`crates/vulcan-core/src/types.rs`
- 修改：`crates/vulcan-core/src/controller/transcript.rs`
- 修改：`crates/vulcan-core/src/controller/tests/tool_batches.rs`
- 修改：`crates/vulcan-context/src/assembly.rs`
- 修改：`crates/vulcan-context/src/protocol/types.rs`
- 修改：`crates/vulcan-context/src/protocol/projection.rs`
- 修改：`crates/vulcan-context/src/runtime/events.rs`
- 修改：`crates/vulcan-context/src/runtime/projection.rs`
- 修改：`crates/vulcan-context/src/runtime/replay.rs`
- 修改：`crates/vulcan-context/src/runtime/replay_error.rs`
- 修改：`crates/vulcan-context/src/runtime/tests.rs`
- 修改：`crates/vulcan-context-compaction/src/rewrite.rs`
- 修改：`crates/vulcan-agent-runtime/src/agent_runtime/provider_protocol_bridge.rs`
- 修改：`crates/vulcan-agent-runtime/src/agent_runtime/provider_transport_stream_executor.rs`
- 修改：`crates/vulcan-agent-runtime/src/agent_runtime/provider_request_bridge_messages/conversions.rs`
- 修改：`crates/vulcan-agent-runtime/src/agent_runtime/provider_request_bridge_messages/projection.rs`
- 修改：`crates/vulcan-agent-runtime/src/agent_runtime/provider_request_preview.rs`
- 修改：`crates/vulcan-agent-runtime/src/session_event_runtime.rs`
- 修改：`crates/vulcan-agent-runtime/src/session_event_runtime/open_tool_records.rs`
- 修改：`crates/vulcan-agent-runtime/src/session_event_runtime/tool_event_records.rs`
- 修改：`crates/vulcan-agent-runtime/src/session_event_runtime/tests.rs`
- 机械调整测试夹具：agent-runtime 的 Provider bridge、Anthropic、Google、Cohere、OpenAI-compatible、OpenAI Responses 与 turn-request 测试文件，共 14 个文件。
- 修改：`crates/vulcan-workspace-store/src/workspace_store/session_load/tool_detail_hydration.rs`
- 修改：`crates/vulcan-workspace-store/src/workspace_store/segmented_session_store/tests/runtime_rebuild.rs`
- 修改：`TASK_LOG.md`
- 新增：无
- 删除：无

### 修改范围代码审核与自动修复

- 首轮删除规范字段后，编译器定位到 `assembly::render_message_layer_content` 的最后一个生产消费点；已改为显示外层 `message.id`。
- 机械删除 16 个文件中的过期结构字段后，Core、Context、Agent Runtime 和 Workspace Store 均一次编译通过；没有使用 serde 默认值或历史字段 fallback 掩盖遗漏。
- 审核发现 ToolFinished metadata 仍把显示结果 id作为输入字段传播；已从 runtime metadata builder 与 Markdown detail 中删除，并让 reducer只按 `tool_call_id` 重新派生。
- 审核发现开放工具补全路径重复实现 `format!("tool-result-{tool_call_id}")`；已改为共享 helper，全仓生产代码只剩一个格式化实现。
- 审核发现 metadata builder 的 `Option<Value>` 永远为 `Some`；已收紧为 `Value`，调用方显式在真正可选的事件 metadata 边界包装。
- 新增/调整测试证明 runtime schema v3 序列化结果没有 `tool_result_id`、ToolResult round-trip 只保留 message id 与 call id、workspace index 只有真实消息键。
- 将 ToolFinished 测试输入改为冲突的 `untrusted-result-id`，最终显示 metadata 仍为 `tool-result-call-1`，证明 metadata 不能夺取身份所有权。
- CodeKit 精确搜索确认 `tool_result_id: String` 在全仓零定义，Core 的隐藏 `next_local_id("tool-result")` 为零，runtime 生产结构中该字段为零。
- CodeKit 最终复查 10 个核心文件、211 个结构项，解析错误为 0；剩余 Rust `tool_result_id` 仅属于显示 metadata、Anthropic 上游块字段或测试断言。

### 验证结果

- `cargo test -p vulcan-core --lib`：15 项全部通过。
- `cargo test -p vulcan-context --lib`：52 项全部通过。
- `cargo test -p vulcan-context-compaction --lib`：21 项全部通过。
- `cargo test -p vulcan-agent-runtime --lib`：583 项全部通过。
- `cargo test -p vulcan-workspace-store --lib`：152 项全部通过。
- `cargo test -p vulcan-service-kernel --lib`：359 项全部通过。
- `cargo test -p vulcan-task-session --lib`：64 项全部通过。
- `cargo test -p vulcan-workbench-model --lib`：75 项全部通过。
- 合计 1321 项相关测试全部通过。
- `cargo clippy -p vulcan-core -p vulcan-context -p vulcan-context-compaction -p vulcan-agent-runtime -p vulcan-workspace-store -p vulcan-service-kernel -p vulcan-task-session -p vulcan-workbench-model --all-targets -- -D warnings`：通过，零告警。
- `cargo fmt --all -- --check`：通过。
- `git diff --check`：通过。

### 遗留问题与下一轮方向

- runtime schema v3 有意不读取 v2 ToolResult 事件；按本任务要求没有添加迁移器、默认字段或兼容分支。
- 显示 metadata 仍保留可由 `tool_call_id` 完全推导的 `tool_result_id`，WebUI 也保留从该字段反推 call id 的 fallback；下一轮可沿新旧转录输入事实判断是否彻底删除显示冗余。
- `SessionConversationRecord.id` 仍是 transcript merge 与 compaction checkpoint 的独立身份域，尚未验证非空、规范形式和全局唯一性。
- `RuntimeRepairEvent.downgraded_tool_segments` 仍恒为 0，结构化 replay diagnostics 设计保持待办。
- workspace-store 仍把类型化 replay/projection 错误格式化为 String，跨 crate 错误边界仍需独立设计。
- 本轮没有提交、暂存或覆盖用户其他未提交变更。

## 第 95 轮：删除 WebUI ToolResult 身份回退并统一工具调用主键（2026-07-11）

### 问题与事实依据

- 第 94 轮已证明规范 ToolResult 只需要 `message.id` 与 `tool_call_id`，但显示记录 metadata 和 WebUI 模型仍保留 `tool_result_id`。
- Rust 的正常完成记录、开放工具补全记录、隐藏详情记录以及浏览器预览夹具都同时提供 `tool_call_id`；没有一个正式生产路径只提供结果 id。
- 显示 `tool_result_id` 始终由 `tool-result-{tool_call_id}` 派生，不承载独立数据；WebUI 又通过 `toolCallIdFromToolResultId` 把它反向拆回 call id。
- `ConversationBlock` 与 `ToolExecutionConversationBlock` 各自保存 `toolResultId`，导致同一个冗余字段在解析模型和工具展示模型中重复传播。
- transcript parser 同时从结构化 metadata、legacy Markdown 的 `Result ID` 标题、`tool_result_id=...` 文本中恢复该字段，形成三套历史输入入口。
- 工具块合并先尝试 `toolCallId`，再从 `toolResultId` 反推同一 key；五类展示组件也使用 `toolCallId ?? toolResultId ?? block.id` 生成 presentation id。
- 工具执行状态把“存在 toolResultId”作为 completed 证据，但正式结果块、显式 status 和 detail_ref 已分别提供更直接的完成事实。
- Web 工具证据判断把 `tool_result_id` 与 call id、命令会话、子代理任务并列，允许仅有历史结果字符串的普通正文被提升成工具执行块。
- Anthropic 层的 `anthropic_tool_result_id` 实际读取上游 `tool_use_id`，命名继续制造与已删除宿主字段的混淆。

### 已查明的执行流程

1. ToolFinished reducer从事件标题或 metadata 得到唯一 `tool_call_id`，并把它写入可见工具记录与隐藏详情记录。
2. Web `structuredMessageAttributes` 从记录 metadata 读取 `tool_call_id`，再把它投影成 `ConversationBlock.toolCallId`。
3. `projectConversationRenderItems` 使用 call/command 基础键建立带实例序号的合并 key，避免历史相同调用 id 互相覆盖。
4. tool-result、command、diff 等结果块通过相同 call key并入最近的工具调用实例；没有 key 的结果块保持独立行，不会错误并入其他调用。
5. 工具执行状态优先读取 explicit status，其次读取 detail_ref，再依据结果块类型判定 completed；结果 id不是必要输入。
6. Ask、Command、File Change、Markdown Output 和 Subagent presentation 只需要 call id；缺少 call id 时使用块自身稳定 id即可。
7. 浏览器 fallback 的预览 ToolResult 已携带 `tool_call_id: preview-service-startup`，删除派生字段不会改变预览合并。
8. Anthropic 原生 JSON 中结果块关联字段是 `tool_use_id`，与宿主显示字段无关，应按调用关系命名。

### 长期优化调整

- 从 Rust 的可见结果 metadata 与隐藏详情 metadata 删除 `tool_result_id`，`normalize_tool_result_metadata` 和 `append_tool_result_detail_record` 不再接收该参数。
- 删除 `build_host_tool_result_id` 及开放工具/正常完成路径中的派生变量；显示结果身份只保留真实 `tool_call_id`。
- 删除 RuntimeEvent、context/compaction/service/workspace 测试夹具和浏览器预览夹具中的冗余显示字段。
- 从 `ConversationBlock` 和 `ToolExecutionConversationBlock` 删除 `toolResultId` 类型字段。
- transcript parser 不再读取 metadata 的结果 id，不再解析 `Result ID` 标题或 `tool_result_id=...` 历史文本，也不再把它写入 block attributes。
- 工具证据判断只接受 call id、command session id、subagent task id、明确 tool evidence 或受信 host kind。
- 工具块合并 key 只由 `toolCallId` 与 `commandSessionId` 构成，删除 `toolCallIdFromToolResultId` 的字符串反向解析。
- 工具完成状态不再依赖结果 id；explicit status、detail_ref 及结果块类型继续覆盖正式完成语义。
- 五类 presentation id 改为 `toolCallId ?? block.id`，不再形成第三层身份 fallback。
- Web 的返回载荷判断删除结果 id证据，只检查非空正文或结构化 host result。
- 将 Anthropic helper 重命名为 `anthropic_tool_result_call_id`，测试同步改为 `mismatched_tool_use_id`，明确其属于上游调用关联。

### 文件变更

- 修改：`apps/vulcan-code-web/src/components/conversation/conversation-model.ts`
- 修改：`apps/vulcan-code-web/src/components/conversation/conversation-classifiers.ts`
- 修改：`apps/vulcan-code-web/src/components/conversation/conversation-tool-execution-types.ts`
- 修改：`apps/vulcan-code-web/src/components/conversation/conversation-tool-execution-model.ts`
- 修改：`apps/vulcan-code-web/src/components/conversation/conversation-tool-metadata.ts`
- 修改：`apps/vulcan-code-web/src/components/conversation/conversation-transcript-parser.ts`
- 修改：`apps/vulcan-code-web/src/components/conversation/conversation-transcript.ts`
- 修改：`apps/vulcan-code-web/src/components/conversation/tool-presentations/ask-question-presentation.ts`
- 修改：`apps/vulcan-code-web/src/components/conversation/tool-presentations/command-execution-presentation.ts`
- 修改：`apps/vulcan-code-web/src/components/conversation/tool-presentations/file-change-presentation.ts`
- 修改：`apps/vulcan-code-web/src/components/conversation/tool-presentations/markdown-output-presentation.ts`
- 修改：`apps/vulcan-code-web/src/components/conversation/tool-presentations/subagent-orchestration-presentation.ts`
- 修改：`apps/vulcan-code-web/src/service/browser-fallback-workbench.ts`
- 修改：`crates/vulcan-agent-runtime/src/agent_runtime/provider_protocol_bridge.rs`
- 修改：`crates/vulcan-agent-runtime/src/agent_runtime/executor.rs`
- 修改：`crates/vulcan-agent-runtime/src/session_event_runtime.rs`
- 修改：`crates/vulcan-agent-runtime/src/session_event_runtime/open_tool_records.rs`
- 修改：`crates/vulcan-agent-runtime/src/session_event_runtime/tool_event_records.rs`
- 修改：`crates/vulcan-agent-runtime/src/session_event_runtime/tool_display_payloads.rs`
- 修改：`crates/vulcan-agent-runtime/src/session_event_runtime/tests.rs`
- 修改：`crates/vulcan-agent-runtime/src/agent_runtime/provider_transport_anthropic_http/topology.rs`
- 修改：`crates/vulcan-agent-runtime/src/agent_runtime/provider_transport_anthropic_http/tests/tool_result_topology.rs`
- 修改测试夹具：context protocol、context compaction、service kernel 与 workspace-store 的相关测试文件。
- 修改：`TASK_LOG.md`
- 新增：无
- 删除：无

### 修改范围代码审核与自动修复

- 首版删除 Web 字段后，精确搜索发现 executor 测试仍导入已删除的 `build_host_tool_result_id`；已删除导入并把测试收紧为只验证 Provider call id。
- 审核发现 session event 测试仍断言 `result.tool_result_id`；已改为负向断言，确保可见结果对象也不再写入该字段。
- 审核发现 browser fallback 在 conversation 目录外仍保存结果 id；已删除，同时保留其正式 `tool_call_id`。
- 审核发现 Anthropic helper 名称与上游真实字段不一致；已按 `tool_use_id` 的调用关联语义重命名，避免删除宿主字段后留下概念污染。
- CodeKit 检查 `toolMergeInstanceKeysForBlock`：无 call/command key 的结果块返回空 key并独立渲染，不会因为删除 fallback 而错误合并。
- CodeKit 检查 `toolExecutionDisplayState`：显式 failed/stopped/completed、detail_ref、结果块类型和 session running 状态形成完整决策链。
- CodeKit 最终复查 7 个核心文件、146 个结构项，解析错误为 0。
- Web 源码中 `toolResultId`、`tool_result_id` 和 `Result ID` 均为零命中。
- Rust 全仓只剩 4 个确认 `tool_result_id` 不存在的负向测试断言；生产、类型和 metadata 写入均为零。

### 验证结果

- `npm run lint`（`apps/vulcan-code-web`）：通过。
- `npm run build`（`apps/vulcan-code-web`）：TypeScript 与 Vite 生产构建通过，2363 个模块完成转换；仅保留既有大 chunk 提示。
- `cargo test -p vulcan-core --lib`：15 项全部通过。
- `cargo test -p vulcan-context --lib`：52 项全部通过。
- `cargo test -p vulcan-context-compaction --lib`：21 项全部通过。
- `cargo test -p vulcan-agent-runtime --lib`：583 项全部通过。
- `cargo test -p vulcan-workspace-store --lib`：152 项全部通过。
- `cargo test -p vulcan-service-kernel --lib`：359 项全部通过。
- `cargo test -p vulcan-task-session --lib`：64 项全部通过。
- `cargo test -p vulcan-workbench-model --lib`：75 项全部通过。
- 合计 1321 项 Rust 相关测试全部通过。
- `cargo clippy -p vulcan-core -p vulcan-context -p vulcan-context-compaction -p vulcan-agent-runtime -p vulcan-workspace-store -p vulcan-service-kernel -p vulcan-task-session -p vulcan-workbench-model --all-targets -- -D warnings`：通过，零告警。
- `cargo fmt --all -- --check`：通过。
- `git diff --check`：通过。

### 遗留问题与下一轮方向

- legacy Tool Result Markdown 仍可通过明确的 `## Tool Result` 或“工具返回”标记识别为独立结果块，但不再从历史结果 id猜测调用归属；这是安全的独立展示，不是兼容性合并。
- `SessionConversationRecord.id` 是 transcript merge、详情引用和 compaction checkpoint 的下一处独立身份域，仍需验证非空、规范形式与全局唯一性。
- `RuntimeRepairEvent.downgraded_tool_segments` 仍恒为 0，结构化 replay diagnostics 设计保持待办。
- workspace-store 仍把类型化 replay/projection 错误格式化为 String，跨 crate 错误边界仍需独立设计。
- 本轮没有提交、暂存或覆盖用户其他未提交变更。

## 第 96 轮：建立会话记录身份不变量并删除破坏性静默修复（2026-07-11）

### 问题与事实依据

- `SessionConversationRecord.id` 是记录流的真实主键，但原模型只声明字段，没有校验非空、规范文本或全局唯一性。
- 流式正文增量、工具结果挂接、回合统计、上下文起点、编辑分叉和压缩锚点均通过记录 ID 使用 `find` 或 `position` 定位；重复值会稳定但错误地命中第一条记录。
- 转录投影把记录 ID 写入 `SessionMessageEntry.id`，合并阶段用集合判定所有权；重复 ID 会让后续记录被折叠到错误的显示身份。
- 工具详情引用保存 `record_id`，workspace 水合再把记录 ID 映射到分段序号；同名记录会覆盖映射并使详情归属不可判定。
- 旧加载逻辑 `repair_duplicate_conversation_record_ids` 只重命名后出现的重复记录，却不更新详情引用、压缩锚点、关闭记录 ID 或工具映射，因此所谓修复会主动制造悬空引用。
- 旧逻辑以时间戳和数组下标生成替代 ID，属于在身份来源不明时猜测新身份，与长期唯一身份语义冲突。

### 已查明的执行流程

1. agent runtime 创建记录并将记录 ID同步保存到流式槽位、工具调用记录映射或回合关闭状态。
2. 后续 delta、ToolFinished 和 turn stats 依靠该 ID回查原记录；查找语义是首个匹配项，不具备重复值消歧能力。
3. context projection 使用记录 ID生成可见转录身份，并以相同 ID确定活动上下文起点。
4. compaction 使用记录 ID选择截断锚点并在 checkpoint merge 中去重；重复值会改变保留窗口。
5. workspace persistence 将完整状态投影为 `state.json` 元数据与 provider-facing runtime JSONL；分叉和边界快照也复用这些共享写入函数。
6. workspace hydration 优先从 runtime JSONL 重建规范会话，再叠加 state/addon 状态；旧实现会在返回前静默改名。
7. 因此身份不变量必须位于共享协议层，并同时在所有规范读写边界验证；仅修复单个调用点无法阻断旁路。

### 长期优化调整

- 在 `vulcan-context` 新增类型化 `SessionConversationRecordIdentityError`，精确区分空 ID、首尾空白和重复 ID，并携带记录下标及冲突值。
- 新增 `validate_session_conversation_record_identities`，一次扫描完成规范文本与全局唯一性校验，不修改输入状态。
- 从 workspace 水合删除 `repair_duplicate_conversation_record_ids` 及其时间戳猜测逻辑；runtime JSONL 重建和最终水合状态现在遇到损坏身份会显式返回错误。
- 将写入校验下沉到 `state_document_from_runtime_state`、普通 runtime log 同步、压缩后 runtime log 同步和直接重建入口，覆盖常规保存、分叉、边界快照及日志重建旁路。
- 删除仅被旧修复逻辑使用的 `non_empty_timestamp_text` 和失效的测试导入。
- 新增协议层回归测试，固定重复、空值和带首尾空白标识均被拒绝且不会被自动改写。

### 文件变更

- 新增：`crates/vulcan-context/src/protocol/identity.rs`
- 修改：`crates/vulcan-context/src/protocol.rs`
- 修改：`crates/vulcan-context/src/protocol/tests.rs`
- 修改：`crates/vulcan-workspace-store/src/lib.rs`
- 修改：`crates/vulcan-workspace-store/src/workspace_store/serialization.rs`
- 修改：`crates/vulcan-workspace-store/src/workspace_store/session_load.rs`
- 修改：`crates/vulcan-workspace-store/src/workspace_store/session_load/hydration.rs`
- 修改：`crates/vulcan-workspace-store/src/workspace_store/session_load/runtime_state.rs`
- 修改：`crates/vulcan-workspace-store/src/workspace_store/session_load/tests.rs`
- 修改：`crates/vulcan-workspace-store/src/workspace_store/segmented_session_store/state_documents.rs`
- 修改：`crates/vulcan-workspace-store/src/workspace_store/segmented_session_store/runtime_log.rs`
- 修改：`TASK_LOG.md`
- 删除：无

### 修改范围代码审核与自动修复

- 首版仅在常规 runtime checkpoint 写入入口校验；全路径搜索发现 boundary snapshot、runtime-log rebuild 和 edit/fork 可直接调用底层写入函数，已把校验下沉到共享状态文档与运行日志边界。
- 审核发现直接 `rebuild_runtime_log_from_conversation_state` 不经过两个 sync wrapper；已补充入口校验，避免重建路径绕过身份不变量。
- 删除旧修复测试后，Clippy 发现 `session_load/tests.rs` 与 workspace-store 测试别名仍导入 `SessionConversationRecord`；已自动清理。
- 删除旧修复函数后，编译发现 workspace serialization 中的同名时间戳 fallback 已无调用；确认另一分段消息模块拥有独立实现后，仅删除无用函数，没有误删真实调用路径。
- CodeKit 最终结构扫描覆盖 4 个核心文件、52 个结构项，解析错误为 0。
- `git diff --check` 通过，未发现空白错误。

### 验证结果

- `cargo test -p vulcan-context -p vulcan-workspace-store`：205 项全部通过。
- `cargo test -p vulcan-context -p vulcan-context-compaction -p vulcan-agent-runtime -p vulcan-workspace-store`：809 项全部通过。
- `cargo clippy -p vulcan-context -p vulcan-context-compaction -p vulcan-agent-runtime -p vulcan-workspace-store --all-targets -- -D warnings`：通过，零告警。
- `cargo fmt --all -- --check`：通过。
- `git diff --check`：通过。

### 遗留问题与下一轮方向

- 本轮按“不考虑历史兼容”要求明确拒绝重复或非规范记录 ID，不提供旧会话迁移器或猜测式重命名。
- `SessionConversationMessage.id` 是 provider-facing 活动窗口的另一身份域，追加函数仍直接接受任意字符串；下一轮可完整追踪其生成、replay、compaction 与 continuation 关系后建立对应不变量。
- workspace-store 目前把类型化身份错误格式化为英文 `String` 以服从现有存储边界；跨 crate 统一类型化错误仍需独立设计。
- `RuntimeRepairEvent.downgraded_tool_segments` 仍恒为 0，结构化 replay diagnostics 设计保持待办。
- 本轮没有提交、暂存或覆盖用户其他未提交变更。

## 第 97 轮：统一规范消息身份并阻断 Provider 桥接脏状态（2026-07-11）

### 问题与事实依据

- `SessionConversationMessage.id` 同时承担活动窗口主键、runtime 事件 `message_id`、`turn_id`、压缩锚点与 TaskSession 上下文身份，但协议模型原先没有可复用的身份校验入口。
- runtime JSONL 投影与重放已经在私有 `runtime/identity.rs` 中拒绝空白、首尾空白及重复消息 ID，说明系统事实上依赖全窗口唯一性。
- Provider 请求桥接 `build_core_conversation_snapshot` 原先是无错误返回的逐条复制；内存中的脏消息身份可以先进入 `vulcan-core` 和 Provider 请求，直到后续 runtime 日志持久化才报错。
- core 返回宿主状态的反向桥接同样无错误返回，TaskSession 准备、检查点、终态和持久化路径会直接接受未校验的消息窗口。
- compaction summary ID 会写入新窗口首项并同步成为 `context_start_message_id` 和 checkpoint `summary_message_id`；若与保留尾部重复，锚点归属会不可判定。
- workspace checkpoint 先构建并写入 `state.json`，再同步 runtime JSONL；只校验记录 ID 会让消息 ID 错误在状态文件写入后才失败，形成部分提交风险。

### 已查明的执行流程

1. 用户、助手、工具结果、AgentMessage 和压缩摘要进入 `SessionConversationState.messages`，各自携带宿主持有的本地消息 ID。
2. Provider 请求桥接把 session message 无损转换为 core message，随后 Provider 可见投影基于 payload 调整文本和工具拓扑，但保留消息 ID。
3. `VulcanCoreController` 在当前快照上追加新的用户、助手和工具结果消息，并在流式 checkpoint 与终态报告中返回完整 core snapshot。
4. 反向桥接把 core snapshot 转回 session state，主会话通过 `FinalConversationState` 消费，TaskSession 则继续用于准备、压缩、结果归档和分段持久化。
5. runtime log 投影把消息 ID复制到 User/Assistant/ToolResult 事件，并以全窗口集合拒绝重复；replay 使用同一规则恢复活动窗口。
6. compaction 以消息 ID定位 source boundary，将摘要 ID作为新消息和后续上下文锚点；重复 ID 会使边界查找命中错误项。
7. 因此完整不变量是：活动消息窗口中每个 ID 必须非空、无首尾空白且全局唯一，并且在进入或离开 core 所有权前必须验证。

### 长期优化调整

- 在协议层新增公开的 `SessionConversationMessageIdentityError`，精确区分空 ID、首尾空白和重复 ID，并携带消息下标及冲突值。
- 新增 `validate_session_conversation_message_identities`，对完整活动窗口执行一次确定性校验且不改写身份。
- 将 runtime 原有的单条消息注册规则提升到协议 identity 模块；runtime projection 与 replay 继续复用同一底层规则，删除重复实现。
- 将 `build_core_conversation_snapshot` 与 `core_snapshot_to_session_conversation_state` 改为显式 `Result`，双向桥接均在交出状态前验证完整消息命名空间。
- 主会话 Provider 请求预览、runtime bootstrap、流式 checkpoint、成功终态和失败终态全部传播身份错误，不再发布无效 `FinalConversationState`。
- TaskSession 请求准备、存储恢复、预执行压缩、执行结果转换和最终持久化全部传播桥接错误，不再把脏快照包装成正常任务状态。
- workspace `state_document_from_runtime_state` 在序列化任何 checkpoint 元数据前同时校验消息与记录身份，阻断先写状态、后因 runtime 投影失败的部分提交。
- 新增协议层消息身份测试和 core 双向桥接回归测试，固定重复、空白及首尾空白 ID 均被拒绝。

### 文件变更

- 修改：`crates/vulcan-context/src/protocol/identity.rs`
- 修改：`crates/vulcan-context/src/protocol.rs`
- 修改：`crates/vulcan-context/src/protocol/tests.rs`
- 修改：`crates/vulcan-context/src/runtime/identity.rs`
- 修改：`crates/vulcan-agent-runtime/src/agent_runtime/executor.rs`
- 修改：`crates/vulcan-agent-runtime/src/agent_runtime/provider_request_bridge_conversation.rs`
- 修改：`crates/vulcan-agent-runtime/src/agent_runtime/provider_request_snapshot_builder.rs`
- 修改：`crates/vulcan-agent-runtime/src/agent_runtime/provider_transport_stream_runtime.rs`
- 修改：`crates/vulcan-agent-runtime/src/agent_runtime/runtime_stream_bootstrap_core.rs`
- 修改：`crates/vulcan-agent-runtime/src/agent_runtime/task_runtime/persistence_methods.rs`
- 修改：`crates/vulcan-agent-runtime/src/agent_runtime/task_runtime/request_records.rs`
- 修改：`crates/vulcan-agent-runtime/src/agent_runtime/task_runtime/runtime_execution.rs`
- 修改：`crates/vulcan-agent-runtime/src/agent_runtime/task_runtime/runtime_launch.rs`
- 修改：`crates/vulcan-agent-runtime/src/agent_runtime/task_runtime/tests/persistence_store.rs`
- 修改：`crates/vulcan-agent-runtime/src/agent_runtime/task_runtime/tests/preparation_config.rs`
- 修改：`crates/vulcan-workspace-store/src/workspace_store/segmented_session_store/state_documents.rs`
- 修改：`TASK_LOG.md`
- 新增：无（协议 identity 文件由第 96 轮创建，本轮扩展）
- 删除：无

### 修改范围代码审核与自动修复

- 首版只调整出站 session-to-core 桥接；审核确认 TaskSession invocation snapshot 和 core 流式终态也属于外部所有权边界，已将反向桥接同步改为严格 `Result`。
- 反向桥接改造后，两个测试把 `Result<SessionConversationState, String>` 直接交给 `serde_json::to_value`，编译合法却写出了 `{Ok: ...}` 形状；真实回归测试暴露“missing field `messages`”，已在序列化前显式解包有效状态。
- 审核流式 checkpoint 发现闭包不能返回错误；已把转换结果直接发送为流项目，失败时输出 `Err` 而不是构造无效终态。
- 审核 workspace 写入顺序发现状态文档构建早于 runtime JSONL 投影；已把消息身份预检加入状态文档构建边界，确保任何磁盘写入前失败。
- 审核 runtime 私有规则与协议新规则发现算法重复；已把单条注册器移入协议 identity 模块并由 runtime 复用。
- 对新增桥接函数、错误传播变量及测试关键变量补齐中英文设计意图注释。
- CodeKit 最终结构扫描覆盖 6 个核心文件、44 个结构项，解析错误为 0。

### 验证结果

- `cargo test -p vulcan-context -p vulcan-agent-runtime -p vulcan-workspace-store`：791 项全部通过。
- `cargo test -p vulcan-context -p vulcan-context-compaction -p vulcan-agent-runtime -p vulcan-workspace-store -p vulcan-service-kernel -p vulcan-task-session`：1235 项全部通过。
- `cargo clippy -p vulcan-context -p vulcan-context-compaction -p vulcan-agent-runtime -p vulcan-workspace-store -p vulcan-service-kernel -p vulcan-task-session --all-targets -- -D warnings`：通过，零告警。
- `cargo fmt --all -- --check`：通过。
- `git diff --check`：通过。

### 遗留问题与下一轮方向

- `append_user_text_message`、`append_assistant_text_message` 和 compaction rewrite 仍是无错误返回的直接 mutation；当前所有 Provider、runtime 与持久化边界都会拒绝脏状态，但生产点尚未做到原子拒绝，下一轮可继续收口 mutation API。
- `VulcanCoreController::derive_initial_local_id_counter` 以消息数加记录数并执行 `usize as u64` 转换，身份生成仍依赖当前时间和派生计数器；需要先证明 snapshot replacement 与时钟回拨场景后再调整。
- workspace-store 继续把协议类型化身份错误格式化为英文 `String`；跨 crate 统一类型化错误仍需独立设计。
- `RuntimeRepairEvent.downgraded_tool_segments` 仍恒为 0，结构化 replay diagnostics 设计保持待办。
- 本轮没有提交、暂存或覆盖用户其他未提交变更。

## 第 98 轮：收口规范消息原子 Mutation 与终态事务（2026-07-11）

### 问题与事实依据

- 第 97 轮已在 Provider、runtime 与持久化边界拒绝脏消息身份，但 `append_user_text_message`、`append_assistant_text_message` 和两种 compaction rewrite 仍直接修改 `state.messages` 且无错误返回。
- 这意味着重复或非规范 ID 会先污染内存状态，再由更晚边界报错；调用方无法区分“尚未修改”与“修改后等待兜底”。
- WebUI compaction draft 与 TaskSession compaction 本来就在独立副本上工作并返回 `Result`，具备原子失败所需的真实事务边界，不需要兼容性降级。
- 共享 prompt turn 会先把 prior transcript 水合进空运行态，再追加当前用户消息；如果水合窗口自身重复或与生成 ID 冲突，旧顺序会留下水合赋值和 unread 修改。
- terminal report 在同一函数内依次应用 checkpoint、用量、开放工具补全、可见记录、助手消息、摘要和统计；最后追加助手消息失败时，前面所有变更已成为部分终态。
- session-boundary 投影仍直接向 `messages` push 用户/助手消息，并对记录 ID 没有最终全局校验；重复事件序号会生成重复记录和消息身份。
- service-kernel 的 checkpoint/terminal 内存更新会先通过 `entry(...).or_insert_with(...)` 插入默认状态，再执行可能失败的应用函数；即使底层原子，外层仍会留下幽灵会话状态。

### 已查明的执行流程

1. prompt turn 根据当前 runtime state 或 prior transcript 选择规范窗口，再生成 `session-{id}-protocol-user-{now}` 并同步追加消息与可见记录。
2. 普通 session-event user prompt 使用相同 ID 规则，并修改 unread、last-safe request count 与 updated_at。
3. compaction 先选择完整 API round 尾部，再将 summary message 放在窗口首项，同时清空 provider continuation 游标并更新时间。
4. 自动压缩随后追加一条模型可见 continuation user message；该路径原先绕过共享追加函数直接 push。
5. terminal report 可优先应用 core checkpoint；若成功且没有匹配的助手消息，再生成 `session-{id}-protocol-assistant-{now}`，之后继续写摘要和 turn stats。
6. session-boundary 按 sequence 排序事件，以 `boundary-{thread}-{sequence}` 形记录 ID 投影记录；用户和助手记录 ID 同时成为规范消息 ID。
7. service-kernel 在内存锁内创建/取得 runtime entry，应用 checkpoint 或 terminal report，再更新显示消息、run phase 和会话状态。
8. 因此原子性必须从单消息追加一直覆盖到 prompt hydration、compaction、terminal 多步骤事务和 service 内存 entry 提交，任何一层提前赋值都会破坏失败不变式。

### 长期优化调整

- 新增 `append_session_conversation_message`，完整结构化消息统一在身份预检成功后一次 push；user/assistant 便捷函数改为调用该原子入口。
- 新增候选追加校验，先验证现有窗口，再检查候选 ID 的空值、首尾空白和占用冲突，不复制完整消息窗口。
- `append_user_text_message` 与 `append_assistant_text_message` 改为类型化 `Result`；全部生产与测试调用方显式传播或断言成功。
- session-event user prompt 追加改为 `Result`，并把 unread、safe request count、record 和 updated_at 修改延后到消息身份成功之后。
- 共享 prompt turn 将 prior transcript 保持为分离候选；仅在候选窗口能够接受当前 prompt ID 后才赋给 runtime state，普通当前窗口也在成功追加后才修改 unread。
- 两种 compaction rewrite 均先验证原窗口，离线构建 summary 加 retained-tail 候选，验证候选后一次性替换 messages 与 continuation。
- 自动压缩 continuation 改用共享原子 user-message API，错误通过 WebUI compaction draft 返回，不再直接 push。
- checkpoint 应用改为离线合并 messages/records、验证 checkpoint 原始身份和合并后身份，再一次提交 conversation state。
- terminal report 改为克隆完整 `SessionRuntimeStateEntry` 草稿，在草稿中完成 checkpoint、记录、消息、用量与统计，全部成功后才替换权威状态。
- terminal assistant 自定义 usage payload 改用完整结构消息原子追加入口。
- session-boundary 投影改为 `Result`，用户/助手走原子消息入口，并在返回前验证完整记录身份命名空间。
- service-kernel checkpoint、TaskSession terminal 和主会话 terminal 都改为克隆现有状态或构造分离默认草稿，成功后才插入 `session_runtime_state`。

### 文件变更

- 修改：`crates/vulcan-context/src/protocol/identity.rs`
- 修改：`crates/vulcan-context/src/protocol/projection.rs`
- 修改：`crates/vulcan-context/src/protocol.rs`
- 修改：`crates/vulcan-context/src/lib.rs`
- 修改：`crates/vulcan-context/src/protocol/tests.rs`
- 修改：`crates/vulcan-context-compaction/src/rewrite.rs`
- 修改：`crates/vulcan-agent-runtime/src/conversation_protocol.rs`
- 修改：`crates/vulcan-agent-runtime/src/session_event_runtime.rs`
- 修改：`crates/vulcan-agent-runtime/src/session_event_runtime/user_prompt.rs`
- 修改：`crates/vulcan-agent-runtime/src/session_event_runtime/checkpoint.rs`
- 修改：`crates/vulcan-agent-runtime/src/session_event_runtime/terminal_report.rs`
- 修改：`crates/vulcan-agent-runtime/src/session_event_runtime/tests.rs`
- 修改：`crates/vulcan-agent-runtime/src/workbench_prompt_turn.rs`
- 修改：`crates/vulcan-agent-runtime/src/workbench_prompt_turn/transcript.rs`
- 修改：`crates/vulcan-agent-runtime/src/workbench_prompt_turn/tests.rs`
- 修改：`crates/vulcan-agent-runtime/src/agent_runtime/prompt_compaction/tests.rs`
- 修改：`crates/vulcan-service-kernel/src/context_compaction.rs`
- 修改：`crates/vulcan-service-kernel/src/context_compaction/draft.rs`
- 修改：`crates/vulcan-service-kernel/src/execution_report_memory.rs`
- 修改：`crates/vulcan-service-kernel/src/tests/detached_subagent_tests.rs`
- 修改：`crates/vulcan-task-session/src/compaction.rs`
- 修改：`crates/vulcan-session-boundary/src/vulcan_projection.rs`
- 修改：`crates/vulcan-workspace-store/src/workspace_store/session_load/windows.rs`
- 修改：`crates/vulcan-workbench-model/src/subagent/tests/panel_state.rs`
- 修改：`TASK_LOG.md`
- 新增：无
- 删除：无

### 修改范围代码审核与自动修复

- 首版仅让底层 append 返回错误；审核发现共享 prompt turn 已在 append 前水合 prior transcript，已增加分离候选预检并延后真实赋值。
- 审核 compaction draft 发现自动 continuation 仍直接 push，已迁移到原子 user-message API 并传播失败。
- 全仓结构搜索发现 terminal report 与 session-boundary 是最后两个生产级 `SessionConversationMessage` 直接 push 入口；均已迁移，当前只剩 runtime replay 在完成逐事件身份注册后直接构造消息。
- terminal report 原先在消息失败前已写入大量记录与用量；已用完整 runtime draft 包裹整个事务，而不是仅回滚最后一条消息。
- checkpoint 初版只验证传入状态；审核发现 preserved messages/records 合并也可能形成最终冲突，已增加合并候选的二次验证后再赋值。
- service 层初版仍通过 map entry 提前插入默认状态；已在 checkpoint、TaskSession terminal 与主会话 terminal 三条路径改为分离草稿后插入。
- Clippy 发现 detached subagent 测试仍忽略 checkpoint 新 `Result`；已改为显式断言有效身份。
- 新增失败不变式测试覆盖：底层重复追加、compaction summary 冲突、prompt hydration 重复、terminal assistant 冲突和 boundary event 重复。
- CodeKit 最终结构扫描覆盖 8 个核心文件、150 个结构项，解析错误为 0。

### 验证结果

- `cargo test -p vulcan-context -p vulcan-context-compaction -p vulcan-agent-runtime -p vulcan-session-boundary -p vulcan-workspace-store -p vulcan-service-kernel -p vulcan-task-session -p vulcan-workbench-model`：1326 项全部通过。
- `cargo clippy -p vulcan-context -p vulcan-context-compaction -p vulcan-agent-runtime -p vulcan-session-boundary -p vulcan-workspace-store -p vulcan-service-kernel -p vulcan-task-session -p vulcan-workbench-model --all-targets -- -D warnings`：通过，零告警。
- `cargo fmt --all -- --check`：通过。
- `git diff --check`：通过。

### 遗留问题与下一轮方向

- `append_record` 仍是无错误返回的直接 push；虽然 checkpoint、boundary、workspace 写入和第 96 轮边界校验会拒绝重复记录身份，但记录生产点尚未获得与消息相同的原子契约。
- `VulcanCoreController::derive_initial_local_id_counter` 仍以消息数加记录数并执行未经检查的 `usize as u64` 转换；下一轮可完整验证 snapshot replacement 与标识生成流程。
- service terminal 草稿解决了身份失败的内存原子性，但 completion hook 和 VMM 副作用仍与 workspace clone 处于同一锁块；更广义的跨运行时副作用事务需要独立审计。
- workspace-store 继续把协议类型化身份错误格式化为英文 `String`；跨 crate 统一类型化错误仍需独立设计。
- 本轮没有提交、暂存或覆盖用户其他未提交变更。

## 第 99 轮：收口规范记录原子追加与转录复放错误传播（2026-07-11）

### 问题与事实依据

- 第 98 轮已完成规范消息的原子追加，但 `append_record` 仍无条件 `push`，重复、空白或非规范记录 ID 会先污染权威状态，再由更晚的持久化边界发现。
- 记录 ID 同时承担流式正文定位、工具详情引用、工具调用配对、回合统计闭合、边界投影与持久化复放身份；它不是普通展示字段。
- `conversation_state_from_session_messages` 原先逐行直接构造记录并返回状态，懒加载或持久化转录中的重复 ID 无法在复放入口显式失败。
- 多条 service 路径虽开始传播底层错误，但仍先通过 `entry(...).or_insert_with(...)` 修改权威 map；追加失败会留下默认运行态、已清理队列或第一条成功记录。

### 已查明的执行流程

1. session-event 流式输出先创建记录 ID，后续 delta 通过该 ID 定位并扩展正文；工具事件还会创建详情记录并把引用写入可见工具卡片。
2. terminal report、任务统计、上下文压缩状态与子智能体反馈会连续追加多条相互关联记录，后写失败时必须撤销整个运行态草稿。
3. WebUI service 把 runtime 记录合并到 `session_messages`，再投影到 workbench；因此 runtime 提交必须先于显示合并且只能发生一次。
4. workspace fork、分段会话复放、compaction 输入和 prompt 水合都会从 `SessionMessageEntry` 重建规范状态；复放入口是历史数据进入权威内存的共同身份边界。
5. stop、compaction failure、subagent feedback 和 streaming projection 原先会先取得可变 map entry，再调用可能失败的记录追加；外层事务必须使用分离草稿。

### 长期优化调整

- 新增候选记录追加校验：先验证已有记录命名空间，再验证候选 ID 的空值、首尾空白和重复占用，成功后才执行唯一一次 `push`。
- `append_record` 改为类型化 `Result`；生产调用全部显式传播身份错误，测试夹具显式断言其身份有效。
- `conversation_state_from_session_messages` 改为可失败复放，逐条通过原子记录入口，并在返回前验证规范消息身份；所有生产复放路径继续向上返回错误。
- session-event 的 streaming、tool detail、tool lifecycle、open-tool completion、terminal derived rows、task stats 和 boundary projection 全部迁移到可失败记录入口。
- runtime event projection 和 terminal report 使用完整运行态草稿；工具详情、映射、统计或记录任一步失败时不提交权威状态。
- WebUI stop、streaming projection、runtime event projection、context-compaction failure 和 subagent feedback 改为从现有运行态克隆分离草稿，全部追加成功后才写回 map。
- streaming 的 `record_id_slot` 也改为局部草稿，避免记录追加失败后调用方保留一个并不存在的记录 ID。
- workspace fork、分段会话读取、prompt compaction、session hydration 与 conversation display 的复放调用全部改为显式错误传播。

### 文件变更

- 修改：`crates/vulcan-context/src/protocol/identity.rs`
- 修改：`crates/vulcan-context/src/protocol/projection.rs`
- 修改：`crates/vulcan-context/src/protocol/tests.rs`
- 修改：`crates/vulcan-agent-runtime/src/session_event_runtime/` 下记录、工具、终态与用户提示投影模块
- 修改：`crates/vulcan-agent-runtime/src/agent_runtime/prompt_compaction/`、`task_runtime/` 与共享 prompt turn 模块
- 修改：`crates/vulcan-session-boundary/src/vulcan_projection.rs`
- 修改：`crates/vulcan-service-kernel/src/agent_stream/projection.rs`
- 修改：`crates/vulcan-service-kernel/src/context_compaction/`、`runtime_workspace_helpers.rs`、`subagent_event_ingest.rs` 与水合/显示调用链
- 修改：`crates/vulcan-workspace-store/src/workspace_store/` 下 fork、分段复放及相关测试
- 修改：受新可失败契约影响的 context、service、workspace、workbench 测试夹具
- 修改：`TASK_LOG.md`
- 新增：无
- 删除：无

### 修改范围代码审核与自动修复

- 首轮迁移只处理底层 `Result`；审核发现 service 仍提前创建 map entry，已将 stop、streaming、runtime event、compaction failure 和 subagent feedback 收口为外层分离事务。
- streaming 审核发现权威记录失败时 `record_id_slot` 仍可能先被赋值，已同步改为局部槽位草稿并与运行态一起提交。
- runtime event 审核发现工具记录虽在 agent-runtime 内原子，但 service 层 todolist、plan mode 和显示合并仍共享可变 entry，已统一在独立运行态副本上完成。
- 机械迁移测试夹具时发现一处正则范围误把 `format!` 结果附加了 `.expect`；编译审查立即定位并自动修复，随后重新运行完整矩阵。
- Clippy 发现 5 处测试仍忽略新 `Result`，已全部改为具名成功断言。
- 新增失败不变式测试覆盖：重复记录追加不修改状态、重复转录记录复放直接失败。
- 全仓生产级结构搜索确认剩余 `records.push` 属于共享原子入口、离线重建集合、Core 独立领域记录或测试代码，不再存在绕过入口的 `SessionConversationState` 权威追加。
- CodeKit 最终扫描 12 个核心文件、225 个结构项，解析错误为 0。

### 验证结果

- `cargo test -p vulcan-context -p vulcan-context-compaction -p vulcan-agent-runtime -p vulcan-session-boundary -p vulcan-workspace-store -p vulcan-service-kernel -p vulcan-task-session -p vulcan-workbench-model`：全部通过。
- 新增两项身份失败回归测试分别独立运行并通过。
- `cargo clippy -p vulcan-context -p vulcan-context-compaction -p vulcan-agent-runtime -p vulcan-session-boundary -p vulcan-workspace-store -p vulcan-service-kernel -p vulcan-task-session -p vulcan-workbench-model --all-targets -- -D warnings`：通过，零告警。
- `cargo fmt --all -- --check`：通过。
- `git diff --check`：通过。

### 遗留问题与下一轮方向

- `VulcanCoreController` 的 `ConversationRecord` 是独立 Core 领域模型，仍通过多个方法直接追加；下一轮应完整追踪本地 ID 计数器、snapshot replacement、时钟回拨与 checkpoint 恢复后再决定统一原子身份分配方案。
- `VulcanCoreController::derive_initial_local_id_counter` 仍以消息数加记录数并执行未经检查的 `usize as u64` 转换，可能与真实最大本地标识脱节。
- service 的 runtime event 主函数还包含 goal summary、mailbox consumption、child projection 与导航状态等非记录副作用；本轮保证记录失败不提交运行态，但更广义跨子系统事务仍需独立审计。
- workspace-store 继续把协议类型化身份错误格式化为英文 `String`；跨 crate 统一类型化错误仍需独立设计。
- 本轮没有提交、暂存或覆盖用户其他未提交变更。

## 第 100 轮：重建 Core 本地身份分配与快照接管边界（2026-07-11）

### 问题与事实依据

- `VulcanCoreController` 原先用 `messages.len() + records.len()` 播种 `u64` 计数器，生成 ID 时再拼接当前毫秒；该数值既不是现有 ID 的最大序号，也无法描述稀疏快照或外部提供的身份。
- `replace_snapshot` 会重新按集合长度重置计数器；时钟回拨、同毫秒重建控制器或替换为等长快照时，旧方案无法证明下一条 ID 未被占用。
- `usize as u64` 是未经检查的截断转换；更关键的是，即使转换不溢出，集合大小也与真实身份空间没有因果关系。
- Core 构造与快照替换此前不验证消息、记录的空值、首尾空白和重复身份，公共控制器可以直接接管损坏状态。
- `append_display_record` 接受调用方提供的稳定记录 ID，却无条件 `push`；子智能体 delivery ID 重复时会形成两个记录所有者。
- 审核 Core 桥接发现双向转换只验证消息身份，没有验证记录身份；这会绕过第 99 轮已经建立的记录不变式。

### 已查明的执行流程

1. agent-runtime 从 `SessionConversationState` 构建 Core snapshot，追加 Provider 可见 agent message 后创建 `VulcanCoreController`。
2. Core 的用户、助手、工具结果和智能体消息分别生成 message ID 与 record ID；旧生成器共享一个按集合长度播种的计数器，并依赖系统时间。
3. Provider 流每次重试都会从同一初始 snapshot 新建控制器；旧实现的计数器和时间组合在重试、快照恢复及系统时钟变化之间没有稳定所有权依据。
4. Core checkpoint 再通过桥接转回 session state；如果桥接只校验消息，重复记录可进入运行态或持久化链路。
5. 子智能体运行中干预先取出 inbox，再追加 Provider 可见 agent message 和带固定 delivery ID 的 display record；显示记录冲突时必须同时撤销 Provider 消息并恢复 inbox。
6. agent-control 已有 `restore_inter_agent_messages_front_after_failure`，因此失败恢复应复用真实队列事务，而不是打印错误后丢弃消息。

### 长期优化调整

- 删除 `local_id_counter` 字段与 `derive_initial_local_id_counter`，彻底移除集合长度、时间戳和未经检查数值转换对身份正确性的影响。
- Core 自生成 ID 改为带语义前缀的 UUID v4，并在返回前同时检查现有消息与记录集合；快照替换和时钟回拨不再影响生成空间。
- 新增 Core snapshot 身份校验，消息与持久化记录使用各自独立命名空间，拒绝空 ID、首尾空白和重复所有者。
- `VulcanCoreController::new` 改为可失败构造；所有生产调用显式传播到请求快照、bootstrap 或 Provider stream，后台任务不再因无效快照 panic。
- `replace_snapshot` 改为先验证候选、成功后一次替换的原子 `Result` API；失败保留原快照与最近成功用量状态。
- `append_display_record` 改为 `Result<Option<String>, CoreError>`，调用方 ID 在 append 前完成精确校验。
- Core/session 双向桥接同时验证消息和记录身份，任何方向都不再暴露部分转换状态。
- 子智能体运行中干预改为克隆完整 Core controller 草稿；agent message 与 display record 全部成功后才提交。
- 干预显示身份冲突时，使用既有 inbox 恢复事务把已取出的消息放回队首，避免错误修复导致消息丢失。

### 文件变更

- 修改：`crates/vulcan-core/Cargo.toml`
- 修改：`crates/vulcan-core/src/controller.rs`
- 修改：`crates/vulcan-core/src/controller/metadata.rs`
- 修改：`crates/vulcan-core/src/controller/completion.rs`
- 修改：`crates/vulcan-core/src/controller/lifecycle.rs`
- 修改：`crates/vulcan-core/src/controller/transcript.rs`
- 修改：`crates/vulcan-core/src/controller/tests.rs` 及 Core 控制器测试调用点
- 修改：`crates/vulcan-agent-runtime/src/agent_runtime/provider_request_bridge_conversation.rs`
- 修改：`crates/vulcan-agent-runtime/src/agent_runtime/provider_request_snapshot_builder.rs`
- 修改：`crates/vulcan-agent-runtime/src/agent_runtime/runtime_stream_bootstrap_core.rs`
- 修改：`crates/vulcan-agent-runtime/src/agent_runtime/provider_transport_stream_runtime.rs`
- 修改：`crates/vulcan-agent-runtime/src/agent_runtime/task_runtime/agent_control.rs`
- 修改：受 Core 可失败构造影响的 agent-runtime 与 service 测试夹具
- 修改：`TASK_LOG.md`
- 新增：无
- 删除：无

### 修改范围代码审核与自动修复

- 首版只替换身份生成器；审核确认公共构造和快照替换仍可接管脏状态，因此把身份验证提升到 Core 所有权边界。
- 构造 API 迁移初版在生产路径使用 `expect`；审核后改为请求构建和 bootstrap 显式 `?`，Provider 后台流则发送错误并停止重试任务，不再 panic。
- 桥接审查发现第 97 至 99 轮只在该文件校验消息身份；已补齐出站与入站记录身份验证及双向回归测试。
- display record 改为可失败后，审查发现运行中子智能体干预会先追加 Provider agent message；已改为完整控制器草稿，消除半提交。
- 进一步 CodeKit 审查发现 agent-control 已有 inbox 恢复函数；首版仅打印显示冲突错误会丢失已取出的消息，已自动修复为失败后恢复队首。
- 新增回归测试覆盖：构造拒绝重复消息、替换重复记录时状态不变、display ID 重复时状态不变、Core 桥接双向拒绝重复记录、子智能体显示冲突时控制器不变且 inbox 恢复。
- 最终 CodeKit 结构扫描覆盖 6 个核心文件、92 个结构项，解析错误为 0。

### 验证结果

- `cargo test -p vulcan-context -p vulcan-context-compaction -p vulcan-core -p vulcan-agent-runtime -p vulcan-session-boundary -p vulcan-workspace-store -p vulcan-service-kernel -p vulcan-task-session -p vulcan-workbench-model`：全部通过。
- 最终修改后 `cargo test -p vulcan-core -p vulcan-agent-runtime`：607 项全部通过。
- 新增 inbox 恢复失败不变式测试独立运行并通过。
- `cargo clippy -p vulcan-core -p vulcan-agent-runtime -p vulcan-service-kernel --all-targets -- -D warnings`：通过，零告警。
- `cargo fmt --all -- --check`：通过。
- `git diff --check`：通过。

### 遗留问题与下一轮方向

- `append_inter_agent_agent_messages_to_snapshot` 与 `append_inter_agent_display_records_to_snapshot` 仍在离线 snapshot 上分别直接追加；当前上层流程依靠调用顺序与后续 Core 构造验证，下一轮可审计离线 inbox resume 的事务边界。
- `RuntimePromptIntervention` 仍以 `bool` 表达“是否追加提示”，无法把身份或 inbox 恢复失败作为类型化错误传回 Provider loop；本轮通过草稿与恢复保证状态安全，但错误传播模型仍可继续收口。
- Core 与 session 分别维护内容相近的身份验证逻辑；若后续需要跨层类型化错误，应先明确领域所有权，不能简单让 Core 依赖 session 协议层。
- service 的 runtime event 主函数仍混合 goal summary、mailbox consumption、child projection 与导航副作用，保留为后续事务审计对象。
- 本轮没有提交、暂存或覆盖用户其他未提交变更。

## 第 101 轮：统一运行时干预错误传播与在线/离线消息事务（2026-07-11）

### 问题与事实依据

- `RuntimePromptIntervention` 原先只能返回 `bool`，只能表达“是否修改控制器”，不能把队列锁失败、消息转换失败、身份冲突或恢复失败传回 Provider loop。
- Core 在工具边界直接把权威控制器传给回调；回调先修改状态再返回 `false` 或失败时，Core 没有事务边界，也无法撤销局部修改。
- 子智能体在线干预会先取出 inbox，再生成 Provider agent message 与 display record；第 100 轮虽增加草稿和恢复，但错误仍被打印后转换为 `false`，Provider 会继续运行。
- 离线 idle-agent resume 分别调用 `append_inter_agent_agent_messages_to_snapshot` 与 `append_inter_agent_display_records_to_snapshot`，两个函数会按 ID 静默跳过既有项，无法区分幂等重放与不同内容抢占同一身份。
- WebUI 干预先移除 guided pending prompts，再逐条修改 Core；随后 mailbox 路径先 drain 当前回合输入，再逐条转换。任意中途失败都会造成队列丢失、Core 半提交或二者状态不一致。
- `TurnInputQueue` 已提供只读 `get_pending_input`，`SessionRuntimeKernel` 与 workspace snapshot 均可克隆，因此可以在同一 service 锁内建立真实分离事务，不需要新增模糊回退。

### 已查明的执行流程

1. Core 完成 Provider 工具调用并追加全部工具结果后，在安全 checkpoint 边界调用 `on_before_next_provider_turn`。
2. agent-runtime 把 `RuntimePromptIntervention` 包装成 Core 回调；旧包装使用 `unwrap_or(false)`，丢失所有宿主错误。
3. 子智能体在线回调从 `agent_inboxes` 取走消息，向控制器追加类型化 agent message 和带 delivery ID 的显示记录。
4. idle-agent 离线恢复从相同 inbox 取消息，准备 invocation snapshot，再分两步追加 message、隐藏记录和显示记录，最后调用 backend；backend 失败已有 inbox 恢复机制。
5. WebUI guided queue 存于 workspace runtime UI state，mailbox 当前回合输入存于 live `SessionRuntimeKernel`；旧逻辑跨两个独立可变入口顺序消费。
6. mailbox 输入的类型化转换依赖结构化 `subagent_mailbox_event`；缺字段时会返回错误，旧路径此时已 drain 队列。
7. 因此正确事务必须覆盖 Core controller、workspace UI queue、live kernel input queue 和 agent inbox，而不仅是单个 append 函数。

### 长期优化调整

- `RuntimePromptIntervention` 改为 `Result<bool, String>`，宿主错误不再被压扁为“没有变化”。
- Core 两种 streaming intervention API 的回调契约改为 `Result<bool, CoreError>`，agent-runtime 在边界把宿主错误转换为 `CoreError::InvalidState`。
- Core 在每次干预前克隆完整控制器草稿：仅 `Ok(true)` 提交草稿并发 checkpoint；`Ok(false)` 丢弃草稿；`Err` 丢弃草稿、标记控制器失败并终止 Provider loop。
- 子智能体在线干预直接传播 inbox 读取和 display 身份错误；显示失败时先恢复已取出的 inbox，再把恢复后的准确错误返回 Provider loop。
- 合并离线 snapshot 的两个追加函数为 `append_inter_agent_messages_to_snapshot`，在一个候选快照内同时处理 Provider message、隐藏 agent record 与显示 record。
- 离线幂等判断从“ID 存在就跳过”改为“ID 和完整序列化内容都一致才复用”；同 ID 不同内容显式报冲突。
- 离线候选在提交前通过 Core/session 双向桥接验证完整消息与记录身份，成功后一次替换；失败由既有 idle resume 机制恢复 inbox。
- WebUI guided prompt 与 mailbox 输入改为单个事务：在 service 锁内克隆 workspace、kernel 和 Core controller，先完成全部规范化与类型化转换，再统一提交三个权威对象。
- mailbox 草稿先通过 `get_pending_input` 克隆目标输入并完成整批转换，随后才在 kernel 草稿中 drain；转换失败不再触碰实时队列。
- 保留 pending prompt delta 为提交后的通知副作用；通知发送失败只记录诊断，不回滚已经完成的权威事务。
- 删除已经失去调用方的旧 guided prompt take 适配器，测试专用 mailbox 入口也复用同一草稿函数。

### 文件变更

- 修改：`crates/vulcan-core/src/controller/run_loop.rs`
- 修改：`crates/vulcan-core/src/controller/tests/interventions.rs`
- 修改：`crates/vulcan-core/src/controller/tests/loop_flow.rs`
- 修改：`crates/vulcan-agent-runtime/src/agent_runtime/runtime_stream_intervention.rs`
- 修改：`crates/vulcan-agent-runtime/src/agent_runtime/provider_transport_stream_runtime.rs`
- 修改：`crates/vulcan-agent-runtime/src/agent_runtime/task_runtime/agent_control.rs`
- 修改：`crates/vulcan-agent-runtime/src/agent_runtime/task_runtime/tests/runtime_flow.rs`
- 修改：`crates/vulcan-service-kernel/src/agent_stream/mailbox.rs`
- 修改：`crates/vulcan-service-kernel/src/pending_prompts.rs`
- 修改：`crates/vulcan-service-kernel/src/lib.rs`
- 修改：`crates/vulcan-service-kernel/src/tests/mailbox_tests.rs`
- 修改：受严格复放身份约束影响的 service/workspace 测试夹具
- 修改：`TASK_LOG.md`
- 新增：无
- 删除：无

### 修改范围代码审核与自动修复

- 首版在线子智能体失败只恢复 inbox 并返回 `false`；审核确认 Provider 会误以为无变化继续运行，已改为类型化错误贯穿 RuntimePromptIntervention、Core 和 Provider stream。
- Core 初版直接对权威控制器执行可失败回调；审核后增加完整控制器草稿，使错误和错误的 `Ok(false)` 都不能泄漏修改。
- 离线两个 append 函数原本各自静默去重；已合并为完整候选事务，并验证同 ID 的既有内容确实属于同一次 delivery。
- WebUI 初版计划分别给 guided queue 和 mailbox 增加恢复逻辑；源码确认 workspace 与 kernel 均可克隆后，改为同锁内三对象草稿提交，避免跨队列补偿事务。
- mailbox 草稿增加 drain 数量一致性检查；如果只读批次和实际取出数量不一致，返回显式错误且权威 kernel 不提交。
- CodeKit 审核发现 agent-control 已有 inbox 失败恢复函数，离线候选冲突直接接入该路径，没有另造恢复语义。
- 完整测试首次暴露第 99 轮严格复放后仍有 4 处测试夹具使用空记录 ID；已补充真实 runtime metadata，而没有放宽生产校验。
- 完整矩阵首次运行还触发一个既有并发时序测试抖动；该测试独立复跑通过，后续完整矩阵通过，未用无依据修改掩盖偶发调度差异。
- 新增失败不变式测试覆盖：Core 回调错误回滚、在线子智能体冲突恢复 inbox、离线 snapshot 冲突不变、WebUI 无效 mailbox 载荷保留 Core 与实时队列。
- 最终 CodeKit 扫描 5 个核心文件、68 个结构项，解析错误为 0。

### 验证结果

- `cargo test -p vulcan-context -p vulcan-context-compaction -p vulcan-core -p vulcan-agent-runtime -p vulcan-session-boundary -p vulcan-workspace-store -p vulcan-service-kernel -p vulcan-task-session -p vulcan-workbench-model`：最终全部通过。
- `cargo test -p vulcan-service-kernel`：360 项全部通过。
- Core 回调回滚、离线 snapshot 原子冲突和 WebUI mailbox 原子失败测试分别独立运行并通过。
- `cargo clippy -p vulcan-context -p vulcan-context-compaction -p vulcan-core -p vulcan-agent-runtime -p vulcan-session-boundary -p vulcan-workspace-store -p vulcan-service-kernel -p vulcan-task-session -p vulcan-workbench-model --all-targets -- -D warnings`：通过，零告警。
- `cargo fmt --all -- --check`：通过。
- `git diff --check`：通过。

### 遗留问题与下一轮方向

- WebUI prompt intervention 当前通过克隆整个 `WorkspaceSnapshot` 获得事务语义；正确但复制范围较大，下一轮可审计 workspace 中是否已有可复用的局部事务封装，避免长期性能浪费。
- RuntimePromptIntervention 的外层错误仍是 `String`，进入 Core 后统一映射为 `InvalidState`；若需要区分队列锁、身份冲突和输入协议错误，应设计 agent-runtime 自有错误枚举，而不是继续解析字符串。
- `append_inter_agent_messages_to_snapshot` 用 JSON 值比较完整实体内容以验证幂等归属；长期可为 Core message/record 增加领域级 `PartialEq` 或显式 ownership fingerprint，减少序列化成本。
- service 的 runtime event 主函数仍混合 goal summary、mailbox consumption、child projection 与导航副作用，继续保留为下一轮事务审计对象。
- 本轮没有提交、暂存或覆盖用户其他未提交变更。

## 第 102 轮：缩小 WebUI 干预事务复制域并修复 Runtime Event 提交顺序（2026-07-11）

### 问题与事实依据

- 第 101 轮为 WebUI prompt intervention 建立事务时克隆了完整 `WorkspaceSnapshot`，但该事务实际只修改一个 session 的 `SessionRuntimeStateEntry`。
- `WorkspaceSnapshot` 同时持有全部活动/归档项目、活动/归档会话、所有会话转录、压缩历史、运行态和水合标记；全量克隆成本随整个工作区增长，而不是随本次干预输入增长。
- runtime event 主投影在解析 `send_message`、`wait_agent` 和 `close_agent` 的可失败结构前后交错修改多个权威对象：goal summary、mailbox kernel、workspace runtime、child transcript、导航缓存和 workbench。
- `close_agent` 结构解析原先发生在 workspace 与 runtime state 已提交之后；解析失败会返回 `Err`，但前面的状态已经无法撤销。
- runtime event 提交后还会调用导航/附加面板通知和 `controller_runtime_from_state`；这些操作失败时函数返回 `Err`，造成调用方看到“事务失败”，实际内存状态已经提交。
- 已有测试明确允许“controller runtime 未就绪但内存先更新”，这正是旧半提交语义的证据，而不是应保留的兼容行为。

### 已查明的执行流程

1. WebUI prompt intervention 在同一 service 锁内读取 pending dispatch context，修改单个 runtime UI queue，并可消费单个 live kernel 的 mailbox 输入。
2. 该路径不修改项目树、其他会话、归档数据、全局转录或压缩历史，因此完整 workspace clone 没有事务依据。
3. runtime event 仅处理 `ToolStarted`/`ToolFinished`；先把 tool 记录投影到 runtime state，再可能追加 child `send_message` 行、消费 mailbox、更新 goal/todolist/plan 和关闭子任务导航。
4. `terminal_subagent_send_message_resume_from_event`、`terminal_subagent_wait_consumed_executions_from_event` 和 `subagent_close_agent_projection_from_tool_metadata` 都可能返回协议错误。
5. runtime event 产生转录、todolist 或 plan 变化后需要 `ControllerRuntimeHandle` 启动异步持久化；旧代码在内存提交后才获取该句柄。
6. workbench delta 发出属于提交后的通知副作用，不应通过返回 `Err` 把已经成功的内存事务标记成失败。

### 长期优化调整

- WebUI prompt intervention 不再克隆完整 workspace，只克隆目标 session 的 `SessionRuntimeStateEntry`、目标 `SessionRuntimeKernel` 和 Core controller。
- pending dispatch context 从不可变 workspace 读取；guided queue 仅在 session runtime 草稿内移除，全部 mailbox 转换成功后才写回该 map entry。
- runtime event 在取得 service 主锁前预解析全部可失败的 subagent send/wait/close 结构，并预取 `ControllerRuntimeHandle`。
- runtime event 的 workspace 修改改在完整 workspace 草稿上完成，包括 runtime record、session messages、child send-message 投影和水合标记；任何一步失败都直接丢弃草稿。
- workspace 草稿成功后一次写回，再执行已完成预检的 goal summary、mailbox consumption、completion gate 和导航清理；锁内提交边界后不再存在可失败操作。
- transcript、todolist 和 plan 持久化复用预检得到的 controller runtime，不再在提交后重新读取可失败依赖。
- 导航 delta 和 addon delta 改为提交后通知：失败记录明确诊断，但不返回事务错误，不诱导调用方重试已提交事件。
- 将原先允许半提交的 send-message cache 测试改为严格失败不变式：缺少 controller runtime 时预检失败，child cache 与 workspace messages 必须保持不变。

### 文件变更

- 修改：`crates/vulcan-service-kernel/src/agent_stream/mailbox.rs`
- 修改：`crates/vulcan-service-kernel/src/agent_stream/projection.rs`
- 修改：`crates/vulcan-service-kernel/src/tests/detached_subagent_tests.rs`
- 修改：`TASK_LOG.md`
- 新增：无
- 删除：无

### 修改范围代码审核与自动修复

- 首轮只计划优化 workspace clone；结构审计发现 runtime event 存在更严重的“先提交、后解析/后取依赖”问题，因此在同轮收口其提交顺序。
- runtime event 初版仍在锁内直接修改 workspace；已改为分离 workspace 草稿，确保 tool record 与 child send-message 解析属于同一提交。
- 审核提交后路径发现导航/addon delta 仍使用 `?`；已改为诊断通知失败，不再把成功事务伪报为失败。
- 预取 controller runtime 后，既有 cache 集成测试不再获得旧半提交结果；已重写为预检失败状态不变测试，而 send-message workspace 追加本身继续由 mailbox 专项测试覆盖。
- Clippy 发现两处嵌套通知错误处理可折叠，已按 let-chain 自动修复。
- CodeKit 最终扫描 4 个相关文件、93 个结构项，解析错误为 0。

### 验证结果

- `cargo test -p vulcan-context -p vulcan-context-compaction -p vulcan-core -p vulcan-agent-runtime -p vulcan-session-boundary -p vulcan-workspace-store -p vulcan-service-kernel -p vulcan-task-session -p vulcan-workbench-model`：全部通过。
- `cargo test -p vulcan-service-kernel`：360 项全部通过。
- prompt intervention 两项事务测试与 runtime event 预检失败状态不变测试分别运行并通过。
- `cargo clippy -p vulcan-context -p vulcan-context-compaction -p vulcan-core -p vulcan-agent-runtime -p vulcan-session-boundary -p vulcan-workspace-store -p vulcan-service-kernel -p vulcan-task-session -p vulcan-workbench-model --all-targets -- -D warnings`：通过，零告警。
- `cargo fmt --all -- --check`：通过。
- `git diff --check`：通过。

### 遗留问题与下一轮方向

- runtime event 为实现跨 workspace 多字段事务仍克隆完整 `WorkspaceSnapshot`；该路径确实修改 runtime、父/子 session messages 和水合标记，但仍可进一步设计 session-scoped workspace patch，避免复制不相关归档与项目数据。
- runtime event 的 goal summary、mailbox consumption、导航缓存和 workbench 仍在 workspace 提交后按顺序原地修改；当前这些步骤均无错误返回，但尚未形成显式的 service-level transaction draft 类型。
- RuntimePromptIntervention 外层仍使用 `String` 错误，Core 统一映射为 `InvalidState`；类型化干预错误仍是后续候选。
- `append_inter_agent_messages_to_snapshot` 的幂等归属仍通过 JSON 序列化比较，后续可改为领域级相等或 ownership fingerprint。
- 本轮没有提交、暂存或覆盖用户其他未提交变更。

## 第 103 轮：将 Runtime Event 全工作区草稿收缩为会话级补丁（2026-07-11）

### 问题、流程与事实依据

- 第 102 轮的 runtime event 事务会克隆完整 `WorkspaceSnapshot`；源码追踪确认其真实写集仅为父会话运行态、父会话转录、可选子会话转录及对应水合标记。
- 完整快照还包含全部活动/归档项目、会话、转录、压缩历史和选择状态，复制成本随工作区增长，与单事件写集不匹配。
- 流程为：预解析 send/wait/close 并取得 controller runtime，投影父会话工具记录，合并父转录，可选构造子会话 send-message，提交内存状态，最后持久化父转录及 todolist/plan addon。
- 持久化读集也已查清：转录只需一个会话的消息；addon 只需项目归属索引、一个 `SessionSummary`、归档标记和已派生 addon state。

### 长期优化调整

- 新增 `RuntimeEventWorkspaceDraft` 与 `RuntimeEventChildTranscriptDraft`，只克隆并提交上述会话级写集；构造失败不会污染权威 workspace。
- 将 send-message 拆出纯构造函数 `webui_subagent_send_message_from_event`，原直接写 workspace 的适配器仅保留给测试。
- 新增单会话转录持久化入口，后台任务直接接收限定后的消息集合，不再复制完整 workspace。
- 新增 `SessionAddonPersistenceScope`，统一携带项目归属、单会话摘要和归档状态，并从 `scope.session.id` 唯一取得身份。
- 新增失败/隔离不变式测试，验证父补丁提交时其他活动/归档消息、归档运行态、项目展开状态和选择会话完全不变。

### 文件变更清单

- 修改：`crates/vulcan-service-kernel/src/agent_stream/projection.rs`
- 修改：`crates/vulcan-service-kernel/src/subagent_event_ingest.rs`
- 修改：`crates/vulcan-service-kernel/src/workspace_session_persistence.rs`
- 修改：`TASK_LOG.md`
- 新增/删除：无

### 修改范围代码审核与自动修复

- 审核发现首版仅移除事务全量克隆，却可能把全快照复制成本转移至异步持久化；已继续收窄两条持久化载荷。
- addon 局部入口首版参数过多；已收口为 `SessionAddonPersistenceScope`，删除重复 `session_id` 参数。
- 结构调整后两个调用点仍保留旧参数列表；编译前核对发现并自动修正。
- Clippy 发现无效初始赋值、晦涩布尔链和仅测试使用的旧适配器；已分别改为确定赋值、显式分支和 `#[cfg(test)]`，未保留告警抑制。
- CodeKit 最终扫描 3 个核心文件、78 个结构项，解析错误为 0。

### 验证结果

- `cargo test -p vulcan-service-kernel`：2 个测试套件共 361 项全部通过。
- 九包测试矩阵：17 个测试套件共 1352 项全部通过。
- `runtime_event_workspace_draft_commits_only_owned_session_fields` 独立通过。
- 九包 `cargo clippy --all-targets -- -D warnings`、`cargo fmt --all -- --check`、`git diff --check` 全部通过。

### 遗留问题与下一轮方向

- goal summary、mailbox consumption、导航缓存和 workbench 仍按顺序直接提交；下一轮应继续核对读写集并评估统一 service-level draft。
- `RuntimePromptIntervention` 外层仍使用 `String` 错误；可继续建立领域错误枚举。
- `append_inter_agent_messages_to_snapshot` 的幂等归属仍依赖 JSON 序列化比较；可审计稳定 ownership fingerprint。
- 本轮没有提交、暂存或覆盖用户其他未提交变更。

## 第 104 轮：严格化 Runtime Event Goal 投影协议与会话所有权（2026-07-11）

### 问题与事实依据

- 第 103 轮遗留方向是评估 goal、mailbox、导航和 workbench 是否需要统一 service-level draft。源码追踪确认这些操作均位于同一 service 主锁内，workspace 会话补丁提交后不再调用可失败的锁内操作，外部无法观察中间状态；为它们复制完整 service state 没有事务收益。
- 审计 goal 分支时发现更实质的问题：`goal_summary_from_tool_finished_event` 对已声明为 `session_goal` 的畸形 payload 返回 `None`，与无关事件无法区分，协议错误会被静默吞掉。
- runtime event 旧逻辑信任 host projection 的 `sessionId`，并以该 ID 写入 `session_goal_summaries`；它没有验证载荷会话等于当前事件的 `session_id`，因此一个事件可以污染另一个会话的 goal cache。
- Goal 原生工具的唯一来源已经确认：create/update 都从当前 `NativeToolExecutionRequest.session_id` 取得会话，写入 `ThreadGoal` 后由 `goal_host_projection` 原样输出 `goal.thread_id`。合法协议不存在跨会话路由分支。

### 已查明的执行流程

1. Goal 原生工具从当前请求取得 controller runtime 与当前 session ID，并只读写该会话的 goal store。
2. 工具完成结果把持久化后的 `ThreadGoal.thread_id` 写入 `host_result.payload.goal.sessionId`。
3. service runtime event 解析 host result，随后构造 workspace 会话级草稿并提交工具记录与转录。
4. 旧代码在 workspace 提交后才尝试解析 summary session ID；解析失败会静默跳过，解析成功则可能写入任意载荷指定的会话。
5. 正确边界必须在取得 controller runtime 和修改任何 service state 前完成严格 payload 解析与当前会话所有权校验；成功后只能按 runtime event 的 session ID 更新缓存。

### 长期优化调整

- 将 `goal_summary_from_tool_finished_event` 改为 `Result<Option<Option<SessionGoalSummary>>, String>`：无关事件、更新、清理和协议错误成为四种明确状态。
- 对已声明为 `session_goal` 的结果强制要求 `payload.goal`；缺字段或无效 goal 不再伪装成无关事件。
- runtime event 在 controller runtime 预取与 workspace 草稿构造前解析 goal projection，并验证 summary session ID 与当前事件 session ID 完全一致。
- goal cache 提交不再使用载荷 ID 路由，只使用已经验证的当前 `session_id`；清理事件也明确清理当前会话。
- 保留 mailbox、completion gate、导航和 workbench 的现有锁内提交方式，因为其读写集和失败语义已证明不需要额外 service 全量草稿。

### 文件变更清单

- 修改：`crates/vulcan-service-kernel/src/session_goal_lifecycle/summary_parse.rs`
- 修改：`crates/vulcan-service-kernel/src/agent_stream/projection.rs`
- 修改：`crates/vulcan-service-kernel/src/tests/goal_todolist_tests.rs`
- 修改：`TASK_LOG.md`
- 新增/删除：无

### 修改范围代码审核与自动修复

- 首轮测试尝试序列化完整 `WorkspaceSnapshot` 证明失败不变式；编译确认该领域类型没有 `Serialize`，审核后没有为测试污染生产类型，而是精确比较该路径可能触及的消息、运行态、水合标记、选择会话和 goal map。
- 严格解析只对明确声明 `kind = session_goal` 的 host result 报协议错误；其他工具与非完成事件继续返回无关状态，没有扩大到模糊兼容判断。
- 所有权校验位于 controller runtime 获取和 workspace 草稿构造之前；跨会话错误不会因缺少 runtime 依赖而被掩盖，也不会产生任何内存提交。
- 新增畸形 goal payload 拒绝测试和跨会话所有权失败不变式测试。
- CodeKit 最终扫描 3 个相关文件、56 个结构项，解析错误为 0。

### 验证结果

- 两项 goal parser 定向测试通过；跨会话投影失败不变式测试独立通过。
- `cargo test -p vulcan-service-kernel`：2 个测试套件共 363 项全部通过。
- 九包测试矩阵：17 个测试套件共 1354 项全部通过。
- 九包 `cargo clippy --all-targets -- -D warnings`：通过，零告警。
- `cargo fmt --all -- --check` 与 `git diff --check`：通过。

### 遗留问题与下一轮方向

- `RuntimePromptIntervention` 外层仍以 `String` 承载队列锁、协议、身份冲突等不同错误；下一轮可追踪完整传播链并设计 agent-runtime 领域错误枚举。
- `append_inter_agent_messages_to_snapshot` 的幂等归属仍通过 JSON 序列化比较；可继续审计 Core message/record 是否具备稳定领域相等语义。
- `goal_summary_from_value` 仍以 `Option` 解析内部字段，外层只能报告统一的 invalid goal；如诊断需求明确，可进一步引入字段级解析错误而不是解析字符串。
- 本轮没有提交、暂存或覆盖用户其他未提交变更。

## 第 105 轮：类型化 Runtime Prompt Intervention 错误传播（2026-07-11）

### 问题与事实依据

- `RuntimePromptIntervention` 原先返回 `Result<bool, String>`，WebUI 与子智能体两类宿主产生的状态访问、状态未就绪、输入协议、队列不变式、投影冲突和恢复失败全部被压扁为字符串。
- agent-runtime 进入 Core 前把任意字符串统一包装为 `CoreError::InvalidState`；Core 能停止循环并回滚控制器草稿，但上层无法在不解析文本的情况下区分故障类别。
- WebUI 干预错误来自 service state 锁、workspace 可用性、mailbox 结构转换和 drain 数量一致性；子智能体干预错误来自 inbox 锁、显示记录身份冲突，以及冲突后恢复 inbox 的二次失败。
- Core 只拥有控制器草稿与 Provider 循环，不拥有 WebUI workspace 或 agent-control inbox，因此宿主错误枚举不应放进 Core；最合适的领域所有者是定义干预回调的 agent-runtime。

### 已查明的执行流程

1. WebUI 或子智能体构造 `RuntimePromptIntervention`，随 Provider transport 请求一路传入 host-controlled stream。
2. Core 在全部工具结果追加完成后的 replay-safe checkpoint 克隆控制器草稿并调用宿主干预。
3. WebUI 干预在同一 service 锁内建立 runtime state、kernel 和 controller 局部草稿；子智能体干预先取 inbox，再建立 controller 草稿。
4. 宿主返回错误时，agent-runtime 将其转换为 `CoreError::InvalidState`；Core 丢弃控制器草稿、标记失败并终止 Provider 循环。
5. 子智能体显示投影冲突时必须恢复已经取出的 inbox；恢复成功与恢复失败具有不同的数据安全含义，旧字符串无法稳定表达。

### 长期优化调整

- 新增公开 `RuntimePromptInterventionError`，明确区分 `StateAccess`、`StateUnavailable`、`InvalidInput`、`InvariantViolation`、`ProjectionConflict` 和 `RecoveryFailure`。
- 为枚举实现稳定 `Display` 与 `Error`；`RuntimePromptIntervention` 改为返回该领域错误，而不是裸字符串。
- WebUI 事务在错误产生点直接分类：锁失败、workspace 未就绪、mailbox 协议失败和 drain 不变式失败不再依赖文本推断。
- 子智能体 inbox 读取失败映射为状态访问错误；显示身份冲突映射为投影冲突；投影失败且 inbox 恢复也失败映射为包含双重原因的恢复失败。
- 将共享 inbox 恢复 helper 的返回值改为内部结构化 `InterAgentMessageRestoreError`，离线续跑路径继续渲染原有文本，runtime intervention 则无损映射为公开领域错误。
- agent-runtime 进入 Core 时仅在架构边界调用 `Display` 转换为 `CoreError::InvalidState`，保留 Core 的宿主无关性和既有失败事务语义。
- 从 agent-runtime 模块与 crate 根重新导出新错误类型，供 service-kernel 和其他宿主直接匹配。

### 文件变更清单

- 修改：`crates/vulcan-agent-runtime/src/agent_runtime/runtime_stream_intervention.rs`
- 修改：`crates/vulcan-agent-runtime/src/agent_runtime/provider_transport_stream_runtime.rs`
- 修改：`crates/vulcan-agent-runtime/src/agent_runtime/task_runtime/agent_control.rs`
- 修改：`crates/vulcan-agent-runtime/src/agent_runtime/task_runtime/tests/runtime_flow.rs`
- 修改：`crates/vulcan-agent-runtime/src/agent_runtime/mod.rs`
- 修改：`crates/vulcan-agent-runtime/src/lib.rs`
- 修改：`crates/vulcan-service-kernel/src/agent_stream/mailbox.rs`
- 修改：`crates/vulcan-service-kernel/src/tests/mailbox_tests.rs`
- 修改：`crates/vulcan-service-kernel/src/lib.rs`
- 修改：`TASK_LOG.md`
- 新增/删除：无

### 修改范围代码审核与自动修复

- 首次编译发现子智能体模块只导入旧回调别名，且既有测试仍对错误调用字符串 `contains`；已补齐类型导入，并把测试改为直接匹配错误变体及字段。
- 新错误最初只在 `agent_runtime` 模块重导出，service-kernel 从 crate 根导入失败；已补齐 crate 根公共导出，保持现有 API 入口一致。
- 首版子智能体干预直接调用 requeue，审核发现会绕过旧恢复 helper 的 trace 与失败日志；已将共享恢复结果结构化，并让在线/离线路径继续复用同一诊断链。
- 结构化恢复初版只保留底层恢复错误，审核后把消息数量、目标 task 和失败阶段写入 `recovery_error`，避免诊断信息退化。
- WebUI 无效 mailbox 测试现在明确断言 `InvalidInput`；子智能体身份冲突测试明确断言 `ProjectionConflict`，同时继续验证 Core 草稿不提交且 inbox 已恢复。
- CodeKit 最终扫描 7 个相关文件、127 个结构项，解析错误为 0。

### 验证结果

- WebUI 无效 mailbox 类型化错误与事务不变式测试独立通过。
- 子智能体显示身份冲突类型化错误、Core 不变和 inbox 恢复测试独立通过。
- `cargo test -p vulcan-agent-runtime -p vulcan-service-kernel`：4 个测试套件共 953 项全部通过。
- 九包测试矩阵：17 个测试套件共 1354 项全部通过。
- 九包 `cargo clippy --all-targets -- -D warnings`：通过，零告警。
- `cargo fmt --all -- --check` 与 `git diff --check`：通过。

### 遗留问题与下一轮方向

- `append_inter_agent_messages_to_snapshot` 的幂等归属仍通过 JSON 序列化比较；下一轮可追踪 Core message/record 的身份与内容相等语义，移除序列化型比较。
- `goal_summary_from_value` 仍以 `Option` 丢失字段级解析原因，可继续收口为类型化解析错误。
- `RuntimePromptInterventionError` 进入 Core 后仍统一为 `InvalidState`，这是刻意的层边界；如果未来 Core 需要宿主错误类别，应通过通用错误代码协议扩展，而不是依赖 agent-runtime 类型。
- 本轮没有提交、暂存或覆盖用户其他未提交变更。

## 第 106 轮：移除智能体离线投影的 JSON 序列化型幂等比较（2026-07-11）

### 问题与事实依据

- `append_inter_agent_messages_to_snapshot` 为验证同一 delivery 的既有 provider message、隐藏 record 和显示 record 是否属于同一内容，分别把既有实体与期望实体序列化成 `serde_json::Value` 后比较。
- 该路径每条消息最多执行六次完整结构转换，产生不必要的分配；同时把本应纯粹的领域相等判断变成可失败操作，迫使函数传播序列化错误。
- Core `ConversationMessage`、`ConversationRecord`、`ConversationPayload` 及嵌套 tool call、role、record kind 的全部字段均是确定性值类型；JSON `Value` 字段本身具有结构相等语义，未发现需要忽略的临时字段或顺序无关集合。
- 旧 JSON 比较并未进行规范化或哈希，只是序列化后逐值比较。因此领域级 `PartialEq/Eq` 与旧判断覆盖同一真实内容集合，不需要 ownership fingerprint。

### 已查明的执行流程

1. idle 子智能体续跑从 inbox 取出排队消息，构造新的 task invocation snapshot。
2. 每个 delivery 确定性地产生一个 provider-visible message、一个隐藏 agent record 和一个使用 delivery ID 的显示 record。
3. 离线 snapshot 可能已经包含这些投影；完全相同表示同一次 delivery 的幂等重放，应直接复用。
4. 同一 ID 但任意内容不同表示身份已被其他所有者占用，必须拒绝整个候选快照并恢复 inbox。
5. 全部三种投影确认或追加后，候选 snapshot 通过 Core/session 双向桥接验证，最后一次性提交。

### 长期优化调整

- 为 Core `ConversationMessage`、`ConversationRecord`、`ConversationToolCall`、`ConversationPayload`、`TranscriptRole` 和 `TranscriptRecordKind` 声明明确的 `PartialEq/Eq` 领域语义。
- 删除三处 `serde_json::to_value` 比较，改为直接比较 Core 实体；幂等验证不再分配 JSON 树，也不再产生序列化错误。
- 抽取通用 `retain_or_append_inter_agent_projection`：统一“按稳定 ID 查找、相同则复用、不同则冲突、未占用则追加”的所有权规则。
- provider message、隐藏 agent record 和显示 record 都通过同一个 helper 执行，消除三段易漂移的重复逻辑，同时保留原有投影类型诊断文本。
- 新增完全相同 delivery 的二次重放测试，验证 message 保持 1 条、record 保持 2 条且全部内容与首次提交完全一致。

### 文件变更清单

- 修改：`crates/vulcan-core/src/types.rs`
- 修改：`crates/vulcan-agent-runtime/src/agent_runtime/task_runtime/agent_control.rs`
- 修改：`crates/vulcan-agent-runtime/src/agent_runtime/task_runtime/tests/runtime_flow.rs`
- 修改：`TASK_LOG.md`
- 新增/删除：无

### 修改范围代码审核与自动修复

- 初步方案考虑 stable ownership fingerprint；源码确认所有参与字段均已有确定值相等语义，且旧逻辑没有规范化，因此 fingerprint 会增加碰撞、版本和维护问题，最终采用直接领域相等。
- 三处直接比较若分别改写仍会保留重复所有权逻辑；审核后抽取泛型 helper，使冲突规则只有一个实现点。
- helper 在比较前仅复制稳定 ID 字符串，用于在 `expected` 被追加移动后保持诊断身份；相较完整 JSON 树分配，成本固定且语义清晰。
- 既有冲突测试继续验证显示 record 同 ID 异内容时整份 snapshot 不变；新增测试覆盖同 ID 同内容的精确幂等重放。
- Core 类型新增 `Eq` 后完整编译与测试证明所有嵌套类型均满足严格相等契约，没有引入浮点或非自反字段。
- CodeKit 最终扫描 3 个相关文件、123 个结构项，解析错误为 0。

### 验证结果

- 两项离线 snapshot 幂等/冲突定向测试全部通过。
- `cargo test -p vulcan-core -p vulcan-agent-runtime`：4 个测试套件共 610 项全部通过。
- 九包测试矩阵：17 个测试套件共 1355 项全部通过。
- 九包 `cargo clippy --all-targets -- -D warnings`：通过，零告警。
- `cargo fmt --all -- --check` 与 `git diff --check`：通过。

### 遗留问题与下一轮方向

- `goal_summary_from_value` 仍以 `Option` 丢失字段级解析原因，可继续改为类型化字段解析错误，提升 provider/runtime 协议诊断。
- 在线 controller 投影依赖 Core append API 自身的身份校验，离线 snapshot 使用本轮的 retain-or-append helper；后续若新增第四种 deterministic projection，应复用相同所有权规则而非再写特例。
- `ConversationSnapshot` 尚未声明整体相等，因为 continuation 的相等语义未在本轮查证；本轮只为已完整验证字段的实体增加相等契约。
- 本轮没有提交、暂存或覆盖用户其他未提交变更。

## 第 107 轮：严格化 Goal Host Projection 字段协议与共享约束（2026-07-11）

### 问题与事实依据

- `goal_summary_from_value` 原先返回 `Option<SessionGoalSummary>`，任意必填字段缺失、类型错误、UUID 无效或数值异常都会统一变成 `None`；外层只能报告笼统的 invalid goal。
- `apiCallCount`、`toolCallCount` 和 token 分项使用 `unwrap_or_default`，导致字段已提供但为负数、字符串或溢出时被静默改写为 0，掩盖 provider/runtime 协议损坏。
- host projection 的基础字段来自持久化 `ThreadGoal`；`usageTokens` 与调用/token 分项是 service 运行期可选增强字段。基础字段必须存在，增强字段只有缺失或 null 才允许默认。
- status 的唯一合法集合由 `ThreadGoalStatus` 定义；goal ID 和 session ID 都由 store 生成 UUID，不存在允许任意字符串的合法分支。
- objective 最大长度 4000 原先在原生 Goal 工具和 service WebUI validation 中各维护一份常量，创建、用户请求和 host projection 三条链存在约束漂移风险。

### 已查明的执行流程

1. create/update Goal 工具从当前 session 写入 `VulcanGoalStore`，store 生成 UUID goal ID 并保存 `ThreadGoalStatus`、预算、用量和时间戳。
2. `goal_host_projection` 把持久化 ThreadGoal 基础字段输出到 runtime tool host result。
3. service runtime event 在任何 workspace 提交前解析 host result，再验证投影 session ID 与当前事件 session 一致。
4. service 的 live accounting 可给 summary 增加 usage token、API/tool call 和 token 分类字段；这些字段在原生 host projection 中允许缺失。
5. 解析成功后 summary 写入当前 session goal cache，并通过 addon delta 刷新工作台；解析失败必须保留具体字段上下文并阻止状态提交。

### 长期优化调整

- 新增内部 `GoalSummaryParseError`，区分 `MissingField` 与 `InvalidField`，错误携带稳定字段名和明确契约。
- `goal_summary_from_value` 改为可失败的严格解析函数；外层错误现在直接指出 `sessionId`、`goalId`、`status`、`tokenBudget`、计数或 token 字段。
- session ID 与 goal ID 强制为 UUID；status 复用 `ThreadGoalStatus::try_from`，不再仅检查非空文本。
- token budget 必须为 null/缺失或正整数；tokens/time/timestamp 与 live token 分项必须为非负整数；调用计数必须可转换为平台 `usize`。
- `usageTokens` 缺失或 null 时明确回退到 `tokensUsed`；其他 live 字段缺失或 null 时默认 0，已提供的错误类型或负数不再降级。
- 将 `MAX_THREAD_GOAL_OBJECTIVE_CHARS` 提升到 workspace-store goal 领域模块并从公共门面导出；原生工具、WebUI validation 和 host parser 共用同一常量。
- objective 解析统一执行 trim、非空与共享最大长度校验。

### 文件变更清单

- 修改：`crates/vulcan-workspace-store/src/workspace_store/goal_store.rs`
- 修改：`crates/vulcan-workspace-store/src/workspace_store/mod.rs`
- 修改：`crates/vulcan-agent-runtime/src/tooling/native.rs`
- 修改：`crates/vulcan-agent-runtime/src/tooling/native/goal.rs`
- 修改：`crates/vulcan-service-kernel/src/session_goal_lifecycle.rs`
- 修改：`crates/vulcan-service-kernel/src/session_goal_lifecycle/summary_parse.rs`
- 修改：`crates/vulcan-service-kernel/src/session_goal_lifecycle/validation.rs`
- 修改：`crates/vulcan-service-kernel/src/tests/goal_todolist_tests.rs`
- 修改：`TASK_LOG.md`
- 新增/删除：无

### 修改范围代码审核与自动修复

- parser 严格化后，既有测试夹具使用非 UUID 的 `goal-three-state` 与 `foreign-goal`；源码确认真实 store 总是生成 UUID，已修正夹具而未放宽生产校验。
- 首轮集中常量后编译发现 `goal_store` 子模块没有从 `workspace_store` 门面重导出，导致下游不可见且常量被判死代码；已补齐公共导出。
- 随后测试编译暴露 service validation 还维护第三份私有 4000 常量；已删除该副本并统一引用 store 领域常量。
- 可选字段测试同时覆盖“缺失合法默认”“负数拒绝”和“字符串类型拒绝”，确保默认逻辑不再掩盖已提供的坏数据。
- 状态与范围测试覆盖未知 status、零 token budget 和超长 objective；缺失字段与无效 UUID 测试验证准确错误文本。
- CodeKit 最终扫描 6 个相关文件、94 个结构项，解析错误为 0。

### 验证结果

- Goal host projection 字段级定向测试 5 项全部通过。
- 原生 Goal 工具 planning 测试 13 项全部通过。
- `cargo test -p vulcan-workspace-store -p vulcan-agent-runtime -p vulcan-service-kernel`：6 个测试套件共 1108 项全部通过。
- 九包测试矩阵：17 个测试套件共 1358 项全部通过。
- 九包 `cargo clippy --all-targets -- -D warnings`：通过，零告警。
- `cargo fmt --all -- --check` 与 `git diff --check`：通过。

### 遗留问题与下一轮方向

- Goal objective 非空、长度与 budget 正值校验仍分别存在原生工具和 service 函数实现，虽然已共享长度常量；后续可评估把完整领域验证下沉到 workspace-store，避免规则实现漂移。
- `SessionGoalSummary.status` 仍以字符串暴露给 service API；内部解析已受 `ThreadGoalStatus` 约束，但 API 类型本身仍允许构造未知状态。
- `ConversationSnapshot` 整体相等语义仍未审计，continuation 是否适合严格相等可作为后续候选。
- 本轮没有提交、暂存或覆盖用户其他未提交变更。

## 第 108 轮：将 Goal 文本与预算规则收口到 Store 写边界（2026-07-11）

### 问题与事实依据

- Goal objective 非空/最大长度和 token budget 正值规则原先分别实现在原生工具与 service WebUI validation；第 107 轮只共享了长度常量，规则代码本身仍会漂移。
- `VulcanGoalStore::replace_thread_goal`、`insert_thread_goal`、`update_thread_goal` 和 `resume_thread_goal` 不执行任何 objective/budget 校验，也不负责 trim；新增调用者可以绕过上层入口写入空白、超长或非正预算数据。
- service save/resume 先调用校验函数，随后再次 trim objective；原生工具也先手动 trim 再调用自己的校验函数，规范化与验证属于两个容易错序的步骤。
- 上层错误文案有明确边界：原生工具面向模型使用英文，service 面向用户使用中文；这些展示文案不应下沉到 store，但底层判断必须唯一。

### 已查明的执行流程

1. 原生 `create_goal` 解析模型参数、规范化 objective、验证 budget，再调用 store insert。
2. WebUI save/resume 解析目标 session、验证 objective/budget、检查 session 与 Plan mode，再调用 store replace/update/resume。
3. runtime host projection parser 对持久化 goal 再执行 objective/budget 协议验证，随后更新 service goal cache。
4. store 四个写方法是所有持久化路径的最终共同边界；若该层不校验，任一未来入口都可能破坏 ThreadGoal 领域约束。
5. update/resume 在没有提供新 objective 时会沿用数据库旧值；长期无兼容策略要求旧值也必须重新规范化和验证，而不是继续传播损坏数据。

### 长期优化调整

- 在 workspace-store 新增 `normalize_thread_goal_objective`，一次完成 trim、非空和最大 Unicode 标量长度校验，并返回规范化借用文本。
- 新增 `validate_thread_goal_token_budget`，唯一表达“缺失或严格正整数”的预算规则。
- 将错误按不可能混淆的领域拆为 `ThreadGoalObjectiveError` 与 `ThreadGoalTokenBudgetError`，避免 objective 调用者被迫处理预算变体或反向处理。
- store 的 replace/insert 在任何 SQL 前规范化 objective 并验证 budget；update/resume 对最终 objective 和最终 budget 执行相同规则。
- update/resume 即使沿用既有 objective，也会重新 trim/验证；损坏的历史值不能借更新流程继续写回。
- 原生工具删除本地 `validate_goal_objective`/`validate_goal_budget`，直接复用 store 规则，再映射为 `NativeToolError` 英文文案。
- service 将旧 validate objective 改为 `normalize_session_goal_objective`，复用领域结果并映射中文错误；save/resume 直接使用返回的规范化字符串，不再二次 trim。
- runtime host projection parser 同样复用 objective 与 budget 领域函数，继续输出字段级协议错误。

### 文件变更清单

- 修改：`crates/vulcan-workspace-store/src/workspace_store/goal_store.rs`
- 修改：`crates/vulcan-workspace-store/src/workspace_store/mod.rs`
- 修改：`crates/vulcan-agent-runtime/src/tooling/native.rs`
- 修改：`crates/vulcan-agent-runtime/src/tooling/native/goal.rs`
- 修改：`crates/vulcan-service-kernel/src/session_goal_lifecycle.rs`
- 修改：`crates/vulcan-service-kernel/src/session_goal_lifecycle/summary_parse.rs`
- 修改：`crates/vulcan-service-kernel/src/session_goal_lifecycle/validation.rs`
- 修改：`crates/vulcan-service-kernel/src/session_goals.rs`
- 修改：`crates/vulcan-service-kernel/src/lib.rs`
- 修改：`TASK_LOG.md`
- 新增/删除：无

### 修改范围代码审核与自动修复

- 首版使用一个 `ThreadGoalValidationError` 同时承载文本与预算错误，导致调用者必须编写逻辑上不可能的兜底分支；审核后拆成两个精确错误类型，类型系统直接排除错误领域混淆。
- 首次多文件补丁写入 goal_store 后未能写入 module 门面；核对当前文件确认前半部分已落盘，随后单独补齐公共重导出，没有重复定义。
- service helper 从 validate 重命名为 normalize 后，定向编译发现 crate 根显式导出仍引用旧名称；已同步门面并删除未使用常量导入。
- 审核 store update/resume 发现沿用 existing objective 时最初未重新验证；已改为先合并最终值，再统一经过领域规范化。
- 新增 workspace-store 领域测试覆盖 trim、空文本、超长文本、缺失/正预算和零/负预算；新增 service adapter 测试覆盖中文文案与规范化结果。
- 原生 Goal planning、host projection 字段测试以及完整三包/九包矩阵共同验证三条入口仍保持各自展示契约。
- CodeKit 最终扫描 8 个相关文件、136 个结构项，解析错误为 0。

### 验证结果

- workspace-store 共享 Goal 领域规则定向测试 2 项通过。
- 原生 Goal planning 测试 13 项通过；host projection 字段级测试 5 项通过。
- `cargo test -p vulcan-workspace-store -p vulcan-agent-runtime -p vulcan-service-kernel`：6 个测试套件共 1112 项全部通过。
- 九包测试矩阵：17 个测试套件共 1362 项全部通过。
- 九包 `cargo clippy --all-targets -- -D warnings`：通过，零告警。
- `cargo fmt --all -- --check` 与 `git diff --check`：通过。

### 遗留问题与下一轮方向

- `SessionGoalSummary.status` 仍为任意字符串；可以审计 service API、WebUI JSON 和 workspace-store 的状态边界，评估改为共享类型或严格序列化枚举。
- store mutation 方法仍以 `String` 返回领域校验与 controller I/O 错误；若调用者需要可靠区分，可设计 workspace-store 自有错误枚举，而不是解析文本。
- `ConversationSnapshot` 整体相等语义仍未审计，continuation 是否适合严格相等可作为后续候选。
- 本轮没有提交、暂存或覆盖用户其他未提交变更。

## 第 109 轮：将 Session Goal API 状态从任意字符串收紧为严格枚举（2026-07-11）

### 问题与事实依据

- workspace-store 的 `ThreadGoalStatus` 已是六值枚举，前端 `SessionGoalSummary.status` 也已是六值 TypeScript 联合类型，但 Rust service API 的 `SessionGoalSummary.status` 仍是任意 `String`。
- service cache、host projection parser、prompt context 和 live accounting 需要把 store 枚举转成字符串，再通过字符串比较恢复状态语义；编译器无法检查遗漏状态或拼写错误。
- `SessionGoalSummary` 公开参与 workbench snapshot、addon delta 和 RPC response；任意字符串意味着内部代码可以构造前端类型系统不承认的 JSON 值。
- 现有线上 JSON 协议固定使用 `active`、`paused`、`blocked`、`usage_limited`、`budget_limited`、`complete`，serde snake_case 枚举可以保持协议完全不变。
- service-api 与 workspace-store 位于独立依赖层，不能让 API 反向依赖 store 类型；转换应由同时依赖两者的 service-kernel 显式拥有。

### 已查明的执行流程

1. workspace-store 从数据库严格解析 `ThreadGoalStatus`，service-kernel 将 `ThreadGoal` 投影成 `SessionGoalSummary`。
2. runtime goal host projection 同样先解析为 `ThreadGoalStatus`，再构造 service summary。
3. summary 被缓存到 `session_goal_summaries`，随后进入 workbench snapshot、addon delta 与 RPC response。
4. 前端接收 camelCase summary，其中 status 已按六值联合类型用于目标条标签、编辑/继续控制器与 workspace scope key。
5. prompt context 只允许 active 状态进入自动续跑提示，live accounting 对 complete 状态停止更新；旧实现依赖字符串比较。

### 长期优化调整

- 在 vulcan-service-api 新增 `SessionGoalStatus` 六值枚举，使用 `#[serde(rename_all = "snake_case")]` 保持既有 JSON。
- `SessionGoalSummary.status` 从 `String` 改为 `SessionGoalStatus`，并移除不再有明确默认状态语义的 `Default` 派生。
- 为 API 枚举提供 `as_str`，用于需要稳定协议键的显示/测试场景。
- service-kernel 新增穷尽式 `session_goal_status_from_thread_goal_status`，明确映射 store 六值到 API 六值；新增状态时编译器会强制更新映射。
- store cache 与 runtime host parser 都通过同一个转换函数构造 summary，不再手写字符串。
- prompt context 改为对六个 API 状态穷尽匹配；live accounting 直接比较 `SessionGoalStatus::Complete`。
- 所有 Rust 测试夹具改为枚举构造与枚举断言，没有保留 `.as_str()` 兼容比较。
- 前端类型本来已经严格且 JSON 未变化，因此不做无意义的 TS 改动。

### 文件变更清单

- 修改：`crates/vulcan-service-api/src/planning_state.rs`
- 修改：`crates/vulcan-service-kernel/src/lib.rs`
- 修改：`crates/vulcan-service-kernel/src/session_goal_lifecycle.rs`
- 修改：`crates/vulcan-service-kernel/src/session_goal_lifecycle/state_cache.rs`
- 修改：`crates/vulcan-service-kernel/src/session_goal_lifecycle/summary_parse.rs`
- 修改：`crates/vulcan-service-kernel/src/session_goal_lifecycle/accounting.rs`
- 修改：`crates/vulcan-service-kernel/src/session_goal_lifecycle/prompt_context.rs`
- 修改：`crates/vulcan-service-kernel/src/tests/goal_todolist_tests.rs`
- 修改：`TASK_LOG.md`
- 新增/删除：无

### 修改范围代码审核与自动修复

- 首次编译发现 `session_goal_lifecycle` 子模块不能依赖 crate 根的私有 service-api 导入；已把 `SessionGoalStatus` 放入该模块自己的明确导入边界。
- crate 根生产导入新枚举会在非测试构建中未使用，而测试模块通过 `use super::*` 需要它；已拆为 `#[cfg(test)]` 根导入，生产模块从领域父模块获取。
- 四处 Goal 测试夹具仍构造字符串状态；已全部改为 `SessionGoalStatus`，并把 parser 成功断言改为枚举比较。
- 新增 service-api JSON 测试覆盖 stable snake_case 序列化、未知值反序列化拒绝，以及六个 `as_str` 键与 serde 输出一致。
- prompt context 使用完整六分支匹配而不是 `_`，保证未来新增状态必须明确决定是否允许自动续跑。
- CodeKit 最终扫描 7 个相关文件、60 个结构项，解析错误为 0。

### 验证结果

- service-api 状态协议测试 3 项全部通过；Goal 相关 service-kernel 测试 21 项通过。
- `cargo test -p vulcan-service-api -p vulcan-service-kernel`：4 个测试套件共 391 项全部通过。
- 十包测试矩阵（含 vulcan-service-api）：19 个测试套件共 1385 项全部通过。
- 十包 `cargo clippy --all-targets -- -D warnings`：通过，零告警。
- `cargo fmt --all -- --check` 与 `git diff --check`：通过。

### 遗留问题与下一轮方向

- workspace-store mutation 方法仍以 `String` 混合领域校验、SQLite/controller I/O 和状态竞争错误；下一轮可追踪所有调用点并设计可区分的 store 错误枚举。
- service 请求的 status update 仍以输入字符串进入 `parse_session_goal_status`，这是 RPC 请求兼容面；可评估把请求类型也改为 service API 枚举，进一步删除字符串解析。
- `ConversationSnapshot` 整体相等语义仍未审计，continuation 是否适合严格相等可作为后续候选。
- 本轮没有提交、暂存或覆盖用户其他未提交变更。

## 第 110 轮：收紧 Session Goal 用户状态更新并消除并发覆盖（2026-07-11）

### 问题与事实依据

- 前端状态摘要已经限定六种生命周期状态，但用户状态更新请求仍以任意字符串进入后端，再由手写解析函数转换。
- 旧解析函数接受全部六种状态，因此普通用户 RPC 可以直接写入 `blocked`、`usage_limited`、`budget_limited`、`complete` 等本应由系统流程拥有的状态。
- 状态更新先读取界面快照、后直接执行通用 store update；目标被替换或运行时已写入终态时，迟到的暂停/恢复请求可能覆盖新目标或系统终态。
- `GoalUpdate` 只有预期目标标识，没有预期状态；预检查不匹配还会返回 `Some(existing)`，调用方可能把未执行的更新误判为成功。

### 已查明的执行流程

1. composer 目标控制器只会在 `active` 与 `paused` 之间切换，并通过 conversation actions service 发起状态更新 RPC。
2. service-kernel 反序列化请求，旧实现将字符串解析为任意 `ThreadGoalStatus` 后直接调用 workspace-store。
3. workspace-store 先查询当前目标，再按目标标识执行 SQL 更新并返回最新行；旧 SQL 没有校验读取时的状态。
4. 运行时记账和错误处理会独立写入预算受限、用量受限或阻塞状态，因此用户请求与系统状态变更存在真实竞争窗口。
5. 更新结果进入 session goal cache、addon delta 和 workbench snapshot，最终驱动前端目标条状态。

### 长期优化调整

- 后端请求新增私有严格枚举 `SessionGoalUserStatusUpdate`，serde 边界只接受 `active` 与 `paused`，删除字符串解析函数。
- 前端新增 `SessionGoalStatus` 与由其提取的 `SessionGoalUserStatusUpdate`，沿 composer、workspace、chat area 和 service 调用链传播窄类型。
- service 更新前读取当前目标，只允许 active/paused 用户态之间转换；系统拥有的终态与阻塞态明确拒绝用户覆盖。
- `GoalUpdate` 新增 `expected_status`，目标标识和读取时状态一并进入 SQL 条件，形成原子比较后更新。
- store 的预期条件不匹配统一返回 `None`，不再用 `Some(existing)` 冒充已应用结果。
- service 对 `None` 映射为明确竞争错误，并校验返回目标标识及最终状态，保证成功响应满足写入后置条件。
- 同步更新原生 Goal、记账、保存和继续路径的构造器；这些路径均显式选择是否携带预期状态。

### 文件变更清单

- 修改：`crates/vulcan-service-kernel/src/session_goals.rs`
- 修改：`crates/vulcan-service-kernel/src/session_goal_lifecycle/validation.rs`
- 修改：`crates/vulcan-service-kernel/src/session_goal_lifecycle/accounting.rs`
- 修改：`crates/vulcan-service-kernel/src/lib.rs`
- 修改：`crates/vulcan-workspace-store/src/workspace_store/goal_store.rs`
- 修改：`crates/vulcan-agent-runtime/src/tooling/native/goal.rs`
- 修改：`apps/vulcan-code-web/src/types/planning-questions.ts`
- 修改：`apps/vulcan-code-web/src/service/conversation-actions-service.ts`
- 修改：`apps/vulcan-code-web/src/components/workspace/workbench-layout/session-actions.ts`
- 修改：`apps/vulcan-code-web/src/components/composer/composer-panel/use-composer-goal-controller.ts`
- 修改：`apps/vulcan-code-web/src/components/composer/composer-dock.tsx`
- 修改：`apps/vulcan-code-web/src/components/composer/composer-panel.tsx`
- 修改：`apps/vulcan-code-web/src/components/conversation/chat-area/types.ts`
- 修改：`apps/vulcan-code-web/src/components/workspace/workspace-shell/types.ts`
- 修改：`TASK_LOG.md`
- 新增/删除：无

### 修改范围代码审核与自动修复

- 初步类型收紧后继续追踪 store 执行链，发现仅限制请求值仍无法阻止迟到请求覆盖并发产生的系统状态；已补充预期目标标识与预期状态双重条件。
- 审核 store 预检查语义发现不匹配时返回现有行会制造假成功；已统一为 `None`，并与 SQL 竞争失败的返回语义对齐。
- 审核所有 `GoalUpdate` 构造点，保存、继续、原生工具和错误记账路径均已显式设置 `expected_status`，没有依赖隐式默认值。
- 新增请求反序列化测试，覆盖 active/paused 接受与四种系统状态拒绝；新增状态迁移测试，覆盖系统拥有状态不可被用户转换。
- CodeKit 最终扫描 8 个关键文件、156 个结构项，解析错误为 0。

### 验证结果

- 用户请求与状态迁移定向测试 3 项全部通过。
- WebUI TypeScript 构建检查 `npm --prefix apps/vulcan-code-web run lint`：通过。
- `cargo test -p vulcan-workspace-store -p vulcan-agent-runtime -p vulcan-service-api -p vulcan-service-kernel`：8 个测试套件共 1138 项全部通过。
- 十包测试矩阵：19 个测试套件共 1388 项全部通过。
- 十包 `cargo clippy --all-targets -- -D warnings`：通过，零告警。
- `cargo fmt --all -- --check` 与 `git diff --check`：通过。

### 遗留问题与下一轮方向

- workspace-store mutation 方法仍以 `String` 混合领域校验、SQLite/controller I/O 与竞争错误；可继续设计可区分的 store 错误枚举。
- `ConversationSnapshot` 整体相等语义仍未审计，continuation 是否适合严格相等可作为后续候选。
- 保存与继续请求已获得状态竞争保护，但其完整错误类型与用户提示仍可在 store 错误类型化后统一收敛。
- 本轮没有提交、暂存或覆盖用户其他未提交变更。

## 第 111 轮：绑定 Goal 执行身份并原子化最终用量记账（2026-07-11）

### 问题与事实依据

- 运行请求过去只携带渲染后的 `session_goal_context` 文本，没有携带该文本所属的持久目标 ID；完成报告只能按会话查询报告结束时的当前目标。
- 如果旧运行期间目标被清理并重新创建，旧报告会把 Token、耗时和错误终态写入新目标；实时检查点也会把旧运行计数投影到新目标摘要。
- workspace-store 的用量记账先读取累计值、状态与预算，在 Rust 中计算新值，再写回绝对值；用户暂停目标或调整预算时，迟到写入可能重新激活暂停目标，或按过期预算错误计算 `budget_limited`。
- 原生 `update_goal` 在更新终态前执行一次零增量“记账”，忽略记账结果后再无条件更新当前目标；该步骤既不产生用量，也不能绑定两次操作期间的目标身份。
- `GoalAccountingMode` 有四个模式，其中两个完全没有生产调用，另外两个只用于上述零增量步骤与最终报告；重复枚举掩盖了真正的调用契约。

### 已查明的执行流程

1. WebUI precheck 从 `session_goal_summaries` 获取活动目标，渲染 Goal prompt context，并写入 `ExecutionPreparationRequest`。
2. agent-runtime 将该请求快照复制进 `ExecutionPreparation`，Provider prompt 只消费渲染文本；最终 `ExecutionRunReport` 保留 preparation。
3. stream task 在每个 conversation checkpoint 更新内存 Goal 用量，Finished 事件调用 `account_session_goal_from_report` 持久化最终 Token 与耗时。
4. 旧记账方法重新按 session ID 读取当前行，在进程内计算累计值和预算状态，再以 UPDATE 覆盖 tokens、time、status。
5. 报告错误随后再次读取当前目标并写入 blocked/usage_limited；该路径同样没有运行启动时的目标身份。
6. 模型调用原生 `update_goal` 时，旧实现先以可选目标 ID 执行零增量记账，再以无预期目标 ID/状态的通用更新写入 complete 或 blocked。

### 长期优化调整

- 新增 `ExecutionSessionGoalContext`，把稳定 `goal_id` 与 `prompt_context` 组成不可分割的请求级快照；request、preparation、prompt composer 与子代理清理链统一使用该类型。
- service-kernel 从同一个 `SessionGoalSummary` 同时捕获目标 ID 和提示文本，避免并行字段来自不同状态快照。
- conversation checkpoint、最终持久记账与错误终态写入全部要求匹配运行启动时的目标 ID；没有 Goal 绑定的运行直接跳过 Goal 记账。
- `account_thread_goal_usage` 的 `expected_goal_id` 从可选参数改为必填引用，从类型层禁止无绑定记账。
- 用单条 SQLite UPDATE 原子完成非负增量裁剪、i64 饱和累加、当前预算判定、活动/完成状态过滤和 RETURNING；不再在进程内读取后计算绝对值。
- SQL 只匹配绑定目标 ID 以及 active/complete 状态，因此暂停、替换和其他系统终态不会被迟到报告覆盖；并发增量直接基于数据库当前值累加，不再丢更新。
- 原生 `update_goal` 删除零增量伪记账，改为读取当前目标、穷尽校验允许的终态转换，并以预期目标 ID 与预期状态执行原子更新及结果后置校验。
- 删除没有剩余生产语义的 `GoalAccountingMode` 及其重复过滤测试。

### 文件变更清单

- 修改：`crates/vulcan-agent-runtime/src/agent_runtime/execution_models.rs`
- 修改：`crates/vulcan-agent-runtime/src/agent_runtime/mod.rs`
- 修改：`crates/vulcan-agent-runtime/src/agent_runtime/provider_runtime_context.rs`
- 修改：`crates/vulcan-agent-runtime/src/agent_runtime/task_runtime/tests/preparation_config.rs`
- 修改：`crates/vulcan-agent-runtime/src/lib.rs`
- 修改：`crates/vulcan-agent-runtime/src/tooling/native.rs`
- 修改：`crates/vulcan-agent-runtime/src/tooling/native/goal.rs`
- 修改：`crates/vulcan-agent-runtime/src/tooling/native/tests/planning.rs`
- 修改：`crates/vulcan-service-kernel/src/agent_stream/stream_task.rs`
- 修改：`crates/vulcan-service-kernel/src/session_goal_lifecycle.rs`
- 修改：`crates/vulcan-service-kernel/src/session_goal_lifecycle/accounting.rs`
- 修改：`crates/vulcan-service-kernel/src/session_goal_lifecycle/prompt_context.rs`
- 修改：`crates/vulcan-service-kernel/src/tests/goal_todolist_tests.rs`
- 修改：`crates/vulcan-workspace-store/src/workspace_store/goal_store.rs`
- 修改：`crates/vulcan-workspace-store/src/workspace_store/mod.rs`
- 修改：`TASK_LOG.md`
- 新增/删除：无

### 修改范围代码审核与自动修复

- 首轮方案只准备给旧 UPDATE 增加预期状态；继续审核发现预算编辑仍会让旧读结果覆盖当前预算语义，因此改成数据库内原子增量与预算判定。
- 根据 SQLite 官方 UPDATE 语义确认同一 SET 子句的所有右值表达式都读取更新前行，状态表达式可安全复用旧 tokens 与本次增量计算新预算状态。
- SQLite 内存库实测覆盖三条关键路径：活动目标增量后进入 budget_limited、耗时在 i64 上限饱和、暂停目标和不匹配目标 ID 均保持不变。
- 定向编译先后发现新类型缺少 agent-runtime 门面重导出、service 子模块导入边界以及两个测试模块显式导入；均已按真实模块层级修复，没有添加模糊 fallback。
- 新增终态转换测试，覆盖 active→complete、paused→complete 拒绝、budget_limited→blocked 与 complete→blocked 拒绝。
- 新增执行上下文测试，确认 prompt 文本与持久 `goal_id` 保持同一绑定。
- CodeKit 最终扫描 10 个关键文件、197 个结构项，解析错误为 0。

### 验证结果

- Goal 定向测试：3 个测试套件共 33 项通过。
- SQLite 3.51.1 内存库原子记账场景：预算限制、饱和累加、暂停保护与目标 ID 隔离均符合预期。
- `cargo test -p vulcan-workspace-store -p vulcan-agent-runtime -p vulcan-service-api -p vulcan-service-kernel`：8 个测试套件共 1139 项全部通过。
- 十包测试矩阵：19 个测试套件共 1389 项全部通过。
- 相关四包及十包 `cargo clippy --all-targets -- -D warnings`：通过，零告警。
- `cargo fmt --all -- --check` 与 `git diff --check`：通过。

### 遗留问题与下一轮方向

- workspace-store Goal 方法仍以 `String` 混合领域校验、controller/SQLite I/O 与持久数据损坏错误；可继续设计稳定错误枚举。
- `ConversationSnapshot` 整体相等语义仍未审计，continuation 是否应参与严格相等可作为下一轮候选。
- Goal live checkpoint 现在按目标 ID 隔离，但检查点计数与持久记账之间的展示一致性仍可进一步审计。
- 本轮没有提交、暂存或覆盖用户其他未提交变更。

## 第 112 轮：绑定 Goal 清理与运行停止身份并隔离流任务清理（2026-07-11）

### 问题与事实依据

- `service_clear_session_goal` 过去直接按 session ID 删除目标；用户基于旧界面发出的迟到清理请求可能删除同一会话刚创建的替换目标。
- `service_stop_conversation_run` 在停止工作完成后按 session ID 将任意 active/budget_limited 目标暂停，没有携带被停止运行启动时的 Goal ID；旧运行可能暂停新目标。
- workspace-store 的 `pause_active_thread_goal`、`usage_limit_active_thread_goal` 和私有状态更新器只按会话与状态匹配，其中 usage-limit 方法没有任何生产调用方。
- `active_stream_tasks` 过去只保存裸 `JoinHandle`，运行启动时已经存在的请求级 Goal 绑定没有进入句柄状态，因此停止入口无法知道该运行属于哪个目标。
- 流任务结束、自动压缩重启和上下文压缩结束都按 session ID 无条件删除 `active_stream_tasks`；过期旧任务结束时可能删除同槽位的新任务句柄，并继续结束新任务的 kernel turn 或投影旧报告。
- 对 `ConversationSnapshot` 候选完成生产调用搜索后，没有发现整体相等比较；当前证据不足，因此本轮没有进行猜测性重构。

### 已查明的执行流程

1. 普通 Provider 运行由 `spawn_agent_stream_task_with_auto_compaction_state` 接收 `ExecutionPreparationRequest`，请求中已有第 111 轮建立的 Goal ID 绑定。
2. 启动函数把异步任务句柄写入 `ServiceBridgeState.active_stream_tasks`；旧 map value 只有句柄，停止服务只能另行查看当前 Goal 缓存或数据库。
3. 停止服务移除并 abort 当前句柄、停止 kernel generation、更新工作区运行状态，最后调用 Goal store 暂停方法。
4. 清理 Goal RPC 解析会话后直接执行 `DELETE WHERE thread_id = ?`，随后清理预算提示缓存并投影空 Goal。
5. 普通流 Finished、无 Finished 退出、Prepared 自动压缩重启/失败、上下文压缩终止都会移除 active stream map 条目并修改 kernel turn。
6. 新流替换旧流时会 abort 旧句柄，但 abort 与旧任务已经进入结束路径可以并发；因此单纯按 session ID 清理不能证明调用方仍拥有 map 条目。

### 长期优化调整

- `delete_thread_goal` 新增必填 `expected_goal_id`，SQL 同时匹配 session ID 与 Goal ID；清理服务先读取目标身份，再执行比较删除并验证 RETURNING 后置条件。
- 清理期间目标发生替换时返回明确竞争错误，不再把删除零行错误投影为“已清空”。
- 新增严格 `pause_thread_goal(thread_id, expected_goal_id)`，只暂停绑定目标且只接受 active/budget_limited；删除旧 pause/usage-limit 通用辅助方法及私有状态更新器。
- 新增 `ActiveStreamTask` 状态实体，将唯一 task ID、可中止句柄和运行启动时的 Goal ID 组成同一 map value；普通 Provider 任务保存 Goal ID，上下文压缩任务明确保存 `None`。
- 停止服务从被移除的 `ActiveStreamTask` 读取启动期 Goal ID，随后执行比较暂停；不再依赖可能已经切换到新目标的当前缓存。
- 新增 `remove_active_stream_task_if_id`，旧任务只有在唯一 task ID 仍匹配 map 当前值时才能删除条目。
- 普通流完成、异常退出、Prepared 自动压缩与上下文压缩终止均改为身份条件清理；未拥有条目时不结束 kernel turn、不重启压缩、不应用旧终态报告。
- 新任务插入仍会显式 abort 被替换任务，但旧任务后续清理无法再触碰替换条目。

### 文件变更清单

- 修改：`crates/vulcan-service-kernel/src/state.rs`
- 修改：`crates/vulcan-service-kernel/src/agent_stream/stream_task.rs`
- 修改：`crates/vulcan-service-kernel/src/agent_stream/stream_task/prepared.rs`
- 修改：`crates/vulcan-service-kernel/src/context_compaction/streaming.rs`
- 修改：`crates/vulcan-service-kernel/src/conversation_run_services.rs`
- 修改：`crates/vulcan-service-kernel/src/session_goals.rs`
- 修改：`crates/vulcan-service-kernel/src/tests/session_kernel_tests.rs`
- 修改：`crates/vulcan-workspace-store/src/workspace_store/goal_store.rs`
- 修改：`TASK_LOG.md`
- 新增/删除：无

### 修改范围代码审核与自动修复

- 初版停止修复准备在 abort 前读取数据库目标；审核发现 Goal 存储故障会阻止真正停止运行，随后改为从活动任务实体携带启动期身份。
- 第二版尝试从当前 `session_goal_summaries` 捕获 Goal；继续推演发现缓存可能已经指向替换目标，无法证明它属于被停止运行，因此升级 `active_stream_tasks` value。
- 引入任务实体后审查全部 remove 调用，发现旧任务仍可无条件移除新任务并结束新 kernel；新增唯一 task ID 及比较删除。
- 进一步审查 Finished 顺序，发现若先记账再检查任务所有权，旧 Finished 仍可能重复记账；所有权消费已移动到 checkpoint 持久化、Goal 记账和终态投影之前。
- Prepared 自动压缩只有成功消费自己的条目才允许启动替换流；压缩失败与上下文压缩结束只有拥有条目时才允许修改 kernel/phase。
- 定向编译发现 context-compaction 子模块缺少新类型和清理函数的明确导入；已按真实模块边界补齐。
- 新增测试确认活动任务保留启动期 Goal ID，并确认过期 task ID 无法删除替换任务。
- SQLite 3.51.1 内存场景确认错误 Goal ID 无法删除或暂停当前目标，正确 ID 才能完成暂停。
- CodeKit 最终扫描 8 个关键文件、123 个结构项，解析错误为 0。

### 验证结果

- active stream 身份定向测试 2 项全部通过。
- SQLite 比较删除/暂停场景：错误目标 ID 保持新目标 active，正确目标 ID 返回并暂停目标。
- `cargo test -p vulcan-workspace-store -p vulcan-agent-runtime -p vulcan-service-api -p vulcan-service-kernel`：8 个测试套件共 1141 项全部通过。
- 十包测试矩阵：19 个测试套件共 1391 项全部通过。
- 相关四包及十包 `cargo clippy --all-targets -- -D warnings`：通过，零告警。
- `cargo fmt --all -- --check` 与 `git diff --check`：通过。

### 遗留问题与下一轮方向

- workspace-store Goal 方法仍以 `String` 混合领域校验、controller/SQLite I/O 与持久数据损坏错误；可继续设计稳定错误枚举。
- active stream task 现在拥有唯一身份，但其他按 session ID 保存的异步任务注册表仍可审计是否存在同类过期清理问题。
- Goal live checkpoint 已按 Goal ID 隔离，检查点计数与持久记账之间的展示一致性仍可进一步审计。
- 本轮没有提交、暂存或覆盖用户其他未提交变更。

## 第 113 轮：建立活动流任务先注册后执行的启动事务（2026-07-11）

### 问题与事实依据

- 第 112 轮为 `active_stream_tasks` 增加 task ID 与条件清理，但普通 Provider 流和上下文压缩仍先调用 `spawn_service_task`，随后才把句柄写入 map。
- Tokio spawn 后任务可以立即获得调度；极快失败或测试任务可能在注册前完成，结束路径条件删除零行，调用方随后再插入一个已经完成的句柄。
- 该已完成句柄会长期保留并让 `contains_key` 误判会话仍有 active stream，影响提交、编辑、压缩和导航等运行闸门。
- 注册锁失败时，普通流旧实现只记录日志；kernel turn 已经启动却没有可停止句柄，会形成幽灵运行状态。
- agent-runtime TaskRuntime 的 `active_task_runs` 已由 `ActiveTaskRunEntry.run_id` 与 `remove_active_task_run(task_id, run_id)` 保护，并有同任务新运行保留测试，不存在相同缺口。
- detached subagent listener 使用 runtime 自身可关闭/重开的事件流；终态事件移除监听集合后旧 receiver 会因 sender 释放结束，当前证据不足以证明存在同类句柄注册竞态，本轮不做猜测修改。

### 已查明的执行流程

1. 普通流启动函数生成 active stream task ID、构建 async move 任务体、调用 Tokio spawn，再把 JoinHandle 插入 `active_stream_tasks`。
2. 上下文压缩走独立但相同的 spawn-then-insert 顺序，并在终态路径按 task ID 条件清理。
3. 两类任务主体一进入执行就可能产生 Provider 请求、阶段更新、终态清理或持久化；注册前没有任何等待点。
4. 新任务替换旧条目后调用方 abort 旧句柄；若新任务已经被 spawn 放行，取消请求与新任务启动顺序也分散在两个调用点。
5. active stream map 同时参与运行判断与停止入口，因此“任务体是否已开始”和“句柄是否已注册”必须组成单一状态事务。

### 长期优化调整

- 在 state 层新增泛型 `spawn_registered_active_stream_task`，统一普通流与上下文压缩的注册协议。
- 共享函数先创建 oneshot 启动闸门，spawn 的任务首先等待 receiver；只有新 `ActiveStreamTask` 成功写入 map 后才发送放行信号。
- 注册表替换返回的旧任务在新闸门放行前收到 abort 请求，固定替换顺序并删除两处调用方重复实现。
- 注册锁失败时立即 abort 仍在等待闸门的任务并返回显式错误，任务主体不会开始执行。
- 闸门发送失败时按 task ID 比较移除刚注册的条目，避免留下已退出句柄。
- 普通 Provider 流注册失败后显式结束当前 kernel turn 为 Failed 并更新会话阶段，消除无句柄幽灵运行。
- 上下文压缩继续通过 Result 把注册/启动失败交给现有调用链处理，不再维护第二套闸门错误逻辑。

### 文件变更清单

- 修改：`crates/vulcan-service-kernel/src/state.rs`
- 修改：`crates/vulcan-service-kernel/src/agent_stream/stream_task.rs`
- 修改：`crates/vulcan-service-kernel/src/context_compaction/streaming.rs`
- 修改：`crates/vulcan-service-kernel/src/tests/session_kernel_tests.rs`
- 修改：`TASK_LOG.md`
- 新增/删除：无

### 修改范围代码审核与自动修复

- 首版在普通流与压缩流分别复制 oneshot 闸门；审核后抽成共享泛型函数，避免注册失败和回收语义长期分叉。
- 共享函数初版把旧句柄返回给调用方后才 abort，但新任务已经被放行；已将旧任务取消请求收进共享事务并置于发送闸门之前。
- Clippy 严格检查发现普通流错误分支末尾存在多余 `return`；删除时继续审计该分支，补齐 kernel Failed 与 phase Failed 收尾。
- 新增真实异步测试：任务主体启动后立即读取共享状态，断言自身 Goal 绑定已在 active stream map 中可见，从行为上验证注册先于执行。
- 第 112 轮的目标身份保留与旧 task ID 不得删除替换条目测试继续共同覆盖完整注册生命周期。
- CodeKit 对 TaskRuntime 运行表复核确认其已有 run_id 世代保护，没有重复引入第二套机制。
- CodeKit 最终扫描 6 个关键文件、76 个结构项，解析错误为 0。

### 验证结果

- active stream 身份与启动闸门定向测试 3 项全部通过。
- `cargo test -p vulcan-workspace-store -p vulcan-agent-runtime -p vulcan-service-api -p vulcan-service-kernel`：8 个测试套件共 1142 项全部通过。
- 十包测试矩阵：19 个测试套件共 1392 项全部通过。
- 相关四包及十包 `cargo clippy --all-targets -- -D warnings`：通过，零告警。
- `cargo fmt --all -- --check` 与 `git diff --check`：通过。

### 遗留问题与下一轮方向

- workspace-store Goal 方法仍以 `String` 混合领域校验、controller/SQLite I/O 与持久数据损坏错误；可继续设计稳定错误枚举。
- detached subagent listener 使用集合而不是带世代的注册实体；若后续证实同 task ID 的新运行可在旧终态处理完成前重启，应把 run identity 扩展到事件协议，而不是仅改集合类型。
- Goal live checkpoint 的展示计数与最终持久记账一致性仍可继续审计。
- 本轮没有提交、暂存或覆盖用户其他未提交变更。

## 第 114 轮：建立 Goal 预算阶段身份并统一实时与最终记账（2026-07-11）

### 问题与事实依据

- `resume_thread_goal` 将 tokens/time 清零并启动“新预算阶段”，却保留原 `goal_id`；运行绑定只能识别逻辑目标，不能识别预算阶段。
- 旧运行在暂停后迟到、用户已经继续同一目标时，旧报告仍会命中相同 Goal ID，并把上一阶段用量写进刚清零的新阶段。
- live checkpoint 对 paused、blocked、usage_limited、budget_limited 状态继续计算临时 `usage_tokens`，但最终 store 记账只接受 active/complete；暂停中的运行结束时界面用量会回退到持久值并永久丢失该回合。
- complete 状态被 live cache 特判跳过，但最终报告允许 complete 记账；模型在回合内调用 `update_goal(complete)` 后，实时计数会停滞，直到 Finished 才跳变。
- 预算耗尽与目标更新引导以 `PendingSessionPromptEntry.task_id = goal_id` 绑定旧阶段；恢复或清理过去只清除“已提示”集合，不清理内部 pending queue，旧阶段提示可能在新阶段运行。
- 前端目标条优先展示 `usageTokens`，因此 live/final 语义不一致会直接表现为计数回退或跳变。

### 已查明的执行流程

1. stream checkpoint 从当前会话状态构建累计 `SessionTurnStats`，将持久 `tokens_used` 与当前回合增量组合为临时 `usage_tokens`。
2. Finished 报告按请求捕获的 Goal ID 调用 workspace-store 原子记账，缓存返回的持久 Goal，再以 `add_turn_usage_to_accounted_total = false` 刷新实时统计字段。
3. 用户暂停只改变 Goal 生命周期，不一定停止 Provider 回合；同一绑定回合仍可能继续产生 checkpoint 和 Finished。
4. 用户继续目标时，store 过去在同一行保留 goal_id，只重置状态、预算、tokens/time；service 随后启动自动续跑。
5. budget-limited guidance 与 objective-updated guidance 进入 session runtime 的内部 pending queue，并以阶段 goal_id 写入 `task_id`。
6. Goal strip 展示 `usageTokens ?? tokensUsed`，prompt context 和完成报告则使用持久 `tokens_used`。

### 长期优化调整

- 新增严格 `GoalResume` 载荷，将新阶段 objective/budget 与必填的旧 Goal ID/状态前置条件分离，不再复用含大量可选字段的通用 `GoalUpdate`。
- `resume_thread_goal` 每次成功恢复都生成新的 UUID Goal ID，同时重置 `created_at_ms`、`updated_at_ms`、tokens 与 time，明确形成独立预算阶段。
- resume SQL 严格匹配旧阶段 ID 与状态；service 后置校验要求返回 active 且 Goal ID 必须变化。
- 最终用量记账只按 session + 预算阶段 Goal ID 匹配，不再按生命周期过滤；paused/blocked/limited/complete 阶段仍累计其绑定运行已经消耗的 Token 与时间，但 SQL 保留现有状态。
- active 状态仍由同一原子 SQL 根据数据库当前预算进入 budget_limited；其他状态不会被最终报告重新激活。
- 提取 `update_session_goal_live_usage`，live checkpoint 对所有生命周期状态采用同一绑定阶段规则；Goal ID 不匹配时完全不修改替换阶段。
- 新增 `clear_session_goal_stage_runtime_state`，在恢复或清理目标时一次锁内删除旧阶段 budget marker，并按 `PendingSessionPromptEntry.task_id` 移除旧阶段内部引导。
- 旧阶段与新阶段 UUID 隔离后，迟到报告、实时 checkpoint、预算提示与自动续跑都拥有同一个明确阶段边界。

### 文件变更清单

- 修改：`crates/vulcan-workspace-store/src/workspace_store/goal_store.rs`
- 修改：`crates/vulcan-workspace-store/src/workspace_store/mod.rs`
- 修改：`crates/vulcan-service-kernel/src/lib.rs`
- 修改：`crates/vulcan-service-kernel/src/session_goals.rs`
- 修改：`crates/vulcan-service-kernel/src/session_goal_lifecycle/accounting.rs`
- 修改：`crates/vulcan-service-kernel/src/session_goal_lifecycle/budget_guidance.rs`
- 修改：`crates/vulcan-service-kernel/src/tests/prompt_runtime_state_tests.rs`
- 修改：`crates/vulcan-service-kernel/src/tests/goal_todolist_tests.rs`
- 修改：`TASK_LOG.md`
- 新增/删除：无

### 修改范围代码审核与自动修复

- 初步只审计 live/final 计数差异；沿恢复流程继续追踪后发现相同 Goal ID 跨越计数清零边界，使第 111 轮运行绑定仍不足以隔离预算阶段，优先修复身份根因。
- resume 原本接收 `GoalUpdate` 并静默忽略其 status 语义；拆分 `GoalResume` 后，字段与 store 实际行为一一对应，预期身份和状态改为必填。
- 审核新阶段 ID 后发现旧 budget marker 会成为无意义常驻数据，进一步追踪到内部 pending guidance 也按旧 ID 绑定；两者已统一清理。
- SQLite 3.51.1 内存场景验证：恢复返回新 stage ID 且计数/时间/created_at 重置；旧 stage ID 更新零行；paused 新阶段仍可累计绑定用量且保持 paused。
- 新增 live usage 测试覆盖 paused 与 complete 状态均更新、临时增量与持久值切换，以及 replacement Goal ID 完全拒绝。
- 新增退役阶段清理测试，确认只移除旧阶段 pending guidance 和 marker，新阶段引导保持不变。
- 相关包首次 Clippy 发现提取 helper 后多余借用；已自动修复并重新完成严格检查。
- CodeKit 最终扫描 8 个相关文件、118 个结构项，解析错误为 0。

### 验证结果

- live usage 与退役阶段清理定向测试 2 项全部通过。
- SQLite 新预算阶段身份、旧报告隔离与 paused 状态记账场景全部符合预期。
- `cargo test -p vulcan-workspace-store -p vulcan-agent-runtime -p vulcan-service-api -p vulcan-service-kernel`：8 个测试套件共 1144 项全部通过。
- 十包测试矩阵：19 个测试套件共 1394 项全部通过。
- 十包 `cargo clippy --all-targets -- -D warnings`：通过，零告警。
- `cargo fmt --all -- --check` 与 `git diff --check`：通过。

### 遗留问题与下一轮方向

- workspace-store Goal 方法仍以 `String` 混合领域校验、controller/SQLite I/O 与持久数据损坏错误；可继续设计稳定错误枚举。
- pending guidance 的持久化与 Goal 恢复目前依赖后续 workspace 保存时机；可审计恢复 RPC 返回后进程崩溃是否会让旧内部队列从磁盘恢复。
- detached subagent listener 的事件协议仍没有显式 run identity；只有取得同 task ID 快速重启与旧终态交错的事实证据后才应扩展协议。
- 本轮没有提交、暂存或覆盖用户其他未提交变更。

## 第 115 轮：为崩溃恢复的旧 Goal 引导建立分发期阶段校验（2026-07-11）

### 问题发现与执行流程

- 第 114 轮已经在 Goal 恢复、暂停与清理时同步移除内存中的旧阶段 marker 和续跑引导，但继续追踪持久化链路后确认：Goal 位于 workspace-store SQLite，内部引导位于会话运行态文件，两者不存在跨存储原子事务。
- 正常链路为：预算或目标事件生成 `GoalContinuation` → 引导进入 `SessionRuntimeStateEntry.ui_state.pending_prompt_queue` → 后续版本化 workspace 保存将其落盘 → Goal 恢复创建新阶段 ID 并清理内存旧引导。
- 如果恢复已经写入 SQLite、内存清理已经完成，但运行态文件尚未持久化时进程崩溃，启动后仍可能从旧文件恢复退役阶段引导。原分发路径只识别 `GoalContinuation` 来源，不会再次核对其 `task_id` 是否仍属于 workspace-store 当前 Goal 阶段。
- 单纯把两次写入改成同步顺序不能解决问题：无论先写 Goal 还是先写运行态，进程都可能在两者之间退出。因此安全边界必须放在实际分发之前，并以持久 Goal 身份作为权威事实。

### 核心修复与调整概述

- 在待分发队首为 `GoalContinuation` 时，读取 workspace-store 当前 Goal；仅保留 `task_id` 与当前 `goal_id` 完全一致的引导。目标已清理时移除全部 Goal 续跑引导，缺少阶段身份的引导同样按过期数据处理。
- 清理发生后更新运行态时间戳，并通过既有会话版本号保护的后台持久化路径保存修正后的队列。即使进程在持久化完成前再次退出，下次分发仍会重复执行权威校验，安全性不依赖后台写入成功时机。
- 为分发判定增加 `StaleGoalGuidanceCleared` 结果，准备流程收到后重新获取最新 workspace 快照并继续选择队首，避免用清理前快照继续执行。
- 代码审核发现 mailbox 过期项与 Goal 过期项共用“一次重试”上限；两类过期项连续出现时，第二次清理会提前停止并延迟后续有效提示。已移除该上限，因为每次成功清理都会严格缩短有界队列，循环具备明确终止条件。
- 将新增的 Goal 阶段读取错误保持为英文，遵循运行时与 Provider 诊断文案约定。

### 📂 文件变更清单

- 修改 `crates/vulcan-service-kernel/src/pending_prompts.rs`：新增按权威 Goal 阶段清理内部引导的队列操作。
- 修改 `crates/vulcan-service-kernel/src/webui_prompt_lifecycle/pending_dispatch.rs`：分发前读取当前 Goal、清理退役引导并触发版本化持久化。
- 修改 `crates/vulcan-service-kernel/src/webui_prompt_lifecycle/pending_prepare.rs`：处理 Goal 清理判定、刷新快照并允许多个严格收缩的连续清理步骤。
- 修改 `crates/vulcan-service-kernel/src/tests/goal_todolist_tests.rs`：扩展退役阶段测试，模拟崩溃后恢复旧引导并验证只保留当前阶段。
- 修改 `TASK_LOG.md`：追加本轮探索、修复、审核与验证记录。

### 💻 关键代码调整详情

- `clear_stale_session_goal_pending_prompts` 在共享状态锁内按 `PendingSessionPromptSource::GoalContinuation` 与唯一 `task_id` 精确过滤，不影响 mailbox、用户输入或当前 Goal 阶段引导。
- `pending_prompt_dispatch_readiness` 改为异步判定；只有队首来源为 Goal 续跑时才访问 Goal store，不为常规用户提示增加数据库读取。
- `maybe_prepare_next_webui_pending_prompt` 对 mailbox 与 Goal 清理结果统一刷新 snapshot 后重试；每次重试均由实际移除数量驱动，`removed == 0` 会显式停止异常空转。
- 恢复场景测试先验证生命周期清理旧阶段，再重新插入模拟磁盘恢复的旧阶段引导，调用分发期过滤后确认旧 ID 被移除、新 ID 保持。

### 修改部分代码审核与自动修复

- 审核跨存储写入后否决“同步双写即可”的方案，因为它仍存在任一写入完成后的崩溃窗口；分发期权威校验才形成可重复恢复的安全闭环。
- 审核准备循环发现共享单次重试标志无法覆盖连续两类过期数据，已自动改为基于队列严格收缩的循环，并用 `removed == 0` 防止无进展循环。
- 审核错误输出后将新增中文运行时错误改为英文，避免破坏统一诊断约定。
- CodeKit 最终扫描 6 个相关文件、113 个结构项，解析错误为 0；确认清理函数、异步分发判定、版本化运行态持久化和 Goal store 权威读取均位于预期调用链。

### 验证结果

- 崩溃恢复旧引导定向测试 1 项通过。
- `cargo test -p vulcan-workspace-store -p vulcan-agent-runtime -p vulcan-service-api -p vulcan-service-kernel`：8 个测试套件共 1144 项全部通过。
- 相关包 `cargo clippy --all-targets -- -D warnings`：通过，零告警。
- 十包测试矩阵：19 个测试套件共 1394 项全部通过。
- 十包 `cargo clippy --all-targets -- -D warnings`：通过，零告警。
- `cargo fmt --all -- --check` 与 `git diff --check`：通过。

### ⚠️ 遗留问题与下一轮方向

- workspace-store Goal API 仍用 `String` 混合表达输入校验、SQLite/controller I/O 与持久数据损坏；下一轮可沿真实调用方设计稳定的类型化错误边界。
- 分发前读取与后续启动之间是否存在可被并发 Goal 恢复穿透的时序，需要继续追踪 service-kernel 的事件串行化与 active-run 注册边界；取得实际并发入口证据后再决定是否需要执行期二次身份校验。
- detached subagent listener 的事件协议仍没有显式 run identity；只有取得同 task ID 快速重启与旧终态交错的事实证据后才应扩展协议。
- 本轮没有提交、暂存或覆盖用户其他未提交变更。
## 第 116 轮：关闭 Goal 权威变更与回合注册之间的并发穿透（2026-07-11）

### 问题发现与执行流程

- 第 115 轮已在排队引导分发前读取 workspace-store 当前 Goal，但继续追踪后确认该读取不是执行事务：`pending_prompt_dispatch_readiness` 完成 SQLite 读取后，从克隆 workspace 取出条目，随后执行 prompt reducer 与 hook 准备，最后才在共享状态锁内注册 kernel turn。
- WebUI RPC 由 Axum 多线程路由直接调用 `dispatch_service_rpc`，不存在全局串行器。Goal 恢复、清理、保存或状态切换可以在“持久 Goal 已校验、kernel turn 尚未注册”的窗口并发提交。
- 原竞态结果为：旧引导读取旧 Goal → 用户 RPC 提交新 Goal 状态并清理共享队列 → 旧分发继续使用此前克隆快照 → kernel turn 注册成功，已经退役的 Goal 工作仍进入 Provider 链路。
- 进一步审计发现两个同源问题：手动暂停只修改 SQLite，不移除同阶段已排队引导；从 `paused` 切回 `active` 只刷新 UI，不主动恢复续跑，因此暂停既可能失效，恢复也可能停滞。
- Goal 权威变更不只来自四个用户 RPC，还来自停止运行自动暂停、原生 goal 工具事件、最终用量记账触发预算状态变化，以及错误状态转换。只修 WebUI 状态接口会留下多条旁路。

### 核心修复与调整概述

- 新增按会话维护的 `session_goal_dispatch_versions` 单调版本。每次权威 Goal 变更提交后推进版本；分发在持久 Goal 读取前后各取一次版本，只接受稳定读取，并把该版本绑定到回合注册。
- 新增 Goal 变更期间的 turn-start exclusion。它允许当前已运行回合自然结束，但从变更开始到 SQLite 提交、版本推进与队列清理完成期间，禁止任何新 kernel turn 注册。
- Goal kernel 注册在共享状态锁内原子核对捕获版本；变更期间到达的分发由 exclusion 拒绝，变更完成后才到达的旧分发由版本不一致永久拒绝。
- 为避免旧分发虽然注册失败却已经污染 hook timeline，将 Goal 版本/互斥/活动回合预检前移到 prompt reducer 之前，并保持同一共享状态锁直到 reducer 完成和 kernel 注册成功。
- 手动暂停现在清除当前阶段的排队引导；切回 `active` 后主动调用统一 Goal continuation 启动链路。恢复、清理与保存同样纳入互斥和版本推进。
- 停止运行自动暂停、运行时 goal 工具投影、最终记账与错误状态转换均接入同一版本失效协议，消除旁路。

### 📂 文件变更清单

- 修改 `crates/vulcan-service-kernel/src/state.rs`：增加按会话 Goal 分发权威版本映射。
- 修改 `crates/vulcan-service-kernel/src/project_conversation_services.rs`：增加允许现有回合完成、但禁止新回合注册的会话互斥守卫。
- 修改 `crates/vulcan-service-kernel/src/session_goal_lifecycle/state_cache.rs`：增加版本读取与推进操作。
- 修改 `crates/vulcan-service-kernel/src/session_goals.rs`：四类 Goal RPC 纳入互斥、版本失效、暂停清队列与重新激活续跑。
- 修改 `crates/vulcan-service-kernel/src/conversation_run_services.rs`：停止运行自动暂停接入互斥、版本推进与阶段清理。
- 修改 `crates/vulcan-service-kernel/src/session_goal_lifecycle/accounting.rs`：持久记账和错误状态变更推进分发权威版本。
- 修改 `crates/vulcan-service-kernel/src/agent_stream/projection.rs`：goal 工具事件在原子投影提交中同步推进版本。
- 修改 `crates/vulcan-service-kernel/src/session_runtime_kernel.rs`：增加无副作用 Goal 启动资格检查与版本绑定注册。
- 修改 `crates/vulcan-service-kernel/src/webui_prompt_lifecycle/pending_dispatch.rs`：围绕持久 Goal 读取捕获稳定版本并处理权威变化重试。
- 修改 `crates/vulcan-service-kernel/src/webui_prompt_lifecycle/pending_prepare.rs`：将 Goal 资格预检、prompt reducer 和 kernel 注册置于同一共享状态锁边界。
- 修改 `crates/vulcan-service-kernel/src/webui_turn_chaining.rs`：直接 Goal 续跑链路同样使用稳定读取与版本绑定注册。
- 修改 `crates/vulcan-service-kernel/src/tests/session_kernel_tests.rs`：新增变更期间与变更后旧版本均不能启动、当前版本可以启动的竞态回归测试。
- 修改若干测试状态构造器及 `crates/vulcan-service-kernel/src/lib.rs`：初始化和导出新增状态能力。
- 修改 `TASK_LOG.md`：追加本轮执行记录。

### 💻 关键代码调整详情

- `begin_session_turn_start_exclusion` 复用既有 session lifecycle reservation，但不拒绝已活动 kernel；其作用是为权威状态变更建立新回合注册栅栏。
- `session_goal_dispatch_version` 与 `advance_session_goal_dispatch_version` 在共享锁内读取/递增版本，使用显式溢出错误而非静默回绕。
- `pending_prompt_dispatch_readiness` 以“版本前读 → SQLite Goal 读取 → 版本后读”获得稳定快照；版本变化时返回 `GoalAuthorityChanged` 重新选择，不把混合时刻的 Goal 与版本绑定。
- `ensure_webui_goal_session_kernel_turn_start_allowed` 在任何 reducer/hook 副作用前检查版本、互斥与活动 kernel；调用方继续持有同一锁，随后 `start_webui_goal_session_kernel_turn` 再次校验并注册。
- `append_runtime_event_projection` 在 workspace 草稿提交前预检下一个版本，提交后以不可失败赋值同步写入 Goal 摘要与权威版本，保持既有“提交边界后无可失败操作”约束。
- `service_update_session_goal_status` 在暂停成功后清理同阶段引导，在重新激活成功后通过 `maybe_start_session_goal_continuation_if_idle` 恢复真实执行链。

### 修改部分代码审核与自动修复

- 第一版只增加临时 exclusion；审核发现守卫释放后才到达注册点的旧分发仍可穿透，已自动升级为“临时互斥 + 永久版本失效”双层协议。
- 第二轮审核发现版本核对位于 `begin_shared_prompt_turn` 之后，失败路径已经可能写入 hook timeline 并发出 hook；已把资格检查前移，并在同一状态锁下完成 reducer 与注册，彻底消除拒绝前副作用。
- 审核所有 Goal store 写入口后补齐停止暂停、goal 工具投影、记账和错误状态转换四类旁路，避免只有 WebUI RPC 推进版本。
- Clippy 发现重新激活分支的嵌套 `if` 可折叠；已自动改为 let-chain 并重新完成严格检查。
- CodeKit 最终扫描 12 个相关文件、193 个结构项，解析错误为 0。

### 验证结果

- Goal 变更期间/变更后竞态回归测试与崩溃恢复旧引导测试共 2 项通过。
- `cargo test -p vulcan-workspace-store -p vulcan-agent-runtime -p vulcan-service-api -p vulcan-service-kernel`：8 个测试套件共 1145 项全部通过。
- 相关包 `cargo clippy --all-targets -- -D warnings`：通过，零告警。
- 十包测试矩阵：19 个测试套件共 1395 项全部通过。
- 十包 `cargo clippy --all-targets -- -D warnings`：通过，零告警。
- `cargo fmt --all -- --check` 与 `git diff --check`：通过。

### ⚠️ 遗留问题与下一轮方向

- `SessionLifecycleTransitionGuard` 当前同时承载破坏性归档事务与非破坏性 Goal turn-start exclusion，虽然行为正确但领域命名和错误语义开始混杂；下一轮可评估拆分为明确的 session turn admission reservation。
- Goal 权威版本当前是进程内并发控制状态，崩溃安全仍由第 115 轮的分发期 SQLite 身份校验保证；未来若需要跨进程并发写，应将 revision 纳入持久 Goal 行并使用数据库 CAS。
- workspace-store Goal API 仍以 `String` 混合领域校验、controller/SQLite I/O 与数据损坏错误，仍适合作为后续类型化错误边界优化目标。
- detached subagent listener 的事件协议仍没有显式 run identity；只有取得同 task ID 快速重启与旧终态交错的事实证据后才应扩展协议。
- 本轮没有提交、暂存或覆盖用户其他未提交变更。

## 第 117 轮：将会话回合准入收口为类型化有所有权事务（2026-07-11）

### 问题发现与执行流程

- 第 116 轮为 Goal 权威变更复用了 `session_lifecycle_transition_ids`。继续枚举 10 个文件、19 个引用后确认，该集合同时代表归档/删除生命周期事务与允许当前回合完成的 Goal 变更栅栏，名称无法表达真实语义。
- 两类生产者都只写入 `SessionId`，消费者只能得到“存在预留”，无法区分阻塞原因；提交入口因此继续报告“正在归档或删除”，即使实际所有者是 Goal 变更。
- 原 `SessionLifecycleTransitionGuard::drop` 按 session ID 和 project ID 无条件删除集合项，没有 owner identity。虽然正常入口拒绝重叠操作，但守卫本身不能证明释放的是自己创建的那一代资源，未来重试、接管或恢复逻辑一旦替换预留，就存在过期 Drop 误删新 owner 的 ABA 风险。
- session 与 project 两张集合共同表达一个项目生命周期事务，却没有共同事务身份；无法审计跨表是否完整提交或完整释放。

### 核心修复与调整概述

- 将 `session_lifecycle_transition_ids` 替换为 `session_turn_admission_reservations: BTreeMap<SessionId, SessionTurnAdmissionReservation>`。
- 每条 session 预留包含唯一 `reservation_id` 和严格 `SessionTurnAdmissionReservationKind`，当前明确区分 `LifecycleTransition` 与 `GoalMutation`。
- 将 `project_lifecycle_transition_ids` 替换为 `project_lifecycle_transition_reservations: BTreeMap<String, Uuid>`；同一项目生命周期事务的 session/project 行共享一个 owner ID。
- 将守卫重命名为 `SessionTurnAdmissionGuard`。Drop 仅删除 owner ID 与自身一致的行；同一 key 上的 replacement reservation 会被完整保留。
- 提取 `reserve_session_turn_admission`，统一“先检查全部冲突、后批量插入”的 session 原子规则；kernel、Goal 启动预检和普通提交入口统一读取类型化预留原因。
- 删除全部旧集合字段与旧守卫名称，不保留历史兼容别名。

### 📂 文件变更清单

- 修改 `crates/vulcan-service-kernel/src/state.rs`：新增准入原因枚举、带 owner 的 session 预留结构，并将 session/project 集合改为映射表。
- 修改 `crates/vulcan-service-kernel/src/project_conversation_services.rs`：重构守卫、构造器、原子插入与 owner 校验释放；新增三项事务测试。
- 修改 `crates/vulcan-service-kernel/src/conversation_submission.rs`：按类型化预留阻止提交并报告真实 owner 原因。
- 修改 `crates/vulcan-service-kernel/src/session_runtime_kernel.rs`：kernel 与 Goal 启动门读取预留类型诊断。
- 修改 `crates/vulcan-service-kernel/src/tests/session_kernel_tests.rs`：生命周期 kernel 拒绝测试改用真实预留对象。
- 修改 `crates/vulcan-service-kernel/src/lib.rs`、测试支持与若干测试状态构造器：初始化新的 session/project 映射表。
- 修改 `TASK_LOG.md`：追加本轮执行记录。

### 💻 关键代码调整详情

- `SessionTurnAdmissionReservationKind::diagnostic_label` 为 runtime/kernel 错误提供稳定英文原因；用户侧 RPC 继续在中文上下文中嵌入精确原因。
- `SessionTurnAdmissionReservation` 将 `reservation_id` 与 `kind` 固化在每个 session 行，既支持冲突诊断，也支持代际安全释放。
- `begin_session_turn_start_exclusion` 创建 `GoalMutation` 预留；`begin_lifecycle_transition` 创建 `LifecycleTransition` 预留，并为项目行写入同一个 `reservation_id`。
- `SessionTurnAdmissionGuard::drop` 对两张表分别执行 owner 比较式 retain，过期守卫不再能删除更新代际。
- 项目生命周期构造顺序固定为：检查活动回合 → 检查全部项目冲突 → 校验并批量插入 session 行 → 无失败地插入 project 行，保证跨表逻辑事务不会局部提交。

### 修改部分代码审核与自动修复

- 初版只为 session 预留增加 owner，审核发现同一 guard 的 project 资源仍是无所有权删除；已自动将项目集合升级为共享 owner ID 的映射表。
- 提取统一 helper 后，审核发现 helper 一度在项目冲突检查前插入 session 行，项目冲突会返回错误并泄漏局部预留；已重新排序所有可失败预检，新增“项目冲突后 session 表为空”回归测试。
- 新增 replacement 测试分别覆盖 session-only Goal guard 与跨 session/project 生命周期 guard，证明 stale Drop 仅释放匹配 owner。
- CodeKit 最终扫描 8 个相关文件、162 个结构项，解析错误为 0；旧 `session_lifecycle_transition_ids`、`project_lifecycle_transition_ids` 与 `SessionLifecycleTransitionGuard` 引用均为 0。

### 验证结果

- 生命周期阻塞/释放、Goal 变更阻塞、session replacement 保留、项目冲突零局部提交、跨表 replacement 保留共 5 项定向测试通过。
- `cargo test -p vulcan-workspace-store -p vulcan-agent-runtime -p vulcan-service-api -p vulcan-service-kernel`：8 个测试套件共 1148 项全部通过。
- 相关包 `cargo clippy --all-targets -- -D warnings`：通过，零告警。
- 十包测试矩阵：19 个测试套件共 1398 项全部通过。
- 十包 `cargo clippy --all-targets -- -D warnings`：通过，零告警。
- `cargo fmt --all -- --check` 与 `git diff --check`：通过。

### ⚠️ 遗留问题与下一轮方向

- 项目准入预留当前仅需要 owner ID，session 预留同时需要 owner 与原因；若未来项目也出现多种准入原因，应升级为与 session 对称的类型化结构，而不是增加平行集合。
- Goal 权威版本仍是进程内并发控制状态；跨进程写入需要持久 Goal revision 和数据库 CAS，不能依赖当前内存表。
- workspace-store Goal API 仍以 `String` 混合领域校验、controller/SQLite I/O 与数据损坏错误，可继续作为下一轮类型化错误边界候选。
- detached subagent listener 的事件协议仍没有显式 run identity；只有取得同 task ID 快速重启与旧终态交错的事实证据后才应扩展协议。
- 本轮没有提交、暂存或覆盖用户其他未提交变更。

## 第 118 轮：以 execution identity 隔离 detached subagent 续跑事件流（2026-07-11）

### 问题发现与执行流程

- `TaskExecutionRequest` 已经为每次子任务执行生成唯一 `execution_id`，mailbox、反馈去重和持久元数据也都使用该身份；但 detached runtime event stream 仍只按稳定 `task_id` 建表，WebUI listener 注册表也只是 `BTreeSet<task_id>`。
- 新执行启动流程为：`open_detached_task_event_stream_for_run(task_id)` → 父流发布 `DelegateStarted` → WebUI 按 task ID 注册 listener → child/parent 事件发布到 task-only stream → 终态关闭 stream 并由 listener 删除 task ID。
- 快速续跑时存在两条真实竞态：
  - 旧执行已发布 `DelegateFinished`，但 WebUI listener 尚未处理；新 `DelegateStarted` 看到 task ID 仍在集合中而跳过新 listener，新执行事件只能永久缓冲。
  - active-run guard 先释放、旧 stream 随后关闭；新执行可能在两步之间打开相同 task key，旧 close 或旧 listener 终态会关闭/删除新执行 owner。
- 即使新 listener 已成功替换，旧 listener 的 child 事件仍会写入共享子转录；旧 parent 终态还会删除新 listener、清理新 parent-turn binding，并把新运行任务卡覆盖为旧完成状态。

### 核心修复与调整概述

- 新增强类型 `DetachedTaskExecutionKey { task_id, execution_id }`，runtime stream registry 改为按具体执行建表。
- `open_detached_task_event_stream_for_run`、`publish_detached_task_event`、`subscribe_detached_task_events`、`close_detached_task_event_stream` 全部强制接收 `execution_id`；删除 task-only 调用契约。
- child execution、成功终态和失败终态均从同一 `TaskExecutionRequest.execution_id` 发布；旧 close 只影响自己的执行 key，新旧 subscriber 与 buffer 完全隔离。
- WebUI listener 表改为 `BTreeMap<task_id, execution_id>`。新 `DelegateStarted` 若 execution ID 不同会原子替换 owner 并启动新 listener，不再被旧 task ID 占位阻塞。
- listener 处理 child/parent 事件前校验自己仍是当前 execution owner。旧 child 输出直接丢弃；旧 parent 终态仍可按独立 execution ID 投递 mailbox 结果，但不再修改新执行投影、listener、completion gate、导航或 parent-turn binding。
- 终态清理通过 `clear_webui_detached_listener_execution_if_current` 在单个共享锁内重新核对 execution owner，避免“初始检查为真、处理中被替换、随后误清新 owner”的二次竞态。

### 📂 文件变更清单

- 修改 `crates/vulcan-agent-runtime/src/agent_runtime/task_runtime.rs`：新增强类型执行流键，并将 registry 改为执行级映射。
- 修改 `crates/vulcan-agent-runtime/src/agent_runtime/task_runtime/lifecycle_methods.rs`：四个 stream API 强制绑定 execution ID，并补全双语契约文档。
- 修改 `crates/vulcan-agent-runtime/src/agent_runtime/task_runtime/runtime_execution.rs`：open、child publish 与 close 使用请求中的唯一执行身份。
- 修改 `crates/vulcan-agent-runtime/src/agent_runtime/task_runtime/parent_events.rs`：成功/失败 parent runtime event 按具体执行发布。
- 修改 `crates/vulcan-agent-runtime/src/agent_runtime/task_runtime/tests/runtime_flow.rs`：迁移既有测试并新增同 task 多 execution 流隔离测试。
- 修改 `crates/vulcan-service-kernel/src/state.rs`：WebUI listener 集合升级为 task 到 execution owner 的映射。
- 修改 `crates/vulcan-service-kernel/src/agent_stream/detached_subagent.rs`：listener 注册、订阅、事件门控和终态清理全部绑定 execution ID。
- 修改 `crates/vulcan-service-kernel/src/subagent_event_ingest.rs`：增加 listener execution 约束的原子投影入口，在每个共享状态提交边界复核 owner。
- 修改 `crates/vulcan-service-kernel/src/conversation_history.rs`、`workbench_refresh.rs`、`agent_stream/projection.rs`、`task_mirror_status_projection.rs` 与 `runtime_workspace_helpers.rs`：从 listener owner map 精确投影 live task IDs 或执行显式删除。
- 修改 `crates/vulcan-service-kernel/src/tests/detached_subagent_tests.rs` 与相关测试构造器：迁移 listener map 并新增 replacement 清理和旧终态投影测试。
- 修改 `TASK_LOG.md`：追加本轮执行记录。

### 💻 关键代码调整详情

- `DetachedTaskExecutionKey::new` 将稳定 task identity 与具体 execution identity 组合为具名键，避免裸元组位置语义和 task-only 注释回退。
- runtime stream 的 buffer、subscriber 与 closed 状态现在全部属于单次执行；terminal-record 的 task 级判定仅在目标 execution key 不存在时用于立即关闭迟到订阅，不会关闭已由 open 创建的新执行流。
- `maybe_start_webui_detached_subagent_task_listener` 从 DelegateStarted host payload 读取已存在的严格 `execution_id`，替换同 task 的旧 owner 后订阅完全相同的 runtime execution key。
- `ingest_webui_subagent_runtime_event_for_listener_execution` 在 normalization 与 workspace/workbench 提交两个锁边界分别校验 listener owner，旧终态无法进入任务卡、转录或反馈投影。
- `clear_webui_detached_listener_execution_if_current` 将 owner 复核、listener 删除、completion-gate 清理与可选 parent binding 清理放在同一锁内完成。

### 修改部分代码审核与自动修复

- 初步只计划修 WebUI listener 集合；沿 runtime close/open 时序继续追踪后确认 task-only stream 本身也会混流或被旧 close 关闭，因此扩展为端到端 execution-scoped 协议。
- 第一版在事件入口捕获一次 `listener_is_current`；审核发现 parent ingest 具有多个锁边界，新 listener 可在两次提交之间替换 owner。已在 ingest 的每个提交边界重新核对 execution，并在终态清理锁内再次核对。
- 审核发现 `ingest_webui_subagent_runtime_event` 原本会在内部无条件删除 task listener，绕过外层 owner 判断；已拆分 execution-aware ingest 入口，listener 路径不再使用无条件删除。
- 最终审查将裸 `(SessionId, String)` registry key 替换为具名 `DetachedTaskExecutionKey`，同步修正“按 task 保存 stream”的过期文档。
- CodeKit 最终扫描 10 个相关文件、280 个结构项，解析错误为 0；四个 stream API 的 task-only 调用点为 0。

### 验证结果

- runtime 同 task 多 execution 流隔离、WebUI stale cleanup 保留 replacement、旧终态跳过新投影共 3 项定向竞态测试通过。
- `cargo test -p vulcan-agent-runtime -p vulcan-task-session -p vulcan-workbench-model -p vulcan-service-api -p vulcan-service-kernel`：10 个测试套件共 1138 项全部通过。
- 相关包 `cargo clippy --all-targets -- -D warnings`：通过，零告警。
- 十包测试矩阵：19 个测试套件共 1401 项全部通过。
- 十包 `cargo clippy --all-targets -- -D warnings`：通过，零告警。
- `cargo fmt --all -- --check` 与 `git diff --check`：通过。

### ⚠️ 遗留问题与下一轮方向

- `subagent_parent_turn_bindings` 仍按 task ID 存储，当前由 execution owner 门保护清理；若未来需要同时保留多次 execution 的父回合关联，应直接将 execution ID 纳入 binding key，而不是增加 fallback。
- `task_has_terminal_record` 是 task 级持久状态查询；当前只在 execution key 缺失时关闭迟到订阅。若未来支持查询任意历史 execution stream，需要持久 execution index，而不能从 task 最新状态推断。
- workspace-store Goal API 的 `String` 错误边界仍待类型化。
- 本轮没有提交、暂存或覆盖用户其他未提交变更。

## 第 119 轮：将子代理父回合绑定按 execution identity 分代持有（2026-07-11）

### 问题发现与执行流程

- 第 118 轮已经把 detached runtime stream 与 WebUI listener 按 `(task_id, execution_id)` 隔离，但 `subagent_parent_turn_bindings` 仍是 `BTreeMap<task_id, binding>`，稳定任务身份与具体执行身份的边界不一致。
- 绑定链路为：`DelegateStarted` 捕获父会话当前 generation/turn → detached listener 注册 binding → child 终态按 metadata 中的 execution ID 投递父 mailbox → feedback ingest 将 binding 写入父反馈记录 → listener 终态清理 binding。
- 同一 task 快速续跑时，新执行会占用 task-only binding。随后到达的旧执行终态会读取新执行的父 generation/turn：旧结果可能被错误投递到新父回合，旧反馈也可能挂到新 turn；旧 listener 清理还可能删除新执行仍需使用的 binding。
- 仅在 binding value 中增加 execution ID 并拒绝不匹配不可行：旧执行终态仍需读取自己的原始父回合绑定并完成独立 mailbox 投递，因此必须同时保留多个执行分代，而不是把旧分代覆盖掉。

### 核心修复与调整概述

- 新增强类型 `SubagentExecutionKey { task_id, execution_id }`，将父回合绑定表改为按具体执行身份索引。
- binding 注册、mailbox 查找、反馈投影与正常终态清理全部使用 metadata/listener 中的精确 execution ID，不再存在 task-only 读取或删除路径。
- 用户显式停止或关闭稳定 task 时，仍按 task ID 清理该任务的全部 binding 分代；单次执行自然结束时只清理自身分代。
- stale 旧 listener 若已失去当前 task owner，但其 mailbox 投递成功，会清理自己的旧 binding；不会删除 replacement listener、completion gate 或新执行 binding。

### 📂 文件变更清单

- 修改 `crates/vulcan-service-kernel/src/state.rs`：新增 `SubagentExecutionKey`，父回合绑定表升级为执行级映射。
- 修改 `crates/vulcan-service-kernel/src/session_runtime_kernel.rs`：binding 注册、精确清理、task 全量清理与 mailbox 查找改用执行级键。
- 修改 `crates/vulcan-service-kernel/src/agent_stream/detached_subagent.rs`：listener 注册与终态清理传递 execution ID，并处理 stale listener 的独立 binding 回收。
- 修改 `crates/vulcan-service-kernel/src/subagent_event_ingest.rs`：反馈投影从事件 metadata 解析 execution ID 后读取精确 binding。
- 修改 `crates/vulcan-service-kernel/src/runtime_workspace_helpers.rs`：显式停止和关闭 task 时调用 task 全分代清理。
- 修改 `crates/vulcan-service-kernel/src/tests/mailbox_tests.rs` 与 `tests/detached_subagent_tests.rs`：迁移执行级 binding 构造并补充同 task 多 execution 回归覆盖。
- 修改 `TASK_LOG.md`：追加本轮执行记录。

### 💻 关键代码调整详情

- `SubagentExecutionKey::new` 统一构造稳定 task identity 与具体 execution identity 的组合键，避免裸元组位置语义。
- `bind_webui_subagent_parent_turn` 允许同一 task 的不同 execution 同时保留各自 parent session、generation 与 turn ID。
- `enqueue_webui_subagent_mailbox_input` 先严格解析 execution ID，再用精确键获取 binding；stale generation 拒绝也因此引用旧执行自身的 generation，而非 replacement 的 generation。
- `clear_webui_subagent_parent_turn_binding` 仅删除一个 execution；`clear_all_webui_subagent_parent_turn_bindings_for_task` 明确表达用户停止/关闭时的跨分代资源回收语义。
- `ingest_webui_subagent_runtime_event_for_listener_execution` 使用事件 execution ID 选择反馈归属，保证旧执行反馈不会附着到新父 turn。

### 修改部分代码审核与自动修复

- 初步候选是在 binding value 中记录 execution ID；执行流审查确认这会使旧执行失去自己的有效绑定，因此改为 execution-keyed 多分代映射。
- 第一版仅让当前 listener 在常规终态清理精确 binding；审核发现 stale 旧 listener 在成功投递 mailbox 后会留下永不回收的旧 binding，已增加只清理其自身 execution binding 的分支。
- 回归测试明确证明：旧、新 execution binding 可并存；旧结果按旧 generation 被拒绝；新结果正常入队；清理旧 binding 不影响新 binding。
- CodeKit 最终扫描 7 个相关文件、242 个结构项，解析错误为 0；父回合 binding 的 task-only 读取与精确终态误删路径均已消除。

### 验证结果

- 同 task 多 execution binding 解析与 stale detached terminal replacement 保护两项定向测试通过。
- `cargo test -p vulcan-agent-runtime -p vulcan-task-session -p vulcan-workbench-model -p vulcan-service-api -p vulcan-service-kernel`：10 个测试套件共 1139 项全部通过。
- 相关包 `cargo clippy --all-targets -- -D warnings`：通过，零告警。
- 十包测试矩阵：19 个测试套件共 1402 项全部通过。
- 十包 `cargo clippy --all-targets -- -D warnings`：通过，零告警。
- `cargo fmt --all -- --check` 与 `git diff --check`：通过。

### ⚠️ 遗留问题与下一轮方向

- `subagent_completion_gate_ignored_tasks` 仍按稳定 task ID 记录忽略状态；下一轮应先追踪其 stop、send_message resume 与同 task 续跑的完整生命周期，确认该状态究竟属于 task 还是 execution，禁止直接类推修改。
- `task_has_terminal_record` 仍是 task 级持久状态查询；若未来需要查询历史 execution stream，必须增加持久 execution index。
- workspace-store Goal API 的 `String` 错误边界仍待类型化。
- 本轮没有提交、暂存或覆盖用户其他未提交变更。

## 第 120 轮：按具体续跑执行收口子代理完成闸门放行语义（2026-07-11）

### 问题发现与执行流程

- 父回合结束前通过 `webui_turn_completion_gate_from_state` 读取权威子代理 registry，统计仍为 running 的子任务；`subagent_completion_gate_ignored_tasks` 用于排除不应继续阻塞本轮父回合的异步子运行。
- `send_message` 的真实流程分为两类：目标已有 active run 时仅将消息排队，宿主结果不含 resumed execution；目标空闲时会同步调用 `resume_idle_agent_from_inbox`，生成新的 `TaskExecutionRequest.execution_id`，并返回 `resumed_execution_id` 与 `resumed_execution_status`。
- runtime 在返回 running outcome 前先发布 `DelegateStarted`，并通过 projection barrier 等待 WebUI 建立对应 execution listener，因此 service 可以用 listener 中的精确 execution identity 判断当前运行。
- 原实现没有区分这两类结果：只要 `send_message` 生成了子转录可见行，就按稳定 task ID 放行完成闸门。向一个原本仍应阻塞父回合的 active child 仅排队消息，也会错误地让父回合提前完成。
- task-only marker 还会跨 execution 泄漏：旧运行被放行后，在旧终态 listener 尚未清理的窗口内启动同 task replacement，新运行会继承旧 marker 并被错误排除。

### 核心修复与调整概述

- `subagent_completion_gate_ignored_tasks` 改为 `BTreeSet<SubagentExecutionKey>`，放行状态只属于一个 `(task_id, execution_id)`。
- 新增 running resume 解析：只有 `send_message` 明确返回 `resumed_execution_status = running` 且携带具体 execution ID 时才写入完成闸门 marker。
- queue-only delivery、同步终态 resume 与普通子转录投影均不再创建 marker；可见消息展示与父回合完成控制彻底解耦。
- 完成闸门以权威 running registry 与当前 listener execution 的交集清理 marker；无 listener 时采用阻塞语义，不通过猜测 execution 放行。
- 单次 execution 终态仅清理自身 marker；显式 stop/close 稳定 task 时清理该 task 的全部 marker 分代。
- `resumed_execution_id` 与 `resumed_execution_status` 被定义为成对协议字段，半截载荷现在显式报错。

### 📂 文件变更清单

- 修改 `crates/vulcan-service-kernel/src/state.rs`：完成闸门忽略集合改为精确执行键。
- 修改 `crates/vulcan-service-kernel/src/runtime_workspace_helpers.rs`：按当前 listener execution 计算、保留、写入和清理 marker，并区分精确清理与 task 全量清理。
- 修改 `crates/vulcan-service-kernel/src/subagent_event_ingest.rs`：统一解析 send_message resume 协议，新增 running execution 提取与成对字段校验。
- 修改 `crates/vulcan-service-kernel/src/agent_stream/projection.rs`：仅对真实启动且仍 running 的 resume execution 放行父回合。
- 修改 `crates/vulcan-service-kernel/src/agent_stream/detached_subagent.rs`：当前与 stale listener 终态均只回收自身 execution marker。
- 修改 `crates/vulcan-service-kernel/src/tests/detached_subagent_tests.rs`：证明旧 marker 不会放行同 task replacement execution。
- 修改 `crates/vulcan-service-kernel/src/tests/mailbox_tests.rs`：证明 queue-only delivery 不产生 running resume identity，真实 running resume 精确解析 execution。
- 修改 `TASK_LOG.md`：追加本轮执行记录。

### 💻 关键代码调整详情

- `webui_turn_completion_gate_from_state` 从 running registry 取得稳定任务，再通过 `detached_subagent_task_listeners` 解析当前 execution；只有精确 key 命中 marker 才排除该运行。
- `running_subagent_send_message_resume_from_event` 与 terminal wrapper 复用同一个严格协议解析器，避免把 display projection 当作运行控制事实。
- `mark_webui_subagent_completion_gate_ignored` 与 `clear_webui_subagent_completion_gate_ignored` 强制接收 execution ID；`clear_all_webui_subagent_completion_gate_ignored_for_task` 仅用于显式稳定任务生命周期操作。
- detached listener replacement 后，旧 marker 会在下一次 gate 读取时因不再属于当前 listener execution 被清理；新 execution 仍计入 pending work。

### 修改部分代码审核与自动修复

- 初始方向只是把 task-only 集合换成 execution key；执行链审查进一步发现 marker 的生产条件本身错误，原代码把“写入一条 send_message 显示记录”等同于“启动异步续跑”，已改为读取宿主返回的真实 resume identity/status。
- 审核确认 runtime 的 DelegateStarted projection barrier 保证 running resume 返回前 listener 已建立；若 listener 缺失，gate 选择继续阻塞而不是用 task ID fallback。
- 第一版协议解析在 status 缺失时直接视为未续跑；审核发现 ID/status 任一单独出现属于损坏载荷，已自动改为成对校验并显式报错。
- stale listener 审查发现旧终态成功投递 mailbox 后也应回收自身精确 marker，已与上一轮 binding 回收放在同一分支，且不会影响 replacement。
- CodeKit 最终扫描 7 个相关文件、245 个结构项，解析错误为 0；完成闸门 marker 的 task-only 写入、读取与自然终态清理路径均已消除。

### 验证结果

- running resume/queue-only 协议解析、精确 execution 放行与 replacement 重新阻塞等定向测试通过。
- `cargo test -p vulcan-agent-runtime -p vulcan-task-session -p vulcan-workbench-model -p vulcan-service-api -p vulcan-service-kernel`：10 个测试套件共 1140 项全部通过。
- 相关包 `cargo clippy --all-targets -- -D warnings`：通过，零告警。
- 十包测试矩阵：20 个测试套件共 1482 项全部通过。
- 十包 `cargo clippy --all-targets -- -D warnings`：通过，零告警。
- `cargo fmt --all -- --check` 与 `git diff --check`：通过。

### ⚠️ 遗留问题与下一轮方向

- `task_has_terminal_record` 仍按稳定 task 判断任意历史终态；当前 execution stream open 可覆盖该影响，但迟到订阅与历史查询仍没有持久 execution index，下一轮可继续审计其真实调用语义。
- `ActiveTaskRunEntry` 只持有内部 `run_id` 与 abort handle，没有暴露协议级 execution ID；当前 service 依赖 projection barrier 后的 listener owner。若其他消费者也需要查询当前执行身份，应在 runtime 建立权威 execution-aware active-run API，不能复制 listener 状态。
- workspace-store Goal API 的 `String` 错误边界仍待类型化。
- 本轮没有提交、暂存或覆盖用户其他未提交变更。

## 第 121 轮：删除 detached execution 订阅中的 task 终态猜测（2026-07-11）

### 问题发现与执行流程

- runtime 为每次子执行生成唯一 `execution_id`，`run_task_with_shared_runtime_context` 在发布 `DelegateStarted` 前调用 `open_detached_task_event_stream_for_run(task_id, execution_id)`，随后通过 projection barrier 等待 WebUI 建立 listener。
- WebUI listener 收到同一 `DelegateStarted` 后才调用 `subscribe_detached_task_events(task_id, execution_id)`；因此合法订阅的精确 stream key 必然已经存在，订阅端不需要也不应创建未来流。
- 旧实现遇到精确 key 缺失时读取 `TaskSessionRecord.last_status`：stable task 最新状态为 completed/failed/stopped 就关闭，否则创建一个默认开放流。
- `last_status` 只描述稳定 task 的最近一次状态，不携带 execution identity。它既无法证明所请求的历史 execution 已终态，也可能在同 task 新旧执行交错时把另一代状态当作当前事实。
- 更严重的是，非终态 task 的 stale、拼错或重放 execution ID 会被订阅端创建为孤儿流；没有任何 publisher 会写入这个未经过 open 边界的 key，listener 将永久等待。

### 核心修复与调整概述

- 删除 `task_has_terminal_record` 及其 task-only 持久状态推断。
- `subscribe_detached_task_events` 现在只接受 runtime 已显式打开的精确 execution stream。
- 精确 key 缺失时立即返回关闭流；不创建默认 state，不等待不存在的 publisher，也不查询 stable task 的最近状态。
- 已关闭且仍持有 buffered terminal event 的精确流继续先回放缓冲，再关闭并移除；正常 open、publish、close 行为不变。
- 回归测试改为直接验证 missing execution key 立即关闭，证明该行为与 task 是否存在、最近状态为何无关。

### 📂 文件变更清单

- 修改 `crates/vulcan-agent-runtime/src/agent_runtime/task_runtime/lifecycle_methods.rs`：收紧精确流订阅契约并删除 task 终态猜测函数。
- 修改 `crates/vulcan-agent-runtime/src/agent_runtime/task_runtime/tests/runtime_flow.rs`：用 missing execution key 回归替换人工 terminal task record 测试。
- 修改 `TASK_LOG.md`：追加本轮执行记录。

### 💻 关键代码调整详情

- `subscribe_detached_task_events` 在持有 stream registry 写锁时仅执行 `get_mut(stream_key)`；存在时转移 buffered events 并登记 subscriber，不存在时返回 `(empty, closed)`。
- closed 精确流仍在读取缓冲后删除 key，后续重复订阅同一 execution 也会立即关闭。
- `open_detached_task_event_stream_for_run` 仍是创建 execution stream state 的唯一入口，订阅操作不再混合资源创建与观测职责。

### 修改部分代码审核与自动修复

- 初步候选是把持久 task metadata 扩展为 last execution ID；执行链审查证明合法 listener 在同进程内受到 open-before-DelegateStarted 与 projection barrier 的强协议保证，持久 schema 扩展既不能证明任意历史执行，又是不必要复杂度，因此选择删除猜测。
- 审核确认 terminal buffered replay 仍依赖精确 stream state，不受删除 task fallback 影响；同 task resumed run 的显式 open 仍会等待并接收新事件。
- 新测试不再构造 completed `TaskSessionRecord`，避免继续把 task 最新状态写成 execution 订阅契约。
- CodeKit 最终扫描 3 个相关文件、63 个结构项，解析错误为 0；`task_has_terminal_record`、`terminal_task_without_stream` 与旧测试名称引用均为 0。

### 验证结果

- missing execution 立即关闭、已关闭缓冲回放、同 task resumed execution 重新打开共 3 项定向测试通过。
- `cargo test -p vulcan-agent-runtime -p vulcan-task-session -p vulcan-workbench-model -p vulcan-service-api -p vulcan-service-kernel`：10 个测试套件共 1140 项全部通过。
- 相关包 `cargo clippy --all-targets -- -D warnings`：通过，零告警。
- 十包测试矩阵：20 个测试套件共 1482 项全部通过。
- 十包 `cargo clippy --all-targets -- -D warnings`：通过，零告警。
- `cargo fmt --all -- --check` 与 `git diff --check`：通过。

### ⚠️ 遗留问题与下一轮方向

- `ActiveTaskRunEntry` 仍只有内部 `run_id` 与 abort handle；service 接收 `DelegateStarted` 时仅按事件到达顺序替换 listener owner，尚未向 runtime 核对该 execution 是否仍为当前 active run。下一轮应追踪 replay/stale DelegateStarted 是否可能夺回新 listener。
- closed execution stream state 在首个迟到 subscriber 读取后删除，符合当前单 listener 契约；若未来允许多独立 observer，需引入显式广播保留/观察者生命周期，不能恢复 subscribe 时隐式创建。
- workspace-store Goal API 的 `String` 错误边界仍待类型化。
- 本轮没有提交、暂存或覆盖用户其他未提交变更。

## 第 122 轮：建立 execution-aware active run 权威并阻断 stale DelegateStarted（2026-07-11）

### 问题发现与执行流程

- `run_task_with_shared_runtime_context` 原流程为：解析配置 → open detached execution stream → 发布 `DelegateStarted` → 等待 WebUI projection barrier → 创建 child provider future → 最后才调用 `install_active_task_run` 注册 abort handle。
- WebUI 在 barrier 内收到 `DelegateStarted` 后，`maybe_start_webui_detached_subagent_task_listener` 只比较 service 本地 `task_id -> execution_id` 映射。任何 execution ID 不同的事件都会直接覆盖当前 listener owner。
- 同 task 新执行已经替换 listener 后，迟到或重放的旧 `DelegateStarted` 仍可再次覆盖新 owner；随后订阅缺失的旧 execution stream 会立即关闭，但 service map 已被污染，新 listener 的 child/terminal 事件会被当作 stale 丢弃。
- runtime 的 `ActiveTaskRunEntry` 当时只有内部 `run_id` 与 `AbortHandle`，没有协议 `execution_id`；更早的启动阶段甚至完全没有 active entry，因此 service 无法向权威运行时核对事件身份。
- stop 也存在启动窗口：abort handle 尚未生成时没有条目，停止请求无法被保留；replacement 覆盖旧 entry 时，丢弃 `AbortHandle` 本身不会中止旧 Provider future。

### 核心修复与调整概述

- 将 active-run 注册前移到 `run_task_with_shared_runtime_context` 入口，任何配置解析或 `DelegateStarted` 发布前先预留 `(task_id, execution_id, run_id)` owner。
- `ActiveTaskRunEntry` 新增协议 `execution_id`、可延迟附加的 `Option<AbortHandle>` 与 `stop_requested`。
- child provider future 创建后通过 guard 将 abort handle 附加到同一 run token；若 owner 已被 replacement 替换，附加失败并由既有 `AbortChildRunOnDrop` 中止旧 future。
- stop、stop tree 与 app shutdown 对预留阶段也返回已停止并记录 `stop_requested`；handle 稍后附加时立即 abort，不再丢失启动前停止。
- replacement 原子写入新 execution owner，并在释放 registry 锁后立即中止旧 entry 已存在的 abort handle。
- 新增 runtime 权威查询 `is_task_execution_current(task_id, execution_id)`；WebUI 在修改 listener map 或绑定 parent turn 前必须通过该查询，stale/replayed `DelegateStarted` 直接忽略。

### 📂 文件变更清单

- 修改 `crates/vulcan-agent-runtime/src/agent_runtime/task_runtime.rs`：扩展 `ActiveTaskRunEntry`，为 guard 增加延迟附加 abort handle 能力。
- 修改 `crates/vulcan-agent-runtime/src/agent_runtime/task_runtime/lifecycle_methods.rs`：新增 prestart reservation、精确 owner 查询、延迟 handle 附加和停止请求保留，并重构 stop 路径。
- 修改 `crates/vulcan-agent-runtime/src/agent_runtime/task_runtime/runtime_execution.rs`：将 active execution 预留前移到运行入口，创建 child future 后附加 handle。
- 修改 `crates/vulcan-agent-runtime/src/agent_runtime/task_runtime/tests.rs`：新增统一的测试 active-run 预留与 handle 附加辅助函数。
- 修改 `crates/vulcan-agent-runtime/src/agent_runtime/task_runtime/tests/runtime_flow.rs`：迁移 active-run 测试，并新增 owner replacement 与 prestart stop 回归。
- 修改 `crates/vulcan-agent-runtime/src/agent_runtime/task_runtime/tests/persistence_store.rs`：迁移 app shutdown active-run 测试构造。
- 修改 `crates/vulcan-service-kernel/src/agent_stream/detached_subagent.rs`：listener 注册前核对 runtime 当前 execution owner。
- 修改 `TASK_LOG.md`：追加本轮执行记录。

### 💻 关键代码调整详情

- `reserve_active_task_run` 在任何启动事件逸出前建立 owner；新 execution 替换同 task 旧 owner 时，旧 guard 的 Drop 因 run ID 不匹配无法删除新条目。
- `attach_active_task_run_abort_handle` 同时核对 stable task 与内部 run token，避免旧启动把 handle 附加到新 execution；已保留的 stop 请求会在附加后立即生效。
- `stop_task_tree` 与 shutdown 路径改用 registry 写锁，在同一临界区标记 `stop_requested` 并快照可用 handle，锁外执行 abort。
- `is_task_execution_current` 只读取 runtime active owner；service 不再根据本地 listener 事件顺序推断“谁更新”。
- `maybe_start_webui_detached_subagent_task_listener` 在 service 状态写入前拒绝不属于 runtime 当前 owner 的 execution，因此旧事件不能夺回 replacement listener 或覆盖 parent-turn binding。

### 修改部分代码审核与自动修复

- 初版只计划为现有 active entry 增加 execution ID；执行时序证明 entry 在 `DelegateStarted` barrier 之后才存在，无法用于 listener 门控，因此将注册重构为 prestart reservation。
- 预留前移后，审核发现 stop 在 abort handle 创建前需要可持久到附加点，已增加 `stop_requested` 并补充晚附加立即中止测试。
- 第一版 replacement 只是覆盖 entry；审核确认 Tokio `AbortHandle` 被 drop 不会中止 future，已自动提取旧 handle 并在锁外显式 abort。
- 审核保留 run ID generation token：旧 guard Drop 与旧 handle attach 都必须再次核对 token，避免清理或修改 replacement。
- CodeKit 最终扫描 6 个相关文件、136 个结构项，解析错误为 0；旧 `install_active_task_run` 与旧 replacement 测试名称引用均为 0，service listener 注册路径存在唯一 runtime execution-owner 门控。

### 验证结果

- replacement owner 保留/旧 future 中止、prestart stop 晚附加生效、任务树停止共 3 项定向测试通过。
- `cargo test -p vulcan-agent-runtime -p vulcan-task-session -p vulcan-workbench-model -p vulcan-service-api -p vulcan-service-kernel`：10 个测试套件共 1141 项全部通过。
- 相关包 `cargo clippy --all-targets -- -D warnings`：通过，零告警。
- 十包测试矩阵：20 个测试套件共 1483 项全部通过。
- 十包 `cargo clippy --all-targets -- -D warnings`：通过，零告警。
- `cargo fmt --all -- --check` 与 `git diff --check`：通过。

### ⚠️ 遗留问题与下一轮方向

- 同 task replacement 仍采用“后预留者获胜”语义；prepare 阶段对 stable `TaskSessionRecord` 的修改与 active reservation 尚不是同一事务。下一轮应追踪 `invoke_task` 的 check → prepare → backend reserve 并发窗口，确认是否需要 execution-owned preparation reservation。
- prestart stop 会在 handle 附加时中止 provider future；如果未来在 DelegateStarted 之前增加长耗时异步准备，应增加显式 stop checkpoint，避免无意义准备继续消耗资源。
- workspace-store Goal API 的 `String` 错误边界仍待类型化。
- 本轮没有提交、暂存或覆盖用户其他未提交变更。

## 第 123 轮：将 TaskSession prepare 与结果提交收口为 execution-owned 事务（2026-07-11）

### 问题发现与执行流程

- `invoke_task` 原流程先读取 cached/active 状态，再调用 `prepare_task_execution_request` 修改 stable `TaskSessionRecord`，随后进入 backend；active execution reservation 直到 backend 入口才建立。
- 两个同 task 调用可以同时通过 active 检查：A prepare 写入 prompt/config，B 随后覆盖同一 stable record；之后 A、B 按 backend 到达顺序竞争 active owner，prepare 所有者与运行 owner 没有一致性保证。
- `TaskSessionRecord` 原本只保存 stable task 字段与 `last_status`，不记录具体 execution owner。`store_running_task_outcome_in_memory`、`store_task_outcome`、`mark_task_failure` 和 `discard_prestarted_task_record` 都可无条件写入或删除该 stable record。
- 真实后果包括：被取代的 A 仍可启动并覆盖 B 的 running/terminal 状态；A 的 backend failure 可把 B 标为 failed；A 的 prestart rollback 可删除 B 仍在使用的 created placeholder；持久化和 registry/mailbox 投影也可能随旧执行一起污染。

### 核心修复与调整概述

- `TaskSessionRecord` 新增 `execution_id: Option<String>`，明确 stable record 当前可变状态的具体 owner；恢复或测试构造的无 owner 记录允许首个执行原子认领。
- `prepare_task_execution_request` 在修改 record 前生成 execution ID，并在同一按 task 事务内更新 prompt、created 状态、配置、record owner 与 prepared registry 投影。
- 新增按 stable task 索引的 execution transaction lock；prepare、backend claim、running/terminal/failure 持久化与 prestart discard 全部通过同一事务边界串行。
- backend 使用 `claim_and_reserve_active_task_run` 在同一事务内校验 record owner 并建立 runtime active owner；若 record 已属于更新 execution，返回标准 prestart cancellation，不发布启动事件。
- running、terminal 与 failure 提交在任何磁盘写入、内存 commit、registry 更新或 mailbox 发布前校验 execution owner；stale execution 直接跳过。
- prestart discard 增加 execution ID 条件，旧调用不能删除新 preparation 的 placeholder。
- 事务锁注册表使用 `Weak<Mutex<()>>`，每次获取时清理无强引用条目，避免随历史 task 数量永久增长。

### 📂 文件变更清单

- 修改 `crates/vulcan-agent-runtime/src/agent_runtime/task_runtime.rs`：为内存 TaskSession record 增加 execution owner，并增加弱引用事务锁注册表。
- 修改 `crates/vulcan-agent-runtime/src/agent_runtime/task_runtime/runtime_state.rs`：初始化 execution transaction registry。
- 修改 `crates/vulcan-agent-runtime/src/agent_runtime/task_runtime/request_records.rs`：prepare 生成并原子写入 execution owner，所有 record 构造补齐无 owner 初始语义。
- 修改 `crates/vulcan-agent-runtime/src/agent_runtime/task_runtime/lifecycle_methods.rs`：提供按 task 事务锁、record owner 认领/检查及 claim+active reserve 原子入口。
- 修改 `crates/vulcan-agent-runtime/src/agent_runtime/task_runtime/runtime_execution.rs`：backend 启动改用 record claim 与 active reservation 的联合事务。
- 修改 `crates/vulcan-agent-runtime/src/agent_runtime/task_runtime/persistence_methods.rs`：running、terminal、failure、discard 全部执行 owner 校验并在事务内提交。
- 修改 `crates/vulcan-agent-runtime/src/agent_runtime/task_runtime/parent_events.rs`：新增 record owner 冲突的标准 prestart cancellation 诊断。
- 修改 `crates/vulcan-agent-runtime/src/agent_runtime/task_runtime/tests/runtime_flow.rs`：新增同 task preparation 被取代后的 claim 与 stale failure 回归。
- 修改 `TASK_LOG.md`：追加本轮执行记录。

### 💻 关键代码调整详情

- `task_execution_state_lock` 为每个 stable task 返回共享事务锁；注册表只保留 `Weak`，活动调用持有的 `Arc` 保证同一事务域，空闲条目自动在后续获取时清扫。
- `prepare_task_execution_request` 将 existing/resumed/new 三条记录路径都写入相同 execution owner，并把 existing resume 的 `last_prompt` 与 `last_status=created` 同步纳入准备提交。
- `claim_task_execution_record_owner` 对不同 owner 返回带 `[task-prestart-cancelled]` 前缀的错误；不存在 record 时允许直接 runtime 请求继续，由后续 outcome 首次创建记录。
- `task_execution_owns_record` 对明确不同 owner 返回 false；无 owner 恢复记录由当前 invocation 认领；不存在记录允许 outcome builder 创建。
- outcome/failure builders 将 `record.execution_id` 固定为 invocation execution，commit 前的 owner 校验与 per-task transaction 保证校验到持久化/registry 投影期间不会被新 prepare 穿透。

### 修改部分代码审核与自动修复

- 初版仅计划在 record 上增加 execution ID；审核发现“先校验、后持久化”之间仍可被新 prepare 穿透，因此增加按 task 事务锁并覆盖全部状态提交路径。
- 第一轮全量测试出现 26 项失败，证明 runtime 与持久化测试存在合法的“直接 TaskExecutionRequest、尚无内存 record”入口；已将不存在 record 明确为允许创建，而只拒绝记录明确属于另一 execution 的情况。修正后 agent-runtime 595 项全部通过。
- 审核发现强引用事务锁表会按历史 task 永久增长，已自动改为 Weak registry，并在获取锁时清理无活动强引用的条目。
- 并发回归证明：B prepare 后 A backend claim 返回 prestart cancellation；B 可正常认领 active owner；A 的 failure 写回不改变 B 的 execution ID、prompt、created 状态或 result。
- CodeKit 最终扫描 7 个相关文件、204 个结构项，解析错误为 0；prepare、backend claim、running/terminal/failure/discard 均存在 execution ownership 或事务门控。

### 验证结果

- superseded preparation claim 拒绝与 stale failure 隔离定向测试通过；agent-runtime 2 个测试套件共 595 项全部通过。
- `cargo test -p vulcan-agent-runtime -p vulcan-task-session -p vulcan-workbench-model -p vulcan-service-api -p vulcan-service-kernel`：10 个测试套件共 1142 项全部通过。
- 相关包 `cargo clippy --all-targets -- -D warnings`：通过，零告警。
- 十包测试矩阵：20 个测试套件共 1484 项全部通过。
- 十包 `cargo clippy --all-targets -- -D warnings`：通过，零告警。
- `cargo fmt --all -- --check` 与 `git diff --check`：通过。

### ⚠️ 遗留问题与下一轮方向

- `TaskSessionRecord.execution_id` 当前是进程内事务 owner，持久化 TaskSession metadata 尚未保存 execution ID；跨进程恢复时记录为无 owner，由首个新执行认领。若未来需要跨进程并发写入，必须在持久 metadata 与事件存储层增加 revision/execution CAS。
- stop/close 等显式 stable-task 生命周期操作刻意不受单 execution owner 限制；下一轮可审计它们与 per-task transaction 的锁边界，确认显式停止不会与 outcome persistence 交错恢复 running 状态。
- workspace-store Goal API 的 `String` 错误边界仍待类型化。
- 本轮没有提交、暂存或覆盖用户其他未提交变更。

## 第 124 轮：将 stop/close 生命周期纳入 execution outcome 事务（2026-07-11）

### 问题发现与执行流程

- 第 123 轮已经让 prepare 与 execution outcome 通过 per-task transaction 串行，但 `stop_task_tree`、`mark_stopped_tasks_in_registry` 与 `mark_archived_tasks_in_sessions` 仍直接写 sessions/registry，不持有相同事务锁。
- stop 流程为：标记 active entry `stop_requested` → abort handle → 写内存 stopped/registry → 后台 provider join 产出 stopped outcome 并持久化。旧实现允许任意同 execution running/completed/failed 结果在 stopped 写入后再次获得事务锁并覆盖状态。
- close 流程为：stop subtree → registry 标记 archived → session 标记 archived → 持久化 archived metadata。虽然持久层与 registry 已有 archived 优先保护，内存 session 仍可能在 archive 与迟到 outcome 交错时被重开。
- app shutdown 同样先写 stopped 再单独持久化；若迟到 completed 抢在持久化前提交，稳定记录与磁盘事件可能产生相互冲突的生命周期。

### 核心修复与调整概述

- 将 stopped session 写入与 registry 投影按 task 分别放入现有 execution transaction，显式 stop 与所有 outcome commit 形成确定顺序。
- 将 archived session 写入按 task 放入同一事务；archive 写入完成后，任何 execution outcome 都不能再修改该 stable record。
- 将 execution owner 检查升级为 lifecycle-aware commit policy：
  - `Archived` 拒绝全部 execution running/terminal/failure 写入。
  - `Stopped` 仅接受同 execution 的 `Stopped` 终态，用于完成正常停止持久化。
  - Created/Running/WaitingChildren/WaitingFollowup/Compacting/Completed/Failed 保持既有 execution-owner 规则。
- `stop_task` 单任务公开入口也同步写入 stopped record、registry 与 `user_interrupt` 原因，不再只发送 abort。
- 新增持久化回归：显式 stop 后迟到 running 与 completed 均被忽略，匹配 stopped outcome 可正常落盘并保持 stopped 摘要。

### 📂 文件变更清单

- 修改 `crates/vulcan-agent-runtime/src/agent_runtime/task_runtime/lifecycle_methods.rs`：stop/archive 写入纳入 per-task transaction，并新增 lifecycle-aware outcome commit 判定。
- 修改 `crates/vulcan-agent-runtime/src/agent_runtime/task_runtime/persistence_methods.rs`：running、terminal 与 failure 提交传入目标 TaskSessionStatus 进行生命周期校验。
- 修改 `crates/vulcan-agent-runtime/src/agent_runtime/task_runtime/tests/persistence_store.rs`：新增 stopped lifecycle 与迟到 outcome 持久化回归。
- 修改 `TASK_LOG.md`：追加本轮执行记录。

### 💻 关键代码调整详情

- `mark_stopped_tasks_in_registry` 逐 task 获取 transaction lock，在锁内更新 stable record，再投影 stopped registry entry；不同 task 仍可独立并发。
- `mark_archived_tasks_in_sessions` 对去重后的 task ID 逐一持锁写入 archived，避免 outcome commit 穿过 archive transition。
- `task_execution_can_commit_status` 同时核对 execution owner、当前 stable lifecycle 与 incoming status；持久化开始前就拒绝非法转换。
- `store_task_outcome` 从 `TaskExecutionOutcome.execution_status` 映射 incoming TaskSessionStatus；running 与 failure 使用明确的 Running/Failed 状态。
- stopped outcome 仍可更新 stopped record 的最终快照、摘要、turn stats 与磁盘 metadata；completed/failed 不会把用户显式停止改写成其他终态。

### 修改部分代码审核与自动修复

- 初版只准备给 stop 写入加事务锁；执行流审查确认仅串行仍不足，stop 先完成后迟到 outcome 依旧会合法获得锁，因此增加 lifecycle-aware 状态转换策略。
- 新测试首次编译暴露 TaskSessionStatus 还有 WaitingChildren、WaitingFollowup 与 Compacting 三种活动状态；已显式纳入允许分支，未使用模糊 wildcard。
- 测试还暴露 `mark_stopped_tasks_in_registry` 的模块可见性不足；已提升为 `pub(super)`，仅供 task-runtime 内部与同模块测试使用，不扩大 crate 外 API。
- 审核发现公开 `stop_task` 仍只 abort；已自动补齐与 stop tree 一致的 stable lifecycle/stop reason 写入。
- 既有 `close_agent_persists_archived_status_for_restored_agent` 已覆盖 archive 后迟到 completed outcome 不重开磁盘状态；新 lifecycle guard 同时阻断内存 commit。
- CodeKit 最终扫描 4 个相关文件、131 个结构项，解析错误为 0；stop、archive、running、terminal 与 failure 路径均共享 per-task transaction 或 lifecycle commit policy。

### 验证结果

- 显式 stop 后 running/completed 拒绝与 stopped outcome 接受的持久化回归通过；agent-runtime 2 个测试套件共 596 项全部通过。
- `cargo test -p vulcan-agent-runtime -p vulcan-task-session -p vulcan-workbench-model -p vulcan-service-api -p vulcan-service-kernel`：10 个测试套件共 1143 项全部通过。
- 相关包 `cargo clippy --all-targets -- -D warnings`：通过，零告警。
- 十包测试矩阵：20 个测试套件共 1485 项全部通过。
- 十包 `cargo clippy --all-targets -- -D warnings`：通过，零告警。
- `cargo fmt --all -- --check` 与 `git diff --check`：通过。

### ⚠️ 遗留问题与下一轮方向

- stop reason 的 session registry 更新目前在 stopped transaction 之后单独执行；outcome registry upsert 会保留已有 stop reason，但下一轮可审计 stop reason 与 status 的原子一致性是否需要收口。
- archive registry 与 session/archive metadata 分阶段执行，当前通过 registry archived 优先、session lifecycle guard 与持久层 archived 优先三层防止回退；若需要 close_agent 全操作原子失败回滚，应设计跨 registry/store 的显式事务，而不是继续叠加补偿分支。
- 持久 TaskSession metadata 仍未保存 execution owner；跨进程并发需要持久 revision/CAS。
- workspace-store Goal API 的 `String` 错误边界仍待类型化。
- 本轮没有提交、暂存或覆盖用户其他未提交变更。

## 第 125 轮：统一 stop status 与 reason 的 TaskSession 生命周期事实源（2026-07-11）

### 问题发现与执行流程

- `stop_task`、`stop_task_tree` 与 app shutdown 原先先调用 `mark_stopped_tasks_in_registry` 写 stopped status，再调用 `set_subagent_stop_reason_for_tasks` 单独写 SubagentRegistry reason；两次写入不在同一锁或事务中。
- `TaskSessionRecord` 没有 stop reason 字段，因此 per-task transaction 只保护 status，无法保护 reason。读者可短暂看到 `Stopped + None`，第二步 registry 锁失败时该不一致会永久保留。
- 正常 user interrupt 的持久化依赖后台 stopped outcome；`persist_task_record_state` 只写 status/result/error，不会把 registry 中的 `user_interrupt` reason 写入 TaskSession metadata。重启后 reason 丢失。
- app shutdown 与 close_agent 之所以能保存 reason，是因为各自另有硬编码持久化分支；同一生命周期字段因此存在 registry、shutdown helper、archive helper 三套来源。
- outcome registry upsert 会尽量保留已有 reason，但这只是事后合并，不能证明 status 与 reason 来自同一次生命周期转换。

### 核心修复与调整概述

- `TaskSessionRecord` 新增 `stop_reason: Option<String>`，成为进程内 stopped/archived reason 的唯一稳定事实源。
- `mark_stopped_tasks_in_registry(task_ids, reason)` 在单个 per-task transaction 内同时写 stopped status、reason、summary 与 registry projection；删除独立 `set_subagent_stop_reason_for_tasks`。
- `stop_task`、stop tree 与 app shutdown 分别传入 `user_interrupt`、`user_interrupt`、`app_shutdown`，不再执行第二阶段 reason 补写。
- archive session transition 同时写入 `Archived + close_agent`，与 archive registry/persistence 使用一致 reason。
- `upsert_subagent_registry_entry` 从 TaskSessionRecord 复制 reason；后续 outcome projection 不再依赖 registry 中的偶然旧值。
- `persist_task_record_state` 对 active 状态清空 metadata reason，对 stopped/archived 等非活动状态写入 record reason。
- 新 execution prepare 会清空旧 stop reason；从 persisted metadata 恢复 record 时保留其 stop reason。

### 📂 文件变更清单

- 修改 `crates/vulcan-agent-runtime/src/agent_runtime/task_runtime.rs`：TaskSessionRecord 增加 stop reason 生命周期字段。
- 修改 `crates/vulcan-agent-runtime/src/agent_runtime/task_runtime/request_records.rs`：新建/恢复/续跑记录初始化、恢复或清理 reason。
- 修改 `crates/vulcan-agent-runtime/src/agent_runtime/task_runtime/lifecycle_methods.rs`：合并 stopped status/reason 原子写入，archive 同步写入 close reason，删除独立 reason helper。
- 修改 `crates/vulcan-agent-runtime/src/agent_runtime/task_runtime/persistence_methods.rs`：registry 与 TaskSession metadata 统一从 record 投影 reason。
- 修改 `crates/vulcan-agent-runtime/src/agent_runtime/task_runtime/tests/persistence_store.rs`：扩展显式停止回归，校验内存 record、registry 与 metadata 的 reason 一致。
- 修改 `TASK_LOG.md`：追加本轮执行记录。

### 💻 关键代码调整详情

- `mark_stopped_tasks_in_registry` 的 reason 参数与 status 在同一个 task transaction 中提交；registry entry 使用已修改 record 的完整快照构建。
- `TaskSessionRecord.stop_reason` 在 stopped outcome builder 中自然随 clone 保留，最终持久化无需访问 registry 或猜测停止来源。
- `persist_task_record_state` 根据 `persisted_status.is_active()` 决定清空或保存 reason，避免 resumed/running metadata 携带上次停止原因。
- existing/resumed prepare 在写入 `last_status=created` 与新 execution owner 的同时清空 reason，确保新一轮生命周期从无停止原因开始。
- persisted record restore 从 `TaskSessionMetadata.stop_reason` 恢复，runtime_lost、app_shutdown 与 close_agent 均能保持跨重启诊断。

### 修改部分代码审核与自动修复

- 初始候选是把 reason helper 放进 stopped transaction；执行流审查确认 reason 若仍只存在 registry，正常 stopped outcome 依旧无法持久化，因此将字段提升到 TaskSessionRecord。
- 测试断言补丁首次因锚点过宽误插到 app-shutdown 用例，产生互斥的 user_interrupt/app_shutdown 断言；检查精确位置后已自动移除并写入显式 stop 用例。
- 审核确认 prepare 必须清理旧 reason，否则 stopped task 续跑后 running metadata 会携带过期停止原因；三条 prepare record 路径均已覆盖。
- 审核确认 archive registry 已有 archived 优先合并，新增 record reason 后 registry、session 与 metadata 三层使用相同 `close_agent` 值。
- CodeKit 最终扫描 5 个相关文件、158 个结构项，解析错误为 0；旧 `set_subagent_stop_reason_for_tasks` 引用为 0，status/reason 写入只剩 record 驱动路径。

### 验证结果

- 显式 stop 回归证明内存 record、SubagentRegistry 与持久 TaskSession metadata 均为 `Stopped + user_interrupt`；agent-runtime 2 个测试套件共 596 项全部通过。
- `cargo test -p vulcan-agent-runtime -p vulcan-task-session -p vulcan-workbench-model -p vulcan-service-api -p vulcan-service-kernel`：10 个测试套件共 1143 项全部通过。
- 相关包 `cargo clippy --all-targets -- -D warnings`：通过，零告警。
- 十包测试矩阵：20 个测试套件共 1485 项全部通过。
- 十包 `cargo clippy --all-targets -- -D warnings`：通过，零告警。
- `cargo fmt --all -- --check` 与 `git diff --check`：通过。

### ⚠️ 遗留问题与下一轮方向

- close_agent 仍由 registry archive、session archive、metadata archive 多阶段组成；字段值已经统一，但任一步持久化失败后的跨存储回滚尚非原子事务，可作为下一轮审计方向。
- `persist_app_shutdown_stopped_task_records` 仍显式写 app_shutdown，与 record 驱动 stopped outcome 存在功能重叠；它承担进程退出前同步落盘职责，后续可收口为复用同一 lifecycle persistence primitive。
- 持久 TaskSession metadata 仍未保存 execution owner；跨进程并发需要持久 revision/CAS。
- workspace-store Goal API 的 `String` 错误边界仍待类型化。
- 本轮没有提交、暂存或覆盖用户其他未提交变更。

## 第 126 轮：收口 close_agent 跨投影关闭事务与失败补偿（2026-07-11）

### 问题发现与执行流程

- `invoke_close_agent` 原执行顺序为停止任务树、修改 SubagentRegistry 为 Archived、修改 TaskSessionRecord、逐条保存 TaskSession metadata、清空 agent inbox。
- 级联关闭中任一后续 metadata 保存失败时，先前 metadata 已经归档，内存 registry/session 也已经归档，但工具返回错误；调用方无法判断哪些节点实际关闭。
- `TaskSessionStore` 没有跨任务批量事务，`save_task_session` 自身还会依次更新 main metadata、task metadata 与根会话 mirror，因此不能把单文件原子替换误当成跨任务事务。
- inbox 清理位于持久化之后且独立取锁；锁失败也会形成“磁盘与内存已关闭、工具却返回失败”的假失败状态。
- 迟到的 stopped outcome 使用 per-task execution transaction；若 archive 提交不取得同一事务锁，仍可能在归档后把内存 TaskSessionRecord 改回 Stopped。

### 核心修复与调整概述

- 将 registry archive 改为只读准备阶段，先生成完整、不可变的 archived entry 投影，不再提前修改运行时状态。
- archive metadata 持久化先加载全部原始快照，再逐条保存；级联中途失败时按逆序恢复已经写入的原始快照。
- 持久化成功后，一次性取得全部 task execution transaction、SubagentRegistry、TaskSession registry 与 agent inbox 写锁，再执行无失败点的内存提交与 inbox 清理。
- 内存提交取锁失败时，复用持久化前的精确 metadata 快照回滚磁盘，避免返回错误后留下已归档 metadata。
- 保留 stop tree 已成功执行的 Stopped 状态；关闭事务失败只回滚 archive 阶段，不伪造任务仍在运行。

### 📂 文件变更清单

- 修改 `crates/vulcan-agent-runtime/src/agent_runtime/task_runtime/invocation_methods.rs`：拆分 archive 准备与运行时提交，建立统一关闭事务边界。
- 修改 `crates/vulcan-agent-runtime/src/agent_runtime/task_runtime/persistence_methods.rs`：增加原始 metadata 快照、级联写入失败逆序补偿及运行时提交失败恢复能力。
- 修改 `crates/vulcan-agent-runtime/src/agent_runtime/task_runtime/tests/persistence_store.rs`：新增真实文件写入故障下的级联回滚回归测试。
- 修改 `TASK_LOG.md`：追加本轮执行记录。

### 💻 关键代码调整详情

- `prepare_archived_subagent_registry_entries` 只读取 registry，并用同一时间戳构造目标子树的 archived projections。
- `persist_archived_subagent_registry_entries` 返回持久化前的 `TaskSessionMetadata` 快照；第二个及后续节点保存失败时，逆序恢复此前已写节点。
- `commit_archived_subagent_runtime_state` 按稳定 task id 排序取得 execution transaction locks，然后同时持有 registry、sessions 与 inbox 写锁，提交期间不再调用可失败操作。
- `restore_task_session_metadata_after_close_failure` 使用同一批精确快照处理持久化成功但内存锁提交失败的补偿路径。
- 回归测试把级联后代的 `metadata.json` 替换为目录，稳定制造“父节点写入成功、后代写入失败”；验证父 metadata 回到 Stopped，父子 registry 均未进入 Archived。

### 修改部分代码审核与自动修复

- 初版事务只处理持久化中途失败；审核发现持久化成功后的运行时锁失败仍会留下磁盘 Archived，已把原始快照贯穿到运行时提交失败补偿。
- 初版统一内存提交没有取得 task execution transaction；审核确认迟到 stopped outcome 可与 archive 交错，已按排序后的 task id 获取同一组事务锁。
- 首次编译发现 SubagentRegistry 的事实 API 是 `upsert`、时间字段是 `updated_at_ms`；已依据真实类型修正，没有增加候选式兼容分支。
- 故障回归最初期望恢复 Running；真实执行证明 stop tree 在 archive 事务之前已合法提交 Stopped，因此修正断言为 Stopped，明确事务边界而非扩大回滚范围。
- `git diff --check` 通过；相关修改未覆盖工作区内其他累计变更。

### 验证结果

- close_agent 定向测试：4 项全部通过，包含正常持久化、级联关闭与故障补偿路径。
- `cargo test -p vulcan-agent-runtime --no-fail-fast`：2 个测试套件共 597 项全部通过。
- `cargo test --workspace --no-fail-fast`：63 个测试套件共 1681 项全部通过。
- `cargo fmt --all` 已执行，`git diff --check` 通过。

### ⚠️ 遗留问题与下一轮方向

- 当前补偿事务覆盖可观测的运行期错误，但通用文件系统不支持多个 metadata 文件的崩溃级原子提交；若要求断电/进程崩溃原子性，需要引入 WAL 或单一权威事务日志，这属于存储架构重大调整，需单独确认。
- stop tree 是 archive 事务的前置操作；archive 失败后任务保持 Stopped 是明确语义，后续可审计工具结果是否需要返回结构化的 `stopped_but_not_archived` 诊断。
- `mark_archived_tasks_in_sessions` 与 `clear_inter_agent_messages_for_tasks` 旧独立 helper 已不再服务 close_agent 主路径，下一轮可确认其他调用后删除无效重复抽象。
- 持久 TaskSession metadata 仍未保存 execution owner；跨进程并发需要持久 revision/CAS。
- workspace-store Goal API 的 `String` 错误边界仍待类型化。
- 本轮没有提交、暂存或覆盖用户其他未提交变更。

## 第 127 轮：删除关闭死抽象并修复触碰任务补偿遗漏（2026-07-11）

### 问题发现与执行流程

- 第 126 轮将 close_agent 收口到 prepare、persist、runtime commit 三阶段后，`mark_archived_tasks_in_sessions` 与 `clear_inter_agent_messages_for_tasks` 已无任何调用，但仍分别保留旧的独立锁与局部提交语义。
- CodeKit 反向调用扫描确认两个 helper 仅剩定义；继续保留会让后续维护者误以为 archive/session/inbox 可以脱离统一事务单独提交。
- `persist_archived_subagent_registry_entries` 在第 N 个任务保存失败时只恢复索引 `< N` 的快照，没有恢复当前任务；但 `save_task_session` 会依次写 main metadata、task metadata 与 root mirror，当前任务可能已经部分写入。
- 原补偿循环遇到首个恢复错误立即返回，后续较早任务不会继续恢复；级联关闭因此可能把一个局部故障扩大为多个残留归档节点。
- archived registry entry 在 prepare 阶段生成时间戳，metadata persistence 又独立调用当前时间；同一次关闭事务的 registry、session record 与 metadata 时间并非同一事实值。

### 核心修复与调整概述

- 删除零调用的 `mark_archived_tasks_in_sessions` 和 `clear_inter_agent_messages_for_tasks`，关闭状态与 inbox 清理只允许通过统一 runtime commit 完成。
- metadata 保存失败时把当前正在保存的任务也纳入 touched snapshots，避免忽略当前任务的多文件部分写入。
- 新增共享 snapshot restore primitive，逆序尝试恢复全部快照并聚合所有错误，不因一个损坏路径停止其余补偿。
- runtime commit 失败与 persistence 中途失败共用同一个恢复 primitive，删除两套不同的错误处理行为。
- metadata 的 status、stop reason 与 updated_at 全部直接复制 prepared archived entry，三个运行时/持久化投影使用同一关闭事实。

### 📂 文件变更清单

- 修改 `crates/vulcan-agent-runtime/src/agent_runtime/task_runtime/lifecycle_methods.rs`：删除旧 session-only archive helper。
- 修改 `crates/vulcan-agent-runtime/src/agent_runtime/task_runtime/agent_control.rs`：删除旧 inbox-only clear helper。
- 修改 `crates/vulcan-agent-runtime/src/agent_runtime/task_runtime/persistence_methods.rs`：修复 touched task 补偿范围，统一恢复 primitive 与归档时间来源。
- 修改 `crates/vulcan-agent-runtime/src/agent_runtime/task_runtime/tests/persistence_store.rs`：验证统一时间戳、聚合错误与后续快照继续恢复。
- 修改 `TASK_LOG.md`：追加本轮执行记录。

### 💻 关键代码调整详情

- `persist_archived_subagent_registry_entries` 在加载原始 metadata 时同步构建 archived metadata，并从对应 `SubagentRegistryEntry` 复制 `stop_reason` 与 `updated_at_ms`。
- 保存索引 `metadata_index` 失败后，补偿切片使用 `[..=metadata_index]`，明确包含可能发生部分写入的当前任务。
- `restore_task_session_metadata_snapshots` 逆序遍历所有快照，将失败记录为 `task_session_id + error`，完成全部恢复尝试后一次性返回聚合错误。
- `restore_task_session_metadata_after_close_failure` 改为复用共享恢复函数；运行时锁失败和持久化失败具有一致的补偿顺序与诊断格式。
- 正常关闭回归新增 `archived_metadata.updated_at == archived_entry.updated_at_ms` 断言，证明 timestamp 不再由两处时钟独立产生。

### 修改部分代码审核与自动修复

- 初始遗留项只是删除两个死 helper；执行流审核发现当前失败任务不在 rollback slice 内，已扩大为真实的事务正确性修复。
- 故障测试原先只制造后代 main metadata 不可替换；修复后当前任务也会尝试回滚，因此该损坏路径会明确报告 rollback failure。测试已调整为校验聚合错误包含后代 task id。
- 同一测试仍验证父 metadata 恢复为 Stopped，证明补偿器在后代恢复失败后没有提前退出，而是继续恢复其余已触碰任务。
- CodeKit 最终反查确认两个旧 helper 均为零命中；新的 snapshot restore primitive 只有持久化失败与 runtime commit 失败两条预期调用链。
- 定向 Clippy 以 `-D warnings` 通过，未出现删除 helper 后的死 import、死代码或类型告警。

### 验证结果

- close_agent 定向测试：4 项全部通过。
- `cargo clippy -p vulcan-agent-runtime --all-targets -- -D warnings`：通过，零告警。
- `cargo test --workspace --no-fail-fast`：63 个测试套件共 1681 项全部通过。
- `cargo fmt --all -- --check` 与 `git diff --check`：通过。

### ⚠️ 遗留问题与下一轮方向

- 多文件 `save_task_session` 仍依赖运行期补偿而非崩溃恢复协议；WAL/manifest transaction 属于存储架构重大调整，未经确认不实施。
- close_agent 失败仍只返回字符串，调用方不能结构化区分 stop 已完成、archive 未提交、archive rollback 不完整等状态；下一轮可追踪工具错误协议与 WebUI 消费链，设计非兼容的结构化失败结果。
- `persist_archived_subagent_registry_entries` 名称仍以 registry entries 描述输入而非 close transaction 语义，可结合下一轮错误协议一起收口命名与返回类型。
- 持久 TaskSession metadata 仍未保存 execution owner；跨进程并发需要持久 revision/CAS。
- workspace-store Goal API 的 `String` 错误边界仍待类型化。
- 本轮没有提交、暂存或覆盖用户其他未提交变更。

## 第 128 轮：建立 close_agent 结构化失败协议并修正工具错误展示状态（2026-07-11）

### 问题发现与执行流程

- `close_agent` 在 stop tree 成功后，prepare archive、persist archive 或 runtime commit 任一步失败都返回普通 `Err(String)`；共享工具桥把它包装成通用 `tool_error`，但已停止任务、失败阶段与 archive rollback 结果全部丢失。
- 直接把结构化失败作为普通 `Ok(VulcanToolInvocationResult)` 返回时，`VulcanManagedToolRuntimeBridge` 原先固定写 `is_error: false`，会把业务失败伪装成成功。
- 即使底层 bridge 已通过普通 `Err` 构造出 `CoreToolInvocationResult.is_error=true`，`StreamingRuntimeToolExecutor` 仍只根据外层 Rust `Result` 判断 display state；bridge 正常返回的工具失败因此一直显示为 completed。
- RuntimeEvent metadata 会保留 host result 与 display state；service close projection 只消费成功的 `subagent_control`，因此使用 `tool_error` 作为结构化失败 kind 可以避免错误关闭面板或投影 archived 状态。
- stop tree 自身返回错误时无法证明哪些任务已完成停止，继续保留普通错误；只有 stop tree 已完整成功后才生成 `stopped=true` 的结构化失败，遵守零猜测原则。

### 核心修复与调整概述

- 新增 close-agent archive rollback 类型化状态：`not_required`、`completed`、`incomplete`。
- archive persistence 改为返回类型化失败，携带人类可读详情与已证实的 rollback status，不再要求调用方解析错误字符串。
- stop tree 成功后的 prepare、persist、runtime commit 失败统一返回结构化 `tool_error` host result，同时把相同 JSON 作为模型可见输出。
- 失败协议明确包含 `closed=false`、`stopped=true`、`archive_committed=false`、`failure_phase`、`archive_rollback` 与已停止 task ids。
- 共享 managed tool bridge 根据 host result kind `tool_error` 设置 Core `is_error=true`。
- 流式执行器改为根据 `CoreToolInvocationResult.is_error` 决定 failed/completed，修复所有正常返回的工具错误被显示成 completed 的既有问题。

### 📂 文件变更清单

- 修改 `crates/vulcan-agent-runtime/src/agent_runtime/task_runtime.rs`：增加 close-agent rollback 状态与类型化持久化失败。
- 修改 `crates/vulcan-agent-runtime/src/agent_runtime/task_runtime/persistence_methods.rs`：返回类型化 archive persistence failure。
- 修改 `crates/vulcan-agent-runtime/src/agent_runtime/task_runtime/invocation_methods.rs`：建立 stop 成功后的结构化 archive 失败协议。
- 修改 `crates/vulcan-agent-runtime/src/agent_runtime/tool_runtime_bridge.rs`：保留工具自有 `tool_error` 的 Core 错误语义。
- 修改 `crates/vulcan-agent-runtime/src/agent_runtime/provider_transport_stream_executor.rs`：以 Core `is_error` 投影工具展示状态。
- 修改 `crates/vulcan-agent-runtime/src/agent_runtime/task_runtime/tests/persistence_store.rs`：验证 close-agent 结构化失败事实。
- 修改 `crates/vulcan-agent-runtime/src/agent_runtime/tool_runtime_bridge/tests.rs`：验证结构化错误桥接与 failed display state。
- 修改 `TASK_LOG.md`：追加本轮执行记录。

### 💻 关键代码调整详情

- `CloseAgentArchiveRollbackStatus` 通过 `as_str` 输出稳定协议值；`CloseAgentArchivePersistenceFailure` 将 message 与 rollback status 绑定为同一个事实对象。
- `build_close_agent_archive_failure_result` 只在 `stop_task_tree` 成功之后调用，统一生成模型载荷和 host-only `tool_error` 载荷。
- prepare archive 失败标记 `failure_phase=prepare_archive` 与 `archive_rollback=not_required`。
- persist archive 失败直接使用类型化 rollback status；runtime commit 失败则根据 metadata restore 的真实结果选择 completed 或 incomplete。
- managed bridge 不再假设所有 `Ok(VulcanToolInvocationResult)` 都是成功；工具可用 `tool_error` host kind 返回机器可读失败而不丢失 Core error bit。
- streaming executor 对策略拒绝仍保持 `is_error=false/display_state=completed`，只有真实执行错误显示 failed。

### 修改部分代码审核与自动修复

- 初始候选是在错误字符串中嵌入 JSON；因这会要求调用方猜测并解析文本，已拒绝，改用类型化 persistence failure 与 host payload。
- 审核发现仅修 close_agent 仍不足：共享流式执行器把已有 `is_error=true` 结果显示为 completed。已修正公共事实源，并将原先固化错误行为的测试改为期望 failed。
- 审核确认成功 close_agent 仍使用 `subagent_control`，失败使用 `tool_error`；service-kernel 的成功关闭投影不会误消费失败载荷。
- 新增 demo structured failure tool，证明 model output、host payload 与 Core `is_error` 在 managed bridge 后均保留。
- 故障持久化回归从 `expect_err(String)` 改为验证 `failure_phase=persist_archive`、`archive_rollback=incomplete`、stopped/closed 事实和具体失败 task id。
- CodeKit 反查确认 rollback status 只由持久化事务和 runtime restore 事实产生，没有字符串解析或候选式字段兼容。

### 验证结果

- `cargo test -p vulcan-agent-runtime --no-fail-fast`：2 个测试套件共 598 项全部通过。
- `cargo clippy -p vulcan-agent-runtime --all-targets -- -D warnings`：通过，零告警。
- `cargo test --workspace --no-fail-fast`：63 个测试套件共 1682 项全部通过。
- `cargo fmt --all -- --check` 与 `git diff --check`：通过。

### ⚠️ 遗留问题与下一轮方向

- `tool_error` 的 payload 当前由各工具自行定义，只有通用 Err 路径固定包含 tool_call_id/tool_name/arguments；下一轮可审计是否需要共享 typed builder 自动注入调用身份，避免结构化工具错误协议再次分叉。
- `persist_archived_subagent_registry_entries` 已承担 archive transaction persistence 语义但名称仍描述旧输入形态，可在协议稳定后改名。
- 多文件 `save_task_session` 仍依赖运行期补偿而非崩溃恢复协议；WAL/manifest transaction 属于存储架构重大调整，未经确认不实施。
- 持久 TaskSession metadata 仍未保存 execution owner；跨进程并发需要持久 revision/CAS。
- workspace-store Goal API 的 `String` 错误边界仍待类型化。
- 本轮没有提交、暂存或覆盖用户其他未提交变更。

## 第 129 轮：统一工具结构化错误构造与可信调用身份注入（2026-07-11）

### 问题发现与执行流程

- 第 128 轮允许工具通过 host kind `tool_error` 返回结构化错误，但工具主体只接收 args/session id，不拥有 Core 的 `tool_call_id`。
- 通用 `Err(String)` 路径由 bridge 构造 `tool_error.payload`，固定包含 tool_call_id、tool_name、status、error、arguments；close_agent 自有结构化错误则直接构造同一 kind，但缺少调用身份和参数，形成两套协议。
- WebUI 的错误摘要只读取 canonical `tool_error.payload.message/error/reason`，会依赖根层错误字段；任由工具自由定义 payload 会导致某些错误有详情但没有可展示摘要。
- RuntimeEvent 与 session projection 都以 managed bridge 输出的 Core host result 为边界；只有该层同时拥有可信 request identity 和工具自有详情，因此它是唯一合理的 canonicalization 所有者。
- 若工具直接返回保留的 canonical `tool_error` kind，bridge 无法判断其中 tool_call_id 是否真实；长期方案必须拒绝这种旁路，而不是用缺失字段补丁兼容。

### 核心修复与调整概述

- 在 tools-manager 定义预规范化 host kind `tool_error_detail`，明确它只承载工具领域错误，不承载调用身份。
- `VulcanToolInvocationResult` 新增 `from_tool_error_details(error, details)` 唯一构造器，固定生成 `status=failed`、非空错误文本与领域 details 信封。
- close_agent 改用统一构造器，不再手工拼接 host result kind 或重复维护模型/宿主两份错误载荷。
- managed bridge 严格验证 detail envelope 的 status、error 与 details，再注入 request 的 tool_call_id、tool_name、arguments，生成唯一 canonical `tool_error`。
- 工具若返回保留的 `tool_error` kind，bridge 明确报告协议违规；不信任或复用工具伪造的调用身份。
- malformed detail 会被转换为 canonical bridge failure，模型、RuntimeEvent 与 WebUI 仍得到结构完整且带可信身份的错误结果。

### 📂 文件变更清单

- 修改 `crates/vulcan-tools-manager/src/types.rs`：增加 error-detail kind 与统一结构化错误构造器。
- 修改 `crates/vulcan-tools-manager/src/lib.rs`：公开预规范化错误详情协议常量。
- 修改 `crates/vulcan-agent-runtime/src/agent_runtime/task_runtime/invocation_methods.rs`：close_agent 迁移到统一构造器。
- 修改 `crates/vulcan-agent-runtime/src/agent_runtime/tool_runtime_bridge.rs`：实现 canonical tool-error builder、严格 detail 校验与可信身份注入。
- 修改 `crates/vulcan-agent-runtime/src/agent_runtime/task_runtime/tests/persistence_store.rs`：适配统一 status/error/details envelope。
- 修改 `crates/vulcan-agent-runtime/src/agent_runtime/tool_runtime_bridge/tests.rs`：验证 canonical identity、领域详情保留与 malformed detail 拒绝。
- 修改 `TASK_LOG.md`：追加本轮执行记录。

### 💻 关键代码调整详情

- `from_tool_error_details` 同时生成模型可见载荷和 host-only detail，二者共享同一 JSON 快照，避免工具维护两份可能漂移的内容。
- `build_canonical_tool_error_host_result` 是 canonical `tool_error` 的唯一生产函数，所有根层调用身份均来自 `CoreToolInvocationRequest`。
- `canonicalize_tool_error_details` 要求 `status == failed`、`error` 为非空字符串且 `details` 字段存在；任一条件不满足都会产生明确协议错误。
- canonical payload 只把工具领域对象放入 `details`，不会把整个预规范化 envelope 再次嵌套，保持结构单层清晰。
- close_agent 的模型结果现在固定为根层 `status/error/details`，关闭阶段、回滚状态和 stopped task ids 均位于领域 details 内。

### 修改部分代码审核与自动修复

- 中断前初版仍让 close_agent 和测试直接构造 `tool_error_detail`；恢复审核后新增 tools-manager 构造器，生产代码不再接触协议常量。
- 初版 canonicalizer 只校验 error，并把整个 envelope 作为 details；审核后增加 status/details 必填校验并只提取领域 details，消除重复嵌套。
- 首次编译发现新常量仅定义在私有 types 模块；已通过 `lib.rs` 显式导出，避免跨 crate 私有路径或重复字符串常量。
- CodeKit 最终反查确认生产路径只有 close_agent 调用统一构造器；直接构造 `tool_error_detail` 只剩故意制造 malformed envelope 的协议测试。
- malformed 测试证明缺少 error 时不会静默降级或猜测其他字段，而会生成带真实 tool_call_id/tool_name 的 canonical failure。

### 验证结果

- `cargo test -p vulcan-tools-manager -p vulcan-agent-runtime --no-fail-fast`：4 个测试套件共 611 项全部通过。
- `cargo clippy -p vulcan-tools-manager -p vulcan-agent-runtime --all-targets -- -D warnings`：通过，零告警。
- `cargo test --workspace --no-fail-fast`：63 个测试套件共 1683 项全部通过。
- `cargo fmt --all -- --check` 与 `git diff --check`：通过。

### ⚠️ 遗留问题与下一轮方向

- `persist_archived_subagent_registry_entries` 已承担 archive transaction persistence 语义但名称仍描述旧输入形态，下一轮可完成语义命名收口并审核其返回类型所有权。
- `tool_error_detail` 当前第一个生产使用者是 close_agent；后续工具需要结构化失败时必须复用构造器，不能直接构造保留 kind。
- 多文件 `save_task_session` 仍依赖运行期补偿而非崩溃恢复协议；WAL/manifest transaction 属于存储架构重大调整，未经确认不实施。
- 持久 TaskSession metadata 仍未保存 execution owner；跨进程并发需要持久 revision/CAS。
- workspace-store Goal API 的 `String` 错误边界仍待类型化。
- 本轮没有提交、暂存或覆盖用户其他未提交变更。

## 第 130 轮：收口 close-agent 持久化提交所有权与精确回滚存储（2026-07-11）

### 问题发现与执行流程

- `persist_archived_subagent_registry_entries` 实际执行的是完整 archive persistence transaction，但名称仍把它描述为“持久化 registry entries”，掩盖了批量快照、补偿和提交语义。
- 成功返回值只是裸 `Vec<TaskSessionMetadata>`；调用方必须自行记住它是 rollback snapshots，并另外保存 root session id。
- runtime archive commit 失败时，旧代码通过 root id 再次查询当前 store root，然后用裸快照回滚；如果 persistence 与 rollback 之间 root 配置被更新，补偿会落到不同目录。
- rollback 方法挂在 `TaskRuntime` 上但完全不使用 `self` 除了二次查 store，说明事务资源所有权归属错误。
- commit 类型若可 Clone 或 rollback 接收 `&self`，同一批补偿资源可以被重复执行；类型层没有表达“一次提交最多消费一次回滚”的约束。

### 核心修复与调整概述

- 将 persistence 入口改名为 `persist_close_agent_archive_transaction`，明确它提交的是 close-agent 归档事务。
- 新增 `CloseAgentArchivePersistenceCommit`，成功提交同时持有本次实际使用的 `SegmentedTaskSessionStore` 与原始 metadata snapshots。
- runtime commit 失败时直接消费 `persistence_commit.rollback()`，不再传 root id、裸数组或重新查询 store registry。
- rollback 改为 commit 的固有方法并消费 `self`；类型层阻止同一 commit 被正常调用路径重复回滚。
- commit 取消 Clone，内部字段降为 task-runtime 私有，调用方不能拆散或重新组合 store 与 snapshots。
- archive prepare/runtime commit 方法分别重命名为 `prepare_close_agent_archive_entries` 与 `commit_close_agent_runtime_archive`，三阶段名称统一表达同一个事务。

### 📂 文件变更清单

- 修改 `crates/vulcan-agent-runtime/src/agent_runtime/task_runtime.rs`：增加私有 persistence commit 类型并收紧 close-agent 类型可见性。
- 修改 `crates/vulcan-agent-runtime/src/agent_runtime/task_runtime/persistence_methods.rs`：事务入口改名、返回 commit 对象，并把 rollback 迁移为 commit 固有行为。
- 修改 `crates/vulcan-agent-runtime/src/agent_runtime/task_runtime/invocation_methods.rs`：使用 prepare/persist/runtime commit 语义命名并消费 persistence commit 回滚。
- 修改 `crates/vulcan-agent-runtime/src/agent_runtime/task_runtime/tests/persistence_store.rs`：新增 store reconfiguration 精确回滚测试。
- 修改 `TASK_LOG.md`：追加本轮执行记录。

### 💻 关键代码调整详情

- `persist_close_agent_archive_transaction` 在没有配置 store 时返回无 store、无 snapshots 的有效 commit；有 store 时把实际 clone 与原始快照绑定为单一对象。
- `CloseAgentArchivePersistenceCommit::rollback(self)` 直接使用持有的 store，逆序恢复绑定快照，不访问可变的 runtime store-root registry。
- persistence failure 仍在函数内部立即补偿 touched snapshots；只有 persistence 全部成功后才产生 commit，类型上区分“提交成功，可供后续回滚”和“提交失败，已尝试内部补偿”。
- close-agent invocation 不再出现 `original_metadata` 裸变量，runtime failure 分支只能调用 commit 的 rollback。
- close-agent 事务相关 enum、failure、commit 从 `agent_runtime` 可见降为 task-runtime 私有，仅子模块共享。

### 修改部分代码审核与自动修复

- 初版只计划重命名函数与包装 Vec；执行流追踪发现 rollback 会二次查询 store，扩大为精确存储所有权修复。
- 初版 rollback 仍是 `TaskRuntime` 方法且接收 commit 引用；审核确认 `self` 已无用途后，将行为迁移到 commit 并消费对象。
- 初版 commit 派生 Clone；审核发现这会弱化一次性事务语义，已删除 Clone 并收紧字段可见性。
- 新回归先在原 store 提交 Archived，再把 runtime root 配置切换到替换目录，最后消费 commit rollback；原 store 恢复 Stopped/user_interrupt，替换 store 保持无任务。
- CodeKit 最终反查确认四个旧阶段函数名均零命中；persist 只返回 commit，rollback 只由 commit 自身提供。

### 验证结果

- 精确 rollback store 定向回归：1 项通过。
- `cargo test -p vulcan-agent-runtime --no-fail-fast`：2 个测试套件共 600 项全部通过。
- `cargo clippy -p vulcan-agent-runtime --all-targets -- -D warnings`：通过，零告警。
- `cargo test --workspace --no-fail-fast`：63 个测试套件共 1684 项全部通过。
- `cargo fmt --all -- --check` 与 `git diff --check`：通过。

### ⚠️ 遗留问题与下一轮方向

- persistence commit 当前持有 store clone 与完整 metadata snapshots，能够处理运行时失败但不处理进程崩溃；崩溃原子性仍需 WAL/manifest transaction，属于需确认的存储架构调整。
- `CloseAgentArchivePersistenceFailure` 与 rollback error 仍分别使用结构体和 String；下一轮可审计是否应统一为一个带阶段、受影响任务与 rollback 状态的内部错误类型。
- `tool_error_detail` 当前第一个生产使用者是 close_agent；后续工具需要结构化失败时必须复用构造器，不能直接构造保留 kind。
- 持久 TaskSession metadata 仍未保存 execution owner；跨进程并发需要持久 revision/CAS。
- workspace-store Goal API 的 `String` 错误边界仍待类型化。
- 本轮没有提交、暂存或覆盖用户其他未提交变更。


## 第 131 轮：统一 close-agent 阶段错误与回滚结果模型（2026-07-11）

### 问题发现与执行流程

- persistence failure 使用 `CloseAgentArchivePersistenceFailure`，prepare failure 仍是 String，runtime commit failure 又在 invocation 中手工拼接字符串，三个阶段没有共享错误事实模型。
- rollback 原来返回 `Result<(), String>`，调用方自行把 Ok 映射成 completed、Err 映射成 incomplete；状态和错误文本可被错误组合。
- persistence save 与 rollback 同时失败时，旧代码把两条错误拼成一个 message；协议只能展示混合文本，无法单独读取主阶段错误与 rollback error。
- persistence commit 没有 store 或 store 存在但 snapshots 为空时，rollback 返回 Ok；runtime commit failure 因而被标成 completed，实际语义应为 not_required。
- `failure_phase` 仍由调用点手写字符串，新增阶段或重命名时可以和类型/测试漂移。

### 核心修复与调整概述

- 新增 `CloseAgentArchiveFailurePhase`，类型化表示 prepare_archive、persist_archive 与 commit_runtime_archive，并由 `as_str` 唯一投影协议值。
- 将 rollback status 升级为 `CloseAgentArchiveRollbackOutcome`；Incomplete 变体自身持有聚合 rollback error，状态与错误不可拆散。
- 删除 persistence 专用 failure，统一使用 `CloseAgentArchiveFailure { phase, error, rollback }` 表示全部 stop 后归档失败。
- persistence、prepare 与 runtime commit 三条失败路径都构造同一个 failure 类型，再由单一结果 builder 投影。
- 主阶段错误保留在 tool error 根层 `error`；仅 incomplete rollback 在 details 中增加独立 `rollback_error`。
- persistence commit rollback 直接返回 outcome；无 store 或零 snapshots 时返回 not_required，有写入且全恢复才返回 completed。

### 📂 文件变更清单

- 修改 `crates/vulcan-agent-runtime/src/agent_runtime/task_runtime.rs`：建立统一 phase、rollback outcome 与 archive failure 类型。
- 修改 `crates/vulcan-agent-runtime/src/agent_runtime/task_runtime/persistence_methods.rs`：所有 persistence 错误迁移到统一 failure，rollback 返回类型化 outcome。
- 修改 `crates/vulcan-agent-runtime/src/agent_runtime/task_runtime/invocation_methods.rs`：删除手写 phase/status/string 拼接，统一消费 failure。
- 修改 `crates/vulcan-agent-runtime/src/agent_runtime/task_runtime/tests/persistence_store.rs`：验证主错误与 rollback error 分离、无写入 not_required 语义。
- 修改 `TASK_LOG.md`：追加本轮执行记录。

### 💻 关键代码调整详情

- `CloseAgentArchiveFailurePhase::as_str` 是 failure phase 的唯一协议投影，调用点不再传字符串。
- `CloseAgentArchiveRollbackOutcome::as_str` 投影 not_required/completed/incomplete；`error()` 只对 Incomplete 返回聚合错误。
- persistence save failure 的 primary error 只描述 save；snapshot restore 的聚合错误保存在 Incomplete outcome，不再拼接到主错误。
- `build_close_agent_archive_failure_result` 接收完整 failure 对象，统一生成 phase、rollback status 与可选 rollback_error。
- `CloseAgentArchivePersistenceCommit::rollback(self)` 对 store=None 和 snapshots 为空分别返回 NotRequired，只有真实恢复动作成功后返回 Completed。

### 修改部分代码审核与自动修复

- 初始计划只是把 rollback String 包成结构体；执行流审核发现 no-store Ok 被误映射成 completed，因此改为能表达 not_required 的 outcome 枚举。
- 初版 incomplete failure 继续把 save/rollback 错误合并；审核后把 primary error 与 rollback error 分离，WebUI 摘要保持主错误，诊断仍可读取完整补偿失败。
- 回归新增未配置 store 与已配置空 store 两条分支，均证明 rollback outcome 为 NotRequired。
- 故障级联回归增加断言：根层 error 不含 rollback 文本，details.rollback_error 单独包含失败 task id。
- CodeKit 最终反查确认旧 `CloseAgentArchiveRollbackStatus`、`CloseAgentArchivePersistenceFailure` 与 `rollback_status` 全部零命中。

### 验证结果

- close-agent 定向测试：6 项全部通过。
- `cargo clippy -p vulcan-agent-runtime --all-targets -- -D warnings`：通过，零告警。
- `cargo test --workspace --no-fail-fast`：63 个测试套件共 1685 项全部通过。
- `cargo fmt --all -- --check` 与 `git diff --check`：通过。

### ⚠️ 遗留问题与下一轮方向

- 统一 failure 目前没有携带受影响 task ids，这些 ids 仍由 invocation 单独传给协议 builder；下一轮可审计是否应把 stopped/archive target 集合纳入 close transaction context，进一步避免并行数组漂移。
- persistence commit 能处理运行时失败但不处理进程崩溃；崩溃原子性仍需 WAL/manifest transaction，属于需确认的存储架构调整。
- `tool_error_detail` 当前第一个生产使用者是 close_agent；后续工具需要结构化失败时必须复用构造器，不能直接构造保留 kind。
- 持久 TaskSession metadata 仍未保存 execution owner；跨进程并发需要持久 revision/CAS。
- workspace-store Goal API 的 `String` 错误边界仍待类型化。
- 本轮没有提交、暂存或覆盖用户其他未提交变更。

## 第 132 轮：统一 close-agent 子树范围并删除孤儿级联模式（2026-07-11）

### 问题发现与执行流程

- close_agent 原先无条件调用 `stop_task_tree(target)`，随后才根据 `cascade` 决定归档目标还是完整子树；`cascade=false` 因而会停止后代但保持其 registry open。
- 修正为“非级联只停止目标”后继续追踪发现，该语义仍不可用：后代的 direct parent 是已归档父会话，根会话虽然可能从 list_agents 看到它，却会被 send_message 的 direct-parent 校验拒绝。
- 保留的活动后代最终只能向已关闭父会话投递结果，成为无法由根会话控制的孤儿运行；不存在可靠的 reparent 协议或会话所有权迁移链路。
- stopped task ids、archive entries、archive task ids、target、previous status 和 cascade 分散在多个变量及函数参数中；持久化、运行时提交和结果投影可能使用不同集合。
- archive task ids 每次从 entries 重复派生，failure builder 只能接收 stopped ids，无法报告实际待归档任务集合。

### 核心修复与调整概述

- 删除 close_agent 的 `cascade` 参数与解析函数；close_agent 现在始终关闭完整子树，不再支持制造孤儿后代的模式。
- tool description 与 schema 明确只接受 subtree target；旧 cascade 参数会在任何停止或归档副作用前被拒绝。
- 新增 `PreparedCloseAgentArchive`，一次性绑定权威 archived entries 与由其派生的 task ids。
- 新增 `CloseAgentTransaction`，统一拥有 parent、target、previous status、实际 stopped ids 和 prepared archive。
- prepare archive 改到停止之前；prepare 失败时没有任何 stop side effect，因此继续返回普通参数/运行时错误，不虚报 stopped=true。
- 新增 `stop_selected_tasks`，仅停止 owning transaction 给出的精确 task ids；`stop_task_tree` 自身也复用该 primitive。
- persistence、runtime commit、inbox clear、成功结果和失败结果全部消费同一 prepared archive/transaction。

### 📂 文件变更清单

- 修改 `crates/vulcan-agent-runtime/src/agent_runtime/task_runtime.rs`：增加 PreparedCloseAgentArchive 与完整 CloseAgentTransaction。
- 修改 `crates/vulcan-agent-runtime/src/agent_runtime/task_runtime/lifecycle_methods.rs`：抽取精确范围的 stop_selected_tasks。
- 修改 `crates/vulcan-agent-runtime/src/agent_runtime/task_runtime/invocation_methods.rs`：prepare 前移，统一事务消费并删除 cascade 分支。
- 修改 `crates/vulcan-agent-runtime/src/agent_runtime/task_runtime/persistence_methods.rs`：持久化入口直接接收 PreparedCloseAgentArchive。
- 修改 `crates/vulcan-agent-runtime/src/agent_runtime/task_runtime/tool_surface.rs`：删除 cascade schema/parser，更新工具说明。
- 修改 `crates/vulcan-agent-runtime/src/agent_runtime/task_runtime/tests/tool_args.rs`：验证 schema 只包含 target 且旧 cascade 被拒绝。
- 修改 `crates/vulcan-agent-runtime/src/agent_runtime/task_runtime/tests/registry_mailbox.rs`：验证拒绝无副作用及合法请求完整关闭活动子树。
- 修改 `crates/vulcan-agent-runtime/src/agent_runtime/task_runtime/tests/persistence_store.rs`：failure 验证精确 archive task ids，并更新子树事务命名。
- 修改 `TASK_LOG.md`：追加本轮执行记录。

### 💻 关键代码调整详情

- `PreparedCloseAgentArchive::new(entries)` 只派生一次 task ids，类型内保证 entries/task ids 不会独立漂移；类型不可 Clone。
- `CloseAgentTransaction` 把请求事实、实际停止集合与 prepared archive 合并，成功/失败结果不再接收并行参数数组。
- `prepare_close_agent_archive_entries` 始终沿 registry parent_session_id 收集完整子树，并在停止前生成统一 Archived/close_agent 时间快照。
- `stop_selected_tasks` 对 prepared task ids 中真实存在 active run 的项设置 stop_requested、abort handle，并返回实际 stopped ids。
- failure details 新增 archive_task_ids/archive_task_count；stopped ids 与待归档 ids 可分别诊断。
- close success 的 registry update、session update、execution transaction locks 和 inbox clear 都从同一个 prepared archive 读取范围。

### 修改部分代码审核与自动修复

- 初版准备保留 cascade=false 并修正停止范围；调用链审核证明后代会成为 direct-parent 不可达孤儿，依据长期无兼容目标删除该坏模式。
- 首轮返回类型补丁锚点过宽，误改 `open_subagent_registry_entries_for_parent_tree`；编译器发现后已恢复原 Vec 返回，并精确修改 prepare 函数。
- 非级联回归最初用 `is_task_running` 判断停止；源码确认该 API 只检查 active-run entry 是否常驻，不检查 stop_requested，已改为直接验证父子 stop_requested 事实。
- 最终回归先提交带 cascade=false 的旧请求，证明父子 stop_requested、registry、inbox 全无变化；随后合法 close 证明父子均停止、归档且 inbox 清空。
- CodeKit 最终反查确认生产代码中 close_agent cascade 解析、字段与条件分支全部零命中；stop_task_tree 只剩 interrupt/send_message 等明确整树停止调用。

### 验证结果

- `cargo test -p vulcan-agent-runtime --no-fail-fast`：2 个测试套件共 602 项全部通过。
- `cargo clippy -p vulcan-agent-runtime --all-targets -- -D warnings`：通过，零告警。
- `cargo test --workspace --no-fail-fast`：63 个测试套件共 1686 项全部通过。
- `cargo fmt --all -- --check` 与 `git diff --check`：通过。

### ⚠️ 遗留问题与下一轮方向

- `subagent_registry_close_agent_ids` 仍通过重复全表扫描扩展 session id 集合，复杂度高且依赖后续 get 二次读取；下一轮可审计 registry 是否已有子树索引/API，并把树遍历所有权下沉到 SubagentRegistry。
- persistence commit 能处理运行时失败但不处理进程崩溃；崩溃原子性仍需 WAL/manifest transaction，属于需确认的存储架构调整。
- `tool_error_detail` 当前第一个生产使用者是 close_agent；后续工具需要结构化失败时必须复用构造器，不能直接构造保留 kind。
- 持久 TaskSession metadata 仍未保存 execution owner；跨进程并发需要持久 revision/CAS。
- workspace-store Goal API 的 `String` 错误边界仍待类型化。
- 本轮没有提交、暂存或覆盖用户其他未提交变更。

## 第 133 轮：下沉 SubagentRegistry 子树遍历并消除重复全表扫描（2026-07-11）

### 问题发现与执行流程

- close-agent 的 `subagent_registry_close_agent_ids` 位于 TaskRuntime，通过循环 `list_all()` 扩展 session id 集合；树深每增加一层都会重新扫描完整 registry。
- 收集完 agent ids 后，prepare 阶段又逐 id 调用 registry.get，形成重复查找和两阶段读取。
- SubagentRegistry 只维护按 agent id 的 BTreeMap，没有 parent/session 二级索引，也没有权威 subtree API；树关系解释因此泄漏到 TaskRuntime。
- 直接增加持久 parent index 会要求 upsert、remove、Clone、Eq、Serialize/Deserialize 全部维护双结构，低频 close 查询不值得引入可漂移状态。
- registry 数据可能因损坏包含父级环路或重复 session id；新遍历必须显式终止环路，不能依赖树结构永远合法。

### 核心修复与调整概述

- 在 SubagentRegistry 新增 `list_subtree_from_session`，注册表自身成为 parent/session 树关系的唯一查询所有者。
- API 单次扫描 entries，同时构建临时 entries-by-session 与 children-by-parent 邻接表，再进行广度优先遍历。
- 使用 visited session id 集合终止自环和多节点父级环路，避免损坏数据导致无限循环或重复条目。
- 根 session 不存在时明确返回空集合，不把仅 parent 指向缺失根的孤儿条目误认成有效子树。
- close-agent prepare 直接遍历返回的 entry references、过滤 Archived 并克隆成 prepared projections；删除私有 id 收集 helper 与二次 get。

### 📂 文件变更清单

- 修改 `crates/vulcan-session-core/src/subagent_registry.rs`：增加权威 subtree API、临时邻接遍历与复杂树回归测试。
- 修改 `crates/vulcan-agent-runtime/src/agent_runtime/task_runtime/invocation_methods.rs`：close-agent 迁移到 registry subtree API，删除重复扫描 helper。
- 修改 `TASK_LOG.md`：追加本轮执行记录。

### 💻 关键代码调整详情

- `list_subtree_from_session(root_session_id)` 在一次 BTreeMap values 扫描中构建 session→entries 和 parent→children 两张临时表，查询复杂度从 O(深度×节点数) 收口为 O(节点数 log 节点数)。
- BFS 先输出根 session 对应条目，再按 agent-id 稳定顺序输出同层子节点，结果顺序可预测。
- entries-by-session 使用 Vec 而非假设 session id 唯一；损坏数据存在重复 session 时仍返回所有真实 registry entries。
- visited_session_ids 只允许每个 session 扩展一次；父级环路仍返回可达条目，但不会重复或无限执行。
- TaskRuntime 不再自行解释 parent_session_id 闭包，只负责把权威 subtree entries 转换为 Archived/close_agent 投影。

### 修改部分代码审核与自动修复

- 初始候选是给 SubagentRegistry 维护永久 children index；审核其序列化与更新成本后，选择查询时临时邻接表，避免第二事实源。
- 审核确认仅使用 children-by-parent 无法判断根是否真实存在；新增 entries-by-session 根存在检查，缺失根返回空。
- 审核确认按 session id 建单值 map 会静默丢弃重复条目；改为 Vec 保存全部条目，并由 visited 只控制遍历扩展而非结果去重。
- 新增四层混合树测试，证明只返回目标根、两个子代和孙代，排除同父无关 sibling；另新增双节点父级环路测试证明终止且不重复。
- CodeKit 最终反查确认 `subagent_registry_close_agent_ids` 零命中，生产调用只剩 close-agent prepare 使用权威 registry API。

### 验证结果

- session-core registry 定向测试：6 项全部通过。
- close-agent 定向测试：7 项全部通过。
- `cargo clippy -p vulcan-session-core -p vulcan-agent-runtime --all-targets -- -D warnings`：通过，零告警。
- `cargo test --workspace --no-fail-fast`：63 个测试套件共 1688 项全部通过。
- `cargo fmt --all -- --check` 与 `git diff --check`：通过。

### ⚠️ 遗留问题与下一轮方向

- list_agents 仍在 TaskRuntime 构建 entries_by_session，并逐 entry 调用 `subagent_entry_tree_path` 回溯父链；大树下存在重复路径计算，下一轮可把路径投影所有权一并下沉到 SubagentRegistry。
- persistence commit 能处理运行时失败但不处理进程崩溃；崩溃原子性仍需 WAL/manifest transaction，属于需确认的存储架构调整。
- `tool_error_detail` 当前第一个生产使用者是 close_agent；后续工具需要结构化失败时必须复用构造器，不能直接构造保留 kind。
- 持久 TaskSession metadata 仍未保存 execution owner；跨进程并发需要持久 revision/CAS。
- workspace-store Goal API 的 `String` 错误边界仍待类型化。
- 本轮没有提交、暂存或覆盖用户其他未提交变更。

## 第 134 轮：统一注册表树索引与 list-agents 路径投影（2026-07-11）

### 问题发现与执行流程

- `list_agents` 先克隆完整注册表为 `session_id -> entry`，再对每个可见条目调用 `subagent_entry_tree_path` 沿父链逐级回溯；树越深，相同祖先链被重复解析越多。
- 路径构建、根归属判断和环路终止都位于 TaskRuntime，而上一轮 close-agent 子树遍历已经下沉到 `SubagentRegistry`；同一份 parent/session 关系存在两套解释。
- 旧单值 `BTreeMap` 会在重复 session id 时静默覆盖一个 agent；缺父节点或父级环路则由 `Option::None` 静默丢弃，调用方无法区分“不匹配”和“注册表损坏”。
- 正常执行顺序已确认：解析 `path_prefix/include_closed` → 获取 registry 快照与根 session → 收集常驻 session → 计算完整树路径 → 按运行态/归档态过滤 → 按路径前缀过滤 → 使用 `(path, agent_id)` 字典序输出。
- close-agent 的执行顺序是：快照选择目标 → 重新读取 registry 准备归档子树 → 停止任务 → 持久化 → 提交运行时归档；准备阶段若把缺失根解释为空集合，会产生 `closed=true/closed_count=0` 的假成功事务。
- 注册表可同时保存多个主会话根树；父 session 不在 registry 中既可能是合法外部主会话根，也可能是真正孤儿，仅凭现有字段无法唯一判断，因此不能投机性地把所有缺父条目判错。

### 核心修复与调整概述

- 新增共享的临时 `SubagentRegistryTreeIndex`，一次扫描构建唯一 session 索引与 parent→children 邻接表，并显式按 agent id 排序同级节点。
- 新增 `SubagentRegistryTreeError`，明确区分重复 session 归属、缺失子树根和注册表父级环路；错误通过 session-core 公共入口导出。
- 在索引构建阶段校验注册表范围的全部父链：每条链只能终止于外部根边界、已验证链或本链环路；环路现在全局显式失败，不再只检查当前可达分支。
- 新增 `list_tree_paths_from_root`，由 registry 统一生成 `/root/...` 路径并保持旧 `(path, agent_id)` 字典序。
- `list_subtree_from_session` 复用同一索引，契约收紧为确定性 BFS：根优先、同级按 agent id；缺失根不再返回空集合，而是显式失败。
- `list_agents` 改为直接消费 registry 路径投影并删除 `subagent_entry_tree_path`；状态过滤与 path-prefix 语义保持不变。
- close-agent 准备阶段传播树结构错误，并在任何任务停止之前拒绝缺失根，消除空归档假成功。

### 📂 文件变更清单

- 修改 `crates/vulcan-session-core/src/subagent_registry.rs`：新增结构错误、共享树索引、全局父链校验、路径投影 API、严格子树 API 与回归测试。
- 修改 `crates/vulcan-session-core/src/lib.rs`：导出 `SubagentRegistryTreeError`。
- 修改 `crates/vulcan-agent-runtime/src/agent_runtime/task_runtime/invocation_methods.rs`：迁移 list-agents 路径读取，传播 close-agent 子树错误。
- 修改 `crates/vulcan-agent-runtime/src/agent_runtime/task_runtime/tool_surface.rs`：删除 TaskRuntime 私有父链回溯函数。
- 修改 `crates/vulcan-agent-runtime/src/agent_runtime/task_runtime/tests/registry_mailbox.rs`：增加重复 session 与 close 缺根集成回归。
- 修改 `TASK_LOG.md`：追加本轮执行记录。

### 💻 关键代码调整详情

- `tree_index()` 先按 agent-id 有序源数据建立 `entries_by_session`；同一 session 被第二个 agent 声明时立即返回包含两个稳定 agent id 的确定性错误。
- `validate_parent_chains()` 使用全局已验证集合与当前链集合迭代追踪唯一父指针，每个节点最多进入一次完整链，总体为 O(n log n)、空间 O(n)，且不存在深树递归栈风险。
- registry 外父节点只作为外部主会话根边界终止校验；合法多根森林分别投影，另一根树不会泄漏到当前结果。
- `list_tree_paths_from_root()` 从选定外部根执行 BFS，再统一按旧 comparator 排序；先排序再过滤仍得到与旧“先过滤再排序”相同的有序子序列。
- `list_subtree_from_session()` 先严格解析根 entry，再沿已排序 children 执行 BFS；全局结构校验已证明无环，因此删除投影层重复 visited/cycle 分支。
- `subagent_registry_entries_for_list_request()` 只负责常驻/归档可见性和 path-prefix 过滤，不再拥有树路径算法。
- `prepare_close_agent_archive_entries()` 对 `MissingRootSession` 使用 `?` 立即返回；`stop_selected_tasks` 位于其后，故失败不会产生 stop、持久化或归档副作用。

### 修改部分代码审核与自动修复

- 初版只把重复 session 和当前遍历可达环改为显式错误，仍保留“缺失子树根返回空集合”；独立审查沿 close-agent 事务发现目标快照被替换时可能形成空事务假成功，已新增 `MissingRootSession` 并补准备阶段回归。
- 审查指出“重复 session 全局拒绝、环路仅局部拒绝”策略不一致；已把父链检查收口到索引构建阶段，任何根投影前都验证完整 registry 森林。
- 审查指出 BFS 同级顺序依赖底层 BTreeMap 的隐式迭代顺序；已显式排序每个 children 列表，并用逆序插入测试固定契约。
- 环路测试最初允许两个节点任一成为错误锚点，未证明诊断确定性；已改用有序固定 UUID，精确断言首个 session/agent，并同步更新为 registry-wide 注释。
- 增加双外部根双向投影断言，证明合法多根互不泄漏；同时保留“缺父无法据现有字段判孤儿”的事实边界，没有添加候选式推断。
- CodeKit 最终反查确认旧 `subagent_entry_tree_path` 与 `ReachableCycle` 零命中，生产树查询只剩 registry 两个权威 API。

### 验证结果

- session-core registry 定向测试：7 项全部通过。
- list-agents 定向测试：10 项全部通过。
- close-agent 定向测试：9 项全部通过。
- `cargo clippy -p vulcan-session-core -p vulcan-agent-runtime --all-targets -- -D warnings`：通过，零告警。
- `cargo test --workspace --no-fail-fast`：63 个测试套件共 1691 项全部通过。
- `cargo fmt --all -- --check` 与 `git diff --check`：通过。

### ⚠️ 遗留问题与下一轮方向

- `SubagentRegistry::upsert` 仍只按 agent id 写入，重复 session 或父级环路可以先驻留到内存，直到树查询时才失败；下一轮可沿所有写入/恢复调用链设计写边界校验，避免损坏状态延迟暴露。
- registry entry 尚未保存可信 `root_session_id`，因此无法仅凭缺父节点区分合法外部根与孤儿；若要严格识别 orphan，必须先补充唯一根归属事实，不能轮询候选来源。
- persistence commit 能处理运行时失败但不处理进程崩溃；崩溃原子性仍需 WAL/manifest transaction，属于需确认的存储架构调整。
- 持久 TaskSession metadata 仍未保存 execution owner；跨进程并发需要持久 revision/CAS。
- workspace-store Goal API 的 `String` 错误边界仍待类型化。
- 本轮没有提交、暂存或覆盖用户其他未提交变更。

## 第 135 轮：收紧子代理身份写边界并统一归档/恢复事务（2026-07-11）

### 问题发现与执行流程

- 上一轮已经把树查询统一到 `SubagentRegistry`，但写入仍以全局 `agent_id` 为唯一槽位：同一直接父级下可以出现多个相同公开任务名，不同父级又无法安全复用相同任务名，读取方只能依赖全局名称碰运气定位。
- 旧 `upsert` 可以覆盖既有条目，缺少 session 全局唯一、父级/任务名槽位唯一以及 Archived 吸收态约束；过期的运行结果、interrupt 或消息恢复路径都可能在归档后重新写入运行态。
- spawn/恢复/测试夹具分别直接修改 registry，写入边界分散；树结构错误常在 list/close 查询时才暴露，无法保证“进入 registry 的状态已经合法”。
- close-agent 原流程是“准备快照 → 停止任务 → 写持久化归档 → 修改运行时 registry → 清理 inbox”，各阶段没有共享同一组 task execution locks；后台完成结果可夹在阶段之间，并被旧快照覆盖最终摘要、片段、最后任务消息或归档时间。
- send-message 与 interrupt 先解析任务、随后再执行动作；在解析与写入之间，目标槽位可能已被归档或由新 session 复用，导致消息投递给过期 owner、归档 inbox 被重新排队或 stale interrupt 修改终态。
- 持久恢复原链路先读元数据，再逐项物化和写入；元数据集合可能在加锁前变化，部分磁盘写入失败时也必须同时证明运行时没有半提交、此前写入可以回滚。
- 持久层按公开任务名查找时未把“唯一开放 owner”和“归档历史”分层；同父同名的多个开放记录必须显式报错，不能选取迭代顺序中的任意一条。
- 已完整确认主链：任务准备创建 session/registry → execution claim → 后台结果提交；list/send/interrupt/close 按 parent/name 解析同一 owner；close 同时变更执行态、持久元数据、registry 与 inbox；进程恢复则从根元数据遍历开放子树、物化上下文/片段后提交驻留状态。

### 核心修复与调整概述

- 将 `SubagentRegistry` 改为 `parent_session_id -> public_task_id -> entry` 的父级作用域结构；公开任务名只在直接父级内唯一，同时继续强制真实 session id 全局唯一。
- 把 registry 写入改为返回显式错误的严格契约：拒绝重复 session、占用槽位、Archived 重开、开放后代覆盖、缺失精确槽位和批量重复槽位；Archived 成为不可逆吸收态。
- 允许父级作用域内归档名称复用，但只有旧子树全部归档时才原子清理旧子树；存在开放后代时拒绝复用，避免制造失去父级的孤儿。
- 删除生产链对全局名称查找的依赖，统一改用 `get_for_parent` 与精确 session 身份；公开 `task_name` 同步拒绝 `/`，工具 schema 固定 `minLength=1` 与 `^[^/]+$`。
- close-agent 在稳定排序、去重后的整棵目标子树 execution locks 内完成运行停止、时间重标、持久归档、运行时归档和 inbox 清理；运行时提交失败时在同一锁域回滚持久状态。
- close 的最终 registry 投影重新读取锁内最新条目，只覆盖 Archived 生命周期字段，保留后台刚完成的摘要、片段和最后任务消息；归档时间取最新状态与准备时间的最大值。
- send-message 在目标 task lock 内重新校验 parent/name 槽位仍由同一 session 占有，再入队并判断是否需要唤醒；失败恢复只允许向仍开放的精确 session 回排，Archived/缺失目标显式丢弃并报恢复错误。
- interrupt 在目标 task lock 内重新读取精确 owner；execution claim、结果状态提交和 stopped 投影都先检查 Archived，过期执行不能重开归档任务。
- 恢复流程改为两阶段事务：读取开放树 → 按根及任务 session id 稳定加锁 → 锁内重读元数据集合，集合变化则释放并重试 → 完整物化 → clone 预演 → 磁盘写入/失败回滚 → registry 与 session clone-swap。
- 恢复时明确校验 resident registry/session 的身份、深度和生命周期一致性；resident 生命周期为权威事实，持久化 Running/Open 可由驻留 Archived 自愈，运行时丢失的开放任务统一归一为 Stopped/runtime_lost。
- 持久层公开任务查找改为：存在多个开放 owner 时显式失败；唯一开放 owner 优先于任意数量归档历史；不存在开放 owner 时才处理归档记录。

### 📂 文件变更清单

- 修改 `crates/vulcan-session-core/src/subagent_registry.rs`：重构父级作用域存储、严格写入/替换/删除契约、Archived 吸收态、子树复用校验及 15 项注册表回归测试。
- 修改 `crates/vulcan-session-core/src/lib.rs`：更新严格 registry 错误类型的公共导出。
- 修改 `crates/vulcan-agent-runtime/src/agent_runtime/task_runtime.rs`：增加两阶段恢复物化结构。
- 修改 `crates/vulcan-agent-runtime/src/agent_runtime/task_runtime/arguments.rs`：收紧公开任务名规范化契约。
- 修改 `crates/vulcan-agent-runtime/src/agent_runtime/task_runtime/agent_control.rs`：迁移父级作用域精确查询。
- 修改 `crates/vulcan-agent-runtime/src/agent_runtime/task_runtime/invocation_methods.rs`：重构 close/send/interrupt 的锁内重验与事务提交。
- 修改 `crates/vulcan-agent-runtime/src/agent_runtime/task_runtime/lifecycle_methods.rs`：增加稳定顺序多任务锁、锁内停止、Archived 防重开和精确 execution owner 约束。
- 修改 `crates/vulcan-agent-runtime/src/agent_runtime/task_runtime/persistence_methods.rs`：实现恢复锁内重读、完整物化、磁盘回滚和运行时 clone-swap。
- 修改 `crates/vulcan-agent-runtime/src/agent_runtime/task_runtime/request_records.rs`：同步父级作用域请求记录字段。
- 修改 `crates/vulcan-agent-runtime/src/agent_runtime/task_runtime/tool_surface.rs`：按 session/树路径隔离 list-agents 队列与宿主父级投影。
- 修改 `crates/vulcan-agent-runtime/src/agent_runtime/task_runtime/tests/persistence_store.rs`：补充归档吸收、嵌套恢复、重复槽位、物化失败、写入回滚、时间一致和驻留归档自愈测试。
- 修改 `crates/vulcan-agent-runtime/src/agent_runtime/task_runtime/tests/registry_mailbox.rs`：补充跨父同名隔离、消息队列计数、路径/父级投影及归档消息恢复测试。
- 修改 `crates/vulcan-agent-runtime/src/agent_runtime/task_runtime/tests/runtime_flow.rs`：补充并发任务准入、过期结果/claim/interrupt 与精确 owner 回归。
- 修改 `crates/vulcan-agent-runtime/src/agent_runtime/task_runtime/tests/tool_args.rs`：固定公开任务名 schema 约束。
- 修改 `crates/vulcan-task-session/src/segmented_store/traits.rs`：严格解析同父同名开放 owner。
- 修改 `crates/vulcan-task-session/src/lib.rs`：增加重复开放 owner 拒绝及开放记录优先归档历史测试。
- 修改 `TASK_LOG.md`：追加本轮执行记录。

### 💻 关键代码调整详情

- `SubagentRegistry::upsert` 先校验 session 全局归属、父级/name 槽位、现有 Archived 状态和旧子树开放节点；任何失败都在修改 map 前返回，成功后再执行全局树不变量验证。
- `replace_existing_entries` 对批量槽位先做重复检测，再在 clone 上应用并校验；失败恢复原 registry，避免逐条替换留下中间状态。
- `remove_exact_leaf` 只删除父级/name/session 三者完全匹配且无后代的叶子；不再提供模糊名称删除。
- `prepare_task_execution_request` 同时按稳定顺序锁定当前父 session 与新任务 session；并发创建同父同名任务只能有一个成功，失败分支回滚刚创建的 session record。
- `ordered_task_execution_state_locks` 统一排序和去重，close、restore 与准备任务共享锁顺序，避免多任务事务形成 ABBA 死锁。
- `commit_close_agent_runtime_archive_with_task_locks_held` 在锁内读取当前精确 entry，并只合并归档状态、最新时间和 stop reason；旧 prepared snapshot 不再覆盖最终业务结果。
- `resume_idle_agent_from_inbox` 使用既有精确 task/session 身份，不再通过公开名称创建候选 session；空 inbox 直接返回，不制造无意义恢复执行。
- `restore_inter_agent_messages_front_after_failure` 在 task lock 内读取 registry，只有精确 session 仍开放才恢复消息；归档或缺失 owner 不再把历史消息注入复用后的新任务。
- `restore_persisted_subagent_registry_for_parent_tree` 比较加锁前后开放元数据集合；发生变化只重试事实读取，不使用字段 fallback 猜测结构。
- 恢复 commit 先构造 registry/session clone，根据四种驻留组合显式处理：两者都在则校验并合并、仅 session 在则合法水合、仅 registry 在则报错、两者都不在则插入；磁盘全部保存成功后才交换运行时状态。
- 元数据写入保留每条原始快照；任意保存失败按已写入集合回滚，并保持运行时 clone 未提交，避免磁盘与内存各成功一半。

### 修改部分代码审核与自动修复

- 初版父级作用域 registry 仍允许过期 lifecycle 写入覆盖 Archived；审核后把 Archived 约束下沉到 registry、execution claim、结果提交、停止投影和消息失败恢复全部边界，形成单一吸收态协议。
- 审核发现 close prepared snapshot 可能抹掉锁前刚落地的最终摘要/片段/消息；改为锁内重读最新 entry 并只覆盖生命周期字段，增加 stale prepared snapshot 回归。
- 审核发现 stop 与归档时间在准备阶段固定会早于真实终止；增加锁内 restamp，并在 registry/session/disk 三层使用同一归一时间。
- 审核发现 send-message 在首次解析后仍有 owner 替换窗口；增加 task lock 内 exact owner 二次校验，并让失败回排同样受 Archived 状态保护。
- 审核发现 execution claim 可能先写 execution id 再发现 Archived；调整为先验证生命周期，再写 owner，测试固定归档状态与 owner 均不变化。
- 审核发现恢复只锁物化后 commit 仍存在“首次读取开放树后集合变化”的 TOCTOU；外层改为锁内重读集合，不一致时释放锁并重新走完整事实链。
- 审核发现 resident Archived 与磁盘 Running/Open 冲突时不能相信旧持久状态；明确 resident 生命周期权威并回写自愈元数据，新增归档不会被恢复重开的测试。
- 独立审查最终确认 close 锁序、过期结果/interrupt/claim、消息 owner 重验、恢复 clone-swap 与回滚链路无剩余 P0/P1；Clippy 与全量测试再次验证自动修复没有引入回归。

### 验证结果

- `cargo fmt --all -- --check`：通过。
- `cargo check -p vulcan-session-core -p vulcan-task-session -p vulcan-agent-runtime --all-targets`：通过。
- session-core registry 定向测试：15 项全部通过。
- task-session segmented-store 定向测试：4 项全部通过。
- agent-runtime persistence-store 定向测试：29 项全部通过。
- `cargo test -p vulcan-agent-runtime`：2 个测试套件共 615 项全部通过。
- `cargo clippy -p vulcan-session-core -p vulcan-task-session -p vulcan-agent-runtime --all-targets -- -D warnings`：通过，零告警。
- `cargo test --workspace`：63 个测试套件共 1712 项全部通过。
- `git diff --check`：通过。

### ⚠️ 遗留问题与下一轮方向

- 当前 registry entry 只记录 `parent_session_id`，无法区分“合法外部主会话根”和“本应存在但丢失的嵌套 TaskSession 父级”；`validate_parent_chains` 只能把所有 registry 外父级视为外部根。长期修复需要引入 `ParentSessionRef::{MainSession, TaskSession}` 或显式登记合法根，属于身份模型架构变更，必须先取得用户确认，不能用候选判断推测。
- persistence commit 能回滚普通运行时失败，但进程在多文件写入中途崩溃仍需要 WAL/manifest transaction；这是存储架构调整，仍需确认。
- 持久 TaskSession metadata 尚未保存 execution owner；跨进程并发需要持久 revision/CAS。
- workspace-store Goal API 的 `String` 错误边界仍待类型化。
- 本轮没有提交、暂存或覆盖用户其他未提交变更。

## 阶段收尾：第 57—135 轮治理统计与测试方案（2026-07-11）

### 统计口径与总量

- 统计范围为 `TASK_LOG.md` 第 57 轮起始行至第 135 轮末尾，共 79 轮、4,791 行治理记录。
- 其中 76 轮形成有效调整；第 58、64、66 轮共 3 次失败实验均已明确撤销或回退，不计为遗留实现。
- 时间分布：2026-07-10 完成第 57—84 轮，共 28 轮；2026-07-11 完成第 85—135 轮，共 51 轮。
- 文件变更章节共记录 660 条变更条目；按路径文本去重后涉及 217 个唯一文件/路径标识，其中 `apps/` 49 个、`crates/` 167 个、`TASK_LOG.md` 1 个。
- 第 57 轮开始前最近一次完整工作区测试记录为 1,566 项；第 135 轮最终为 1,712 项，累计净增加 146 项自动化测试。
- 最终 Rust 工作区仍为 63 个测试套件；Web 生产构建当前转换 2,363 个模块。

### 五个治理阶段

| 轮次 | 轮数 | 治理范围 | 核心结果 |
| --- | ---: | --- | --- |
| 57—75 | 19 | WebUI 会话切换、布局、滚动与工具调用展示 | 消除新会话转正闪烁；明确长转录阻塞事实；稳定输入区、右侧面板与 Git 工具框布局；限制自动滚动作用域；统一连续工具调用、运行态 shimmer 与 checkpoint 顺序。 |
| 76—84 | 9 | 智能体目录、能力策略与 sidecar 协议 | 删除旧平铺扫描；收口能力策略；拆分 sidecar 规范化/持久化；强制 schemaVersion；统一可移植路径、原子写入、显式错误和目录链接逃逸拒绝。 |
| 85—100 | 16 | 分段存储、运行时日志与规范消息身份 | 删除存在性预检和死 API；建立日志锁内事务、压缩备份、双向投影状态机与工具批次顺序校验；统一 ToolResult/消息主键；收口原子 mutation、追加、复放和本地身份接管。 |
| 101—117 | 17 | 运行时干预、事件投影与 Goal 生命周期 | 类型化在线/离线干预错误；把工作区草稿缩为会话补丁；严格 Goal 投影、文本/预算/状态写边界；阻断并发覆盖；绑定 execution identity、预算阶段、最终记账、清理、分发与回合准入。 |
| 118—135 | 18 | detached subagent、close-agent 与注册表事务 | 以 execution identity 隔离续跑和父回合；建立 active run、prepare/result/stop 事务；统一关闭状态、原因、结构化错误、持久回滚和子树范围；最终收紧 parent/name/session 身份、Archived 吸收态、消息/interrupt 并发与两阶段恢复。 |

### 累计工程结果

- 状态所有权从 UI 猜测、JSON 比较、全局名称和历史 payload，逐步收口到类型化身份、权威状态机、父级作用域索引及锁内事务。
- 失败路径从静默吞错、模糊 fallback 和局部写入，逐步收口到结构化错误、预演、显式拒绝、补偿回滚和运行时 clone-swap。
- 长生命周期流程形成一致协议：先确认唯一 owner，再按稳定顺序加锁，锁内重读事实，验证 lifecycle，再提交持久化和运行时投影。
- 兼容性代码只在事实证明仍有调用时保留；已确认无调用或破坏唯一事实源的旧扫描、死 API、身份回退和猜测分支均已删除。
- 第 135 轮独立代码审核未发现剩余 P0/P1；已知架构级事项作为明确交接项保留，不用候选路径或多重 fallback 掩盖。

### 分层测试方案

#### 第一层：每次提交的静态门禁

- `rtk cargo fmt --all -- --check`：Rust 格式必须零差异。
- `rtk cargo check --workspace --all-targets`：全部 workspace target 必须编译通过。
- `rtk cargo clippy --workspace --all-targets -- -D warnings`：Clippy 必须零告警。
- `rtk npm --prefix apps/vulcan-code-web run lint`：Web TypeScript 严格检查必须通过。
- `rtk npm --prefix apps/vulcan-code-web run build`：Vite 生产构建必须成功；现有大 chunk 提示单独跟踪，不作为本阶段功能失败。
- `rtk git diff --check`：禁止新增空白错误和冲突标记。

#### 第二层：自动化回归

- `rtk cargo test --workspace --no-fail-fast`：作为合并前总门禁，必须保持全部测试通过。
- `rtk cargo test -p vulcan-session-core subagent_registry`：覆盖父级作用域槽位、session 唯一性、Archived 吸收态、子树和路径投影。
- `rtk cargo test -p vulcan-task-session segmented_store_`：覆盖分段存储查询、同父同名开放 owner 拒绝和归档历史选择。
- `rtk cargo test -p vulcan-agent-runtime persistence_store`：覆盖恢复物化、写入回滚、驻留状态自愈和归档不可重开。
- `rtk cargo test -p vulcan-agent-runtime registry_mailbox`：覆盖跨父同名隔离、消息 owner 重验、队列清理和树路径展示。
- `rtk cargo test -p vulcan-agent-runtime runtime_flow`：覆盖 execution claim、过期结果、并发任务准入、interrupt/close 生命周期。
- `rtk cargo test -p vulcan-workspace-store -p vulcan-service-kernel`：覆盖 Goal 写边界、生命周期预留、事件投影与 kernel 准入。
- `rtk cargo test -p vulcan-service-runtime -p vulcan-session-core`：覆盖运行时日志双向投影、规范消息身份与复放错误传播。

#### 第三层：并发与故障注入

- 对同父同名 spawn、后台完成与 close 竞争、send-message 与归档竞争、interrupt 与 slot 复用竞争分别做高频重复运行；任何一次都不得出现双 owner、Archived 重开或消息跨 session 投递。
- 在恢复事务的第一条和中间元数据写入处分别注入失败，验证已写入文件回滚、registry/session 不发生半提交、重试后状态一致。
- 在日志追加、压缩备份、反向重放和规范消息追加处注入损坏输入与 I/O 失败，验证错误显式上报且旧快照仍可读取。
- 对 Goal 更新、停止、清理和最终用量记账制造过期 execution，验证旧执行无法覆盖新一代状态或重复结算。
- 并发测试建议在 CI 中重复至少 20 次；出现一次抖动即视为失败，必须查清 owner、锁序和事件顺序，禁止只加重试掩盖。

#### 第四层：桌面端端到端验收

- 使用 `rtk make br` 启动调试桌面端，保留录像和运行时日志作为验收证据。
- 会话/UI：覆盖新会话转正、空/短/超长转录互切、输入区位移、右侧上下文与 Git 工具框贴靠、活动页签滚动、连续工具调用组合及 checkpoint 顺序。
- 智能体目录：覆盖合法 sidecar、缺失/错误 schemaVersion、不可移植标识、原子写入失败和目录链接拒绝；错误必须可诊断且不能生成半文件。
- 日志/会话：执行工具调用、流式文本、压缩、重启和重放；调用主键、ToolResult 归属和显示顺序必须前后一致。
- Goal：覆盖创建、状态更新、预算耗尽、停止、清理、崩溃恢复和新回合准入；实时用量与最终用量必须一致且只结算一次。
- 子代理：覆盖不同父级复用相同任务名、同父重复拒绝、嵌套消息、interrupt、关闭整棵子树、关闭过程中后台完成、重启恢复和归档名称复用。

### 发布验收标准

- Rust 全工作区测试、Clippy、格式、Web lint/build 和 diff 检查全部通过。
- 并发/故障注入矩阵无双 owner、无 Archived 重开、无半提交、无静默降级。
- 桌面端关键流程录像逐项通过，运行时日志不存在未解释错误。
- 所有新增状态字段都能追溯到唯一类型或协议定义；不得以 `a || b`、候选字段轮询或模糊名称兼容掩盖不确定性。

### 结束时已知交接项

- registry 仍缺少可区分合法主会话根与 TaskSession 父级的显式 `ParentSessionRef` 身份模型。
- 多文件持久事务在进程中途崩溃时仍需 WAL/manifest transaction 才能具备真正崩溃原子性。
- TaskSession metadata 仍需持久 execution owner/revision/CAS 才能覆盖跨进程并发。
- workspace-store Goal API 仍有 `String` 错误边界待类型化。
- Web 工程当前只有 lint/build 脚本，没有独立自动化 UI 测试脚本；关键桌面流程目前必须依赖手工端到端验收，后续应建立 Playwright/桌面壳自动化覆盖。

## 第 136 轮：将运行时事件格式重置为唯一干净 v1（2026-07-11）

### 问题发现与执行流程

- 第 135 轮后的未提交工作树把 `RUNTIME_LOG_SCHEMA_VERSION` 从 1 连续提升到 2、3，但磁盘已有会话仍保存旧 v1；读取器在第一行严格比较当前常量后报出 `Unsupported runtime line schema version 1 at line 1`，导致整段正文拒绝加载。
- 逐字段 diff 证明 runtime 事件结构只有 ToolResult 身份发生变化：旧结构保存 `tool_result_id`，新结构保存本地协议消息 `message_id`；其余 User、Assistant、ToolCall、批次、文本、状态、错误、调用关系和审计事件字段均未变化。
- 调用链确认旧 `tool_result_id` 不参与 Provider 工具结果匹配；匹配事实由 `tool_call_id`、`runtime_tool_call_id` 等调用身份持有。旧 v1 replay 也已经把 `tool_result_id` 当作外层消息 id，因此新 `message_id` 在表达能力上能够完整承载旧会话可重放语义。
- 但新旧两种不兼容形态若同时标记为 v1，读取器不能在遵守零猜测原则的情况下通过字段探测区分；直接把常量改回 1 而保留历史数据会形成同号异构协议。
- 用户明确选择方案 1：主动删除全部旧历史会话，不保留旧 v1/v2/v3 数据迁移或兼容；当前新格式重新定义为唯一干净 v1。

### 核心修复与调整概述

- 将当前 `RUNTIME_LOG_SCHEMA_VERSION` 固定为 1；所有新建、重建、追加和修复事件统一写入新 v1。
- 删除本轮探索阶段加入的 v1/v2→v3 解码迁移、迁移行状态与自动重写逻辑，最终代码中不存在历史 schema 分支。
- 新增 current-only runtime codec：先解析 JSON，再读取唯一 `schema_version`，只接受当前 v1，随后按当前类型严格解码。
- codec 只把真正的 JSON 语法错误标记为可修复损坏尾部；版本错误、缺字段、错误字段形态等结构问题均显式失败，不能被容错读取静默删除。
- `RuntimeToolResultEvent` 增加 `deny_unknown_fields`，当前 v1 只接受 `message_id` 与定义内字段；已删除的 `tool_result_id` 即使复用版本号 1 也不会被忽略。
- 会话存储读取入口统一使用 current-only codec；未知版本与旧 ToolResult 形态均在任何备份、修复或重写前失败并保留源字节。

### 📂 文件变更清单

- 新增 `crates/vulcan-context/src/runtime/codec.rs`：实现只支持当前 v1 的严格日志行解码与类型化错误。
- 修改 `crates/vulcan-context/src/runtime.rs`：注册并导出 current-only codec。
- 修改 `crates/vulcan-context/src/lib.rs`：向存储 crate 导出严格解码入口与错误类型。
- 修改 `crates/vulcan-context/src/runtime/events.rs`：将新格式固定为 v1，并严格拒绝 ToolResult 未知字段。
- 修改 `crates/vulcan-context/src/runtime/tests.rs`：覆盖干净 v1 ToolResult、未知版本、旧字段形态和畸形 JSON 分类。
- 修改 `crates/vulcan-workspace-store/src/workspace_store/segmented_session_store.rs`：接入 current-only codec。
- 修改 `crates/vulcan-workspace-store/src/workspace_store/segmented_session_store/runtime_io.rs`：只对语法损坏尾部容错，结构错误直接传播。
- 修改 `crates/vulcan-workspace-store/src/workspace_store/segmented_session_store/tests/runtime_rebuild.rs`：覆盖完整 ToolResult 会话往返、索引、旧形态非破坏性拒绝和未知版本拒绝。
- 修改 `TASK_LOG.md`：追加本轮回归原因、Schema 决策与验证记录。

### 💻 关键代码调整详情

- `decode_runtime_log_event_line_json` 先从 JSON 对象读取无符号 32 位 `schema_version`；不是当前 1 时返回 `UnsupportedSchemaVersion`，不查看事件字段猜测来源。
- schema 为 1 后才反序列化 `RuntimeLogEventLine`；结构缺失或字段不合法返回 `InvalidCurrentPayload`，与 `InvalidJson` 明确分层。
- `read_runtime_segment_lines_tolerant` 只有在 `error.is_invalid_json()` 时才进入损坏后缀收集；旧结构或未知版本立即携带文件路径和物理行号返回。
- 新 v1 ToolResult 用唯一 `message_id` 建立 runtime index；Provider/runtime call id 继续只承担调用关系，不恢复已删除伪结果身份。
- fork 路径虽然在身份重写后使用 typed deserialize，但进入该函数前已经通过 `read_runtime_log_snapshot_with_owned_lock` 执行 current-only codec 与版本校验，不存在绕过入口。

### 修改部分代码审核与自动修复

- 初始修复实现了 v1/v2→v3 单向迁移并已通过测试；用户确认放弃全部历史后，完整删除迁移模块、迁移状态和发布条件，没有留下死兼容代码。
- 审核发现直接恢复 `serde_json::from_str::<RuntimeLogEventLine>` 会把“语法正确但结构错误”的旧 ToolResult 当作损坏尾部；保留并简化为 current-only codec，阻止静默裁剪。
- 审核发现 serde 默认忽略 ToolResult 未知字段，可能让同时包含 `message_id/tool_result_id` 的混合数据伪装成新 v1；增加 `deny_unknown_fields` 固定唯一形态。
- CodeKit 全仓反查确认 `DecodedRuntimeLogEventLine`、`was_migrated`、`migrated_line_numbers`、runtime schema migration 与 v2/v3 当前结构描述均为零命中。
- 审核唯一直接 `from_value::<RuntimeLogEventLine>` 的 fork 路径，确认其前置 snapshot 已严格验证同一锁事务中的全部运行时行。

### 验证结果

- current-only codec 定向测试：4 项全部通过。
- workspace-store runtime-log 定向测试：24 项全部通过。
- `cargo test -p vulcan-context --lib`：63 项全部通过。
- `cargo test -p vulcan-workspace-store --lib`：154 项全部通过。
- `cargo test --workspace --no-fail-fast`：63 个测试套件共 1718 项全部通过。
- `cargo clippy --workspace --all-targets -- -D warnings`：通过，零告警。
- `cargo fmt --all -- --check` 与 `git diff --check`：通过。

### ⚠️ 使用要求与注意事项

- 运行新构建前必须按用户决策删除旧会话数据；代码不会迁移、猜测或自动重写旧历史。
- 因为新旧格式都曾使用版本号 1，不含旧 ToolResult 的旧日志可能在字段形态相同处被正常解码；彻底删除旧数据是保证数据根只存在新 v1 的必要前置条件。
- 含旧 `tool_result_id` 的日志会显式报 `Invalid runtime log v1 payload`，且源文件不会被修改。
- 本轮没有提交、暂存或主动删除用户本地会话数据。
