import React from "react";
import { LoaderCircle, ShieldCheck } from "lucide-react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { useI18n, type TranslationKey } from "@/i18n";
import type { ConversationSummary, WorkbenchSnapshot } from "@/types";
import {
  addCollaborationGrant,
  createCollaborationGrantOperationId,
  listCollaborationGrants,
  removeCollaborationGrant,
  type CollaborationGrantAction,
  type CollaborationGrantRecord,
} from "@/service/collaboration-grants-service";

const GRANT_ACTIONS: CollaborationGrantAction[] = ["read", "ask", "steer", "start", "reply", "notify"];

// GrantActionSet stores the editable capability checkboxes for one new user grant.
// GrantActionSet 保存一条新用户授权的可编辑能力复选框。
type GrantActionSet = Record<CollaborationGrantAction, boolean>;

// PendingAddRequest preserves one user operation identity across network retries.
// PendingAddRequest 在网络重试之间保留一次用户操作身份。
interface PendingAddRequest {
  // sourceSessionId is the sole durable owner of the new grant.
  // sourceSessionId 是新授权的唯一可靠 owner。
  sourceSessionId: string;
  // targetSessionId is the exact session receiving the selected capabilities.
  // targetSessionId 是获得所选能力的精确目标会话。
  targetSessionId: string;
  // actions is frozen at the first click and must not change during retry.
  // actions 在首次点击时冻结，重试期间不能改变。
  actions: CollaborationGrantAction[];
  // operationId is reused when the request is retried after an uncertain response.
  // operationId 在不确定响应后的重试中复用。
  operationId: string;
}

// PendingRemoveRequest preserves one revoke operation identity across network retries.
// PendingRemoveRequest 在网络重试之间保留一次撤销操作身份。
interface PendingRemoveRequest {
  // sourceSessionId identifies the source metadata document that owns the grant.
  // sourceSessionId 标识拥有授权的来源 metadata 文档。
  sourceSessionId: string;
  // grantId identifies the exact authorization being revoked.
  // grantId 标识正在撤销的精确授权。
  grantId: string;
  // operationId is reused for the same user click and its retries.
  // operationId 在同一次用户点击及其重试中复用。
  operationId: string;
}

// actionSet creates a fully explicit checkbox state with no implicit capability fallback.
// actionSet 创建完整明确的复选框状态，不使用隐式能力兜底。
function actionSet(): GrantActionSet {
  return { read: true, ask: false, steer: false, start: false, reply: false, notify: false };
}

// sessionLabel returns a stable human label while retaining the exact session id for actions.
// sessionLabel 返回稳定的人类可读名称，同时保留动作使用的精确会话 ID。
function sessionLabel(session: ConversationSummary, projects: WorkbenchSnapshot["projects"]): string {
  const project = projects.find((candidate) => candidate.id === session.projectId);
  return project ? `${project.name} / ${session.title}` : session.title;
}

// grantActionLabel returns the localized display label for one capability.
// grantActionLabel 返回一项能力的本地化显示名称。
function grantActionLabel(action: CollaborationGrantAction, t: (key: TranslationKey) => string): string {
  // keys binds every wire action to one localized user label.
  // keys 将每个协议动作绑定到一个本地化用户标签。
  const keys: Record<CollaborationGrantAction, TranslationKey> = {
    read: "conversation.collaboration.grantAction.read",
    ask: "conversation.collaboration.grantAction.ask",
    steer: "conversation.collaboration.grantAction.steer",
    start: "conversation.collaboration.grantAction.start",
    reply: "conversation.collaboration.grantAction.reply",
    notify: "conversation.collaboration.grantAction.notify",
  };
  return t(keys[action]);
}

// grantActionSummary renders the durable capability set without exposing internal enum names.
// grantActionSummary 渲染可靠能力集合，不直接展示内部枚举名称。
function grantActionSummary(
  grant: CollaborationGrantRecord,
  t: (key: TranslationKey) => string,
  separator: string,
): string {
  // separator follows the active locale while the action order remains durable.
  // separator 遵循当前语言，同时保持可靠动作顺序。
  return grant.actions.map((action) => grantActionLabel(action, t)).join(separator);
}

