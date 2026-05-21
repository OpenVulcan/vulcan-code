# 上下文运行态与显示转录分离架构设计

## 1. 目标

当前会话系统已经切换到文件化 session event store，但“显示给用户看的转录内容”和“真正发给模型的活动上下文”仍然共享大量结构。  
这在静态上下文压缩阶段尚可工作，但一旦后续支持：

- AI 动态分片总结
- 记忆节点折叠/展开
- 长期记忆失活/恢复
- 工具结果按策略替换原文
- provider prompt cache 稳定前缀

显示层与上下文层继续共用一套事实源，会不断出现以下问题：

1. UI 折叠/隐藏误伤真实上下文。
2. 压缩后无法准确复原某次请求当时真正看到的上下文。
3. 工具输出、summary、memory 节点混入显示结构后，反推上下文越来越不可靠。
4. 某次历史请求的 prompt cache 命中条件无法审计。
5. 后续 AI 自主治理记忆时，基础消息顺序无法表达“节点替换、节点失活、局部展开”等行为。

因此，本设计目标是明确把会话系统拆成三层：

1. **Transcript Event Log**
2. **Context Runtime State**
3. **Request Snapshot**

并将它们分别赋予单一职责，避免互相污染。

---

## 2. 设计原则

### 2.1 Transcript 不承担上下文真相职责

Transcript 的职责是：

- 向用户完整展示对话过程
- 保留审计痕迹
- 支持复制、选择、分页、搜索预览
- 保留工具调用、思考、失败、中断、重试、媒体结果等完整过程

Transcript 的职责不包括：

- 直接决定下一次模型请求窗口
- 反向还原 active context
- 持有上下文折叠状态
- 表达长期记忆图依赖关系

一句话：  
**Transcript 是显示与审计流，不是上下文运行流。**

### 2.2 Context Runtime State 独立存在

Context Runtime State 的职责是：

- 描述当前真正参与模型请求的活动上下文结构
- 表达记忆节点的激活/折叠/替换/展开
- 记录 summary 节点与原始段之间的映射关系
- 维护 provider-visible context window 的稳定边界

它不负责：

- 直接承担 UI 渲染
- 作为历史展示顺序来源
- 保存所有用户可见的富展示细节

一句话：  
**Context Runtime State 是模型推理所依赖的活动记忆图。**

### 2.3 每次真实请求都必须留下独立快照

仅有 Transcript 和 Runtime Context 仍然不够。  
因为如果上下文是动态变化的，那么系统必须回答：

> 某一次真实请求发出时，模型到底看到了什么？

因此每次真实 provider 调用都必须形成独立 Request Snapshot，用于：

- 请求审计
- prompt cache 稳定性分析
- 上下文回放
- 真实问题排查
- provider usage 归因

一句话：  
**Request Snapshot 是“当时真正发出去的上下文”留痕。**

---

## 3. 三层架构定义

## 3.1 Transcript Event Log

### 3.1.1 定义

Transcript Event Log 是 session 目录下只追加的完整展示事件流。

### 3.1.2 保留内容

应保留：

- `user_prompt`
- `user_intervention`
- `assistant_thinking_visible`
- `assistant_output_visible`
- `tool_call_visible`
- `tool_result_visible`
- `media_tool_result_visible`
- `turn_summary_visible`
- `turn_terminated_visible`
- `turn_stats_visible`
- `system_notice_visible`

### 3.1.3 特征

- 只追加，不做原地重写
- 可分页、可搜索、可审计
- 保留用户看到的完整过程
- 允许存在仅供展示的辅助节点
- 允许与 Context Runtime State 完全不同步

### 3.1.4 关键约束

Transcript 中任何节点的显示变化：

- 不能改变 Context Runtime State
- 不能改变下次请求实际上下文
- 不能影响 Request Snapshot

---

## 3.2 Context Runtime State

### 3.2.1 定义

Context Runtime State 是当前 session 的活动记忆图，用来描述：

