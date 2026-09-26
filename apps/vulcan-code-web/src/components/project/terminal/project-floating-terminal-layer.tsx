import React from "react";
import {
  Check,
  Maximize2,
  Minus,
  Minimize2,
  PanelLeftClose,
  PanelLeftOpen,
  Pencil,
  Plus,
  Square,
  TerminalSquare,
  X,
} from "lucide-react";

import { TerminalViewport } from "@/components/terminal/terminal-viewport";
import { Button } from "@/components/ui/button";
import { StreamPlaceholder } from "@/components/ui/stream-placeholder";
import { formatBytes } from "@/lib/display-format";
import { useI18n, type TranslationKey, formatMessage, translateActive } from "@/i18n";
import { acquireVisibleTerminalDisplayListener } from "@/components/terminal/terminal-display-listener";
import { useTerminalDisplayConnectionState } from "@/components/terminal/use-terminal-display-connection-epoch";
import {
  clearProjectTerminalInputBuffer,
  getOrCreateProjectTerminalInputBuffer,
  type QueuedProjectTerminalInput,
} from "@/components/project/terminal/project-terminal-input-buffer";
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import {
  ProjectFloatingLayer,
  type ProjectFloatingLayerControls,
} from "@/components/workspace/project-floating-layer";
import {
  releaseProjectTerminalInputOwner,
  setWorkspaceInputOwner,
} from "@/components/workspace/workspace-input-owner";
import { serviceClient } from "@/service/webui-service-runtime";
import {
  attachProjectTerminalDisplay,
  closeProjectTerminal,
  createProjectTerminal,
  detachProjectTerminalDisplay,
  listProjectTerminals,
  resizeProjectTerminal,
  renameProjectTerminal,
  sendProjectTerminalInputBatch,
  stopProjectTerminal,
  type ProjectTerminalInputEventRequest,
} from "@/service/terminal-service";
import type {
  ProjectSummary,
  ProjectTerminalCatalog,
  ProjectTerminalDisplayChangedEvent,
  ProjectTerminalSummary,
  TerminalDisplayChunk,
} from "@/types";

// PROJECT_TERMINAL_INPUT_BATCH_MAX_BYTES stays below the service hard limit for predictable batching.
// PROJECT_TERMINAL_INPUT_BATCH_MAX_BYTES 低于服务硬限制，以实现可预测的输入批处理。
const PROJECT_TERMINAL_INPUT_BATCH_MAX_BYTES = 60 * 1024;

// PROJECT_TERMINAL_INPUT_BATCH_MAX_EVENTS mirrors the service metadata bound.
// PROJECT_TERMINAL_INPUT_BATCH_MAX_EVENTS 对齐服务端元数据上限。
const PROJECT_TERMINAL_INPUT_BATCH_MAX_EVENTS = 256;

// PROJECT_TERMINAL_PENDING_INPUT_MAX_BYTES bounds retained unacknowledged browser input.
// PROJECT_TERMINAL_PENDING_INPUT_MAX_BYTES 限制浏览器保留的未确认输入大小。
const PROJECT_TERMINAL_PENDING_INPUT_MAX_BYTES = 4 * 1024 * 1024;

// PROJECT_TERMINAL_INPUT_FRAGMENT_MAX_BYTES bounds one queued xterm event fragment.
// PROJECT_TERMINAL_INPUT_FRAGMENT_MAX_BYTES 限制单个排队 xterm 输入事件分片大小。
const PROJECT_TERMINAL_INPUT_FRAGMENT_MAX_BYTES = 32 * 1024;

// ProjectTerminalResizeSettlement orders terminal input after every grid change already observed by xterm.
// ProjectTerminalResizeSettlement 将终端输入排在 xterm 已观察到的全部网格变化之后。
interface ProjectTerminalResizeSettlement {
  // Promise resolved after the latest queued PTY resize request has completed.
  // 最新排队 PTY 尺寸请求完成后解析的 Promise。
  promise: Promise<void>;
  // Completes the current resize series exactly once.
  // 仅一次完成当前尺寸变化序列。
  resolve: () => void;
}

// ProjectTerminalUiState preserves floating terminal UI state independently for each project.
// ProjectTerminalUiState 为每个项目独立保留浮动终端界面状态。
interface ProjectTerminalUiState {
  // Whether this project's terminal surface is expanded.
  // 当前项目的终端表面是否展开。
  expanded: boolean;
  // Whether the project terminal list is reduced to compact numeric selectors.
  // 项目终端列表是否缩减为紧凑编号选择器。
  sidebarCollapsed: boolean;
  // Last metadata catalog loaded for this project.
  // 当前项目最后加载的元数据目录。
  catalog: ProjectTerminalCatalog | null;
  // Terminal selected for visible rendering in this project.
  // 当前项目中为可见渲染选中的终端。
  selectedTerminalId: string | null;
  // Monotonic request that returns keyboard focus to the selected terminal.
  // 将键盘焦点返回选中终端的单调请求序号。
  terminalFocusRequestNonce: number;
  // Shell selected for the next terminal in this project.
  // 当前项目下一个终端所选的 Shell。
  selectedShellId: string | null;
  // In-flight lifecycle action scoped to this project.
  // 归属于当前项目的进行中生命周期动作。
  busyAction: string | null;
  // Latest explicit failure scoped to this project.
  // 归属于当前项目的最近一次明确故障。
  error: string | null;
  // Terminal currently being renamed inline.
  // 当前正在行内重命名的终端。
  renamingTerminalId: string | null;
  // Editable terminal name draft.
  // 可编辑终端名称草稿。
  renameDraft: string;
}

// projectTerminalUiStateCache preserves project UI ownership across conversation-driven React remounts.
// projectTerminalUiStateCache 在会话切换引发的 React 重挂载之间保留项目界面所有权。
const projectTerminalUiStateCache = new Map<string, ProjectTerminalUiState>();

// projectTerminalSeenRequests prevents one old toolbar request from replaying after a remount.
// projectTerminalSeenRequests 防止旧工具栏请求在重挂载后再次播放。
const projectTerminalSeenRequests = new Map<string, number>();

// projectTerminalResizeRequestTails serializes PTY grid changes across React pane remounts.
// projectTerminalResizeRequestTails 在 React 面板重挂载之间串行化 PTY 网格变化。
const projectTerminalResizeRequestTails = new Map<string, Promise<void>>();

// projectTerminalResizeRequestKey returns one collision-safe project and terminal resize ownership key.
// projectTerminalResizeRequestKey 返回一个无碰撞的项目与终端尺寸所有权键。
function projectTerminalResizeRequestKey(projectId: string, terminalId: string): string {
  return JSON.stringify([projectId, terminalId]);
}

// resizeProjectTerminalInOrder preserves resize request order even when the owning visible pane remounts.
// resizeProjectTerminalInOrder 即使可见终端面板重挂载，也会保留尺寸请求顺序。
async function resizeProjectTerminalInOrder(
  projectId: string,
  terminalId: string,
  rows: number,
  cols: number,
): Promise<void> {
  // requestKey selects the exact cross-remount serialization chain.
  // requestKey 选择精确的跨重挂载串行链。
  const requestKey = projectTerminalResizeRequestKey(projectId, terminalId);
  // previousRequest is allowed to fail without preventing the newest authoritative grid attempt.
  // previousRequest 可以失败，但不会阻止最新权威网格继续尝试。
  const previousRequest = projectTerminalResizeRequestTails.get(requestKey) ?? Promise.resolve();
  // currentRequest starts only after every earlier request for the same terminal has settled.
  // currentRequest 仅在同一终端的全部早期请求结束后启动。
  const currentRequest = previousRequest
    .catch(() => undefined)
    .then(() => resizeProjectTerminal(projectId, terminalId, rows, cols));
  projectTerminalResizeRequestTails.set(requestKey, currentRequest);
  try {
    await currentRequest;
  } finally {
    if (projectTerminalResizeRequestTails.get(requestKey) === currentRequest) {
      projectTerminalResizeRequestTails.delete(requestKey);
    }
  }
}

// ProjectFloatingTerminalLayerProps binds the floating terminal container to one explicit active project.
// ProjectFloatingTerminalLayerProps 将浮动终端容器绑定到一个明确的活动项目。
// ProjectTerminalVisibilityRequest identifies one explicit open or toggle action for a project terminal.
// ProjectTerminalVisibilityRequest 标识项目终端的一次明确展开或切换动作。
export type ProjectTerminalVisibilityRequest = {
  // Exact project targeted by the visibility action.
  // 可见性动作的精确目标项目。
  projectId: string;
  // Explicit visibility command issued by the workbench.
  // 工作台发出的明确可见性命令。
  action: "open" | "toggle";
  // Monotonic request revision used to process each command exactly once.
  // 用于确保每个命令只处理一次的单调请求修订号。
  nonce: number;
};