// ProjectCollaborationGrantsSection renders the user-only source-owned authorization editor.
// ProjectCollaborationGrantsSection 渲染仅用户可用、来源拥有的授权编辑器。
//
// The source project is fixed by the settings dialog. The target list is project-wide so the
// service can apply its own same-project and cross-project checks before persisting a grant.
// 来源项目由设置弹窗固定。目标列表覆盖工作区会话，最终由服务校验同项目与跨项目权限。
export function ProjectCollaborationGrantsSection(props: {
  // Exact project whose visible conversations can own grants.
  // 可拥有授权的精确项目。
  projectId: string;
  // Current workbench snapshot used only to select real session identities.
  // 仅用于选择真实会话身份的当前工作台快照。
  workbench: WorkbenchSnapshot;
}) {
  const { locale, t } = useI18n();
  const isChinese = locale === "zh-CN";
  const sourceSessions = React.useMemo(
    () => props.workbench.conversations.filter((conversation) => conversation.projectId === props.projectId),
    [props.projectId, props.workbench.conversations],
  );
  const allSessions = props.workbench.conversations;
  const [sourceSessionId, setSourceSessionId] = React.useState("");
  const [targetSessionId, setTargetSessionId] = React.useState("");
  const [selectedActions, setSelectedActions] = React.useState<GrantActionSet>(actionSet);
  const [grants, setGrants] = React.useState<CollaborationGrantRecord[]>([]);
  const [loading, setLoading] = React.useState(true);
  const [loadError, setLoadError] = React.useState<string | null>(null);
  const [mutationError, setMutationError] = React.useState<string | null>(null);
  const [addBusy, setAddBusy] = React.useState(false);
  const [pendingAdd, setPendingAdd] = React.useState<PendingAddRequest | null>(null);
  const [busyGrantId, setBusyGrantId] = React.useState<string | null>(null);
  const [pendingRemoves, setPendingRemoves] = React.useState<Record<string, PendingRemoveRequest>>({});
  const loadSequenceRef = React.useRef(0);
  const pendingAddRef = React.useRef<PendingAddRequest | null>(null);
  const pendingRemovesRef = React.useRef(new Map<string, PendingRemoveRequest>());
  // sourceLabelId and targetLabelId name the shared Select triggers, which cannot sit inside a native label.
  // sourceLabelId 与 targetLabelId 为共享 Select 触发器提供名称，触发器不能放在原生 label 内。
  const sourceLabelId = React.useId();
  const targetLabelId = React.useId();

  React.useEffect(() => {
    const source = sourceSessions[0]?.id ?? "";
    setSourceSessionId((current) => sourceSessions.some((session) => session.id === current) ? current : source);
  }, [sourceSessions]);

  React.useEffect(() => {
    const target = allSessions.find((session) => session.id !== sourceSessionId)?.id ?? "";
    setTargetSessionId((current) => (
      current !== sourceSessionId && allSessions.some((session) => session.id === current) ? current : target
    ));
  }, [allSessions, sourceSessionId]);

  const loadGrants = React.useCallback(async () => {
    const sequence = loadSequenceRef.current + 1;
    loadSequenceRef.current = sequence;
    const requestedSource = sourceSessionId;
    if (!requestedSource) {
      setGrants([]);
      setLoading(false);
      setLoadError(null);
      return;
    }
    setLoading(true);
    setLoadError(null);
    try {
      const response = await listCollaborationGrants(requestedSource);
      if (loadSequenceRef.current === sequence && sourceSessionId === requestedSource) {
        setGrants(response.grants);
      }
    } catch (error) {
      if (loadSequenceRef.current === sequence && sourceSessionId === requestedSource) {
        setGrants([]);
        setLoadError(error instanceof Error ? error.message : String(error));
      }
    } finally {
      if (loadSequenceRef.current === sequence && sourceSessionId === requestedSource) {
        setLoading(false);
      }
    }
  }, [sourceSessionId]);

  React.useEffect(() => {
    pendingAddRef.current = null;
    setPendingAdd(null);
    pendingRemovesRef.current.clear();
    setPendingRemoves({});
    setMutationError(null);
    void loadGrants();
    return () => {
      loadSequenceRef.current += 1;
    };
  }, [loadGrants]);

  // submitGrant freezes the first-click payload so a retry cannot silently mutate its intent.
  // submitGrant 冻结首次点击载荷，避免重试静默改变用户意图。
  const submitGrant = async () => {
    if (addBusy || !sourceSessionId || !targetSessionId || sourceSessionId === targetSessionId) return;
    const actions = GRANT_ACTIONS.filter((action) => selectedActions[action]);
    if (actions.length === 0) {
      setMutationError(isChinese ? "至少选择一项授权能力。" : "Select at least one capability.");
      return;
    }
    const request = pendingAddRef.current ?? {
      sourceSessionId,
      targetSessionId,
      actions,
      operationId: createCollaborationGrantOperationId("add"),
    };
    pendingAddRef.current = request;
    setPendingAdd(request);
    setAddBusy(true);
    setMutationError(null);
    try {
      await addCollaborationGrant(request.sourceSessionId, request.targetSessionId, request.actions, request.operationId);
      pendingAddRef.current = null;
      setPendingAdd(null);
      setSelectedActions(actionSet());
      toast.success(isChinese ? "授权已保存。" : "Grant saved.");
      await loadGrants();
    } catch (error) {
      setMutationError(error instanceof Error ? error.message : String(error));
    } finally {
      setAddBusy(false);
    }
  };

  // revokeGrant freezes one revoke request and leaves it available for an exact retry on failure.
  // revokeGrant 冻结一次撤销请求，并在失败时保留原请求供精确重试。
  const revokeGrant = async (grant: CollaborationGrantRecord) => {
    if (busyGrantId) return;
    const request = pendingRemovesRef.current.get(grant.grant_id) ?? {
      sourceSessionId,
      grantId: grant.grant_id,
      operationId: createCollaborationGrantOperationId("remove"),
    };
    pendingRemovesRef.current.set(grant.grant_id, request);
    setPendingRemoves(Object.fromEntries(pendingRemovesRef.current));
    setBusyGrantId(grant.grant_id);
    setMutationError(null);
    try {
      await removeCollaborationGrant(request.sourceSessionId, request.grantId, request.operationId);
      pendingRemovesRef.current.delete(grant.grant_id);
      setPendingRemoves(Object.fromEntries(pendingRemovesRef.current));
      toast.success(isChinese ? "授权已撤销。" : "Grant revoked.");
      await loadGrants();
    } catch (error) {
      setMutationError(error instanceof Error ? error.message : String(error));
    } finally {
      setBusyGrantId(null);
    }
  };

  const sourceSession = sourceSessions.find((session) => session.id === sourceSessionId);
  const targetOptions = allSessions.filter((session) => session.id !== sourceSessionId);
  const labels = isChinese
    ? {
      title: "跨会话授权",
      description: "授权记录只保存在来源会话。AI 不能创建或撤销授权。",
      source: "来源会话",
      target: "目标会话",
      capabilities: "允许的能力",
      save: pendingAdd ? "重试保存授权" : "保存授权",
      loading: "正在读取授权…",
      empty: "当前来源会话没有活动授权。",
      retry: "重试读取",
      revoke: "撤销",
      noSource: "当前项目没有可授权的会话。",
      noTarget: "没有可作为目标的会话。",
      pending: "请求失败；再次点击将复用同一个操作 ID。",
    }
    : {
      title: "Cross-session grants",
      description: "Grants live only in the source session. AI cannot create or revoke them.",
      source: "Source session",
      target: "Target session",
      capabilities: "Allowed capabilities",
      save: pendingAdd ? "Retry grant" : "Save grant",
      loading: "Loading grants…",
      empty: "This source session has no active grants.",
      retry: "Retry read",
      revoke: "Revoke",
      noSource: "This project has no sessions that can own a grant.",
      noTarget: "There are no target sessions available.",
      pending: "The request failed; clicking again reuses the same operation ID.",
    };

  return (
    <div className="project-settings-section">
      <div className="project-settings-summary">
        <ShieldCheck size={22} />
        <div>
          <strong>{labels.title}</strong>
          <span>{labels.description}</span>
        </div>
      </div>
      {sourceSessions.length === 0 ? (
        <p className="text-muted-foreground">{labels.noSource}</p>
      ) : (
        <div className="space-y-4">
          <div className="space-y-1.5">
            <span className="block text-xs font-medium" id={sourceLabelId}>{labels.source}</span>
            <Select
              disabled={addBusy || busyGrantId !== null}
              onValueChange={setSourceSessionId}
              value={sourceSessionId || undefined}
            >
              <SelectTrigger aria-labelledby={sourceLabelId} className="w-full">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                {sourceSessions.map((session) => <SelectItem key={session.id} value={session.id}>{sessionLabel(session, props.workbench.projects)}</SelectItem>)}
              </SelectContent>
            </Select>
          </div>
          <div className="space-y-1.5">
            <span className="block text-xs font-medium" id={targetLabelId}>{labels.target}</span>
            <Select
              disabled={addBusy || busyGrantId !== null || targetOptions.length === 0}
              onValueChange={setTargetSessionId}
              value={targetSessionId || undefined}
            >
              <SelectTrigger aria-labelledby={targetLabelId} className="w-full">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                {targetOptions.map((session) => <SelectItem key={session.id} value={session.id}>{sessionLabel(session, props.workbench.projects)}</SelectItem>)}
              </SelectContent>
            </Select>
          </div>
          {targetOptions.length === 0 ? <p className="text-sm text-muted-foreground">{labels.noTarget}</p> : null}
          <fieldset className="space-y-2">
            <legend className="text-xs font-medium">{labels.capabilities}</legend>
            <div className="grid gap-2 sm:grid-cols-2">
              {GRANT_ACTIONS.map((action) => (
                <label className="flex items-center gap-2 text-sm" key={action}>
                  <Checkbox
                    checked={selectedActions[action]}
                    disabled={addBusy || busyGrantId !== null}
                    onCheckedChange={(checked) => setSelectedActions((current) => ({ ...current, [action]: checked === true }))}
                  />
                  <span>{grantActionLabel(action, t)}</span>
                </label>
              ))}
            </div>
          </fieldset>
          <Button disabled={addBusy || busyGrantId !== null || !sourceSession || !targetSessionId} onClick={() => void submitGrant()} type="button">
            {addBusy ? <LoaderCircle className="mr-1.5 animate-spin" size={14} /> : null}
            {labels.save}
          </Button>
          {pendingAdd ? <p className="text-xs text-muted-foreground">{labels.pending}</p> : null}
          {mutationError ? <p className="break-words text-sm text-destructive" role="alert">{mutationError}</p> : null}
          {loading ? (
            <div className="flex items-center gap-2 text-sm text-muted-foreground" role="status"><LoaderCircle className="animate-spin" size={16} /> {labels.loading}</div>
          ) : loadError ? (
            <div className="space-y-2" role="alert">
              <p className="break-words text-sm text-destructive">{loadError}</p>
              <Button disabled={addBusy || busyGrantId !== null} onClick={() => void loadGrants()} size="sm" type="button" variant="outline">{labels.retry}</Button>
            </div>
          ) : grants.length === 0 ? (
            <p className="text-sm text-muted-foreground">{labels.empty}</p>
          ) : (
            <div className="space-y-2">
              {grants.map((grant) => (
                <div className="flex items-center justify-between gap-3 rounded-md border border-border p-3" key={grant.grant_id}>
                  <div className="min-w-0 space-y-1">
                    <strong className="block truncate text-sm">
                      {allSessions.find((session) => session.id === grant.target_session_id)
                        ? sessionLabel(allSessions.find((session) => session.id === grant.target_session_id)!, props.workbench.projects)
                        : grant.target_session_id}
                    </strong>
                    <span className="block text-xs text-muted-foreground">{grantActionSummary(grant, t, isChinese ? "、" : ", ")}</span>
                  </div>
                  <Button
                    disabled={addBusy || busyGrantId !== null}
                    onClick={() => void revokeGrant(grant)}
                    size="sm"
                    type="button"
                    variant="outline"
                  >
                    {busyGrantId === grant.grant_id ? <LoaderCircle className="mr-1.5 animate-spin" size={14} /> : null}
                    {pendingRemoves[grant.grant_id] ? (isChinese ? "重试撤销" : "Retry revoke") : labels.revoke}
                  </Button>
                </div>
              ))}
            </div>
          )}
        </div>
      )}
    </div>
  );
}
