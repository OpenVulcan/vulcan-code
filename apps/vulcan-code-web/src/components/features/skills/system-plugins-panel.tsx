import * as React from "react";
import { Puzzle, Search } from "lucide-react";
import { Badge } from "@/components/ui/badge";
import { StatusPill } from "@/components/shared/status";
import { useI18n } from "@/i18n";
import type { PluginSummary } from "@/types";
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

  return (
    <div className="skill-page-body lua-skill-page-body">
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
          return (
            <article className="agent-definition-row system-plugin-definition-row" key={plugin.id}>
              <div className="agent-definition-row-icon" aria-hidden="true">
                <span className="agent-definition-row-glyph"><Puzzle size={17} /></span>
              </div>
              <div className="agent-definition-row-main">
                <div className="agent-definition-row-title-line">
                  <strong className="agent-definition-row-title-text">{localizedName}</strong>
                  <div className="agent-definition-row-badges">
                    {showRuntimeStatus ? <StatusPill
                      label={pluginRuntimeStatusLabel(plugin, t)}
                      status={!plugin.available || plugin.diagnostics.length > 0 ? "warning" : plugin.enabled ? "ready" : "pending"}
                    /> : null}
                    <Badge variant="outline">{t("skills.plugins.systemKind")}</Badge>
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
