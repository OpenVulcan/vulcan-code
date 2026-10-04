// Browser projections follow Rust capacity management; the cross-language fixture verifies the wire contract.
// 浏览器投影遵循 Rust 容量管理；跨语言夹具验证线协议契约。

// PLUGIN_CAPACITY_PURPOSES lists the six declared host routes, not arbitrary plugin-created pools.
// PLUGIN_CAPACITY_PURPOSES 列出六个声明的宿主路由，而非插件任意创建的池。
export const PLUGIN_CAPACITY_PURPOSES = ["tool_call", "tool_session", "hook", "ui_action", "lifecycle", "activation_probe"] as const;
// PluginCapacityPurpose is one exact serialized host purpose.
// PluginCapacityPurpose 为单个精确序列化宿主用途。
export type PluginCapacityPurpose = typeof PLUGIN_CAPACITY_PURPOSES[number];

// PluginCapacityConfig preserves the complete core configuration when replacing an override.
// PluginCapacityConfig 在替换覆盖时保留完整核心配置。
export interface PluginCapacityConfig {
  // resources is the sole native VM reservation and execution policy.
  // resources 是唯一原生 VM 预留及执行策略。
  resources: {
    // kind selects shared ownership or a dedicated guarantee.
    // kind 选择公共归属或专用保证。
    kind: "shared" | "dedicated";
    // min_resident_vms is reserved capacity, not a prewarm count.
    // min_resident_vms 为预留容量，而非预热数量。
    min_resident_vms: number;
    // max_resident_vms includes creating and retiring instances.
    // max_resident_vms 包含创建及退役实例。
    max_resident_vms: number;
    // max_running_calls bounds concurrent native execution.
    // max_running_calls 限制并发原生执行。
    max_running_calls: number;
  };
  // max_queued_calls bounds accepted queued operations.
  // max_queued_calls 限制已接纳排队操作。
  max_queued_calls: number;
  // max_queued_bytes bounds exact queued input bytes.
  // max_queued_bytes 限制精确排队输入字节数。
  max_queued_bytes: number;
}

// PluginCapacityRegistration retains the original caller intent through uncertain transport results.
// PluginCapacityRegistration 跨不确定传输结果保留原调用意图。
export interface PluginCapacityRegistration {
  // registrationId is generated once per explicit registration intent.
  // registrationId 为每个明确登记意图仅生成一次。
  registrationId: string;
  // runtimeId selects the exact observed native parent.
  // runtimeId 选择精确观测原生父级。
  runtimeId: string;
  // expectedRegistryRevision represents observed absence, never automatically refreshed.
  // expectedRegistryRevision 表示观测缺失，绝不自动刷新。
  expectedRegistryRevision: string;
  // pluginId names the installed owner.
  // pluginId 指定已安装所有者。
  pluginId: string;
  // purpose selects the original route.
  // purpose 选择原路由。
  purpose: PluginCapacityPurpose;
}

// PluginCapacityPredecessor authorizes only the originally observed capacity and policy.
// PluginCapacityPredecessor 仅授权原观测容量及策略。
export interface PluginCapacityPredecessor {
  // runtimeId fences replacement parents.
  // runtimeId 阻止替代父级。
  runtimeId: string;
  // capacityId fences replacement capacity registrations.
  // capacityId 阻止替代容量登记。
  capacityId: string;
  // expectedRevision is opaque and remains unchanged throughout one draft.
  // expectedRevision 为不透明值，在单个草稿全程保持不变。
  expectedRevision: string;
  // pluginId is the exact semantic owner.
  // pluginId 为精确语义所有者。
  pluginId: string;
  // purpose is the observed host route.
  // purpose 为观测宿主路由。
  purpose: PluginCapacityPurpose;
}

// PluginCapacityUpdate is a full replacement or an explicit inheritance reset.
// PluginCapacityUpdate 为完整替换或明确继承重置。
export interface PluginCapacityUpdate extends PluginCapacityPredecessor {
  // updateId remains queryable independently of the submitting view.
  // updateId 独立于提交视图保持可查询。
  updateId: string;
  // config must be present; null explicitly removes the override.
  // config 必须存在；空值明确移除覆盖。
  config: PluginCapacityConfig | null;
}

// PluginCapacityReceipt is the exact durable transaction checkpoint, not a predicted result.
// PluginCapacityReceipt 为精确持久事务检查点，而非预测结果。
export interface PluginCapacityReceipt {
  // request retains the original complete intent.
  // request 保留原始完整意图。
  request: PluginCapacityUpdate;
  // phase distinguishes native application from durable acknowledgement.
  // phase 区分原生应用及持久确认。
  phase: "prepared" | "applied" | "committed" | "rejected" | "interrupted";
  // appliedRevision exists only after acknowledged native application.
  // appliedRevision 仅在原生应用已确认后存在。
  appliedRevision: string | null;
  // error retains the original refusal or recovery diagnostic.
  // error 保留原拒绝或恢复诊断。
  error: string | null;
  // persistenceError is independently retained when disk publication needs recovery.
  // persistenceError 在磁盘发布需要恢复时独立保留。
  persistenceError?: string;
}

