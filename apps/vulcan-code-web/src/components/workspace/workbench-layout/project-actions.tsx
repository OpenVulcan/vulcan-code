import React from "react";
import type { SkillActionProps } from "@/components/features/skills/SkillsArea";
import type { RenameTarget } from "@/components/project/project-action-dialogs";
import { normalizeProjectIconSelection, type ProjectIconDialogTarget, type ProjectIconSelection } from "@/components/project/project-icons";
import { type ProjectAgentSkillState, type ProjectSettingsDialogTarget } from "@/components/project/project-settings-dialog";
import type { ProjectMenuAction } from "@/components/project/ProjectSidebar";
import type { DraftConversationSelection } from "@/components/workspace/workspace-resizable-body";
import type { AgentSkillsCatalogSummary, ProjectSummary, VmmConfigDraftSummary, WorkbenchSnapshot } from "@/types";
import {
  addProjectFromPath,
  openProjectDirectory,
  reorderProjects,
  resetProjectIcon,
  setExpandedTaskProjects,
  setProjectIcon,
  toggleProjectPinned,
} from "@/service/project-actions-service";
import { loadWorkbenchSnapshot } from "@/service/workbench-snapshot-service";
import {
  installLuaSkill,
  refreshAgentSkillsCatalog,
  refreshLuaSkillsCatalog,
  setAgentSkillEnabled,
  setAgentSkillProjectState,
  setAgentSkillsEnabled,
  setLuaSkillEnabled,
  setLuaSkillToolRegistrationState,
  uninstallLuaSkill,
  updateLuaSkill,
} from "@/service/skills-actions-service";
import { saveVmmMemoryConfig } from "@/service/memory-actions-service";
import { applyPreviewProjectAction, applyPreviewProjectIcon, applyPreviewProjectOrder } from "@/state/workbench-preview-actions";
import { useI18n, type TranslationKey } from "@/i18n";
import { toast } from "sonner";

// WorkbenchProjectActionSnapshotSource separates backend snapshots from local optimistic previews.
// WorkbenchProjectActionSnapshotSource 区分后端快照与本地乐观预览。
type WorkbenchProjectActionSnapshotSource = "service" | "local";

// WorkbenchProjectActionsOptions groups state and service bridges used by project-side workbench actions.
// WorkbenchProjectActionsOptions 归组项目侧工作台动作使用的状态与服务桥接。
interface WorkbenchProjectActionsOptions {
  // Current optimistic workbench snapshot.
  // 当前乐观工作台快照。
  workbenchState: WorkbenchSnapshot;
  // Updates the local workbench snapshot.
  // 更新本地工作台快照。
  setWorkbenchState: React.Dispatch<React.SetStateAction<WorkbenchSnapshot>>;
  // Merges a service snapshot with local subagent and hydration caches.
  // 将服务快照与本地子代理及水合缓存合并。
  mergeWorkbenchWithLocalCaches: (current: WorkbenchSnapshot, incoming: WorkbenchSnapshot) => WorkbenchSnapshot;
  // Reserves one service snapshot revision before applying side effects outside workbench state.
  // 在应用工作台状态之外的副作用前预占一个服务快照版本。
  acceptServiceWorkbenchSnapshot: (snapshot: WorkbenchSnapshot) => boolean;
  // Applies a local optimistic preview without enforcing service revision monotonicity.
  // 应用本地乐观预览，不执行服务版本单调门禁。
  applyLocalPreviewWithLocalCaches: (current: WorkbenchSnapshot, incoming: WorkbenchSnapshot) => WorkbenchSnapshot;
  // Clears or creates the active draft conversation.
  // 清理或创建当前草稿会话。
  setDraftConversation: React.Dispatch<React.SetStateAction<DraftConversationSelection | null>>;
  // Updates the selected conversation id.
  // 更新当前选中会话标识。
  setSelectedConversationId: React.Dispatch<React.SetStateAction<string | null>>;
  // Opens the rename dialog target.
  // 打开重命名弹窗目标。
  setRenameTarget: React.Dispatch<React.SetStateAction<RenameTarget | null>>;
  // Opens the project icon dialog target.
  // 打开项目图标弹窗目标。
  setProjectIconTarget: React.Dispatch<React.SetStateAction<ProjectIconDialogTarget | null>>;
  // Opens the project settings dialog target.
  // 打开项目设置弹窗目标。
  setProjectSettingsTarget: React.Dispatch<React.SetStateAction<ProjectSettingsDialogTarget | null>>;
  // Opens the project folder picker.
  // 打开项目文件夹选择器。
  setProjectFolderPickerOpen: React.Dispatch<React.SetStateAction<boolean>>;
  // Opens the archive-project confirmation target.
  // 打开归档项目确认目标。
  setArchiveProjectTarget: React.Dispatch<React.SetStateAction<ProjectSummary | null>>;
  // Opens the remove-project confirmation target.
  // 打开移除项目确认目标。
  setRemoveProjectTarget: React.Dispatch<React.SetStateAction<ProjectSummary | null>>;
  // Marks locally edited expanded project ids.
  // 标记本地已编辑的展开项目标识。
  expandedProjectIdsLocallyEditedRef: React.MutableRefObject<boolean>;
  // Applies a service-backed skill action snapshot.
  // 应用服务驱动的技能动作快照。
  applySkillWorkbenchAction: (action: () => Promise<WorkbenchSnapshot>) => Promise<void>;
  // Applies the latest agent skill catalog summary.
  // 应用最新 Agent Skill 目录摘要。
  applyAgentSkillsCatalog: (agentSkills: AgentSkillsCatalogSummary) => void;
  // Switches the workbench back to chat.
  // 将工作台切回聊天区域。
  handleOpenWorkspaceChat: () => void;
}

