import React from "react";
import type { AgentActionProps } from "@/components/features/agents/agents-area";
import type { SkillCatalogActionProps } from "@/components/features/skills/skill-area-types";
import type { AgentSkillActionProps } from "@/components/features/agent-skills/agent-skills-types";
import type { RenameTarget } from "@/components/project/project-action-dialogs";
import { normalizeProjectIconSelection, type ProjectIconDialogTarget, type ProjectIconSelection } from "@/components/project/project-icons";
import { type ProjectAgentSkillState, type ProjectSettingsDialogTarget } from "@/components/project/project-settings-dialog";
import type { ProjectMenuAction } from "@/components/project/project-sidebar";
import type { DraftConversationSelection } from "@/components/workspace/workspace-resizable-body";
import type { AgentSkillsCatalogUpdate, LuaSkillsCatalogUpdate, ProjectSummary, SubagentCatalogUpdate, VmmConfigDraftSummary, WorkbenchSnapshot } from "@/types";
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
  deleteSubagentDefinition,
  deleteLuaSkillConfig,
  deletePluginMcpSecret,
  installLuaSkill,
  installLocalPlugin,
  inspectLocalPlugin,
  describeLuaSkillConfig,
  deleteAgentSkill,
  deleteAgentSkillCategory,
  openAgentSkillsUserDirectory,
  openSubagentUserDirectory,
  refreshAgentSkillsCatalog,
  refreshLuaSkillsCatalog,
  pollLuaSkillConfigEvents,
  pluginEnvironmentReady,
  pluginEnvironmentInstallStatus,
  pluginEnvironmentInstallStatusForPlugin,
  preparePluginEnvironment,
  startPluginEnvironmentInstall,
  retryLuaSkillsManagedRuntimes,
  refreshSubagentCatalog,
  restoreSubagentDefinitionDefault,
  restoreAgentSkillDefault,
  saveAgentSkill,
  saveAgentSkillCategory,
  saveLuaSkillsRuntimeSettings,
  setLuaSkillConfig,
  saveSubagentDefinition,
  setAgentSkillEnabled,
  setAgentSkillCategory,
  setAgentSkillProjectState,
  setSubagentDefinitionEnabled,
  setLuaSkillEnabled,
  setVulcanPluginEnabled,
  setLuaSkillToolRegistrationState,
  setPluginMcpSecret,
  uninstallLuaSkill,
  uninstallUserPlugin,
  updateLocalPlugin,
  updateLuaSkill,
  validateLuaSkillConfig,
} from "@/service/skills-actions-service";
import { saveVmmMemoryConfig } from "@/service/memory-actions-service";
import { applyPreviewProjectIcon } from "@/state/workbench-preview-actions";
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
  // Applies one revisioned complete LuaSkills domain response.
  // 应用单个带版本的完整 LuaSkills 领域响应。
  applyLuaSkillsCatalogUpdate: (update: LuaSkillsCatalogUpdate) => void;
  // Applies one revisioned complete Agent Skills domain response.
  // 应用单个带版本的完整 Agent Skills 领域响应。
  applyAgentSkillsCatalogUpdate: (update: AgentSkillsCatalogUpdate) => void;
  // Applies one revisioned complete agent-catalog domain response.
  // 应用单个带版本的完整智能体目录领域响应。
  applySubagentCatalogUpdate: (update: SubagentCatalogUpdate) => void;
  // Switches the workbench back to chat.
  // 将工作台切回聊天区域。
  handleOpenWorkspaceChat: () => void;
}

