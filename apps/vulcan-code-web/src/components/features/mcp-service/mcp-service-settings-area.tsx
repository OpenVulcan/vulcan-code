import * as React from "react";
import { Copy, KeyRound, Pencil, Plug, Plus, Trash2 } from "lucide-react";
import "@/ui-kit/styles/features/mcp-service.css";
import { PageBody, PageEmptyState, PageGuide, PageItem, PageItemList, PageNotice, PageRow, PageSection } from "@/components/page";
import { copyText } from "@/components/workspace/workbench-layout/types";
import { errorMessage, formatDateTime } from "@/lib/display-format";
import { formatMessage, useI18n } from "@/i18n";
import {
  deleteMcpClient,
  loadMcpService,
  rotateMcpClientToken,
  setMcpServiceEnabled,
} from "@/service/mcp-service-service";
import type { McpServiceClientSummary, McpServiceIssuedToken, McpServiceSnapshot, ProjectSummary } from "@/types";
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
import { StreamPlaceholder } from "@/ui-kit/primitives/stream-placeholder";
import { Switch } from "@/ui-kit/primitives/switch";
import { toast } from "@/ui-kit/primitives/toast";
import { McpClientDialog } from "./mcp-client-dialog";
import { grantSummaryParts } from "./mcp-service-model";
import { McpTokenDialog } from "./mcp-token-dialog";

// PendingConfirmation is the destructive client action waiting for the user's confirmation.
// PendingConfirmation 是等待用户确认的破坏性客户端操作。
type PendingConfirmation = { kind: "rotate" | "delete"; client: McpServiceClientSummary };

// IssuedTokenView is a token to show once, with the client it belongs to.
// IssuedTokenView 是需要展示一次的令牌及其所属客户端。
type IssuedTokenView = { clientName: string; token: string };

/** MCP service page: the endpoint switch and address, then one entry per external AI client with its grants.
 * Tokens are shown once when issued; the service keeps only their digests.
 * MCP 服务页：端点开关与地址，其后每个外部 AI 客户端一项及其授权。令牌只在签发时展示一次；服务只保留其摘要。 */
