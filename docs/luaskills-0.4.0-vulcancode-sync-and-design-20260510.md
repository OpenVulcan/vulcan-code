# LuaSkills 0.4.0 同步结论与 VulcanCode 接入设计（2026-05-10）

## 1. 文档目的

本文档只解决两件事：

1. 同步 `LuaSkills 0.4.0` 已经正式落地的能力与协议，收敛掉我们此前仍停留在 0.3 规划阶段的旧假设。
2. 基于 `LuaSkills 0.4.0` 已有的 `runtime_lease`、`system_runtime_lease`、`system_lua_lib`、`host_result` 等能力，为 `VulcanCode` 制定一套现实可落地的接入方案。

本文档不是泛泛而谈的架构讨论，而是面向当前代码库与当前宿主需求的接入决策稿。

---

## 2. 本次同步的上游范围

本次同步基于以下仓库与文件：

- `D:/projects/vulcan-luaskills/README.zh-CN.md`
- `D:/projects/vulcan-luaskills/src/runtime/result.rs`
- `D:/projects/vulcan-luaskills/src/runtime/engine.rs`
- `D:/projects/vulcan-luaskills/src/host/options.rs`
- `D:/projects/vulcan-luaskills/src/ffi.rs`
- `D:/projects/vulcan-luaskills/src/ffi_standard.rs`
- `D:/projects/vulcan-luaskills/docs/zh-CN/skill-development.md`
- `D:/projects/vulcan-luaskills/docs/zh-CN/ffi/integration-guide.md`
- `D:/projects/vulcan-luaskills/docs/zh-CN/architecture/host-tooling-result-bridge-design.md`

`VulcanCode` 侧当前实际接入面主要位于：

- `D:/openvulcan/vulcan-code/apps/vulcan-code-gui/src/luaskills_runtime.rs`

---

## 3. LuaSkills 0.4.0 已经正式具备的能力

### 3.1 运行时租约正式从 `runtime_session` 收敛为 `runtime_lease`

上游已经不再停留在“内部 session 草案”阶段，而是正式形成了完整的租约能力面：

- `runtime_lease`
- `system_runtime_lease`

代码落点：

- `D:/projects/vulcan-luaskills/src/runtime/engine.rs`
- `D:/projects/vulcan-luaskills/src/ffi.rs`
- `D:/projects/vulcan-luaskills/src/ffi_standard.rs`

这意味着：

1. `VulcanCode` 不需要再自创一套会话化 Lua VM 协议。
2. 不需要继续围绕旧的 `runtime_session` 命名做兼容设计。
3. 后续宿主接入应直接对齐 `runtime_lease` / `system_runtime_lease` 正式接口。

### 3.2 `system_runtime_lease` 已经是真正独立语义，不再只是 authority 包装

上游 `0.4.0` 中，`system_runtime_lease` 已经不是“普通租约 + system authority”的薄包装，而是带有明确 profile 的正式入口。

内部已经有：

- `RuntimeLeaseProfile::Public`
- `RuntimeLeaseProfile::SystemLuaLib`

并且会在租约创建、路径装配、TTL 处理和后续状态查询中按 profile 分流。

代码落点：

- `D:/projects/vulcan-luaskills/src/runtime/engine.rs`

### 3.3 `system_lua_lib_dir` 已经进入宿主 HostOptions

`LuaRuntimeHostOptions` 里已经正式存在：

- `system_lua_lib_dir: Option<PathBuf>`

代码落点：

- `D:/projects/vulcan-luaskills/src/host/options.rs`

这意味着宿主不需要再发明一套额外配置对象来表达系统 Lua 库目录，应该直接沿用上游已有字段。

### 3.4 `host_result` 已经进入主结果协议

`RuntimeInvocationResult` 现在已经正式包含：

- `content`
- `overflow_mode`
- `template_hint`
- `content_bytes`
- `content_lines`
- `host_result`

同时上游已经有：

- `RuntimeHostResult { kind, payload }`

代码落点：

- `D:/projects/vulcan-luaskills/src/runtime/result.rs`

这意味着：

