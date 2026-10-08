import { formatMessage, translateActive } from "@/i18n";
import type {
  AgentSkillsCatalogUpdate,
  AgentSkillCategorySaveRequest,
  AgentSkillSaveRequest,
  LuaSkillsCatalogUpdate,
  LuaSkillConfigDeleteRequest,
  LuaSkillConfigDeleteResult,
  LuaSkillConfigDescriptor,
  LuaSkillConfigEventBatch,
  LuaSkillConfigStoreRefresh,
  LuaSkillConfigSetRequest,
  LuaSkillConfigSetResult,
  LuaSkillConfigValidation,
  LuaSkillsManagedRuntimeTarget,
  LuaSkillsRuntimeSettings,
  PluginUiModuleSource,
  PluginTextAssetSource,
  VulcanPluginCredentialChange,
  VulcanPluginInitialSecret,
  VulcanPluginPermissionPreview,
  SubagentCatalogUpdate,
  SubagentDefinitionSummary,
  SubagentDefinitionSaveRequest,
  WorkbenchSnapshot,
  StartupSnapshot,
} from "@/types";
import {
  BROWSER_PREVIEW_USER_VULCAN_ROOT,
  updateBrowserFallbackWorkbench,
} from "@/service/browser-fallback-workbench";
import {
  hasServiceRuntime,
  serviceClient,
} from "@/service/webui-service-runtime";
import { invalidatePersistentPluginUiModules } from "@/service/plugin-ui-module-runtime";
import { waitForPluginStartup } from "@/service/plugin-ui-startup";

// AgentSkillProjectState describes one project-level prompt skill override state.
// AgentSkillProjectState 描述单个项目级提示词技能覆盖状态。
export type AgentSkillProjectState = "inherit" | "enabled" | "disabled";

// luaSkillRegistrationLabel returns the preview label for one LuaSkill tool registration state.
// luaSkillRegistrationLabel 返回单个 LuaSkill 工具注册状态的预览标签。
function luaSkillRegistrationLabel(registrationState: string): string {
  if (registrationState === "explicit") {
    return "Explicit";
  }
  if (registrationState === "implicit") {
    return "Implicit";
  }
  return "Disabled";
}

// browserPreviewAgentDefinitionPath builds the user definition path shown by browser fallback data.
// browserPreviewAgentDefinitionPath 构建浏览器预览数据展示的用户定义路径。
function browserPreviewAgentDefinitionPath(userRoot: string, agentId: string, extension: "md" | "yaml"): string {
  return `${userRoot}/definitions/${agentId}.${extension}`;
}

// luaSkillsCatalogUpdateFromSnapshot projects one browser-preview workbench into the LuaSkills domain contract.
// luaSkillsCatalogUpdateFromSnapshot 将单个浏览器预览工作台投影为 LuaSkills 领域契约。
//
// Parameters: snapshot is the committed browser-preview workbench snapshot.
// 参数：snapshot 是已提交的浏览器预览工作台快照。
// Returns: the revisioned LuaSkills catalog and its derived plugin rows.
// 返回值：带版本的 LuaSkills 目录及其派生插件行。
function luaSkillsCatalogUpdateFromSnapshot(snapshot: WorkbenchSnapshot): LuaSkillsCatalogUpdate {
  return {
    revision: snapshot.revision ?? 0,
    luaSkills: snapshot.luaSkills,
    plugins: snapshot.plugins,
  };
}

// agentSkillsCatalogUpdateFromSnapshot projects one browser-preview workbench into the Agent Skills domain contract.
// agentSkillsCatalogUpdateFromSnapshot 将单个浏览器预览工作台投影为 Agent Skills 领域契约。
//
// Parameters: snapshot is the committed browser-preview workbench snapshot.
// 参数：snapshot 是已提交的浏览器预览工作台快照。
// Returns: the revisioned complete Agent Skills catalog.
// 返回值：带版本的完整 Agent Skills 目录。
function agentSkillsCatalogUpdateFromSnapshot(snapshot: WorkbenchSnapshot): AgentSkillsCatalogUpdate {
  return {
    revision: snapshot.revision ?? 0,
    agentSkills: snapshot.agentSkills,
  };
}

// subagentCatalogUpdateFromSnapshot projects one browser-preview workbench into the agent-catalog domain contract.
// subagentCatalogUpdateFromSnapshot 将单个浏览器预览工作台投影为智能体目录领域契约。
//
// Parameters: snapshot is the committed browser-preview workbench snapshot.
// 参数：snapshot 是已提交的浏览器预览工作台快照。
// Returns: the revisioned complete agent catalog.
// 返回值：带版本的完整智能体目录。
function subagentCatalogUpdateFromSnapshot(snapshot: WorkbenchSnapshot): SubagentCatalogUpdate {
  return {
    revision: snapshot.revision ?? 0,
    subagentCatalog: snapshot.subagentCatalog,
  };
}

// refreshLuaSkillsCatalog refreshes the Lua Skills catalog through the local service.
// refreshLuaSkillsCatalog 通过本地服务刷新 Lua Skills 目录。
export async function refreshLuaSkillsCatalog(): Promise<LuaSkillsCatalogUpdate> {
  if (!hasServiceRuntime()) {
    return luaSkillsCatalogUpdateFromSnapshot(updateBrowserFallbackWorkbench(() => undefined));
  }
  return serviceClient.request<LuaSkillsCatalogUpdate>("service_refresh_luaskills_catalog");
}

// describeLuaSkillConfig loads one host-safe effective package configuration descriptor.
// describeLuaSkillConfig 加载单个宿主安全的生效技能包配置描述。
//
// Parameters: skillId is the exact effective package identifier.
// 参数：skillId 是精确的生效技能包标识。
// Returns: one descriptor or null when the package declares no configuration.
// 返回值：一个配置描述，技能包未声明配置时返回 null。
export async function describeLuaSkillConfig(skillId: string): Promise<LuaSkillConfigDescriptor | null> {
  if (!hasServiceRuntime()) {
    return null;
  }
  return serviceClient.request<LuaSkillConfigDescriptor | null>(
    "service_describe_luaskill_config",
    { skillId },
  );
}

