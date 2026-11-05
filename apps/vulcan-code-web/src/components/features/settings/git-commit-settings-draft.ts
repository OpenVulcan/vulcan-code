import type { GitCommitSummaryModelSelectionSaveRequest } from "@/components/features/settings/settings-action-types"
import type { GitCommitSettingsSummary } from "@/types"

// DEFAULT_COMMIT_BRANCH_PREFIX mirrors the service default for empty Git commit settings.
// DEFAULT_COMMIT_BRANCH_PREFIX 对齐服务端空 Git 提交设置的默认值。
export const DEFAULT_COMMIT_BRANCH_PREFIX = "vulcan/"

// GitCommitSettingsDraft is the editable Git commit settings payload.
// GitCommitSettingsDraft 是可编辑的 Git 提交设置载荷。
export interface GitCommitSettingsDraft {
  // Default namespace prefix for new branches.
  // 新分支的默认命名空间前缀。
  newBranchPrefix: string
  // Commit-summary model selection.
  // 提交总结模型选择。
  summaryModel: GitCommitSummaryModelSelectionSaveRequest
  // User-authored extra prompt.
  // 用户额外提示词。
  summaryExtraPrompt: string
}

// commitSummaryModelDraft rebuilds a model selection with a fixed field order, so drafts compare by value
// whatever order the service serialized the snapshot in.
// commitSummaryModelDraft 以固定字段顺序重建模型选择，使草稿按值比较，不受服务端快照序列化顺序影响。
// Parameters: selection is a persisted or picked selection. Returns the canonical selection.
// 参数：selection 为已保存或刚选择的选择。返回值：规范形式的选择。
export function commitSummaryModelDraft(
  selection: GitCommitSummaryModelSelectionSaveRequest | undefined,
): GitCommitSummaryModelSelectionSaveRequest {
  if (selection?.mode !== "fixed") return { mode: "session" }
  return { mode: "fixed", providerId: selection.providerId, modelId: selection.modelId }
}

// gitCommitSettingsDraft builds the canonical editable draft from persisted settings.
// gitCommitSettingsDraft 从已保存设置构建规范的可编辑草稿。
// Parameters: settings is the snapshot value, absent before the first save. Returns the draft.
// 参数：settings 为快照取值，首次保存前缺省。返回值：草稿。
export function gitCommitSettingsDraft(settings: GitCommitSettingsSummary | undefined): GitCommitSettingsDraft {
  return {
    newBranchPrefix: settings?.newBranchPrefix ?? DEFAULT_COMMIT_BRANCH_PREFIX,
    summaryModel: commitSummaryModelDraft(settings?.summaryModel),
    summaryExtraPrompt: settings?.summaryExtraPrompt ?? "",
  }
}

// gitCommitSettingsDraftKey serializes a draft for change detection.
// gitCommitSettingsDraftKey 序列化草稿用于变化检测。
// Parameters: draft is any editable draft. Returns a value-equal key for value-equal drafts.
// 参数：draft 为任一可编辑草稿。返回值：值相等的草稿得到相同的键。
export function gitCommitSettingsDraftKey(draft: GitCommitSettingsDraft): string {
  return JSON.stringify({
    newBranchPrefix: draft.newBranchPrefix,
    summaryModel: commitSummaryModelDraft(draft.summaryModel),
    summaryExtraPrompt: draft.summaryExtraPrompt,
  })
}