1. 我们不需要再把 `host_result` 当成未来提案。
2. 也不需要继续假设“运行时结果仍然只有文本层”。
3. `VulcanCode` 现在可以直接以 `host_result` 为正式宿主结构化结果通道进行接入设计。

### 3.5 `host_result` 的协商方式已经定型为“请求级能力协商”

上游 0.4.0 已经明确：

1. 宿主是否允许 `host_result`，不由全局静态布尔开关决定。
2. 真实协商入口是：
   - `request_context.client_capabilities.host_result`
3. Lua 侧优先读取：
   - `vulcan.context.host_result`

代码与文档落点：

- `D:/projects/vulcan-luaskills/src/runtime/engine.rs`
- `D:/projects/vulcan-luaskills/docs/zh-CN/skill-development.md`
- `D:/projects/vulcan-luaskills/docs/zh-CN/ffi/integration-guide.md`

这意味着我们此前提出的：

- `enable_host_result_bridge`

这类静态宿主开关命名，在上游当前正式实现里已经不是主协商入口。

### 3.6 Lua 第四返回值协议已经正式成立

上游现在已经支持：

```lua
return content, overflow_mode, template_hint, host_result
```

并且具有如下语义：

1. 宿主未开启 `host_result` 时，第四返回值会被忽略。
2. 宿主开启后，`kind`、`payload`、大小上限与允许种类会被校验。
3. 第一版 canonical kind 是：
   - `change_set`

代码与文档落点：

- `D:/projects/vulcan-luaskills/src/runtime/engine.rs`
- `D:/projects/vulcan-luaskills/docs/zh-CN/skill-development.md`

### 3.7 标准 C ABI 已经直接支持 `host_result`

这一点与我们此前的预判不同。

当前上游标准 C ABI 中：

- `FfiRuntimeInvocationResult` 已经直接带有：
  - `host_result: *mut FfiRuntimeHostResult`
- `FfiLuaRuntimeHostOptions` 已经直接带有：
  - `system_lua_lib_dir`

代码落点：

- `D:/projects/vulcan-luaskills/src/ffi_standard.rs`

这意味着：

1. 我们不应再沿用“标准 C ABI 必须单独走 V2 才能承载 host_result”的旧判断。
2. 至少从 `0.4.0` 的当前实现看，上游已经选择了直接扩展当前结构。

---

## 4. 必须明确废弃的旧假设

结合 `0.4.0` 当前代码，以下旧判断需要正式废弃：

### 4.1 废弃“`RuntimeInvocationResult` 只有文本结果”的判断

这个判断已经失效。`host_result` 已经进入正式结构。

### 4.2 废弃“需要我们自己发明 `runtime_session -> runtime_lease` 命名收敛”的判断

上游已经完成这一步，当前正式能力面就是 `runtime_lease` 与 `system_runtime_lease`。

### 4.3 废弃“`system_runtime_lease` 只是 authority wrapper”的判断

这也已经失效。当前是正式 profile 语义。

### 4.4 废弃“`host_result` 需要新增 `enable_host_result_bridge` 到 HostOptions 才能使用”的判断

上游当前正式协商入口是：

- `request_context.client_capabilities.host_result`

而不是 `LuaRuntimeCapabilityOptions` 中的静态布尔字段。

### 4.5 废弃“标准 C ABI 一定要先做 `host_result` V2”的判断

至少在 `0.4.0` 当前实现里，这一点已经不是事实。

---

## 5. VulcanCode 当前接入现状

### 5.1 当前只完成了最小引导

`D:/openvulcan/vulcan-code/apps/vulcan-code-gui/src/luaskills_runtime.rs` 当前只做了这些事情：

1. 创建 `LuaEngine`
2. 同步 runtime assets
3. 挂载 `ROOT / USER` skill roots
4. 保存长期 `LuaEngine` 句柄

### 5.2 当前尚未接入的关键能力

当前代码里还没有真正接上：

1. `system_lua_lib_dir`
2. `runtime_lease` 宿主封装
3. `system_runtime_lease` 宿主封装
4. `LuaInvocationContext`
5. `request_context.client_capabilities.host_result`
6. `host_result` 结构化消费
7. 基于 `system_runtime_lease` 的长期宿主 LuaRuntime

