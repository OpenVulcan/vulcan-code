import type { PluginVmUsage } from "./plugin-runtime";

// PluginPrewarmOwner identifies the original parent and controller for every management operation.
// PluginPrewarmOwner 为每次管理操作标识原父级与控制器。
export interface PluginPrewarmOwner {
  // runtimeId is the exact native parent, never a replacement selected after reconnecting.
  // runtimeId 为精确原生父级，绝不使用重连后选中的替代父级。
  runtimeId: string;
  // ownerSessionId is the canonical host session UUID.
  // ownerSessionId 为规范宿主会话 UUID。
  ownerSessionId: string;
}

// PluginPrewarmTool describes one enabled reusable declaration from the host catalog.
// PluginPrewarmTool 描述宿主目录中的一个已启用复用声明。
export interface PluginPrewarmTool {
  // pluginId owns this tool declaration.
  // pluginId 拥有此工具声明。
  pluginId: string;
  // toolId is the globally unique declared tool identifier.
  // toolId 为全局唯一的声明工具标识。
  toolId: string;
  // pluginGeneration retains canonical decimal u64 identity without floating-point conversion.
  // pluginGeneration 保留规范十进制 u64 身份，不作浮点转换。
  pluginGeneration: string;
  // maxResidentVms is a declaration limit, not a reservation or current free capacity.
  // maxResidentVms 为声明上限，不是预留量或当前空闲容量。
  maxResidentVms: number;
}

// PluginPrewarmPreparation freezes the explicit target; the host alone resolves its workspace.
// PluginPrewarmPreparation 冻结明确目标；工作区仅由宿主解析。
export interface PluginPrewarmPreparation extends PluginPrewarmOwner, Omit<PluginPrewarmTool, "maxResidentVms"> {
  // targetIdle requests a confirmed idle count, including an explicitly selected zero target.
  // targetIdle 请求已确认空闲数量，允许零目标。
  targetIdle: number;
}

// PluginPrewarmIdentity addresses exactly one original receipt through cleanup and retirement.
// PluginPrewarmIdentity 在清理及退役期间定位唯一原回执。
export interface PluginPrewarmIdentity extends PluginPrewarmOwner {
  // invocationId remains unchanged from preparation onward.
  // invocationId 自准备起保持不变。
  invocationId: string;
}

// PluginPrewarmSnapshot retains the native historical completion observation, not live availability.
// PluginPrewarmSnapshot 保留原生历史完成观测，不代表实时可用量。
export interface PluginPrewarmSnapshot {
  // pool_id identifies the original native pool.
  // pool_id 标识原原生池。
  pool_id: string;
  // ready counts confirmed idle VMs at completion.
  // ready 为完成时确认的空闲 VM 数。
  ready: number;
  // unavailable counts VMs unavailable at that observation.
  // unavailable 为该观测时不可用的 VM 数。
  unavailable: number;
  // physical retains independently sampled native resource counters.
  // physical 保留独立采样的原生资源计数。
  physical: PluginVmUsage;
  // max_resident_vms is the native pool limit at observation.
  // max_resident_vms 为观测时原生池上限。
  max_resident_vms: number;
  // closing records whether admission was closing then.
  // closing 记录当时是否正在关闭入场。
  closing: boolean;
  // admission_blocked records the original admission state.
  // admission_blocked 记录原入场状态。
  admission_blocked: boolean;
}

// PluginPrewarmReceipt projects host lifecycle flags; opaque native evidence is display-only.
// PluginPrewarmReceipt 投影宿主生命周期标志；不透明原生证据仅供展示。
export interface PluginPrewarmReceipt extends PluginPrewarmPreparation, PluginPrewarmIdentity {
  // automatic identifies a host-owned lifecycle trigger that management may observe or cancel, but never start.
  // automatic 标识宿主拥有的生命周期触发；管理端可以观测或取消，但不能启动。
  automatic?: {
    // triggerId remains the original host lifecycle identity.
    // triggerId 保持为原宿主生命周期身份。
    triggerId: string;
    // policyRevision is the exact policy authorizing the original target.
    // policyRevision 为授权原目标的精确策略。
    policyRevision: string;
  };
  // workspaceRoot is the host-resolved frozen workspace, never sent back as authority.
  // workspaceRoot 为宿主解析并冻结的工作区，绝不回传作为权威输入。
  workspaceRoot: string;
  // started means the original input was transferred, not that initialization succeeded.
  // started 表示原输入已转移，不代表初始化成功。
  started: boolean;
  // retired confirms actual request cleanup completion.
  // retired 确认真实请求清理完成。
  retired: boolean;
  // requiresCleanupRecovery permits recovery of retained interrupted cleanup only.
  // requiresCleanupRecovery 仅允许恢复保留的中断清理。
  requiresCleanupRecovery: boolean;
  // succeeded distinguishes an original successful result from pending work.
  // succeeded 区分原成功结果与待处理工作。
  succeeded: boolean;
  // error is the original host diagnostic, separate from transport errors.
  // error 为原宿主诊断，与传输错误分离。
  error: string | null;
  // operation is retained verbatim for diagnostic display, never used to authorize actions.
  // operation 原样保留用于诊断展示，绝不用于授权操作。
  operation: Record<string, unknown> | null;
  // completed preserves earlier native operation evidence in original order.
  // completed 按原顺序保留此前原生操作证据。
  completed: Record<string, unknown>[];
  // satisfied is a historical confirmation and cannot promise future residency.
  // satisfied 为历史确认，不能保证未来常驻。
  satisfied: PluginPrewarmSnapshot | null;
}

// PluginPrewarmCatalog combines enabled choices and original receipts for one exact owner.
// PluginPrewarmCatalog 组合单个精确归属的已启用选项及原回执。
export interface PluginPrewarmCatalog extends PluginPrewarmOwner {
  // tools contains enabled reusable declarations from one immutable host snapshot.
  // tools 包含同一不可变宿主快照中的已启用复用声明。
  tools: PluginPrewarmTool[];
  // targets excludes ordinary business invocations and other controllers.
  // targets 排除普通业务调用及其他控制器。
  targets: PluginPrewarmReceipt[];
}
