import React from "react";
import { Brain, BrainCircuit, Database, Search, Trash2 } from "lucide-react";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import {
  Tabs,
  TabsContent,
  TabsList,
  TabsTrigger,
} from "@/components/ui/tabs";
import { useI18n } from "@/i18n";
import type {
  ContextInjectionCardSummary,
  ContextInjectionItemSummary,
  ContextInjectionSnapshot,
  VmmStaticProfileSummary,
  VmmMemoryActionRequest,
  VmmMemoryActionResult,
} from "@/types";

// WorkspaceMemoryPanel renders WebUI-aligned dynamic and static context injection lists.
// WorkspaceMemoryPanel 渲染对齐 WebUI 的动态与静态上下文注入列表。
export function WorkspaceMemoryPanel(props: {
  // Selected-session context injection snapshot from the service.
  // 来自服务的当前选中会话上下文注入快照。
  snapshot: ContextInjectionSnapshot | null;
  // Optional project id used to resolve project-scoped VMM configuration.
  // 用于解析项目级 VMM 配置的可选项目标识。
  selectedProjectId?: string | null;
  // Runs one VMM memory management action.
  // 执行单个 VMM 记忆管理动作。
  onRunVmmAction?: (request: VmmMemoryActionRequest) => Promise<VmmMemoryActionResult>;
}) {
  // t translates stable side-panel labels.
  // t 翻译稳定侧面板标签。
  const { t } = useI18n();
  const [hiddenMemoryIds, setHiddenMemoryIds] = React.useState<Set<number>>(() => new Set());
  const [hiddenDedupeKeys, setHiddenDedupeKeys] = React.useState<Set<string>>(() => new Set());
  const [busyDeleteKey, setBusyDeleteKey] = React.useState<string | null>(null);
  const [pendingDelete, setPendingDelete] = React.useState<ContextInjectionDeleteTarget | null>(null);
  const [lookupDialog, setLookupDialog] = React.useState<ContextInjectionLookupDialogState | null>(null);
  const [deleteNotice, setDeleteNotice] = React.useState<string | null>(null);
  const dynamicCards = props.snapshot?.dynamicCards ?? [];
  const staticCards = props.snapshot?.staticCards ?? [];
  const staticProfile = props.snapshot?.staticProfile ?? null;

  // hideDeleteTarget removes one target from the current rendered panel.
  // hideDeleteTarget 从当前渲染面板中隐藏一个目标条目。
  const hideDeleteTarget = React.useCallback((target: ContextInjectionDeleteTarget) => {
    if (target.memoryId > 0) {
      setHiddenMemoryIds((current) => new Set([...current, target.memoryId]));
    }
    const dedupeKey = target.dedupeKey;
    if (dedupeKey) {
      setHiddenDedupeKeys((current) => new Set([...current, dedupeKey]));
    }
  }, []);

  // handleSuppressDeleteTarget hides one target only for this session.
  // handleSuppressDeleteTarget 仅在本会话中隐藏一个目标条目。
  const handleSuppressDeleteTarget = React.useCallback(async (target: ContextInjectionDeleteTarget) => {
    if (!props.onRunVmmAction || busyDeleteKey != null) {
      return;
    }
    const deleteKey = contextInjectionDeleteKey(target);
    setBusyDeleteKey(deleteKey);
    setDeleteNotice(null);
    try {
      const result = await props.onRunVmmAction({
        action: "suppress_context_injection_item",
        scope: props.selectedProjectId ? "project" : "global",
        projectId: props.selectedProjectId ?? null,
        payload: {
          dedupeKey: target.dedupeKey,
          memoryId: target.memoryId > 0 ? target.memoryId : null,
          reason: "manual context injection suppression from memory panel",
        },
      });
      hideDeleteTarget(target);
      setDeleteNotice(result.summary || "已在当前会话中隐藏该动态记忆。");
      setPendingDelete(null);
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      setDeleteNotice(`隐藏失败：${message}`);
    } finally {
      setBusyDeleteKey(null);
    }
  }, [busyDeleteKey, hideDeleteTarget, props]);

  // handlePermanentDeleteTarget deletes one durable VMM memory id.
  // handlePermanentDeleteTarget 删除单个长期 VMM 记忆标识。
  const handlePermanentDeleteTarget = React.useCallback(async (target: ContextInjectionDeleteTarget) => {
    if (!props.onRunVmmAction || busyDeleteKey != null || target.memoryId <= 0) {
      return;
    }
    const deleteKey = contextInjectionDeleteKey(target);
    setBusyDeleteKey(deleteKey);
    setDeleteNotice(null);
    try {
      const result = await props.onRunVmmAction({
        action: "delete_memories",
        scope: props.selectedProjectId ? "project" : "global",
        projectId: props.selectedProjectId ?? null,
        payload: {
          memoryIds: [target.memoryId],
          reason: "manual memory delete from context panel",
        },
      });
      if (vmmDeletedMemoryIds(result.response).has(target.memoryId)) {
        hideDeleteTarget(target);
        setDeleteNotice("记忆已删除。");
        setPendingDelete(null);
      } else {
        setDeleteNotice(result.summary || "删除已执行，但服务端未返回已删除 ID。");
      }
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      setDeleteNotice(`删除失败：${message}`);
    } finally {
      setBusyDeleteKey(null);
    }
  }, [busyDeleteKey, hideDeleteTarget, props]);

  // handleLookupTurn loads one full VMM turn detail and opens a dialog.
  // handleLookupTurn 加载单条完整 VMM 回合详情并打开弹窗。
  const handleLookupTurn = React.useCallback(async (target: ContextInjectionLookupTarget) => {
    if (!props.onRunVmmAction || target.turnId <= 0) {
      return;
    }
    setLookupDialog({ target, loading: true });
    try {
      const result = await props.onRunVmmAction({
        action: "get_turn_details",
        scope: props.selectedProjectId ? "project" : "global",
        projectId: props.selectedProjectId ?? null,
        payload: {
          turnIds: [target.turnId],
        },
      });
      setLookupDialog({ target, result, loading: false });
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      setLookupDialog({ target, error: message, loading: false });
    }
  }, [props]);

  return (
    <Tabs className="context-injection-tabs" defaultValue="dynamic">
      <TabsList className="context-injection-tabs-list">
        <TabsTrigger value="dynamic">
          <BrainCircuit className="context-injection-tab-icon left-brain" size={13} />
          <span>{t("contextPanel.dynamic")}</span>
        </TabsTrigger>
        <TabsTrigger value="static">
          <Brain className="context-injection-tab-icon right-brain" size={13} />
          <span>{t("contextPanel.static")}</span>
        </TabsTrigger>
      </TabsList>
      <TabsContent className="context-injection-tab-content" value="dynamic">
        <ContextInjectionCardList
          busyDeleteKey={busyDeleteKey}
          cards={dynamicCards}
          hiddenDedupeKeys={hiddenDedupeKeys}
          hiddenMemoryIds={hiddenMemoryIds}
          mode="dynamic"
          emptyIcon={<BrainCircuit size={20} />}
          emptyText={t("contextPanel.dynamicEmpty")}
          title={t("contextPanel.dynamic")}
          onLookupTurn={props.onRunVmmAction ? handleLookupTurn : undefined}
          onRequestDelete={props.onRunVmmAction ? setPendingDelete : undefined}
        />
        {deleteNotice ? <p className="context-injection-delete-notice">{deleteNotice}</p> : null}
      </TabsContent>
      <TabsContent className="context-injection-tab-content" value="static">
        <StaticMemoryPanelContent
          busyDeleteKey={busyDeleteKey}
          cards={staticCards}
          hiddenDedupeKeys={hiddenDedupeKeys}
          hiddenMemoryIds={hiddenMemoryIds}
          emptyIcon={<Database size={20} />}
          emptyText={t("contextPanel.staticEmpty")}
          profile={staticProfile}
          title={t("contextPanel.static")}
          onLookupTurn={props.onRunVmmAction ? handleLookupTurn : undefined}
          onRequestDelete={props.onRunVmmAction ? setPendingDelete : undefined}
        />
        {deleteNotice ? <p className="context-injection-delete-notice">{deleteNotice}</p> : null}
      </TabsContent>
      {pendingDelete ? (
        <ContextInjectionDeleteDialog
          busy={busyDeleteKey === contextInjectionDeleteKey(pendingDelete)}
          target={pendingDelete}
          onCancel={() => setPendingDelete(null)}
          onPermanentDelete={() => void handlePermanentDeleteTarget(pendingDelete)}
          onSuppress={() => void handleSuppressDeleteTarget(pendingDelete)}
        />
      ) : null}
      {lookupDialog ? (
        <ContextInjectionLookupDialog
          state={lookupDialog}
          onClose={() => setLookupDialog(null)}
        />
      ) : null}
    </Tabs>
  );
}