换句话说，`VulcanCode` 目前还停留在：

- “引擎已经起起来了”

但尚未进入：

- “把 LuaSkills 当成 IDE 工具运行时与宿主结构结果桥来用”

的阶段。

---

## 6. VulcanCode 现在应当采用的总体策略

### 6.1 总体原则

基于 `LuaSkills 0.4.0` 的实际能力，`VulcanCode` 侧应正式采用下面四条原则：

1. **LuaSkills-first**
   - 把 LuaSkills 作为 AI Coding 工具体系统一底座。
2. **Managed Skills for AI-facing tools**
   - 面向 AI 的常规工具优先走普通 skill / managed skill。
3. **System LuaRuntime for host-native internals**
   - 宿主长期驻留、宿主私有契约、宿主固定依赖能力走 `system_runtime_lease + system_lua_lib`。
4. **Text result + host_result dual path**
   - 普通文本仍然给模型与用户看，结构化结果通过 `host_result` 给宿主原生 UI 与状态层消费。

### 6.2 现在不要做的事

当前阶段不建议：

1. 再发明一套独立于 LuaSkills 的宿主长期 Lua VM 管理器。
2. 把 `system_lua_lib` 重新包装成“普通技能管理面”的一部分。
3. 把 `host_result` 继续当成纯文档提案，而不进入宿主接入主路径。
4. 假设上游已经实现了 entry 级 `exposure / management_plane` 元数据并直接依赖它们。

---

## 7. VulcanCode 的接入分层设计

## 7.1 第一层：引擎与宿主路径底座

`luaskills_runtime.rs` 需要扩展为真正的宿主运行时引导层，而不只是 `LuaEngine` 创建器。

第一层至少要补这些配置：

1. `system_lua_lib_dir`
2. 运行时级 `temp/resources/lua_packages/libs/tools` 目录规划
3. `LuaInvocationContext` 构建辅助
4. 宿主统一 request context 组装器

推荐目录模型：

```text
{application_data_root}/luaskills-runtime/
  temp/
  resources/
  lua_packages/
  libs/
  bin/tools/
  skills/root/
  skills/user/
  system_lua_lib/
```

也就是说，`VulcanCode` 应当在当前 runtime root 下补一个固定目录：

- `system_lua_lib/`

并把它显式写入：

- `LuaRuntimeHostOptions.system_lua_lib_dir`

### 7.2 第二层：普通 AI 工具调用面

面向 AI 的普通工具，继续按 managed skill 路线走：

1. skill root 加载
2. 列出 entry
3. 由宿主决定哪些工具真正对 AI 可见
4. 调用时附带 `LuaInvocationContext`

这一层的重点不是 `system_runtime_lease`，而是：

1. 把 `content` 返回给模型
2. 在宿主开启时读取 `host_result`
3. 对编辑类、分析类、文件类结果做原生联动

### 7.3 第三层：宿主内部长期能力面

宿主内部固定依赖能力，不应伪装成普通 AI 工具。

这一层建议直接用：

- `system_runtime_lease`

去承载长期 VM。

适合放进这层的能力例如：

1. 动态 AST Tree
2. 宿主结构快照
3. 会话级代码结构缓存
4. 局部增量刷新器
5. 宿主内部索引维护器

它们的共同特征是：

1. 不适合每轮无状态重建
2. 不一定直接给 AI 看
3. 更适合宿主先消费，再决定如何投喂给 AI

---

## 8. `system_lua_lib` 在 VulcanCode 中的推荐定位

### 8.1 它不是普通 skill

对 `VulcanCode` 来说，`system_lua_lib` 的语义应明确为：

1. 宿主固定内建
2. 宿主自有代码
3. 宿主自有契约
4. 宿主直接调度
5. 不进普通技能安装/卸载/启停管理面

因此它应看作：

- 宿主 LuaRuntime

而不是：

- “另一类隐藏 skill”

### 8.2 推荐实例模型