// validateLuaSkillConfig validates one effective package configuration.
// validateLuaSkillConfig 校验单个生效技能包配置。
//
// Parameters: skillId is the exact effective package identifier.
// 参数：skillId 是精确的生效技能包标识。
// Returns: the authoritative completeness and validity result.
// 返回值：权威的完整性与合法性结果。
export async function validateLuaSkillConfig(skillId: string): Promise<LuaSkillConfigValidation> {
  if (!hasServiceRuntime()) {
    throw new Error("LuaSkills package configuration requires the local service runtime.");
  }
  return serviceClient.request<LuaSkillConfigValidation>(
    "service_validate_luaskill_config",
    { skillId },
  );
}

// setLuaSkillConfig atomically updates one typed package configuration batch.
// setLuaSkillConfig 原子更新一组类型化技能包配置。
//
// Parameters: request contains the exact package id, observed revision, and changed values.
// 参数：request 包含精确技能包标识、已观察修订号与变更值。
// Returns: the host-safe mutation result and fresh descriptor after the CAS transaction.
// 返回值：CAS 事务后的宿主安全变更结果与最新描述。
export async function setLuaSkillConfig(
  request: LuaSkillConfigSetRequest,
): Promise<LuaSkillConfigSetResult> {
  if (!hasServiceRuntime()) {
    throw new Error("LuaSkills package configuration requires the local service runtime.");
  }
  return serviceClient.request<LuaSkillConfigSetResult>(
    "service_set_luaskill_config",
    { request },
  );
}

// deleteLuaSkillConfig deletes one explicit or orphaned package configuration value.
// deleteLuaSkillConfig 删除单个显式或遗留技能包配置值。
//
// Parameters: request contains the exact package id, key, and observed revision.
// 参数：request 包含精确技能包标识、配置键与已观察修订号。
// Returns: the host-safe deletion result and fresh descriptor after the CAS transaction.
// 返回值：CAS 事务后的宿主安全删除结果与最新描述。
export async function deleteLuaSkillConfig(
  request: LuaSkillConfigDeleteRequest,
): Promise<LuaSkillConfigDeleteResult> {
  if (!hasServiceRuntime()) {
    throw new Error("LuaSkills package configuration requires the local service runtime.");
  }
  return serviceClient.request<LuaSkillConfigDeleteResult>(
    "service_delete_luaskill_config",
    { request },
  );
}

// pollLuaSkillConfigEvents polls ordered configuration changes after one optional cursor.
// pollLuaSkillConfigEvents 轮询一个可选游标之后的有序配置变更。
//
// Parameters: afterSequence is the engine-local cursor returned by the previous call.
// 参数：afterSequence 是上一次调用返回的引擎内游标。
// Returns: a bounded event batch and the next cursor.
// 返回值：有界事件批次与下一游标。
export async function pollLuaSkillConfigEvents(
  afterSequence?: string | null,
): Promise<LuaSkillConfigEventBatch> {
  if (!hasServiceRuntime()) {
    return { events: [], nextSequence: afterSequence ?? "0" };
  }
  return serviceClient.request<LuaSkillConfigEventBatch>(
    "service_poll_luaskill_config_events",
    { afterSequence: afterSequence ?? null },
  );
}

// refreshLuaSkillConfig reloads package configuration stores from disk through the host.
// refreshLuaSkillConfig 通过宿主从磁盘重新载入技能包配置存储。
//
// Parameters: storeScope selects one exact store; null refreshes both configured stores.
// 参数：storeScope 选择一个精确存储；null 刷新两个已配置存储。
// Returns: one value-free refresh result per refreshed store.
// 返回值：每个已刷新存储对应一个不含值的刷新结果。
export async function refreshLuaSkillConfig(
  storeScope: string | null,
): Promise<LuaSkillConfigStoreRefresh[]> {
  if (!hasServiceRuntime()) {
    throw new Error("LuaSkills package configuration requires the local service runtime.");
  }
  return serviceClient.request<LuaSkillConfigStoreRefresh[]>(
    "service_refresh_luaskill_config",
    { request: { storeScope } },
  );
}

// retryLuaSkillsManagedRuntimes retries the packaged Python and Node bootstrap through the local service.
// retryLuaSkillsManagedRuntimes 通过本地服务重试随包 Python 与 Node 初始化。
// Parameters: target is the exact independently repairable runtime group selected by the user.
// 参数：target 是用户选择的精确且可独立修复的运行时组。
// Returns: the revisioned catalog containing fresh component validation states.
// 返回值：包含最新组件校验状态的带版本目录。
export async function retryLuaSkillsManagedRuntimes(
  target: LuaSkillsManagedRuntimeTarget,
): Promise<LuaSkillsCatalogUpdate> {
  if (!hasServiceRuntime()) {
    return luaSkillsCatalogUpdateFromSnapshot(updateBrowserFallbackWorkbench(() => undefined));
  }
  return serviceClient.request<LuaSkillsCatalogUpdate>(
    "service_retry_luaskills_managed_runtimes",
    { target },
  );
}

// saveLuaSkillsRuntimeSettings saves the Vulcan-owned LuaSkills startup document.
// saveLuaSkillsRuntimeSettings 保存 Vulcan 自有 LuaSkills 启动文档。
//
// Parameters: settings is the complete validated startup document shown by the settings page.
// 参数：settings 是设置页展示的完整待校验启动文档。
// Returns: the revisioned LuaSkills catalog including the restart-required result.
// 返回值：带版本的 LuaSkills 目录，其中包含是否需要重启的结果。
export async function saveLuaSkillsRuntimeSettings(
  settings: LuaSkillsRuntimeSettings,
): Promise<LuaSkillsCatalogUpdate> {
  if (!hasServiceRuntime()) {
    return luaSkillsCatalogUpdateFromSnapshot(updateBrowserFallbackWorkbench((snapshot) => {
      snapshot.luaSkills.runtimeSettings = structuredClone(settings);
      snapshot.luaSkills.restartRequired = true;
    }));
  }
  return serviceClient.request<LuaSkillsCatalogUpdate>(
    "service_save_luaskills_runtime_settings",
    { settings },
  );
}