// PluginVmUsage contains the actual resident lifecycle counts used by the management view.
// PluginVmUsage 包含管理视图使用的实际常驻生命周期计数。
export interface PluginVmUsage {
  // resident includes every charged allocation.
  // resident 包含每个记账分配。
  resident: number;
  // creating is initialization still in progress.
  // creating 为仍在进行的初始化。
  creating: number;
  // idle is initialized without execution.
  // idle 为已初始化但未执行。
  idle: number;
  // running is actual held execution capacity.
  // running 为实际持有执行容量。
  running: number;
  // retiring remains charged until cleanup completes.
  // retiring 在清理完成前保持记账。
  retiring: number;
}

// PluginCapacityStatus projects native metadata; unknown read-only fields need not be copied into drafts.
// PluginCapacityStatus 投影原生元数据；未知只读字段无需复制进草稿。
export interface PluginCapacityStatus {
  // managementRequest identifies an explicit registration, or null for host/business registration.
  // managementRequest 标识明确登记；宿主或业务登记时为空值。
  managementRequest: PluginCapacityRegistration | null;
  // runtimeId owns this exact capacity.
  // runtimeId 拥有此精确容量。
  runtimeId: string;
  // policyRevision is the current opaque predecessor.
  // policyRevision 为当前不透明前驱。
  policyRevision: string;
  // pendingConvergence means a committed shrink still has physical work remaining.
  // pendingConvergence 表示已提交缩容仍有物理工作未完成。
  pendingConvergence: boolean;
  // purpose identifies the host routing domain.
  // purpose 标识宿主路由域。
  purpose: PluginCapacityPurpose;
  // budgetSource identifies inherited or explicit configuration.
  // budgetSource 标识继承或明确配置。
  budgetSource: "plugin_budget" | "host_override";
  // preparingMembers preserves work before native pool publication.
  // preparingMembers 保留原生池发布前的工作。
  preparingMembers: number;
  // retirementRequested identifies unfinished plugin lifecycle reclamation.
  // retirementRequested 标识尚未完成的插件生命周期回收。
  retirementRequested: boolean;
  // snapshot preserves native snake_case field names.
  // snapshot 保留原生下划线字段名。
  snapshot: {
    // capacity_id is the never-rebound native identity.
    // capacity_id 为绝不重新绑定的原生身份。
    capacity_id: string;
    // plugin_id owns this capacity.
    // plugin_id 拥有此容量。
    plugin_id: string;
    // config is the complete effective policy.
    // config 为完整生效策略。
    config: PluginCapacityConfig;
    // resources reports actual VM usage.
    // resources 报告实际 VM 使用。
    resources: PluginVmUsage;
    // committed_resident_vms includes the non-lendable reservation.
    // committed_resident_vms 包含不可出借预留。
    committed_resident_vms: number;
    // active_operations remain charged through cleanup.
    // active_operations 在清理期间仍记账。
    active_operations: number;
    // queued_calls counts accepted waiting work.
    // queued_calls 统计已接纳等待工作。
    queued_calls: number;
    // queued_bytes measures exact waiting input.
    // queued_bytes 衡量精确等待输入。
    queued_bytes: number;
    // retained_pools includes members awaiting acknowledgement.
    // retained_pools 包含等待确认的成员。
    retained_pools: number;
    // closing permanently prevents new admission.
    // closing 永久阻止新入场。
    closing: boolean;
  };
}

// PluginCapacityCatalog is one coherent host mapping observation, with independently sampled physical counters.
// PluginCapacityCatalog 为一致宿主映射观测，物理计数分别采样。
export interface PluginCapacityCatalog {
  // runtimeId owns all entries.
  // runtimeId 拥有全部条目。
  runtimeId: string;
  // registryRevision changes whenever a mapping is inserted or removed.
  // registryRevision 在映射插入或移除时变化。
  registryRevision: string;
  // capacities contains only actual retained owners.
  // capacities 仅包含真实保留所有者。
  capacities: PluginCapacityStatus[];
}

// PluginRuntimeView is a read-only projection; the browser never writes whole runtime settings.
// PluginRuntimeView 为只读投影；浏览器绝不写入整个运行时设置。
export interface PluginRuntimeView {
  // runtimeId identifies the current native parent.
  // runtimeId 标识当前原生父级。
  runtimeId: string;
  // policyUpdate exposes the globally retained receipt across page reloads.
  // policyUpdate 跨页面重载暴露全局保留回执。
  policyUpdate: PluginCapacityReceipt | null;
  // resources reports actual parent VM usage independently of individual capacity samples.
  // resources 独立于各容量采样报告实际父级 VM 使用。
  resources: PluginVmUsage;
}
