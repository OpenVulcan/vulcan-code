import * as React from "react";
import { Puzzle, Route, Search, ShieldCheck, Trash2 } from "lucide-react";
import { Badge } from "@/components/ui/badge";
import { StatusPill } from "@/components/shared/status";
import { useI18n } from "@/i18n";
import type { PluginSummary, SystemPluginPermissionPreview } from "@/types";
import type { SkillActionProps } from "./skill-area-types";

// SystemPluginStatusFilter identifies the lifecycle subset shown in the System Plugin list.
// SystemPluginStatusFilter 标识系统插件列表中显示的生命周期子集。
type SystemPluginStatusFilter = "all" | "enabled" | "disabled" | "issues";

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

// systemPluginMatchesSearch checks one plugin against localized user-visible fields.
// systemPluginMatchesSearch 使用本地化后的用户可见字段检查单个插件是否匹配搜索词。
//
// Parameters: plugin is the projected plugin row, name and capability are localized display values, and query is the current filter.
// 参数：plugin 是投影插件行，name 与 capability 是本地化显示值，query 是当前筛选词。
// Returns: true when the normalized query is empty or matches a visible plugin field.
// 返回值：规范化查询为空或匹配可见插件字段时返回 true。
function systemPluginMatchesSearch(
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
  return [plugin.id, plugin.kind, plugin.version ?? "", name, capability]
    .some((value) => value.toLocaleLowerCase().includes(normalizedQuery));
}

