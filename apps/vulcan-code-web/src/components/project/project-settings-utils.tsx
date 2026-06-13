import { Bot, BrainCircuit, Database, FolderCog } from "lucide-react";
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
export function projectSettingsSections(project: ProjectSummary, workbench: WorkbenchSnapshot): ProjectSettingsSectionSummary[] {
  return [
    {
      id: "overview",
      icon: <FolderCog size={16} />,
      label: "概览",
      detail: project.pinned ? "已置顶项目" : "项目空间摘要",
    },
    {
      id: "vmm",
      icon: <BrainCircuit size={16} />,
      label: "VMM 记忆",
      detail: workbench.vmmMemory.projectDraft.hasOverride ? "项目覆盖" : "继承全局",
    },
    {
      id: "agent-skills",
      icon: <Bot size={16} />,
      label: "Agent Skills",
      detail: `${workbench.agentSkills.skills.length} 个提示词技能`,
    },
    {
      id: "attachments",
      icon: <Database size={16} />,
      label: "附件缓存",
      detail: "历史附件清理",
    },
  ];
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
export function projectAgentSkillStateLabel(inheritedEnabled: boolean, stateKey: ProjectAgentSkillState): string {
  if (stateKey === "enabled") {
    return "项目启用";
  }
  if (stateKey === "disabled") {
    return "项目关闭";
  }
  return inheritedEnabled ? "继承启用" : "继承关闭";
}

// projectAgentSkillOptionLabel returns the segmented-control label for one project state.
// projectAgentSkillOptionLabel 返回单个项目状态的分段控件标签。
export function projectAgentSkillOptionLabel(stateKey: ProjectAgentSkillState, inheritedEnabled: boolean): string {
  if (stateKey === "inherit") {
    return inheritedEnabled ? "继承全局（启用）" : "继承全局（关闭）";
  }
  if (stateKey === "enabled") {
    return "启用";
  }
  return "关闭";
}

// projectAgentSkillSaveMessage returns the toast message for one project state save.
// projectAgentSkillSaveMessage 返回单个项目状态保存后的提示消息。
export function projectAgentSkillSaveMessage(name: string, stateKey: ProjectAgentSkillState): string {
  if (stateKey === "inherit") {
    return `已将 ${name} 改为继承全局设置`;
  }
  return stateKey === "enabled" ? `已在项目中启用 ${name}` : `已在项目中关闭 ${name}`;
}

// projectAttachmentKindLabel returns the localized label for an attachment kind.
// projectAttachmentKindLabel 返回附件类型的本地化标签。
export function projectAttachmentKindLabel(kind: keyof ProjectAttachmentCleanupKinds): string {
  switch (kind) {
    case "image":
      return "图片";
    case "audio":
      return "音频";
    case "video":
      return "视频";
    case "pdf":
      return "PDF";
  }
}

// projectAttachmentPreviewFallback builds a no-op cleanup preview for browser-only mode.
// projectAttachmentPreviewFallback 为纯浏览器模式构建无副作用清理预览。
export function projectAttachmentPreviewFallback(request: ProjectAttachmentCleanupRequest): ProjectAttachmentCleanupPreviewSummary {
  const selectedKinds = projectAttachmentSelectedKindLabels(request.selection);
  let summary = `预览模式将检查 ${request.cutoffDate} 之前的 ${selectedKinds.join("、")} 托管附件。`;
  if (!request.cutoffDate) {
    summary = "请选择有效的截止日期。";
  } else if (selectedKinds.length === 0) {
    summary = "至少选择一种附件类型后才能预览。";
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
export function projectAttachmentSelectedKindLabels(kinds: ProjectAttachmentCleanupKinds): string[] {
  return (Object.keys(kinds) as Array<keyof ProjectAttachmentCleanupKinds>)
    .filter((kind) => kinds[kind])
    .map(projectAttachmentKindLabel);
}
