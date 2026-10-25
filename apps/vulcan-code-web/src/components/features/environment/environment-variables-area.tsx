import * as React from "react";
import { KeyRound, Pencil, Plus, SquareTerminal, Trash2, Variable } from "lucide-react";
import "@/ui-kit/styles/features/environment-variables.css";
import { PageBody, PageEmptyState, PageGuide, PageItem, PageItemList, PageNotice, PageSection } from "@/components/page";
import { errorMessage, formatDateTime } from "@/lib/display-format";
import { formatMessage, useI18n } from "@/i18n";
import {
  deleteEnvironmentVariable,
  deleteSessionEnvironmentVariable,
  loadEnvironmentVariables,
  loadSessionEnvironment,
} from "@/service/environment-variables-service";
import type {
  ConversationSummary,
  EnvironmentVariableEntry,
  EnvironmentVariablesSnapshot,
  ProjectSummary,
  SessionEnvironmentSnapshot,
  SessionEnvironmentVariableEntry,
} from "@/types";
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/ui-kit/primitives/alert-dialog";
import { Badge } from "@/ui-kit/primitives/badge";
import { Button } from "@/ui-kit/primitives/button";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/ui-kit/primitives/select";
import { StreamPlaceholder } from "@/ui-kit/primitives/stream-placeholder";
import { toast } from "@/ui-kit/primitives/toast";
import { EnvironmentVariableDialog } from "./environment-variable-dialog";

// PendingDelete is the variable waiting for the user's delete confirmation.
// PendingDelete 是等待用户确认删除的变量。
type PendingDelete =
  | { kind: "stored"; variable: EnvironmentVariableEntry }
  | { kind: "session"; sessionId: string; variable: SessionEnvironmentVariableEntry };

// MASKED_VALUE stands in for a secret value, which the service never sends.
// MASKED_VALUE 代替服务从不发送的密钥值。
const MASKED_VALUE = "••••••";

/** Environment variable manager: global and project variables injected into commands, terminals and MCP
 * servers, and the variables one session keeps itself. Secret values are never shown.
 * 环境变量管理器：注入命令、终端与 MCP 服务的全局与项目变量，以及单个会话自身保存的变量。密钥值从不显示。 */