export interface ProjectFloatingTerminalLayerProps {
  // Explicit project selected by the workbench project state.
  // 工作台项目状态明确选中的项目。
  project: ProjectSummary | null;
  // Authoritative workbench projects used to prune removed project UI state.
  // 用于裁剪已移除项目界面状态的权威工作台项目集合。
  projects: readonly ProjectSummary[];
  // Explicit request that changes visibility only for its exact target project's terminal container.
  // 仅改变其精确目标项目终端容器可见性的明确请求。
  openRequest: ProjectTerminalVisibilityRequest | null;
  // Live top-bar button used as the macOS-style minimize and restore animation anchor.
  // 用作 macOS 风格最小化与恢复动画锚点的实时顶部栏按钮。
  minimizeAnchorRef: React.RefObject<HTMLButtonElement | null>;
}

// ProjectFloatingTerminalLayer renders project-owned terminal controls independently of conversation selection.
// ProjectFloatingTerminalLayer 独立于会话选择渲染项目持有的终端控制功能。
// Parameters: props supplies project identity and floating-window controls; returns project-owned terminal management without transferring conversation ownership.
// 参数：props 提供项目身份与浮动窗口控制；返回不转移会话归属的项目终端管理界面。
export function ProjectFloatingTerminalLayer(props: ProjectFloatingTerminalLayerProps) {
  const { t } = useI18n();
  // projectId is the explicit workbench-owned project identity and never derives from conversation state.
  // projectId 是工作台明确持有的项目标识，绝不从对话状态推导。
  const projectId = props.project?.id ?? null;
  // visibleProjectId prevents delayed creation from claiming another project's keyboard focus.
  // visibleProjectId 防止延迟完成的创建操作抢占另一项目的键盘焦点。
  const visibleProjectId = React.useRef(projectId);
  React.useLayoutEffect(() => {
    visibleProjectId.current = projectId;
    return () => { visibleProjectId.current = null; };
  }, [projectId]);
  // catalogRevision invalidates reads started before an explicit lifecycle mutation.
  // catalogRevision 使明确生命周期变更之前发起的目录读取失效。
  const catalogRevision = React.useRef(new Map<string, number>());
  // catalogReload provides an explicit retry without creating or stopping any terminal.
  // catalogReload 提供不创建或停止任何终端的明确重试入口。
  const [catalogReload, setCatalogReload] = React.useState(0);
  // catalogLoading separates failed initial reads from active loading.
  // catalogLoading 将首次读取失败与实际加载中区分开。
  const [catalogLoading, setCatalogLoading] = React.useState(false);
  // projectStates retains independent UI state while users switch between projects.
  // projectStates 在用户切换项目时保留相互独立的界面状态。
  const projectStates = React.useRef(projectTerminalUiStateCache);
  // seenRequests prevents an old explicit visibility request from replaying after later project switches.
  // seenRequests 防止旧的明确可见性请求在后续项目切换后重新播放。
  const seenRequests = React.useRef(projectTerminalSeenRequests);
  // stateRevision rerenders after the active map entry changes.
  // stateRevision 在活动映射条目变化后触发重新渲染。
  const [, setStateRevision] = React.useState(0);
  // terminalConfirmation owns one destructive lifecycle decision for the currently active project.
  // terminalConfirmation 持有当前活动项目的一次破坏性生命周期决定。
  const [terminalConfirmation, setTerminalConfirmation] = React.useState<{
    projectId: string;
    terminalId: string;
    action: "stop" | "close";
  } | null>(null);

  // Prune project-scoped UI caches after authoritative project removal.
  // 权威项目移除后裁剪项目级界面缓存。
  React.useEffect(() => {
    pruneRemovedProjectTerminalUiState(projectStates.current, seenRequests.current, props.projects);
  }, [props.projects]);
  // activeState is the exact project entry rendered during this pass.
  // activeState 是本次渲染使用的精确项目条目。
  const activeState = projectId
    ? getOrCreateProjectTerminalUiState(projectStates.current, projectId)
    : createProjectTerminalUiState();
  const {
    expanded,
    sidebarCollapsed,
    catalog,
    selectedTerminalId,
    terminalFocusRequestNonce,
    selectedShellId,
    busyAction,
    error,
    renamingTerminalId,
    renameDraft,
  } = activeState;

  // updateActiveState applies one immutable update to the currently captured project entry.
  // updateActiveState 对当前捕获的项目条目应用一次不可变更新。
  const updateActiveState = React.useCallback((
    updater: (current: ProjectTerminalUiState) => ProjectTerminalUiState,
  ) => {
    if (!projectId) return;
    const current = projectStates.current.get(projectId);
    if (!current) return;
    projectStates.current.set(projectId, updater(current));
    setStateRevision((revision) => revision + 1);
  }, [projectId]);
  // setExpanded controls display attachment visibility only for the captured project.
  // setExpanded 仅控制所捕获项目的显示订阅可见性。
  const setExpanded = React.useCallback((action: React.SetStateAction<boolean>) => {
    updateActiveState((current) => ({ ...current, expanded: resolveStateAction(action, current.expanded) }));
  }, [updateActiveState]);
  // minimizeProjectTerminal releases keyboard ownership before hiding the terminal surface.
  // minimizeProjectTerminal 在隐藏终端表面前释放键盘所有权。
  const minimizeProjectTerminal = React.useCallback(() => {
    if (selectedTerminalId) releaseProjectTerminalInputOwner(selectedTerminalId);
    setExpanded(false);
  }, [selectedTerminalId, setExpanded]);
  // setSidebarCollapsed preserves the project-owned navigation density independently from window visibility.
  // setSidebarCollapsed 独立于窗口可见性保留项目持有的导航密度。
  const setSidebarCollapsed = React.useCallback((action: React.SetStateAction<boolean>) => {
    updateActiveState((current) => ({
      ...current,
      sidebarCollapsed: resolveStateAction(action, current.sidebarCollapsed),
    }));
  }, [updateActiveState]);
  // setCatalog updates project metadata without storing terminal screen output.
  // setCatalog 更新项目元数据，但不保存终端屏幕输出。
  const setCatalog = React.useCallback((action: React.SetStateAction<ProjectTerminalCatalog | null>) => {
    updateActiveState((current) => ({ ...current, catalog: resolveStateAction(action, current.catalog) }));
  }, [updateActiveState]);
  // setSelectedTerminalId changes only the selected terminal of the captured project.
  // setSelectedTerminalId 只改变所捕获项目的选中终端。
  const setSelectedTerminalId = React.useCallback((action: React.SetStateAction<string | null>) => {
    updateActiveState((current) => ({
      ...current,
      selectedTerminalId: resolveStateAction(action, current.selectedTerminalId),
    }));
  }, [updateActiveState]);
  // setSelectedShellId changes only the next-shell choice of the captured project.
  // setSelectedShellId 只改变所捕获项目的下一个 Shell 选择。
  const setSelectedShellId = React.useCallback((action: React.SetStateAction<string | null>) => {
    updateActiveState((current) => ({
      ...current,
      selectedShellId: resolveStateAction(action, current.selectedShellId),
    }));
  }, [updateActiveState]);
  // setBusyAction serializes lifecycle mutations for the captured project.
  // setBusyAction 串行化所捕获项目的生命周期变更。
  const setBusyAction = React.useCallback((action: React.SetStateAction<string | null>) => {
    if (projectId && action === null) {
      catalogRevision.current.set(projectId, (catalogRevision.current.get(projectId) ?? 0) + 1);
    }
    updateActiveState((current) => ({ ...current, busyAction: resolveStateAction(action, current.busyAction) }));
  }, [projectId, updateActiveState]);
  // beginLifecycleAction synchronously locks the captured project before React can rerender.
  // beginLifecycleAction 在 React 重新渲染前同步锁定所捕获的项目。
  const beginLifecycleAction = React.useCallback((action: string): boolean => {
    if (!projectId || !projectStates.current.has(projectId) || projectStates.current.get(projectId)?.busyAction) return false;
    catalogRevision.current.set(projectId, (catalogRevision.current.get(projectId) ?? 0) + 1);
    setBusyAction(action);
    return true;
  }, [projectId, setBusyAction]);
  // setError publishes an explicit failure only to the captured project UI.
  // setError 仅向所捕获项目的界面发布明确故障。
  const setError = React.useCallback((action: React.SetStateAction<string | null>) => {
    updateActiveState((current) => ({ ...current, error: resolveStateAction(action, current.error) }));
  }, [updateActiveState]);
  React.useEffect(() => {
    if (!projectId || props.openRequest?.projectId !== projectId) return;
    const seenNonce = seenRequests.current.get(projectId) ?? 0;
    if (props.openRequest.nonce <= seenNonce) return;
    seenRequests.current.set(projectId, props.openRequest.nonce);
    if (props.openRequest.action === "toggle" && expanded) minimizeProjectTerminal();
    else setExpanded(true);
  }, [expanded, minimizeProjectTerminal, projectId, props.openRequest, setExpanded]);
  React.useEffect(() => {
    setTerminalConfirmation(null);
  }, [projectId]);
  // beginRename captures the selected terminal's exact current name for inline editing.
  // beginRename 捕获选中终端的精确当前名称以进行行内编辑。
  const beginRename = React.useCallback((terminal: ProjectTerminalSummary) => {
    updateActiveState((current) => ({
      ...current,
      renamingTerminalId: terminal.id,
      renameDraft: terminal.name,
    }));
  }, [updateActiveState]);
  // setRenameDraft updates the project-scoped inline name field.
  // setRenameDraft 更新项目级行内名称字段。
  const setRenameDraft = React.useCallback((value: string) => {
    updateActiveState((current) => ({ ...current, renameDraft: value }));
  }, [updateActiveState]);

  React.useEffect(() => {
    if (!expanded || !projectId) return;
    const attachedProjectId = projectId;
    const readRevision = catalogRevision.current.get(attachedProjectId) ?? 0;
    let cancelled = false;
    setCatalogLoading(true);
    setError(null);
    // Parameters: none; reads the mounted project captured by this effect. Returns after the read settles and publishes only if its project/version still owns the view.
    // 参数：无，读取此 effect 捕获的已挂载项目。在读取结束后返回，仅当项目及版本仍拥有视图时发布结果。
    const loadCatalog = async () => {
      try {
        const nextCatalog = await listProjectTerminals(attachedProjectId);
        if (!cancelled && (catalogRevision.current.get(attachedProjectId) ?? 0) === readRevision && nextCatalog) {
          applyCatalog(nextCatalog, setCatalog, setSelectedTerminalId, setSelectedShellId);
        }
      } catch (loadError) {
        if (!cancelled && (catalogRevision.current.get(attachedProjectId) ?? 0) === readRevision) {
          setError(terminalErrorMessage(loadError));
        }
      } finally {
        if (!cancelled) setCatalogLoading(false);
      }
    };
    void loadCatalog();
    return () => {
      cancelled = true;
    };
  }, [catalogReload, expanded, projectId, setCatalog, setError, setSelectedShellId, setSelectedTerminalId]);

  if (!props.project) return null;
  const project = props.project;
  const selectedTerminal = catalog?.terminals.find((terminal) => terminal.id === selectedTerminalId) ?? null;

  // handleSelectTerminal atomically selects one terminal, assigns input ownership, and requests focus.
  // handleSelectTerminal 以原子方式选中一个终端、分配输入所有权并请求焦点。
  //
  // Parameters / 参数:
  // - `terminalId`: Exact project-terminal UUID selected by the user. / 用户选择的精确项目终端 UUID。
  //
  // Returns / 返回值:
  // Returns nothing after updating this project's UI state.
  // 更新当前项目的界面状态后无返回值。
  const handleSelectTerminal = (terminalId: string): void => {
    if (visibleProjectId.current === project.id && projectStates.current.get(project.id)?.expanded) {
      setWorkspaceInputOwner({ kind: "project_terminal", terminalId });
    }
    updateActiveState((current) => ({
      ...current,
      selectedTerminalId: terminalId,
      terminalFocusRequestNonce: current.terminalFocusRequestNonce + 1,
      renamingTerminalId: null,
      renameDraft: "",
    }));
  };
  // handleTerminalTabKeyDown applies vertical-tab keyboard navigation to enabled terminal choices.
  // handleTerminalTabKeyDown 为可用终端选项提供垂直标签键盘导航。
  // Parameters: event is a terminal-tab key event; moves focus and selection among enabled tabs and returns no value.
  // 参数：event 是终端页签键盘事件；在可用页签间移动焦点和选择，不返回值。
  const handleTerminalTabKeyDown = (event: React.KeyboardEvent<HTMLDivElement>): void => {
    if (!["ArrowUp", "ArrowDown", "Home", "End"].includes(event.key)) return;
    const tabs = Array.from(event.currentTarget.querySelectorAll<HTMLButtonElement>('button[role="tab"]:not(:disabled)'));
    const index = tabs.indexOf(event.target as HTMLButtonElement);
    if (index < 0) return;
    event.preventDefault();
    event.stopPropagation();
    const nextIndex = event.key === "Home" ? 0 : event.key === "End" ? tabs.length - 1
      : (index + (event.key === "ArrowDown" ? 1 : -1) + tabs.length) % tabs.length;
    tabs[nextIndex].focus();
    tabs[nextIndex].click();
  };

  // handleCreateTerminal creates one isolated host using the explicitly selected shell.
  // handleCreateTerminal 使用明确选择的 Shell 创建一个隔离宿主。
  // Parameters: none; uses the exact selected shell and current project. Returns after guarded host creation, granting focus only to the still-visible owner.
  // 参数：无，使用精确选中 Shell 与当前项目。在受保护宿主创建后返回，仅向仍可见归属授予焦点。
  const handleCreateTerminal = async () => {
    if (!catalog || !selectedShellId || !catalog.shells.some((shell) => shell.id === selectedShellId)
      || !beginLifecycleAction("create")) return;
    setError(null);
    try {
      const nextCatalog = await createProjectTerminal({
        projectId: project.id,
        shellId: selectedShellId,
        rows: 24,
        cols: 80,
      });
      if (nextCatalog) {
        const newestTerminal = nextCatalog.terminals.at(-1) ?? null;
        setCatalog(nextCatalog);
        if (newestTerminal) handleSelectTerminal(newestTerminal.id);
        else setSelectedTerminalId(null);
      }
    } catch (createError) {
      setError(terminalErrorMessage(createError));
    } finally {
      setBusyAction(null);
    }
  };

  // handleStopTerminal stops the selected child while retaining its output tab.
  // handleStopTerminal 停止选中的子进程，同时保留其输出标签页。
  // Parameters: terminal is the exact selected summary; returns a promise after stopping that child, retaining its tab and exposing failure.
  // 参数：terminal 是精确选中摘要；返回停止该子进程后的 Promise，保留标签并暴露失败。
  const handleStopTerminal = async (terminal: ProjectTerminalSummary) => {
    if (terminal.status !== "running" || !beginLifecycleAction("stop")) return;
    setError(null);
    try {
      await stopProjectTerminal(project.id, terminal.id);
      setTerminalConfirmation((current) => current?.projectId === project.id ? null : current);
    } catch (stopError) {
      setError(terminalErrorMessage(stopError));
    } finally {
      setBusyAction(null);
    }
  };

  // handleCloseTerminal removes the selected tab after stopping its isolated host.
  // handleCloseTerminal 在停止隔离宿主后移除选中的标签页。
  // Parameters: terminal identifies the exact host/tab; returns a promise after close completes, retaining confirmation on failure.
  // 参数：terminal 标识精确宿主和标签；返回关闭完成后的 Promise，失败时保留确认。
  const handleCloseTerminal = async (terminal: ProjectTerminalSummary) => {
    if (!beginLifecycleAction("close")) return;
    setError(null);
    try {
      const nextCatalog = await closeProjectTerminal(project.id, terminal.id);
      if (nextCatalog) {
        releaseProjectTerminalInputOwner(terminal.id);
        clearProjectTerminalInputBuffer(project.id, terminal.id);
        applyCatalog(nextCatalog, setCatalog, setSelectedTerminalId, setSelectedShellId);
        setTerminalConfirmation((current) => current?.projectId === project.id ? null : current);
      }
    } catch (closeError) {
      setError(terminalErrorMessage(closeError));
    } finally {
      setBusyAction(null);
    }
  };

  // handleRenameTerminal persists one validated name without restarting the terminal process.
  // handleRenameTerminal 持久化一个已验证名称，而不重启终端进程。
  // Parameters: none; uses the selected terminal and raw name draft. Returns after validation and guarded rename, preserving the draft on failure.
  // 参数：无，使用选中终端及原始名称草稿。在校验和受保护重命名后返回，失败时保留草稿。
  const handleRenameTerminal = async () => {
    if (!selectedTerminal || renamingTerminalId !== selectedTerminal.id || busyAction) return;
    const name = renameDraft.trim();
    if (!name) {
      setError(t("workspaceAudit.emptyName"));
      return;
    }
    if (!beginLifecycleAction("rename")) return;
    setError(null);
    try {
      const nextCatalog = await renameProjectTerminal(project.id, selectedTerminal.id, name);
      if (nextCatalog) applyCatalog(nextCatalog, setCatalog, setSelectedTerminalId, setSelectedShellId);
      updateActiveState((current) => ({ ...current, renamingTerminalId: null, renameDraft: "" }));
    } catch (renameError) {
      setError(terminalErrorMessage(renameError));
    } finally {
      setBusyAction(null);
    }
  };

  // header renders terminal identity plus window commands owned by the generic floating container.
  // header 渲染终端标识及通用浮动容器持有的窗口命令。
  // Parameters: floatingControls exposes window commands owned by the floating layer; returns terminal identity and window-action React content.
  // 参数：floatingControls 暴露浮动层持有的窗口命令；返回终端身份与窗口操作的 React 内容。
  const header = (floatingControls: ProjectFloatingLayerControls) => (
    <div className="project-terminal-header" data-workspace-input-preserve-owner="true">
      <span
        className="project-terminal-current-title"
        title={selectedTerminal ? `${selectedTerminal.name} · ${project.path}` : project.path}
      >
        <TerminalSquare size={15} />
        <span className="project-terminal-current-title-copy">
          <strong>{selectedTerminal?.name ?? t("workspaceAudit.terminal")}</strong>
          <small>{project.name}</small>
        </span>
      </span>
      <div className="project-terminal-header-actions">
        <button
          aria-label={t(sidebarCollapsed ? "workspaceAudit.expandList" : "workspaceAudit.collapseList")}
          onClick={() => setSidebarCollapsed((current) => !current)}
          title={t(sidebarCollapsed ? "workspaceAudit.expandList" : "workspaceAudit.collapseList")}
          type="button"
        >
          {sidebarCollapsed ? <PanelLeftOpen size={14} /> : <PanelLeftClose size={14} />}
        </button>
        <button
          aria-label={t(floatingControls.maximized ? "workspaceAudit.restore" : "workspaceAudit.maximize")}
          onClick={floatingControls.toggleMaximized}
          title={t(floatingControls.maximized ? "workspaceAudit.restore" : "workspaceAudit.maximize")}
          type="button"
        >
          {floatingControls.maximized ? <Minimize2 size={14} /> : <Maximize2 size={14} />}
        </button>
        <button
          aria-label={t("workspaceAudit.minimize")}
          onClick={minimizeProjectTerminal}
          title={t("workspaceAudit.minimize")}
          type="button"
        >
          <Minus size={14} />
        </button>
      </div>
    </div>
  );

  const confirmationTerminal = terminalConfirmation?.projectId === project.id
    ? catalog?.terminals.find((terminal) => terminal.id === terminalConfirmation.terminalId) ?? null
    : null;

  return (
    <>
      <ProjectFloatingLayer
        key={project.id}
        expanded={expanded}
        header={header}
        minimizeAnchorRef={props.minimizeAnchorRef}
        projectId={project.id}
      >
        <div className="project-terminal-body" data-sidebar-collapsed={sidebarCollapsed ? "true" : "false"}>
          {error ? <div className="project-terminal-error" role="alert">{error}
            {catalog ? <Button disabled={catalogLoading || busyAction !== null} size="sm" variant="outline"
              onClick={() => setCatalogReload((current) => current + 1)}>{t("workspaceAudit.retry")}</Button> : null}
          </div> : null}
          {!catalog ? (
            <div className="project-terminal-placeholder" role="status">
              {catalogLoading ? <StreamPlaceholder className="w-full max-w-sm" label={t("workspaceAudit.terminalLoading")} shape="rows" count={3} /> : (
                <Button size="sm" variant="outline" onClick={() => setCatalogReload((current) => current + 1)}>
                  {t("workspaceAudit.retry")}
                </Button>
              )}
            </div>
          ) : (
            <>
              <aside
                className="project-terminal-sidebar"
                data-collapsed={sidebarCollapsed ? "true" : "false"}
                data-workspace-input-preserve-owner="true"
              >
                {sidebarCollapsed ? (
                  <div
                    aria-label={t("workspaceAudit.numberedList")}
                    aria-orientation="vertical"
                    className="project-terminal-compact-session-list"
                    role="tablist"
                    onKeyDown={handleTerminalTabKeyDown}
                  >
                    {catalog.terminals.map((terminal, index) => (
                      <button
                        aria-controls={`project-terminal-panel-${terminal.id}`}
                        aria-label={t("workspaceAudit.switchTerminal").replace("{index}", String(index + 1)).replace("{name}", terminal.name)}
                        aria-selected={terminal.id === selectedTerminalId}
                        className="project-terminal-compact-session-button"
                        data-active={terminal.id === selectedTerminalId ? "true" : "false"}
                        data-status={terminal.status}
                        disabled={busyAction !== null}
                        key={terminal.id}
                        id={`project-terminal-tab-${terminal.id}`}
                        tabIndex={terminal.id === selectedTerminalId ? 0 : -1}
                        onClick={() => handleSelectTerminal(terminal.id)}
                        role="tab"
                        title={`${index + 1} · ${terminal.name} · ${terminal.shellId}`}
                        type="button"
                      >
                        {index + 1}
                      </button>
                    ))}
                  </div>
                ) : (
                  <>
                    <div className="project-terminal-sidebar-create">
                  <Select
                    disabled={catalog.shells.length === 0 || busyAction !== null}
                    onValueChange={(value) => setSelectedShellId(value)}
                    value={selectedShellId ?? undefined}
                  >
                    <SelectTrigger
                      aria-label={t("workspaceAudit.chooseShell")}
                      className="project-terminal-select-trigger"
                      size="sm"
                    >
                      <SelectValue />
                    </SelectTrigger>
                    <SelectContent data-workspace-input-preserve-owner="true" placement="top">
                      {catalog.shells.map((shell) => (
                        <SelectItem key={shell.id} value={shell.id}>{shell.label}</SelectItem>
                      ))}
                    </SelectContent>
                  </Select>
                  <button
                    aria-label={t("workspaceAudit.create")}
                    className="project-terminal-sidebar-add"
                    disabled={!catalog.shells.some((shell) => shell.id === selectedShellId) || busyAction !== null}
                    onClick={() => void handleCreateTerminal()}
                    title={t("workspaceAudit.create")}
                    type="button"
                  >
                    <Plus size={14} />
                  </button>
                    </div>
                    <div
                  aria-label={t("workspaceAudit.list")}
                  aria-orientation="vertical"
                  className="project-terminal-session-list"
                  role="tablist"
                  onKeyDown={handleTerminalTabKeyDown}
                    >
                      {catalog.terminals.map((terminal) => (
                    <div
                      className="project-terminal-session-row"
                      data-active={terminal.id === selectedTerminalId ? "true" : "false"}
                      data-status={terminal.status}
                      key={terminal.id}
                    >
                      {renamingTerminalId === terminal.id && selectedTerminalId === terminal.id ? (
                        <input
                          aria-label={t("workspaceAudit.name")}
                          autoFocus
                          className="project-terminal-session-rename-input"
                          disabled={busyAction !== null}
                          id={`project-terminal-tab-${terminal.id}`}
                          maxLength={128}
                          onChange={(event) => setRenameDraft(event.target.value)}
                          onKeyDown={(event) => {
                            if (event.key === "Enter") {
                              event.preventDefault();
                              void handleRenameTerminal();
                            } else if (event.key === "Escape") {
                              event.preventDefault();
                              updateActiveState((current) => ({
                                ...current,
                                renamingTerminalId: null,
                                renameDraft: "",
                              }));
                            }
                          }}
                          value={renameDraft}
                        />
                      ) : (
                        <button
                          aria-controls={`project-terminal-panel-${terminal.id}`}
                          aria-selected={terminal.id === selectedTerminalId}
                          className="project-terminal-session-select"
                          disabled={busyAction !== null}
                          onClick={() => handleSelectTerminal(terminal.id)}
                          role="tab"
                          id={`project-terminal-tab-${terminal.id}`}
                          tabIndex={terminal.id === selectedTerminalId ? 0 : -1}
                          title={`${terminal.name} · ${terminal.shellId}`}
                          type="button"
                        >
                          <span aria-hidden="true" className="project-terminal-session-status" />
                          <span className="project-terminal-session-name">{terminal.name}</span>
                        </button>
                      )}
                      <div className="project-terminal-session-actions">
                        <button
                          aria-label={t(renamingTerminalId === terminal.id ? "workspaceAudit.saveName" : "workspaceAudit.editName").replace("{name}", terminal.name)}
                          disabled={busyAction !== null}
                          onClick={() => {
                            if (renamingTerminalId === terminal.id) {
                              void handleRenameTerminal();
                              return;
                            }
                            handleSelectTerminal(terminal.id);
                            beginRename(terminal);
                          }}
                          title={t(renamingTerminalId === terminal.id ? "workspaceAudit.saveName" : "workspaceAudit.editName").replace("{name}", terminal.name)}
                          type="button"
                        >
                          {renamingTerminalId === terminal.id ? <Check size={13} /> : <Pencil size={12} />}
                        </button>
                        <button
                          aria-label={t("workspaceAudit.stop").replace("{name}", terminal.name)}
                          disabled={terminal.status !== "running" || busyAction !== null}
                          onClick={() => setTerminalConfirmation({
                            projectId: project.id,
                            terminalId: terminal.id,
                            action: "stop",
                          })}
                          title={t("workspaceAudit.stopTitle")}
                          type="button"
                        >
                          <Square fill="currentColor" size={9} strokeWidth={0} />
                        </button>
                        <button
                          aria-label={t("workspaceAudit.close").replace("{name}", terminal.name)}
                          disabled={busyAction !== null}
                          onClick={() => {
                            if (terminal.status === "running") {
                              setTerminalConfirmation({
                                projectId: project.id,
                                terminalId: terminal.id,
                                action: "close",
                              });
                            } else {
                              void handleCloseTerminal(terminal);
                            }
                          }}
                          title={t("workspaceAudit.closeTitle")}
                          type="button"
                        >
                          <X size={12} />
                        </button>
                      </div>
                    </div>
                      ))}
                    </div>
                  </>
                )}
              </aside>
              <div
                className="project-terminal-stage"
                data-selected-terminal-id={selectedTerminal?.id}
              >
                {selectedTerminal ? (
                  <ProjectTerminalPane
                    key={selectedTerminal.id}
                    autoFocus={renamingTerminalId === null}
                    focusRequestNonce={terminalFocusRequestNonce}
                    projectId={project.id}
                    terminal={selectedTerminal}
                    visible={expanded}
                    onError={setError}
                    onTerminalChanged={(changedTerminal) => {
                      setCatalog((current) => current ? {
                        ...current,
                        terminals: current.terminals.map((entry) => (
                          entry.id === changedTerminal.id ? changedTerminal : entry
                        )),
                      } : current);
                    }}
                  />
                ) : (
                  <div className="project-terminal-empty">
                    <TerminalSquare size={24} />
                    <span>{t("workspaceAudit.empty")}</span>
                  </div>
                )}
              </div>
            </>
          )}
        </div>
      </ProjectFloatingLayer>
      <AlertDialog
        open={confirmationTerminal !== null}
        onOpenChange={(open) => {
          if (!open && !projectStates.current.get(project.id)?.busyAction) setTerminalConfirmation(null);
        }}
      >
        <AlertDialogContent layerClassName="z-[1000]">
          <AlertDialogHeader>
            <AlertDialogTitle>
              {t(terminalConfirmation?.action === "close" ? "workspaceAudit.closeConfirm" : "workspaceAudit.stopConfirm")}
            </AlertDialogTitle>
            <AlertDialogDescription>
              {terminalConfirmation?.action === "close"
                ? t("workspaceAudit.closeDescription").replace("{name}", confirmationTerminal?.name ?? t("workspaceAudit.terminal"))
                : t("workspaceAudit.stopDescription").replace("{name}", confirmationTerminal?.name ?? t("workspaceAudit.terminal"))}
            </AlertDialogDescription>
          </AlertDialogHeader>
          {error ? <p className="text-destructive break-words" role="alert">{error}</p> : null}
          <AlertDialogFooter>
            <AlertDialogCancel disabled={busyAction !== null}>{t("workspaceAudit.cancel")}</AlertDialogCancel>
            <AlertDialogAction
              variant="destructive"
              disabled={!confirmationTerminal || busyAction !== null}
              onClick={(event) => {
                event.preventDefault();
                if (!confirmationTerminal || !terminalConfirmation) return;
                if (terminalConfirmation.action === "close") void handleCloseTerminal(confirmationTerminal);
                else void handleStopTerminal(confirmationTerminal);
              }}
            >
              {busyAction ? t("workspaceAudit.processing") : t(terminalConfirmation?.action === "close"
                ? "workspaceAudit.close" : "workspaceAudit.stop").replace("{name}", confirmationTerminal?.name ?? "")}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </>
  );
}