// useWorkbenchProjectActions owns project, project-settings, icon, order, and skill action callbacks.
// useWorkbenchProjectActions 持有项目、项目设置、图标、排序与技能动作回调。
export function useWorkbenchProjectActions(options: WorkbenchProjectActionsOptions) {
  // t resolves stable WebUI copy keys for the active language.
  // t 根据当前语言解析稳定的 WebUI 文案键。
  const { t, locale } = useI18n();
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
    applyLuaSkillsCatalogUpdate,
    applyAgentSkillsCatalogUpdate,
    applySubagentCatalogUpdate,
    handleOpenWorkspaceChat,
  } = options;
  // applyProjectActionSnapshot merges a project-menu operation result into the local workbench.
  // applyProjectActionSnapshot 将项目菜单操作结果合并进本地工作台。
  const applyProjectActionSnapshot = React.useCallback((
    snapshot: WorkbenchSnapshot | null,
    source: WorkbenchProjectActionSnapshotSource = "service",
  ): boolean => {
    if (!snapshot) {
      return false;
    }
    if (source === "service" && !acceptServiceWorkbenchSnapshot(snapshot)) {
      return false;
    }
    const applySnapshot = source === "local" ? applyLocalPreviewWithLocalCaches : mergeWorkbenchWithLocalCaches;
    setWorkbenchState((current) => applySnapshot(current, snapshot));
    setDraftConversation(null);
    setSelectedConversationId((current) => {
      if (current && snapshot.conversations.some((conversation) => conversation.id === current)) {
        return current;
      }
      return snapshot.selectedConversationId ?? null;
    });
    return true;
  }, [
    acceptServiceWorkbenchSnapshot,
    applyLocalPreviewWithLocalCaches,
    mergeWorkbenchWithLocalCaches,
    setDraftConversation,
    setSelectedConversationId,
    setWorkbenchState,
  ]);

  // pinRequests prevents duplicate toggles while their authoritative state is still being read.
  // pinRequests 在读取权威状态期间阻止同一项目重复切换置顶。
  const pinRequests = React.useRef(new Set<string>());

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

      if (pinRequests.current.has(project.id)) return;
      pinRequests.current.add(project.id);
      try {
        await toggleProjectPinned(project.id);
        // The toggle RPC acknowledges no target value; only a revisioned read can establish its outcome.
        // 切换 RPC 不返回目标值；仅带版本的读取能确定其实际结果。
        try {
          const snapshot = await loadWorkbenchSnapshot();
          if (acceptServiceWorkbenchSnapshot(snapshot)) {
            setWorkbenchState((current) => mergeWorkbenchWithLocalCaches(current, snapshot));
          }
          toast.success(t("projectActions.listUpdated"));
        } catch (readError) {
          toast.warning(t("workspace.projectActions.thePinChangeWas"), {
            description: readError instanceof Error ? readError.message : String(readError),
          });
        }
      } finally {
        pinRequests.current.delete(project.id);
      }
    } catch (nextError) {
      toast.error(nextError instanceof Error ? nextError.message : String(nextError));
    }
  }, [acceptServiceWorkbenchSnapshot, locale, mergeWorkbenchWithLocalCaches, setArchiveProjectTarget, setProjectIconTarget, setProjectSettingsTarget, setRemoveProjectTarget, setRenameTarget, setWorkbenchState, t]);

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

  // runLuaSkillsCatalogAction executes one service action and applies its revisioned complete LuaSkills domain.
  // runLuaSkillsCatalogAction 执行单个服务动作并应用其带版本的完整 LuaSkills 领域。
  //
  // Parameters: action returns the committed LuaSkills domain response.
  // 参数：action 返回已提交的 LuaSkills 领域响应。
  // Returns: a promise that settles after the accepted domain response is scheduled locally.
  // 返回值：在已接受领域响应安排到本地状态后结束的 Promise。
  const runLuaSkillsCatalogAction = React.useCallback(async (action: () => Promise<LuaSkillsCatalogUpdate>) => {
    applyLuaSkillsCatalogUpdate(await action());
  }, [applyLuaSkillsCatalogUpdate]);

  // runAgentSkillsCatalogAction executes one service action and applies its revisioned complete Agent Skills domain.
  // runAgentSkillsCatalogAction 执行单个服务动作并应用其带版本的完整 Agent Skills 领域。
  //
  // Parameters: action returns the committed Agent Skills domain response.
  // 参数：action 返回已提交的 Agent Skills 领域响应。
  // Returns: a promise that settles after the accepted domain response is scheduled locally.
  // 返回值：在已接受领域响应安排到本地状态后结束的 Promise。
  const runAgentSkillsCatalogAction = React.useCallback(async (action: () => Promise<AgentSkillsCatalogUpdate>) => {
    applyAgentSkillsCatalogUpdate(await action());
  }, [applyAgentSkillsCatalogUpdate]);

  // runSubagentCatalogAction executes one service action and applies its revisioned complete agent-catalog domain.
  // runSubagentCatalogAction 执行单个服务动作并应用其带版本的完整智能体目录领域。
  //
  // Parameters: action returns the committed agent-catalog domain response.
  // 参数：action 返回已提交的智能体目录领域响应。
  // Returns: a promise that settles after the accepted domain response is scheduled locally.
  // 返回值：在已接受领域响应安排到本地状态后结束的 Promise。
  const runSubagentCatalogAction = React.useCallback(async (action: () => Promise<SubagentCatalogUpdate>) => {
    applySubagentCatalogUpdate(await action());
  }, [applySubagentCatalogUpdate]);

  // handleSetAgentSkillEnabled persists one Agent Skill state with a lightweight catalog refresh.
  // handleSetAgentSkillEnabled 使用轻量目录刷新持久化单个 Agent Skill 状态。
  const handleSetAgentSkillEnabled = React.useCallback(async (name: string, enabled: boolean) => {
    await runAgentSkillsCatalogAction(() => setAgentSkillEnabled(name, enabled));
  }, [runAgentSkillsCatalogAction]);

  // handleSetAgentDefinitionEnabled persists one agent state and applies the revisioned full catalog domain.
  // handleSetAgentDefinitionEnabled 持久化单个智能体状态并应用带版本的完整目录领域。
  const handleSetAgentDefinitionEnabled = React.useCallback(async (agentId: string, enabled: boolean) => {
    await runSubagentCatalogAction(() => setSubagentDefinitionEnabled(agentId, enabled));
  }, [runSubagentCatalogAction]);

  // luaSkillActions groups the independent LuaSkills lifecycle boundary.
  // luaSkillActions 归组独立 LuaSkills 生命周期边界。
  const luaSkillActions = React.useMemo<SkillCatalogActionProps>(() => ({
    onRefreshLuaSkills: () => runLuaSkillsCatalogAction(refreshLuaSkillsCatalog),
    onRetryLuaSkillsManagedRuntimes: (target) => runLuaSkillsCatalogAction(
      () => retryLuaSkillsManagedRuntimes(target),
    ),
    onSaveLuaSkillsRuntimeSettings: (settings) => runLuaSkillsCatalogAction(
      () => saveLuaSkillsRuntimeSettings(settings),
    ),
    onInstallLuaSkill: (skillId, source, sourceType) => runLuaSkillsCatalogAction(() => installLuaSkill(skillId, source, sourceType)),
    onUpdateLuaSkill: (skillId) => runLuaSkillsCatalogAction(() => updateLuaSkill(skillId)),
    onSetLuaSkillEnabled: (skillId, enabled) => runLuaSkillsCatalogAction(() => setLuaSkillEnabled(skillId, enabled)),
    onSetVulcanPluginEnabled: (pluginId, enabled) => runLuaSkillsCatalogAction(() => setVulcanPluginEnabled(pluginId, enabled)),
    onPluginEnvironmentReady: pluginEnvironmentReady,
    onPreparePluginEnvironment: preparePluginEnvironment,
    onStartPluginEnvironmentInstall: startPluginEnvironmentInstall,
    onPluginEnvironmentInstallStatus: pluginEnvironmentInstallStatus,
    onPluginEnvironmentInstallStatusForPlugin: pluginEnvironmentInstallStatusForPlugin,
    onInspectLocalPlugin: inspectLocalPlugin,
    onInstallLocalPlugin: (sourcePath, contentFingerprint, approvedPermissions, allowRegistryDependencies, initialSecrets) => runLuaSkillsCatalogAction(
      () => installLocalPlugin(sourcePath, contentFingerprint, approvedPermissions, allowRegistryDependencies, initialSecrets),
    ),
    onUpdateLocalPlugin: (sourcePath, contentFingerprint, approvedPermissions, allowRegistryDependencies, initialSecrets) => runLuaSkillsCatalogAction(
      () => updateLocalPlugin(sourcePath, contentFingerprint, approvedPermissions, allowRegistryDependencies, initialSecrets),
    ),
    onUninstallUserPlugin: (pluginId) => runLuaSkillsCatalogAction(() => uninstallUserPlugin(pluginId)),
    onSetPluginMcpSecret: (pluginId, serverId, secretKey, value) => runLuaSkillsCatalogAction(
      () => setPluginMcpSecret(pluginId, serverId, secretKey, value),
    ),
    onDeletePluginMcpSecret: (pluginId, serverId, secretKey) => runLuaSkillsCatalogAction(
      () => deletePluginMcpSecret(pluginId, serverId, secretKey),
    ),
    onUninstallLuaSkill: (skillId) => runLuaSkillsCatalogAction(() => uninstallLuaSkill(skillId)),
    onSetLuaSkillToolRegistrationState: (skillId, localName, registrationState) => (
      runLuaSkillsCatalogAction(() => setLuaSkillToolRegistrationState(skillId, localName, registrationState))
    ),
    onDescribeLuaSkillConfig: describeLuaSkillConfig,
    onValidateLuaSkillConfig: validateLuaSkillConfig,
    onSetLuaSkillConfig: setLuaSkillConfig,
    onDeleteLuaSkillConfig: deleteLuaSkillConfig,
    onPollLuaSkillConfigEvents: pollLuaSkillConfigEvents,
  }), [runLuaSkillsCatalogAction]);

  // agentSkillActions groups the independent prompt-based Agent Skills boundary.
  // agentSkillActions 归组独立提示词 Agent Skills 边界。
  const agentSkillActions = React.useMemo<AgentSkillActionProps>(() => ({
    onRefreshAgentSkills: () => runAgentSkillsCatalogAction(refreshAgentSkillsCatalog),
    onOpenAgentSkillsUserDirectory: () => runAgentSkillsCatalogAction(openAgentSkillsUserDirectory),
    onSetAgentSkillEnabled: handleSetAgentSkillEnabled,
    onSaveAgentSkill: (skill) => runAgentSkillsCatalogAction(() => saveAgentSkill(skill)),
    onDeleteAgentSkill: (name) => runAgentSkillsCatalogAction(() => deleteAgentSkill(name)),
    onRestoreAgentSkillDefault: (name) => runAgentSkillsCatalogAction(() => restoreAgentSkillDefault(name)),
    onSaveAgentSkillCategory: (category) => runAgentSkillsCatalogAction(() => saveAgentSkillCategory(category)),
    onDeleteAgentSkillCategory: (categoryId) => runAgentSkillsCatalogAction(() => deleteAgentSkillCategory(categoryId)),
    onSetAgentSkillCategory: (name, categoryId) => runAgentSkillsCatalogAction(() => setAgentSkillCategory(name, categoryId)),
  }), [handleSetAgentSkillEnabled, runAgentSkillsCatalogAction]);

  // agentActions groups service-backed independent agent management actions.
  // agentActions 归组独立智能体管理使用的服务端动作。
  const agentActions = React.useMemo<AgentActionProps>(() => ({
    onRefreshAgentCatalog: () => runSubagentCatalogAction(refreshSubagentCatalog),
    onOpenAgentUserDirectory: () => runSubagentCatalogAction(openSubagentUserDirectory),
    onSetAgentDefinitionEnabled: handleSetAgentDefinitionEnabled,
    onSaveAgentDefinition: (definition) => runSubagentCatalogAction(() => saveSubagentDefinition(definition)),
    onDeleteAgentDefinition: (agentId) => runSubagentCatalogAction(() => deleteSubagentDefinition(agentId)),
    onRestoreAgentDefinitionDefault: (agentId) => runSubagentCatalogAction(() => restoreSubagentDefinitionDefault(agentId)),
  }), [handleSetAgentDefinitionEnabled, runSubagentCatalogAction]);

  // handleProjectIconConfirm persists the selected project icon override.
  // handleProjectIconConfirm 持久化选择的项目图标覆盖。
  const handleProjectIconConfirm = React.useCallback(async (project: ProjectSummary, icon: ProjectIconSelection) => {
    const normalizedIcon = normalizeProjectIconSelection(icon);
    await setProjectIcon(project.id, normalizedIcon);
    // Apply the acknowledged icon to current state without rolling back unrelated workbench changes.
    // 将已确认图标应用到当前状态，不回滚无关工作台变化。
    setWorkbenchState(current => applyPreviewProjectIcon(current, project.id, normalizedIcon).snapshot);
    setProjectIconTarget(null);
    toast.success(formatProjectActionTranslation(t, "projectActions.iconUpdated", { project: project.name }));
  }, [setProjectIconTarget, setWorkbenchState, t]);

  // handleProjectIconReset clears the selected project icon override.
  // handleProjectIconReset 清除选择的项目图标覆盖。
  const handleProjectIconReset = React.useCallback(async (project: ProjectSummary) => {
    await resetProjectIcon(project.id);
    // The void RPC acknowledges persistence; only the exact icon fields are projected locally afterward.
    // void RPC 确认持久化完成；之后仅在本地投影精确图标字段。
    setWorkbenchState(current => applyPreviewProjectIcon(current, project.id, null).snapshot);
    setProjectIconTarget(null);
    toast.success(formatProjectActionTranslation(t, "projectActions.iconReset", { project: project.name }));
  }, [setProjectIconTarget, setWorkbenchState, t]);

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
    // The binding notice belongs only to this project-add response and must not be inferred later.
    // 绑定提示只属于本次项目添加响应，后续不得根据快照状态推断。
    const bindingNotice = snapshot.vmmMemory.projectBindingNotice;
    if (bindingNotice?.status === "bound") {
      toast.success(bindingNotice.message);
    } else if (bindingNotice?.status === "failed") {
      toast.error(bindingNotice.message);
    }
  }, [applyProjectActionSnapshot, t]);

  // handleReorderProjects persists a same-section project order using the existing order model.
  // handleReorderProjects 使用现有顺序模型持久化同一分区内的项目排序。
  const handleReorderProjects = React.useCallback(async (projectIds: string[]) => {
    try {
      const snapshot = await reorderProjects(projectIds);
      if (!snapshot) {
        throw new Error(t("workspace.projectActions.reorderingReturnedNoAuthoritative"));
      }
      if (acceptServiceWorkbenchSnapshot(snapshot)) {
        setWorkbenchState((current) => mergeWorkbenchWithLocalCaches(current, snapshot));
      }
    } catch (nextError) {
      toast.error(nextError instanceof Error ? nextError.message : String(nextError));
    }
  }, [acceptServiceWorkbenchSnapshot, locale, mergeWorkbenchWithLocalCaches, setWorkbenchState]);

  // handleExpandedProjectIdsChange persists project folder expansion without reapplying stale snapshots.
  // handleExpandedProjectIdsChange 持久化项目文件夹展开状态，但不重新套用可能过期的快照。
  const handleExpandedProjectIdsChange = React.useCallback((projectIds: string[]) => {
    expandedProjectIdsLocallyEditedRef.current = true;
    setWorkbenchState((current) => ({ ...current, expandedProjectIds: projectIds }));
    void setExpandedTaskProjects(projectIds).then((snapshot) => {
      if (!snapshot) throw new Error(t("workspace.projectActions.savingReturnedNoAcknowledged"));
    }).catch((nextError) => {
      toast.warning(t("workspace.projectActions.theExpandedViewIs"), {
        description: nextError instanceof Error ? nextError.message : String(nextError),
      });
    });
  }, [expandedProjectIdsLocallyEditedRef, locale, setWorkbenchState]);

  return {
    applyProjectActionSnapshot,
    handleProjectMenuAction,
    handleProjectSettingsVmmSave,
    handleProjectSettingsAgentSkillSave,
    handleSetAgentSkillEnabled,
    luaSkillActions,
    agentSkillActions,
    agentActions,
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