export function EnvironmentVariablesArea(props: {
  projects: ProjectSummary[];
  conversations: ConversationSummary[];
  selectedConversationId?: string | null;
}) {
  const { t, locale } = useI18n();
  const [snapshot, setSnapshot] = React.useState<EnvironmentVariablesSnapshot | null>(null);
  const [loadError, setLoadError] = React.useState<string | null>(null);
  const [editing, setEditing] = React.useState<EnvironmentVariableEntry | "new" | null>(null);
  const [pendingDelete, setPendingDelete] = React.useState<PendingDelete | null>(null);
  const [deleteBusy, setDeleteBusy] = React.useState(false);
  const [sessionId, setSessionId] = React.useState<string | null>(props.selectedConversationId ?? props.conversations[0]?.id ?? null);
  const [session, setSession] = React.useState<SessionEnvironmentSnapshot | null>(null);
  const [sessionError, setSessionError] = React.useState<string | null>(null);

  const load = React.useCallback(async () => {
    setLoadError(null);
    try {
      setSnapshot(await loadEnvironmentVariables());
    } catch (error) {
      setLoadError(errorMessage(error));
    }
  }, []);
  React.useEffect(() => { void load(); }, [load]);

  // The session list follows the chosen conversation; a late answer for an earlier choice is dropped.
  // 会话列表跟随所选会话；针对先前选择的迟到回答被丢弃。
  React.useEffect(() => {
    setSession(null);
    setSessionError(null);
    if (!sessionId) return;
    let current = true;
    loadSessionEnvironment(sessionId)
      .then((result) => { if (current) setSession(result); })
      .catch((error) => { if (current) setSessionError(errorMessage(error)); });
    return () => { current = false; };
  }, [sessionId]);

  const projectNames = React.useMemo(() => new Map(props.projects.map((project) => [project.id, project.name])), [props.projects]);
  const scopeLabel = (variable: EnvironmentVariableEntry) => variable.global
    ? t("environment.scope.global")
    : variable.projectIds.map((id) => projectNames.get(id) ?? id).join(", ");

  const confirmDelete = async () => {
    if (!pendingDelete) return;
    setDeleteBusy(true);
    try {
      if (pendingDelete.kind === "stored") {
        setSnapshot(await deleteEnvironmentVariable(pendingDelete.variable.id));
      } else {
        const next = await deleteSessionEnvironmentVariable(pendingDelete.sessionId, pendingDelete.variable.name);
        if (pendingDelete.sessionId === sessionId) setSession(next);
      }
      toast.success(t("environment.deleted"));
      setPendingDelete(null);
    } catch (error) {
      toast.error(t("environment.deleteFailed"), { description: errorMessage(error) });
    } finally {
      setDeleteBusy(false);
    }
  };

  if (loadError) {
    return (
      <PageBody>
        <PageNotice actions={<Button onClick={() => void load()} size="sm" type="button" variant="outline">{t("composer.capability.retry")}</Button>}
          details={loadError} detailsLabel={t("settings.errors.showErrorDetails")} tone="error">
          {t("environment.loadFailed")}
        </PageNotice>
      </PageBody>
    );
  }
  if (!snapshot) {
    return <PageBody><StreamPlaceholder count={3} label={t("environment.loading")} shape="rows" /></PageBody>;
  }
  const valueText = (kind: string, value: string) => kind === "secret" ? MASKED_VALUE : value;

  return (
    <PageBody data-environment-area="">
      <PageSection
        actions={<Button data-environment-add="" onClick={() => setEditing("new")} size="sm" type="button">
          <Plus size={14} />{t("environment.add")}
        </Button>}
        description={t("environment.stored.detail")}
        guide={<PageGuide items={[
          { term: t("environment.guide.order"), detail: t("environment.guide.orderDetail") },
          { term: t("environment.guide.prefix"), detail: formatMessage(t("environment.guide.prefixDetail"), { prefix: snapshot.rules.recommendedPrefix }) },
          { term: t("environment.guide.reserved"), detail: formatMessage(t("environment.guide.reservedDetail"), { names: snapshot.rules.reservedNames.join(", ") }) },
          { term: t("environment.guide.where"), detail: t("environment.guide.whereDetail") },
          { term: t("environment.guide.secret"), detail: t("environment.guide.secretDetail") },
        ]} />}
        status={<Badge variant="secondary">{formatMessage(t("environment.stored.count"), { count: snapshot.variables.length })}</Badge>}
        title={t("environment.stored.title")}
      >
        {snapshot.variables.length === 0 ? (
          <PageEmptyState description={t("environment.stored.emptyDetail")} title={t("environment.stored.empty")} />
        ) : (
          <PageItemList>
            {snapshot.variables.map((variable) => (
              <PageItem
                actions={<>
                  <Button aria-label={t("environment.edit")} onClick={() => setEditing(variable)} size="sm" type="button" variant="ghost">
                    <Pencil size={14} />{t("environment.edit")}
                  </Button>
                  <Button aria-label={t("environment.delete")} onClick={() => setPendingDelete({ kind: "stored", variable })} size="sm" type="button" variant="ghost">
                    <Trash2 size={14} />
                  </Button>
                </>}
                data-environment-variable={variable.name}
                description={variable.description || undefined}
                icon={variable.kind === "secret" ? <KeyRound /> : <Variable />}
                key={variable.id}
                meta={<span className="vk-env-meta">
                  <Badge variant="secondary">{variable.kind === "secret" ? t("environment.kind.secret") : t("environment.kind.plain")}</Badge>
                  <span>{scopeLabel(variable)}</span>
                  <span>{formatMessage(t("environment.updated"), { time: formatDateTime(variable.updatedAtMs, locale) ?? "" })}</span>
                </span>}
                title={<span className="vk-env-mono">{variable.name}</span>}
              >
                <code className="vk-env-value" data-secret={variable.kind === "secret" ? "" : undefined}>{valueText(variable.kind, variable.value)}</code>
              </PageItem>
            ))}
          </PageItemList>
        )}
      </PageSection>
      <PageSection
        actions={props.conversations.length > 0 ? (
          <Select onValueChange={setSessionId} value={sessionId ?? ""}>
            <SelectTrigger aria-label={t("environment.session.pick")} className="vk-env-session-pick" size="sm">
              <SelectValue placeholder={t("environment.session.pick")} />
            </SelectTrigger>
            <SelectContent>
              {props.conversations.map((conversation) => (
                <SelectItem key={conversation.id} value={conversation.id}>{conversation.title}</SelectItem>
              ))}
            </SelectContent>
          </Select>
        ) : null}
        description={t("environment.session.detail")}
        title={t("environment.session.title")}
      >
        {sessionError ? (
          <PageNotice details={sessionError} detailsLabel={t("settings.errors.showErrorDetails")} tone="error">{t("environment.session.loadFailed")}</PageNotice>
        ) : !sessionId ? (
          <PageEmptyState title={t("environment.session.none")} />
        ) : !session ? (
          <StreamPlaceholder count={2} label={t("environment.loading")} shape="rows" />
        ) : session.variables.length === 0 ? (
          <PageEmptyState description={t("environment.session.emptyDetail")} title={t("environment.session.empty")} />
        ) : (
          <PageItemList>
            {session.variables.map((variable) => (
              <PageItem
                actions={<Button aria-label={t("environment.delete")} onClick={() => setPendingDelete({ kind: "session", sessionId: session.sessionId, variable })}
                  size="sm" type="button" variant="ghost">
                  <Trash2 size={14} />
                </Button>}
                data-session-environment-variable={variable.name}
                description={variable.description || undefined}
                icon={<SquareTerminal />}
                key={variable.name}
                meta={<span className="vk-env-meta">
                  <Badge variant="secondary">{variable.kind === "secret" ? t("environment.kind.secret") : t("environment.kind.plain")}</Badge>
                  <span>{formatMessage(t("environment.updated"), { time: formatDateTime(variable.updatedAtMs, locale) ?? "" })}</span>
                </span>}
                title={<span className="vk-env-mono">{variable.name}</span>}
              >
                <code className="vk-env-value" data-secret={variable.kind === "secret" ? "" : undefined}>{valueText(variable.kind, variable.value)}</code>
              </PageItem>
            ))}
          </PageItemList>
        )}
      </PageSection>
      {editing ? (
        <EnvironmentVariableDialog
          onClose={() => setEditing(null)}
          onSaved={(saved) => { setEditing(null); setSnapshot(saved); }}
          projects={props.projects}
          rules={snapshot.rules}
          variable={editing === "new" ? undefined : editing}
        />
      ) : null}
      <AlertDialog open={pendingDelete !== null} onOpenChange={(open) => { if (!open && !deleteBusy) setPendingDelete(null); }}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>{pendingDelete ? formatMessage(t("environment.delete.title"), { name: pendingDelete.variable.name }) : ""}</AlertDialogTitle>
            <AlertDialogDescription>{pendingDelete?.kind === "session" ? t("environment.delete.sessionDetail") : t("environment.delete.detail")}</AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel disabled={deleteBusy}>{t("environment.cancel")}</AlertDialogCancel>
            <AlertDialogAction data-environment-confirm-delete="" disabled={deleteBusy} onClick={(event) => { event.preventDefault(); void confirmDelete(); }} variant="destructive">
              {t("environment.delete.confirm")}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </PageBody>
  );
}