- 哪些内容还在上下文里
- 哪些内容已被 summary 替换
- 哪些节点是长期记忆
- 哪些节点当前失活
- 哪些工具结果使用 preview 替代原文
- 哪些段正在被临时展开

### 3.2.2 核心对象

建议引入以下核心概念：

#### 1. Context Node

表示参与上下文管理的最小记忆单元。

建议类型：

- `live_turn_segment`
- `summary_node`
- `memory_node`
- `tool_result_ref`
- `transient_runtime_note`
- `system_context_node`

#### 2. Context Segment

表示一段可以被折叠、替换、展开的上下文区间。

例如：

- 第 1~8 轮对话原始区段
- 某个大型工具输出段
- 某组长期约束摘要段

#### 3. Context Action

表示 AI 或系统对上下文做的治理动作：

- `fold_segment`
- `expand_segment`
- `replace_with_summary`
- `invalidate_node`
- `reactivate_node`
- `replace_tool_result_payload`

#### 4. Active Window

当前真正进入 provider 请求的上下文窗口。

它不是 transcript 子串，而是由 runtime graph 拼装得出：

- system layer
- memory nodes
- active summaries
- live recent tail
- selected tool result refs

### 3.2.3 特征

- 可以重写
- 可以折叠
- 可以删除活动引用
- 可以恢复历史节点
- 可以与 transcript 完全不同序
- 可以拥有比 transcript 更少或不同的节点

---

## 3.3 Request Snapshot

### 3.3.1 定义

Request Snapshot 表示一次真实 provider 调用之前，系统最终发出的完整上下文快照。

### 3.3.2 它必须记录什么

建议至少记录：

- `request_id`
- `session_id`
- `turn_id`
- `provider_id`
- `model_id`
- `created_at`
- `system_prompt_hash`
- `context_runtime_revision`
- `active_checkpoint_id`
- `context_node_refs`
- `replacement_refs`
- `final_provider_messages`
- `final_provider_messages_hash`
- `request_budget_snapshot`
- `provider_usage_result`

### 3.3.3 为什么必须独立

因为不能依赖“未来再从 transcript + runtime graph 回放一次”得到完全一致的结果。  
尤其在以下情况中，回放会漂移：

- AI 后续再次折叠上下文
- summary 节点被替换
- 工具结果 preview 策略升级
- system preamble 更新
- provider continuation 状态变化

所以 Request Snapshot 必须是**真实执行前的冻结结果**。

---

## 4. 与当前 Session Event Store 的关系

当前 event store 已经有：

- `manifest.json`
- `events/000001.jsonl`
- `blobs/`
- `runtime_state.json`
- `checkpoints/`

这套结构仍然可以保留，但要调整职责边界。

### 4.1 当前可保留部分

保留：

- Transcript event file 目录结构
- blob 外部化机制
- checkpoint 文件目录
- SQLite turn index / search projection / session metadata

### 4.2 当前需要重构的部分

当前 `runtime_state.json` 承担的职责过重，后续应拆成：

1. `transcript_ui_state`
2. `context_runtime_state`
3. `request_snapshot_state`

建议：

- `runtime_state.json` 继续承载 UI 草稿、折叠状态、局部选择状态等
- 新增 `context_state.json`
- 新增 `requests/` 目录持久化请求快照

---

## 5. 推荐目录结构

```text
sessions/
  <project-id>/
    <session-id>/
      manifest.json
      events/
        000001.jsonl
        000002.jsonl
      blobs/
        <sha256>.json
        <sha256>.txt
        <sha256>.bin
      checkpoints/
        checkpoint-000001.json
        checkpoint-000002.json
      requests/
        request-000001.json
        request-000002.json
      context/
        context-state.json
        context-actions.jsonl
      runtime_state.json
```

### 5.1 各目录职责

#### `events/`
- 只存 Transcript Event Log

#### `blobs/`
- 存超长工具输出、大文本、大附件元信息

#### `checkpoints/`
- 存 durable context checkpoint

