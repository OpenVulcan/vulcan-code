import React from "react";
import { StreamPlaceholder } from "@/components/ui/stream-placeholder";
import { BookMarked, RefreshCw, Settings2 } from "lucide-react";

import {
  DropdownMenuCheckboxItem,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuSub,
  DropdownMenuSubContent,
  DropdownMenuSubTrigger,
} from "@/components/ui/dropdown-menu";
import {
  getSessionKnowledgeLibraries,
  listKnowledgeLibraries,
  listProjectKnowledgeBindings,
  setSessionKnowledgeLibraries,
  subscribeKnowledgeProjectBindingsChanged,
} from "@/service/knowledge-base-service";
import type {
  KnowledgeLibrarySummary,
  SessionKnowledgeLibrarySelectionSummary,
} from "@/types";
import { useI18n } from "@/i18n";

// ComposerKnowledgeControlProps identifies the session and project whose knowledge selection is edited.
// ComposerKnowledgeControlProps 标识要编辑知识库选择的会话与项目。
export interface ComposerKnowledgeControlProps {
  // Owning project identifier; null disables the selector.
  // 所属项目标识符；为空时禁用选择器。
  projectId: string | null;
  // Owning conversation identifier; null disables persistence.
  // 所属会话标识符；为空时禁用持久化。
  conversationId: string | null;
  // Opens the project knowledge-library management surface.
  // 打开项目知识库管理界面。
  onOpenKnowledge: () => void;
  // Optional draft selection captured before a conversation exists.
  // 会话创建前捕获的可选草稿选择。
  draftSelection?: {
    // Inherited or explicit selection mode.
    // 继承或显式选择模式。
    mode: "inherit" | "explicit";
    // Explicit library identifiers.
    // 显式知识库标识列表。
    libraryIds: string[];
  } | null;
  // Saves one draft selection without calling a session RPC.
  // 在不调用会话 RPC 的情况下保存草稿选择。
  onDraftSelectionChange?: (
    selection: { mode: "inherit" | "explicit"; libraryIds: string[] },
  ) => void;
}

// nextExplicitKnowledgeLibraryIds derives a deduplicated explicit selection from one checkbox change.
// nextExplicitKnowledgeLibraryIds 从一次复选框变更派生去重后的显式选择。
export function nextExplicitKnowledgeLibraryIds(
  selection: SessionKnowledgeLibrarySelectionSummary | null,
  libraryId: string,
  checked: boolean,
): string[] {
  const currentIds = selection?.effectiveLibraryIds ?? [];
  return checked
    ? [...new Set([...currentIds, libraryId])]
    : currentIds.filter((candidateId) => candidateId !== libraryId);
}