export function McpServiceSettingsArea(props: { projects: ProjectSummary[] }) {
  const { t, locale } = useI18n();
  const [snapshot, setSnapshot] = React.useState<McpServiceSnapshot | null>(null);
  const [loadError, setLoadError] = React.useState<string | null>(null);
  const [toggling, setToggling] = React.useState(false);
  const [editing, setEditing] = React.useState<McpServiceClientSummary | "new" | null>(null);
  const [confirming, setConfirming] = React.useState<PendingConfirmation | null>(null);
  const [confirmBusy, setConfirmBusy] = React.useState(false);
  const [issued, setIssued] = React.useState<IssuedTokenView | null>(null);
  const load = React.useCallback(async () => {
    setLoadError(null);
    try {
      setSnapshot(await loadMcpService());
    } catch (error) {
      setLoadError(errorMessage(error));
    }
  }, []);
  React.useEffect(() => { void load(); }, [load]);
  const toggle = async (enabled: boolean) => {
    setToggling(true);
    try {
      setSnapshot(await setMcpServiceEnabled(enabled));
    } catch (error) {
      toast.error(t("mcpService.toggleFailed"), { description: errorMessage(error) });
    } finally {
      setToggling(false);
    }
  };
  const showIssued = (result: McpServiceIssuedToken, clientName: string) => {
    setSnapshot(result.snapshot);
    setIssued({ clientName, token: result.token });
  };
  const confirm = async () => {
    if (!confirming) return;
    setConfirmBusy(true);
    try {
      if (confirming.kind === "rotate") {
        showIssued(await rotateMcpClientToken(confirming.client.id), confirming.client.name);
      } else {
        setSnapshot(await deleteMcpClient(confirming.client.id));
        toast.success(t("mcpService.deleted"));
      }
      setConfirming(null);
    } catch (error) {
      toast.error(t(confirming.kind === "rotate" ? "mcpService.rotateFailed" : "mcpService.deleteFailed"), { description: errorMessage(error) });
    } finally {
      setConfirmBusy(false);
    }
  };
  const copyEndpoint = async (url: string) => {
    try {
      await copyText(url);
      toast.success(t("mcpService.copied"));
    } catch {
      toast.error(t("mcpService.copyFailed"));
    }
  };
  if (loadError) {
    return (
      <PageBody>
        <PageNotice actions={<Button onClick={() => void load()} size="sm" type="button" variant="outline">{t("composer.capability.retry")}</Button>}
          details={loadError} detailsLabel={t("settings.errors.showErrorDetails")} tone="error">
          {t("mcpService.loadFailed")}
        </PageNotice>
      </PageBody>
    );
  }
  if (!snapshot) {
    return <PageBody><StreamPlaceholder count={3} label={t("mcpService.loading")} shape="rows" /></PageBody>;
  }
  const atLimit = snapshot.clients.length >= snapshot.limits.maxClients;
  return (
    <PageBody data-mcp-service-area="">
      <PageSection
        description={t("mcpService.endpoint.detail")}
        guide={<PageGuide items={[
          { term: t("mcpService.guide.what"), detail: t("mcpService.guide.whatDetail") },
          { term: t("mcpService.guide.approvals"), detail: t("mcpService.guide.approvalsDetail") },
          { term: t("mcpService.guide.view"), detail: t("mcpService.guide.viewDetail") },
        ]} />}
        layout="rows"
        status={<Badge variant={snapshot.enabled ? "default" : "secondary"}>{snapshot.enabled ? t("mcpService.status.on") : t("mcpService.status.off")}</Badge>}
        title={t("mcpService.endpoint.title")}
      >
        <PageRow description={t("mcpService.endpoint.enableDetail")} label={t("mcpService.endpoint.enable")}>
          <Switch aria-label={t("mcpService.endpoint.enable")} checked={snapshot.enabled} data-mcp-enabled=""
            disabled={toggling} onCheckedChange={(checked) => void toggle(checked)} />
        </PageRow>
        <PageRow description={snapshot.endpointUrl ? t("mcpService.endpoint.urlDetail") : t("mcpService.endpoint.unavailable")} label={t("mcpService.endpoint.url")}>
          {snapshot.endpointUrl ? (
            <span className="vk-mcp-endpoint">
              <code data-mcp-endpoint="">{snapshot.endpointUrl}</code>
              <Button aria-label={t("mcpService.copy")} onClick={() => void copyEndpoint(snapshot.endpointUrl!)} size="sm" type="button" variant="ghost">
                <Copy size={14} />
              </Button>
            </span>
          ) : null}
        </PageRow>
      </PageSection>
      <PageSection
        actions={<Button data-mcp-add-client="" disabled={atLimit} onClick={() => setEditing("new")} size="sm" type="button">
          <Plus size={14} />{t("mcpService.clients.add")}
        </Button>}
        description={atLimit ? formatMessage(t("mcpService.limitReached"), { max: snapshot.limits.maxClients }) : t("mcpService.clients.detail")}
        status={<Badge variant="secondary">{formatMessage(t("mcpService.clients.count"), { count: snapshot.clients.length, max: snapshot.limits.maxClients })}</Badge>}
        title={t("mcpService.clients.title")}
      >
        {snapshot.clients.length === 0 ? (
          <PageEmptyState description={t("mcpService.clients.emptyDetail")} title={t("mcpService.clients.empty")} />
        ) : (
          <PageItemList>
            {snapshot.clients.map((client) => (
              <PageItem
                actions={<>
                  <Button aria-label={t("mcpService.client.edit")} onClick={() => setEditing(client)} size="sm" type="button" variant="ghost">
                    <Pencil size={14} />{t("mcpService.client.edit")}
                  </Button>
                  <Button aria-label={t("mcpService.client.rotate")} onClick={() => setConfirming({ kind: "rotate", client })} size="sm" type="button" variant="ghost">
                    <KeyRound size={14} />{t("mcpService.client.rotate")}
                  </Button>
                  <Button aria-label={t("mcpService.client.delete")} onClick={() => setConfirming({ kind: "delete", client })} size="sm" type="button" variant="ghost">
                    <Trash2 size={14} />
                  </Button>
                </>}
                data-mcp-client={client.id}
                description={grantSummaryParts(client, t, formatMessage).join(" · ")}
                icon={<Plug />}
                key={client.id}
                meta={<span className="vk-mcp-meta">
                  {!client.enabled ? <Badge variant="secondary">{t("mcpService.client.disabled")}</Badge> : null}
                  <span>{formatMessage(t("mcpService.client.token"), { hint: client.tokenHint })}</span>
                  <span>{client.lastUsedAtMs
                    ? formatMessage(t("mcpService.client.lastUsed"), {
                      time: formatDateTime(client.lastUsedAtMs, locale) ?? "",
                      count: client.callCount,
                    })
                    : t("mcpService.client.neverUsed")}</span>
                </span>}
                title={client.name}
                tone={client.enabled ? "neutral" : "attention"}
              />
            ))}
          </PageItemList>
        )}
      </PageSection>
      {editing ? (
        <McpClientDialog
          candidates={snapshot.mediaCandidates}
          client={editing === "new" ? undefined : editing}
          limits={snapshot.limits}
          onClose={() => setEditing(null)}
          onCreated={(result, name) => { setEditing(null); showIssued(result, name); }}
          onSaved={(saved) => { setEditing(null); setSnapshot(saved); }}
          projects={props.projects}
        />
      ) : null}
      {issued ? (
        <McpTokenDialog clientName={issued.clientName} endpointUrl={snapshot.endpointUrl} onClose={() => setIssued(null)} token={issued.token} />
      ) : null}
      <AlertDialog open={confirming !== null} onOpenChange={(open) => { if (!open && !confirmBusy) setConfirming(null); }}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>{confirming ? formatMessage(t(confirming.kind === "rotate" ? "mcpService.rotate.title" : "mcpService.delete.title"), { name: confirming.client.name }) : ""}</AlertDialogTitle>
            <AlertDialogDescription>{confirming ? formatMessage(t(confirming.kind === "rotate" ? "mcpService.rotate.detail" : "mcpService.delete.detail"), { name: confirming.client.name }) : ""}</AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel disabled={confirmBusy}>{t("mcpService.cancel")}</AlertDialogCancel>
            <AlertDialogAction data-mcp-confirm="" disabled={confirmBusy} onClick={(event) => { event.preventDefault(); void confirm(); }}
              variant={confirming?.kind === "delete" ? "destructive" : "default"}>
              {confirming?.kind === "rotate" ? t("mcpService.rotate.confirm") : t("mcpService.delete.confirm")}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </PageBody>
  );
}