// installLuaSkill installs one Lua Skill package through the local service.
// installLuaSkill 通过本地服务安装单个 Lua Skill 包。
export async function installLuaSkill(skillId: string, source: string, sourceType: string): Promise<LuaSkillsCatalogUpdate> {
  if (!hasServiceRuntime()) {
    const normalizedSkillId = skillId.trim() || source.split("/").filter(Boolean).pop() || "preview-skill";
    return luaSkillsCatalogUpdateFromSnapshot(updateBrowserFallbackWorkbench((snapshot) => {
      if (snapshot.luaSkills.skills.some((skill) => skill.skillId === normalizedSkillId)) {
        return;
      }
      snapshot.luaSkills.skills.push({
        skillId: normalizedSkillId,
        displayName: normalizedSkillId,
        version: "0.1.0",
        rootName: "USER",
        skillDir: `C:/Users/20000/${BROWSER_PREVIEW_USER_VULCAN_ROOT}/code/luaskills-user/skills/${normalizedSkillId}`,
        enabled: true,
        managed: true,
        sourceType,
        sourceLocator: source,
        entryCount: 0,
        systemOwned: false,
        tools: [],
      });
    }));
  }
  return serviceClient.request<LuaSkillsCatalogUpdate>("service_install_luaskill", { skillId: skillId.trim() || null, source, sourceType });
}

// updateLuaSkill updates one managed Lua Skill package.
// updateLuaSkill 更新单个受管 Lua Skill 包。
export async function updateLuaSkill(skillId: string): Promise<LuaSkillsCatalogUpdate> {
  if (!hasServiceRuntime()) {
    return luaSkillsCatalogUpdateFromSnapshot(updateBrowserFallbackWorkbench((snapshot) => {
      const skill = snapshot.luaSkills.skills.find((item) => item.skillId === skillId);
      if (skill) {
        skill.version = skill.version || "0.1.0";
      }
    }));
  }
  return serviceClient.request<LuaSkillsCatalogUpdate>("service_update_luaskill", { skillId });
}

// setLuaSkillEnabled toggles one Lua Skill package lifecycle state.
// setLuaSkillEnabled 切换单个 Lua Skill 包生命周期状态。
export async function setLuaSkillEnabled(skillId: string, enabled: boolean): Promise<LuaSkillsCatalogUpdate> {
  if (!hasServiceRuntime()) {
    return luaSkillsCatalogUpdateFromSnapshot(updateBrowserFallbackWorkbench((snapshot) => {
      const skill = snapshot.luaSkills.skills.find((item) => item.skillId === skillId);
      if (skill) {
        skill.enabled = enabled;
        skill.disabledReason = enabled ? null : "disabled from browser preview";
      }
    }));
  }
  return serviceClient.request<LuaSkillsCatalogUpdate>("service_set_luaskill_enabled", { skillId, enabled });
}

// setVulcanPluginEnabled toggles one host-owned Vulcan Plugin through its initial public RPC.
// setVulcanPluginEnabled 通过首发公开 RPC 切换单个宿主自有 Vulcan 插件。
export async function setVulcanPluginEnabled(pluginId: string, enabled: boolean): Promise<LuaSkillsCatalogUpdate> {
  if (!hasServiceRuntime()) {
    return luaSkillsCatalogUpdateFromSnapshot(updateBrowserFallbackWorkbench((snapshot) => {
      // plugin resolves only a host-owned Vulcan Plugin row in browser preview mode.
      // plugin 在浏览器预览模式中仅解析宿主自有 Vulcan Plugin 行。
      const plugin = snapshot.plugins.find((item) => item.id === pluginId && item.managementKind === "vulcanPlugin");
      if (plugin) {
        plugin.enabled = enabled;
      }
    }));
  }
  const update = await serviceClient.request<LuaSkillsCatalogUpdate>("service_set_vulcan_plugin_enabled", { pluginId, enabled });
  if (!enabled) {
    invalidatePersistentPluginUiModules(pluginId);
  }
  return update;
}

// pluginEnvironmentReady checks the exact installed generation without side effects.
// pluginEnvironmentReady 无副作用地检查精确的已安装代次。
// Parameters: pluginId is the original host plugin identity.
// 参数：pluginId 是原始宿主插件身份。
// Returns: true only when the execution environment is prepared.
// 返回值：仅在执行环境已准备好时返回真。
export async function pluginEnvironmentReady(pluginId: string): Promise<boolean> {
  if (!hasServiceRuntime()) {
    return true;
  }
  return serviceClient.request<boolean>("service_plugin_environment_ready", { pluginId });
}

// PluginEnvironmentInstallTaskStatus is the Core-owned lifecycle of one explicit install request.
// PluginEnvironmentInstallTaskStatus 是一次显式安装请求由 Core 持有的生命周期。
export interface PluginEnvironmentInstallTaskStatus {
  // taskId is the stable identity used for read-only polling.
  // taskId 是用于只读轮询的稳定身份。
  taskId: string;
  // pluginId is the catalog identity selected when the task was queued.
  // pluginId 是任务排队时所选的目录身份。
  pluginId: string;
  // generation identifies the exact plugin package revision captured at enqueue time.
  // generation 标识排队时捕获的精确插件包修订版。
  generation: string;
  // state is the bounded Core task lifecycle.
  // state 是 Core 任务的有界生命周期。
  state: "queued" | "running" | "succeeded" | "failed";
  // stage identifies the current installation phase without exposing private paths.
  // stage 标识当前安装阶段，不暴露私有路径。
  stage: "queued" | "preparing" | "verifying" | "completed" | "failed";
  // error is a bounded service diagnostic for failed tasks only.
  // error 是仅失败任务携带的有界服务诊断。
  error?: string;
}

// startPluginEnvironmentInstall asks Core to own one explicit background install operation.
// startPluginEnvironmentInstall 请求 Core 持有一次显式后台安装操作。
// Parameters: pluginId selects an installed, disabled plugin from the current catalog.
// 参数：pluginId 从当前目录选择一个已安装且停用的插件。
// Returns: the newly queued or already running task status.
// 返回值：新排队或已在运行的任务状态。
export async function startPluginEnvironmentInstall(pluginId: string): Promise<PluginEnvironmentInstallTaskStatus> {
  if (!hasServiceRuntime()) {
    throw new Error("Plugin environment installation requires the Core service.");
  }
  return serviceClient.request<PluginEnvironmentInstallTaskStatus>("service_start_plugin_environment_install", { pluginId });
}

