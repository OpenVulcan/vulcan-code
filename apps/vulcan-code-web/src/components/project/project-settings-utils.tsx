import { BookMarked, Bot, BrainCircuit, Database, FolderCog, Network, ShieldCheck, ShieldQuestion } from "lucide-react";
import type { TranslationKey } from "@/i18n";
import type { ProjectSummary, WorkbenchSnapshot } from "@/types";
import type {
  ProjectAgentSkillState,
  ProjectAttachmentCleanupKinds,
  ProjectAttachmentCleanupPreviewSummary,
  ProjectAttachmentCleanupRequest,
  ProjectSettingsSectionSummary,
} from "./project-settings-types";

// projectSettingsSections builds the left-side project settings navigation.
// projectSettingsSections 构建项目设置左侧导航。
// Parameters: project/workbench provide current data, t localizes labels, and hasVmmOverride comes from the exact dialog project's loaded draft.
// 参数：project/workbench 提供当前数据，t 本地化标签，hasVmmOverride 来自弹窗准确所属项目的已加载草稿。
// Returns: ordered settings sections; configuration access is independent from runtime tool admission.
// 返回值：有序设置分区；配置访问独立于运行时工具准入。
export function projectSettingsSections(
  project: ProjectSummary,
  workbench: WorkbenchSnapshot,
  t: (key: TranslationKey) => string,
  hasVmmOverride: boolean | null,
): ProjectSettingsSectionSummary[] {
  const sections: ProjectSettingsSectionSummary[] = [
    {
      id: "overview",
      icon: <FolderCog size={16} />,
      label: t("projectSettings.overview"),
      detail: project.pinned ? t("projectSettings.pinnedProject") : t("projectSettings.projectSpaceSummary"),
    },
    {
      id: "vmm",
      icon: <BrainCircuit size={16} />,
      label: t("projectSettings.vmmMemory"),
      detail: hasVmmOverride === null ? "-" : t(hasVmmOverride ? "projectSettings.projectOverride" : "projectSettings.inheritGlobal"),
    },
    {
      id: "knowledge-bases",
      icon: <BookMarked size={16} />,
      label: t("projectSettings.knowledgeBases"),
      detail: t("projectSettings.knowledgeBasesDetail"),
    },
    {
      id: "agent-skills",
      icon: <Bot size={16} />,
      label: t("projectSettings.agentTitle"),
      detail: t("projectSettings.promptSkillCount").replace("{count}", String(workbench.agentSkills.records.filter((skill) => skill.effective).length)),
    },
    {
      id: "attachments",
      icon: <Database size={16} />,
      label: t("projectSettings.attachmentCache"),
      detail: t("projectSettings.attachmentCleanup"),
    },
    {
      id: "collaboration-grants",
      icon: <ShieldCheck size={16} />,
      label: t("conversation.collaboration.timeline.grant"),
      detail: t("conversation.collaboration.status.granted"),
    },
  ];
  // The permission strategy exists only while the judge is enabled.
  // 权限策略仅在判定启用时存在。
  if (workbench.permissionJudgeBackend) {
    sections.push({
      id: "permission-judge",
      icon: <ShieldQuestion size={16} />,
      label: t("projectSettings.permissionJudge.title"),
      detail: t("projectSettings.permissionJudge.detail"),
    });
  }
  return sections;
}

// projectConversationCount returns the visible conversation count under one project.
// projectConversationCount 返回单个项目下的可见会话数量。
export function projectConversationCount(workbench: WorkbenchSnapshot, projectId: string): number {
  return workbench.conversations.filter((conversation) => conversation.projectId === projectId).length;
}

// projectAttachmentRoot returns the managed attachment directory shown for a project.
// projectAttachmentRoot 返回项目展示的托管附件目录。
export function projectAttachmentRoot(project: ProjectSummary): string {
  const separator = project.path.includes("\\") ? "\\" : "/";
  return `${project.path.replace(/[\\/]+$/, "")}${separator}.vulcan${separator}code${separator}attachments`;
}