// StaticMemoryPanelContent renders structured VMM profile data plus legacy static cards.
// StaticMemoryPanelContent 渲染结构化 VMM 画像数据与旧静态卡片。
function StaticMemoryPanelContent(props: {
  // Structured VMM profile snapshot from the backend.
  // 后端返回的结构化 VMM 画像快照。
  profile: VmmStaticProfileSummary | null;
  // Static context cards to display after the profile snapshot.
  // 画像快照后需要展示的静态上下文卡片。
  cards: ContextInjectionCardSummary[];
  // Memory ids hidden after delete or session suppression.
  // 删除或会话抑制后需要隐藏的记忆标识集合。
  hiddenMemoryIds: Set<number>;
  // Dedupe keys hidden after delete or session suppression.
  // 删除或会话抑制后需要隐藏的去重键集合。
  hiddenDedupeKeys: Set<string>;
  // Delete key currently being processed.
  // 当前正在处理的删除键。
  busyDeleteKey: string | null;
  // Empty state icon.
  // 空状态图标。
  emptyIcon: React.ReactNode;
  // Empty state title.
  // 空状态标题。
  title: string;
  // Empty state message.
  // 空状态消息。
  emptyText: string;
  // Optional memory delete request callback.
  // 可选记忆删除请求回调。
  onRequestDelete?: (target: ContextInjectionDeleteTarget) => void;
  // Optional turn lookup callback.
  // 可选回合回查回调。
  onLookupTurn?: (target: ContextInjectionLookupTarget) => void;
}) {
  const hasProfile = Boolean(props.profile?.sections.some((section) => (
    section.dates.some((date) => date.nodes.length > 0)
  )));

  if (!hasProfile) {
    return (
      <ContextInjectionCardList
        busyDeleteKey={props.busyDeleteKey}
        cards={props.cards}
        hiddenDedupeKeys={props.hiddenDedupeKeys}
        hiddenMemoryIds={props.hiddenMemoryIds}
        mode="static"
        emptyIcon={props.emptyIcon}
        emptyText={props.emptyText}
        title={props.title}
        onLookupTurn={props.onLookupTurn}
        onRequestDelete={props.onRequestDelete}
      />
    );
  }

  return (
    <div className="context-injection-card-list">
      <VmmStaticProfileView profile={props.profile as VmmStaticProfileSummary} />
      <ContextInjectionCardList
        busyDeleteKey={props.busyDeleteKey}
        cards={props.cards}
        hiddenDedupeKeys={props.hiddenDedupeKeys}
        hiddenMemoryIds={props.hiddenMemoryIds}
        mode="static"
        emptyIcon={props.emptyIcon}
        emptyText={props.emptyText}
        showEmpty={false}
        title={props.title}
        onLookupTurn={props.onLookupTurn}
        onRequestDelete={props.onRequestDelete}
      />
    </div>
  );
}