推荐 `VulcanCode` 对 system runtime 采用“按用途固定 SID / Lease”的模型，而不是每次临时创建。

例如：

1. `system.ast-live-tree.{workspace}`
2. `system.code-snapshot.{workspace}`
3. `system.analysis-cache.{workspace}`

这样可以保证：

1. 同一工作区的长期 Lua 状态稳定保留
2. 增量刷新可以只传少量 changed files
3. 宿主无需每轮重建完整 Lua 世界

### 8.3 推荐第一批系统运行时能力

结合当前需求，优先级最高的系统 LuaRuntime 能力建议是：

1. `dynamic ast tree`
2. `workspace structure snapshot`
3. `incremental code navigation cache`

这些能力更适合：

- `system_runtime_lease`
- `host_result.kind = host-private structured_tree`

而不是直接暴露成 AI 可见工具。

---

## 9. `host_result` 在 VulcanCode 中的消费策略

### 9.1 第一优先级：`change_set`

`change_set` 是 `VulcanCode` 现在最应该优先打通的 canonical kind。

原因：

1. 它已经是上游第一版标准 kind。
2. 它最直接对应 IDE 的 diff / patch / preview / applied 结果面板。
3. 它能把编辑工具从“文本说明”升级成“宿主原生可消费结果”。

推荐第一批消费场景：

1. `vulcan-file-edit`
2. 后续代码修改类工具
3. Patch / apply 预览类工具

### 9.2 第二优先级：宿主私有结构树 kind

对动态 AST Tree 这类能力，当前不应等待上游先标准化 `structured_tree` 再接。

在 `0.4.0` 阶段，建议 `VulcanCode` 直接使用宿主私有 kind，例如：

- `vulcan_ast_live_tree`
- `vulcan_workspace_structure_tree`

原因：

1. 上游当前第一版 canonical kind 明确的是 `change_set`。
2. 宿主私有结构结果本身就属于 `VulcanCode` 原生面板能力。
3. 我们可以先在宿主消费层稳定下来，再看是否反推上游标准化。

### 9.3 请求级协商策略

宿主在每次调用 skill 或 lease eval 时，都应决定是否开启 `host_result`。

推荐默认规则：

1. AI 普通问答型调用：
   - 默认关闭 `host_result`
2. 文件编辑、代码分析、宿主结构工具调用：
   - 按工具类型显式开启
3. `system_runtime_lease` 内部宿主能力：
   - 通常开启

协商载荷建议通过：

- `request_context.client_capabilities.host_result`

明确控制：

1. `enabled`
2. `allowed_kinds`
3. `max_payload_bytes`

---

## 10. AI 可见性与宿主管理面的现实处理方式

### 10.1 当前不要假设上游已经有 `exposure` 与 `management_plane`

虽然从长期架构上，我们仍然需要：

1. `ai_public`
2. `host_only`
3. `hybrid`
4. `system`
5. `managed`

这些语义，但在 `LuaSkills 0.4.0` 当前仓库里，它们还不是正式的统一 entry 元数据协议。

因此 `VulcanCode` 当前阶段不应把这层职责上推给 `LuaSkills`，而应先由宿主自己的工具导出层管理。

### 10.2 现阶段推荐做法

`VulcanCode` 当前应自己维护一层“工具导出策略”，把运行时 entry 分成三类：

1. **Managed + AI Public**
   - 普通技能，直接可对 AI 暴露
2. **Managed + Hybrid**
   - 技能可被宿主直接用，也可以按策略对 AI 开放
3. **System + Host Only**
   - 只给宿主内部调用，完全不进入 AI 工具表

也就是说：

- `LuaSkills` 负责运行
- `VulcanCode` 负责导出

直到未来上游正式沉淀这组 entry 元数据。

---

## 11. 对当前代码库的具体调整建议

### 11.1 第一批必须调整

#### A. 扩展 `luaskills_runtime.rs`

需要补：

1. `system_lua_lib_dir` 注入
2. `LuaInvocationContext` 构建辅助
3. runtime lease 调用辅助
4. system runtime lease 调用辅助

当前文件：