// projectAgentSkillCurrentState normalizes one stored project Agent Skill override.
// projectAgentSkillCurrentState 归一化单个已存储的项目 Agent Skill 覆盖。
export function projectAgentSkillCurrentState(overrideState?: string): ProjectAgentSkillState {
  if (overrideState === "enabled" || overrideState === "disabled") {
    return overrideState;
  }
  return "inherit";
}

// projectAgentSkillStateLabel returns the status label for one Agent Skill row.
// projectAgentSkillStateLabel 返回单个 Agent Skill 行的状态标签。
export function projectAgentSkillStateLabel(inheritedEnabled: boolean, stateKey: ProjectAgentSkillState, t: (key: TranslationKey) => string): string {
  if (stateKey === "enabled") {
    return t("projectSettings.agent.projectEnabled");
  }
  if (stateKey === "disabled") {
    return t("projectSettings.agent.projectDisabled");
  }
  return inheritedEnabled ? t("projectSettings.agent.inheritEnabled") : t("projectSettings.agent.inheritDisabled");
}

// projectAgentSkillOptionLabel returns the segmented-control label for one project state.
// projectAgentSkillOptionLabel 返回单个项目状态的分段控件标签。
export function projectAgentSkillOptionLabel(stateKey: ProjectAgentSkillState, inheritedEnabled: boolean, t: (key: TranslationKey) => string): string {
  if (stateKey === "inherit") {
    return inheritedEnabled ? t("projectSettings.agent.optionInheritEnabled") : t("projectSettings.agent.optionInheritDisabled");
  }
  if (stateKey === "enabled") {
    return t("skills.action.enable");
  }
  return t("skills.label.closed");
}

// projectAgentSkillSaveMessage returns the toast message for one project state save.
// projectAgentSkillSaveMessage 返回单个项目状态保存后的提示消息。
export function projectAgentSkillSaveMessage(name: string, stateKey: ProjectAgentSkillState, t: (key: TranslationKey) => string): string {
  if (stateKey === "inherit") {
    return t("projectActions.skillInherited").replace("{skill}", name);
  }
  return (stateKey === "enabled" ? t("projectActions.skillEnabled") : t("projectActions.skillDisabled")).replace("{skill}", name);
}

// projectAttachmentKindLabel returns the localized label for an attachment kind.
// projectAttachmentKindLabel 返回附件类型的本地化标签。
export function projectAttachmentKindLabel(kind: keyof ProjectAttachmentCleanupKinds, t: (key: TranslationKey) => string): string {
  switch (kind) {
    case "image":
      return t("projectSettings.attachmentKind.image");
    case "audio":
      return t("projectSettings.attachmentKind.audio");
    case "video":
      return t("projectSettings.attachmentKind.video");
    case "pdf":
      return "PDF";
  }
}

// projectAttachmentPreviewFallback builds a no-op cleanup preview for browser-only mode.
// projectAttachmentPreviewFallback 为纯浏览器模式构建无副作用清理预览。
export function projectAttachmentPreviewFallback(request: ProjectAttachmentCleanupRequest, t: (key: TranslationKey) => string): ProjectAttachmentCleanupPreviewSummary {
  const selectedKinds = projectAttachmentSelectedKindLabels(request.selection, t);
  let summary = t("projectSettings.cleanup.previewFallback").replace("{date}", request.cutoffDate).replace("{kinds}", selectedKinds.join(t("global.listDelimiter")));
  if (!request.cutoffDate) {
    summary = t("projectSettings.cleanup.invalidDate");
  } else if (selectedKinds.length === 0) {
    summary = t("projectSettings.cleanup.selectKind");
  }
  return {
    cutoffDate: request.cutoffDate,
    matchedAssetCount: 0,
    matchedByteLenLabel: "0 B",
    impactedSessionCount: 0,
    perKindCounts: {},
    summary,
  };
}

// projectAttachmentSelectedKindLabels returns selected cleanup-kind labels.
// projectAttachmentSelectedKindLabels 返回选中的清理类型标签。
export function projectAttachmentSelectedKindLabels(kinds: ProjectAttachmentCleanupKinds, t: (key: TranslationKey) => string): string[] {
  return (Object.keys(kinds) as Array<keyof ProjectAttachmentCleanupKinds>)
    .filter((kind) => kinds[kind])
    .map((kind) => projectAttachmentKindLabel(kind, t));
}
