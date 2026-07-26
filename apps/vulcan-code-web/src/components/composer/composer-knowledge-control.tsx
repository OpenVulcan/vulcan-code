import React from "react";
import { BookMarked, Settings2 } from "lucide-react";

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
  // loading prevents edits while loading or persisting a selection.
  // loading 在加载或持久化选择期间阻止编辑。
  const [loading, setLoading] = React.useState(false);
  // error retains a bounded user-visible service failure.
  // error 保留有界的用户可见服务错误。
  const [error, setError] = React.useState<string | null>(null);
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
    if (!props.projectId || !props.conversationId) {
      setLibraries([]);
      setSelection(null);
      setError(null);
      setLoading(false);
      return;
    }
    setLoading(true);
    setError(null);
    try {
      // nextLibraries and nextSelection are parallel service-authoritative projections.
      // nextLibraries 与 nextSelection 是并行取得的服务端权威投影。
      const [nextLibraries, nextSelection] = await Promise.all([
        listKnowledgeLibraries(props.projectId),
        getSessionKnowledgeLibraries(props.conversationId),
      ]);
      if (requestSequence.current !== sequence) return;
      setLibraries(nextLibraries);
      setSelection(nextSelection);
    } catch (reason) {
      if (requestSequence.current !== sequence) return;
      setError(String(reason).slice(0, 240));
    } finally {
      if (requestSequence.current === sequence) setLoading(false);
    }
  }, [props.conversationId, props.projectId]);
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
      unsubscribe();
    };
  }, [props.projectId, refresh]);

  // persist stores one exact selection and then reloads the service-authoritative projection.
  // persist 保存一份精确选择，然后重新加载服务端权威投影。
  const persist = React.useCallback(async (mode: "inherit" | "explicit", libraryIds: string[]) => {
    if (!props.projectId || !props.conversationId || mutationInFlight.current) return;
    mutationInFlight.current = true;
    const sequence = requestSequence.current + 1;
    requestSequence.current = sequence;
    setLoading(true);
    setError(null);
    try {
      await setSessionKnowledgeLibraries(props.projectId, props.conversationId, mode, libraryIds);
      const nextSelection = await getSessionKnowledgeLibraries(props.conversationId);
      if (requestSequence.current === sequence) setSelection(nextSelection);
    } catch (reason) {
      if (requestSequence.current === sequence) setError(String(reason).slice(0, 240));
    } finally {
      if (requestSequence.current === sequence) setLoading(false);
      mutationInFlight.current = false;
      if (refreshPending.current) {
        refreshPending.current = false;
        void latestRefresh.current();
      }
    }
  }, [props.conversationId, props.projectId]);

  // effectiveCount is the enabled-library count shown on the knowledge submenu trigger.
  // effectiveCount 是知识库子菜单入口显示的已启用知识库数量。
  const effectiveCount = selection?.effectiveLibraryIds.length ?? 0;
  // selectionDisabled prevents persistence before a project conversation exists.
  // selectionDisabled 在项目会话存在前阻止持久化选择。
  const selectionDisabled = !props.projectId || !props.conversationId;
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
      <DropdownMenuSubContent className="composer-knowledge-menu">
        <DropdownMenuItem onSelect={props.onOpenKnowledge}>
          <Settings2 size={15} />
          <span>{t("knowledge.composer.manage")}</span>
        </DropdownMenuItem>
        <DropdownMenuSeparator />
        {error ? (
          <DropdownMenuItem disabled>{error}</DropdownMenuItem>
        ) : libraries.length === 0 ? (
          <DropdownMenuItem disabled>{t("knowledge.composer.noneAuthorized")}</DropdownMenuItem>
        ) : (
          libraries.map((library) => {
            // checked reflects the service-authoritative effective state in both inherited and explicit modes.
            // checked 在继承与显式模式下都反映服务端权威的实际生效状态。
            const checked = selection?.effectiveLibraryIds.includes(library.id) ?? false;
            return (
              <DropdownMenuCheckboxItem
                checked={checked}
                disabled={loading || selectionDisabled}
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
                <span>{library.name}</span>
              </DropdownMenuCheckboxItem>
            );
          })
        )}
      </DropdownMenuSubContent>
    </DropdownMenuSub>
  );
}
