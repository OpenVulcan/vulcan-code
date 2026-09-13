import type { TranslationKey } from "@/i18n-messages";

// formatKnowledgeIndexStatus labels states written by the knowledge registry without inventing readiness.
// formatKnowledgeIndexStatus 为知识库注册表实际写入的状态提供文案，不推测是否就绪。
// Parameters: status is the registry's open string contract; t resolves the current locale.
// 参数：status 是注册表的开放字符串契约；t 按当前语言解析文案。
// Returns a short known-state label, or a dash for an unrecognized state retained in the caller's detail hint.
// 返回已知状态的简短文案；未知状态返回短横线，原始值由调用方的详情提示保留。
export function formatKnowledgeIndexStatus(status: string, t: (key: TranslationKey) => string): string {
  // These states come from workspace-store knowledge_base/runtime/library.rs and fts.rs.
  // 这些状态来自 workspace-store 的 knowledge_base/runtime/library.rs 与 fts.rs。
  switch (status) {
    case "empty": return t("knowledge.indexStatus.empty");
    case "ready": return t("knowledge.indexStatus.ready");
    case "dirty": return t("knowledge.indexStatus.dirty");
    case "rebuilding": return t("knowledge.indexStatus.rebuilding");
    case "failed": return t("knowledge.indexStatus.failed");
    default: return "-";
  }
}
