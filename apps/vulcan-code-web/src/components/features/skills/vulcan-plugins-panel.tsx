import * as React from "react";
import { ChartNoAxesCombined, KeyRound, Puzzle, Route, Search, ShieldCheck, Trash2 } from "lucide-react";
import { Badge } from "@/components/ui/badge";
import { StatusPill } from "@/components/shared/status";
import { useI18n } from "@/i18n";
import type { PluginSummary, VulcanPluginPermissionPreview } from "@/types";
import type { SkillActionProps } from "./skill-area-types";

// VulcanPluginStatusFilter identifies the lifecycle subset shown in the Vulcan Plugin list.
// VulcanPluginStatusFilter 标识 Vulcan 插件列表中显示的生命周期子集。
type VulcanPluginStatusFilter = "all" | "enabled" | "disabled" | "issues";

// pluginRuntimeStatusLabel resolves one stable runtime state through the active UI locale.
// pluginRuntimeStatusLabel 通过当前界面语言解析单个稳定运行时状态。
//
// Parameters: plugin is the projected plugin row and t is the active translation resolver.
// 参数：plugin 是投影插件行，t 是当前翻译解析器。
// Returns: a localized lifecycle label.
// 返回值：本地化生命周期标签。
function pluginRuntimeStatusLabel(
  plugin: PluginSummary,
  t: ReturnType<typeof useI18n>["t"],
): string {
  switch (plugin.runtimeState) {
    case "enabled":
      return t("skills.status.enabled");
    case "disabled":
      return t("skills.status.stopped");
    case "enabling":
      return t("skills.plugins.enabling");
    case "draining":
      return t("skills.plugins.draining");
    case "disabledWithCleanupWarning":
      return t("skills.plugins.cleanupWarning");
    case "unsupportedPlatform":
      return t("skills.plugins.unsupportedPlatform");
    default:
      return t("skills.plugins.unavailable");
  }
}

// localizedPluginText resolves plugin manifest text for the active WebUI locale.
// localizedPluginText 为当前 WebUI 语言解析插件清单文案。
//
// Parameters: values contains locale-keyed manifest text, locale is the active WebUI locale, and fallback is the service-projected default text.
// 参数：values 包含按语言标识组织的清单文案，locale 是当前 WebUI 语言，fallback 是服务投影的默认文案。
// Returns: the exact-locale value, a same-language value, or the projected fallback in that order.
// 返回值：依次返回完整语言匹配值、同语言匹配值或投影回退文案。
function localizedPluginText(
  values: Record<string, string>,
  locale: string,
  fallback: string,
): string {
  // exactValue preserves region-specific plugin translations when the package supplies them.
  // exactValue 在插件包提供地区专用翻译时保留该翻译。
  const exactValue = values[locale]?.trim();
  if (exactValue) {
    return exactValue;
  }
  // language identifies the configured base language for locale-compatible lookup.
  // language 标识用于兼容语言查找的当前基础语言。
  const language = locale.split("-")[0];
  // languageValue follows the same exact-then-language convention as other localized catalogs.
  // languageValue 遵循其他本地化目录使用的完整匹配后同语言匹配约定。
  const languageValue = Object.entries(values).find(([candidate, value]) => (
    (candidate === language || candidate.startsWith(`${language}-`)) && value.trim().length > 0
  ))?.[1]?.trim();
  return languageValue || fallback;
}

// vulcanPluginMatchesSearch checks one plugin against localized user-visible fields.
// vulcanPluginMatchesSearch 使用本地化后的用户可见字段检查单个插件是否匹配搜索词。
//
// Parameters: plugin is the projected plugin row, name and capability are localized display values, and query is the current filter.
// 参数：plugin 是投影插件行，name 与 capability 是本地化显示值，query 是当前筛选词。
// Returns: true when the normalized query is empty or matches a visible plugin field.
// 返回值：规范化查询为空或匹配可见插件字段时返回 true。
function vulcanPluginMatchesSearch(
  plugin: PluginSummary,
  name: string,
  capability: string,
  query: string,
): boolean {
  // normalizedQuery provides case-insensitive matching without changing source metadata.
  // normalizedQuery 在不改动来源元数据的情况下提供不区分大小写的匹配。
  const normalizedQuery = query.trim().toLocaleLowerCase();
  if (!normalizedQuery) {
    return true;
  }
  return [
    plugin.id,
    plugin.kind,
    plugin.version ?? "",
    name,
    capability,
    ...(plugin.extension?.skills ?? []),
    ...(plugin.extension?.agents ?? []),
  ]
    .some((value) => value.toLocaleLowerCase().includes(normalizedQuery));
}