// useWorkbenchProjectActions owns project, project-settings, icon, order, and skill action callbacks.
// useWorkbenchProjectActions 持有项目、项目设置、图标、排序与技能动作回调。
export function useWorkbenchProjectActions(options: WorkbenchProjectActionsOptions) {
  // t resolves stable WebUI copy keys for the active language.
  // t 根据当前语言解析稳定的 WebUI 文案键。
  const { t } = useI18n();
  const {
    workbenchState,
    setWorkbenchState,
    mergeWorkbenchWithLocalCaches,
    acceptServiceWorkbenchSnapshot,
    applyLocalPreviewWithLocalCaches,
    setDraftConversation,
    setSelectedConversationId,
    setRenameTarget,
    setProjectIconTarget,
    setProjectSettingsTarget,
    setProjectFolderPickerOpen,
    setArchiveProjectTarget,
    setRemoveProjectTarget,
    expandedProjectIdsLocallyEditedRef,
    applySkillWorkbenchAction,
    applyAgentSkillsCatalog,
    handleOpenWorkspaceChat,
  } = options;
  // applyProjectActionSnapshot merges a project-menu operation result into the local workbench.
  // applyProjectActionSnapshot 将项目菜单操作结果合并进本地工作台。
  const applyProjectActionSnapshot = React.useCallback((
    snapshot: WorkbenchSnapshot | null,
    source: WorkbenchProjectActionSnapshotSource = "service",
  ) => {
    if (!snapshot) {
      return;
    }
    if (source === "service" && !acceptServiceWorkbenchSnapshot(snapshot)) {
      return;
    }
    const applySnapshot = source === "local" ? applyLocalPreviewWithLocalCaches : mergeWorkbenchWithLocalCaches;
    setWorkbenchState((current) => applySnapshot(current, snapshot));
    setDraftConversation(null);
    setSelectedConversationId((current) => {
      if (current && snapshot.conversations.some((conversation) => conversation.id === current)) {
        return current;
      }
      return snapshot.selectedConversationId ?? snapshot.conversations[0]?.id ?? null;
    });
  }, [
    acceptServiceWorkbenchSnapshot,
    applyLocalPreviewWithLocalCaches,
    mergeWorkbenchWithLocalCaches,
    setDraftConversation,
    setSelectedConversationId,
    setWorkbenchState,
  ]);

  // handleProjectMenuAction executes immediate project-tree context menu commands.
  // handleProjectMenuAction 执行可立即处理的项目树右键菜单命令。
  const handleProjectMenuAction = React.useCallback(async (action: ProjectMenuAction, project: ProjectSummary) => {
    try {
      if (action === "open-directory") {
        const message = await openProjectDirectory(project.id);
        toast.success(message ?? formatProjectActionTranslation(t, "projectActions.openRequested", { project: project.name }));
        return;
      }

      if (action === "rename") {
        setRenameTarget({ kind: "project", id: project.id, name: project.name });
        return;
      }

      if (action === "set-icon") {
        setProjectIconTarget({ project });
        return;
      }

      if (action === "archive-conversations") {
        setArchiveProjectTarget(project);
        return;
      }

      if (action === "remove-project") {
        setRemoveProjectTarget(project);
        return;
      }

      if (action === "settings") {
        setProjectSettingsTarget({ project });
        return;
      }

      const preview = applyPreviewProjectAction(workbenchState, project.id, action);
      applyProjectActionSnapshot(preview.snapshot, "local");
      await toggleProjectPinned(project.id);
      toast.success(formatProjectActionTranslation(
        t,
        project.pinned ? "projectActions.unpinned" : "projectActions.pinned",
        { project: project.name },
      ));
    } catch (nextError) {
      applyProjectActionSnapshot(workbenchState, "local");
      toast.error(nextError instanceof Error ? nextError.message : String(nextError));
    }
  }, [applyProjectActionSnapshot, t, workbenchState]);

  // handleProjectSettingsVmmSave persists the project-scoped VMM settings draft.
  // handleProjectSettingsVmmSave 持久化项目级 VMM 设置草稿。
  const handleProjectSettingsVmmSave = React.useCallback(async (projectId: string, draft: VmmConfigDraftSummary) => {
    await applySkillWorkbenchAction(() => saveVmmMemoryConfig({ scope: "project", projectId, draft }));
    toast.success(t("projectActions.vmmSaved"));
  }, [applySkillWorkbenchAction, t]);

  // handleProjectSettingsAgentSkillSave persists one project Agent Skill override.
  // handleProjectSettingsAgentSkillSave 持久化单个项目 Agent Skill 覆盖。
  const handleProjectSettingsAgentSkillSave = React.useCallback(async (projectId: string, name: string, stateKey: ProjectAgentSkillState) => {
    await setAgentSkillProjectState(projectId, name, stateKey);
    toast.success(projectAgentSkillSaveMessage(name, stateKey, t));
  }, [t]);

  // handleSetAgentSkillsEnabled persists the global Agent Skills switch with a lightweight catalog refresh.
  // handleSetAgentSkillsEnabled 使用轻量目录刷新持久化 Agent Skills 全局开关。
  const handleSetAgentSkillsEnabled = React.useCallback(async (enabled: boolean) => {
    const previousAgentSkills = workbenchState.agentSkills;
    applyAgentSkillsCatalog({
      ...previousAgentSkills,
      globalEnabled: enabled,
      skills: previousAgentSkills.skills.map((skill) => ({ ...skill, enabled: enabled && skill.enabled })),
    });
    try {
      applyAgentSkillsCatalog(await setAgentSkillsEnabled(enabled));
    } catch (error) {
      applyAgentSkillsCatalog(previousAgentSkills);
      throw error;
    }
  }, [applyAgentSkillsCatalog, workbenchState.agentSkills]);

  // handleSetAgentSkillEnabled persists one Agent Skill state with a lightweight catalog refresh.
  // handleSetAgentSkillEnabled 使用轻量目录刷新持久化单个 Agent Skill 状态。
  const handleSetAgentSkillEnabled = React.useCallback(async (name: string, enabled: boolean) => {
    const previousAgentSkills = workbenchState.agentSkills;
    applyAgentSkillsCatalog({
      ...previousAgentSkills,
      skills: previousAgentSkills.skills.map((skill) => (
        skill.name === name ? { ...skill, enabled: enabled && previousAgentSkills.globalEnabled } : skill
      )),
    });
    try {
      applyAgentSkillsCatalog(await setAgentSkillEnabled(name, enabled));
    } catch (error) {
      applyAgentSkillsCatalog(previousAgentSkills);
      throw error;
    }
  }, [applyAgentSkillsCatalog, workbenchState.agentSkills]);

  // skillActions groups service-backed skill management actions for every skills surface.
  // skillActions 为所有技能界面归组服务端技能管理动作。
  const skillActions = React.useMemo<SkillActionProps>(() => ({
    onRefreshLuaSkills: () => applySkillWorkbenchAction(refreshLuaSkillsCatalog),
    onInstallLuaSkill: (skillId, source, sourceType) => applySkillWorkbenchAction(() => installLuaSkill(skillId, source, sourceType)),
    onUpdateLuaSkill: (skillId) => applySkillWorkbenchAction(() => updateLuaSkill(skillId)),
    onSetLuaSkillEnabled: (skillId, enabled) => applySkillWorkbenchAction(() => setLuaSkillEnabled(skillId, enabled)),
    onUninstallLuaSkill: (skillId) => applySkillWorkbenchAction(() => uninstallLuaSkill(skillId)),
    onSetLuaSkillToolRegistrationState: (skillId, localName, registrationState) => (
      applySkillWorkbenchAction(() => setLuaSkillToolRegistrationState(skillId, localName, registrationState))
    ),
    onRefreshAgentSkills: () => applySkillWorkbenchAction(refreshAgentSkillsCatalog),
    onSetAgentSkillsEnabled: handleSetAgentSkillsEnabled,
    onSetAgentSkillEnabled: handleSetAgentSkillEnabled,
  }), [applySkillWorkbenchAction, handleSetAgentSkillEnabled, handleSetAgentSkillsEnabled]);

  // handleProjectIconConfirm persists the selected project icon override.
  // handleProjectIconConfirm 持久化选择的项目图标覆盖。
  const handleProjectIconConfirm = React.useCallback(async (project: ProjectSummary, icon: ProjectIconSelection) => {
    const normalizedIcon = normalizeProjectIconSelection(icon);
    try {
      applyProjectActionSnapshot(applyPreviewProjectIcon(workbenchState, project.id, normalizedIcon).snapshot, "local");
      await setProjectIcon(project.id, normalizedIcon);
      setProjectIconTarget(null);
      toast.success(formatProjectActionTranslation(t, "projectActions.iconUpdated", { project: project.name }));
    } catch (error) {
      applyProjectActionSnapshot(workbenchState, "local");
      toast.error(error instanceof Error ? error.message : String(error));
    }
  }, [applyProjectActionSnapshot, t, workbenchState]);

  // handleProjectIconReset clears the selected project icon override.
  // handleProjectIconReset 清除选择的项目图标覆盖。
  const handleProjectIconReset = React.useCallback(async (project: ProjectSummary) => {
    try {
      applyProjectActionSnapshot(applyPreviewProjectIcon(workbenchState, project.id, null).snapshot, "local");
      await resetProjectIcon(project.id);
      setProjectIconTarget(null);
      toast.success(formatProjectActionTranslation(t, "projectActions.iconReset", { project: project.name }));
    } catch (error) {
      applyProjectActionSnapshot(workbenchState, "local");
      toast.error(error instanceof Error ? error.message : String(error));
    }
  }, [applyProjectActionSnapshot, t, workbenchState]);

  // handleCreateConversation creates and selects a new empty conversation for a project.
  // handleCreateConversation 为项目创建并选中一个新的空白会话。
  const handleCreateConversation = React.useCallback(async (project: ProjectSummary) => {
    handleOpenWorkspaceChat();
    const draftId = `draft-${project.id}-${Date.now()}`;
    setDraftConversation({
      id: draftId,
      projectId: project.id,
      projectName: project.name,
    });
    setSelectedConversationId(draftId);
  }, [handleOpenWorkspaceChat]);

  // handleCreateProject opens the service-backed project folder picker.
  // handleCreateProject 打开服务驱动的项目文件夹选择器。
  const handleCreateProject = React.useCallback(() => {
    setProjectFolderPickerOpen(true);
  }, []);

  // handleConfirmProjectFolder adds the selected service-host directory as a project.
  // handleConfirmProjectFolder 将选中的服务宿主目录添加为项目。
  const handleConfirmProjectFolder = React.useCallback(async (path: string) => {
    const snapshot = await addProjectFromPath(path) ?? await loadWorkbenchSnapshot();
    applyProjectActionSnapshot(snapshot);
    toast.success(t("projectActions.listUpdated"));
  }, [applyProjectActionSnapshot, t]);

  // handleReorderProjects persists a same-section project order using the existing order model.
  // handleReorderProjects 使用现有顺序模型持久化同一分区内的项目排序。
  const handleReorderProjects = React.useCallback(async (projectIds: string[]) => {
    setWorkbenchState((current) => applyPreviewProjectOrder(current, projectIds).snapshot);
    try {
      const snapshot = await reorderProjects(projectIds);
      if (snapshot) {
        applyProjectActionSnapshot(snapshot);
      }
    } catch (nextError) {
      toast.error(nextError instanceof Error ? nextError.message : String(nextError));
      try {
        const snapshot = await loadWorkbenchSnapshot();
        applyProjectActionSnapshot(snapshot);
      } catch {
        // Keep the optimistic order when the refresh fallback is unavailable.
        // 刷新回退不可用时保留乐观排序结果。
      }
    }
  }, [applyProjectActionSnapshot]);

  // handleExpandedProjectIdsChange persists project folder expansion without reapplying stale snapshots.
  // handleExpandedProjectIdsChange 持久化项目文件夹展开状态，但不重新套用可能过期的快照。
  const handleExpandedProjectIdsChange = React.useCallback((projectIds: string[]) => {
    expandedProjectIdsLocallyEditedRef.current = true;
    setWorkbenchState((current) => ({ ...current, expandedProjectIds: projectIds }));
    void setExpandedTaskProjects(projectIds).catch((nextError) => {
      console.warn("persist expanded projects failed", nextError);
    });
  }, []);

  return {
    applyProjectActionSnapshot,
    handleProjectMenuAction,
    handleProjectSettingsVmmSave,
    handleProjectSettingsAgentSkillSave,
    handleSetAgentSkillsEnabled,
    handleSetAgentSkillEnabled,
    skillActions,
    handleProjectIconConfirm,
    handleProjectIconReset,
    handleCreateConversation,
    handleCreateProject,
    handleConfirmProjectFolder,
    handleReorderProjects,
    handleExpandedProjectIdsChange,
  };
}

