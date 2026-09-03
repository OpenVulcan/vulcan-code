# VCE 工具面与 VulcanCode 对接契约

> 工具映射不是 VCE 的核心入口。实际业务调用应直接进入 `VceEngine` 的 `refresh`、`retrieve`、`management_snapshot`、`management_query`、`observe`、`remove` 和状态快照方法；本文件只约束 AI 可见面的薄映射。VCE 不依赖待移除的 CBM。

本文描述 VCE 为后续 VulcanCode 原生接入预置的工具映射。它只定义稳定的工具身份、Schema、
可见性和参数边界，不启动 MCP，不创建执行器，也不持有 LLM、Embedding、Rerank、VLDB、
LanceDB 或 SQLite。

## 1. 来源对照

OCE-client 的 MCP 层当前实际只有一个模型可见工具：`codebase-retrieval`。它接收
`information_request`，根据已配置工作区等待索引就绪，再调用客户端上下文的 `retrieve`。
`workspace_folder` 是客户端在多工作区下选择已授权根目录的运行参数，不是代码语义查询内容。

OCE 服务端提供 `find-missing`、`batch-upload`、`agents/blob-status`、
`agents/codebase-retrieval` 和 `checkpoint-blobs` 等应用边界；客户端还负责 `sync`、
`sync_paths`、`status`、`observe`、`remove` 和 `watch`。这些操作是同步、索引或生命周期控制，
不应全部投影为模型可见工具。

## 2. AI 可见工具

`VceToolCatalog::new().ai_visible_definitions()` 只返回一条定义：

| 字段 | 值 |
| --- | --- |
| canonical id | `codebase_retrieval` |
| descriptor name | `codebase-retrieval` |
| scope | `Workspace` |
| `ai_visible` | `true` |
| `available_in_plan` | `true` |
| 参数 | `information_request: string` |

模型可见 Schema 只允许以下参数：

```json
{
  "type": "object",
  "properties": {
    "information_request": {
      "type": "string",
      "description": "A description of the code information you need.",
      "minLength": 1
    }
  },
  "required": ["information_request"],
  "additionalProperties": false
}
```

工具名称解析同时接受 OCE 的 `codebase-retrieval` 和 Vulcan 的 `codebase_retrieval`，但目录只
保存一个规范定义，不注册两个 AI 工具。

## 3. 宿主专用操作

以下定义可以被上层调度器读取，用于实现索引生命周期，但 `ai_visible=false` 且
`available_in_plan=false`：

| VCE 操作 | OCE 来源 | 主要作用 |
| --- | --- | --- |
| `find_missing` | 服务端 `find-missing` | 区分 unknown 与 nonindexed Blob |
| `batch_upload` | 服务端 `batch-upload` | 对应 VCE 的有界 source-read、prepare、stage 流程，不直接公开上传端口 |
| `blob_status` | 服务端 `agents/blob-status` | 查询 Blob/Checkpoint 就绪状态 |
| `checkpoint_blobs` | 服务端 `checkpoint-blobs` | 推进工作区 Chain/Checkpoint |
| `sync` | 客户端 `WorkspaceContext.sync` | 全量 reconcile 并提交同步计划 |
| `sync_paths` | 客户端 `WorkspaceContext.sync_paths` | 路径级 reconcile 并提交同步计划 |
| `status` | 客户端 `WorkspaceIndexer.status` | 读取索引生命周期状态 |
| `observe` | 客户端 `WorkspaceContext.observe_file` | 写入未保存内容覆盖层 |
| `remove` | 客户端 `WorkspaceContext.remove_file` | 标记工作区相对路径删除 |
| `watch` | 客户端 `WorkspaceContext.start_watching` | 维护文件监听生命周期 |

宿主操作的 Schema 已按 OCE 当前 DTO 和客户端端口定义写入 `VceToolCatalog`，但其原始 JSON
载荷会在 `HostOperationRequest` 中保留，由上层存储/索引实现决定是否执行以及如何持久化。

## 4. VulcanCode 适配顺序

当前 `vulcan-code` 的工具链使用以下事实字段：

```text
VceToolDefinition
  -> VulcanToolDescriptor
       id / name / description / help_text
  -> VulcanToolCandidate
       parameters_schema / ai_visible / plan_policy / invoke
  -> VulcanToolsManager
       mode surface / invocation guard / dispatch
```

适配器应遵循以下规则：

1. 只从 `ai_visible_definitions()` 生成模型候选，因此只注册 `codebase_retrieval`。
2. 将 `available_in_plan=true` 映射为 Vulcan 的 `Visible` Plan 策略。
3. 在 `invoke` 闭包中解析 `VceToolCatalog::parse_ai_call()`，再把
   `NativeToolExecutionRequest.workspace_root` 转为宿主授权的工作区范围。
4. 宿主通过现有监听器形成 Full/Paths `RefreshRequest`，调用 VCE 的刷新入口；检索时只调用已提交 checkpoint，不在工具闭包复制同步逻辑。
5. 将检索结果转换为 `VulcanToolInvocationResult`；VCE 不直接依赖该类型，避免反向绑定
   `vulcan-tools-manager`。
6. 不把 `workspace_folder` 作为模型参数重新暴露。VCE 对模型提交该字段会返回显式的
   `HostManagedArgument` 错误，防止模型绕过宿主工作区授权。

## 5. 已预置与未预置

已预置：

- 稳定工具 ID、OCE 原始名称和单一公开入口；
- AI/Host-only 可见性和 Plan 策略字段；
- 模型 Schema 与 OCE 服务端/客户端生命周期 Schema；
- `information_request` 类型校验、空值校验和未知参数拒绝；
- OCE 名称到 Vulcan 名称的规范化解析；
- Host-only 原始载荷保留；
- 无重复 AI 工具面的回归测试。
- AI 调用上下文固定携带 registration/profile/vector-space/purpose，宿主索引固定携带 namespace/schema/checkpoint binding；
- VCE 功能入口已经实现源码端口、有界暂存、CAS 发布、严格回执和版本化状态恢复。

仍需上层完成：

- 将定义投影为 `VulcanToolCandidate` 并接入当前 `NativeToolCatalog`/工具准备链；
- 将当前工作区、项目和会话授权绑定到执行上下文；
- 将现有文件/Git watcher 和任务调度映射到 `RefreshRequest` 与状态持久化；
- 在 VLDB/SQLite 代码专属 namespace 上实现 `VceIndexPort` 与 `VceSourcePort`；
- 将 VCE 检索编排连接到 VulcanCode 统一注册的 LLM、Embedding 和 Rerank 调用；
- 把类型化管理查询投影到现有页面管理器，并为真正的修复/重建命令建立独立授权；
- 将模型可见结果投影到 Vulcan 的工具回执和上下文显示；
- 在真实 VulcanCode 运行时执行一次首个检索调用验收。

这些未完成项是宿主接入工作，不应通过在 VCE 内创建第二套 Provider、存储或 MCP 运行时来
填补。