// vulcanPluginHasIssue reports whether one plugin is unavailable or outside a normal lifecycle transition.
// vulcanPluginHasIssue 判断单个插件是否不可用或处于正常生命周期过渡之外。
//
// Parameters: plugin is the exact projected Vulcan Plugin row.
// 参数：plugin 是精确投影的 Vulcan 插件行。
// Returns: true for diagnostics, unavailable assets, cleanup warnings, unsupported platforms, and unknown runtime states.
// 返回值：存在诊断、资产不可用、清理警告、平台不支持或未知运行时状态时返回 true。
function vulcanPluginHasIssue(plugin: PluginSummary): boolean {
  if (!plugin.available || plugin.diagnostics.length > 0) {
    return true;
  }
  switch (plugin.runtimeState) {
    case "enabled":
    case "disabled":
    case "enabling":
    case "draining":
      return false;
    default:
      return true;
  }
}

// VulcanPluginsPanel renders host-owned plugins and their unified content/runtime lifecycle controls.
// VulcanPluginsPanel 渲染宿主自有插件及其统一的内容与运行时生命周期控件。
export function VulcanPluginsPanel(props: {
  // Service-backed plugin lifecycle actions.
  // 服务端支持的插件生命周期动作。
  actions: SkillActionProps;
  // Plugin rows from the committed host plugin projection.
  // 来自已提交宿主插件领域投影的插件行。
  plugins: PluginSummary[];
}) {
  // locale and t resolve plugin manifest text and stable plugin-page labels.
  // locale 与 t 分别解析插件清单文案和稳定的插件页面文案。
  const { locale, t } = useI18n();
  // searchQuery stores the free-text Vulcan Plugin filter.
  // searchQuery 保存 Vulcan 插件自由文本筛选词。
  const [searchQuery, setSearchQuery] = React.useState("");
  // statusFilter stores the selected Vulcan Plugin lifecycle filter.
  // statusFilter 保存选中的 Vulcan 插件生命周期筛选。
  const [statusFilter, setStatusFilter] = React.useState<VulcanPluginStatusFilter>("all");
  // busyPluginId identifies the single Vulcan Plugin mutation currently in flight.
  // busyPluginId 标识当前正在执行变更的唯一 Vulcan Plugin。
  const [busyPluginId, setBusyPluginId] = React.useState<string | null>(null);
  // installSourcePath stores the exact local plugin directory selected for inspection.
  // installSourcePath 保存等待检查的精确本地插件目录。
  const [installSourcePath, setInstallSourcePath] = React.useState("");
  // permissionPreview stores only the latest host-validated package authorization surface.
  // permissionPreview 仅保存最近一次经宿主校验的插件包授权界面数据。
  const [permissionPreview, setPermissionPreview] = React.useState<VulcanPluginPermissionPreview | null>(null);
  // installError stores inspection or installation diagnostics without hiding the preview.
  // installError 保存检查或安装诊断，同时不隐藏授权预览。
  const [installError, setInstallError] = React.useState("");
  // installBusy blocks overlapping package inspection, installation, and uninstallation.
  // installBusy 阻止插件包检查、安装与卸载操作重叠执行。
  const [installBusy, setInstallBusy] = React.useState(false);
  // authorizationConfirmed records explicit approval of every displayed capability and mount.
  // authorizationConfirmed 记录用户对所有已展示能力与挂载点的明确批准。
  const [authorizationConfirmed, setAuthorizationConfirmed] = React.useState(false);
  // registryDependenciesConfirmed records separate network consent when requested by the package.
  // registryDependenciesConfirmed 记录插件请求注册表依赖时的独立联网许可。
  const [registryDependenciesConfirmed, setRegistryDependenciesConfirmed] = React.useState(false);
  // installSecretDrafts holds required local-MCP credentials only until the installation request settles.
  // installSecretDrafts 仅在安装请求结束前保存所需的本地 MCP 凭据。
  const [installSecretDrafts, setInstallSecretDrafts] = React.useState<Record<string, string>>({});
  // secretDrafts stores only user-entered replacement values and is cleared immediately after each mutation.
  // secretDrafts 仅保存用户输入的替换值，并在每次变更后立即清空。
  const [secretDrafts, setSecretDrafts] = React.useState<Record<string, string>>({});
  // secretBusyKey identifies the exact plugin/server/key mutation currently in flight.
  // secretBusyKey 标识当前正在执行的精确插件/服务器/键变更。
  const [secretBusyKey, setSecretBusyKey] = React.useState<string | null>(null);
  // secretError stores sanitized service diagnostics without ever retaining a credential value.
  // secretError 保存已脱敏服务诊断，绝不保留凭据值。
  const [secretError, setSecretError] = React.useState("");
  // actionRunningRef closes the repeat-click window before React commits the busy state.
  // actionRunningRef 在 React 提交忙碌状态前关闭重复点击窗口。
  const actionRunningRef = React.useRef(false);
  // vulcanPlugins keeps ordinary Lua Skill rows on their existing catalog page.
  // vulcanPlugins 使普通 Lua Skill 行继续留在原有技能目录页面。
  const vulcanPlugins = React.useMemo(
    () => props.plugins.filter((plugin) => plugin.managementKind === "vulcanPlugin"),
    [props.plugins],
  );
  // visiblePlugins applies localized text and lifecycle filters to the Vulcan Plugin projection.
  // visiblePlugins 对 Vulcan 插件投影应用本地化文本与生命周期筛选。
  const visiblePlugins = React.useMemo(() => vulcanPlugins.filter((plugin) => {
    if (statusFilter === "enabled" && !plugin.desiredEnabled) {
      return false;
    }
    if (statusFilter === "disabled" && plugin.desiredEnabled) {
      return false;
    }
    if (statusFilter === "issues" && !vulcanPluginHasIssue(plugin)) {
      return false;
    }
    // localizedName is the active-locale plugin name used by both filtering and rendering.
    // localizedName 是同时用于筛选和渲染的当前语言插件名称。
    const localizedName = localizedPluginText(plugin.nameByLocale, locale, plugin.name);
    // localizedCapability is the active-locale plugin introduction used by both filtering and rendering.
    // localizedCapability 是同时用于筛选和渲染的当前语言插件介绍。
    const localizedCapability = localizedPluginText(plugin.capabilityByLocale, locale, plugin.capability);
    return vulcanPluginMatchesSearch(plugin, localizedName, localizedCapability, searchQuery);
  }), [locale, searchQuery, statusFilter, vulcanPlugins]);
  // inspectedInstalledPlugin resolves whether the inspected identity is a new install, update, or bundled conflict.
  // inspectedInstalledPlugin 解析已检查身份属于新安装、更新还是内建冲突。
  const inspectedInstalledPlugin = permissionPreview === null
    ? undefined
    : props.plugins.find((plugin) => plugin.id === permissionPreview.pluginId);
  // togglePlugin persists one plugin state through the unified plugin catalog transaction.
  // togglePlugin 通过统一插件目录事务持久化单个插件状态。
  //
  // Parameters: plugin is the exact projected row to toggle.
  // 参数：plugin 是需要切换的精确投影行。
  // Returns: a promise that settles after the service response updates the catalog.
  // 返回值：服务响应更新目录后结束的 Promise。
  const togglePlugin = React.useCallback(async (plugin: PluginSummary) => {
    if (actionRunningRef.current || !plugin.available) {
      return;
    }
    actionRunningRef.current = true;
    setBusyPluginId(plugin.id);
    try {
      // requestedEnabled retries cleanup without accidentally re-enabling a warning-state plugin.
      // requestedEnabled 会重试清理，避免意外重新启用处于警告状态的插件。
      const requestedEnabled = plugin.runtimeState === "disabledWithCleanupWarning"
        ? false
        : !plugin.desiredEnabled;
      await props.actions.onSetVulcanPluginEnabled(plugin.id, requestedEnabled);
    } finally {
      actionRunningRef.current = false;
      setBusyPluginId(null);
    }
  }, [props.actions]);

  // inspectPluginPackage validates the current path and resets every prior authorization decision.
  // inspectPluginPackage 校验当前路径并重置此前的所有授权决定。
  const inspectPluginPackage = React.useCallback(async () => {
    const sourcePath = installSourcePath.trim();
    if (!sourcePath || installBusy) {
      return;
    }
    setInstallBusy(true);
    setInstallError("");
    setPermissionPreview(null);
    setAuthorizationConfirmed(false);
    setRegistryDependenciesConfirmed(false);
    setInstallSecretDrafts({});
    try {
      setPermissionPreview(await props.actions.onInspectLocalPlugin(sourcePath));
    } catch (reason: unknown) {
      setInstallError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setInstallSecretDrafts({});
      setInstallBusy(false);
    }
  }, [installBusy, installSourcePath, props.actions]);

  // installInspectedPlugin installs only the unchanged path and exact host-projected capability set.
  // installInspectedPlugin 仅使用未变化路径与宿主投影的精确能力集合安装插件。
  const installInspectedPlugin = React.useCallback(async () => {
    if (permissionPreview === null || !authorizationConfirmed || installBusy) {
      return;
    }
    const requiresRegistryConsent = permissionPreview.permissions.includes("dependency_registry_network");
    if (requiresRegistryConsent && !registryDependenciesConfirmed) {
      return;
    }
    const initialSecrets = permissionPreview.installSecretRequirements.map((requirement) => ({
      serverId: requirement.serverId,
      secretKey: requirement.secretKey,
      value: installSecretDrafts[`${requirement.serverId}:${requirement.secretKey}`] ?? "",
    }));
    if (initialSecrets.some((secret) => secret.value.length === 0)) {
      return;
    }
    setInstallBusy(true);
    setInstallError("");
    try {
      const mutation = inspectedInstalledPlugin?.extension?.origin === "userInstalled"
        ? props.actions.onUpdateLocalPlugin
        : props.actions.onInstallLocalPlugin;
      await mutation(
        installSourcePath.trim(),
        permissionPreview.contentFingerprint,
        permissionPreview.permissions,
        registryDependenciesConfirmed,
        initialSecrets,
      );
      setInstallSourcePath("");
      setPermissionPreview(null);
      setAuthorizationConfirmed(false);
      setRegistryDependenciesConfirmed(false);
      setInstallSecretDrafts({});
    } catch (reason: unknown) {
      setInstallError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setInstallSecretDrafts({});
      setInstallBusy(false);
    }
  }, [
    authorizationConfirmed,
    installBusy,
    installSourcePath,
    installSecretDrafts,
    inspectedInstalledPlugin,
    permissionPreview,
    props.actions,
    registryDependenciesConfirmed,
  ]);

  // uninstallPlugin confirms and removes only one disabled user-installed plugin.
  // uninstallPlugin 经确认后仅移除单个已停用的用户安装插件。
  const uninstallPlugin = React.useCallback(async (plugin: PluginSummary) => {
    if (installBusy || plugin.extension?.origin !== "userInstalled" || plugin.runtimeState !== "disabled") {
      return;
    }
    const confirmed = window.confirm(
      locale.startsWith("zh")
        ? `确定卸载用户插件“${plugin.name}”吗？此操作会删除插件包、配置和隔离环境。`
        : `Uninstall user plugin “${plugin.name}”? Its package, configuration, and isolated environments will be removed.`,
    );
    if (!confirmed) {
      return;
    }
    setInstallBusy(true);
    setInstallError("");
    try {
      await props.actions.onUninstallUserPlugin(plugin.id);
    } catch (reason: unknown) {
      setInstallError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setInstallBusy(false);
    }
  }, [installBusy, locale, props.actions]);

  // storePluginMcpSecret writes one blank-by-default replacement value and clears the draft on every outcome.
  // storePluginMcpSecret 写入一个默认空白的替换值，并在所有结果下清空草稿。
  const storePluginMcpSecret = React.useCallback(async (
    pluginId: string,
    serverId: string,
    secretKey: string,
  ) => {
    const draftKey = `${pluginId}:${serverId}:${secretKey}`;
    const value = secretDrafts[draftKey] ?? "";
    if (!value || secretBusyKey !== null) {
      return;
    }
    setSecretBusyKey(draftKey);
    setSecretError("");
    try {
      await props.actions.onSetPluginMcpSecret(pluginId, serverId, secretKey, value);
    } catch (reason: unknown) {
      setSecretError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setSecretDrafts((current) => ({ ...current, [draftKey]: "" }));
      setSecretBusyKey(null);
    }
  }, [props.actions, secretBusyKey, secretDrafts]);

  // deletePluginMcpSecret removes one exact credential after explicit confirmation.
  // deletePluginMcpSecret 在明确确认后移除单个精确凭据。
  const deletePluginMcpSecret = React.useCallback(async (
    pluginId: string,
    serverId: string,
    secretKey: string,
  ) => {
    const draftKey = `${pluginId}:${serverId}:${secretKey}`;
    if (secretBusyKey !== null || !window.confirm(
      locale.startsWith("zh")
        ? `确定删除 Secret“${secretKey}”吗？已启用服务器会先通过就绪检查。`
        : `Delete secret “${secretKey}”? An enabled server must pass readiness before the change commits.`,
    )) {
      return;
    }
    setSecretBusyKey(draftKey);
    setSecretError("");
    try {
      await props.actions.onDeletePluginMcpSecret(pluginId, serverId, secretKey);
    } catch (reason: unknown) {
      setSecretError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setSecretDrafts((current) => ({ ...current, [draftKey]: "" }));
      setSecretBusyKey(null);
    }
  }, [locale, props.actions, secretBusyKey]);

  return (
    <div className="skill-page-body lua-skill-page-body">
      <section className="vulcan-plugin-install-panel" aria-label={locale.startsWith("zh") ? "安装本地插件" : "Install local plugin"}>
        <div className="vulcan-plugin-install-heading">
          <ShieldCheck size={17} />
          <div>
            <strong>{locale.startsWith("zh") ? "安装本地插件" : "Install local plugin"}</strong>
            <p>{locale.startsWith("zh")
              ? "先检查插件包，再明确授权其能力、Hook、界面挂载点和进程入口。"
              : "Inspect the package before explicitly authorizing its capabilities, hooks, UI mounts, and process entries."}</p>
          </div>
        </div>
        <div className="vulcan-plugin-install-input-row">
          <input
            aria-label={locale.startsWith("zh") ? "本地插件目录" : "Local plugin directory"}
            disabled={installBusy}
            onChange={(event) => {
              setInstallSourcePath(event.target.value);
              setPermissionPreview(null);
              setAuthorizationConfirmed(false);
              setRegistryDependenciesConfirmed(false);
              setInstallSecretDrafts({});
            }}
            placeholder={locale.startsWith("zh") ? "输入包含 plugin.yaml 的本地目录" : "Directory containing plugin.yaml"}
            value={installSourcePath}
          />
          <button
            className="secondary-button"
            disabled={installBusy || installSourcePath.trim().length === 0}
            onClick={() => void inspectPluginPackage()}
            type="button"
          >
            {locale.startsWith("zh") ? "检查" : "Inspect"}
          </button>
        </div>
        {installError ? <div className="skill-empty-state warning">{installError}</div> : null}
        {permissionPreview ? (
          <div className="vulcan-plugin-permission-preview">
            <div className="vulcan-plugin-permission-title">
              <strong>{permissionPreview.pluginId}</strong>
              <Badge variant="secondary">v{permissionPreview.version}</Badge>
            </div>
            <dl>
              <div><dt>{locale.startsWith("zh") ? "权限" : "Permissions"}</dt><dd>{permissionPreview.permissions.join("、") || "—"}</dd></div>
              <div><dt>Hooks</dt><dd>{permissionPreview.hookPoints.join("、") || "—"}</dd></div>
              <div><dt>{locale.startsWith("zh") ? "界面挂载点" : "UI mounts"}</dt><dd>{permissionPreview.uiMountPoints.join("、") || "—"}</dd></div>
              <div><dt>{locale.startsWith("zh") ? "原生进程" : "Native processes"}</dt><dd>{permissionPreview.binaryIds.join("、") || "—"}</dd></div>
              <div><dt>Node.js</dt><dd>{permissionPreview.nodeEntryIds.join("、") || "—"}</dd></div>
              <div><dt>Python</dt><dd>{permissionPreview.pythonEntryIds.join("、") || "—"}</dd></div>
              <div><dt>{locale.startsWith("zh") ? "插件类型" : "Plugin type"}</dt><dd>{permissionPreview.hasLuaRuntime
                ? (locale.startsWith("zh") ? "运行时插件" : "Runtime plugin")
                : (locale.startsWith("zh") ? "内容插件" : "Content plugin")}</dd></div>
              <div><dt>{locale.startsWith("zh") ? "技能" : "Skills"}</dt><dd>{permissionPreview.skillIds.join("、") || "—"}</dd></div>
              <div><dt>{locale.startsWith("zh") ? "智能体" : "Agents"}</dt><dd>{permissionPreview.agentIds.join("、") || "—"}</dd></div>
              <div>
                <dt>MCP</dt>
                <dd>{permissionPreview.mcpServers.map((server) => {
                  const secretUsage = server.secretKeys.length > 0
                    ? ` · ${locale.startsWith("zh") ? "Secret 键" : "Secret keys"}: ${server.secretKeys.join(", ")}`
                    : "";
                  return `${server.name} (${server.serverId}, ${server.transport}${secretUsage})`;
                }).join("；") || "—"}</dd>
              </div>
            </dl>
            <label className="vulcan-plugin-permission-consent">
              <input
                checked={authorizationConfirmed}
                onChange={(event) => setAuthorizationConfirmed(event.target.checked)}
                type="checkbox"
              />
              <span>{locale.startsWith("zh")
                ? "我已核对并授权以上全部能力、Hook、挂载点和进程入口。"
                : "I reviewed and authorize every capability, hook, mount, and process entry above."}</span>
            </label>
            {permissionPreview.permissions.includes("dependency_registry_network") ? (
              <label className="vulcan-plugin-permission-consent warning">
                <input
                  checked={registryDependenciesConfirmed}
                  onChange={(event) => setRegistryDependenciesConfirmed(event.target.checked)}
                  type="checkbox"
                />
                <span>{locale.startsWith("zh")
                  ? "允许 Vulcan 在安装时联网从注册表获取该插件声明的锁定依赖。"
                  : "Allow Vulcan to access registries for this plugin's locked dependencies during installation."}</span>
              </label>
            ) : null}
            {permissionPreview.installSecretRequirements.length > 0 ? (
              <div className="vulcan-plugin-component-list">
                <strong>{locale.startsWith("zh") ? "本地 MCP 安装凭据" : "Local MCP installation credentials"}</strong>
                <p>{locale.startsWith("zh")
                  ? "这些值仅写入操作系统凭据库，并用于安装提交前的本地 MCP 启动验证。"
                  : "These values are written only to the operating-system credential store and used to validate local MCP startup before installation commits."}</p>
                {permissionPreview.installSecretRequirements.map((requirement) => {
                  const draftKey = `${requirement.serverId}:${requirement.secretKey}`;
                  return (
                    <div className="vulcan-plugin-install-input-row" key={draftKey}>
                      <KeyRound aria-hidden="true" size={15} />
                      <input
                        aria-label={`${requirement.serverId} ${requirement.secretKey}`}
                        autoComplete="new-password"
                        disabled={installBusy}
                        onChange={(event) => setInstallSecretDrafts((current) => ({
                          ...current,
                          [draftKey]: event.target.value,
                        }))}
                        placeholder={`${requirement.serverId} / ${requirement.secretKey}`}
                        type="password"
                        value={installSecretDrafts[draftKey] ?? ""}
                      />
                    </div>
                  );
                })}
              </div>
            ) : null}
            {inspectedInstalledPlugin ? (
              <div className={`skill-empty-state ${inspectedInstalledPlugin.extension?.origin === "userInstalled" && inspectedInstalledPlugin.runtimeState === "disabled" ? "" : "warning"}`}>
                {inspectedInstalledPlugin.extension?.origin !== "userInstalled"
                  ? (locale.startsWith("zh") ? "该 ID 已由内建插件占用，不能从本地包替换。" : "This ID is owned by a bundled plugin and cannot be replaced by a local package.")
                  : inspectedInstalledPlugin.runtimeState === "disabled"
                    ? (locale.startsWith("zh") ? "将更新现有用户插件；旧插件包会保留到候选验证与代次发布成功。" : "This will update the existing user plugin; its previous package is retained until candidate validation and generation publication succeed.")
                    : (locale.startsWith("zh") ? "更新前必须先停用并完全排空现有插件。" : "Disable and fully drain the existing plugin before updating it.")}
              </div>
            ) : null}
            <button
              className="primary-button"
              disabled={installBusy
                 || !authorizationConfirmed
                 || (inspectedInstalledPlugin !== undefined
                   && (inspectedInstalledPlugin.extension?.origin !== "userInstalled" || inspectedInstalledPlugin.runtimeState !== "disabled"))
                 || permissionPreview.installSecretRequirements.some((requirement) => !(installSecretDrafts[`${requirement.serverId}:${requirement.secretKey}`] ?? ""))
                 || (permissionPreview.permissions.includes("dependency_registry_network") && !registryDependenciesConfirmed)}
              onClick={() => void installInspectedPlugin()}
              type="button"
            >
              {inspectedInstalledPlugin?.extension?.origin === "userInstalled"
                ? (locale.startsWith("zh") ? "授权并更新" : "Authorize and update")
                : (locale.startsWith("zh") ? "授权并安装" : "Authorize and install")}
            </button>
          </div>
        ) : null}
      </section>
      <div className="agent-catalog-filter-row skill-list-filter-row plugin-skill-filter-row">
        <label className="agent-catalog-search-input skill-list-search-input">
          <Search size={15} />
          <input
            aria-label={t("skills.filter.searchLabel")}
            onChange={(event) => setSearchQuery(event.target.value)}
            placeholder={t("skills.filter.searchPlaceholder")}
            value={searchQuery}
          />
        </label>
        <select
          aria-label={t("skills.filter.statusLabel")}
          className="agent-catalog-status-select"
          onChange={(event) => setStatusFilter(event.target.value as VulcanPluginStatusFilter)}
          value={statusFilter}
        >
          <option value="all">{t("skills.filter.allStatuses")}</option>
          <option value="enabled">{t("skills.status.enabled")}</option>
          <option value="disabled">{t("skills.status.stopped")}</option>
          <option value="issues">{t("skills.filter.issues")}</option>
        </select>
      </div>
      {secretError ? <div className="vulcan-plugin-install-error">{secretError}</div> : null}
      <div className="skill-card-list agent-skill-card-list">
        {visiblePlugins.length === 0 ? (
          <div className="skill-empty-state">
            {vulcanPlugins.length === 0 ? t("skills.plugins.emptyDetail") : t("skills.filter.empty")}
          </div>
        ) : visiblePlugins.map((plugin) => {
          // localizedName follows the active UI locale instead of the service snapshot locale.
          // localizedName 跟随当前界面语言，而不是服务快照生成时的语言。
          const localizedName = localizedPluginText(plugin.nameByLocale, locale, plugin.name);
          // localizedCapability resolves the package-provided localized introduction independently.
          // localizedCapability 独立解析插件包提供的本地化介绍。
          const localizedCapability = localizedPluginText(plugin.capabilityByLocale, locale, plugin.capability);
          // showRuntimeStatus preserves actionable transient or exceptional lifecycle feedback only.
          // showRuntimeStatus 仅保留可操作的过渡态或异常生命周期反馈。
          const showRuntimeStatus = plugin.runtimeState !== "enabled" && plugin.runtimeState !== "disabled";
          // toggleLabel describes the exact action represented by the integrated switch.
          // toggleLabel 描述合并开关所代表的精确操作。
          const toggleLabel = plugin.runtimeState === "disabledWithCleanupWarning"
            ? t("skills.plugins.retryCleanup")
            : plugin.desiredEnabled ? t("skills.action.disable") : t("skills.action.enable");
          // primaryMountIcon is declared by the first plugin-detail mount available on this card.
          // primaryMountIcon 由当前卡片上首个可用的插件详情挂载点声明。
          const primaryMountIcon = plugin.extension?.uiMounts.find((mount) => mount.point === "skillsPluginDetail")?.icon ?? "plugin";
          // PluginIcon maps the stable manifest icon key without special-casing plugin identities.
          // PluginIcon 映射稳定的清单图标键，不对插件身份进行特判。
          const PluginIcon = primaryMountIcon === "route" ? Route : primaryMountIcon === "chart" ? ChartNoAxesCombined : Puzzle;
          return (
            <article className="agent-definition-row vulcan-plugin-definition-row" key={plugin.id}>
              <div className="agent-definition-row-icon" aria-hidden="true">
                <span className="agent-definition-row-glyph"><PluginIcon size={17} /></span>
              </div>
              <div className="agent-definition-row-main">
                <div className="agent-definition-row-title-line">
                  <strong className="agent-definition-row-title-text">{localizedName}</strong>
                  <div className="agent-definition-row-badges">
                    {showRuntimeStatus ? <StatusPill
                      label={pluginRuntimeStatusLabel(plugin, t)}
                      status={!plugin.available || plugin.diagnostics.length > 0 ? "warning" : plugin.enabled ? "ready" : "pending"}
                    /> : null}
                    <Badge variant="outline">{plugin.extension?.origin === "userInstalled"
                      ? (locale.startsWith("zh") ? "用户插件" : "User plugin")
                      : t("skills.plugins.systemKind")}</Badge>
                    {plugin.version ? <Badge variant="secondary">v{plugin.version}</Badge> : null}
                    <Badge variant="outline">{plugin.extension?.hasLuaRuntime
                      ? (locale.startsWith("zh") ? "运行时插件" : "Runtime plugin")
                      : (locale.startsWith("zh") ? "内容插件" : "Content plugin")}</Badge>
                    <Badge variant="outline">{locale.startsWith("zh") ? "技能" : "Skills"} {plugin.skillCount}</Badge>
                    <Badge variant="outline">{locale.startsWith("zh") ? "智能体" : "Agents"} {plugin.agentCount}</Badge>
                    <Badge variant="outline">{t("skills.plugins.tools")} {plugin.toolCount}</Badge>
                    <Badge variant="outline">{t("skills.plugins.hooks")} {plugin.hookCount}</Badge>
                    <Badge variant="outline">{t("skills.plugins.sessions")} {plugin.activeSessionCount}</Badge>
                  </div>
                </div>
                <p>{localizedCapability}</p>
                <span className={`agent-definition-row-path ${plugin.diagnostics.length > 0 ? "warning" : ""}`}>
                  {plugin.diagnostics.length > 0 ? plugin.diagnostics.join("；") : plugin.id}
                </span>
                {plugin.skillCount > 0 || plugin.agentCount > 0 ? (
                  <details className="vulcan-plugin-component-details">
                    <summary>{locale.startsWith("zh") ? "查看只读组件" : "View read-only components"}</summary>
                    {plugin.skillCount > 0 ? (
                      <div className="vulcan-plugin-component-group">
                        <strong>{locale.startsWith("zh") ? "技能" : "Skills"}</strong>
                        <div>{plugin.extension?.skills.map((skillId) => (
                          <Badge key={skillId} variant="secondary">{skillId}</Badge>
                        ))}</div>
                      </div>
                    ) : null}
                    {plugin.agentCount > 0 ? (
                      <div className="vulcan-plugin-component-group">
                        <strong>{locale.startsWith("zh") ? "智能体" : "Agents"}</strong>
                        <div>{plugin.extension?.agents.map((agentId) => (
                          <Badge key={agentId} variant="secondary">{agentId}</Badge>
                        ))}</div>
                      </div>
                    ) : null}
                  </details>
                ) : null}
                {(plugin.extension?.mcpServers.length ?? 0) > 0 ? (
                  <details className="vulcan-plugin-component-details">
                    <summary>{locale.startsWith("zh") ? "MCP 服务器与凭据" : "MCP servers and credentials"}</summary>
                    {plugin.extension?.mcpServers.map((server) => (
                      <div className="vulcan-plugin-component-group" key={server.id}>
                        <div className="agent-definition-row-title-line">
                          <strong>{server.name}</strong>
                          <div className="agent-definition-row-badges">
                            <StatusPill
                              label={server.status}
                              status={server.status === "ready" ? "ready" : server.status === "failed" || server.status === "degraded" ? "warning" : "pending"}
                            />
                            <Badge variant="outline">{server.transport}</Badge>
                            <Badge variant="outline">gen {server.generation}</Badge>
                            <Badge variant="outline">retry {server.retryCount}</Badge>
                            <Badge variant="outline">tools {server.toolCount}</Badge>
                            <Badge variant="outline">prompts {server.promptCount}</Badge>
                            <Badge variant="outline">resources {server.resourceCount}</Badge>
                          </div>
                        </div>
                        <span className={`agent-definition-row-path ${server.lastError ? "warning" : ""}`}>
                          {server.lastError ?? server.endpoint}
                        </span>
                        {(server.recentLogs?.length ?? 0) > 0 ? (
                          <details className="vulcan-plugin-component-details">
                            <summary>{locale.startsWith("zh") ? "最近脱敏日志" : "Recent sanitized logs"}</summary>
                            <pre className="agent-definition-row-path">{server.recentLogs?.join("\n")}</pre>
                          </details>
                        ) : null}
                        {server.secretKeys.map((secretKey) => {
                          const draftKey = `${plugin.id}:${server.id}:${secretKey}`;
                          return (
                            <div className="vulcan-plugin-install-input-row" key={secretKey}>
                              <KeyRound aria-hidden="true" size={15} />
                              <input
                                aria-label={`${server.name} ${secretKey}`}
                                autoComplete="new-password"
                                disabled={secretBusyKey !== null}
                                onChange={(event) => setSecretDrafts((current) => ({
                                  ...current,
                                  [draftKey]: event.target.value,
                                }))}
                                placeholder={secretKey}
                                type="password"
                                value={secretDrafts[draftKey] ?? ""}
                              />
                              <button
                                className="secondary-button"
                                disabled={secretBusyKey !== null || !(secretDrafts[draftKey] ?? "")}
                                onClick={() => void storePluginMcpSecret(plugin.id, server.id, secretKey)}
                                type="button"
                              >
                                {locale.startsWith("zh") ? "安全保存" : "Store securely"}
                              </button>
                              <button
                                className="icon-button danger"
                                disabled={secretBusyKey !== null}
                                onClick={() => void deletePluginMcpSecret(plugin.id, server.id, secretKey)}
                                title={locale.startsWith("zh") ? "删除 Secret" : "Delete secret"}
                                type="button"
                              >
                                <Trash2 size={14} />
                              </button>
                            </div>
                          );
                        })}
                      </div>
                    ))}
                  </details>
                ) : null}
              </div>
              <div className="agent-definition-row-actions">
              {plugin.extension?.origin === "userInstalled" ? (
                <button
                  aria-label={locale.startsWith("zh") ? "卸载用户插件" : "Uninstall user plugin"}
                  className="icon-button danger"
                  disabled={installBusy || plugin.runtimeState !== "disabled"}
                  onClick={() => void uninstallPlugin(plugin)}
                  title={plugin.runtimeState === "disabled"
                    ? (locale.startsWith("zh") ? "卸载" : "Uninstall")
                    : (locale.startsWith("zh") ? "请先停用插件" : "Disable the plugin first")}
                  type="button"
                >
                  <Trash2 size={15} />
                </button>
              ) : null}
              <button
                aria-busy={busyPluginId === plugin.id}
                aria-checked={plugin.desiredEnabled}
                aria-label={toggleLabel}
                className={`agent-definition-switch ${plugin.desiredEnabled ? "on" : ""}`}
                disabled={!plugin.available || busyPluginId !== null || plugin.runtimeState === "enabling" || plugin.runtimeState === "draining"}
                onClick={() => void togglePlugin(plugin)}
                role="switch"
                title={toggleLabel}
                type="button"
              >
                <span />
              </button>
              </div>
            </article>
          );
        })}
      </div>
    </div>
  );
}