#### `requests/`
- 存每次真实请求快照

#### `context/`
- 存活动上下文运行图
- 存上下文治理动作日志

#### `runtime_state.json`
- 仅保留 UI/runtime 辅助状态

---

## 6. 数据模型建议

## 6.1 Transcript Event

```json
{
  "event_id": "evt_xxx",
  "session_id": "sess_xxx",
  "turn_id": "turn_0012",
  "seq": 183,
  "kind": "tool_call_visible",
  "created_at": "...",
  "payload": {}
}
```

### 特点

- 只为展示与审计服务
- 不需要完全等价于 provider-visible context
- 允许保留更多 UI 友好元信息

---

## 6.2 Context Node

```json
{
  "node_id": "ctx_xxx",
  "kind": "summary_node",
  "source_event_refs": ["evt_101", "evt_102", "evt_103"],
  "blob_ref": null,
  "status": "active",
  "replacement_of": ["ctx_old_1", "ctx_old_2"],
  "metadata": {}
}
```

### 字段含义

- `node_id`：稳定上下文节点 id
- `kind`：节点类型
- `source_event_refs`：该节点源自哪些 transcript event
- `blob_ref`：大正文引用
- `status`：`active / folded / inactive / archived`
- `replacement_of`：说明它替换了哪些旧节点

---

## 6.3 Context Action

```json
{
  "action_id": "ctx_action_xxx",
  "created_at": "...",
  "actor": "ai",
  "kind": "replace_with_summary",
  "target_nodes": ["ctx_seg_1"],
  "result_nodes": ["ctx_summary_1"],
  "reason": "reduce token budget"
}
```

### 用途

- 审计 AI 做过哪些上下文治理动作
- 为未来可解释性与恢复提供依据
- 为调试“为什么某段记忆消失”提供证据

---

## 6.4 Request Snapshot

```json
{
  "request_id": "req_xxx",
  "session_id": "sess_xxx",
  "turn_id": "turn_0012",
  "provider_id": "deepseek",
  "model_id": "deepseek-chat",
  "created_at": "...",
  "context_revision": 42,
  "context_node_refs": ["ctx_summary_1", "ctx_memory_3", "ctx_tail_9"],
  "replacement_refs": [{
    "kind": "tool_result_preview",
    "source_node_id": "ctx_tool_17",
    "blob_ref": "blob_xxx"
  }],
  "provider_messages": [],
  "provider_messages_hash": "sha256:...",
  "usage": null
}
```

---

## 7. Turn 模型修正规则

根据当前约束，turn 应按以下规则统一：

### 7.1 turn 起点

- `user_prompt`

### 7.2 turn 内部允许包含

- `assistant_thinking`
- `tool_call`
- `tool_result`
- `assistant_output`
- `user_intervention`

其中：

`user_intervention` 仍属于当前 turn，而不是新 turn。

### 7.3 turn 闭合方式

1. `turn_summary`
2. `turn_terminated`

### 7.4 中断规则

如果人为停止、超时、失败、中止：

- 本轮 turn 以 `turn_terminated` 闭合
- 该 turn 没有 summary
- 下一次新的用户提示词必须开启新 turn

### 7.5 媒体规则

图片、视频、音频结果在 canonical 上仍然是：

- `tool_result`

UI 只是在 transcript projection 阶段将其渲染为媒体卡片。  
这能保证上下文与 provider replay 不被显示形式污染。

---

## 8. 动态上下文折叠场景下的行为

## 8.1 示例场景

假设第 1~12 轮内容过长，AI 将其压缩。

### Transcript 层

仍保留：

- 原始 1~12 轮完整过程
- 原始工具调用
- 原始工具结果
- 原始中断与重试记录

### Context Runtime 层

变成：

- `ctx_summary_12`
- `ctx_memory_constraint_1`
- `ctx_tail_13`
- `ctx_tail_14`

### Request Snapshot 层

第 15 轮实际发给模型的上下文记录为：