// pruneRemovedProjectTerminalUiState removes floating-layer state that no longer has a workbench owner.
// pruneRemovedProjectTerminalUiState 移除已不存在工作台归属的浮动层状态。
//
// Parameters / 参数:
// - `projectStates`: Project-keyed floating terminal UI state. / 按项目键控的浮动终端界面状态。
// - `seenRequests`: Project-keyed handled visibility-request revisions. / 按项目键控的已处理可见性请求修订。
// - `projects`: Authoritative projects currently present in the workbench. / 当前存在于工作台的权威项目集合。
//
// Returns / 返回值:
// Returns nothing after deleting state for every removed project.
// 删除每个已移除项目的状态后无返回值。
export function pruneRemovedProjectTerminalUiState<T>(
  projectStates: Map<string, T>,
  seenRequests: Map<string, number>,
  projects: readonly Pick<ProjectSummary, "id">[],
): void {
  const retainedProjectIds = new Set(projects.map((project) => project.id));
  for (const projectId of projectStates.keys()) {
    if (!retainedProjectIds.has(projectId)) projectStates.delete(projectId);
  }
  for (const projectId of seenRequests.keys()) {
    if (!retainedProjectIds.has(projectId)) seenRequests.delete(projectId);
  }
}

// ProjectTerminalPane owns the visible subscription and ordered xterm input for one project terminal.
// ProjectTerminalPane 为一个项目终端持有可见订阅与有序 xterm 输入。
// Parameters: props identifies the project terminal, display snapshot and interaction callbacks; returns one ordered terminal viewport and its live status.
// 参数：props 标识项目终端、显示快照及交互回调；返回一个有序终端视口及其实时状态。
function ProjectTerminalPane(props: {
  // Exact owning project identity.
  // 精确的归属项目标识。
  projectId: string;
  // Exact visible terminal summary.
  // 精确的可见终端摘要。
  terminal: ProjectTerminalSummary;
  // Whether the visible xterm may claim focus outside inline terminal-name editing.
  // 可见 xterm 是否可在行内终端名称编辑之外获取焦点。
  autoFocus: boolean;
  // Monotonic explicit focus request for repeated selection of the current terminal.
  // 重复选择当前终端时使用的单调明确焦点请求。
  focusRequestNonce: number;
  // Whether the display layer may attach and receive terminal content.
  // 显示层是否可以挂接并接收终端内容。
  visible: boolean;
  // Publishes live terminal lifecycle changes to the catalog.
  // 向目录发布实时终端生命周期变化。
  onTerminalChanged: (terminal: ProjectTerminalSummary) => void;
  // Publishes an explicit input, resize, or subscription failure.
  // 发布明确的输入、调整尺寸或订阅失败。
  onError: (message: string | null) => void;
}) {
  const { t } = useI18n();
  // connectionState detaches display delivery during interruption and forces a new checkpoint after recovery.
  // connectionState 在中断期间解除显示交付，并在恢复后强制获取新检查点。
  const { connectionAvailable, connectionEpoch } = useTerminalDisplayConnectionState();
  // chunks contains either the initial retained tail or one incremental push.
  // chunks 包含首次保留尾部或一次增量推送。
  const [chunks, setChunks] = React.useState<TerminalDisplayChunk[]>([]);
  // displayAttached keeps cached hidden xterm instances read-only until their fresh visible checkpoint is ready.
  // displayAttached 在新的可见检查点就绪前，使缓存的隐藏 xterm 实例保持只读。
  const [displayAttached, setDisplayAttached] = React.useState(false);
  // inputBuffer survives tab, floating-surface, and project visibility changes for this terminal UUID.
  // inputBuffer 在当前终端 UUID 的标签、浮动界面与项目可见性变化期间持续存在。
  const inputBuffer = React.useRef(getOrCreateProjectTerminalInputBuffer(
    props.projectId,
    props.terminal.id,
    props.terminal.lastAcceptedInputSeq,
  ));
  // inputStopped blocks new delivery after an acknowledgement failure.
  // inputStopped 在确认失败后阻止新的输入交付。
  const inputStopped = React.useRef(false);
  // inputAnimationFrame coalesces already-arrived events once per browser frame.
  // inputAnimationFrame 每个浏览器帧合并一次已经到达的事件。
  const inputAnimationFrame = React.useRef<number | null>(null);
  // flushInputRef breaks the scheduling callback cycle while retaining current project ownership.
  // flushInputRef 在保留当前项目归属的同时打破调度回调循环。
  const flushInputRef = React.useRef<() => Promise<void>>(async () => undefined);
  // pendingResize retains only the newest grid while a resize request is active.
  // pendingResize 在尺寸请求执行期间仅保留最新字符网格。
  const pendingResize = React.useRef<{ rows: number; cols: number } | null>(null);
  // resizeActive prevents parallel PTY resize requests.
  // resizeActive 防止 PTY 尺寸请求并行执行。
  const resizeActive = React.useRef(false);
  // resizeSettlement blocks input until the latest event-driven resize series has drained.
  // resizeSettlement 在最新事件驱动尺寸序列排空前阻塞输入。
  const resizeSettlement = React.useRef<ProjectTerminalResizeSettlement | null>(null);
  // resizeDisposed prevents an in-flight resize completion from scheduling work after pane disposal.
  // resizeDisposed 防止进行中的尺寸请求在面板释放后继续调度工作。
  const resizeDisposed = React.useRef(false);

  React.useEffect(() => {
    inputStopped.current = false;
    resizeDisposed.current = false;
    if (inputAnimationFrame.current !== null) {
      window.cancelAnimationFrame(inputAnimationFrame.current);
      inputAnimationFrame.current = null;
    }
    return () => {
      releaseProjectTerminalInputOwner(props.terminal.id);
      inputStopped.current = true;
      resizeDisposed.current = true;
      pendingResize.current = null;
      // abandonedSettlement releases input/reconnect waiters when this terminal pane no longer owns delivery.
      // abandonedSettlement 在当前终端面板不再持有交付权时释放输入及重连等待者。
      const abandonedSettlement = resizeSettlement.current;
      resizeSettlement.current = null;
      abandonedSettlement?.resolve();
      if (inputAnimationFrame.current !== null) {
        window.cancelAnimationFrame(inputAnimationFrame.current);
        inputAnimationFrame.current = null;
      }
    };
  }, [props.projectId, props.terminal.id]);

  // Discard queued PTY resizing once lifecycle state no longer accepts resize controls.
  // 生命周期状态不再接受尺寸控制后丢弃排队的 PTY 尺寸调整。
  React.useEffect(() => {
    if (props.terminal.status === "running") return;
    pendingResize.current = null;
    if (!resizeActive.current) {
      // stoppedSettlement releases any input already waiting behind a resize that will not be delivered.
      // stoppedSettlement 释放已等待在不会继续交付的尺寸请求之后的输入。
      const stoppedSettlement = resizeSettlement.current;
      resizeSettlement.current = null;
      stoppedSettlement?.resolve();
    }
  }, [props.terminal.status]);

  React.useEffect(() => {
    setDisplayAttached(false);
    if (!props.visible || !connectionAvailable) {
      inputStopped.current = true;
      return;
    }
    let cancelled = false;
    let subscriptionId: string | null = null;
    let unsubscribe: (() => void) | null = null;
    const pendingEvents: ProjectTerminalDisplayChangedEvent[] = [];
    // releaseDisplayListener removes the browser topic handler exactly once.
    // releaseDisplayListener 仅一次移除浏览器主题处理器。
    const releaseDisplayListener = () => {
      // retainedUnsubscribe moves the disposer out before invocation to make cleanup idempotent.
      // retainedUnsubscribe 在调用前移出释放函数，使清理具备幂等性。
      const retainedUnsubscribe = unsubscribe;
      unsubscribe = null;
      retainedUnsubscribe?.();
    };
    // releaseDisplayAttachment detaches the service-side push subscription exactly once.
    // releaseDisplayAttachment 仅一次解除服务端推送订阅。
    const releaseDisplayAttachment = () => {
      // retainedSubscriptionId moves the service id out before issuing the detach request.
      // retainedSubscriptionId 在发出解除请求前移出服务标识。
      const retainedSubscriptionId = subscriptionId;
      subscriptionId = null;
      if (retainedSubscriptionId) void detachProjectTerminalDisplay(retainedSubscriptionId);
    };
    const applyEvent = (event: ProjectTerminalDisplayChangedEvent) => {
      if (cancelled || event.subscriptionId !== subscriptionId) return;
      props.onTerminalChanged(event.terminal);
      setChunks((current) => mergeVisibleTerminalChunks(
        current,
        event.chunks,
        event.terminal.droppedBeforeSeq,
      ));
    };
    const attach = async () => {
      try {
        // A reconnect query must run after the previous transport request settles; otherwise
        // it could observe an older acknowledgement and resend bytes that the host just accepted.
        // 重连查询必须等待上一条传输请求结束，否则可能读取到较旧确认并重发宿主刚刚接受的字节。
        const unsettledInput = inputBuffer.current.batchSettled;
        if (unsettledInput) await unsettledInput;
        if (cancelled) return;
        // listenerAcquired reports whether this pane remained visible through asynchronous topic setup.
        // listenerAcquired 报告当前面板在异步主题建立期间是否始终保持可见。
        const listenerAcquired = await acquireVisibleTerminalDisplayListener(
          () => serviceClient.subscribe<ProjectTerminalDisplayChangedEvent>(
            "project.terminal.display.changed",
            (event) => {
              // Ignore other visible project terminals while this attachment id is not known yet.
              // 当前挂接标识尚未返回时，忽略其他可见项目终端的事件。
              if (event.terminal.projectId !== props.projectId
                || event.terminal.id !== props.terminal.id) return;
              if (subscriptionId === null) pendingEvents.push(event);
              else applyEvent(event);
            },
          ),
          () => cancelled,
          (retainedUnsubscribe) => {
            unsubscribe = retainedUnsubscribe;
          },
        );
        if (!listenerAcquired) return;
        const attachment = await attachProjectTerminalDisplay(props.projectId, props.terminal.id);
        if (!attachment) {
          releaseDisplayListener();
          return;
        }
        subscriptionId = attachment.subscriptionId;
        if (cancelled) {
          releaseDisplayListener();
          // cancelledSubscriptionId retains the just-created service id for awaited cancellation cleanup.
          // cancelledSubscriptionId 为等待完成的取消清理保留刚创建的服务标识。
          const cancelledSubscriptionId = subscriptionId;
          subscriptionId = null;
          if (cancelledSubscriptionId) await detachProjectTerminalDisplay(cancelledSubscriptionId);
          return;
        }
        const acknowledgedSeq = attachment.terminal.lastAcceptedInputSeq;
        while (inputBuffer.current.pending[0]?.seq <= acknowledgedSeq) {
          const accepted = inputBuffer.current.pending.shift();
          if (accepted) inputBuffer.current.pendingBytes -= accepted.byteLength;
        }
        const firstPendingSeq = inputBuffer.current.pending[0]?.seq;
        if (firstPendingSeq !== undefined && firstPendingSeq !== acknowledgedSeq + 1) {
          inputStopped.current = true;
          props.onError(
            formatMessage(translateActive("project.terminal.inputRecoveryFailed"), { acknowledgedSeq, firstPendingSeq }),
          );
          return;
        }
        inputBuffer.current.nextClientSeq = Math.max(
          inputBuffer.current.nextClientSeq,
          acknowledgedSeq + 1,
        );
        inputStopped.current = attachment.terminal.status !== "running";
        setDisplayAttached(true);
        props.onTerminalChanged(attachment.terminal);
        setChunks(mergeVisibleTerminalChunks(
          [],
          attachment.chunks,
          attachment.terminal.droppedBeforeSeq,
        ));
        for (const event of pendingEvents) applyEvent(event);
        props.onError(null);
        if (!inputStopped.current
          && !inputBuffer.current.sending
          && inputBuffer.current.pending.length > 0
          && inputAnimationFrame.current === null) {
          inputAnimationFrame.current = window.requestAnimationFrame(() => {
            inputAnimationFrame.current = null;
            void flushInputRef.current();
          });
        }
      } catch (attachError) {
        setDisplayAttached(false);
        releaseDisplayListener();
        releaseDisplayAttachment();
        if (!cancelled) props.onError(terminalErrorMessage(attachError));
      }
    };
    setChunks([]);
    void attach();
    return () => {
      cancelled = true;
      inputStopped.current = true;
      if (inputAnimationFrame.current !== null) {
        window.cancelAnimationFrame(inputAnimationFrame.current);
        inputAnimationFrame.current = null;
      }
      releaseDisplayListener();
      releaseDisplayAttachment();
    };
  }, [connectionAvailable, connectionEpoch, props.projectId, props.terminal.id, props.visible]);

  // scheduleInputFlush creates exactly one event-driven animation-frame batch request.
  // scheduleInputFlush 创建恰好一个由事件驱动的动画帧批次请求。
  const scheduleInputFlush = React.useCallback(() => {
    if (inputStopped.current || inputAnimationFrame.current !== null) return;
    inputAnimationFrame.current = window.requestAnimationFrame(() => {
      inputAnimationFrame.current = null;
      void flushInputRef.current();
    });
  }, []);

  // flushInput sends one bounded contiguous batch and removes events only after exact acknowledgement.
  // flushInput 发送一个有界连续批次，并仅在精确确认后移除事件。
  const flushInput = React.useCallback(async () => {
    if (inputStopped.current || inputBuffer.current.sending
      || inputBuffer.current.pending.length === 0) return;
    inputBuffer.current.sending = true;
    // settledPromise lets a reconnect serialize its authoritative acknowledgement query after this request.
    // settledPromise 使重连能够在当前请求之后串行执行权威确认查询。
    let markSettled: () => void = () => undefined;
    const settledPromise = new Promise<void>((resolve) => {
      markSettled = resolve;
    });
    inputBuffer.current.batchSettled = settledPromise;
    try {
      // resizeBarrier preserves browser event order across independent input and resize service requests.
      // resizeBarrier 在相互独立的输入与尺寸服务请求之间保留浏览器事件顺序。
      const resizeBarrier = resizeSettlement.current?.promise;
      if (resizeBarrier) await resizeBarrier;
      if (inputStopped.current || inputBuffer.current.pending.length === 0) return;
      // batch contains one bounded contiguous prefix after the latest known PTY grid is ordered first.
      // batch 在最新已知 PTY 网格先行排序后包含一个有界连续前缀。
      const batch: QueuedProjectTerminalInput[] = [];
      // batchBytes tracks the exact UTF-8 or binary byte total for the service limit.
      // batchBytes 跟踪服务限制使用的精确 UTF-8 或二进制字节总量。
      let batchBytes = 0;
      for (const entry of inputBuffer.current.pending) {
        if (batch.length >= PROJECT_TERMINAL_INPUT_BATCH_MAX_EVENTS) break;
        if (batch.length > 0 && batchBytes + entry.byteLength > PROJECT_TERMINAL_INPUT_BATCH_MAX_BYTES) break;
        batch.push(entry);
        batchBytes += entry.byteLength;
      }
      const firstClientSeq = batch[0].seq;
      const expectedAcknowledgement = batch[batch.length - 1].seq;
      const acknowledgedSeq = await sendProjectTerminalInputBatch(
        props.projectId,
        props.terminal.id,
        firstClientSeq,
        batch.map((entry) => entry.event),
      );
      if (acknowledgedSeq !== expectedAcknowledgement) {
        throw new Error(
          formatMessage(translateActive("project.terminal.ackGap"), { expected: expectedAcknowledgement, actual: acknowledgedSeq }),
        );
      }
      while (inputBuffer.current.pending[0]?.seq <= acknowledgedSeq) {
        const accepted = inputBuffer.current.pending.shift();
        if (accepted) inputBuffer.current.pendingBytes -= accepted.byteLength;
      }
    } catch (inputError) {
      inputStopped.current = true;
      props.onError(formatMessage(translateActive("project.terminal.inputPaused"), { reason: terminalErrorMessage(inputError) }));
    } finally {
      inputBuffer.current.sending = false;
      if (inputBuffer.current.batchSettled === settledPromise) {
        inputBuffer.current.batchSettled = null;
      }
      markSettled();
      if (!inputStopped.current && inputBuffer.current.pending.length > 0) scheduleInputFlush();
    }
  }, [props.onError, props.projectId, props.terminal.id, scheduleInputFlush]);
  flushInputRef.current = flushInput;

  // enqueueInput fragments large values, allocates sequences, and preserves a bounded unacknowledged queue.
  // enqueueInput 分割大输入、分配序号并维护有界的未确认队列。
  const enqueueInput = React.useCallback((kind: "text" | "binary", data: string) => {
    if (inputStopped.current || data.length === 0) return;
    const fragments = kind === "binary"
      ? splitBinaryTerminalInput(data, PROJECT_TERMINAL_INPUT_FRAGMENT_MAX_BYTES)
      : splitUtf8TerminalInput(data, PROJECT_TERMINAL_INPUT_FRAGMENT_MAX_BYTES);
    const inputBytes = fragments.reduce((total, fragment) => total + fragment.byteLength, 0);
    // Preflight the complete xterm event so capacity or sequence failure can never queue a partial paste.
    // 预检完整 xterm 事件，确保容量或序号失败绝不会排队部分粘贴内容。
    if (inputBuffer.current.pendingBytes + inputBytes > PROJECT_TERMINAL_PENDING_INPUT_MAX_BYTES) {
      inputStopped.current = true;
      props.onError(formatMessage(translateActive("project.terminal.inputQueueFull"), { limit: formatBytes(PROJECT_TERMINAL_PENDING_INPUT_MAX_BYTES) }));
      return;
    }
    const firstSequence = inputBuffer.current.nextClientSeq;
    const lastSequence = firstSequence + fragments.length - 1;
    if (!Number.isSafeInteger(firstSequence) || firstSequence <= 0
      || !Number.isSafeInteger(lastSequence)) {
      inputStopped.current = true;
      props.onError(translateActive("project.terminal.sequenceOverflow"));
      return;
    }
    for (const fragment of fragments) {
      const seq = inputBuffer.current.nextClientSeq;
      inputBuffer.current.nextClientSeq = seq + 1;
      inputBuffer.current.pending.push({
        seq,
        event: fragment.event,
        byteLength: fragment.byteLength,
      });
      inputBuffer.current.pendingBytes += fragment.byteLength;
    }
    scheduleInputFlush();
  }, [props.onError, scheduleInputFlush]);

  // flushResize serializes immediate latest-wins PTY grid requests without timers or concurrent writes.
  // flushResize 串行化即时且最新值优先的 PTY 网格请求，不使用计时器或并发写入。
  const flushResize = React.useCallback(async () => {
    if (resizeActive.current || resizeDisposed.current) return;
    // dimensions moves the newest observed grid out before the asynchronous request starts.
    // dimensions 在异步请求开始前移出最新观察到的网格。
    const dimensions = pendingResize.current;
    if (!dimensions) return;
    pendingResize.current = null;
    resizeActive.current = true;
    try {
      await resizeProjectTerminalInOrder(
        props.projectId,
        props.terminal.id,
        dimensions.rows,
        dimensions.cols,
      );
    } catch (resizeError) {
      if (!resizeDisposed.current) props.onError(terminalErrorMessage(resizeError));
    } finally {
      resizeActive.current = false;
      if (!resizeDisposed.current && pendingResize.current) {
        void flushResize();
      } else {
        // Settle only the drained series, without overriding an exception from the error callback.
        // 仅结算已经排空的序列，避免覆盖错误回调抛出的异常。
        const completedSettlement = resizeSettlement.current;
        resizeSettlement.current = null;
        completedSettlement?.resolve();
      }
    }
  }, [props.onError, props.projectId, props.terminal.id]);

  // handleResize immediately starts a serial resize series and retains only the newest in-flight measurement.
  // handleResize 立即启动串行尺寸序列，并仅保留请求期间最新的测量值。
  const handleResize = React.useCallback((rows: number, cols: number) => {
    if (props.terminal.status !== "running" || resizeDisposed.current) return;
    pendingResize.current = { rows, cols };
    if (!resizeSettlement.current) {
      // resolveSettlement completes the promise owned by this exact burst of grid changes.
      // resolveSettlement 完成当前这组精确网格变化持有的 Promise。
      let resolveSettlement: () => void = () => undefined;
      // settlementPromise is awaited by input that arrived after this measured grid change.
      // settlementPromise 由在当前网格测量变化之后到达的输入等待。
      const settlementPromise = new Promise<void>((resolve) => {
        resolveSettlement = resolve;
      });
      resizeSettlement.current = { promise: settlementPromise, resolve: resolveSettlement };
    }
    void flushResize();
  }, [flushResize, props.terminal.status]);

  return (
    <div
      aria-labelledby={`project-terminal-tab-${props.terminal.id}`}
      className="project-terminal-pane"
      hidden={!props.visible}
      id={`project-terminal-panel-${props.terminal.id}`}
      role="tabpanel"
    >
      <TerminalViewport
        ariaLabel={`${props.terminal.name} · ${t("workspaceAudit.terminal")}`}
        autoFocus={props.visible && props.autoFocus}
        chunks={chunks}
        focusRequestNonce={props.focusRequestNonce}
        inputOwnerId={props.terminal.id}
        onBinary={(data) => enqueueInput("binary", data)}
        onData={(data) => enqueueInput("text", data)}
        onChunksRendered={(lastRenderedSeq) => {
          setChunks((current) => retainRenderedTerminalCursor(current, lastRenderedSeq));
        }}
        onResize={handleResize}
        readOnly={!props.visible
          || !displayAttached
          || !connectionAvailable
          || props.terminal.status !== "running"}
      />
      {props.visible && props.terminal.status === "running" && !displayAttached ? (
        <div className="project-terminal-status" role="status">{t("workspaceAudit.connecting")}</div>
      ) : null}
      {props.terminal.status !== "running" ? (
        <div className="project-terminal-status" role="status">
          {projectTerminalStatusLabel(props.terminal, t)}
        </div>
      ) : null}
    </div>
  );
}

