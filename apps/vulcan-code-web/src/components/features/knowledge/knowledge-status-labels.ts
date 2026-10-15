import type { TranslationKey } from "@/i18n";

// Durable operation codes are produced by operations.rs and vector.rs.
// 持久操作代码由 operations.rs 及 vector.rs 产出。
const OPERATION_LABELS: Readonly<Record<string, TranslationKey>> = {
  queued: "knowledge.operation.queued", running: "knowledge.operation.running", completed: "knowledge.operation.completed", cancelled: "knowledge.operation.cancelled", failed: "knowledge.operation.failed",
};
// Content change codes are produced by content.rs and package_content.rs.
// 内容变更代码由 content.rs 及 package_content.rs 产出。
const CHANGE_LABELS: Readonly<Record<string, TranslationKey>> = {
  create: "knowledge.change.create", update: "knowledge.change.update", edit: "knowledge.change.edit", move: "knowledge.change.move", delete: "knowledge.change.delete", folder: "knowledge.change.folder", delete_folder: "knowledge.change.deleteFolder", ingest: "knowledge.change.ingest", restore: "knowledge.change.restore", import: "knowledge.change.import",
};

// Localizes a declared durable operation state, preserving unknown codes in caller diagnostics.
// 本地化声明持久操作状态，未知代码保留在调用方诊断。
// Parameters: state is the service code; t resolves a catalog key. Returns the localized state label.
// 参数：state 为服务代码；t 解析目录键。返回本地化状态标签。
export function knowledgeOperationLabel(state: string, t: (key: TranslationKey) => string): string {
  return t(Object.hasOwn(OPERATION_LABELS, state) ? OPERATION_LABELS[state] : "knowledge.operation.unknown");
}

// Localizes a declared content change without displaying raw implementation codes as product labels.
// 本地化声明内容变更，不将原始实现代码显示为产品标签。
// Parameters: operation is the immutable change code; t resolves a catalog key. Returns its localized action label.
// 参数：operation 为不可变变更代码；t 解析目录键。返回本地化动作标签。
export function knowledgeChangeLabel(operation: string, t: (key: TranslationKey) => string): string {
  return t(Object.hasOwn(CHANGE_LABELS, operation) ? CHANGE_LABELS[operation] : "knowledge.operation.unknown");
}
