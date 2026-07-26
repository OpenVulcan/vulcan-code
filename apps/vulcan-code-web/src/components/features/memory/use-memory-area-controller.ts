import * as React from "react";

import { useI18n } from "@/i18n";
import type {
  VmmConfigDraftSummary,
  VmmConfigSaveRequest,
  VmmMemoryActionRequest,
  VmmMemoryActionResult,
  VmmMemorySnapshot,
} from "@/types";

import type { VmmScope } from "./vmm-memory-sections";

// MemoryActionProps groups service-backed actions used by the VMM memory page.
// MemoryActionProps 分组 VMM 记忆页面使用的服务端动作。
export interface MemoryActionProps {
  // Loads the exact editable VMM draft for one task project.
  // 加载单个任务项目对应的准确可编辑 VMM 草稿。
  onLoadProjectVmmConfig: (projectId: string) => Promise<VmmConfigDraftSummary>;
  // Runs one VMM memory management action.
  // 执行单个 VMM 记忆管理动作。
  onRunVmmAction: (request: VmmMemoryActionRequest) => Promise<VmmMemoryActionResult>;
  // Saves one VMM memory configuration draft.
  // 保存单个 VMM 记忆配置草稿。
  onSaveVmmConfig: (request: VmmConfigSaveRequest) => Promise<void>;
}

// MemoryAreaController exposes route-safe drafts and one exclusive management operation channel.
// MemoryAreaController 暴露路由安全草稿与单一独占管理操作通道。
export interface MemoryAreaController {
  // Action id that currently owns the exclusive operation channel.
  // 当前拥有独占操作通道的动作标识。
  busyAction: string | null;
  // Editable global VMM configuration draft.
  // 可编辑的全局 VMM 配置草稿。
  globalDraft: VmmConfigDraftSummary;
  // Whether any load, action, or save operation is currently running.
  // 当前是否有加载、动作或保存操作正在运行。
  operationBusy: boolean;
  // Selected task project id for project-scoped operations.
  // 项目级操作当前选择的任务项目标识。
  projectId: string;
  // Exact editable draft owned by the selected project route.
  // 当前选中项目路由拥有的准确可编辑草稿。
  projectDraft: VmmConfigDraftSummary;
  // Whether the selected project owns a loaded or locally edited draft.
  // 当前选中项目是否拥有已加载或本地编辑的草稿。
  projectDraftReady: boolean;
  // Latest user-visible management notice.
  // 最新用户可见管理提示。
  notice: string;
  // Latest accepted VMM action result.
  // 最新被接受的 VMM 动作结果。
  result: VmmMemoryActionResult | null;
  // Current global or project configuration scope.
  // 当前全局或项目配置作用域。
  scope: VmmScope;
  // Runs one action through the exclusive operation channel.
  // 通过独占操作通道执行单个动作。
  runAction: (action: string, payload?: Record<string, unknown>) => Promise<void>;
  // Saves the current or explicitly requested configuration scope.
  // 保存当前或明确请求的配置作用域。
  saveDraft: (targetScope?: VmmScope) => Promise<void>;
  // Selects and loads one exact project configuration route.
  // 选择并加载一个准确项目配置路由。
  selectProject: (projectId: string) => Promise<void>;
  // Selects the configuration scope and clears results from the previous scope.
  // 选择配置作用域并清除先前作用域结果。
  selectScope: (scope: VmmScope) => void;
  // Applies a partial update to the active scope draft.
  // 对当前作用域草稿应用局部更新。
  setActiveDraft: (patch: Partial<VmmConfigDraftSummary>) => void;
  // Updates the global draft while preserving dirty ownership.
  // 更新全局草稿并保留脏状态所有权。
  setGlobalDraft: React.Dispatch<React.SetStateAction<VmmConfigDraftSummary>>;
}

// ProjectDraftEntry binds one editable draft and its dirty state to an exact project id.
// ProjectDraftEntry 将单个可编辑草稿及其脏状态绑定到准确项目标识。
interface ProjectDraftEntry {
  // Editable project configuration draft.
  // 可编辑项目配置草稿。
  draft: VmmConfigDraftSummary;
  // Whether local edits must be protected from incoming snapshots.
  // 是否存在必须避免被传入快照覆盖的本地编辑。
  dirty: boolean;
}