- `D:/openvulcan/vulcan-code/apps/vulcan-code-gui/src/luaskills_runtime.rs`

应从“引擎引导辅助”升级为“LuaSkills 宿主运行时接入层”。

#### B. 新增宿主租约桥接模块

建议新增一层专门桥接模块，例如：

- `apps/vulcan-code-gui/src/luaskills_bridge/runtime_lease.rs`
- `apps/vulcan-code-gui/src/luaskills_bridge/system_lua_lib.rs`
- `apps/vulcan-code-gui/src/luaskills_bridge/host_result.rs`

职责分开：

1. 普通租约
2. 系统租约
3. 宿主结果消费

#### C. 新增 `host_result` 到原生 UI 结果链

需要让现有工具结果链具备：

1. 读取 `RuntimeInvocationResult.host_result`
2. 对 `change_set` 直接驱动 diff 面板
3. 对宿主私有 kind 驱动专属面板

### 11.2 第二批建议调整

#### A. 定义 `system_lua_lib` 的目录与初始化约定

建议固定：

- `runtime_root/system_lua_lib`

并约定：

1. 初始化脚本归宿
2. 模块布局
3. 常驻 `_G.__host` 状态命名方式

#### B. 定义宿主侧 `sid / lease_id` 规划规则

避免后续 system runtime 乱建乱关。

#### C. 建立一层 AI 工具导出白名单

在上游 entry 元数据尚未正式具备 `exposure` 的阶段，这一步必须在 `VulcanCode` 宿主完成。

---

## 12. 推荐实施顺序

### 第一阶段：同步接入底座

目标：

1. `system_lua_lib_dir` 正式接入
2. `LuaInvocationContext` 正式进入调用路径
3. 宿主可以显式发起：
   - `runtime_lease`
   - `system_runtime_lease`

### 第二阶段：打通 `host_result`

目标：

1. 宿主能够在调用时开启 `request_context.client_capabilities.host_result`
2. `change_set` 能稳定驱动 diff 面板
3. 普通文本与宿主结构结果正式分流

### 第三阶段：落地 `system_lua_lib` 首个真实能力

目标：

1. 先做 `dynamic ast tree`
2. 采用：
   - `system_runtime_lease`
   - 宿主私有 kind
3. 宿主端实现结构树消费与缓存刷新

### 第四阶段：推进 LuaSkills-first 工具迁移

目标：

1. 常规 AI Coding 工具逐步迁到 LuaSkills
2. 宿主只保留：
   - 策略
   - 权限
   - 面板
   - 原生消费

---

## 13. 当前结论

结合 `LuaSkills 0.4.0` 真实代码与文档，`VulcanCode` 现在应该明确采用如下结论：

1. **上游已经具备 `runtime_lease` / `system_runtime_lease` / `system_lua_lib_dir` / `host_result` 主能力，不需要我们再重造一套协议。**
2. **`host_result` 已经是正式主链协议，`VulcanCode` 必须把它纳入本轮接入设计，而不是继续当成未来提案。**
3. **`system_runtime_lease` 应成为宿主内部长期 LuaRuntime 的正式底座，优先承载 AST Tree 这类宿主固定依赖能力。**
4. **上游尚未正式沉淀 AI 可见性与管理平面元数据，因此当前阶段这层职责仍应由 `VulcanCode` 宿主自己掌控。**
5. **`LuaSkills-first` 现在已经不是方向性口号，而是具备现实落地条件的实施路线。**

---

## 14. 对我们现有旧文档的处理建议

当前仓库中的旧文档：

- `D:/openvulcan/vulcan-code/docs/luaskills-host-tooling-protocol-20260509.md`

仍然保留了不少 `0.4.0` 之前的假设，例如：

1. `RuntimeInvocationResult` 尚未包含 `host_result`
2. 需要新增 `enable_host_result_bridge`
3. 标准 C ABI 需要单独 `V2`
4. 还在围绕旧的 `runtime_session` 思维做命名与桥接设计

因此后续应当以本文为准，对旧文档进行一次正式收口。  
旧文档可以保留为历史讨论稿，但不应继续作为实现基线。
