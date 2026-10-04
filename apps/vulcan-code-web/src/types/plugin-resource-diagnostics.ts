// PluginResourceKind follows the exact retained host registry, never inferred from native sessions.
// PluginResourceKind 遵循精确保留宿主注册表，绝不从原生会话推断。
export type PluginResourceKind = "pinned_session" | "formal_call" | "reusable_domain" | "hook_runtime" | "ui_action_runtime";
// PluginOperationPhase mirrors the native serialized OperationPhase.
// PluginOperationPhase 对应原生序列化 OperationPhase。
export type PluginOperationPhase = "queued" | "initializing" | "running" | "waiting_for_host" | "cleaning" | "succeeded" | "failed" | "cancelled";
// PluginResourceErrorCode mirrors EmbeddedErrorCode without exposing raw messages.
// PluginResourceErrorCode 对应 EmbeddedErrorCode，不暴露原始文案。
export type PluginResourceErrorCode = "invalid_argument" | "not_found" | "stale_generation" | "capacity_exceeded" | "busy" | "already_completed" | "closed" | "cancelled" | "deadline_exceeded" | "permission_denied" | "unsupported" | "execution_failed" | "cleanup_failed" | "internal";
// PluginResourceError preserves a source stage and nullable native code only.
// PluginResourceError 仅保留来源阶段及可空原生错误码。
export interface PluginResourceError {
  // stage names the original failure source.
  // stage 指定原失败来源。
  stage: string;
  // code is null for host errors without a native classification.
  // code 在宿主错误没有原生分类时为空。
  code: PluginResourceErrorCode | null;
}
// PluginResourceOperation exposes identity, phase and safe errors without business results or effects.
// PluginResourceOperation 暴露身份、阶段及安全错误，不包含业务结果或副作用。
export interface PluginResourceOperation {
  // operationId is the original core operation identity.
  // operationId 是原核心操作身份。
  operationId: string;
  // phase is the actual sampled native stage.
  // phase 是实际采样原生阶段。
  phase: PluginOperationPhase;
  // cancellationRequested expresses intent rather than completed retirement.
  // cancellationRequested 表示意图，而非已完成退役。
  cancellationRequested: boolean;
  // errors contains independent source stages.
  // errors 包含独立来源阶段。
  errors: PluginResourceError[];
}
// PluginResourceDiagnostic preserves the actual host and core identities of one retained resource.
// PluginResourceDiagnostic 保留单个保留资源的实际宿主与核心身份。
export interface PluginResourceDiagnostic {
  // resourceKind identifies the owning registry.
  // resourceKind 标识所属注册表。
  resourceKind: PluginResourceKind;
  // resourceId is the original registry identity.
  // resourceId 是原注册表身份。
  resourceId: string;
  // pluginGeneration is an exact decimal string to avoid browser rounding.
  // pluginGeneration 是精确十进制字符串，避免浏览器舍入。
  pluginGeneration: string;
  // ownerSessionId is the conversation owner; internal resources have no owner.
  // ownerSessionId 是会话所有者；内部资源没有所有者。
  ownerSessionId: string | null;
  // originalPoolId remains distinct from session and VM identities.
  // originalPoolId 与会话及 VM 身份保持区分。
  originalPoolId: string | null;
  // coreSessionId is never a conversation identifier.
  // coreSessionId 绝不是对话标识。
  coreSessionId: string | null;
  // corePhase is absent without a live core session snapshot.
  // corePhase 在没有实时核心会话快照时为空。
  corePhase: "opening" | "ready" | "running" | "closing" | "closed" | null;
  // activeOperationId is nullable when no native session operation is active.
  // activeOperationId 在没有活动原生会话操作时为空。
  activeOperationId: string | null;
  // finalizationOperationId identifies the original independent shutdown operation.
  // finalizationOperationId 标识原独立关闭操作。
  finalizationOperationId: string | null;
  // preparing records actual host preparation ownership.
  // preparing 记录实际宿主准备所有权。
  preparing: boolean;
  // closing records closure intent.
  // closing 记录关闭意图。
  closing: boolean;
  // retired records physical retirement rather than an operation's terminal phase.
  // retired 记录物理退役，而非操作的终态阶段。
  retired: boolean;
  // isDrainBlocking comes from the actual drain selection.
  // isDrainBlocking 来自实际排空选择。
  isDrainBlocking: boolean;
  // cleanupRecovery records unfinished physical cleanup after supervision ended.
  // cleanupRecovery 记录监督结束后未完成的物理清理。
  cleanupRecovery: boolean;
  // queuedCalls is null without a native session observation.
  // queuedCalls 在没有原生会话观测时为空。
  queuedCalls: number | null;
  // borrowers is present only for reusable domains.
  // borrowers 仅在复用域中存在。
  borrowers: number | null;
  // pendingFinalizations reports retained independent cleanup.
  // pendingFinalizations 报告保留独立清理。
  pendingFinalizations: boolean;
  // finalizationOperationIds retains original reusable shutdown identities.
  // finalizationOperationIds 保留原复用关闭身份。
  finalizationOperationIds: string[];
  // operations contains only whitelisted metadata.
  // operations 仅包含白名单元数据。
  operations: PluginResourceOperation[];
  // errors excludes raw diagnostic messages.
  // errors 排除原始诊断文案。
  errors: PluginResourceError[];
}
// PluginResourceDiagnostics reports one exact plugin and parent with independently sampled rows.
// PluginResourceDiagnostics 报告单个精确插件及父级，各行独立采样。
export interface PluginResourceDiagnostics {
  // runtimeId fences native parent replacement.
  // runtimeId 阻止原生父级替换。
  runtimeId: string;
  // serviceEpoch identifies the manager incarnation.
  // serviceEpoch 标识管理器实例。
  serviceEpoch: string;
  // pluginId names the exact requested namespace.
  // pluginId 指定精确请求命名空间。
  pluginId: string;
  // pluginGeneration is null for an unfiltered retained-resource read.
  // pluginGeneration 在未过滤保留资源读取时为空。
  pluginGeneration: string | null;
  // nonAtomicSnapshot explicitly describes the sampling boundary.
  // nonAtomicSnapshot 明确描述采样边界。
  nonAtomicSnapshot: boolean;
  // pinnedSessionCount counts physically unretired host pinned-tool records only.
  // pinnedSessionCount 仅统计宿主固定工具注册表中未物理退役的记录。
  pinnedSessionCount: number;
  // resources includes nonblocking terminal history as well as unfinished cleanup.
  // resources 包含不阻塞终态历史及未完成清理。
  resources: PluginResourceDiagnostic[];
}
// PluginUpdateDrainDiagnostics retains the original predecessor drain identity; null resources means unavailable.
// PluginUpdateDrainDiagnostics 保留原前驱排空身份；空资源表示不可观测。
export interface PluginUpdateDrainDiagnostics {
  // requestId is the original update request.
  // requestId 是原更新请求。
  requestId: string;
  // pluginId is the original plugin owner.
  // pluginId 是原插件所有者。
  pluginId: string;
  // serviceEpoch is the original drain manager incarnation.
  // serviceEpoch 是原排空管理器实例。
  serviceEpoch: string;
  // predecessorGeneration is the original predecessor route.
  // predecessorGeneration 是原前驱路由。
  predecessorGeneration: string;
  // drainingGeneration is the original catalog drain transition, encoded as an exact decimal string.
  // drainingGeneration 是原目录排空转换代次，编码为精确十进制字符串。
  drainingGeneration: string;
  // resources is unavailable when the original drain owner cannot be observed.
  // resources 在原排空所有者无法观测时不可用。
  resources: PluginResourceDiagnostics | null;
}
// PluginUninstallDrainDiagnostics retains the original reservation's generation and exact request.
// PluginUninstallDrainDiagnostics 保留原预留代次及精确请求。
export interface PluginUninstallDrainDiagnostics {
  // requestId is the original uninstall request.
  // requestId 是原卸载请求。
  requestId: string;
  // pluginId is the original plugin namespace.
  // pluginId 是原插件命名空间。
  pluginId: string;
  // serviceEpoch is the reservation's manager incarnation.
  // serviceEpoch 是预留所属管理器实例。
  serviceEpoch: string;
  // pluginGeneration is the original reserved descriptor generation.
  // pluginGeneration 是原预留描述符代次。
  pluginGeneration: string;
  // resources is null when original live drain diagnostics are unavailable.
  // resources 在原实时排空诊断不可用时为空。
  resources: PluginResourceDiagnostics | null;
}
// PluginDiagnosticsSelection freezes the exact plugin or original management request for read-only navigation.
// PluginDiagnosticsSelection 为只读导航固定精确插件或原管理请求。
export type PluginDiagnosticsSelection =
  | { kind: "plugin"; pluginId: string }
  | { kind: "update" | "uninstall"; pluginId: string; requestId: string };
