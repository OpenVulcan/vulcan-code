import React from "react";
import { BookMarked, CheckCircle2 } from "lucide-react";

import { Button } from "@/components/ui/button";
import {
  DropdownMenu,
  DropdownMenuCheckboxItem,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip";
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
import { cn } from "@/lib/utils";
import { useI18n } from "@/i18n";

// ComposerKnowledgeControlProps identifies the session and project whose knowledge selection is edited.
// ComposerKnowledgeControlProps 标识要编辑资料库选择的会话与项目。
export interface ComposerKnowledgeControlProps {
  // Owning project identifier; null disables the selector.
  // 所属项目标识符；为空时禁用选择器。
  projectId: string | null;
  // Owning conversation identifier; null disables persistence.
  // 所属会话标识符；为空时禁用持久化。
  conversationId: string | null;
}

// nextExplicitKnowledgeLibraryIds derives a deduplicated explicit selection from one checkbox change.
// nextExplicitKnowledgeLibraryIds 从一次复选框变更派生去重后的显式选择。
export function nextExplicitKnowledgeLibraryIds(
  selection: SessionKnowledgeLibrarySelectionSummary | null,
  libraryId: string,
  checked: boolean,
): string[] {
  const currentIds = selection?.mode === "explicit" ? selection.libraryIds : [];
  return checked
    ? [...new Set([...currentIds, libraryId])]
    : currentIds.filter((candidateId) => candidateId !== libraryId);
}

// ComposerKnowledgeControl renders the session-scoped knowledge selector beside the prompt controls.
// ComposerKnowledgeControl 在提示词控件旁渲染会话级资料库选择器。
export function ComposerKnowledgeControl(props: ComposerKnowledgeControlProps) {
  // t resolves session knowledge-control copy in the active WebUI language.
  // t 使用当前 WebUI 语言解析会话资料库控件文案。
  const { t } = useI18n();
  // libraries contains only libraries authorized for the active project.
  // libraries 仅包含当前项目已授权的资料库。
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
  // refresh 重新加载权威授权资料库与持久会话投影。
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

  // effectiveCount is the compact enabled-library count shown on the trigger.
  // effectiveCount 是触发器显示的紧凑已启用资料库数量。
  const effectiveCount = selection?.effectiveLibraryIds.length ?? 0;
  // disabled explains that draft conversations or unauthorized projects cannot persist a choice.
  // disabled 表示草稿会话或无授权项目无法持久化选择。
  const disabled = !props.projectId || !props.conversationId;
  return (
    <DropdownMenu onOpenChange={(open) => {
      if (open) void refresh();
    }}>
      <Tooltip>
        <TooltipTrigger asChild>
          <DropdownMenuTrigger asChild>
            <Button
              aria-label={t("knowledge.composer.aria")}
              aria-busy={loading}
              className={cn("composer-knowledge-trigger", effectiveCount > 0 && "enabled")}
              disabled={disabled}
              size="sm"
              type="button"
              variant="secondary"
            >
              <BookMarked size={15} />
              <span>{effectiveCount > 0 ? `${t("knowledge.composer.label")} ${effectiveCount}` : t("knowledge.composer.label")}</span>
            </Button>
          </DropdownMenuTrigger>
        </TooltipTrigger>
        <TooltipContent className="composer-control-tooltip" side="top">
          <strong>{error ?? (libraries.length === 0 ? t("knowledge.composer.noneAuthorized") : t("knowledge.composer.description"))}</strong>
        </TooltipContent>
      </Tooltip>
      <DropdownMenuContent align="start" className="composer-knowledge-menu" side="top" sideOffset={8}>
        <DropdownMenuItem disabled={loading} onSelect={() => void persist("inherit", [])}>
          <span>{t("knowledge.inheritProjectDefault")}</span>
          {selection?.mode === "inherit" ? <CheckCircle2 size={14} /> : null}
        </DropdownMenuItem>
        <DropdownMenuSeparator />
        {libraries.map((library) => {
          const checked = selection?.mode === "explicit" && selection.libraryIds.includes(library.id);
          return (
            <DropdownMenuCheckboxItem
              checked={checked}
              disabled={loading}
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
        })}
      </DropdownMenuContent>
    </DropdownMenu>
  );
}