// ComposerKnowledgeControl renders the session-scoped knowledge selector inside the composer plus menu.
// ComposerKnowledgeControl 在输入区加号菜单内渲染会话级知识库选择器。
export function ComposerKnowledgeControl(props: ComposerKnowledgeControlProps) {
  // t resolves session knowledge-control copy in the active WebUI language.
  // t 使用当前 WebUI 语言解析会话知识库控件文案。
  const { t } = useI18n();
  // libraries contains only libraries authorized for the active project.
  // libraries 仅包含当前项目已授权的知识库。
  const [libraries, setLibraries] = React.useState<KnowledgeLibrarySummary[]>([]);
  // selection mirrors the persisted and authorization-filtered session choice.
  // selection 对齐已持久化且经过授权过滤的会话选择。
  const [selection, setSelection] = React.useState<SessionKnowledgeLibrarySelectionSummary | null>(null);
  // draftDefaultLibraryIds contains the project defaults used by an unmaterialized inherited session.
  // draftDefaultLibraryIds 保存未物化继承会话使用的项目默认知识库标识。
  const [draftDefaultLibraryIds, setDraftDefaultLibraryIds] = React.useState<string[]>([]);
  // loading prevents edits while loading or persisting a selection.
  // loading 在加载或持久化选择期间阻止编辑。
  const [loading, setLoading] = React.useState(false);
  // error separates failed reads, failed writes and committed writes whose reload failed.
  // error 区分读取失败、写入失败及已写入但重新读取失败。
  const [error, setError] = React.useState<{ message: string; phase: "read" | "save" | "reload" } | null>(null);
  // errorExpanded keeps diagnostics secondary to the recoverable action.
  // errorExpanded 让诊断信息从属于可恢复的操作。
  const [errorExpanded, setErrorExpanded] = React.useState(false);
  // requestSequence rejects stale responses after the active conversation changes.
  // requestSequence 在活动会话变化后拒绝过期响应。
  const requestSequence = React.useRef(0);
  // mutationInFlight synchronously serializes durable session-selection writes.
  // mutationInFlight 同步串行化持久会话选择写入。
  const mutationInFlight = React.useRef(false);
  // refreshPending remembers a binding revision received while a selection write is active.
  // refreshPending 记住会话选择写入期间收到的项目绑定修订。
  const refreshPending = React.useRef(false);
  // latestRefresh always points deferred work at the callback for the currently rendered project and conversation.
  // latestRefresh 始终让延迟任务指向当前渲染项目与会话对应的刷新回调。
  const latestRefresh = React.useRef<() => Promise<void>>(async () => {});
  // retryFocusPending restores explicit retry focus after the error row is replaced by valid choices.
  // retryFocusPending 在错误行被有效选项替换后恢复显式重试动作的焦点。
  const retryFocusPending = React.useRef(false);
  // inheritItem and retryItem name the two exact focus destinations across recovery.
  // inheritItem 与 retryItem 标识恢复过程中的两个精确焦点目标。
  const inheritItem = React.useRef<HTMLDivElement>(null);
  const retryItem = React.useRef<HTMLDivElement>(null);

  React.useLayoutEffect(() => {
    if (!retryFocusPending.current || loading) return;
    retryFocusPending.current = false;
    (error ? retryItem.current : inheritItem.current)?.focus();
  }, [loading, error]);

  // Clear the previous owner's projection before paint without releasing its in-flight write lock.
  // 在绘制前清除上一归属的投影，同时保留其进行中的写入锁。
  React.useLayoutEffect(() => {
    requestSequence.current += 1;
    retryFocusPending.current = false;
    setLibraries([]);
    setSelection(null);
    setDraftDefaultLibraryIds([]);
    setError(null);
    setErrorExpanded(false);
    setLoading(Boolean(props.projectId));
  }, [props.projectId, props.conversationId]);

  // refresh reloads the authoritative authorized libraries and persisted session projection.
  // refresh 重新加载权威授权知识库与持久会话投影。
  // Parameters: none. Returns: a promise completed after the newest response is applied or discarded.
  // 参数：无。返回值：最新响应应用或丢弃后完成的 Promise。
  const refresh = React.useCallback(async (): Promise<void> => {
    if (mutationInFlight.current) {
      refreshPending.current = true;
      return;
    }
    // sequence owns this exact refresh response.
    // sequence 持有本次精确刷新响应。
    const sequence = requestSequence.current + 1;
    requestSequence.current = sequence;
    if (!props.projectId) {
      setLibraries([]);
      setSelection(null);
      setDraftDefaultLibraryIds([]);
      setError(null);
      setLoading(false);
      return;
    }
    setLoading(true);
    setErrorExpanded(false);
    try {
      if (!props.conversationId) {
        const [nextLibraries, nextBindings] = await Promise.all([
          listKnowledgeLibraries(props.projectId),
          listProjectKnowledgeBindings(props.projectId),
        ]);
        if (requestSequence.current !== sequence) return;
        setError(null);
        const defaultLibraryIds = nextBindings
          .filter((binding) => binding.defaultEnabled)
          .map((binding) => binding.libraryId);
        setLibraries(nextLibraries);
        setDraftDefaultLibraryIds(defaultLibraryIds);
        setSelection({
          mode: props.draftSelection?.mode ?? "inherit",
          libraryIds: props.draftSelection?.libraryIds ?? [],
          effectiveLibraryIds: props.draftSelection?.mode === "explicit"
            ? props.draftSelection.libraryIds
            : defaultLibraryIds,
        });
        return;
      }
      // nextLibraries and nextSelection are parallel service-authoritative projections.
      // nextLibraries 与 nextSelection 是并行取得的服务端权威投影。
      const [nextLibraries, nextSelection] = await Promise.all([
        listKnowledgeLibraries(props.projectId),
        getSessionKnowledgeLibraries(props.conversationId),
      ]);
      if (requestSequence.current !== sequence) return;
      setError(null);
      setLibraries(nextLibraries);
      setSelection(nextSelection);
    } catch (reason) {
      if (requestSequence.current !== sequence) return;
      setError({ message: String(reason), phase: "read" });
    } finally {
      if (requestSequence.current === sequence) setLoading(false);
    }
  }, [props.conversationId, props.draftSelection, props.projectId]);
  latestRefresh.current = refresh;

  React.useEffect(() => {
    void refresh();
    // unsubscribe removes the project-binding observer owned by this exact component lifetime.
    // unsubscribe 移除由本次组件生命周期持有的项目绑定观察者。
    const unsubscribe = subscribeKnowledgeProjectBindingsChanged((changedProjectId) => {
      if (changedProjectId === props.projectId) void refresh();
    });
    return () => {
      requestSequence.current += 1;
      refreshPending.current = false;
      unsubscribe();
    };
  }, [props.projectId, refresh]);

  // persist stores one exact selection and then reloads the service-authoritative projection.
  // persist 保存一份精确选择，然后重新加载服务端权威投影。
  const persist = React.useCallback(async (mode: "inherit" | "explicit", libraryIds: string[]) => {
    if (!props.projectId || loading || error || mutationInFlight.current) return;
    if (!props.conversationId) {
      if (!props.onDraftSelectionChange) return;
      const normalizedIds = mode === "inherit" ? [] : [...new Set(libraryIds)];
      props.onDraftSelectionChange?.({ mode, libraryIds: normalizedIds });
      setSelection({
        mode,
        libraryIds: normalizedIds,
        effectiveLibraryIds: mode === "inherit" ? draftDefaultLibraryIds : normalizedIds,
      });
      return;
    }
    mutationInFlight.current = true;
    const sequence = requestSequence.current + 1;
    requestSequence.current = sequence;
    setLoading(true);
    setError(null);
    setErrorExpanded(false);
    // committed prevents a failed confirmation read from being reported as a failed write.
    // committed 防止把确认读取失败误报为写入失败。
    let committed = false;
    try {
      await setSessionKnowledgeLibraries(props.projectId, props.conversationId, mode, libraryIds);
      committed = true;
      // The completed write remains valid, but an obsolete owner no longer needs a confirmation read.
      // 已完成的写入仍有效，但过期归属无需再执行确认读取。
      if (requestSequence.current !== sequence) return;
      setError(null);
      const nextSelection = await getSessionKnowledgeLibraries(props.conversationId);
      if (requestSequence.current === sequence) setSelection(nextSelection);
    } catch (reason) {
      if (requestSequence.current === sequence) setError({ message: String(reason), phase: committed ? "reload" : "save" });
    } finally {
      if (requestSequence.current === sequence) setLoading(false);
      mutationInFlight.current = false;
      if (refreshPending.current) {
        refreshPending.current = false;
        void latestRefresh.current();
      }
    }
  }, [
    props.conversationId,
    draftDefaultLibraryIds,
    props.onDraftSelectionChange,
    props.projectId,
    loading,
    error,
  ]);

  // effectiveCount is the enabled-library count shown on the knowledge submenu trigger.
  // effectiveCount 是知识库子菜单入口显示的已启用知识库数量。
  const effectiveCount = selection?.effectiveLibraryIds.length ?? 0;
  // selectionDisabled requires a current authoritative projection and an exact writable owner.
  // selectionDisabled 要求当前权威投影及精确可写归属。
  const selectionDisabled = loading || Boolean(error) || !selection || !props.projectId
    || (!props.conversationId && !props.onDraftSelectionChange);
  return (
    <DropdownMenuSub onOpenChange={(open) => {
      if (open) void refresh();
    }}>
      <DropdownMenuSubTrigger aria-label={t("knowledge.composer.aria")}>
        <BookMarked size={15} />
        <span>
          {effectiveCount > 0
            ? `${t("knowledge.composer.label")} ${effectiveCount}`
            : t("knowledge.composer.label")}
        </span>
      </DropdownMenuSubTrigger>
      <DropdownMenuSubContent className="composer-knowledge-menu w-80">
        <DropdownMenuItem onSelect={props.onOpenKnowledge}>
          <Settings2 size={15} />
          <span>{t("knowledge.composer.manage")}</span>
        </DropdownMenuItem>
        <DropdownMenuSeparator />
        <DropdownMenuCheckboxItem
          ref={inheritItem}
          checked={selection?.mode === "inherit"}
          disabled={selectionDisabled}
          onCheckedChange={(checked) => void persist(checked ? "inherit" : "explicit", checked ? [] : selection?.effectiveLibraryIds ?? [])}
          onSelect={(event) => event.preventDefault()}
        >
          {t("knowledge.composer.inherit")}
        </DropdownMenuCheckboxItem>
        <DropdownMenuSeparator />
        {loading ? <StreamPlaceholder className="px-2 py-1.5" label={t("knowledge.composer.loading")} count={2} /> : null}
        {error ? (
          <>
            <p role="alert" className="px-2 py-1.5 text-xs text-destructive">{t(`knowledge.composer.${error.phase}Failed`)}</p>
            <DropdownMenuItem ref={retryItem} disabled={loading} onSelect={(event) => {
              event.preventDefault(); retryFocusPending.current = true; void refresh();
            }}>
              <RefreshCw size={15} /><span>{t("knowledge.composer.retry")}</span>
            </DropdownMenuItem>
            <DropdownMenuItem aria-expanded={errorExpanded} onSelect={(event) => { event.preventDefault(); setErrorExpanded(!errorExpanded); }}>
              {t("knowledge.composer.details")}
            </DropdownMenuItem>
            {errorExpanded ? <pre className="max-h-32 overflow-auto whitespace-pre-wrap break-words px-2 py-1.5 text-xs text-muted-foreground">{error.message}</pre> : null}
          </>
        ) : loading && !selection ? null : libraries.length === 0 ? (
          <DropdownMenuItem disabled>{t("knowledge.composer.noneAuthorized")}</DropdownMenuItem>
        ) : (
          libraries.map((library) => {
            // checked reflects the service-authoritative effective state in both inherited and explicit modes.
            // checked 在继承与显式模式下都反映服务端权威的实际生效状态。
            const checked = selection?.effectiveLibraryIds.includes(library.id) ?? false;
            return (
              <DropdownMenuCheckboxItem
                checked={checked}
                disabled={selectionDisabled}
                key={library.id}
                onCheckedChange={(nextChecked) => {
                  const nextIds = nextExplicitKnowledgeLibraryIds(
                    selection,
                    library.id,
                    Boolean(nextChecked),
                  );
                  void persist("explicit", nextIds);
                }}
                onSelect={(event) => event.preventDefault()}
              >
                <span className="min-w-0 flex-1 truncate" title={library.name}>{library.name}</span>
              </DropdownMenuCheckboxItem>
            );
          })
        )}
      </DropdownMenuSubContent>
    </DropdownMenuSub>
  );
}
