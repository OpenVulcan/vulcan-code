import * as React from "react";
import { Bot, ChartNoAxesCombined, Cpu, FileText, FolderOpen, Hammer, Info, KeyRound, MessageSquareText, MessagesSquare, Puzzle, Route, ScrollText, Search, Server, ShieldCheck, Trash2, TriangleAlert, Webhook, Wrench, type LucideIcon } from "lucide-react";
import { Badge } from "@/components/ui/badge";
import { Button, IconButton } from "@/components/ui/button";
import { AlertDialog, AlertDialogAction, AlertDialogCancel, AlertDialogContent, AlertDialogDescription, AlertDialogFooter, AlertDialogHeader, AlertDialogTitle } from "@/components/ui/alert-dialog";
import { Checkbox } from "@/components/ui/checkbox";
import { Input } from "@/components/ui/input";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { Switch } from "@/components/ui/switch";
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from "@/components/ui/tooltip";
import { StatusPill } from "@/components/shared/status";
import { formatMessage, useI18n } from "@/i18n";
import { CoreBrowserRuntimeControls } from "@/components/features/browser/core-browser-runtime-controls";
import { PluginRuntimeControls } from "./plugin-runtime-controls";
import { PluginUpdateControls } from "./plugin-update-controls";
import { PluginResourceDiagnostics } from "./plugin-resource-diagnostics";
import type { PluginDiagnosticsSelection } from "@/types/plugin-resource-diagnostics";
import { PluginRequestRecoveryControls } from "./plugin-request-recovery-controls";
import { PluginUninstallRecoveryControls, type PluginUninstallUncertainty } from "./plugin-uninstall-recovery-controls";
import type { PluginSummary, VulcanPluginPermissionPreview, WorkbenchSnapshot } from "@/types";
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

// pluginMcpStatusLabel resolves one plugin-owned MCP server state key through the active UI locale.
// pluginMcpStatusLabel 通过当前界面语言解析一个插件自有 MCP 服务器状态键。
//
// Parameters: status is the stable runtime state key and t is the active translation resolver.
// 参数：status 是稳定运行时状态键，t 是当前翻译解析器。
// Returns: the localized label, or the raw key for a state this client does not know yet.
// 返回值：本地化标签；遇到当前客户端尚不认识的状态时返回原始键。
function pluginMcpStatusLabel(status: string, t: ReturnType<typeof useI18n>["t"]): string {
  switch (status) {
    case "disabled": return t("skills.plugins.mcpStatus.disabled");
    case "preparing": return t("skills.plugins.mcpStatus.preparing");
    case "connecting": return t("skills.plugins.mcpStatus.connecting");
    case "ready": return t("skills.plugins.mcpStatus.ready");
    case "degraded": return t("skills.plugins.mcpStatus.degraded");
    case "failed": return t("skills.plugins.mcpStatus.failed");
    case "draining": return t("skills.plugins.mcpStatus.draining");
    case "stopped": return t("skills.plugins.mcpStatus.stopped");
    default: return status;
  }
}