// VmmStaticProfileView renders active profile nodes grouped by target and date.
// VmmStaticProfileView 按目标和日期渲染 active 画像节点。
function VmmStaticProfileView(props: {
  // Structured VMM profile snapshot to render.
  // 需要渲染的结构化 VMM 画像快照。
  profile: VmmStaticProfileSummary;
}) {
  return (
    <div className="vmm-static-profile">
      {props.profile.sections.map((section) => (
        <section className="vmm-static-profile-section" key={section.target}>
          <div className="vmm-static-profile-section-title">
            <ContextInjectionPill label="VMM" tone="inverse" />
            <strong>{section.label}</strong>
          </div>
          {section.dates.map((date) => (
            <div className="vmm-static-profile-date-group" key={`${section.target}-${date.date}`}>
              <div className="vmm-static-profile-date">{formatVmmMemoryDateLabel(date.date)}</div>
              <div className="vmm-static-profile-node-list">
                {date.nodes.map((node) => (
                  <article className="vmm-static-profile-node" key={node.id}>
                    <div className="vmm-static-profile-node-meta">
                      <ContextInjectionPill label={node.priority} />
                      <ContextInjectionPill label={node.level} />
                      <ContextInjectionPill label={node.refreshWeight} />
                    </div>
                    <div className="vmm-static-profile-node-content">{node.content}</div>
                  </article>
                ))}
              </div>
            </div>
          ))}
        </section>
      ))}
    </div>
  );
}

