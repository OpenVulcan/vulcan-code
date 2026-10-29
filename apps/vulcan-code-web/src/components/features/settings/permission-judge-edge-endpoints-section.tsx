import * as React from "react";
import { RefreshCw, Send } from "lucide-react";
import { PageEmptyState, PageNotice, PageRow, PageSection, PageTable, type PageTableRow } from "@/components/page";
import { PermissionJudgeEdgeRequestTestDialog } from "@/components/features/settings/permission-judge-edge-request-test-dialog";
import { StatusPill } from "@/components/shared/status";
import { Badge } from "@/ui-kit/primitives/badge";
import { Button } from "@/ui-kit/primitives/button";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/ui-kit/primitives/select";
import { formatMessage, type Translate, type TranslationKey } from "@/i18n";
import { errorMessage, formatDateTime } from "@/lib/display-format";
import {
  loadPermissionJudgeEdgeEndpoints,
  lockPermissionJudgeEdgeEndpoint,
  probePermissionJudgeEdgeEndpoints,
} from "@/service/permission-judge-service";
import type { PermissionJudgeEdgeEndpoint, PermissionJudgeEdgeEndpoints } from "@/types";

// AUTO is the select value of automatic selection; endpoint ids never contain "*".
// AUTO 是自动选择在下拉框中的取值；端点 id 从不包含 "*"。
const AUTO = "*";

// endpointStatus maps one endpoint's latest probe to a pill status and its label key.
// endpointStatus 将一个端点的最近探测映射为状态徽标及其标签键。
export function endpointStatus(endpoint: PermissionJudgeEdgeEndpoint): { status: string; key: TranslationKey } {
  if (endpoint.error !== null) return { status: "error", key: "settings.permissionJudge.edge.endpoints.status.failed" };
  if (endpoint.rttMs === null) return { status: "pending", key: "settings.permissionJudge.edge.endpoints.status.notProbed" };
  if (endpoint.acceptsSite === false) return { status: "warning", key: "settings.permissionJudge.edge.endpoints.status.siteNotAccepted" };
  if (endpoint.ready === false) return { status: "warning", key: "settings.permissionJudge.edge.endpoints.status.notReady" };
  return { status: "ready", key: "settings.permissionJudge.edge.endpoints.status.available" };
}