// retainRenderedTerminalCursor keeps one sequence marker while releasing rendered output text history.
// retainRenderedTerminalCursor 保留一个序号标记，同时释放已渲染的输出文本历史。
function retainRenderedTerminalCursor(
  current: TerminalDisplayChunk[],
  lastRenderedSeq: number,
): TerminalDisplayChunk[] {
  const retained = current.filter((chunk) => chunk.seq >= lastRenderedSeq);
  return retained.length === current.length ? current : retained;
}

// createProjectTerminalUiState builds one untouched project-scoped UI entry.
// createProjectTerminalUiState 构建一个未使用的项目级界面条目。
function createProjectTerminalUiState(): ProjectTerminalUiState {
  return {
    expanded: false,
    sidebarCollapsed: false,
    catalog: null,
    selectedTerminalId: null,
    terminalFocusRequestNonce: 0,
    selectedShellId: null,
    busyAction: null,
    error: null,
    renamingTerminalId: null,
    renameDraft: "",
  };
}

// getOrCreateProjectTerminalUiState returns the unique UI entry for one exact project.
// getOrCreateProjectTerminalUiState 返回一个精确项目的唯一界面条目。
function getOrCreateProjectTerminalUiState(
  states: Map<string, ProjectTerminalUiState>,
  projectId: string,
): ProjectTerminalUiState {
  const existing = states.get(projectId);
  if (existing) return existing;
  const created = createProjectTerminalUiState();
  states.set(projectId, created);
  return created;
}

