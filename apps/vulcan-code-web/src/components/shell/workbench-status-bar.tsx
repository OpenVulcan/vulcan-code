import React from "react";
import { Brain, Puzzle, RadioTower, SquareTerminal } from "lucide-react";
import { formatMessage, useI18n, type TranslationKey } from "@/i18n";
import type { PluginSummary, StartupSnapshot, VmmMemorySnapshot } from "@/types";
import { RepositoryLink } from "./app-chrome";

// StatusTone is the tone a status item takes; undefined keeps the neutral bar color.
// StatusTone 是状态项的色调；undefined 保持状态栏的中性颜色。
type StatusTone = "ok" | "warn" | "bad" | "ember" | undefined;

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
}

// WorkbenchStatusBar renders the bottom status strip: core connection, the session hosts and the project
// terminal on the left; plugins, memory, the repository link and the version on the right. Every item opens
// the place that owns its state; display preferences live only in the title-bar app menu.
// WorkbenchStatusBar 渲染底部状态条：左侧为内核连接、会话宿主节点与项目终端；右侧为插件、记忆、仓库链接与版本。
// 每一项都打开持有其状态的位置；显示偏好只在标题栏应用菜单中提供。
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
  const { t } = useI18n();

  const preview = props.startup.phase === "browser-preview";
  const kernelTone: StatusTone = preview ? "warn" : props.startup.ready ? "ok" : "ember";
  const kernelLabel = preview ? t("global.preview") : props.startup.ready ? t("status.kernel.ready") : t("status.kernel.starting");

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
  const memoryLabel = formatMessage(t("status.memory"), {
    state: t(memoryDisabled ? "vmm.experience.deployment.disabled" : `vmm.experience.health.${memoryState.health}` as TranslationKey),
  });

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
      <div className="vk-status-host" ref={props.hosts.settings} />
      <div className="vk-status-host" ref={props.hosts.todolist} />
      {terminal !== null ? (
        <button
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
      {props.plugins.length > 0 ? (
        <button className="vk-status-item" data-optional="" data-tone={pluginTone} onClick={props.onOpenPlugins} title={t("status.plugins.open")} type="button">
          <Puzzle />
          <span>{formatMessage(t("status.plugins"), { loaded: loadedPlugins.length, total: wantedPlugins.length })}</span>
        </button>
      ) : null}
      <button className="vk-status-item" data-optional="" data-tone={memoryTone} onClick={props.onOpenMemory} title={t("status.memory.open")} type="button">
        <Brain />
        <span>{memoryLabel}</span>
      </button>
      <RepositoryLink onOpenExternalUrl={props.onOpenExternalUrl} />
      <span className="vk-status-item" data-optional="" data-static="">v{props.appVersion}</span>
    </footer>
  );
}
