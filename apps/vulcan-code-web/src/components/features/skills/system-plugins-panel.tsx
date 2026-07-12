import * as React from "react";
import { Power, Puzzle } from "lucide-react";
import { Badge } from "@/components/ui/badge";
import { StatusPill } from "@/components/shared/status";
import { useI18n } from "@/i18n";
import type { PluginSummary } from "@/types";
import type { SkillActionProps } from "./skill-area-types";

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
  // t resolves localized plugin-page labels.
  // t 解析插件页面本地化文案。
  const { t } = useI18n();
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
      <div className="skill-card-list">
        {systemPlugins.length === 0 ? (
          <article className="skill-empty-card">
            <Puzzle size={18} />
            <strong>{t("skills.plugins.emptyTitle")}</strong>
            <span>{t("skills.plugins.emptyDetail")}</span>
          </article>
        ) : systemPlugins.map((plugin) => (
          <article className="skill-card lua-skill-card" key={plugin.id}>
            <header className="skill-card-header">
              <div className="skill-card-title">
                <div className="skill-card-title-line">
                  <strong>{plugin.name}</strong>
                  <div className="skill-card-title-meta">
                    <StatusPill
                      label={pluginRuntimeStatusLabel(plugin, t)}
                      status={!plugin.available || plugin.diagnostics.length > 0 ? "warning" : plugin.enabled ? "ready" : "pending"}
                    />
                    <Badge variant="outline">{t("skills.plugins.systemKind")}</Badge>
                  </div>
                </div>
              </div>
            </header>
            <div className="skill-card-summary">
              <span>{plugin.capability}</span>
            </div>
            <div className="skill-card-metrics">
              <span>{plugin.id}</span>
              {plugin.version ? <span>v{plugin.version}</span> : null}
              <span>{t("skills.plugins.tools")}: {plugin.toolCount}</span>
              <span>{t("skills.plugins.hooks")}: {plugin.hookCount}</span>
              <span>{t("skills.plugins.sessions")}: {plugin.activeSessionCount}</span>
            </div>
            {plugin.diagnostics.length > 0 ? (
              <div className="skill-card-summary warning" role="status">
                <strong>{t("skills.plugins.diagnostics")}</strong>
                <span>{plugin.diagnostics.join("；")}</span>
              </div>
            ) : null}
            <div className="skill-actions-row">
              <button
                className="skill-action-button secondary"
                disabled={!plugin.available || busyPluginId !== null || plugin.runtimeState === "enabling" || plugin.runtimeState === "draining"}
                onClick={() => void togglePlugin(plugin)}
                type="button"
              >
                <Power size={15} />
                <span>{busyPluginId === plugin.id || plugin.runtimeState === "enabling" || plugin.runtimeState === "draining" ? t("skills.lua.syncing") : plugin.runtimeState === "disabledWithCleanupWarning" ? t("skills.plugins.retryCleanup") : plugin.desiredEnabled ? t("skills.action.disable") : t("skills.action.enable")}</span>
              </button>
            </div>
          </article>
        ))}
      </div>
    </div>
  );
}