// PluginFact renders one secondary plugin fact as an icon, optionally with a count, and explains it in a tooltip.
// PluginFact 把一项次要插件信息渲染为图标（可附带数量），并在 Tooltip 中说明。
//
// Parameters: icon and label name the fact, value is an optional count, items lists contributed identifiers,
// and tooltip replaces the default "label value" explanation.
// 参数：icon 与 label 命名该信息，value 是可选数量，items 列出贡献的标识，tooltip 替换默认的“标签 数量”说明。
// Returns: one focusable fact chip with a portal-backed tooltip.
// 返回值：一个可聚焦且带 Portal Tooltip 的信息标签。
function PluginFact(props: { icon: LucideIcon; label: string; value?: number; items?: string[]; tooltip?: React.ReactNode }) {
  const Icon = props.icon;
  // summary is the accessible and default tooltip text of the fact.
  // summary 是该信息的无障碍文本与默认 Tooltip 文本。
  const summary = props.value === undefined ? props.label : `${props.label} ${props.value}`;
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <span aria-label={summary} className="vulcan-plugin-fact" role="img" tabIndex={0}>
          <Icon aria-hidden="true" size={13} />
          {props.value === undefined ? null : <span aria-hidden="true">{props.value}</span>}
        </span>
      </TooltipTrigger>
      <TooltipContent className="vulcan-plugin-fact-tooltip" side="top" sideOffset={5}>
        {props.tooltip ?? (props.items?.length ? <div className="vulcan-plugin-fact-items">
          <strong>{summary}</strong>
          {props.items.map((item) => <span key={item}>{item}</span>)}
        </div> : summary)}
      </TooltipContent>
    </Tooltip>
  );
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
  // conversations carries authoritative owner IDs for resource navigation.
  // conversations 携带资源导航的权威所有者 ID。
  conversations?: WorkbenchSnapshot["conversations"];
  // onSelectPluginResourceOwner follows the existing service-backed workspace selection chain.
  // onSelectPluginResourceOwner 遵循既有服务支持的工作台选择链。
  onSelectPluginResourceOwner?: (conversationId: string, observation: AbortSignal) => Promise<boolean>;
}) {
  // diagnosticsSelection retains the exact card or original management request identity.
  // diagnosticsSelection 保留精确卡片或原管理请求身份。
  const [diagnosticsSelection, setDiagnosticsSelection] = React.useState<PluginDiagnosticsSelection | null>(null);
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
  // environmentReadyByPlugin records the service-checked readiness of each exact plugin generation.
  // environmentReadyByPlugin 记录每个精确插件代次经服务检查后的就绪状态。
  const [environmentReadyByPlugin, setEnvironmentReadyByPlugin] = React.useState<Record<string, boolean>>({});
  // installTasksByPlugin reflects Core-owned work, including work started by another client.
  // installTasksByPlugin 反映 Core 持有的任务，包括其他客户端启动的任务。
  const [installTasksByPlugin, setInstallTasksByPlugin] = React.useState<Record<string, import("../../../service/skills-actions-service").PluginEnvironmentInstallTaskStatus>>({});
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
  // uninstallUncertainty prevents ordinary replay while the original foreground result is unconfirmed.
  // uninstallUncertainty 在原前台结果未确认时阻止普通重放。
  const [uninstallUncertainty, setUninstallUncertainty] = React.useState<PluginUninstallUncertainty | null>(null);
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
  // pendingDestructive retains exact public identities without retaining secret values.
  // pendingDestructive 保留精确公开身份，不保留秘密值。
  const [pendingDestructive, setPendingDestructive] = React.useState<
    { kind: "uninstall"; plugin: PluginSummary } | { kind: "secret"; pluginId: string; serverId: string; secretKey: string } | null
  >(null);
  // mutationBusy synchronizes every lifecycle, installation and credential control on this page.
  // mutationBusy 同步当前页面所有生命周期、安装与凭据控件。
  const mutationBusy = installBusy || busyPluginId !== null || secretBusyKey !== null;
  // Recovery acquires the same synchronous page gate before any service mutation.
  // 恢复在任何服务变更之前取得相同同步页面门。
  // Returns false when another page action owns admission, otherwise reserves it immediately.
  // 另一页面动作拥有入场时返回假，否则立即预留。
  const acquireRecoveryMutation = React.useCallback((): boolean => {
    if (actionRunningRef.current) return false;
    actionRunningRef.current = true; setInstallBusy(true); return true;
  }, []);
  // Release only the recovery's acquired page admission after its outcome observation.
  // 在结果观测之后仅释放恢复取得的页面入场。
  // No arguments or return value; service-owned resources remain governed by the original request.
  // 无参数或返回值；服务自有资源仍由原请求管理。
  const releaseRecoveryMutation = React.useCallback((): void => {
    actionRunningRef.current = false; setInstallBusy(false);
  }, []);
  // vulcanPlugins keeps ordinary Lua Skill rows on their existing catalog page.
  // vulcanPlugins 使普通 Lua Skill 行继续留在原有技能目录页面。
  const vulcanPlugins = React.useMemo(
    () => props.plugins.filter((plugin) => plugin.managementKind === "vulcanPlugin"),
    [props.plugins],
  );
  // Refresh readiness when the committed plugin catalog changes; stale responses cannot replace a newer view.
  // 已提交插件目录变化时刷新就绪状态；过期响应不能覆盖较新的视图。
  React.useEffect(() => {
    // active prevents an outdated catalog request from publishing readiness after cleanup.
    // active 防止过期目录请求在清理后发布就绪状态。
    let active = true;
    void Promise.all(vulcanPlugins.map(async (plugin) => {
      // ready is read from Core, which owns the plugin generation and private environment.
      // ready 从持有插件代次和私有环境的 Core 读取。
      const ready = await props.actions.onPluginEnvironmentReady(plugin.id).catch(() => false);
      const task = await props.actions.onPluginEnvironmentInstallStatusForPlugin(plugin.id).catch(() => null);
      return [plugin.id, ready, task] as const;
    })).then((entries) => {
      if (active) {
        setEnvironmentReadyByPlugin(Object.fromEntries(entries.map(([id, ready]) => [id, ready])));
        setInstallTasksByPlugin(Object.fromEntries(entries.flatMap(([id, , task]) => task ? [[id, task]] : [])));
      }
    });
    return () => { active = false; };
  }, [props.actions, vulcanPlugins]);
  // Poll only unfinished Core tasks; unmounting the panel never cancels their installation.
  // 只轮询尚未完成的 Core 任务；卸载面板绝不取消安装。
  React.useEffect(() => {
    const pending = Object.values(installTasksByPlugin).filter((task) => task.state === "queued" || task.state === "running");
    if (pending.length === 0) return;
    let active = true;
    let polling = false;
    const poll = async () => {
      if (polling) return;
      polling = true;
      try {
        const updates = await Promise.all(pending.map((task) => props.actions.onPluginEnvironmentInstallStatus(task.taskId)));
        if (!active) return;
        setInstallTasksByPlugin((current) => {
          const next = { ...current };
          for (const task of updates) {
            if (next[task.pluginId]?.taskId === task.taskId) next[task.pluginId] = task;
          }
          return next;
        });
        for (const task of updates) {
          if (task.state === "succeeded") {
            const ready = await props.actions.onPluginEnvironmentReady(task.pluginId);
            if (active) setEnvironmentReadyByPlugin((current) => ({ ...current, [task.pluginId]: ready }));
          }
        }
      } catch (reason) {
        if (active) setInstallError(reason instanceof Error ? reason.message : String(reason));
      } finally {
        polling = false;
      }
    };
    void poll();
    const timer = window.setInterval(() => { void poll(); }, 1000);
    return () => { active = false; window.clearInterval(timer); };
  }, [installTasksByPlugin, props.actions]);
  // A second client can start installation while this panel is open, so refresh Core's latest task by plugin.
  // 其他客户端可能在本面板打开期间启动安装，因此按插件刷新 Core 的最近任务。
  React.useEffect(() => {
    if (vulcanPlugins.length === 0) return;
    let active = true;
    const synchronize = async () => {
      const entries = await Promise.all(vulcanPlugins.map(async (plugin) => [
        plugin.id,
        await props.actions.onPluginEnvironmentInstallStatusForPlugin(plugin.id).catch(() => null),
      ] as const));
      if (!active) return;
      setInstallTasksByPlugin(Object.fromEntries(entries.flatMap(([id, task]) => task ? [[id, task] as const] : [])));
      const succeeded = entries.filter(([, task]) => task?.state === "succeeded");
      const readiness = await Promise.all(succeeded.map(async ([id]) => [
        id,
        await props.actions.onPluginEnvironmentReady(id).catch(() => false),
      ] as const));
      if (active) setEnvironmentReadyByPlugin((current) => ({ ...current, ...Object.fromEntries(readiness) }));
    };
    const timer = window.setInterval(() => { void synchronize(); }, 3000);
    return () => { active = false; window.clearInterval(timer); };
  }, [props.actions, vulcanPlugins]);
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
    if (actionRunningRef.current || !plugin.available || (!plugin.desiredEnabled && environmentReadyByPlugin[plugin.id] !== true)) {
      return;
    }
    actionRunningRef.current = true;
    setBusyPluginId(plugin.id);
    setInstallError("");
    try {
      // requestedEnabled retries cleanup without accidentally re-enabling a warning-state plugin.
      // requestedEnabled 会重试清理，避免意外重新启用处于警告状态的插件。
      const requestedEnabled = plugin.runtimeState === "disabledWithCleanupWarning"
        ? false
        : !plugin.desiredEnabled;
      await props.actions.onSetVulcanPluginEnabled(plugin.id, requestedEnabled);
    } catch (reason) {
      setInstallError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      actionRunningRef.current = false;
      setBusyPluginId(null);
    }
  }, [environmentReadyByPlugin, props.actions]);

  // installPluginEnvironment starts one detached Core task without changing the plugin state.
  // installPluginEnvironment 启动一个分离的 Core 任务，不改变插件启用状态。
  // Parameters: plugin is the exact catalog row selected by the user.
  // 参数：plugin 是用户选择的精确目录行。
  // Returns: a promise that settles after Core accepts the task identity.
  // 返回值：Core 接受任务身份后结束的 Promise。
  const installPluginEnvironment = React.useCallback(async (plugin: PluginSummary) => {
    const currentTask = installTasksByPlugin[plugin.id];
    if (actionRunningRef.current || !plugin.available || plugin.desiredEnabled || plugin.runtimeState !== "disabled"
      || currentTask?.state === "queued" || currentTask?.state === "running") {
      return;
    }
    actionRunningRef.current = true;
    setBusyPluginId(plugin.id);
    setInstallError("");
    try {
      const task = await props.actions.onStartPluginEnvironmentInstall(plugin.id);
      setInstallTasksByPlugin((current) => ({ ...current, [plugin.id]: task }));
    } catch (reason) {
      setInstallError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      actionRunningRef.current = false;
      setBusyPluginId(null);
    }
  }, [installTasksByPlugin, props.actions]);

  // inspectPluginPackage validates the current path and resets every prior authorization decision.
  // inspectPluginPackage 校验当前路径并重置此前的所有授权决定。
  const inspectPluginPackage = React.useCallback(async () => {
    const sourcePath = installSourcePath.trim();
    if (!sourcePath || actionRunningRef.current) {
      return;
    }
    actionRunningRef.current = true;
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
      actionRunningRef.current = false;
      setInstallSecretDrafts({});
      setInstallBusy(false);
    }
  }, [installBusy, installSourcePath, props.actions]);

  // installInspectedPlugin installs only the unchanged path and exact host-projected capability set.
  // installInspectedPlugin 仅使用未变化路径与宿主投影的精确能力集合安装插件。
  const installInspectedPlugin = React.useCallback(async () => {
    if (permissionPreview === null || !authorizationConfirmed || actionRunningRef.current) {
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
    actionRunningRef.current = true;
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
      actionRunningRef.current = false;
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
    if (actionRunningRef.current || uninstallUncertainty !== null || plugin.extension?.origin !== "userInstalled" || plugin.runtimeState !== "disabled") {
      return;
    }
    actionRunningRef.current = true;
    setInstallBusy(true);
    setInstallError("");
    try {
      await props.actions.onUninstallUserPlugin(plugin.id);
      setPendingDestructive(null);
    } catch (reason: unknown) {
      // failures preserve the initial response failure separately from read-only discovery failure.
      // failures 独立于只读发现失败保留初始响应失败。
      const failures = [reason instanceof Error ? reason.message : String(reason)];
      // requestId comes only from the actual retained request registry, never an error string.
      // requestId 仅来自实际保留请求注册表，绝不来自错误字符串。
      let requestId: string | null = null;
      try {
        // matching selects this dispatched namespace without inferring success from an empty list.
        // matching 选择此已派发命名空间，不从空列表推断成功。
        const matching = (await props.actions.onReadPluginUninstallRequests()).filter((record) => record.pluginId === plugin.id);
        if (matching.length > 1) throw new Error(t("pluginRecovery.identityMismatch"));
        if (matching.length === 1) requestId = matching[0].requestId;
      } catch (observationError) { failures.push(String(observationError)); }
      setUninstallUncertainty({ pluginId: plugin.id, requestId });
      setInstallError(failures.join("\n"));
      setPendingDestructive(null);
    } finally {
      actionRunningRef.current = false;
      setInstallBusy(false);
    }
  }, [props.actions, t, uninstallUncertainty]);

  // storePluginMcpSecret writes one blank-by-default replacement value and clears the draft on every outcome.
  // storePluginMcpSecret 写入一个默认空白的替换值，并在所有结果下清空草稿。
  const storePluginMcpSecret = React.useCallback(async (
    pluginId: string,
    serverId: string,
    secretKey: string,
  ) => {
    const draftKey = `${pluginId}:${serverId}:${secretKey}`;
    const value = secretDrafts[draftKey] ?? "";
    if (!value || actionRunningRef.current) {
      return;
    }
    actionRunningRef.current = true;
    setSecretBusyKey(draftKey);
    setSecretError("");
    try {
      await props.actions.onSetPluginMcpSecret(pluginId, serverId, secretKey, value);
    } catch (reason: unknown) {
      setSecretError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      actionRunningRef.current = false;
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
    if (actionRunningRef.current) {
      return;
    }
    actionRunningRef.current = true;
    setSecretBusyKey(draftKey);
    setSecretError("");
    try {
      await props.actions.onDeletePluginMcpSecret(pluginId, serverId, secretKey);
      setPendingDestructive(null);
    } catch (reason: unknown) {
      setSecretError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      actionRunningRef.current = false;
      setSecretDrafts((current) => ({ ...current, [draftKey]: "" }));
      setSecretBusyKey(null);
    }
  }, [locale, props.actions, secretBusyKey]);

  return (
    <div className="skill-page-body lua-skill-page-body">
      <section className="vulcan-plugin-install-panel" aria-label={t("skills.plugins.installLocalPlugin")}>
        <div className="vulcan-plugin-install-heading">
          <ShieldCheck size={17} />
          <div>
            <strong>{t("skills.plugins.installLocalPlugin")}</strong>
            <p>{t("skills.plugins.inspectThePackageBefore")}</p>
          </div>
        </div>
        <div className="vulcan-plugin-install-input-row">
          <input
            aria-label={t("skills.plugins.localPluginDirectory")}
            disabled={mutationBusy}
            onChange={(event) => {
              setInstallSourcePath(event.target.value);
              setPermissionPreview(null);
              setAuthorizationConfirmed(false);
              setRegistryDependenciesConfirmed(false);
              setInstallSecretDrafts({});
            }}
            placeholder={t("skills.plugins.directoryContainingPluginYaml")}
            value={installSourcePath}
          />
          <Button
            disabled={mutationBusy || installSourcePath.trim().length === 0}
            onClick={() => void inspectPluginPackage()}
            size="sm"
            type="button"
            variant="secondary"
          >
            {t("skills.plugins.inspect")}
          </Button>
        </div>
        {installError ? <div className="skill-empty-state warning" role="alert">
          <p>{t("skills.plugins.pluginActionFailedTry")}</p>
          <details><summary>{t("skills.plugins.errorDetails")}</summary><pre className="plugin-control-error-details">{installError}</pre></details>
        </div> : null}
        {permissionPreview ? (
          <div className="vulcan-plugin-permission-preview">
            <div className="vulcan-plugin-permission-title">
              <strong>{permissionPreview.pluginId}</strong>
              <Badge variant="secondary">v{permissionPreview.version}</Badge>
            </div>
            <dl>
              <div><dt>{t("skills.plugins.permissions")}</dt><dd>{permissionPreview.permissions.join("、") || "—"}</dd></div>
              <div><dt>Hooks</dt><dd>{permissionPreview.hookPoints.join("、") || "—"}</dd></div>
              <div><dt>{t("skills.plugins.uiMounts")}</dt><dd>{permissionPreview.uiMountPoints.join("、") || "—"}</dd></div>
              <div><dt>{t("skills.plugins.nativeProcesses")}</dt><dd>{permissionPreview.binaryIds.join("、") || "—"}</dd></div>
              <div><dt>Node.js</dt><dd>{permissionPreview.nodeEntryIds.join("、") || "—"}</dd></div>
              <div><dt>Python</dt><dd>{permissionPreview.pythonEntryIds.join("、") || "—"}</dd></div>
              <div><dt>{t("skills.plugins.pluginType")}</dt><dd>{permissionPreview.hasLuaRuntime
                ? t("skills.plugins.runtimePlugin")
                : t("skills.plugins.contentPlugin")}</dd></div>
              <div><dt>{t("skills.plugins.skills")}</dt><dd>{permissionPreview.skillIds.join("、") || "—"}</dd></div>
              <div><dt>{t("skills.plugins.agents")}</dt><dd>{permissionPreview.agentIds.join("、") || "—"}</dd></div>
              <div>
                <dt>MCP</dt>
                <dd>{permissionPreview.mcpServers.map((server) => {
                  const secretUsage = server.secretKeys.length > 0
                    ? ` · ${t("skills.plugins.secretKeys")}: ${server.secretKeys.join(", ")}`
                    : "";
                  return `${server.name} (${server.serverId}, ${server.transport}${secretUsage})`;
                }).join("；") || "—"}</dd>
              </div>
            </dl>
            {permissionPreview.permissions.includes("system_vldb_access") ? (
              <div className="skill-empty-state warning">
                {t("skills.plugins.highTrustPermissionThis")}
              </div>
            ) : null}
            <label className="vulcan-plugin-permission-consent">
              <Checkbox
                checked={authorizationConfirmed}
                onCheckedChange={(checked) => setAuthorizationConfirmed(checked === true)}
              />
              <span>{t("skills.plugins.iReviewedAndAuthorize")}</span>
            </label>
            {permissionPreview.permissions.includes("dependency_registry_network") ? (
              <label className="vulcan-plugin-permission-consent warning">
                <Checkbox
                  checked={registryDependenciesConfirmed}
                  onCheckedChange={(checked) => setRegistryDependenciesConfirmed(checked === true)}
                />
                <span>{t("skills.plugins.allowVulcanToAccess")}</span>
              </label>
            ) : null}
            {permissionPreview.installSecretRequirements.length > 0 ? (
              <div className="vulcan-plugin-component-list">
                <strong>{t("skills.plugins.localMcpInstallationCredentials")}</strong>
                <p>{t("skills.plugins.theseValuesAreWritten")}</p>
                {permissionPreview.installSecretRequirements.map((requirement) => {
                  const draftKey = `${requirement.serverId}:${requirement.secretKey}`;
                  return (
                    <div className="vulcan-plugin-install-input-row" key={draftKey}>
                      <KeyRound aria-hidden="true" size={15} />
                      <input
                        aria-label={`${requirement.serverId} ${requirement.secretKey}`}
                        autoComplete="new-password"
                        disabled={mutationBusy}
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
                  ? t("skills.plugins.thisIdIsOwned")
                  : inspectedInstalledPlugin.runtimeState === "disabled"
                    ? t("skills.plugins.thisWillUpdateThe")
                    : t("skills.plugins.disableAndFullyDrain")}
              </div>
            ) : null}
            <Button
              disabled={mutationBusy
                 || !authorizationConfirmed
                 || (inspectedInstalledPlugin !== undefined
                   && (inspectedInstalledPlugin.extension?.origin !== "userInstalled" || inspectedInstalledPlugin.runtimeState !== "disabled"))
                 || permissionPreview.installSecretRequirements.some((requirement) => !(installSecretDrafts[`${requirement.serverId}:${requirement.secretKey}`] ?? ""))
                 || (permissionPreview.permissions.includes("dependency_registry_network") && !registryDependenciesConfirmed)}
              onClick={() => void installInspectedPlugin()}
              size="sm"
              type="button"
            >
              {inspectedInstalledPlugin?.extension?.origin === "userInstalled"
                ? t("skills.plugins.authorizeAndUpdate")
                : t("skills.plugins.authorizeAndInstall")}
            </Button>
          </div>
        ) : null}
      </section>
      <PluginRuntimeControls plugins={props.plugins} onRefreshCatalog={props.actions.onRefreshLuaSkills} />
      <PluginResourceDiagnostics selection={diagnosticsSelection} onSelect={setDiagnosticsSelection}
        conversations={props.conversations ?? []} onSelectPluginResourceOwner={props.onSelectPluginResourceOwner} />
      <PluginUpdateControls plugins={props.plugins} actions={props.actions}
        onLocateDiagnostics={(pluginId, requestId) => setDiagnosticsSelection({ kind: "update", pluginId, requestId })} />
      {(["installation", "credential"] as const).map((kind) => <PluginRequestRecoveryControls key={kind} kind={kind}
        plugins={vulcanPlugins} actions={props.actions} suggestedPluginId={permissionPreview === null ? "" : permissionPreview.pluginId}
        mutationBusy={mutationBusy} acquireMutation={acquireRecoveryMutation} releaseMutation={releaseRecoveryMutation} />)}
      <PluginUninstallRecoveryControls actions={props.actions} initialUncertainty={uninstallUncertainty}
        onLocateDiagnostics={(pluginId, requestId) => setDiagnosticsSelection({ kind: "uninstall", pluginId, requestId })}
        onConfirmed={(pluginId) => { if (uninstallUncertainty?.pluginId === pluginId) { setUninstallUncertainty(null); setInstallError(""); } }}
        mutationBusy={mutationBusy} acquireMutation={acquireRecoveryMutation} releaseMutation={releaseRecoveryMutation} />
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
        <Select onValueChange={(value) => setStatusFilter(value as VulcanPluginStatusFilter)} value={statusFilter}>
          <SelectTrigger aria-label={t("skills.filter.statusLabel")} className="agent-catalog-status-select">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value="all">{t("skills.filter.allStatuses")}</SelectItem>
            <SelectItem value="enabled">{t("skills.status.enabled")}</SelectItem>
            <SelectItem value="disabled">{t("skills.status.stopped")}</SelectItem>
            <SelectItem value="issues">{t("skills.filter.issues")}</SelectItem>
          </SelectContent>
        </Select>
      </div>
      {secretError ? <div className="vulcan-plugin-install-error" role="alert">
        <p>{t("skills.plugins.credentialActionFailedTry")}</p>
        <details><summary>{t("skills.plugins.errorDetails")}</summary><pre className="plugin-control-error-details">{secretError}</pre></details>
      </div> : null}
      <TooltipProvider delayDuration={300}>
      <div className="skill-card-list agent-skill-card-list">
        {visiblePlugins.length === 0 ? (
          <div className="skill-empty-state">
            {vulcanPlugins.length === 0 ? t("skills.plugins.emptyDetail") : t("skills.filter.empty")}
          </div>
        ) : visiblePlugins.map((plugin) => {
          // installTask is the newest Core-owned installation attempt for this plugin generation.
          // installTask 是当前插件代次最近一次由 Core 持有的安装尝试。
          const installTask = installTasksByPlugin[plugin.id];
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
                      ? t("skills.plugins.userPlugin")
                      : plugin.id === "vulcan-browser" ? t("skills.plugins.builtInComponent")
                      : t("skills.plugins.systemKind")}</Badge>
                    {plugin.version ? <Badge variant="secondary">v{plugin.version}</Badge> : null}
                  </div>
                </div>
                <p>{localizedCapability}</p>
                {plugin.extension !== undefined && plugin.extension !== null && <p className="break-all text-xs text-muted-foreground">
                  {t("pluginDiagnostics.runtimeIdentity")}: <code>{plugin.extension.runtimeIdentity}</code>
                </p>}
                <Button type="button" size="sm" variant="outline" onClick={() => setDiagnosticsSelection({ kind: "plugin", pluginId: plugin.id })}>
                  {t("pluginDiagnostics.title")}
                </Button>
                <div className="vulcan-plugin-facts">
                  <span className="agent-definition-row-path">{plugin.id}</span>
                  <PluginFact
                    icon={plugin.extension?.hasLuaRuntime ? Cpu : FileText}
                    label={plugin.extension?.hasLuaRuntime ? t("skills.plugins.runtimePlugin") : t("skills.plugins.contentPlugin")}
                  />
                  {plugin.toolCount > 0 ? <PluginFact icon={Wrench} label={t("skills.plugins.tools")} value={plugin.toolCount} /> : null}
                  {plugin.skillCount > 0 ? <PluginFact icon={Hammer} label={t("skills.plugins.skills")} value={plugin.skillCount} items={plugin.extension?.skills} /> : null}
                  {plugin.agentCount > 0 ? <PluginFact icon={Bot} label={t("skills.plugins.agents")} value={plugin.agentCount} items={plugin.extension?.agents} /> : null}
                  {plugin.hookCount > 0 ? <PluginFact icon={Webhook} label={t("skills.plugins.hooks")} value={plugin.hookCount} /> : null}
                  {plugin.activeSessionCount > 0 ? <PluginFact icon={MessagesSquare} label={t("pluginDiagnostics.activeOwners")} value={plugin.activeSessionCount} /> : null}
                </div>
                {plugin.diagnostics.length > 0 ? <ul className="vulcan-plugin-diagnostics" role="alert">
                  {plugin.diagnostics.map((diagnostic, index) => <li key={`${index}:${diagnostic}`}><TriangleAlert aria-hidden="true" size={13} />{diagnostic}</li>)}
                </ul> : null}
                {plugin.id === "vulcan-browser" ? <div className="vulcan-plugin-runtime-panel">
                  <div role="status">
                  {formatMessage(t("skills.plugins.playwrightStatus"), {
                    status: t(environmentReadyByPlugin[plugin.id] === true
                      ? "skills.plugins.playwrightReady"
                      : environmentReadyByPlugin[plugin.id] === false ? "skills.plugins.playwrightMissing" : "skills.plugins.playwrightChecking"),
                  })}
                  </div>
                  <CoreBrowserRuntimeControls />
                </div> : null}
                {(plugin.extension?.mcpServers.length ?? 0) > 0 ? (
                  <section className="vulcan-plugin-mcp" aria-label={t("skills.plugins.mcpServersAndCredentials")}>
                    <h4>{t("skills.plugins.mcpServersAndCredentials")}</h4>
                    {plugin.extension?.mcpServers.map((server) => (
                      <div className="vulcan-plugin-mcp-server" key={server.id}>
                        <div className="vulcan-plugin-mcp-server-head">
                          <Server aria-hidden="true" className="vulcan-plugin-mcp-server-icon" size={14} />
                          <strong>{server.name}</strong>
                          <StatusPill
                            label={pluginMcpStatusLabel(server.status, t)}
                            status={server.status === "ready" ? "ready" : server.status === "failed" || server.status === "degraded" ? "warning" : "pending"}
                          />
                          <Badge variant="outline">{server.transport}</Badge>
                          <div className="vulcan-plugin-facts">
                            <PluginFact icon={Wrench} label={t("skills.plugins.tools")} value={server.toolCount} />
                            <PluginFact icon={MessageSquareText} label={t("skills.plugins.mcpPrompts")} value={server.promptCount} />
                            <PluginFact icon={FolderOpen} label={t("skills.plugins.mcpResources")} value={server.resourceCount} />
                            <PluginFact
                              icon={Info}
                              label={t("skills.plugins.mcpConnectionDetails")}
                              tooltip={<dl className="vulcan-plugin-fact-list">
                                <div><dt>{t("skills.plugins.mcpEndpoint")}</dt><dd>{server.endpoint}</dd></div>
                                <div><dt>{t("skills.plugins.mcpGeneration")}</dt><dd>{server.generation}</dd></div>
                                <div><dt>{t("skills.plugins.mcpRetryCount")}</dt><dd>{server.retryCount}</dd></div>
                              </dl>}
                            />
                            {(server.recentLogs?.length ?? 0) > 0 ? <PluginFact
                              icon={ScrollText}
                              label={t("skills.plugins.recentSanitizedLogs")}
                              tooltip={<pre className="vulcan-plugin-log-tooltip">{server.recentLogs?.join("\n")}</pre>}
                            /> : null}
                          </div>
                        </div>
                        {server.lastError ? <p className="vulcan-plugin-mcp-error" role="alert"><TriangleAlert aria-hidden="true" size={13} />{server.lastError}</p> : null}
                        {server.secretKeys.map((secretKey) => {
                          const draftKey = `${plugin.id}:${server.id}:${secretKey}`;
                          return (
                            <div className="vulcan-plugin-mcp-secret" key={secretKey}>
                              <span className="vulcan-plugin-mcp-secret-key"><KeyRound aria-hidden="true" size={13} />{secretKey}</span>
                              <Input
                                aria-label={`${server.name} ${secretKey}`}
                                autoComplete="new-password"
                                density="compact"
                                disabled={mutationBusy}
                                onChange={(event) => setSecretDrafts((current) => ({
                                  ...current,
                                  [draftKey]: event.target.value,
                                }))}
                                placeholder={secretKey}
                                type="password"
                                value={secretDrafts[draftKey] ?? ""}
                              />
                              <Button
                                disabled={mutationBusy || !(secretDrafts[draftKey] ?? "")}
                                onClick={() => void storePluginMcpSecret(plugin.id, server.id, secretKey)}
                                size="sm"
                                type="button"
                                variant="secondary"
                              >
                                {t("skills.plugins.storeSecurely")}
                              </Button>
                              <IconButton
                                aria-label={t("skills.plugins.deleteSecret")}
                                disabled={mutationBusy}
                                onClick={() => { setSecretError(""); setPendingDestructive({ kind: "secret", pluginId: plugin.id, serverId: server.id, secretKey }); }}
                                size="sm"
                                title={t("skills.plugins.deleteSecret")}
                                type="button"
                                variant="ghost"
                              >
                                <Trash2 size={14} />
                              </IconButton>
                            </div>
                          );
                        })}
                      </div>
                    ))}
                  </section>
                ) : null}
              </div>
              <div className="agent-definition-row-actions">
              {!plugin.desiredEnabled && environmentReadyByPlugin[plugin.id] === false && plugin.runtimeState === "disabled" ? (
                <Button
                  disabled={!plugin.available || mutationBusy || installTask?.state === "queued" || installTask?.state === "running"}
                  onClick={() => void installPluginEnvironment(plugin)}
                  size="sm"
                  type="button"
                  variant="secondary"
                >
                  {installTask?.state === "queued"
                    ? t("skills.plugins.installQueued")
                    : installTask?.state === "running"
                      ? t("skills.plugins.installingRuntime")
                      : plugin.id === "vulcan-browser" ? t("skills.plugins.repairLocalEnvironment")
                      : t("skills.plugins.installRuntime")}
                </Button>
              ) : null}
              {plugin.extension?.origin === "userInstalled" ? (
                <IconButton
                  aria-label={t("skills.plugins.uninstallUserPlugin")}
                  disabled={mutationBusy || uninstallUncertainty !== null || plugin.runtimeState !== "disabled"}
                  onClick={() => { setInstallError(""); setPendingDestructive({ kind: "uninstall", plugin }); }}
                  size="sm"
                  title={plugin.runtimeState === "disabled"
                    ? t("skills.plugins.uninstall")
                    : t("skills.plugins.disableThePluginFirst")}
                  type="button"
                  variant="destructive"
                >
                  <Trash2 size={15} />
                </IconButton>
              ) : null}
              <Switch
                aria-busy={busyPluginId === plugin.id}
                checked={plugin.desiredEnabled}
                aria-label={toggleLabel}
                disabled={!plugin.available || mutationBusy || (!plugin.desiredEnabled && environmentReadyByPlugin[plugin.id] !== true) || plugin.runtimeState === "enabling" || plugin.runtimeState === "draining"}
                onCheckedChange={() => void togglePlugin(plugin)}
                title={toggleLabel}
              />
              </div>
              {installTask ? <div className="text-xs" role={installTask.state === "failed" ? "alert" : "status"}>
                {installTask.state === "failed"
                  ? formatMessage(t("skills.plugins.runtimeInstallFailed"), { error: installTask.error ?? t("skills.plugins.unknownError") })
                  : installTask.state === "succeeded"
                    ? t("skills.plugins.runtimeInstallationComplete")
                    : formatMessage(t("skills.plugins.installStage"), { stage: installTask.stage })}
              </div> : null}
            </article>
          );
        })}
      </div>
      </TooltipProvider>
      <AlertDialog open={pendingDestructive !== null} onOpenChange={(open) => { if (!open && !actionRunningRef.current) setPendingDestructive(null); }}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>{pendingDestructive?.kind === "uninstall" ? t("skills.plugins.uninstallPlugin") : t("skills.plugins.deleteCredential")}</AlertDialogTitle>
            <AlertDialogDescription>{pendingDestructive?.kind === "uninstall"
              ? (formatMessage(t("skills.plugins.uninstallingRemovesItsPackage"), { name: pendingDestructive.plugin.name }))
              : pendingDestructive ? (formatMessage(t("skills.plugins.deleteCredentialEnabledServers"), { secretKey: pendingDestructive.secretKey })) : ""}</AlertDialogDescription>
          </AlertDialogHeader>
          {(pendingDestructive?.kind === "uninstall" ? installError : secretError) ? <p role="alert" className="text-sm text-destructive">{pendingDestructive?.kind === "uninstall" ? installError : secretError}</p> : null}
          <AlertDialogFooter>
            <AlertDialogCancel disabled={mutationBusy}>{t("skills.plugins.cancel")}</AlertDialogCancel>
            <AlertDialogAction disabled={mutationBusy} onClick={(event) => {
              event.preventDefault();
              if (pendingDestructive?.kind === "uninstall") void uninstallPlugin(pendingDestructive.plugin);
              else if (pendingDestructive) void deletePluginMcpSecret(pendingDestructive.pluginId, pendingDestructive.serverId, pendingDestructive.secretKey);
            }}>{mutationBusy ? t("skills.plugins.working") : t("skills.plugins.confirm")}</AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </div>
  );
}