// MemoryAreaOperation identifies the only asynchronous operation allowed to update controller state.
// MemoryAreaOperation 标识唯一允许更新控制器状态的异步操作。
interface MemoryAreaOperation {
  // User-facing action key shown by the page.
  // 页面展示的用户可见动作键。
  action: string;
  // Monotonic operation id unique to this mounted controller.
  // 当前挂载控制器内唯一的单调操作标识。
  requestId: number;
}

// UseMemoryAreaControllerOptions supplies the current snapshot and service actions.
// UseMemoryAreaControllerOptions 提供当前快照与服务动作。
interface UseMemoryAreaControllerOptions {
  // Service-backed memory actions.
  // 服务端支持的记忆动作。
  actions: MemoryActionProps;
  // Current VMM workbench snapshot.
  // 当前 VMM 工作台快照。
  snapshot: VmmMemorySnapshot;
}

// useMemoryAreaController owns VMM scope selection, exact project drafts, and async result ownership.
// useMemoryAreaController 持有 VMM 作用域选择、准确项目草稿与异步结果所有权。
// Parameters: options supplies service actions and the latest workbench VMM snapshot.
// 参数：options 提供服务动作与最新工作台 VMM 快照。
// Returns: a stable controller for rendering and mutating the memory management page.
// 返回值：用于渲染和修改记忆管理页面的稳定控制器。
export function useMemoryAreaController(options: UseMemoryAreaControllerOptions): MemoryAreaController {
  // t resolves localized operation notices.
  // t 解析本地化操作提示。
  const { t } = useI18n();
  // initialProjectId is the only project whose draft is represented by the initial snapshot.
  // initialProjectId 是初始快照草稿实际代表的唯一项目。
  const initialProjectId = memorySnapshotProjectId(options.snapshot);
  // scope stores the active configuration and action scope.
  // scope 保存当前配置与动作作用域。
  const [scope, setScopeState] = React.useState<VmmScope>("global");
  // projectId stores the project route selected by the management page.
  // projectId 保存管理页面选择的项目路由。
  const [projectId, setProjectIdState] = React.useState(initialProjectId);
  // globalDraft stores local global edits independently from incoming snapshots.
  // globalDraft 独立于传入快照保存本地全局编辑。
  const [globalDraft, setGlobalDraftState] = React.useState(options.snapshot.globalDraft);
  // projectDraftsById stores loaded and locally edited drafts under their exact project ids.
  // projectDraftsById 按准确项目标识保存已加载及本地编辑草稿。
  const [projectDraftsById, setProjectDraftsById] = React.useState<Map<string, ProjectDraftEntry>>(() => (
    initialProjectId
      ? new Map([[initialProjectId, { draft: options.snapshot.projectDraft, dirty: false }]])
      : new Map()
  ));
  // loadingProjectId identifies a selected project whose authoritative draft is still loading.
  // loadingProjectId 标识权威草稿仍在加载的当前选中项目。
  const [loadingProjectId, setLoadingProjectId] = React.useState<string | null>(null);
  // result stores the latest accepted management response.
  // result 保存最新被接受的管理响应。
  const [result, setResult] = React.useState<VmmMemoryActionResult | null>(null);
  // notice stores snapshot status or the latest locally owned operation message.
  // notice 保存快照状态或最新本地操作提示。
  const [notice, setNotice] = React.useState(options.snapshot.notice);
  // busyAction stores the operation currently holding the exclusive channel.
  // busyAction 保存当前持有独占通道的操作。
  const [busyAction, setBusyAction] = React.useState<string | null>(null);
  // scopeRef exposes the committed scope to stable async callbacks.
  // scopeRef 向稳定异步回调暴露已提交作用域。
  const scopeRef = React.useRef(scope);
  // projectIdRef exposes the committed project route to stable async callbacks.
  // projectIdRef 向稳定异步回调暴露已提交项目路由。
  const projectIdRef = React.useRef(projectId);
  // globalDraftRef owns the latest global draft before React commits a render.
  // globalDraftRef 在 React 提交渲染前持有最新全局草稿。
  const globalDraftRef = React.useRef(globalDraft);
  // globalDraftDirtyRef protects unsaved global edits from snapshot hydration.
  // globalDraftDirtyRef 保护未保存全局编辑不被快照水合覆盖。
  const globalDraftDirtyRef = React.useRef(false);
  // projectDraftsRef owns the latest per-project draft map before React commits a render.
  // projectDraftsRef 在 React 提交渲染前持有最新逐项目草稿映射。
  const projectDraftsRef = React.useRef(projectDraftsById);
  // snapshotRef exposes the latest snapshot notice and project catalog to callbacks.
  // snapshotRef 向回调暴露最新快照提示与项目目录。
  const snapshotRef = React.useRef(options.snapshot);
  // actionsRef exposes the latest service callbacks without rebuilding every controller operation.
  // actionsRef 暴露最新服务回调，避免重建每个控制器操作。
  const actionsRef = React.useRef(options.actions);
  // noticeOwnedRef prevents unrelated snapshots from replacing a local operation result.
  // noticeOwnedRef 防止无关快照替换本地操作结果。
  const noticeOwnedRef = React.useRef(false);
  // nextRequestIdRef provides collision-free operation ids.
  // nextRequestIdRef 提供无冲突操作标识。
  const nextRequestIdRef = React.useRef(0);
  // activeOperationRef synchronously excludes duplicate or overlapping management operations.
  // activeOperationRef 同步排除重复或重叠的管理操作。
  const activeOperationRef = React.useRef<MemoryAreaOperation | null>(null);

  snapshotRef.current = options.snapshot;
  actionsRef.current = options.actions;

  // replaceProjectDraftEntry commits one exact project entry to both synchronous and rendered state.
  // replaceProjectDraftEntry 将一个准确项目条目同时提交到同步状态与渲染状态。
  const replaceProjectDraftEntry = React.useCallback((targetProjectId: string, entry: ProjectDraftEntry) => {
    // nextEntries isolates the immutable React state update from the previous map.
    // nextEntries 将不可变 React 状态更新与先前映射隔离。
    const nextEntries = new Map(projectDraftsRef.current);
    nextEntries.set(targetProjectId, entry);
    projectDraftsRef.current = nextEntries;
    setProjectDraftsById(nextEntries);
  }, []);

  // invalidateActiveOperation prevents an unmounted or force-rerouted request from writing state.
  // invalidateActiveOperation 防止已卸载或被强制改路由的请求写入状态。
  const invalidateActiveOperation = React.useCallback(() => {
    activeOperationRef.current = null;
    setBusyAction(null);
  }, []);

  React.useEffect(() => {
    return () => {
      activeOperationRef.current = null;
    };
  }, []);

  React.useEffect(() => {
    // validProjectIds contains the exact active project catalog represented by this snapshot.
    // validProjectIds 包含当前快照代表的准确活动项目目录。
    const validProjectIds = new Set(options.snapshot.projectScopes.map((project) => project.projectId));
    // snapshotProjectId identifies the only project whose draft exists in this snapshot.
    // snapshotProjectId 标识当前快照中唯一实际存在草稿的项目。
    const snapshotProjectId = memorySnapshotProjectId(options.snapshot);
    if (!globalDraftDirtyRef.current) {
      globalDraftRef.current = options.snapshot.globalDraft;
      setGlobalDraftState(options.snapshot.globalDraft);
    }
    // nextProjectDrafts removes cache entries for projects no longer present in the authoritative catalog.
    // nextProjectDrafts 删除权威目录中已不存在项目的缓存条目。
    const nextProjectDrafts = new Map(
      [...projectDraftsRef.current].filter(([cachedProjectId]) => validProjectIds.has(cachedProjectId)),
    );
    if (snapshotProjectId) {
      // currentEntry protects locally dirty project edits while accepting authoritative clean updates.
      // currentEntry 在接受权威干净更新时保护本地脏项目编辑。
      const currentEntry = nextProjectDrafts.get(snapshotProjectId);
      if (!currentEntry?.dirty) {
        nextProjectDrafts.set(snapshotProjectId, {
          draft: options.snapshot.projectDraft,
          dirty: false,
        });
      }
    }
    projectDraftsRef.current = nextProjectDrafts;
    setProjectDraftsById(nextProjectDrafts);
    // currentProjectId is empty only when the workspace has no project route to invalidate.
    // currentProjectId 仅在工作区没有可失效项目路由时为空。
    const currentProjectId = projectIdRef.current;
    // projectRouteMustChange distinguishes a removed project from the stable no-project state.
    // projectRouteMustChange 区分已删除项目与稳定的无项目状态。
    const projectRouteMustChange = (Boolean(currentProjectId) && !validProjectIds.has(currentProjectId))
      || (!currentProjectId && Boolean(snapshotProjectId));
    if (projectRouteMustChange) {
      invalidateActiveOperation();
      projectIdRef.current = snapshotProjectId;
      setProjectIdState(snapshotProjectId);
      setLoadingProjectId(null);
      setResult(null);
      noticeOwnedRef.current = false;
      setNotice(options.snapshot.notice);
    } else if (!noticeOwnedRef.current
      && !activeOperationRef.current
      && (scopeRef.current === "global" || projectIdRef.current === snapshotProjectId)) {
      setNotice(options.snapshot.notice);
    }
  }, [invalidateActiveOperation, options.snapshot]);

  // beginOperation claims the exclusive channel synchronously before React can commit busy state.
  // beginOperation 在 React 提交忙碌状态前同步认领独占通道。
  const beginOperation = React.useCallback((action: string): MemoryAreaOperation | null => {
    if (activeOperationRef.current) {
      return null;
    }
    // operation is captured by every async continuation that may update controller state.
    // operation 由每个可能更新控制器状态的异步延续逻辑捕获。
    const operation = {
      action,
      requestId: nextRequestIdRef.current + 1,
    };
    nextRequestIdRef.current = operation.requestId;
    activeOperationRef.current = operation;
    setBusyAction(action);
    return operation;
  }, []);

  // ownsOperation verifies that one continuation still owns the exclusive channel.
  // ownsOperation 校验一个异步延续是否仍拥有独占通道。
  const ownsOperation = React.useCallback((operation: MemoryAreaOperation): boolean => {
    return activeOperationRef.current?.requestId === operation.requestId;
  }, []);

  // finishOperation releases only the operation that still owns busy state.
  // finishOperation 仅释放仍拥有忙碌状态的操作。
  const finishOperation = React.useCallback((operation: MemoryAreaOperation): boolean => {
    if (activeOperationRef.current?.requestId !== operation.requestId) {
      return false;
    }
    activeOperationRef.current = null;
    setBusyAction(null);
    return true;
  }, []);

  // setGlobalDraft updates the draft and marks local ownership before React renders.
  // setGlobalDraft 在 React 渲染前更新草稿并标记本地所有权。
  const setGlobalDraft = React.useCallback<React.Dispatch<React.SetStateAction<VmmConfigDraftSummary>>>((action) => {
    // nextDraft is resolved against the synchronous latest draft rather than a stale render closure.
    // nextDraft 基于同步最新草稿解析，而不是使用过期渲染闭包。
    const nextDraft = typeof action === "function" ? action(globalDraftRef.current) : action;
    globalDraftRef.current = nextDraft;
    globalDraftDirtyRef.current = true;
    setGlobalDraftState(nextDraft);
  }, []);

  // updateProjectDraft applies one local edit only to the currently committed project route.
  // updateProjectDraft 仅对当前已提交项目路由应用一次本地编辑。
  const updateProjectDraft = React.useCallback((patch: Partial<VmmConfigDraftSummary>) => {
    // targetProjectId fixes the map owner before applying the immutable patch.
    // targetProjectId 在应用不可变补丁前固定映射所有者。
    const targetProjectId = projectIdRef.current;
    // currentEntry is the only draft row allowed to receive this active-route patch.
    // currentEntry 是唯一允许接收当前路由补丁的草稿行。
    const currentEntry = projectDraftsRef.current.get(targetProjectId);
    if (!targetProjectId || !currentEntry) {
      return;
    }
    replaceProjectDraftEntry(targetProjectId, {
      draft: { ...currentEntry.draft, ...patch },
      dirty: true,
    });
  }, [replaceProjectDraftEntry]);

  // setActiveDraft routes one patch to the exact active scope draft.
  // setActiveDraft 将一个补丁路由到准确当前作用域草稿。
  const setActiveDraft = React.useCallback((patch: Partial<VmmConfigDraftSummary>) => {
    if (scopeRef.current === "global") {
      setGlobalDraft((draft) => ({ ...draft, ...patch }));
      return;
    }
    updateProjectDraft(patch);
  }, [setGlobalDraft, updateProjectDraft]);

  // selectScope commits an idle scope change and clears results owned by the previous scope.
  // selectScope 提交空闲状态下的作用域切换，并清除先前作用域拥有的结果。
  const selectScope = React.useCallback((nextScope: VmmScope) => {
    if (activeOperationRef.current || scopeRef.current === nextScope) {
      return;
    }
    scopeRef.current = nextScope;
    setScopeState(nextScope);
    setResult(null);
    if (nextScope === "project"
      && projectIdRef.current
      && projectIdRef.current !== memorySnapshotProjectId(snapshotRef.current)) {
      noticeOwnedRef.current = true;
      setNotice(t("memory.vmm.project.selected").replace(
        "{project}",
        memoryProjectLabel(snapshotRef.current, projectIdRef.current),
      ));
      return;
    }
    noticeOwnedRef.current = false;
    setNotice(snapshotRef.current.notice);
  }, [t]);

  // selectProject loads and commits the exact draft for one project route.
  // selectProject 加载并提交一个项目路由的准确草稿。
  const selectProject = React.useCallback(async (nextProjectId: string) => {
    // normalizedProjectId is the exact non-blank project key requested by the select control.
    // normalizedProjectId 是下拉控件请求的准确非空项目键。
    const normalizedProjectId = nextProjectId.trim();
    if (!normalizedProjectId || activeOperationRef.current || projectIdRef.current === normalizedProjectId) {
      return;
    }
    // projectLabel is the catalog-backed user-facing name for this exact project route.
    // projectLabel 是该准确项目路由在目录中的用户可见名称。
    const projectLabel = memoryProjectLabel(snapshotRef.current, normalizedProjectId);
    // cachedEntry preserves deliberate unsaved edits without reloading over them.
    // cachedEntry 保留明确的未保存编辑，不会重新加载并覆盖它们。
    const cachedEntry = projectDraftsRef.current.get(normalizedProjectId);
    if (cachedEntry?.dirty) {
      projectIdRef.current = normalizedProjectId;
      setProjectIdState(normalizedProjectId);
      setLoadingProjectId(null);
      setResult(null);
      noticeOwnedRef.current = true;
      setNotice(t("memory.vmm.project.selected").replace("{project}", projectLabel));
      return;
    }
    // previousProjectId is restored if the requested project's authoritative draft cannot load.
    // previousProjectId 在请求项目的权威草稿无法加载时恢复。
    const previousProjectId = projectIdRef.current;
    // operation owns project selection, loading state, and the eventual draft commit.
    // operation 拥有项目选择、加载状态与最终草稿提交。
    const operation = beginOperation("load-project-config");
    if (!operation) {
      return;
    }
    projectIdRef.current = normalizedProjectId;
    setProjectIdState(normalizedProjectId);
    setLoadingProjectId(normalizedProjectId);
    setResult(null);
    noticeOwnedRef.current = true;
    setNotice(t("memory.vmm.project.loading").replace("{project}", projectLabel));
    try {
      // draft is the authoritative editable configuration returned for normalizedProjectId.
      // draft 是为 normalizedProjectId 返回的权威可编辑配置。
      const draft = await actionsRef.current.onLoadProjectVmmConfig(normalizedProjectId);
      if (!ownsOperation(operation)) {
        return;
      }
      replaceProjectDraftEntry(normalizedProjectId, { draft, dirty: false });
      setLoadingProjectId(null);
      noticeOwnedRef.current = true;
      setNotice(t("memory.vmm.project.selected").replace("{project}", projectLabel));
    } catch (error) {
      if (!ownsOperation(operation)) {
        return;
      }
      projectIdRef.current = previousProjectId;
      setProjectIdState(previousProjectId);
      setLoadingProjectId(null);
      noticeOwnedRef.current = true;
      // message normalizes the project-load failure for the localized notice.
      // message 规范化项目加载失败以生成本地化提示。
      const message = error instanceof Error ? error.message : String(error);
      setNotice(t("memory.vmm.action.failed").replace("{message}", message));
    } finally {
      finishOperation(operation);
    }
  }, [beginOperation, finishOperation, ownsOperation, replaceProjectDraftEntry, t]);

  // runAction executes one management request against the scope captured by the exclusive operation.
  // runAction 针对独占操作捕获的作用域执行一个管理请求。
  const runAction = React.useCallback(async (action: string, payload: Record<string, unknown> = {}) => {
    // operation prevents double clicks and cross-action result overwrites.
    // operation 防止双击与跨动作结果覆盖。
    const operation = beginOperation(action);
    if (!operation) {
      return;
    }
    // requestScope fixes the configuration scope for the entire asynchronous request.
    // requestScope 为整个异步请求固定配置作用域。
    const requestScope = scopeRef.current;
    // requestProjectId fixes the project route only when project scope is active.
    // requestProjectId 仅在项目作用域激活时固定项目路由。
    const requestProjectId = requestScope === "project" ? projectIdRef.current : null;
    noticeOwnedRef.current = true;
    setNotice(t("memory.vmm.action.running").replace("{action}", action));
    try {
      // nextResult is the only response allowed to replace the visible action result.
      // nextResult 是唯一允许替换可见动作结果的响应。
      const nextResult = await actionsRef.current.onRunVmmAction({
        action,
        commandId: crypto.randomUUID(),
        scope: requestScope,
        projectId: requestProjectId,
        payload,
      });
      if (!ownsOperation(operation)) {
        return;
      }
      setResult((currentResult) => vmmResultAfterAction(currentResult, nextResult, action));
      setNotice(nextResult.summary);
    } catch (error) {
      if (!ownsOperation(operation)) {
        return;
      }
      // message normalizes the action failure for the localized notice.
      // message 规范化动作失败以生成本地化提示。
      const message = error instanceof Error ? error.message : String(error);
      setNotice(t("memory.vmm.action.failed").replace("{message}", message));
    } finally {
      finishOperation(operation);
    }
  }, [beginOperation, finishOperation, ownsOperation, t]);

  // saveDraft persists one captured scope draft through the exclusive operation channel.
  // saveDraft 通过独占操作通道持久化一个已捕获作用域草稿。
  const saveDraft = React.useCallback(async (targetScope: VmmScope = scopeRef.current) => {
    // targetProjectId captures the only project allowed to own a project save.
    // targetProjectId 捕获唯一允许拥有项目保存的项目。
    const targetProjectId = targetScope === "project" ? projectIdRef.current : null;
    // projectEntry proves that the visible project draft belongs to the captured project id.
    // projectEntry 证明可见项目草稿属于捕获的项目标识。
    const projectEntry = targetProjectId ? projectDraftsRef.current.get(targetProjectId) : null;
    // draft is the immutable payload saved even if the user continues editing during the request.
    // draft 是即使用户在请求期间继续编辑也保持不变的保存载荷。
    let draft: VmmConfigDraftSummary;
    if (targetScope === "global") {
      draft = globalDraftRef.current;
    } else {
      if (!targetProjectId || !projectEntry || loadingProjectId === targetProjectId) {
        return;
      }
      draft = projectEntry.draft;
    }
    // action distinguishes global and project save ownership in the rendered busy state.
    // action 在渲染忙碌状态中区分全局与项目保存所有权。
    const action = `save-${targetScope}`;
    // operation synchronously excludes overlapping saves and management actions.
    // operation 同步排除重叠保存与管理动作。
    const operation = beginOperation(action);
    if (!operation) {
      return;
    }
    noticeOwnedRef.current = true;
    setNotice(targetScope === "global"
      ? t("memory.vmm.save.global.running")
      : t("memory.vmm.save.project.running"));
    try {
      await actionsRef.current.onSaveVmmConfig({
        scope: targetScope,
        projectId: targetProjectId,
        draft,
      });
      if (!ownsOperation(operation)) {
        return;
      }
      if (targetScope === "global") {
        if (globalDraftRef.current === draft) {
          globalDraftDirtyRef.current = false;
        }
      } else if (targetProjectId) {
        // currentEntry detects edits made after the captured project draft started saving.
        // currentEntry 检测捕获项目草稿开始保存后产生的编辑。
        const currentEntry = projectDraftsRef.current.get(targetProjectId);
        if (currentEntry?.draft === draft) {
          replaceProjectDraftEntry(targetProjectId, { draft, dirty: false });
        }
      }
      setNotice(targetScope === "global"
        ? t("memory.vmm.save.global.completed")
        : t("memory.vmm.save.project.completed"));
    } catch (error) {
      if (!ownsOperation(operation)) {
        return;
      }
      // message normalizes the save failure for the localized notice.
      // message 规范化保存失败以生成本地化提示。
      const message = error instanceof Error ? error.message : String(error);
      setNotice(t("memory.vmm.save.failed").replace("{message}", message));
    } finally {
      finishOperation(operation);
    }
  }, [beginOperation, finishOperation, loadingProjectId, ownsOperation, replaceProjectDraftEntry, t]);

  // selectedProjectEntry is the exact cache row rendered by the current project route.
  // selectedProjectEntry 是当前项目路由渲染的准确缓存行。
  const selectedProjectEntry = projectDraftsById.get(projectId);
  // projectDraftReady prevents a previous project's draft from rendering during an exact load.
  // projectDraftReady 防止准确加载期间渲染先前项目草稿。
  const projectDraftReady = Boolean(selectedProjectEntry && loadingProjectId !== projectId);
  // projectDraft provides a required shape while projectDraftReady gates every editor and save path.
  // projectDraft 在 projectDraftReady 门禁所有编辑与保存路径时提供必需结构。
  const projectDraft = selectedProjectEntry?.draft ?? options.snapshot.projectDraft;

  return React.useMemo<MemoryAreaController>(() => ({
    busyAction,
    globalDraft,
    operationBusy: busyAction !== null,
    projectId,
    projectDraft,
    projectDraftReady,
    notice,
    result,
    scope,
    runAction,
    saveDraft,
    selectProject,
    selectScope,
    setActiveDraft,
    setGlobalDraft,
  }), [
    busyAction,
    globalDraft,
    notice,
    projectDraft,
    projectDraftReady,
    projectId,
    result,
    runAction,
    saveDraft,
    scope,
    selectProject,
    selectScope,
    setActiveDraft,
    setGlobalDraft,
  ]);
}

