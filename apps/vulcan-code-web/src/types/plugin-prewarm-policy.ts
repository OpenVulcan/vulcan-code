// PluginAutomaticPrewarmRule declares a desired tool target; execution resolves its current generation and workspace.
// PluginAutomaticPrewarmRule 声明期望工具目标；执行时解析当前代次及工作区。
export interface PluginAutomaticPrewarmRule {
  // pluginId identifies the exact declaring package.
  // pluginId 标识精确声明包。
  pluginId: string;
  // toolId is the globally unique declared tool identifier.
  // toolId 为全局唯一声明工具标识。
  toolId: string;
  // targetIdle is positive; an empty rule collection disables automatic prewarming.
  // targetIdle 必须为正数；空规则集合关闭自动预热。
  targetIdle: number;
}

// PluginPrewarmPolicyUpdate freezes the caller UUID and the exact observed policy predecessor.
// PluginPrewarmPolicyUpdate 冻结调用方 UUID 及精确已观测策略前驱。
export interface PluginPrewarmPolicyUpdate {
  // updateId remains stable across uncertain responses and explicit recovery.
  // updateId 在不确定响应及明确恢复期间保持稳定。
  updateId: string;
  // runtimeId identifies the original native parent.
  // runtimeId 标识原原生父级。
  runtimeId: string;
  // expectedRevision is never refreshed to force a stale request through.
  // expectedRevision 绝不通过刷新来强行接受陈旧请求。
  expectedRevision: string;
  // rules replaces the entire collection, preserving explicit removal intent.
  // rules 替换完整集合，保留明确移除意图。
  rules: PluginAutomaticPrewarmRule[];
}

// PluginPrewarmPolicyPhase follows the exact host publication checkpoints.
// PluginPrewarmPolicyPhase 遵循精确宿主发布检查点。
export type PluginPrewarmPolicyPhase = "prepared" | "publishing" | "committed" | "cancelled" | "interrupted";

// PluginPrewarmPolicyReceipt concerns settings publication only, never execution completion.
// PluginPrewarmPolicyReceipt 仅涉及设置发布，绝不表示执行完成。
export interface PluginPrewarmPolicyReceipt {
  // request retains the original transaction without substituting a newer parent.
  // request 保留原事务，不替换为较新父级。
  request: PluginPrewarmPolicyUpdate;
  // phase distinguishes unresolved publication from durable terminal evidence.
  // phase 区分未决发布与持久终态证据。
  phase: PluginPrewarmPolicyPhase;
  // resultRevision is frozen once and becomes effective only after acknowledged commit.
  // resultRevision 一次冻结，仅在确认提交后生效。
  resultRevision: string;
  // error reports original lifecycle failure independently from transport errors.
  // error 报告原生命周期失败，独立于传输错误。
  error: string | null;
  // persistenceError is omitted when the file publication is acknowledged.
  // persistenceError 在文件发布确认后省略。
  persistenceError?: string;
}

// PluginPrewarmPolicyStatus is one atomic projection of effective rules and the retained transaction.
// PluginPrewarmPolicyStatus 为生效规则及保留事务的单个原子投影。
export interface PluginPrewarmPolicyStatus {
  // runtimeId is the current parent and can differ from a historical receipt's owner.
  // runtimeId 为当前父级，可与历史回执归属不同。
  runtimeId: string;
  // revision supplies the predecessor for a new explicit transaction.
  // revision 为新的明确事务提供前驱。
  revision: string;
  // rules contains effective settings, never an unacknowledged candidate.
  // rules 包含生效设置，绝不包含未确认候选。
  rules: PluginAutomaticPrewarmRule[];
  // update remains available until its exact terminal receipt is acknowledged.
  // update 保持可用，直至其精确终态回执被确认。
  update: PluginPrewarmPolicyReceipt | null;
  // capacityUpdateRetained blocks another settings transaction until capacity management resolves it.
  // capacityUpdateRetained 在容量管理处理完成前阻止另一设置事务。
  capacityUpdateRetained: boolean;
}
