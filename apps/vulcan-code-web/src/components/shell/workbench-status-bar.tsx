import React from "react";
import { Brain, Puzzle, RadioTower, SquareTerminal } from "lucide-react";
import { formatMessage, useI18n, type TranslationKey } from "@/i18n";
import type { PluginSummary, StartupSnapshot, VmmMemorySnapshot } from "@/types";
import { RepositoryLink } from "./app-chrome";
import { StatusPeek, StatusPeekFacts, type StatusPeekTone } from "./status-peek";
import { useServiceConnectionStatus } from "./use-service-connection-status";

// StatusTone is the tone a status item takes; undefined keeps the neutral bar color.
// StatusTone 是状态项的色调；undefined 保持状态栏的中性颜色。
type StatusTone = StatusPeekTone;

// PluginLoadState is how one installed plugin stands right now.
// PluginLoadState 是单个已安装插件当前的状态。
type PluginLoadState = "loaded" | "loading" | "off" | "unavailable";

// pluginLoadState reads one plugin's standing from the user's choice and the runtime's availability.
// pluginLoadState 根据用户选择与运行时可用性读取单个插件的状态。
function pluginLoadState(plugin: PluginSummary): PluginLoadState {
  if (!plugin.desiredEnabled) return "off";
  if (!plugin.available) return "unavailable";
  return plugin.enabled ? "loaded" : "loading";
}

const PLUGIN_STATE_TONE: Record<PluginLoadState, StatusTone> = { loaded: "ok", loading: "ember", off: undefined, unavailable: "bad" };
// PLUGIN_STATE_ORDER lists plugins needing attention first.
// PLUGIN_STATE_ORDER 让需要关注的插件排在前面。
const PLUGIN_STATE_ORDER: Record<PluginLoadState, number> = { unavailable: 0, loading: 1, loaded: 2, off: 3 };

// camelSetupState maps a VMM setup state to its catalog key segment.
// camelSetupState 将 VMM 配置状态映射为其目录键片段。
const camelSetupState = (state: string) => state.replace(/_([a-z])/g, (_, letter: string) => letter.toUpperCase());

// WorkbenchStatusBarHosts are the nodes other surfaces portal their own status into.
// WorkbenchStatusBarHosts 是其他界面以传送门方式放入自身状态的节点。
export interface WorkbenchStatusBarHosts {
  // Receives the composer's session settings menu.
  // 接收输入区的会话设置菜单。
  settings: (node: HTMLDivElement | null) => void;
  // Receives the session todolist progress capsule.
  // 接收会话待办进度胶囊。
  todolist: (node: HTMLDivElement | null) => void;
  // Receives the composer runtime cue.
  // 接收输入区运行提示。
  runtime: (node: HTMLDivElement | null) => void;
  // Receives the composer's model account item.
  // 接收输入区的模型账号项。
  account: (node: HTMLDivElement | null) => void;
}