// memorySnapshotProjectId returns the only project whose draft is represented by one snapshot.
// memorySnapshotProjectId 返回一个快照草稿实际代表的唯一项目。
// Parameters: snapshot is the VMM workbench projection built for the selected session project.
// 参数：snapshot 是为选中会话项目构建的 VMM 工作台投影。
// Returns: the selected project id, or an empty string when no project scope exists.
// 返回值：选中项目标识；不存在项目作用域时返回空字符串。
function memorySnapshotProjectId(snapshot: VmmMemorySnapshot): string {
  return snapshot.selectedProjectId ?? "";
}

// memoryProjectLabel resolves the catalog name for one explicit project route.
// memoryProjectLabel 解析一个明确项目路由对应的目录名称。
// Parameters: snapshot owns the current project catalog and projectId identifies the requested route.
// 参数：snapshot 持有当前项目目录，projectId 标识请求路由。
// Returns: the project name, or its stable id if the project disappears during an asynchronous transition.
// 返回值：项目名称；若项目在异步过渡期间消失则返回其稳定标识。
function memoryProjectLabel(snapshot: VmmMemorySnapshot, projectId: string): string {
  // project is optional because the authoritative catalog may remove a route while its request is in flight.
  // project 为可选，因为权威目录可能在路由请求执行期间将其删除。
  const project = snapshot.projectScopes.find((candidate) => candidate.projectId === projectId);
  return project?.name ?? projectId;
}