// pluginEnvironmentInstallStatus reads one Core-owned task without changing it.
// pluginEnvironmentInstallStatus 只读查询一个 Core 自有任务，不修改它。
// Parameters: taskId is the exact identifier returned by the start operation.
// 参数：taskId 是启动操作返回的精确标识。
// Returns: the currently retained stage and terminal outcome.
// 返回值：当前保留的阶段与终态结果。
export async function pluginEnvironmentInstallStatus(taskId: string): Promise<PluginEnvironmentInstallTaskStatus> {
  if (!hasServiceRuntime()) {
    throw new Error("Plugin environment installation requires the Core service.");
  }
  return serviceClient.request<PluginEnvironmentInstallTaskStatus>("service_plugin_environment_install_status", { taskId });
}

// pluginEnvironmentInstallStatusForPlugin reads the current package generation's latest task.
// pluginEnvironmentInstallStatusForPlugin 只读查询当前插件包代次最近的安装任务。
// Parameters: pluginId selects the exact installed plugin row.
// 参数：pluginId 选择精确的已安装插件行。
// Returns: the latest retained task, or null if this generation has never been installed.
// 返回值：最近保留的任务；本代次从未请求安装时为 null。
export async function pluginEnvironmentInstallStatusForPlugin(pluginId: string): Promise<PluginEnvironmentInstallTaskStatus | null> {
  if (!hasServiceRuntime()) {
    return null;
  }
  return serviceClient.request<PluginEnvironmentInstallTaskStatus | null>(
    "service_plugin_environment_install_status_for_plugin", { pluginId },
  );
}

// preparePluginEnvironment explicitly installs dependencies before plugin enablement.
// preparePluginEnvironment 在启用插件前显式安装依赖。
// Parameters: pluginId is the original identity of one disabled host plugin.
// 参数：pluginId 是一个已停用宿主插件的原始身份。
// Returns: true after the current generation passes an independent readiness check.
// 返回值：当前代次通过独立就绪检查后返回真。
export async function preparePluginEnvironment(pluginId: string): Promise<boolean> {
  if (!hasServiceRuntime()) {
    return true;
  }
  return serviceClient.request<boolean>("service_prepare_plugin_environment", { pluginId });
}

// inspectLocalPlugin validates one local package without changing installed state.
// inspectLocalPlugin 在不改变安装状态的情况下校验一个本地插件包。
//
// Parameters: sourcePath is the exact directory selected by the user.
// 参数：sourcePath 是用户选择的精确目录。
// Returns: the complete host-derived capability, Hook, UI, and process authorization preview.
// 返回值：宿主派生的完整能力、Hook、界面与进程授权预览。
export async function inspectLocalPlugin(sourcePath: string): Promise<VulcanPluginPermissionPreview> {
  if (!hasServiceRuntime()) {
    throw new Error("Local plugin inspection requires the Vulcan service runtime.");
  }
  return serviceClient.request<VulcanPluginPermissionPreview>("service_inspect_local_plugin", { sourcePath });
}

// loadInstalledPluginPermissionPreview reads the authorization surface of the installed package version.
// loadInstalledPluginPermissionPreview 读取已安装插件包版本的授权界面数据。
//
// Parameters: pluginId is the exact installed plugin identity.
// 参数：pluginId 是精确的已安装插件身份。
// Returns: the host-derived preview of the installed version, for comparison with an update candidate.
// 返回值：宿主派生的已安装版本预览，用于与更新候选比较。
export async function loadInstalledPluginPermissionPreview(pluginId: string): Promise<VulcanPluginPermissionPreview> {
  if (!hasServiceRuntime()) {
    throw new Error("Installed plugin inspection requires the Vulcan service runtime.");
  }
  return serviceClient.request<VulcanPluginPermissionPreview>("service_installed_plugin_permission_preview", { pluginId });
}

// installLocalPlugin atomically installs one package with the exact approved capability set.
// installLocalPlugin 使用精确批准的能力集合原子安装一个插件包。
// Parameters: sourcePath selects the package, contentFingerprint binds its inspection, and the remaining values carry exact user consent.
// 参数：sourcePath 选择插件包，contentFingerprint 绑定其检查结果，其余值承载用户的精确授权。
// Returns: the catalog update published after the complete installation transaction succeeds.
// 返回值：完整安装事务成功后发布的目录更新。
export async function installLocalPlugin(
  sourcePath: string,
  contentFingerprint: string,
  approvedPermissions: string[],
  allowRegistryDependencies: boolean,
  initialSecrets: VulcanPluginInitialSecret[],
): Promise<LuaSkillsCatalogUpdate> {
  if (!hasServiceRuntime()) {
    throw new Error("Local plugin installation requires the Vulcan service runtime.");
  }
  return serviceClient.request<LuaSkillsCatalogUpdate>("service_install_local_plugin", {
    sourcePath,
    contentFingerprint,
    approvedPermissions,
    allowRegistryDependencies,
    initialSecrets,
  });
}

// updateLocalPlugin replaces one disabled user plugin only after full candidate validation succeeds.
// updateLocalPlugin 仅在完整候选校验成功后替换单个已停用用户插件。
// Parameters: sourcePath selects the candidate, contentFingerprint binds its inspection, and the remaining values carry exact user consent.
// 参数：sourcePath 选择候选包，contentFingerprint 绑定其检查结果，其余值承载用户的精确授权。
// Returns: the catalog update published after the complete update transaction succeeds.
// 返回值：完整更新事务成功后发布的目录更新。
export async function updateLocalPlugin(
  sourcePath: string,
  contentFingerprint: string,
  approvedPermissions: string[],
  allowRegistryDependencies: boolean,
  initialSecrets: VulcanPluginInitialSecret[],
): Promise<LuaSkillsCatalogUpdate> {
  if (!hasServiceRuntime()) {
    throw new Error("Local plugin update requires the Vulcan service runtime.");
  }
  return serviceClient.request<LuaSkillsCatalogUpdate>("service_update_local_plugin", {
    sourcePath,
    contentFingerprint,
    approvedPermissions,
    allowRegistryDependencies,
    initialSecrets,
  });
}

// uninstallUserPlugin removes one disabled and fully drained user-installed plugin.
// uninstallUserPlugin 移除一个已停用且完全排空的用户安装插件。
export async function uninstallUserPlugin(pluginId: string): Promise<LuaSkillsCatalogUpdate> {
  if (!hasServiceRuntime()) {
    throw new Error("User plugin uninstallation requires the Vulcan service runtime.");
  }
  const update = await serviceClient.request<LuaSkillsCatalogUpdate>("service_uninstall_user_plugin", { pluginId });
  invalidatePersistentPluginUiModules(pluginId);
  return update;
}