// WorkbenchStatusBar renders the bottom status strip: core connection, the session hosts and the project
// terminal on the left; the model account, plugins, memory, the repository link and the version on the right.
// Plugins and memory are read in place: their cards show the common states and carry the way to settings.
// Display preferences live only in the title-bar app menu.
// WorkbenchStatusBar 渲染底部状态条：左侧为内核连接、会话宿主节点与项目终端；右侧为模型账号、插件、记忆、仓库链接与版本。
// 插件与记忆就地查看：其卡片显示常见状态并提供前往设置的入口。显示偏好只在标题栏应用菜单中提供。
export function WorkbenchStatusBar(props: {
  // Application version shown at the right end.
  // 显示在最右端的应用版本。
  appVersion: string;
  // Portal hosts for session status owned by other surfaces.
  // 由其他界面持有的会话状态传送门节点。
  hosts: WorkbenchStatusBarHosts;
  // Plugin rows used for the loaded count.
  // 用于计算已载入数量的插件行。
  plugins: PluginSummary[];
  // Startup snapshot used for the core connection item.
  // 内核连接项使用的启动快照。
  startup: StartupSnapshot;
  // The active project terminal, or null when no project is active.
  // 当前项目终端；没有活动项目时为 null。
  terminal: {
    // Whether the terminal window is expanded.
    // 终端窗口是否已展开。
    expanded: boolean;
    // Name of the project that owns the terminal window.
    // 持有终端窗口的项目名称。
    projectName: string;
    // Running terminals in that project.
    // 该项目中运行的终端数量。
    runningCount: number;
  } | null;
  // Receives the terminal item, which the terminal window minimizes into.
  // 接收终端项；终端窗口最小化时收入此处。
  terminalAnchorRef: React.Ref<HTMLButtonElement>;
  // Memory snapshot used for the memory item.
  // 记忆项使用的记忆快照。
  vmmMemory: VmmMemorySnapshot;
  // Opens the runtime diagnostics settings page.
  // 打开运行诊断设置页。
  onOpenDiagnostics: () => void;
  // Opens memory management.
  // 打开记忆管理。
  onOpenMemory: () => void;
  // Opens an external URL through the host shell.
  // 通过宿主壳打开外部地址。
  onOpenExternalUrl: (url: string) => Promise<void>;
  // Opens plugin management.
  // 打开插件管理。
  onOpenPlugins: () => void;
  // Opens or hides the active project's terminal window.
  // 打开或隐藏当前项目的终端窗口。
  onToggleTerminal: () => void;
}) {
  const { t, locale } = useI18n();

  const preview = props.startup.phase === "browser-preview";
  // A ready kernel whose connection dropped must not keep reading as connected while the app reconnects.
  // 已就绪内核的连接中断后，在应用重连期间不能继续显示为已连接。
  const connection = useServiceConnectionStatus();
  const connectionLost = !preview && (connection === "disconnected" || connection === "reconnecting");
  const kernelTone: StatusTone = preview ? "warn" : connectionLost ? "bad" : props.startup.ready ? "ok" : "ember";
  const kernelLabel = preview
    ? t("global.preview")
    : connectionLost ? t("status.kernel.reconnecting") : props.startup.ready ? t("status.kernel.ready") : t("status.kernel.starting");

  // Plugins count as loaded when the user enabled them and the runtime has them available.
  // 插件在用户已启用且运行时可用时计为已载入。
  const wantedPlugins = props.plugins.filter((plugin) => plugin.desiredEnabled);
  const loadedPlugins = wantedPlugins.filter((plugin) => plugin.enabled && plugin.available);
  const pluginTone: StatusTone = loadedPlugins.length < wantedPlugins.length ? "warn" : undefined;

  const memoryState = props.vmmMemory.experienceState;
  const memoryDisabled = memoryState.deployment === "disabled";
  const memoryTone: StatusTone = memoryDisabled ? undefined
    : memoryState.health === "unavailable" ? "bad"
      : memoryState.health === "degraded" ? "warn" : undefined;
  const memoryStateLabel = t(memoryDisabled ? "vmm.experience.deployment.disabled" : `vmm.experience.health.${memoryState.health}` as TranslationKey);
  const memoryLabel = formatMessage(t("status.memory"), { state: memoryStateLabel });
  const activity = props.vmmMemory.recentActivity;
  const memoryFacts = [
    { label: t("vmm.experience.deployment"), value: t(`vmm.experience.deployment.${memoryState.deployment}` as TranslationKey) },
    ...(memoryDisabled ? [] : [
      { label: t("vmm.experience.setup"), value: t(`vmm.experience.setup.${camelSetupState(memoryState.setup)}` as TranslationKey), tone: memoryState.setup === "ready" ? "ok" as const : memoryState.setup === "blocked" ? "bad" as const : undefined },
      { label: t("vmm.experience.lifecycle"), value: t(`vmm.experience.lifecycle.${memoryState.lifecycle}` as TranslationKey), tone: memoryState.lifecycle === "failed" ? "bad" as const : undefined },
      { label: t("vmm.experience.health"), value: memoryStateLabel, tone: memoryTone ?? (memoryState.health === "healthy" ? "ok" as const : undefined) },
      { label: t("vmm.experience.pendingWritebacks"), value: activity.pendingWritebacks.toLocaleString(locale) },
      { label: t("vmm.experience.completedTurns"), value: activity.completedTurns.toLocaleString(locale) },
    ]),
  ];
  const pluginRows = [...props.plugins]
    .map((plugin) => ({ plugin, state: pluginLoadState(plugin) }))
    .sort((left, right) => PLUGIN_STATE_ORDER[left.state] - PLUGIN_STATE_ORDER[right.state]);

  const terminal = props.terminal;
  const terminalLabel = terminal === null ? "" : terminal.runningCount > 0
    ? formatMessage(t("workspace.terminalToggle.withRunning"), { name: terminal.projectName, count: terminal.runningCount })
    : formatMessage(t("workspace.terminalToggle.named"), { name: terminal.projectName });

  return (
    <footer aria-label={t("status.label")} className="vk-status">
      <button className="vk-status-item" data-tone={kernelTone} onClick={props.onOpenDiagnostics} title={t("status.kernel.open")} type="button">
        <RadioTower />
        <span>{kernelLabel}</span>
      </button>
      <div className="vk-status-host" data-fixed="" ref={props.hosts.settings} />
      <div className="vk-status-host" data-fixed="" ref={props.hosts.todolist} />
      {terminal !== null ? (
        <button
          aria-expanded={terminal.expanded}
          aria-haspopup="dialog"
          aria-label={terminalLabel}
          className="vk-status-item"
          data-tone={terminal.runningCount > 0 ? "ember" : undefined}
          onClick={props.onToggleTerminal}
          ref={props.terminalAnchorRef}
          title={terminalLabel}
          type="button"
        >
          <SquareTerminal />
          <span>{t("status.terminal")}</span>
          {terminal.runningCount > 0 ? <span aria-hidden className="vk-count">{terminal.runningCount}</span> : null}
        </button>
      ) : null}
      <div className="vk-status-host" data-grow="" ref={props.hosts.runtime} />
      <div className="vk-status-host" ref={props.hosts.account} />
      {props.plugins.length > 0 ? (
        <StatusPeek
          card={(
            <ul className="vk-peek-list">
              {pluginRows.map(({ plugin, state }) => (
                <li key={plugin.id}>
                  <span className="vk-dot" data-tone={PLUGIN_STATE_TONE[state]} />
                  <span>{plugin.nameByLocale[locale] ?? plugin.name}</span>
                  <small data-tone={PLUGIN_STATE_TONE[state]}>{t(`status.plugin.${state}` as TranslationKey)}</small>
                </li>
              ))}
            </ul>
          )}
          label={t("status.plugins.title")}
          onOpenSettings={props.onOpenPlugins}
          optional
          settingsLabel={t("status.plugins.open")}
          summary={formatMessage(t("status.plugins"), { loaded: loadedPlugins.length, total: wantedPlugins.length })}
          title={t("status.plugins.title")}
          tone={pluginTone}
        >
          <Puzzle />
          <span>{formatMessage(t("status.plugins"), { loaded: loadedPlugins.length, total: wantedPlugins.length })}</span>
        </StatusPeek>
      ) : null}
      <StatusPeek
        card={<StatusPeekFacts facts={memoryFacts} />}
        label={memoryLabel}
        onOpenSettings={props.onOpenMemory}
        optional
        settingsLabel={t("status.memory.open")}
        summary={memoryStateLabel}
        title={t("vmm.experience.title")}
        tone={memoryTone}
      >
        <Brain />
        <span>{memoryLabel}</span>
      </StatusPeek>
      <RepositoryLink onOpenExternalUrl={props.onOpenExternalUrl} />
      <span className="vk-status-item" data-optional="" data-static="">v{props.appVersion}</span>
    </footer>
  );
}