// systemPluginHasIssue reports whether one plugin is unavailable or outside a normal lifecycle transition.
// systemPluginHasIssue 判断单个插件是否不可用或处于正常生命周期过渡之外。
//
// Parameters: plugin is the exact projected System Plugin row.
// 参数：plugin 是精确投影的系统插件行。
// Returns: true for diagnostics, unavailable assets, cleanup warnings, unsupported platforms, and unknown runtime states.
// 返回值：存在诊断、资产不可用、清理警告、平台不支持或未知运行时状态时返回 true。
function systemPluginHasIssue(plugin: PluginSummary): boolean {
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

// SystemPluginsPanel renders host-owned LuaSkills System Plugins and lifecycle controls.
// SystemPluginsPanel 渲染宿主自有 LuaSkills System Plugin 及其生命周期控件。
export function SystemPluginsPanel(props: {
  // Service-backed plugin lifecycle actions.
  // 服务端支持的插件生命周期动作。
  actions: SkillActionProps;
  // Plugin rows from the committed LuaSkills domain projection.
  // 来自已提交 LuaSkills 领域投影的插件行。
  plugins: PluginSummary[];
}) {
  // locale and t resolve plugin manifest text and stable plugin-page labels.
  // locale 与 t 分别解析插件清单文案和稳定的插件页面文案。
  const { locale, t } = useI18n();
  // searchQuery stores the free-text System Plugin filter.
  // searchQuery 保存系统插件自由文本筛选词。
  const [searchQuery, setSearchQuery] = React.useState("");
  // statusFilter stores the selected System Plugin lifecycle filter.
  // statusFilter 保存选中的系统插件生命周期筛选。
  const [statusFilter, setStatusFilter] = React.useState<SystemPluginStatusFilter>("all");
  // busyPluginId identifies the single System Plugin mutation currently in flight.
  // busyPluginId 标识当前正在执行变更的唯一 System Plugin。
  const [busyPluginId, setBusyPluginId] = React.useState<string | null>(null);
  // installSourcePath stores the exact local plugin directory selected for inspection.
  // installSourcePath 保存等待检查的精确本地插件目录。
  const [installSourcePath, setInstallSourcePath] = React.useState("");
  // permissionPreview stores only the latest host-validated package authorization surface.
  // permissionPreview 仅保存最近一次经宿主校验的插件包授权界面数据。
  const [permissionPreview, setPermissionPreview] = React.useState<SystemPluginPermissionPreview | null>(null);
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
  // actionRunningRef closes the repeat-click window before React commits the busy state.
  // actionRunningRef 在 React 提交忙碌状态前关闭重复点击窗口。
  const actionRunningRef = React.useRef(false);
  // systemPlugins keeps ordinary Lua Skill rows on their existing catalog page.
  // systemPlugins 使普通 Lua Skill 行继续留在原有技能目录页面。
  const systemPlugins = React.useMemo(
    () => props.plugins.filter((plugin) => plugin.managementKind === "systemPlugin"),
    [props.plugins],
  );
  // visiblePlugins applies localized text and lifecycle filters to the System Plugin projection.
  // visiblePlugins 对系统插件投影应用本地化文本与生命周期筛选。
  const visiblePlugins = React.useMemo(() => systemPlugins.filter((plugin) => {
    if (statusFilter === "enabled" && !plugin.desiredEnabled) {
      return false;
    }
    if (statusFilter === "disabled" && plugin.desiredEnabled) {
      return false;
    }
    if (statusFilter === "issues" && !systemPluginHasIssue(plugin)) {
      return false;
    }
    // localizedName is the active-locale plugin name used by both filtering and rendering.
    // localizedName 是同时用于筛选和渲染的当前语言插件名称。
    const localizedName = localizedPluginText(plugin.nameByLocale, locale, plugin.name);
    // localizedCapability is the active-locale plugin introduction used by both filtering and rendering.
    // localizedCapability 是同时用于筛选和渲染的当前语言插件介绍。
    const localizedCapability = localizedPluginText(plugin.capabilityByLocale, locale, plugin.capability);
    return systemPluginMatchesSearch(plugin, localizedName, localizedCapability, searchQuery);
  }), [locale, searchQuery, statusFilter, systemPlugins]);
  // togglePlugin persists one System Plugin state through the LuaSkills catalog transaction.
  // togglePlugin 通过 LuaSkills 目录事务持久化单个 System Plugin 状态。
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
      await props.actions.onSetSystemPluginEnabled(plugin.id, requestedEnabled);
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
    try {
      setPermissionPreview(await props.actions.onInspectLocalPlugin(sourcePath));
    } catch (reason: unknown) {
      setInstallError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setInstallBusy(false);
    }
  }, [installBusy, installSourcePath, props.actions]);

  // installInspectedPlugin installs only the unchanged path and exact host-projected capability set.
  // installInspectedPlugin 仅使用未变化路径与宿主投影的精确能力集合安装插件。
  const installInspectedPlugin = React.useCallback(async () => {
    if (permissionPreview === null || !authorizationConfirmed || installBusy) {
      return;
    }
    const requiresRegistryConsent = permissionPreview.capabilities.includes("dependency_registry_network");
    if (requiresRegistryConsent && !registryDependenciesConfirmed) {
      return;
    }
    setInstallBusy(true);
    setInstallError("");
    try {
      await props.actions.onInstallLocalPlugin(
        installSourcePath.trim(),
        permissionPreview.capabilities,
        registryDependenciesConfirmed,
      );
      setInstallSourcePath("");
      setPermissionPreview(null);
      setAuthorizationConfirmed(false);
      setRegistryDependenciesConfirmed(false);
    } catch (reason: unknown) {
      setInstallError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setInstallBusy(false);
    }
  }, [
    authorizationConfirmed,
    installBusy,
    installSourcePath,
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

  return (
    <div className="skill-page-body lua-skill-page-body">
      <section className="system-plugin-install-panel" aria-label={locale.startsWith("zh") ? "安装本地插件" : "Install local plugin"}>
        <div className="system-plugin-install-heading">
          <ShieldCheck size={17} />
          <div>
            <strong>{locale.startsWith("zh") ? "安装本地插件" : "Install local plugin"}</strong>
            <p>{locale.startsWith("zh")
              ? "先检查插件包，再明确授权其能力、Hook、界面挂载点和进程入口。"
              : "Inspect the package before explicitly authorizing its capabilities, hooks, UI mounts, and process entries."}</p>
          </div>
        </div>
        <div className="system-plugin-install-input-row">
          <input
            aria-label={locale.startsWith("zh") ? "本地插件目录" : "Local plugin directory"}
            disabled={installBusy}
            onChange={(event) => {
              setInstallSourcePath(event.target.value);
              setPermissionPreview(null);
              setAuthorizationConfirmed(false);
              setRegistryDependenciesConfirmed(false);
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
          <div className="system-plugin-permission-preview">
            <div className="system-plugin-permission-title">
              <strong>{permissionPreview.pluginId}</strong>
              <Badge variant="secondary">v{permissionPreview.version}</Badge>
            </div>
            <dl>
              <div><dt>{locale.startsWith("zh") ? "能力" : "Capabilities"}</dt><dd>{permissionPreview.capabilities.join("、") || "—"}</dd></div>
              <div><dt>Hooks</dt><dd>{permissionPreview.hookPoints.join("、") || "—"}</dd></div>
              <div><dt>{locale.startsWith("zh") ? "界面挂载点" : "UI mounts"}</dt><dd>{permissionPreview.uiMountPoints.join("、") || "—"}</dd></div>
              <div><dt>{locale.startsWith("zh") ? "原生进程" : "Native processes"}</dt><dd>{permissionPreview.binaryIds.join("、") || "—"}</dd></div>
              <div><dt>Node.js</dt><dd>{permissionPreview.nodeEntryIds.join("、") || "—"}</dd></div>
              <div><dt>Python</dt><dd>{permissionPreview.pythonEntryIds.join("、") || "—"}</dd></div>
            </dl>
            <label className="system-plugin-permission-consent">
              <input
                checked={authorizationConfirmed}
                onChange={(event) => setAuthorizationConfirmed(event.target.checked)}
                type="checkbox"
              />
              <span>{locale.startsWith("zh")
                ? "我已核对并授权以上全部能力、Hook、挂载点和进程入口。"
                : "I reviewed and authorize every capability, hook, mount, and process entry above."}</span>
            </label>
            {permissionPreview.capabilities.includes("dependency_registry_network") ? (
              <label className="system-plugin-permission-consent warning">
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
            <button
              className="primary-button"
              disabled={installBusy
                || !authorizationConfirmed
                || (permissionPreview.capabilities.includes("dependency_registry_network") && !registryDependenciesConfirmed)}
              onClick={() => void installInspectedPlugin()}
              type="button"
            >
              {locale.startsWith("zh") ? "授权并安装" : "Authorize and install"}
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
          onChange={(event) => setStatusFilter(event.target.value as SystemPluginStatusFilter)}
          value={statusFilter}
        >
          <option value="all">{t("skills.filter.allStatuses")}</option>
          <option value="enabled">{t("skills.status.enabled")}</option>
          <option value="disabled">{t("skills.status.stopped")}</option>
          <option value="issues">{t("skills.filter.issues")}</option>
        </select>
      </div>
      <div className="skill-card-list agent-skill-card-list">
        {visiblePlugins.length === 0 ? (
          <div className="skill-empty-state">
            {systemPlugins.length === 0 ? t("skills.plugins.emptyDetail") : t("skills.filter.empty")}
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
          // PluginIcon preserves the original RTK route symbol while other plugins retain the generic plugin glyph.
          // PluginIcon 保留原有 RTK 路由图标，其他插件继续使用通用插件图标。
          const PluginIcon = plugin.id === "vulcan-rtk" ? Route : Puzzle;
          return (
            <article className="agent-definition-row system-plugin-definition-row" key={plugin.id}>
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
                    <Badge variant="outline">{t("skills.plugins.tools")} {plugin.toolCount}</Badge>
                    <Badge variant="outline">{t("skills.plugins.hooks")} {plugin.hookCount}</Badge>
                    <Badge variant="outline">{t("skills.plugins.sessions")} {plugin.activeSessionCount}</Badge>
                  </div>
                </div>
                <p>{localizedCapability}</p>
                <span className={`agent-definition-row-path ${plugin.diagnostics.length > 0 ? "warning" : ""}`}>
                  {plugin.diagnostics.length > 0 ? plugin.diagnostics.join("；") : plugin.id}
                </span>
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