// resolveStateAction applies React's value-or-updater state contract outside useState.
// resolveStateAction 在 useState 之外应用 React 的值或更新器状态契约。
function resolveStateAction<T>(action: React.SetStateAction<T>, current: T): T {
  return typeof action === "function"
    ? (action as (previous: T) => T)(current)
    : action;
}

// mergeVisibleTerminalChunks preserves every ordered push across React event batching.
// mergeVisibleTerminalChunks 在 React 事件批处理中保留每个有序推送。
export function mergeVisibleTerminalChunks(
  current: TerminalDisplayChunk[],
  incoming: TerminalDisplayChunk[],
  droppedBeforeSeq: number,
): TerminalDisplayChunk[] {
  const lastSequence = current.at(-1)?.seq ?? 0;
  const appended = incoming.filter((chunk) => chunk.seq > lastSequence);
  if (lastSequence < droppedBeforeSeq && appended[0]?.seq > droppedBeforeSeq) {
    appended.unshift({
      seq: droppedBeforeSeq,
      stream: "stderr",
      text: `\r\n\x1b[33m${translateActive("project.terminal.outputTruncated")}\x1b[0m\r\n`,
    });
  }
  if (appended.length === 0) return current;
  const merged = [...current, ...appended];
  return merged.length > 4096 ? merged.slice(-4096) : merged;
}