// vmmResultAfterAction preserves search results when deletion removes visible rows.
// vmmResultAfterAction 在删除记忆移除可见行时保留当前搜索结果。
// Parameters: currentResult is the visible result, nextResult is the action response, and action identifies the operation.
// 参数：currentResult 是可见结果，nextResult 是动作响应，action 标识该操作。
// Returns: the next visible result with deleted search rows removed when applicable.
// 返回值：适用时移除已删除检索行后的下一可见结果。
function vmmResultAfterAction(
  currentResult: VmmMemoryActionResult | null,
  nextResult: VmmMemoryActionResult,
  action: string,
): VmmMemoryActionResult {
  if (action !== "delete_memories" || currentResult?.kind !== "search_memory_events") {
    return nextResult;
  }
  // deletedIds contains durable ids confirmed by the delete response.
  // deletedIds 包含删除响应确认的长期标识。
  const deletedIds = vmmDeletedMemoryIdSet(nextResult.response);
  if (deletedIds.size === 0) {
    return currentResult;
  }
  return {
    ...currentResult,
    summary: nextResult.summary,
    response: vmmSearchResultWithoutMemoryIds(currentResult.response, deletedIds),
  };
}

// vmmDeletedMemoryIdSet extracts deleted ids from one delete response.
// vmmDeletedMemoryIdSet 从删除响应中提取已删除标识集合。
// Parameters: response is the unknown VMM transport payload.
// 参数：response 是未知 VMM 传输载荷。
// Returns: the positive durable memory ids confirmed as deleted.
// 返回值：确认已删除的正数长期记忆标识。
function vmmDeletedMemoryIdSet(response: unknown): Set<number> {
  // object is the safe record projection of the transport payload.
  // object 是传输载荷的安全记录投影。
  const object = vmmObject(response);
  // values supports the two field names explicitly emitted by current transport serializers.
  // values 支持当前传输序列化器明确产生的两种字段名。
  const values = object.deletedMemoryIds ?? object.deleted_memory_ids;
  if (!Array.isArray(values)) {
    return new Set();
  }
  return new Set(values.map(vmmNumber).filter((value) => value > 0));
}

