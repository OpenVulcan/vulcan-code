import React from "react";
import { LoaderCircle, ShieldCheck } from "lucide-react";
import { toast } from "@/ui-kit/primitives/toast";
import { Button } from "@/ui-kit/primitives/button";
import { StreamPlaceholder } from "@/ui-kit/primitives/stream-placeholder";
import { Checkbox } from "@/ui-kit/primitives/checkbox";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/ui-kit/primitives/select";
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
import "@/ui-kit/styles/features/settings-forms.css"

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

// sessionLabel returns a stable human label while retaining the exact session id for actions; a project name
// shared by several projects also shows its path so the sessions can be told apart.
// sessionLabel 返回稳定的人类可读名称，同时保留动作使用的精确会话 ID；多个项目同名时附上路径以便区分会话。
function sessionLabel(session: ConversationSummary, projects: WorkbenchSnapshot["projects"]): string {
  const project = projects.find((candidate) => candidate.id === session.projectId);
  if (!project) return session.title;
  const sharedName = projects.some((candidate) => candidate.id !== project.id && candidate.name === project.name);
  return `${sharedName ? `${project.name} (${project.path})` : project.name} / ${session.title}`;
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
  const { t } = useI18n();
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
      setMutationError(t("project.grants.selectAtLeastOne"));
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
      toast.success(t("project.grants.grantSaved"));
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
      toast.success(t("project.grants.grantRevoked"));
      await loadGrants();
    } catch (error) {
      setMutationError(error instanceof Error ? error.message : String(error));
    } finally {
      setBusyGrantId(null);
    }
  };

  const sourceSession = sourceSessions.find((session) => session.id === sourceSessionId);
  const targetOptions = allSessions.filter((session) => session.id !== sourceSessionId);
  const labels = {
    title: t("project.grants.title"),
    description: t("project.grants.description"),
    source: t("project.grants.source"),
    target: t("project.grants.target"),
    capabilities: t("project.grants.capabilities"),
    save: t(pendingAdd ? "project.grants.saveRetry" : "project.grants.save"),
    loading: t("project.grants.loading"),
    empty: t("project.grants.empty"),
    retry: t("project.grants.retry"),
    revoke: t("project.grants.revoke"),
    noSource: t("project.grants.noSource"),
    noTarget: t("project.grants.noTarget"),
    pending: t("project.grants.pending"),
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
        <p className="vk-form-text">{labels.noSource}</p>
      ) : (
        <div className="vk-form-loose">
          <div className="vk-field">
            <span className="vk-field-label" id={sourceLabelId}>{labels.source}</span>
            <Select
              disabled={addBusy || busyGrantId !== null}
              onValueChange={setSourceSessionId}
              value={sourceSessionId || undefined}
            >
              <SelectTrigger aria-labelledby={sourceLabelId} className="vk-form-w-xl">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                {sourceSessions.map((session) => <SelectItem key={session.id} value={session.id}>{sessionLabel(session, props.workbench.projects)}</SelectItem>)}
              </SelectContent>
            </Select>
          </div>
          <div className="vk-field">
            <span className="vk-field-label" id={targetLabelId}>{labels.target}</span>
            <Select
              disabled={addBusy || busyGrantId !== null || targetOptions.length === 0}
              onValueChange={setTargetSessionId}
              value={targetSessionId || undefined}
            >
              <SelectTrigger aria-labelledby={targetLabelId} className="vk-form-w-xl">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                {targetOptions.map((session) => <SelectItem key={session.id} value={session.id}>{sessionLabel(session, props.workbench.projects)}</SelectItem>)}
              </SelectContent>
            </Select>
          </div>
          {targetOptions.length === 0 ? <p className="vk-form-text">{labels.noTarget}</p> : null}
          <fieldset className="vk-form-stack">
            <legend className="vk-field-label">{labels.capabilities}</legend>
            <div className="vk-form-checks">
              {GRANT_ACTIONS.map((action) => (
                <label className="vk-check-label" key={action}>
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
          <Button className="vk-form-fit" disabled={addBusy || busyGrantId !== null || !sourceSession || !targetSessionId} onClick={() => void submitGrant()} type="button">
            {addBusy ? <LoaderCircle className="vk-spin" size={14} /> : null}
            {labels.save}
          </Button>
          {pendingAdd ? <p className="vk-form-note">{labels.pending}</p> : null}
          {mutationError ? <p className="vk-field-error vk-form-break" role="alert">{mutationError}</p> : null}
          {loading ? (
            <StreamPlaceholder label={labels.loading} shape="rows" count={3} />
          ) : loadError ? (
            <div className="vk-form-stack" role="alert">
              <p className="vk-field-error vk-form-break">{loadError}</p>
              <Button disabled={addBusy || busyGrantId !== null} onClick={() => void loadGrants()} size="sm" type="button" variant="outline">{labels.retry}</Button>
            </div>
          ) : grants.length === 0 ? (
            <p className="vk-form-text">{labels.empty}</p>
          ) : (
            <div className="vk-form-stack">
              {grants.map((grant) => (
                <div className="vk-form-item" key={grant.grant_id}>
                  <div className="vk-form-fill vk-form-tight">
                    <strong className="vk-form-name">
                      {allSessions.find((session) => session.id === grant.target_session_id)
                        ? sessionLabel(allSessions.find((session) => session.id === grant.target_session_id)!, props.workbench.projects)
                        : grant.target_session_id}
                    </strong>
                    <span className="vk-field-hint">{grantActionSummary(grant, t, t("project.grants.listSeparator"))}</span>
                  </div>
                  <Button
                    disabled={addBusy || busyGrantId !== null}
                    onClick={() => void revokeGrant(grant)}
                    size="sm"
                    type="button"
                    variant="outline"
                  >
                    {busyGrantId === grant.grant_id ? <LoaderCircle className="vk-spin" size={14} /> : null}
                    {pendingRemoves[grant.grant_id] ? t("project.grants.retryRevoke") : labels.revoke}
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