// setVulcanPluginCredentials applies every change as one new credential revision of the plugin.
// setVulcanPluginCredentials 将全部变更作为插件的单个新凭据修订应用。
export async function setVulcanPluginCredentials(
  pluginId: string,
  changes: VulcanPluginCredentialChange[],
): Promise<LuaSkillsCatalogUpdate> {
  if (!hasServiceRuntime()) {
    throw new Error("Plugin credential storage requires the Vulcan service runtime.");
  }
  return serviceClient.request<LuaSkillsCatalogUpdate>("service_set_vulcan_plugin_credentials", {
    pluginId,
    changes,
  });
}

// resetVulcanPluginConfig replaces the plugin's stored configuration with its package defaults.
// resetVulcanPluginConfig 将插件已存配置替换为插件包默认值。
export async function resetVulcanPluginConfig(pluginId: string): Promise<LuaSkillsCatalogUpdate> {
  if (!hasServiceRuntime()) {
    throw new Error("Plugin configuration reset requires the Vulcan service runtime.");
  }
  return serviceClient.request<LuaSkillsCatalogUpdate>("service_reset_vulcan_plugin_config", { pluginId });
}

// loadPluginUiModule loads one exact manifest-declared uncompressed ESM module.
// loadPluginUiModule 加载一个精确的清单声明未压缩 ESM 模块。
//
// Parameters: pluginId and mountId identify one enabled plugin UI contribution.
// 参数：pluginId 与 mountId 标识一个已启用插件界面贡献。
// signal retires this module read when its service generation is invalidated.
// signal 在服务代次失效时退役当前模块读取。
// Returns: authenticated module identity and untouched UTF-8 source.
// 返回值：经过认证的模块身份与原样 UTF-8 源码。
export async function loadPluginUiModule(pluginId: string, mountId: string, signal: AbortSignal): Promise<PluginUiModuleSource> {
  if (!hasServiceRuntime()) {
    throw new Error("Dynamic plugin UI modules require the Vulcan service runtime.");
  }
  await waitForPluginStartup({
    read: startupSignal => serviceClient.request<StartupSnapshot>("startup.snapshot", undefined, { signal: startupSignal }),
    subscribe: listener => serviceClient.subscribe<StartupSnapshot>("startup", listener),
  }, signal);
  return serviceClient.request<PluginUiModuleSource>("service_load_plugin_ui_module", {
    pluginId,
    mountId,
  }, { signal });
}

// readPluginTextAsset reads one exact manifest-declared UTF-8 asset from an enabled plugin package.
// readPluginTextAsset 从已启用插件包读取一个由清单精确声明的 UTF-8 资源。
//
// Parameters: pluginId selects the owning plugin and path selects one exact required_files entry.
// 参数：pluginId 选择所属插件，path 选择一个精确的 required_files 条目。
// Returns: authenticated plugin identity, relative path, and untouched bounded text.
// 返回值：返回经过认证的插件身份、相对路径与原样有界文本。
export async function readPluginTextAsset(pluginId: string, path: string): Promise<PluginTextAssetSource> {
  if (!hasServiceRuntime()) {
    throw new Error("Plugin text assets require the Vulcan service runtime.");
  }
  return serviceClient.request<PluginTextAssetSource>("service_read_plugin_text_asset", {
    pluginId,
    path,
  });
}

// One host-owned renderer nonce isolates invocation status from other windows.
// 单个宿主持有的渲染器随机标识将调用状态与其他窗口隔离。
const pluginActionClientId = crypto.randomUUID();
// Registration-race polling only queries admission; it never replays an action.
// 注册竞态轮询仅查询准入；绝不重放 Action。
const ACTION_REGISTRATION_POLL_MS = 50;

// Safe diagnostic projection deliberately excludes action arguments and result bodies.
// 安全诊断投影刻意排除 Action 参数与结果正文。
export interface PluginActionStatus {
  invocationId: string;
  pluginId: string;
  actionId: string;
  phase: string;
  terminal: boolean;
  deadlineExceeded: boolean;
  elapsedMs: number;
  stagesMs: Record<string, number>;
}

// Failure retains a query capability for the actual invocation, without replaying it.
// 失败保留实际调用的查询能力，不重放该调用。
export class PluginActionError extends Error {
  constructor(message: string, readonly invocationId: string, readonly queryStatus: () => Promise<PluginActionStatus>) {
    super(message);
    this.name = "PluginActionError";
  }
}

// invokePluginAction invokes one action through the plugin-owning mounted UI identity.
// invokePluginAction 通过插件所属挂载界面身份调用一个 Action。
//
// Parameters: ids select the exact action, arguments is schema validated, and workspacePath is optional context.
// 参数：标识选择精确 Action，arguments 会经过 Schema 校验，workspacePath 是可选上下文。
// Returns: the exact schema-validated plugin result.
// 返回值：经过 Schema 校验的精确插件结果。
export async function invokePluginAction<Result>(
  pluginId: string,
  actionId: string,
  argumentsValue: unknown,
  runtimeIdentity: string,
  workspacePath?: string | null,
  cancellation?: AbortSignal,
): Promise<Result> {
  if (!hasServiceRuntime()) {
    throw new Error("Plugin actions require the Vulcan service runtime.");
  }
  // Identity is generated outside the plugin; a timeout can never become an implicit retry.
  // 身份在插件之外生成；超时绝不转变为隐式重试。
  const identity = { clientId: pluginActionClientId, invocationId: crypto.randomUUID() };
  cancellation?.throwIfAborted();
  const queryAdmission = (cancel: boolean) => serviceClient.request<PluginActionStatus | null>("service_plugin_action_status", { pluginId, runtimeIdentity, identity, cancel });
  const queryStatus = async () => {
    const status = await queryAdmission(false);
    if (!status) throw new Error("Plugin invocation status is unavailable or expired; outcome is unknown.");
    return status;
  };
  // The original response bounds cancellation polling even if registration is rejected.
  // 即使注册被拒绝，原始响应也限定取消轮询的生命周期。
  let settled = false;
  let stopPolling!: () => void;
  const completed = new Promise<void>(resolve => { stopPolling = resolve; });
  const cancelQueued = async () => {
    while (!settled) {
      try { if (await queryAdmission(true)) return; }
      catch { return; } // Transport failure is not evidence of cancellation. / 传输失败不能证明取消成功。
      let timer!: ReturnType<typeof setTimeout>;
      await Promise.race([completed, new Promise<void>(resolve => { timer = setTimeout(resolve, ACTION_REGISTRATION_POLL_MS); })]);
      clearTimeout(timer);
    }
  };
  const withdraw = () => { void cancelQueued(); };
  cancellation?.addEventListener("abort", withdraw, { once: true });
  try { return await serviceClient.request<Result>("service_invoke_plugin_action", {
    identity,
    runtimeIdentity,
    pluginId,
    actionId,
    arguments: argumentsValue,
    workspacePath: workspacePath ?? null,
  }); } catch (error) {
    const diagnostic = error instanceof Error ? error.message : String(error);
    let status: PluginActionStatus;
    try { status = await queryStatus(); }
    catch { throw new PluginActionError(`${diagnostic} (invocation ${identity.invocationId}; execution outcome unavailable, do not automatically replay)`, identity.invocationId, queryStatus); }
    throw new PluginActionError(`${diagnostic} (invocation ${identity.invocationId}; ${status.phase}${status.terminal ? ", execution finished" : ", execution or cleanup is still pending"})`, identity.invocationId, queryStatus);
  } finally {
    settled = true;
    stopPolling();
    cancellation?.removeEventListener("abort", withdraw);
  }
}

