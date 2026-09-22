import type { ReactNode } from "react";
import type { ProjectSummary } from "@/types";

// ProjectSettingsDialogTarget stores the project currently shown in the settings dialog.
// ProjectSettingsDialogTarget 保存当前显示设置弹窗的项目。
export interface ProjectSettingsDialogTarget {
  // Project being configured by the settings dialog.
  // 设置弹窗正在配置的项目。
  project: ProjectSummary;
  // Optional explicit navigation target supplied by a project-owned feature entry.
  // 项目自有功能入口提供的可选明确导航目标。
  initialSection?: ProjectSettingsSectionId;
}

export type { ProjectSettingsRevision } from "@/types";

// ProjectSettingsSectionId names the left-side categories in the project settings dialog.
// ProjectSettingsSectionId 命名项目设置弹窗左侧分类。
export type ProjectSettingsSectionId = "overview" | "vmm" | "code-context" | "knowledge-bases" | "agent-skills" | "attachments" | "collaboration-grants";

// ProjectSettingsSectionSummary describes one navigable settings category.
// ProjectSettingsSectionSummary 描述单个可导航的设置分类。
export interface ProjectSettingsSectionSummary {
  // Stable section identifier.
  // 稳定分类标识。
  id: ProjectSettingsSectionId;
  // Icon rendered in the left navigation.
  // 左侧导航中渲染的图标。
  icon: ReactNode;
  // Human-facing section label.
  // 面向用户的分类名称。
  label: string;
  // Compact section description.
  // 紧凑分类说明。
  detail: string;
}

// ProjectAttachmentCleanupKinds stores the project cleanup type filters.
// ProjectAttachmentCleanupKinds 保存项目清理类型筛选。
export interface ProjectAttachmentCleanupKinds {
  // Whether managed image attachments are included.
  // 是否包含托管图片附件。
  image: boolean;
  // Whether managed audio attachments are included.
  // 是否包含托管音频附件。
  audio: boolean;
  // Whether managed video attachments are included.
  // 是否包含托管视频附件。
  video: boolean;
  // Whether managed PDF attachments are included.
  // 是否包含托管 PDF 附件。
  pdf: boolean;
}

// ProjectAgentSkillState names one project-level Agent Skill tri-state.
// ProjectAgentSkillState 命名单个项目级 Agent Skill 三态。
export type ProjectAgentSkillState = "inherit" | "enabled" | "disabled";

// ProjectAttachmentCleanupRequest describes one project attachment cleanup command.
// ProjectAttachmentCleanupRequest 描述单个项目附件清理命令。
export interface ProjectAttachmentCleanupRequest {
  // Stable project identifier owning the cleanup command.
  // 拥有清理命令的稳定项目标识。
  projectId: string;
  // Local cutoff date in YYYY-MM-DD format.
  // YYYY-MM-DD 格式的本地截止日期。
  cutoffDate: string;
  // Attachment-kind filters selected by the user.
  // 用户选择的附件类型筛选。
  selection: ProjectAttachmentCleanupKinds;
}

// ProjectAttachmentCleanupPreviewSummary describes a cleanup preview returned by the service.
// ProjectAttachmentCleanupPreviewSummary 描述服务返回的清理预览。
export interface ProjectAttachmentCleanupPreviewSummary {
  // Cutoff date applied by the preview.
  // 预览使用的截止日期。
  cutoffDate: string;
  // Total matched attachment asset count.
  // 匹配的附件资产总数。
  matchedAssetCount: number;
  // Human-readable matched byte count.
  // 匹配字节数的人类可读文本。
  matchedByteLenLabel: string;
  // Unique session count impacted by matched assets.
  // 受匹配资产影响的唯一会话数量。
  impactedSessionCount: number;
  // Per-kind matched asset counts.
  // 按类型统计的匹配资产数。
  perKindCounts: Record<string, number>;
  // One-line user-facing summary.
  // 面向用户的一行摘要。
  summary: string;
}

// ProjectAttachmentCleanupReportSummary describes a cleanup execution report returned by the service.
// ProjectAttachmentCleanupReportSummary 描述服务返回的清理执行报告。
export interface ProjectAttachmentCleanupReportSummary {
  // Cutoff date applied by the execution.
  // 执行使用的截止日期。
  cutoffDate: string;
  // Total purged attachment asset count.
  // 被清理的附件资产总数。
  purgedAssetCount: number;
  // Total deleted managed file count.
  // 已删除的托管文件总数。
  deletedFileCount: number;
  // Total managed files already missing before cleanup.
  // 清理前已经缺失的托管文件总数。
  alreadyMissingFileCount: number;
  // Total managed files that could not be deleted.
  // 未能删除的托管文件总数。
  failedDeleteCount: number;
  // Human-readable purged byte count.
  // 清理字节数的人类可读文本。
  purgedByteLenLabel: string;
  // Unique session count impacted by purged assets.
  // 受清理资产影响的唯一会话数量。
  impactedSessionCount: number;
  // One-line user-facing summary.
  // 面向用户的一行摘要。
  summary: string;
}