// ContextInjectionCardList renders one injection card collection or its empty state.
// ContextInjectionCardList 渲染一组注入卡片或其空状态。
function ContextInjectionCardList(props: {
  // Injection cards to display.
  // 需要展示的注入卡片。
  cards: ContextInjectionCardSummary[];
  // Memory ids hidden after delete or session suppression.
  // 删除或会话抑制后需要隐藏的记忆标识集合。
  hiddenMemoryIds: Set<number>;
  // Dedupe keys hidden after delete or session suppression.
  // 删除或会话抑制后需要隐藏的去重键集合。
  hiddenDedupeKeys: Set<string>;
  // Delete key currently being processed.
  // 当前正在处理的删除键。
  busyDeleteKey: string | null;
  // Visible card mode used to adjust dynamic and static metadata.
  // 用于调整动态和静态元数据的可见卡片模式。
  mode: "dynamic" | "static";
  // Empty state icon.
  // 空状态图标。
  emptyIcon: React.ReactNode;
  // Empty state title.
  // 空状态标题。
  title: string;
  // Empty state message.
  // 空状态消息。
  emptyText: string;
  // Whether an empty state should be rendered when no cards remain.
  // 没有剩余卡片时是否渲染空状态。
  showEmpty?: boolean;
  // Optional memory delete request callback.
  // 可选记忆删除请求回调。
  onRequestDelete?: (target: ContextInjectionDeleteTarget) => void;
  // Optional turn lookup callback.
  // 可选回合回查回调。
  onLookupTurn?: (target: ContextInjectionLookupTarget) => void;
}) {
  const visibleCards = props.cards
    .map((card) => ({
      ...card,
      items: card.items.filter((item) => {
        const memoryId = contextInjectionMemoryId(item);
        const dedupeKey = contextInjectionDedupeKey(item);
        return !props.hiddenMemoryIds.has(memoryId)
          && !(dedupeKey && props.hiddenDedupeKeys.has(dedupeKey));
      }),
    }))
    .filter((card) => card.items.length > 0);

  if (visibleCards.length === 0) {
    if (props.showEmpty === false) {
      return null;
    }
    return (
      <div className="context-empty-state">
        {props.emptyIcon}
        <strong>{props.title}</strong>
        <span>{props.emptyText}</span>
      </div>
    );
  }

  return (
    <div className="context-injection-card-list">
      {visibleCards.map((card) => (
        <ContextInjectionCardView
          busyDeleteKey={props.busyDeleteKey}
          card={card}
          key={card.tag}
          mode={props.mode}
          onLookupTurn={props.onLookupTurn}
          onRequestDelete={props.onRequestDelete}
        />
      ))}
    </div>
  );
}