// applyCatalog reconciles terminal and shell selection against one exact project catalog.
// applyCatalog 根据一个精确项目目录协调终端与 Shell 选择。
function applyCatalog(
  catalog: ProjectTerminalCatalog,
  setCatalog: React.Dispatch<React.SetStateAction<ProjectTerminalCatalog | null>>,
  setSelectedTerminalId: React.Dispatch<React.SetStateAction<string | null>>,
  setSelectedShellId: React.Dispatch<React.SetStateAction<string | null>>,
): void {
  setCatalog(catalog);
  setSelectedTerminalId((current) => catalog.terminals.some((terminal) => terminal.id === current)
    ? current
    : catalog.terminals[0]?.id ?? null);
  setSelectedShellId((current) => catalog.shells.some((shell) => shell.id === current)
    ? current
    : catalog.shells.find((shell) => shell.default)?.id ?? catalog.shells[0]?.id ?? null);
}

// terminalErrorMessage converts one unknown service failure into explicit user-facing text.
// terminalErrorMessage 将一个未知服务失败转换为明确的用户可见文本。
function terminalErrorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

// projectTerminalStatusLabel returns a compact terminal lifecycle explanation.
// projectTerminalStatusLabel 返回紧凑的终端生命周期说明。
function projectTerminalStatusLabel(terminal: ProjectTerminalSummary, t: (key: TranslationKey) => string): string {
  if (terminal.failureReason) return t("workspaceAudit.terminalFailure").replace("{error}", terminal.failureReason);
  if (terminal.status === "completed") return terminal.exitCode == null ? t("workspaceAudit.exited")
    : t("workspaceAudit.exitedWithCode").replace("{code}", String(terminal.exitCode));
  if (terminal.status === "stopped") return t("workspaceAudit.stopped");
  if (terminal.status === "failed") return t("workspaceAudit.failed");
  return terminal.status;
}