// uninstallLuaSkill removes one ordinary Lua Skill package.
// uninstallLuaSkill 移除单个普通 Lua Skill 包。
export async function uninstallLuaSkill(skillId: string): Promise<LuaSkillsCatalogUpdate> {
  if (!hasServiceRuntime()) {
    return luaSkillsCatalogUpdateFromSnapshot(updateBrowserFallbackWorkbench((snapshot) => {
      snapshot.luaSkills.skills = snapshot.luaSkills.skills.filter((skill) => skill.skillId !== skillId || skill.systemOwned);
    }));
  }
  return serviceClient.request<LuaSkillsCatalogUpdate>("service_uninstall_luaskill", { skillId });
}

// setLuaSkillToolRegistrationState persists one Lua Skill tool visibility state.
// setLuaSkillToolRegistrationState 持久化单个 Lua Skill 工具可见性状态。
export async function setLuaSkillToolRegistrationState(skillId: string, localName: string, registrationState: string): Promise<LuaSkillsCatalogUpdate> {
  if (!hasServiceRuntime()) {
    return luaSkillsCatalogUpdateFromSnapshot(updateBrowserFallbackWorkbench((snapshot) => {
      const skill = snapshot.luaSkills.skills.find((item) => item.skillId === skillId);
      const tool = skill?.tools.find((item) => item.localName === localName);
      if (tool) {
        tool.registrationState = registrationState as "explicit" | "implicit" | "disabled";
        tool.registrationLabel = luaSkillRegistrationLabel(registrationState);
      }
    }));
  }
  return serviceClient.request<LuaSkillsCatalogUpdate>("service_set_luaskill_tool_registration_state", { skillId, localName, registrationState });
}

// refreshAgentSkillsCatalog refreshes prompt-based Agent Skills through the local service.
// refreshAgentSkillsCatalog 通过本地服务刷新基于提示词的 Agent Skills。
export async function refreshAgentSkillsCatalog(): Promise<AgentSkillsCatalogUpdate> {
  if (!hasServiceRuntime()) {
    return agentSkillsCatalogUpdateFromSnapshot(updateBrowserFallbackWorkbench(() => undefined));
  }
  return serviceClient.request<AgentSkillsCatalogUpdate>("service_refresh_agent_skills_catalog");
}

// openAgentSkillsUserDirectory opens the configured user package root.
// openAgentSkillsUserDirectory 打开配置后的用户技能包根目录。
export async function openAgentSkillsUserDirectory(): Promise<AgentSkillsCatalogUpdate> {
  if (!hasServiceRuntime()) {
    return agentSkillsCatalogUpdateFromSnapshot(updateBrowserFallbackWorkbench((snapshot) => {
      snapshot.agentSkills.notice = snapshot.uiLocale?.startsWith("zh") !== false
        ? `浏览器预览用户技能目录：${snapshot.agentSkills.userRoot}`
        : `Browser preview user skill directory: ${snapshot.agentSkills.userRoot}`
    }))
  }
  return serviceClient.request<AgentSkillsCatalogUpdate>("service_open_agent_skills_user_directory")
}

// saveAgentSkill saves one complete strict user Agent Skill package.
// saveAgentSkill 保存一个完整严格用户 Agent Skill 技能包。
export async function saveAgentSkill(skill: AgentSkillSaveRequest): Promise<AgentSkillsCatalogUpdate> {
  if (!hasServiceRuntime()) {
    throw new Error("Agent Skill package editing requires the desktop service runtime.")
  }
  return serviceClient.request<AgentSkillsCatalogUpdate>("service_save_agent_skill", { skill })
}

// deleteAgentSkill deletes one exact user-owned Agent Skill package.
// deleteAgentSkill 删除一个精确用户 Agent Skill 技能包。
export async function deleteAgentSkill(name: string): Promise<AgentSkillsCatalogUpdate> {
  if (!hasServiceRuntime()) {
    throw new Error("Agent Skill package deletion requires the desktop service runtime.")
  }
  return serviceClient.request<AgentSkillsCatalogUpdate>("service_delete_agent_skill", { name })
}

// restoreAgentSkillDefault removes one user override and restores its system source.
// restoreAgentSkillDefault 移除一个用户覆盖并恢复其系统来源。
export async function restoreAgentSkillDefault(name: string): Promise<AgentSkillsCatalogUpdate> {
  if (!hasServiceRuntime()) {
    throw new Error("Agent Skill restoration requires the desktop service runtime.")
  }
  return serviceClient.request<AgentSkillsCatalogUpdate>("service_restore_agent_skill_default", { name })
}

// saveAgentSkillCategory saves one complete user-defined logical category.
// saveAgentSkillCategory 保存一个完整用户定义逻辑分类。
export async function saveAgentSkillCategory(category: AgentSkillCategorySaveRequest): Promise<AgentSkillsCatalogUpdate> {
  if (!hasServiceRuntime()) {
    throw new Error("Agent Skill category editing requires the desktop service runtime.")
  }
  return serviceClient.request<AgentSkillsCatalogUpdate>("service_save_agent_skill_category", { category })
}