// vmmSearchResultWithoutMemoryIds removes deleted hits from one search response.
// vmmSearchResultWithoutMemoryIds 从搜索响应中移除已删除的命中项。
// Parameters: response is the current search payload and deletedIds identifies confirmed deletions.
// 参数：response 是当前检索载荷，deletedIds 标识已确认删除项。
// Returns: a copied search payload without hits backed by deleted memories.
// 返回值：移除已删除记忆命中项后的检索载荷副本。
function vmmSearchResultWithoutMemoryIds(response: unknown, deletedIds: Set<number>): unknown {
  // object is the safe search-response projection.
  // object 是安全的检索响应投影。
  const object = vmmObject(response);
  // groups contains response result groups when the transport returned an array.
  // groups 在传输返回数组时包含响应结果分组。
  const groups = Array.isArray(object.results) ? object.results : [];
  return {
    ...object,
    results: groups.map((group) => {
      // groupObject is one safe result-group projection.
      // groupObject 是单个安全结果分组投影。
      const groupObject = vmmObject(group);
      // hits contains the group's transport hit rows.
      // hits 包含当前分组的传输命中行。
      const hits = Array.isArray(groupObject.hits) ? groupObject.hits : [];
      return {
        ...groupObject,
        hits: hits.filter((hit) => !deletedIds.has(vmmNumber(vmmObject(hit).memoryId ?? vmmObject(hit).memory_id))),
      };
    }),
  };
}

// vmmObject normalizes unknown values into an object.
// vmmObject 将未知值规范化为对象。
// Parameters: value is an unknown transport value.
// 参数：value 是未知传输值。
// Returns: the record value or an empty record for non-objects.
// 返回值：记录值；非对象时返回空记录。
function vmmObject(value: unknown): Record<string, unknown> {
  return typeof value === "object" && value !== null ? value as Record<string, unknown> : {};
}

// vmmNumber normalizes unknown values into a finite number.
// vmmNumber 将未知值规范化为有限数字。
// Parameters: value is an unknown numeric transport field.
// 参数：value 是未知数值传输字段。
// Returns: a finite number or zero when the field is invalid.
// 返回值：有限数值；字段无效时返回零。
function vmmNumber(value: unknown): number {
  if (typeof value === "number" && Number.isFinite(value)) {
    return value;
  }
  if (typeof value === "string") {
    // parsed is the explicit numeric conversion of the transport string.
    // parsed 是传输字符串的明确数值转换结果。
    const parsed = Number(value);
    return Number.isFinite(parsed) ? parsed : 0;
  }
  return 0;
}