// ContextInjectionCardView renders one injection source group and its items.
// ContextInjectionCardView 渲染单个注入来源分组及其条目。
function ContextInjectionCardView(props: {
  // Injection source card to render.
  // 要渲染的注入来源卡片。
  card: ContextInjectionCardSummary;
  // Delete key currently being processed.
  // 当前正在处理的删除键。
  busyDeleteKey: string | null;
  // Visible card mode used to adjust dynamic and static metadata.
  // 用于调整动态和静态元数据的可见卡片模式。
  mode: "dynamic" | "static";
  // Optional memory delete request callback.
  // 可选记忆删除请求回调。
  onRequestDelete?: (target: ContextInjectionDeleteTarget) => void;
  // Optional turn lookup callback.
  // 可选回合回查回调。
  onLookupTurn?: (target: ContextInjectionLookupTarget) => void;
}) {
  const sourceLabel = contextInjectionCardSourceLabel(props.card);
  return (
    <>
        {props.card.items.map((item, index) => {
          const memoryId = contextInjectionMemoryId(item);
          const turnId = contextInjectionTurnId(item);
          const dedupeKey = contextInjectionDedupeKey(item);
          const scoreLabel = contextInjectionScoreLabel(item);
          const scoreTone = contextInjectionScoreTone(scoreLabel);
          const dateLabel = contextInjectionDateLabel(item);
          const deleteTarget = { content: item.content, dedupeKey, memoryId };
          const lookupTarget = { content: item.content, turnId };
          const deleteKey = contextInjectionDeleteKey(deleteTarget);
          const canDelete = Boolean((memoryId > 0 || dedupeKey) && props.onRequestDelete);
          const canLookup = Boolean(item.hasDialogue && turnId > 0 && props.onLookupTurn);
          return (
          <article className="context-injection-item" key={`${props.card.tag}-${index}`}>
            <div className="context-injection-item-top">
              <span className="context-injection-item-meta">
                {sourceLabel ? <ContextInjectionPill label={sourceLabel} tone="inverse" /> : null}
                {memoryId > 0 ? <ContextInjectionPill label="ID" tone="success" value={String(memoryId)} /> : null}
                {turnId > 0 ? <ContextInjectionPill label="TID" tone="success" value={String(turnId)} /> : null}
                {props.mode === "dynamic" ? <ContextInjectionPill label="TTL" value={contextInjectionTtlText(item.ttlLabel)} /> : null}
              </span>
              <span className="context-injection-item-actions">
                {canLookup ? (
                  <button
                    aria-label={`查看详情 ${turnId}`}
                    className="context-injection-lookup-button"
                    onClick={() => props.onLookupTurn?.(lookupTarget)}
                    title={`查看详情 ${turnId}`}
                    type="button"
                  >
                    <Search size={14} />
                  </button>
                ) : null}
                {canDelete ? (
                  <button
                    aria-label={memoryId > 0 ? `处理记忆 ${memoryId}` : "处理动态记忆"}
                    className="context-injection-delete-button"
                    disabled={props.busyDeleteKey === deleteKey}
                    onClick={(event) => {
                      event.preventDefault();
                      event.stopPropagation();
                      props.onRequestDelete?.(deleteTarget);
                    }}
                    title={memoryId > 0 ? `处理记忆 ${memoryId}` : "处理动态记忆"}
                    type="button"
                  >
                    <Trash2 size={14} />
                  </button>
                ) : null}
              </span>
            </div>
            <div className="context-injection-item-content">{item.content}</div>
            <div className="context-injection-item-footer">
              {dateLabel ? <ContextInjectionPill label={dateLabel} /> : null}
              {scoreLabel ? <ContextInjectionPill label={scoreLabel} tone={scoreTone} /> : null}
            </div>
          </article>
          );
        })}
    </>
  );
}

// ContextInjectionPillProps describes one compact memory metadata chip.
// ContextInjectionPillProps 描述单个紧凑记忆元数据标签。
interface ContextInjectionPillProps {
  // Label text shown inside the pill.
  // 标签内展示的文本。
  label: string;
  // Optional value rendered as a rounded count badge.
  // 以圆角计数标记展示的可选数值。
  value?: string;
  // Optional semantic color tone for the pill.
  // 标签使用的可选语义颜色。
  tone?: "default" | "inverse" | "success" | "warning";
}

// ContextInjectionPill renders one framed memory metadata label.
// ContextInjectionPill 渲染单个带框记忆元数据标签。
function ContextInjectionPill(props: ContextInjectionPillProps) {
  return (
    <span className={`context-injection-pill ${props.tone ?? "default"}`}>
      {props.value ? `${props.label}:${props.value}` : props.label}
    </span>
  );
}