// deleteAgentSkillCategory deletes one unused user-defined logical category.
// deleteAgentSkillCategory 删除一个未使用的用户定义逻辑分类。
export async function deleteAgentSkillCategory(categoryId: string): Promise<AgentSkillsCatalogUpdate> {
  if (!hasServiceRuntime()) {
    throw new Error("Agent Skill category deletion requires the desktop service runtime.")
  }
  return serviceClient.request<AgentSkillsCatalogUpdate>("service_delete_agent_skill_category", { categoryId })
}

// setAgentSkillCategory assigns one effective skill to one exact logical category.
// setAgentSkillCategory 将一个有效技能归入一个精确逻辑分类。
export async function setAgentSkillCategory(name: string, categoryId: string): Promise<AgentSkillsCatalogUpdate> {
  if (!hasServiceRuntime()) {
    throw new Error("Agent Skill category assignment requires the desktop service runtime.")
  }
  return serviceClient.request<AgentSkillsCatalogUpdate>("service_set_agent_skill_category", { name, categoryId })
}

// refreshSubagentCatalog refreshes subagent and expert definitions through the local service.
// refreshSubagentCatalog 通过本地服务刷新子代理与专家定义。
export async function refreshSubagentCatalog(): Promise<SubagentCatalogUpdate> {
  if (!hasServiceRuntime()) {
    return subagentCatalogUpdateFromSnapshot(updateBrowserFallbackWorkbench(() => undefined));
  }
  return serviceClient.request<SubagentCatalogUpdate>("service_refresh_subagent_catalog");
}

// openSubagentUserDirectory opens the user-owned agent definition directory.
// openSubagentUserDirectory 打开用户拥有的智能体定义目录。
export async function openSubagentUserDirectory(): Promise<SubagentCatalogUpdate> {
  if (!hasServiceRuntime()) {
    return subagentCatalogUpdateFromSnapshot(updateBrowserFallbackWorkbench((snapshot) => {
      snapshot.subagentCatalog.notice = `Browser preview user agent directory: ${snapshot.subagentCatalog.userRoot}`;
    }));
  }
  return serviceClient.request<SubagentCatalogUpdate>("service_open_subagent_user_directory");
}

// saveSubagentDefinition persists one editable agent definition.
// saveSubagentDefinition 持久化单个可编辑智能体定义。
export async function saveSubagentDefinition(definition: SubagentDefinitionSaveRequest): Promise<SubagentCatalogUpdate> {
  if (!hasServiceRuntime()) {
    return subagentCatalogUpdateFromSnapshot(updateBrowserFallbackWorkbench((snapshot) => {
      const targetAgentId = browserPreviewTargetAgentId(definition);
      const originalAgentId = definition.originalAgentId?.trim() || definition.agentId;
      const existingIndex = snapshot.subagentCatalog.definitions.findIndex((item) => item.agentId === originalAgentId);
      const existingDefinition = existingIndex >= 0 ? snapshot.subagentCatalog.definitions[existingIndex] : undefined;
      if (existingDefinition?.scopeKey === "system" || existingDefinition?.overrideOf) {
        throw new Error(translateActive("agents.service.builtinReadOnly"));
      }
      if (existingIndex < 0 && snapshot.subagentCatalog.definitions.some((item) => item.agentId === targetAgentId)) {
        throw new Error(formatMessage(translateActive("agents.service.alreadyExists"), { targetAgentId }));
      }
      const nextDefinition: SubagentDefinitionSummary = {
        agentId: targetAgentId,
        name: definition.name.trim() || targetAgentId,
        description: definition.description?.trim() || null,
        locales: {},
        emoji: browserPreviewDefaultedTextPatch(definition.emoji, existingDefinition?.emoji, "🤖"),
        color: browserPreviewDefaultedTextPatch(definition.color, existingDefinition?.color, "gray"),
        vibe: browserPreviewOptionalTextPatch(definition.vibe, existingDefinition?.vibe ?? null),
        sourceKey: `user:${targetAgentId}`,
        categoryId: targetAgentId.includes("/") ? targetAgentId.split("/").slice(0, -1).join("/") : null,
        packageId: definition.packageId?.trim() || null,
        scope: "User",
        scopeKey: "user",
        canEdit: true,
        canDelete: true,
        canToggle: true,
        canCopy: true,
        enabled: definition.enabled,
        definitionFile: browserPreviewAgentDefinitionPath(snapshot.subagentCatalog.userRoot, targetAgentId, "md"),
        sidecarFile: browserPreviewAgentDefinitionPath(snapshot.subagentCatalog.userRoot, targetAgentId, "yaml"),
        remoteUrl: null,
        overrideOf: null,
        canRestoreDefault: false,
        sourceHash: null,
        version: null,
        updatedAt: Date.now().toString(),
        toolPolicyMode: definition.toolPolicyMode ?? "all",
        toolPolicyTools: definition.toolPolicyTools ?? [],
        allowedToolCount: definition.toolPolicyMode === "allow_only" ? (definition.toolPolicyTools?.length ?? 0) : definition.allowedTools.length,
        disabledToolCount: definition.toolPolicyMode === "deny_only" ? (definition.toolPolicyTools?.length ?? 0) : definition.disabledTools.length,
        enabledPromptSkillCount: definition.promptSkillPolicyMode === "allow_only" ? (definition.promptSkillPolicySkills?.length ?? 0) : definition.enabledPromptSkills.length,
        disabledPromptSkillCount: definition.promptSkillPolicyMode === "deny_only" ? (definition.promptSkillPolicySkills?.length ?? 0) : definition.disabledPromptSkills.length,
        promptSkillPolicyMode: definition.promptSkillPolicyMode ?? "all",
        promptSkillPolicySkills: definition.promptSkillPolicySkills ?? [],
        providerModel: definition.providerModel ?? {},
        systemPrompt: definition.systemPrompt ?? null,
        systemPromptAppend: definition.systemPromptAppend ?? null,
        allowedTools: definition.allowedTools,
        disabledTools: definition.disabledTools,
        allowedAiCapabilities: [],
        disabledAiCapabilities: [],
        luaskillsOverrides: definition.luaskillsOverrides ?? null,
        mcpServerOverrides: definition.mcpServerOverrides ?? null,
        enabledPromptSkills: definition.enabledPromptSkills,
        disabledPromptSkills: definition.disabledPromptSkills,
        capabilityDiagnostics: ["Browser preview saved this definition in the user agent layer."],
      };
      if (existingIndex >= 0) {
        snapshot.subagentCatalog.definitions[existingIndex] = nextDefinition;
      } else {
        snapshot.subagentCatalog.definitions.push(nextDefinition);
      }
    }));
  }
  return serviceClient.request<SubagentCatalogUpdate>("service_save_subagent_definition", { definition });
}