// PermissionJudgeEdgeEndpointsSection shows the endpoints of the saved cloud model with
// their latest background probe, marks the endpoint judge requests use now, and lets the user lock
// one endpoint, re-test now, or send real requests to one endpoint. It reads the in-memory table at
// the status poll interval.
// PermissionJudgeEdgeEndpointsSection 显示已保存云端模型的端点及其最近一次后台探测，标出判定请求
// 当前使用的端点，并允许用户锁定端点、立即重新测试或向某个端点发送真实请求。它按状态轮询间隔读取内存
// 端点表。
// Parameters: the poll interval, whether controls are locked, the translator and the locale.
// Returns the section.
// 参数：轮询间隔、控件是否锁定、翻译函数与语言。返回该区块。
export function PermissionJudgeEdgeEndpointsSection(props: {
  pollIntervalMs: number | undefined;
  disabled: boolean;
  t: Translate;
  locale: string;
}) {
  const { t, locale, pollIntervalMs } = props;
  const [table, setTable] = React.useState<PermissionJudgeEdgeEndpoints | null>(null);
  const [loadError, setLoadError] = React.useState<string | null>(null);
  const [actionError, setActionError] = React.useState<string | null>(null);
  const [busy, setBusy] = React.useState<"probe" | "lock" | null>(null);
  const [testing, setTesting] = React.useState<string | null>(null);

  React.useEffect(() => {
    let cancelled = false;
    const read = () => {
      void loadPermissionJudgeEdgeEndpoints().then((next) => {
        if (!cancelled) { setTable(next); setLoadError(null); }
      }).catch((cause: unknown) => { if (!cancelled) setLoadError(errorMessage(cause)); });
    };
    read();
    if (pollIntervalMs === undefined) return () => { cancelled = true; };
    const timer = window.setInterval(read, pollIntervalMs);
    return () => { cancelled = true; window.clearInterval(timer); };
  }, [pollIntervalMs]);

  const run = async (kind: "probe" | "lock", action: () => Promise<PermissionJudgeEdgeEndpoints>) => {
    setBusy(kind);
    setActionError(null);
    try {
      setTable(await action());
    } catch (cause) {
      setActionError(errorMessage(cause));
    } finally {
      setBusy(null);
    }
  };

  const ms = (value: number | null) => value === null ? "—"
    : formatMessage(t("settings.permissionJudge.edge.endpoints.ms"), { value });
  const time = (value: number | null) => value === null
    ? t("settings.permissionJudge.edge.endpoints.never")
    : formatDateTime(value, locale) ?? "—";
  const endpoints = table?.endpoints ?? [];
  const lockedMissing = table?.mode === "locked" && table.lockedEndpointId !== null
    && !endpoints.some((endpoint) => endpoint.endpointId === table.lockedEndpointId)
    && endpoints.length > 0;
  const rows: PageTableRow[] = endpoints.map((endpoint) => {
    const status = endpointStatus(endpoint);
    const current = endpoint.endpointId === table?.selectedEndpointId;
    return {
      key: endpoint.endpointId,
      selected: current,
      cells: {
        endpoint: <span className="vk-form-inline" data-judge-edge-endpoint={endpoint.endpointId}>
          <span title={endpoint.endpoint}>{endpoint.endpointId}</span>
          {current ? <Badge variant="outline" data-judge-edge-current="">
            {t(table?.mode === "locked"
              ? "settings.permissionJudge.edge.endpoints.locked"
              : "settings.permissionJudge.edge.endpoints.current")}
          </Badge> : null}
        </span>,
        region: endpoint.region ?? "—",
        network: ms(endpoint.rttMs),
        compute: ms(endpoint.computeP50Ms),
        total: ms(endpoint.scoreMs),
        status: <span title={endpoint.error ?? undefined}><StatusPill status={status.status} label={t(status.key)} /></span>,
        test: <Button type="button" size="sm" variant="ghost" disabled={props.disabled}
          data-judge-edge-request-test-open={endpoint.endpointId} onClick={() => setTesting(endpoint.endpointId)}>
          <Send size={14} />{t("settings.permissionJudge.edge.requestTest.open")}
        </Button>,
      },
    };
  });
  const lockValue = table?.mode === "locked" && table.lockedEndpointId !== null ? table.lockedEndpointId : AUTO;
  const controlsDisabled = props.disabled || busy !== null || table === null;

  return <PageSection layout="rows" data-judge-edge-endpoints=""
    title={t("settings.permissionJudge.edge.endpoints.title")}
    description={t("settings.permissionJudge.edge.endpoints.description")}
    actions={<Button type="button" size="sm" variant="outline" disabled={controlsDisabled} data-judge-edge-probe=""
      onClick={() => void run("probe", probePermissionJudgeEdgeEndpoints)}>
      <RefreshCw size={14} />{t(busy === "probe"
        ? "settings.permissionJudge.edge.endpoints.probing"
        : "settings.permissionJudge.edge.endpoints.probe")}
    </Button>}>
    <PageRow label={t("settings.permissionJudge.edge.endpoints.routing")}
      description={formatMessage(t("settings.permissionJudge.edge.endpoints.schedule"), {
        probed: time(table?.probedAtMs ?? null),
        next: time(table?.nextProbeAtMs ?? null),
      })}>
      <Select disabled={controlsDisabled} value={lockValue}
        onValueChange={(value) => void run("lock", () => lockPermissionJudgeEdgeEndpoint(value === AUTO ? null : value))}>
        <SelectTrigger aria-label={t("settings.permissionJudge.edge.endpoints.routing")} className="vk-form-w-lg"
          data-judge-edge-lock=""><SelectValue /></SelectTrigger>
        <SelectContent>
          <SelectItem value={AUTO}>{t("settings.permissionJudge.edge.endpoints.auto")}</SelectItem>
          {endpoints.map((endpoint) => <SelectItem key={endpoint.endpointId} value={endpoint.endpointId}>
            {formatMessage(t("settings.permissionJudge.edge.endpoints.lockTo"), { endpoint: endpoint.endpointId })}
          </SelectItem>)}
          {table?.mode === "locked" && table.lockedEndpointId !== null && !endpoints.some((endpoint) => endpoint.endpointId === table.lockedEndpointId)
            ? <SelectItem value={table.lockedEndpointId} disabled>
              {formatMessage(t("settings.permissionJudge.edge.endpoints.lockTo"), { endpoint: table.lockedEndpointId })}
            </SelectItem>
            : null}
        </SelectContent>
      </Select>
    </PageRow>
    {lockedMissing ? <PageNotice tone="warning" data-judge-edge-locked-missing="">
      {formatMessage(t("settings.permissionJudge.edge.endpoints.lockedMissing"), { endpoint: table?.lockedEndpointId ?? "" })}
    </PageNotice> : null}
    {table?.error ? <PageNotice tone="error" details={table.error} detailsLabel={t("settings.errors.showErrorDetails")}>
      {t("settings.permissionJudge.edge.endpoints.accessFailed")}
    </PageNotice> : null}
    {loadError ? <PageNotice tone="error" details={loadError} detailsLabel={t("settings.errors.showErrorDetails")}>
      {t("settings.permissionJudge.edge.endpoints.loadFailed")}
    </PageNotice> : null}
    {actionError ? <PageNotice tone="error" details={actionError} detailsLabel={t("settings.errors.showErrorDetails")}>
      {t("settings.permissionJudge.edge.endpoints.actionFailed")}
    </PageNotice> : null}
    <PageTable
      columns={[
        { key: "endpoint", label: t("settings.permissionJudge.edge.endpoints.column.endpoint") },
        { key: "region", label: t("settings.permissionJudge.edge.endpoints.column.region") },
        { key: "network", label: t("settings.permissionJudge.edge.endpoints.column.network"), numeric: true },
        { key: "compute", label: t("settings.permissionJudge.edge.endpoints.column.compute"), numeric: true },
        { key: "total", label: t("settings.permissionJudge.edge.endpoints.column.total"), numeric: true },
        { key: "status", label: t("settings.permissionJudge.edge.endpoints.column.status") },
        { key: "test", label: t("settings.permissionJudge.edge.requestTest.open") },
      ]}
      label={t("settings.permissionJudge.edge.endpoints.title")}
      rows={rows}
      minWidth={680}
      empty={<PageEmptyState size="inline" title={t("settings.permissionJudge.edge.endpoints.empty")} />}
    />
    <PermissionJudgeEdgeRequestTestDialog endpointId={testing} onClose={() => setTesting(null)} t={t} />
  </PageSection>;
}