// splitUtf8TerminalInput preserves Unicode code-point boundaries while creating bounded service events.
// splitUtf8TerminalInput 在创建有界服务事件时保留 Unicode 码点边界。
export function splitUtf8TerminalInput(
  data: string,
  maximumBytes: number,
): Array<{ event: ProjectTerminalInputEventRequest; byteLength: number }> {
  const encoder = new TextEncoder();
  const fragments: Array<{ event: ProjectTerminalInputEventRequest; byteLength: number }> = [];
  let current = "";
  let currentBytes = 0;
  for (const character of data) {
    const characterBytes = encoder.encode(character).byteLength;
    if (current && currentBytes + characterBytes > maximumBytes) {
      fragments.push({ event: { utf8Data: current }, byteLength: currentBytes });
      current = "";
      currentBytes = 0;
    }
    current += character;
    currentBytes += characterBytes;
  }
  if (current) fragments.push({ event: { utf8Data: current }, byteLength: currentBytes });
  return fragments;
}

// splitBinaryTerminalInput preserves each xterm binary code unit as one exact low-eight-bit byte.
// splitBinaryTerminalInput 将每个 xterm 二进制码元保留为一个精确的低八位字节。
export function splitBinaryTerminalInput(
  data: string,
  maximumBytes: number,
): Array<{ event: ProjectTerminalInputEventRequest; byteLength: number }> {
  const fragments: Array<{ event: ProjectTerminalInputEventRequest; byteLength: number }> = [];
  for (let offset = 0; offset < data.length; offset += maximumBytes) {
    const part = data.slice(offset, offset + maximumBytes);
    const bytes = Uint8Array.from(part, (character) => character.charCodeAt(0) & 0xff);
    fragments.push({
      event: { binaryDataBase64: terminalBytesToBase64(bytes) },
      byteLength: bytes.byteLength,
    });
  }
  return fragments;
}

// terminalBytesToBase64 converts one bounded byte fragment without Unicode transformation.
// terminalBytesToBase64 在不执行 Unicode 转换的情况下编码一个有界字节分片。
function terminalBytesToBase64(bytes: Uint8Array): string {
  let binaryText = "";
  for (const byte of bytes) binaryText += String.fromCharCode(byte);
  return btoa(binaryText);
}