// browserPreviewTargetAgentId applies package assignment in browser fallback mode.
// browserPreviewTargetAgentId 在浏览器预览模式下应用包分配。
function browserPreviewTargetAgentId(definition: SubagentDefinitionSaveRequest): string {
  const agentId = definition.agentId.trim();
  const packageId = definition.packageId?.trim();
  if (!packageId) {
    return agentId;
  }
  const prefix = `${packageId}/`;
  return agentId.startsWith(prefix) ? agentId : `${packageId}/${agentId}`;
}

// browserPreviewDefaultedTextPatch applies the service tri-state text patch contract to a defaulted field.
// browserPreviewDefaultedTextPatch 将服务端三态文本补丁契约应用到带默认值字段。
function browserPreviewDefaultedTextPatch(value: string | null | undefined, previousValue: string | null | undefined, defaultValue: string): string {
  if (value === undefined) {
    return previousValue?.trim() || defaultValue;
  }
  if (value === null) {
    return defaultValue;
  }
  return value.trim() || defaultValue;
}

// browserPreviewOptionalTextPatch applies the service tri-state text patch contract to an optional field.
// browserPreviewOptionalTextPatch 将服务端三态文本补丁契约应用到可选字段。
function browserPreviewOptionalTextPatch(value: string | null | undefined, previousValue: string | null | undefined): string | null {
  if (value === undefined) {
    return previousValue?.trim() || null;
  }
  if (value === null) {
    return null;
  }
  return value.trim() || null;
}

// setSubagentDefinitionEnabled persists one subagent definition enabled state.
// setSubagentDefinitionEnabled 持久化单个子代理定义启停状态。
export async function setSubagentDefinitionEnabled(agentId: string, enabled: boolean): Promise<SubagentCatalogUpdate> {
  if (!hasServiceRuntime()) {
    return subagentCatalogUpdateFromSnapshot(updateBrowserFallbackWorkbench((snapshot) => {
      const definition = snapshot.subagentCatalog.definitions.find((item) => item.agentId === agentId);
      if (definition) {
        definition.enabled = enabled;
      }
    }));
  }
  return serviceClient.request<SubagentCatalogUpdate>("service_set_subagent_definition_enabled", { agentId, enabled });
}

// deleteSubagentDefinition removes one user-owned subagent definition.
// deleteSubagentDefinition 移除单个用户拥有的子代理定义。
export async function deleteSubagentDefinition(agentId: string): Promise<SubagentCatalogUpdate> {
  if (!hasServiceRuntime()) {
    return subagentCatalogUpdateFromSnapshot(updateBrowserFallbackWorkbench((snapshot) => {
      const index = snapshot.subagentCatalog.definitions.findIndex((item) => item.agentId === agentId);
      const definition = index >= 0 ? snapshot.subagentCatalog.definitions[index] : undefined;
      if (!definition || (definition.scopeKey !== "user" && definition.scopeKey !== "package") || definition.overrideOf) {
        throw new Error(translateActive("agents.service.onlyCustomDeletable"));
      }
      snapshot.subagentCatalog.definitions.splice(index, 1);
    }));
  }
  return serviceClient.request<SubagentCatalogUpdate>("service_delete_subagent_definition", { agentId });
}

// restoreSubagentDefinitionDefault removes one user override for a system-backed definition.
// restoreSubagentDefinitionDefault 移除单个系统集成定义的用户覆盖。
export async function restoreSubagentDefinitionDefault(agentId: string): Promise<SubagentCatalogUpdate> {
  if (!hasServiceRuntime()) {
    return subagentCatalogUpdateFromSnapshot(updateBrowserFallbackWorkbench((snapshot) => {
      const definition = snapshot.subagentCatalog.definitions.find((item) => item.agentId === agentId);
      if (definition?.canRestoreDefault || definition?.overrideOf) {
        definition.scope = "System";
        definition.scopeKey = "system";
        definition.overrideOf = null;
        definition.canRestoreDefault = false;
      }
    }));
  }
  return serviceClient.request<SubagentCatalogUpdate>("service_restore_subagent_definition_default", { agentId });
}

// setAgentSkillEnabled persists one Agent Skill state.
// setAgentSkillEnabled 持久化单个 Agent Skill 状态。
export async function setAgentSkillEnabled(name: string, enabled: boolean): Promise<AgentSkillsCatalogUpdate> {
  if (!hasServiceRuntime()) {
    return agentSkillsCatalogUpdateFromSnapshot(updateBrowserFallbackWorkbench((snapshot) => {
    const skill = snapshot.agentSkills.records.find((item) => item.name === name && item.effective);
      if (skill) {
        skill.enabled = enabled;
      }
    }));
  }
  return serviceClient.request<AgentSkillsCatalogUpdate>("service_set_agent_skill_enabled", { name, enabled });
}

// loadProjectAgentSkillOverrides loads project-local Agent Skill overrides.
// loadProjectAgentSkillOverrides 加载项目本地 Agent Skill 覆盖。
export async function loadProjectAgentSkillOverrides(projectId: string): Promise<Record<string, string>> {
  if (!hasServiceRuntime()) {
    return {};
  }
  return serviceClient.request<Record<string, string>>("service_load_project_agent_skill_overrides", { projectId });
}

// setAgentSkillProjectState persists one project-level Agent Skill tri-state override.
// setAgentSkillProjectState 持久化单个项目级 Agent Skill 三态覆盖。
export async function setAgentSkillProjectState(projectId: string, name: string, stateKey: AgentSkillProjectState): Promise<void> {
  if (!hasServiceRuntime()) {
    return;
  }
  return serviceClient.request<void>("service_set_agent_skill_project_state", { projectId, name, stateKey });
}