// ContextInjectionDeleteTarget identifies one memory panel item selected for removal.
// ContextInjectionDeleteTarget 标识一个被选择移除的记忆面板条目。
interface ContextInjectionDeleteTarget {
  // Compact content preview displayed in the confirmation dialog.
  // 确认弹窗中展示的紧凑内容预览。
  content: string;
  // Optional stable injection dedupe key.
  // 可选稳定注入去重键。
  dedupeKey: string | null;
  // Optional durable VMM memory id.
  // 可选长期 VMM 记忆标识。
  memoryId: number;
}

// ContextInjectionLookupTarget identifies one VMM turn detail lookup request.
// ContextInjectionLookupTarget 标识一次 VMM 回合详情回查请求。
interface ContextInjectionLookupTarget {
  // Compact content preview displayed in the lookup dialog.
  // 回查弹窗中展示的紧凑内容预览。
  content: string;
  // Durable VMM source turn id.
  // 长期 VMM 来源回合标识。
  turnId: number;
}

// ContextInjectionLookupDialogState stores the current VMM lookup modal state.
// ContextInjectionLookupDialogState 保存当前 VMM 回查弹窗状态。
interface ContextInjectionLookupDialogState {
  // Target selected by the user.
  // 用户选择的目标条目。
  target: ContextInjectionLookupTarget;
  // Whether the lookup request is still running.
  // 回查请求是否仍在运行。
  loading: boolean;
  // Optional lookup result.
  // 可选回查结果。
  result?: VmmMemoryActionResult;
  // Optional lookup error message.
  // 可选回查错误信息。
  error?: string;
}