// projectAgentSkillSaveMessage returns the localized toast message for one project skill override.
// projectAgentSkillSaveMessage 返回单个项目技能覆盖保存后的本地化提示消息。
function projectAgentSkillSaveMessage(
  // Agent Skill display name.
  // Agent Skill 展示名称。
  name: string,
  // Saved project override state.
  // 已保存的项目覆盖状态。
  stateKey: ProjectAgentSkillState,
  // Translates stable UI copy keys.
  // 翻译稳定 UI 文案键。
  t: (key: TranslationKey) => string,
): string {
  if (stateKey === "inherit") {
    return formatProjectActionTranslation(t, "projectActions.skillInherited", { skill: name });
  }
  return formatProjectActionTranslation(
    t,
    stateKey === "enabled" ? "projectActions.skillEnabled" : "projectActions.skillDisabled",
    { skill: name },
  );
}

// formatProjectActionTranslation replaces named placeholders in localized project action messages.
// formatProjectActionTranslation 替换项目动作本地化消息中的具名占位符。
function formatProjectActionTranslation(
  t: (key: TranslationKey) => string,
  key: TranslationKey,
  values: Record<string, string>,
): string {
  return Object.entries(values).reduce(
    (text, [name, value]) => text.replaceAll(`{${name}}`, value),
    t(key),
  );
}
