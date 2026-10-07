import React from "react";
import "@/styles/context-injection.css";
import { Brain, BrainCircuit, Database, Search, Trash2 } from "lucide-react";
import { Button } from "@/components/ui/button";
import { StreamPlaceholder } from "@/components/ui/stream-placeholder";
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
import {
  useWorkspaceMemoryRequestOwnership,
  workspaceMemoryPanelRouteKey,
} from "@/components/workspace/workspace-memory-panel-request-ownership";

// WorkspaceMemoryPanel renders WebUI-aligned dynamic and static context injection lists.
// WorkspaceMemoryPanel 渲染对齐 WebUI 的动态与静态上下文注入列表。
export function WorkspaceMemoryPanel(props: {
  // Selected-session context injection snapshot from the service.
  // 来自服务的当前选中会话上下文注入快照。
  snapshot: ContextInjectionSnapshot | null;
  // Exact persisted conversation represented by the selected-session snapshot.
  // 当前选中会话快照所代表的准确已持久化会话。
  selectedConversationId: string | null;
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
  // selectedProjectId normalizes the optional project prop for route identity and request capture.
  // selectedProjectId 规范化可选项目属性，用于路由身份与请求捕获。
  const selectedProjectId = props.selectedProjectId ?? null;
  // routeKey separates every session-scoped optimistic state and asynchronous result writer.
  // routeKey 隔离每个会话级乐观状态与异步结果写入者。
  const routeKey = workspaceMemoryPanelRouteKey(props.selectedConversationId, selectedProjectId);
  // requestOwnership synchronously arbitrates destructive actions and latest-wins lookups.
  // requestOwnership 同步仲裁破坏性动作与仅最新结果生效的查询。
  const requestOwnership = useWorkspaceMemoryRequestOwnership(routeKey);
  // stateRouteKeyRef records which route owns the current optimistic state and dialogs.
  // stateRouteKeyRef 记录当前乐观状态与弹窗所属的路由。
  const stateRouteKeyRef = React.useRef(routeKey);
  const [hiddenMemoryIds, setHiddenMemoryIds] = React.useState<Set<number>>(() => new Set());
  const [hiddenDedupeKeys, setHiddenDedupeKeys] = React.useState<Set<string>>(() => new Set());
  const [busyDeleteKey, setBusyDeleteKey] = React.useState<string | null>(null);
  const [pendingDelete, setPendingDelete] = React.useState<ContextInjectionDeleteTarget | null>(null);
  const [lookupDialog, setLookupDialog] = React.useState<ContextInjectionLookupDialogState | null>(null);
  const [deleteNotice, setDeleteNotice] = React.useState<string | null>(null);
  // activeTab loads durable profile data only when the static memory surface is opened.
  // activeTab 仅在静态记忆页面打开时加载长期画像数据。
  const [activeTab, setActiveTab] = React.useState<"dynamic" | "static">("dynamic");
  // profileRead retains only a read-only sidebar result for its exact selected route.
  // profileRead 仅为准确选中路由保留只读侧栏结果。
  const [profileRead, setProfileRead] = React.useState<{ routeKey: string; profile: VmmStaticProfileSummary | null; error: string | null } | null>(null);
  // requestedProfileRoute prevents repeated reads when a workbench snapshot rerenders unchanged.
  // requestedProfileRoute 防止工作台快照未变化但重复渲染时反复读取。
  const requestedProfileRoute = React.useRef<string | null>(null);
  // dynamicRead holds only authoritative VMM leases; transient prompt injections remain in the live snapshot.
  // dynamicRead 仅保存权威 VMM 租约；临时提示词注入仍由实时快照提供。
  const [dynamicRead, setDynamicRead] = React.useState<{ routeKey: string; snapshot: ContextInjectionSnapshot | null; cards: ContextInjectionCardSummary[]; error: string | null } | null>(null);
  // requestedDynamicRoute deduplicates the initial read for each selected conversation.
  // requestedDynamicRoute 去重每个选中会话的首次读取。
  const requestedDynamicRoute = React.useRef<{ routeKey: string; snapshot: ContextInjectionSnapshot | null } | null>(null);
  const liveDynamicCards = props.snapshot?.dynamicCards ?? [];
  const dynamicCards = dynamicRead?.routeKey === routeKey && dynamicRead.snapshot === props.snapshot
    ? [...liveDynamicCards, ...dynamicRead.cards.filter((card) => !liveDynamicCards.some((live) => live.tag === card.tag))]
    : liveDynamicCards;
  const staticCards = props.snapshot?.staticCards ?? [];
  const staticProfile = props.snapshot?.staticProfile ?? (profileRead?.routeKey === routeKey ? profileRead.profile : null);

  React.useEffect(() => {
    if (activeTab !== "dynamic" || !props.selectedConversationId || !props.onRunVmmAction || (props.snapshot?.dynamicCards.length ?? 0) > 0 || (requestedDynamicRoute.current?.routeKey === routeKey && requestedDynamicRoute.current.snapshot === props.snapshot)) {
      return;
    }
    requestedDynamicRoute.current = { routeKey, snapshot: props.snapshot };
    let cancelled = false;
    void props.onRunVmmAction({
      action: "load_dynamic_memory_for_panel",
      commandId: crypto.randomUUID(),
      scope: selectedProjectId ? "project" : "global",
      projectId: selectedProjectId,
      conversationId: props.selectedConversationId,
      payload: {},
    }).then((result) => {
      if (cancelled) return;
      if (result.data.kind !== "dynamic_memory_cards") {
        setDynamicRead({ routeKey, snapshot: props.snapshot, cards: [], error: "Unexpected dynamic memory response." });
        return;
      }
      setDynamicRead({ routeKey, snapshot: props.snapshot, cards: result.data.value, error: null });
    }).catch((error: unknown) => {
      if (cancelled) return;
      setDynamicRead({ routeKey, snapshot: props.snapshot, cards: [], error: error instanceof Error ? error.message : String(error) });
    });
    return () => { cancelled = true; };
  }, [activeTab, props.onRunVmmAction, props.selectedConversationId, props.snapshot, routeKey, selectedProjectId]);

  React.useEffect(() => {
    if (activeTab !== "static" || !props.selectedConversationId || !props.onRunVmmAction || props.snapshot?.staticProfile || requestedProfileRoute.current === routeKey) {
      return;
    }
    requestedProfileRoute.current = routeKey;
    let cancelled = false;
    void props.onRunVmmAction({
      action: "load_static_profile_for_panel",
      commandId: crypto.randomUUID(),
      scope: selectedProjectId ? "project" : "global",
      projectId: selectedProjectId,
      conversationId: props.selectedConversationId,
      payload: {},
    }).then((result) => {
      if (cancelled) return;
      if (result.data.kind !== "static_profile") {
        setProfileRead({ routeKey, profile: null, error: "Unexpected static profile response." });
        return;
      }
      setProfileRead({ routeKey, profile: result.data.value, error: null });
    }).catch((error: unknown) => {
      if (cancelled) return;
      setProfileRead({ routeKey, profile: null, error: error instanceof Error ? error.message : String(error) });
    });
    return () => { cancelled = true; };
  }, [activeTab, props.onRunVmmAction, props.selectedConversationId, props.snapshot?.staticProfile, routeKey, selectedProjectId]);

  // requestDelete starts a fresh confirmation without carrying another item's previous feedback.
  // requestDelete 开始新的确认，不携带其他条目的历史反馈。
  const requestDelete = React.useCallback((target: ContextInjectionDeleteTarget) => {
    setDeleteNotice(null);
    setPendingDelete(target);
  }, []);

  React.useLayoutEffect(() => {
    if (stateRouteKeyRef.current === routeKey) {
      return;
    }
    stateRouteKeyRef.current = routeKey;
    // Optimistic removals and dialogs describe one selected session only and must not cross routes.
    // 乐观移除与弹窗仅描述一个选中会话，不能跨路由保留。
    setHiddenMemoryIds(new Set());
    setHiddenDedupeKeys(new Set());
    setBusyDeleteKey(null);
    setPendingDelete(null);
    setLookupDialog(null);
    setDeleteNotice(null);
  }, [routeKey]);

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
    if (!props.onRunVmmAction || !props.selectedConversationId || !target.suppressibleInSession) {
      return;
    }
    // request excludes synchronous duplicate submissions and binds the result to this committed route.
    // request 排除同步重复提交，并将结果绑定到当前已提交路由。
    const request = requestOwnership.beginMutationRequest();
    if (!request) {
      return;
    }
    // conversationId captures the exact backend session that owns this visible injection.
    // conversationId 捕获拥有当前可见注入内容的准确后端会话。
    const conversationId = props.selectedConversationId;
    const deleteKey = contextInjectionDeleteKey(target);
    setBusyDeleteKey(deleteKey);
    setDeleteNotice(null);
    try {
      const result = await props.onRunVmmAction({
        action: "suppress_context_injection_item",
        commandId: crypto.randomUUID(),
        scope: selectedProjectId ? "project" : "global",
        projectId: selectedProjectId,
        conversationId,
        payload: {
          dedupeKey: target.dedupeKey,
          memoryId: target.memoryId > 0 ? target.memoryId : null,
          reason: "manual context injection suppression from memory panel",
        },
      });
      if (!requestOwnership.ownsMutationRequest(request)) {
        return;
      }
      hideDeleteTarget(target);
      setDeleteNotice(result.summary || t("workspaceMemory.suppressedNotice"));
      setPendingDelete(null);
    } catch (error) {
      if (!requestOwnership.ownsMutationRequest(request)) {
        return;
      }
      const message = error instanceof Error ? error.message : String(error);
      setDeleteNotice(t("workspaceMemory.suppressFailed").replace("{error}", message));
    } finally {
      if (requestOwnership.finishMutationRequest(request)) {
        setBusyDeleteKey(null);
      }
    }
  }, [hideDeleteTarget, props.onRunVmmAction, props.selectedConversationId, requestOwnership, selectedProjectId, t]);

  // handlePermanentDeleteTarget deletes one durable VMM memory id.
  // handlePermanentDeleteTarget 删除单个长期 VMM 记忆标识。
  const handlePermanentDeleteTarget = React.useCallback(async (target: ContextInjectionDeleteTarget) => {
    if (!props.onRunVmmAction || target.memoryId <= 0) {
      return;
    }
    // request excludes synchronous duplicate destructive actions and captures visible-route ownership.
    // request 排除同步重复破坏性动作，并捕获可见路由所有权。
    const request = requestOwnership.beginMutationRequest();
    if (!request) {
      return;
    }
    const deleteKey = contextInjectionDeleteKey(target);
    setBusyDeleteKey(deleteKey);
    setDeleteNotice(null);
    try {
      const result = await props.onRunVmmAction({
        action: "delete_memories",
        commandId: crypto.randomUUID(),
        scope: selectedProjectId ? "project" : "global",
        projectId: selectedProjectId,
        payload: {
          memoryIds: [target.memoryId],
          reason: "manual memory delete from context panel",
        },
      });
      if (!requestOwnership.ownsMutationRequest(request)) {
        return;
      }
      if (vmmDeletedMemoryIds(result.response).has(target.memoryId)) {
        hideDeleteTarget(target);
        setDeleteNotice(t("workspaceMemory.deletedNotice"));
        setPendingDelete(null);
      } else {
        setDeleteNotice(result.summary || t("workspaceMemory.deletedWithoutId"));
      }
    } catch (error) {
      if (!requestOwnership.ownsMutationRequest(request)) {
        return;
      }
      const message = error instanceof Error ? error.message : String(error);
      setDeleteNotice(t("workspaceMemory.deleteFailed").replace("{error}", message));
    } finally {
      if (requestOwnership.finishMutationRequest(request)) {
        setBusyDeleteKey(null);
      }
    }
  }, [hideDeleteTarget, props.onRunVmmAction, requestOwnership, selectedProjectId, t]);

  // handleLookupTurn loads one full VMM turn detail and opens a dialog.
  // handleLookupTurn 加载单条完整 VMM 回合详情并打开弹窗。
  const handleLookupTurn = React.useCallback(async (target: ContextInjectionLookupTarget) => {
    if (!props.onRunVmmAction || target.turnId <= 0) {
      return;
    }
    // request replaces every older lookup and binds this result to the currently committed route.
    // request 替换全部旧查询，并将当前结果绑定到已提交路由。
    const request = requestOwnership.beginLookupRequest();
    if (!request) {
      return;
    }
    setLookupDialog({ target, loading: true });
    try {
      const result = await props.onRunVmmAction({
        action: "get_turn_details",
        commandId: crypto.randomUUID(),
        scope: selectedProjectId ? "project" : "global",
        projectId: selectedProjectId,
        payload: {
          turnIds: [target.turnId],
        },
      });
      if (requestOwnership.ownsLookupRequest(request)) {
        setLookupDialog({ target, result, loading: false });
      }
    } catch (error) {
      if (requestOwnership.ownsLookupRequest(request)) {
        const message = error instanceof Error ? error.message : String(error);
        setLookupDialog({ target, error: message, loading: false });
      }
    } finally {
      requestOwnership.finishLookupRequest(request);
    }
  }, [props.onRunVmmAction, requestOwnership, selectedProjectId]);

  // handleCloseLookupDialog invalidates pending lookup ownership before removing the visible dialog.
  // handleCloseLookupDialog 在移除可见弹窗前使待处理查询所有权失效。
  const handleCloseLookupDialog = React.useCallback(() => {
    requestOwnership.invalidateLookupRequest();
    setLookupDialog(null);
  }, [requestOwnership]);

  return (
    <Tabs className="context-injection-tabs" value={activeTab} onValueChange={(value) => {
      if (value === "dynamic" && activeTab !== "dynamic") {
        // Reopening dynamic memory refreshes the persisted lease view after later turns or lifecycle changes.
        // 重新打开动态记忆时刷新持久租约视图，反映后续回合或生命周期变化。
        requestedDynamicRoute.current = null;
        setDynamicRead(null);
      }
      if (value === "static" && activeTab !== "static") {
        // Reopening the static tab explicitly refreshes its read-only view after failures or changes.
        // 重新打开静态页会在失败或内容变化后明确刷新其只读视图。
        requestedProfileRoute.current = null;
        setProfileRead(null);
      }
      setActiveTab(value === "static" ? "static" : "dynamic");
    }}>
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
          onRequestDelete={props.onRunVmmAction ? requestDelete : undefined}
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
          onRequestDelete={props.onRunVmmAction ? requestDelete : undefined}
        />
        {dynamicRead?.routeKey === routeKey && dynamicRead.snapshot === props.snapshot && dynamicRead.error ? <p className="context-injection-delete-notice">{t("workspaceMemory.lookupFailed").replace("{error}", dynamicRead.error)}</p> : null}
        {profileRead?.routeKey === routeKey && profileRead.error ? <p className="context-injection-delete-notice">{t("workspaceMemory.lookupFailed").replace("{error}", profileRead.error)}</p> : null}
        {deleteNotice ? <p className="context-injection-delete-notice">{deleteNotice}</p> : null}
      </TabsContent>
      {pendingDelete ? (
        <ContextInjectionDeleteDialog
          busy={busyDeleteKey === contextInjectionDeleteKey(pendingDelete)}
          canSuppress={Boolean(props.selectedConversationId && pendingDelete.suppressibleInSession)}
          target={pendingDelete}
          notice={deleteNotice}
          onCancel={() => setPendingDelete(null)}
          onPermanentDelete={() => void handlePermanentDeleteTarget(pendingDelete)}
          onSuppress={() => void handleSuppressDeleteTarget(pendingDelete)}
        />
      ) : null}
      {lookupDialog ? (
        <ContextInjectionLookupDialog
          state={lookupDialog}
          onClose={handleCloseLookupDialog}
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
  const { t } = useI18n();
  const sourceLabel = contextInjectionCardSourceLabel(props.card);
  // expiredGroup identifies the bottom-only VMM region retained until context compaction.
  // expiredGroup 标识仅在底部展示并保留到上下文压缩时清理的 VMM 区域。
  const expiredGroup = props.card.tag === "VMM_MEMORY_EXPIRED";
  return (
    <section className={`context-injection-card-group${expiredGroup ? " expired" : ""}`}>
        {expiredGroup ? (
          <header className="context-injection-expired-header">
            <strong>{t("workspaceMemory.expiredRegionTitle")}</strong>
            <span>{t("workspaceMemory.expiredRegionDescription")}</span>
          </header>
        ) : null}
        {props.card.items.map((item, index) => {
          const memoryId = contextInjectionMemoryId(item);
          const turnId = contextInjectionTurnId(item);
          const dedupeKey = contextInjectionDedupeKey(item);
          const scoreLabel = contextInjectionScoreLabel(item);
          const scoreTone = contextInjectionScoreTone(scoreLabel);
          const dateLabel = contextInjectionDateLabel(item);
          const deleteTarget = {
            content: item.content,
            dedupeKey,
            memoryId,
            suppressibleInSession: props.mode === "dynamic",
          };
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
                    aria-label={t("workspaceMemory.viewDetailsWithTurn").replace("{turnId}", String(turnId))}
                    className="context-injection-lookup-button"
                    onClick={() => props.onLookupTurn?.(lookupTarget)}
                    title={t("workspaceMemory.viewDetailsWithTurn").replace("{turnId}", String(turnId))}
                    type="button"
                  >
                    <Search size={14} />
                  </button>
                ) : null}
                {canDelete ? (
                  <button
                    aria-label={memoryId > 0 ? t("workspaceMemory.handleMemoryWithId").replace("{memoryId}", String(memoryId)) : t("workspaceMemory.handleDynamicMemory")}
                    className="context-injection-delete-button"
                    disabled={props.busyDeleteKey === deleteKey}
                    onClick={(event) => {
                      event.preventDefault();
                      event.stopPropagation();
                      props.onRequestDelete?.(deleteTarget);
                    }}
                    title={memoryId > 0 ? t("workspaceMemory.handleMemoryWithId").replace("{memoryId}", String(memoryId)) : t("workspaceMemory.handleDynamicMemory")}
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
    </section>
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
  // Whether this item belongs to transient injection state that supports session suppression.
  // 当前条目是否属于支持会话级抑制的瞬态注入状态。
  suppressibleInSession: boolean;
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
  // notice exposes the selected operation's outcome inside the still-open confirmation.
  // notice 在尚未关闭的确认框内展示当前操作结果。
  notice: string | null;
  // Whether the selected delete target is currently busy.
  // 当前删除目标是否正在处理中。
  busy: boolean;
  // Whether the panel owns an exact persisted session that can receive suppression.
  // 面板是否拥有可接收抑制动作的准确已持久化会话。
  canSuppress: boolean;
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
  const { t } = useI18n();
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
          <DialogTitle>{t("workspaceMemory.handleDynamicMemory")}</DialogTitle>
          <DialogDescription className="context-injection-dialog-preview">
            {props.target.content}
          </DialogDescription>
        </DialogHeader>
        {props.notice ? <p className="context-injection-delete-notice" role="alert">{props.notice}</p> : null}
        <DialogFooter className="context-injection-delete-dialog-actions">
          <Button disabled={props.busy} onClick={props.onCancel} type="button" variant="ghost">
            {t("conversation.action.cancel")}
          </Button>
          <Button disabled={props.busy || !props.canSuppress} onClick={props.onSuppress} type="button" variant="outline">
            {t("workspaceMemory.suppressForSession")}
          </Button>
          <Button
            disabled={props.busy || props.target.memoryId <= 0}
            onClick={props.onPermanentDelete}
            type="button"
            variant="destructive"
          >
            {t("workspaceMemory.permanentDeleteInvalid")}
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
  const { t } = useI18n();
  const turnEntries = contextInjectionLookupTurns(props.state.result);
  return (
    <Dialog open onOpenChange={(open) => !open && props.onClose()}>
      <DialogContent className="context-injection-lookup-dialog">
        <DialogHeader>
          <DialogTitle>{t("workspaceMemory.viewDetails")}</DialogTitle>
          <DialogDescription>{t("workspaceMemory.turnId").replace("{turnId}", String(props.state.target.turnId))}</DialogDescription>
        </DialogHeader>
        <p className="context-injection-dialog-preview">{props.state.target.content}</p>
        {props.state.loading ? <StreamPlaceholder label={t("workspaceMemory.loadingTurn")} count={3} /> : null}
        {props.state.error ? <p className="context-injection-delete-notice">{t("workspaceMemory.lookupFailed").replace("{error}", props.state.error)}</p> : null}
        {!props.state.loading && !props.state.error ? (
          turnEntries.length > 0 ? (
            <div className="context-injection-turn-list">
              {turnEntries.map((turn, index) => (
                <article className="context-injection-turn-entry" key={index}>
                  <strong>TURNID:{String(contextInjectionLookupTurnId(turn, props.state.target.turnId))}</strong>
                  <span>{t("workspaceMemory.user").replace("{content}", contextInjectionLookupText(turn, "user_question"))}</span>
                  <span>{t("workspaceMemory.assistant").replace("{content}", contextInjectionLookupText(turn, "assistant_answer"))}</span>
                  {contextInjectionLookupText(turn, "detail") ? <pre>{contextInjectionLookupText(turn, "detail")}</pre> : null}
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
  return 0;
}

// contextInjectionTurnId returns the durable VMM turn id for one visible item.
// contextInjectionTurnId 返回单条可见条目对应的长期 VMM 回合标识。
function contextInjectionTurnId(item: ContextInjectionItemSummary): number {
  if (typeof item.turnId === "number" && Number.isFinite(item.turnId)) {
    return item.turnId;
  }
  return 0;
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
  return null;
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

// contextInjectionLookupText reads one VMM turn detail text field from the wire payload.
// contextInjectionLookupText 从 VMM 回合详情 wire 载荷读取单个文本字段。
function contextInjectionLookupText(turn: Record<string, unknown>, field: "user_question" | "assistant_answer" | "detail"): string {
  const camelField = field.replace(/_([a-z])/gu, (_, letter: string) => letter.toUpperCase());
  const value = turn[field] ?? turn[camelField];
  return typeof value === "string" ? value : "";
}

// contextInjectionLookupTurnId reads the VMM turn id from the wire payload.
// contextInjectionLookupTurnId 从 VMM wire 载荷读取回合标识。
function contextInjectionLookupTurnId(turn: Record<string, unknown>, fallbackTurnId: number): number | string {
  const value = turn.turn_id ?? turn.turnId;
  if (typeof value === "number" && Number.isFinite(value)) {
    return value;
  }
  if (typeof value === "string" && value.trim()) {
    return value;
  }
  return fallbackTurnId;
}

// vmmDeletedMemoryIds extracts deleted ids from a delete action response.
// vmmDeletedMemoryIds 从删除动作响应中提取已删除标识集合。
function vmmDeletedMemoryIds(response: unknown): Set<number> {
  const object = typeof response === "object" && response !== null ? response as Record<string, unknown> : {};
  const values = object.deletedMemoryIds;
  if (!Array.isArray(values)) {
    return new Set();
  }
  return new Set(values.map(Number).filter((value) => Number.isFinite(value) && value > 0));
}