// ContextInjectionDeleteDialog asks the user which deletion mode should run.
// ContextInjectionDeleteDialog 询问用户要执行哪种删除模式。
function ContextInjectionDeleteDialog(props: {
  // Whether the selected delete target is currently busy.
  // 当前删除目标是否正在处理中。
  busy: boolean;
  // Target selected by the user.
  // 用户选择的目标条目。
  target: ContextInjectionDeleteTarget;
  // Cancels the pending delete prompt.
  // 取消待处理删除提示。
  onCancel: () => void;
  // Permanently deletes the backing VMM memory.
  // 永久删除背后的 VMM 记忆。
  onPermanentDelete: () => void;
  // Suppresses the item only for this session.
  // 仅在本会话中抑制该条目。
  onSuppress: () => void;
}) {
  return (
    <Dialog open onOpenChange={(open) => !props.busy && !open && props.onCancel()}>
      <DialogContent
        className="context-injection-delete-dialog"
        onEscapeKeyDown={(event) => {
          if (props.busy) {
            event.preventDefault();
          }
        }}
        onInteractOutside={(event) => {
          if (props.busy) {
            event.preventDefault();
          }
        }}
        showCloseButton={!props.busy}
      >
        <DialogHeader>
          <DialogTitle>处理动态记忆</DialogTitle>
          <DialogDescription className="context-injection-dialog-preview">
            {props.target.content}
          </DialogDescription>
        </DialogHeader>
        <DialogFooter className="context-injection-delete-dialog-actions">
          <Button disabled={props.busy} onClick={props.onCancel} type="button" variant="ghost">
            取消
          </Button>
          <Button disabled={props.busy} onClick={props.onSuppress} type="button" variant="outline">
            仅本会话不再注入
          </Button>
          <Button
            disabled={props.busy || props.target.memoryId <= 0}
            onClick={props.onPermanentDelete}
            type="button"
            variant="destructive"
          >
            永久删除无效记忆
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

// ContextInjectionLookupDialog shows full VMM turn details returned by the backend.
// ContextInjectionLookupDialog 展示后端返回的完整 VMM 回合详情。
function ContextInjectionLookupDialog(props: {
  // Lookup state to render.
  // 需要渲染的回查状态。
  state: ContextInjectionLookupDialogState;
  // Closes the lookup dialog.
  // 关闭回查弹窗。
  onClose: () => void;
}) {
  const turnEntries = contextInjectionLookupTurns(props.state.result);
  return (
    <Dialog open onOpenChange={(open) => !open && props.onClose()}>
      <DialogContent className="context-injection-lookup-dialog">
        <DialogHeader>
          <DialogTitle>查看详情</DialogTitle>
          <DialogDescription>对话编号 {props.state.target.turnId}</DialogDescription>
        </DialogHeader>
        <p className="context-injection-dialog-preview">{props.state.target.content}</p>
        {props.state.loading ? <p className="context-injection-delete-notice">正在查询完整 TURN 记录...</p> : null}
        {props.state.error ? <p className="context-injection-delete-notice">查询失败：{props.state.error}</p> : null}
        {!props.state.loading && !props.state.error ? (
          turnEntries.length > 0 ? (
            <div className="context-injection-turn-list">
              {turnEntries.map((turn, index) => (
                <article className="context-injection-turn-entry" key={index}>
                  <strong>TURNID:{String(turn.turnId ?? turn.turn_id ?? props.state.target.turnId)}</strong>
                  <span>用户：{String(turn.userQuestion ?? turn.user_question ?? "")}</span>
                  <span>助手：{String(turn.assistantAnswer ?? turn.assistant_answer ?? "")}</span>
                  {turn.detail ? <pre>{String(turn.detail)}</pre> : null}
                </article>
              ))}
            </div>
          ) : (
            <pre className="context-injection-lookup-json">{JSON.stringify(props.state.result?.response ?? {}, null, 2)}</pre>
          )
        ) : null}
      </DialogContent>
    </Dialog>
  );
}

// contextInjectionMemoryId returns the durable VMM memory id for one visible item.
// contextInjectionMemoryId 返回单条可见条目对应的长期 VMM 记忆标识。
function contextInjectionMemoryId(item: ContextInjectionItemSummary): number {
  if (typeof item.memoryId === "number" && Number.isFinite(item.memoryId)) {
    return item.memoryId;
  }
  const match = item.detail.match(/记忆 ID：\s*(\d+)/);
  return match ? Number(match[1]) : 0;
}

// contextInjectionTurnId returns the durable VMM turn id for one visible item.
// contextInjectionTurnId 返回单条可见条目对应的长期 VMM 回合标识。
function contextInjectionTurnId(item: ContextInjectionItemSummary): number {
  if (typeof item.turnId === "number" && Number.isFinite(item.turnId)) {
    return item.turnId;
  }
  const match = item.detail.match(/TURN_ID:(\d+)/);
  return match ? Number(match[1]) : 0;
}

// contextInjectionDedupeKey returns one stable item key when the backend provides it.
// contextInjectionDedupeKey 在后端提供时返回稳定条目键。
function contextInjectionDedupeKey(item: ContextInjectionItemSummary): string | null {
  return typeof item.dedupeKey === "string" && item.dedupeKey.trim() ? item.dedupeKey : null;
}

// contextInjectionCardSourceLabel returns the compact source marker for one card.
// contextInjectionCardSourceLabel 返回单个卡片使用的紧凑来源标记。
function contextInjectionCardSourceLabel(card: ContextInjectionCardSummary): string | null {
  return card.tag === "vmm_inject" || card.description.toLowerCase().includes("vmm")
    ? "VMM"
    : null;
}

// contextInjectionTtlText returns a compact turn-count label from the backend TTL label.
// contextInjectionTtlText 从后端 TTL 标签返回紧凑回合数标签。
function contextInjectionTtlText(ttlLabel: string): string {
  const normalized = ttlLabel.replace(/^TTL:\s*/i, "").trim();
  return normalized || ttlLabel;
}

// contextInjectionScoreLabel returns a visible relevance score label.
// contextInjectionScoreLabel 返回可见的相关度标签。
function contextInjectionScoreLabel(item: ContextInjectionItemSummary): string | null {
  if (typeof item.scoreLabel === "string" && item.scoreLabel.trim()) {
    return item.scoreLabel.trim();
  }
  const match = item.detail.match(/相关性：\s*([^\s\n]+)/u);
  return match?.[1]?.trim() || null;
}

// contextInjectionScoreTone maps a relevance score to a compact visual tone.
// contextInjectionScoreTone 将相关度分数映射为紧凑视觉色调。
function contextInjectionScoreTone(scoreLabel: string | null): ContextInjectionPillProps["tone"] {
  const score = Number.parseFloat(scoreLabel ?? "");
  if (!Number.isFinite(score)) {
    return "default";
  }
  if (score >= 0.9) {
    return "success";
  }
  if (score >= 0.8) {
    return "warning";
  }
  return "default";
}

// contextInjectionDateLabel returns a date-only label for memory metadata.
// contextInjectionDateLabel 返回记忆元数据使用的仅日期标签。
function contextInjectionDateLabel(item: ContextInjectionItemSummary): string | null {
  const source = typeof item.createdDatetime === "string" && item.createdDatetime.trim()
    ? item.createdDatetime.trim()
    : typeof item.updatedAt === "string" && item.updatedAt.trim()
      ? item.updatedAt.trim()
      : "";
  if (!source) {
    return null;
  }
  return formatVmmMemoryDateLabel(source);
}

// formatVmmMemoryDateLabel renders VMM date strings and Unix timestamps as readable local time.
// formatVmmMemoryDateLabel 将 VMM 日期字符串与 Unix 时间戳渲染为可读本地时间。
function formatVmmMemoryDateLabel(source: string): string {
  const normalized = source.trim();
  if (!normalized) {
    return source;
  }
  if (/^\d{4}-\d{2}-\d{2}$/u.test(normalized)) {
    return normalized;
  }
  const timestampMs = vmmMemoryTimestampMillis(normalized);
  if (timestampMs !== null) {
    return formatVmmMemoryDateTime(new Date(timestampMs));
  }
  const parsedMs = Date.parse(normalized);
  if (Number.isFinite(parsedMs)) {
    return formatVmmMemoryDateTime(new Date(parsedMs));
  }
  const match = normalized.match(/\d{4}-\d{2}-\d{2}/u);
  return match?.[0] ?? normalized;
}

// vmmMemoryTimestampMillis normalizes second-level and millisecond-level Unix timestamps.
// vmmMemoryTimestampMillis 归一化秒级与毫秒级 Unix 时间戳。
function vmmMemoryTimestampMillis(source: string): number | null {
  if (!/^\d{10,13}$/u.test(source)) {
    return null;
  }
  const value = Number.parseInt(source, 10);
  if (!Number.isFinite(value) || value <= 0) {
    return null;
  }
  return source.length <= 10 ? value * 1000 : value;
}

// formatVmmMemoryDateTime formats one valid Date without locale-dependent punctuation.
// formatVmmMemoryDateTime 用稳定格式渲染一个有效日期，避免依赖区域标点。
function formatVmmMemoryDateTime(date: Date): string {
  if (!Number.isFinite(date.getTime())) {
    return "";
  }
  const pad = (value: number) => String(value).padStart(2, "0");
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())} ${pad(date.getHours())}:${pad(date.getMinutes())}`;
}

// contextInjectionDeleteKey builds one local pending key for delete operations.
// contextInjectionDeleteKey 为删除操作构建本地待处理键。
function contextInjectionDeleteKey(target: ContextInjectionDeleteTarget): string {
  return target.dedupeKey ?? `memory:${target.memoryId}`;
}

// contextInjectionLookupTurns extracts turn entries from one lookup action result.
// contextInjectionLookupTurns 从一次回查动作结果中提取回合条目。
function contextInjectionLookupTurns(result: VmmMemoryActionResult | undefined): Array<Record<string, unknown>> {
  const response = typeof result?.response === "object" && result.response !== null
    ? result.response as Record<string, unknown>
    : {};
  const turns = response.turns;
  return Array.isArray(turns)
    ? turns.filter((turn): turn is Record<string, unknown> => typeof turn === "object" && turn !== null)
    : [];
}

// vmmDeletedMemoryIds extracts deleted ids from a delete action response.
// vmmDeletedMemoryIds 从删除动作响应中提取已删除标识集合。
function vmmDeletedMemoryIds(response: unknown): Set<number> {
  const object = typeof response === "object" && response !== null ? response as Record<string, unknown> : {};
  const values = object.deletedMemoryIds ?? object.deleted_memory_ids;
  if (!Array.isArray(values)) {
    return new Set();
  }
  return new Set(values.map(Number).filter((value) => Number.isFinite(value) && value > 0));
}