- system prompt
- `ctx_summary_12`
- `ctx_memory_constraint_1`
- `ctx_tail_13`
- `ctx_tail_14`

这时三层是不同的，但都正确。

---

## 9. 搜索与分页策略

## 9.1 Transcript 分页

Transcript 分页继续以：

- `turn_index`
- `event seq`

为基础。

因为用户上拉历史，是在浏览展示记录，不是在浏览活动上下文。

### 关键点

- 分页只影响 UI 展示
- 不影响 Context Runtime State
- 不影响下一次 provider 请求窗口

## 9.2 BM25 搜索

BM25 不应直接索引 Context Runtime State。  
它应主要索引：

- user prompt
- assistant final output
- turn summary
- 工具结果 preview

而不是所有原始工具正文。

### 原因

否则 Context Runtime 的动态折叠会污染搜索语义，且超长工具结果会严重干扰召回。

---

## 10. Core / GUI 边界建议

## 10.1 Core 应负责

- Context Runtime State 管理
- Context Node 图构建
- context action 执行
- request snapshot 构建
- provider-visible context window 组装
- tool result replacement 策略
- budget preflight
- durable context checkpoint

## 10.2 GUI 应负责

- Transcript 展示
- 上拉分页
- 工具详情查看
- turn stats 展示
- context runtime 的可视化面板（如果后续需要）
- request snapshot 调试面板（如果后续需要）

## 10.3 不应再由 GUI 负责

- 从显示记录推导真实上下文
- 决定 provider-visible context window
- 自行拼接压缩窗口
- 用 transcript metadata 反向补 canonical context

---

## 11. 推荐实施顺序

## 第一阶段：边界分离

目标：

- 明确 Transcript、Context Runtime、Request Snapshot 三层对象
- 在现有 session event store 基础上引入 `context/` 和 `requests/`

## 第二阶段：Context Runtime 落盘

目标：

- `context-state.json`
- `context-actions.jsonl`
- Request Snapshot 文件化

## 第三阶段：请求构建改道

目标：

- provider request 不再直接从 transcript/canonical message list 推导
- 改为统一从 Context Runtime State 组装

## 第四阶段：动态记忆折叠

目标：

- 支持 AI 触发 `replace_with_summary`
- 支持长期 memory node
- 支持节点失活/展开/恢复

## 第五阶段：可视化与调试

目标：

- 显示当前 active context graph
- 显示某次 request snapshot
- 显示某段记忆为何被折叠或替换

---

## 12. 风险与注意事项

### 12.1 Request Snapshot 体积控制

如果每次请求都保存完整 provider message list，长期会增长很快。  
建议：

- 保存完整消息体
- 同时提供 blob 外部化策略
- 对大型工具结果依旧只保存 preview + blob ref

### 12.2 Context Runtime 与 Transcript 不一致是预期，不是 bug

必须在架构和调试认知上明确：

- 两者不同步是正常状态
- 不应强制做“显示内容必须等于上下文内容”

### 12.3 搜索不能直接混用 Context Node

Context Node 是运行态对象，不是稳定搜索文档。  
搜索应优先索引 transcript projection 与 summary 文档。

### 12.4 turn stats 不应进入 Context Runtime

`TurnStats` 只属于展示/审计，不参与上下文拼装。

---

## 13. 最终结论

针对未来动态上下文记忆折叠、AI 自主治理、provider cache 稳定性以及请求审计的需求，  
最佳方向不是继续在现有 transcript/canonical message 基础上打补丁，而是正式把会话系统拆成：

1. **Transcript Event Log：只负责显示与审计**
2. **Context Runtime State：只负责模型活动上下文**
3. **Request Snapshot：只负责每次真实请求留痕**

这样才能保证：

- UI 怎么折叠都不会污染上下文
- 上下文怎么变化都不会破坏历史展示
- 任意一次请求都可以准确审计
- 后续 AI 动态记忆治理有明确落点

这是后续 context memory / memory mesh / durable compaction / prompt cache 稳定化的共同基础。
