import * as React from "react";
import { CircleSlash, Download, ShieldAlert, ShieldCheck, ShieldX, Trash2 } from "lucide-react";
import { toast } from "sonner";
import { HIGH_RISK_KEYS } from "@/components/ask/tool-approval-bar";
import { PageDetails, PageEmptyState, PageHint, PageItem, PageNotice, PageRow, PageSection, type PageFact } from "@/components/page";
import {
  AlertDialog, AlertDialogAction, AlertDialogCancel, AlertDialogContent, AlertDialogDescription, AlertDialogFooter,
  AlertDialogHeader, AlertDialogTitle,
} from "@/components/ui/alert-dialog";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { FilterChip, FilterChipGroup } from "@/components/ui/choice-controls";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { StreamPlaceholder } from "@/components/ui/stream-placeholder";
import { formatMessage, useI18n, type Translate, type TranslationKey } from "@/i18n";
import { errorMessage } from "@/lib/display-format";
import { label } from "@/components/features/settings/permission-judge-labels";
import {
  clearPermissionJudgeRecords,
  exportPermissionJudgeRecords,
  loadPermissionJudgeRecords,
  markPermissionJudgeRecord,
} from "@/service/permission-judge-service";
import type { JudgeLabel, PermissionJudgeRecord, PermissionJudgeRecordPage } from "@/types";

// Records per page; the service caps it with its own page limit.
// 每页记录数；服务以自身的分页上限约束。
const PAGE_SIZE = 50;

// Judge result filters in display order; "failed" keeps records the judge could not score.
// 按显示顺序排列的判定结果筛选值；"failed" 保留判定器无法评分的记录。
const RESULT_KEYS: Record<JudgeLabel | "failed", TranslationKey> = {
  allow: "settings.permissionJudge.records.label.allow",
  review: "settings.permissionJudge.records.label.review",
  block: "settings.permissionJudge.records.label.block",
  failed: "settings.permissionJudge.records.label.failed",
};
const OUTCOME_KEYS: Record<string, TranslationKey> = {
  judge_allowed: "settings.permissionJudge.records.outcome.judgeAllowed",
  review_allowed: "settings.permissionJudge.records.outcome.reviewAllowed",
  review_denied: "settings.permissionJudge.records.outcome.reviewDenied",
  user_allowed: "settings.permissionJudge.records.outcome.userAllowed",
  user_denied: "settings.permissionJudge.records.outcome.userDenied",
  failed: "settings.permissionJudge.records.outcome.failed",
  unattended_denied: "settings.permissionJudge.records.outcome.unattendedDenied",
  approval_expired: "settings.permissionJudge.records.outcome.approvalExpired",
};
const MARK_LABELS: JudgeLabel[] = ["allow", "review", "block"];
// Value of the workspace filter that keeps every workspace; NUL never occurs in a path, and the
// select rejects an empty value.
// 保留全部工作区的工作区筛选值；NUL 不会出现在路径中，且下拉选单拒绝空值。
const ALL_WORKSPACES = "\0";

// Filters are the page's record filters.
// Filters 是本页的记录筛选条件。
interface Filters {
  label: JudgeLabel | "failed" | null;
  workspace: string | null;
  markedOnly: boolean;
}

// callSummary names what a call does: its command or path when the arguments carry one.
// callSummary 说明调用做什么：参数带有命令或路径时显示它。
function callSummary(record: PermissionJudgeRecord): string {
  try {
    const parsed: unknown = JSON.parse(record.arguments);
    if (parsed && typeof parsed === "object") {
      const fields = parsed as Record<string, unknown>;
      for (const key of ["command", "path", "destination"]) {
        if (typeof fields[key] === "string" && fields[key] !== "") return fields[key];
      }
    }
  } catch {
    // Arguments that are not JSON fall through to the tool name.
    // 不是 JSON 的参数回退为工具名称。
  }
  return record.toolName;
}

// resultOf returns the judge result key of a record, or null without a judge.
// resultOf 返回记录的判定结果键；没有判定器时为 null。
function resultOf(record: PermissionJudgeRecord): JudgeLabel | "failed" | null {
  return record.judge ? record.judge.label ?? "failed" : null;
}

// RESULT_ICONS shows each judge result with a glyph and tone.
// RESULT_ICONS 以图标与色调显示各判定结果。
const RESULT_ICONS = {
  allow: { icon: <ShieldCheck />, tone: "success" },
  review: { icon: <ShieldAlert />, tone: "attention" },
  block: { icon: <ShieldX />, tone: "danger" },
  failed: { icon: <CircleSlash />, tone: "neutral" },
} as const;

/** Decision log page: every decision the judge or the reviewer took part in, newest first, with
 * filters, marks that say what the judge should have given, sample export and clearing.
 * 判定记录页：判定器或复核器参与的每次决定（最新在前），提供筛选、标记判定器本应给出的结果、样本导出与清除。 */
export function PermissionJudgeRecordsPage() {
  const { locale, t } = useI18n();
  const [filters, setFilters] = React.useState<Filters>({ label: null, workspace: null, markedOnly: false });
  const [page, setPage] = React.useState<PermissionJudgeRecordPage | null>(null);
  const [loadError, setLoadError] = React.useState<string | null>(null);
  const [busy, setBusy] = React.useState<string | null>(null);
  const [clearOpen, setClearOpen] = React.useState(false);
  // sequence drops pages of superseded reads.
  // sequence 丢弃被取代读取的分页结果。
  const sequence = React.useRef(0);

  // load reads the first page for `next`, or appends the page after `before`.
  // load 读取 `next` 的第一页，或追加 `before` 之后的一页。
  const load = React.useCallback(async (next: Filters, before: PermissionJudgeRecordPage | null) => {
    const current = ++sequence.current;
    try {
      const result = await loadPermissionJudgeRecords({
        label: next.label,
        workspace: next.workspace,
        markedOnly: next.markedOnly,
        before: before ? before.records[before.records.length - 1]?.id ?? null : null,
        limit: PAGE_SIZE,
      });
      if (sequence.current !== current) return;
      setPage(before ? { ...result, records: [...before.records, ...result.records] } : result);
      setLoadError(null);
    } catch (cause) {
      if (sequence.current === current) setLoadError(errorMessage(cause));
    }
  }, []);

  React.useEffect(() => { void load(filters, null); }, [filters, load]);
  React.useEffect(() => () => { sequence.current += 1; }, []);

  const time = React.useMemo(() => new Intl.DateTimeFormat(locale, { dateStyle: "medium", timeStyle: "medium" }), [locale]);

  const mark = async (record: PermissionJudgeRecord, correction: JudgeLabel | null) => {
    setBusy(`mark-${record.id}`);
    try {
      const updated = await markPermissionJudgeRecord(record.id, correction);
      setPage((current) => current && {
        ...current,
        marked: current.marked + (updated.correction ? 1 : 0) - (record.correction ? 1 : 0),
        records: current.records.map((item) => item.id === updated.id ? updated : item),
      });
    } catch (cause) {
      toast.error(t("settings.permissionJudge.records.markFailed"), { description: errorMessage(cause) });
    } finally {
      setBusy(null);
    }
  };

  const exportSamples = async () => {
    setBusy("export");
    try {
      const exported = await exportPermissionJudgeRecords();
      const url = URL.createObjectURL(new Blob([exported.content], { type: "application/jsonl" }));
      const anchor = document.createElement("a");
      anchor.href = url;
      anchor.download = exported.fileName;
      anchor.click();
      window.setTimeout(() => URL.revokeObjectURL(url), 60_000);
      toast.success(formatMessage(t("settings.permissionJudge.records.exported"), { count: exported.count }));
    } catch (cause) {
      toast.error(t("settings.permissionJudge.records.exportFailed"), { description: errorMessage(cause) });
    } finally {
      setBusy(null);
    }
  };

  const clear = async () => {
    setBusy("clear");
    try {
      await clearPermissionJudgeRecords();
      setClearOpen(false);
      toast.success(t("settings.permissionJudge.records.cleared"));
      await load(filters, null);
    } catch (cause) {
      toast.error(t("settings.permissionJudge.records.clearFailed"), { description: errorMessage(cause) });
    } finally {
      setBusy(null);
    }
  };

  const clearButton = <Button type="button" size="sm" variant="ghost" data-records-clear="" disabled={busy !== null}
    onClick={() => setClearOpen(true)}>
    <Trash2 size={14} />{t("settings.permissionJudge.records.clear")}
  </Button>;

  if (!page) {
    return <PageSection title={t("settings.permissionJudge.records.title")}>
      {loadError ? <PageNotice tone="error" details={loadError} detailsLabel={t("settings.errors.showErrorDetails")}
        actions={clearButton}>
        {t("settings.permissionJudge.records.loadFailed")}
      </PageNotice> : <StreamPlaceholder label={t("ui.loading")} shape="rows" count={4} />}
      <ClearDialog busy={busy !== null} count={0} open={clearOpen} t={t} onClear={() => void clear()} onOpenChange={setClearOpen} />
    </PageSection>;
  }

  return <>
    <PageSection layout="rows" data-records-list=""
      title={formatMessage(t("settings.permissionJudge.records.count"), { count: page.total })}
      description={formatMessage(t("settings.permissionJudge.records.hint"), { records: page.retentionRecords, days: page.retentionDays })}
      actions={<>
        <Button type="button" size="sm" variant="outline" data-records-export="" disabled={busy !== null || page.marked === 0}
          onClick={() => void exportSamples()}>
          <Download size={14} />{formatMessage(t("settings.permissionJudge.records.export"), { count: page.marked })}
        </Button>
        {clearButton}
      </>}
      footer={page.hasMore ? <Button type="button" size="sm" variant="outline" data-records-more=""
        onClick={() => void load(filters, page)}>
        {t("settings.permissionJudge.records.loadMore")}
      </Button> : undefined}>
      <PageRow label={t("settings.permissionJudge.records.filterResult")} stacked>
        <FilterChipGroup aria-label={t("settings.permissionJudge.records.filterResult")}>
          <FilterChip selected={filters.label === null} onClick={() => setFilters({ ...filters, label: null })}>
            {t("settings.permissionJudge.records.all")}
          </FilterChip>
          {(Object.keys(RESULT_KEYS) as (JudgeLabel | "failed")[]).map((result) => (
            <FilterChip key={result} data-records-filter={result} selected={filters.label === result}
              onClick={() => setFilters({ ...filters, label: result })}>
              {t(RESULT_KEYS[result])}
            </FilterChip>
          ))}
        </FilterChipGroup>
        <FilterChip data-records-marked="" selected={filters.markedOnly}
          onClick={() => setFilters({ ...filters, markedOnly: !filters.markedOnly })}>
          {t("settings.permissionJudge.records.markedOnly")}
        </FilterChip>
        {page.workspaces.length > 0 ? <Select value={filters.workspace ?? ALL_WORKSPACES}
          onValueChange={(workspace) => setFilters({ ...filters, workspace: workspace === ALL_WORKSPACES ? null : workspace })}>
          <SelectTrigger aria-label={t("settings.permissionJudge.records.detail.workspace")} className="h-7 w-auto max-w-72"><SelectValue /></SelectTrigger>
          <SelectContent>
            <SelectItem value={ALL_WORKSPACES}>{t("settings.permissionJudge.records.allWorkspaces")}</SelectItem>
            {page.workspaces.map((workspace) => <SelectItem key={workspace} value={workspace}>{workspace}</SelectItem>)}
          </SelectContent>
        </Select> : null}
      </PageRow>
      {loadError ? <PageNotice tone="error" details={loadError} detailsLabel={t("settings.errors.showErrorDetails")}>
        {t("settings.permissionJudge.records.loadFailed")}
      </PageNotice> : null}
      {page.records.length === 0
        ? <PageEmptyState icon={<ShieldCheck />} title={t("settings.permissionJudge.records.empty")} />
        : page.records.map((record) => <RecordItem key={record.id} busy={busy !== null} record={record} t={t}
          time={time.format(record.judgedAtMs)} onMark={(correction) => void mark(record, correction)} />)}
    </PageSection>

    <ClearDialog busy={busy !== null} count={page.total} open={clearOpen} t={t} onClear={() => void clear()} onOpenChange={setClearOpen} />
  </>;
}

// RecordItem shows one record: what was called and how each layer decided, with details and marks.
// RecordItem 显示一条记录：调用了什么、各层如何决定，并附详细信息与标记。
function RecordItem(props: {
  busy: boolean;
  record: PermissionJudgeRecord;
  t: Translate;
  time: string;
  onMark: (correction: JudgeLabel | null) => void;
}) {
  const { record, t } = props;
  const result = resultOf(record);
  const look = result ? RESULT_ICONS[result] : RESULT_ICONS.failed;
  const trace = record.judge?.trace ?? null;
  const facts: PageFact[] = [
    { key: "tool", label: t("settings.permissionJudge.records.detail.tool"), value: record.toolName },
    ...(record.workspaceRoot ? [{ key: "workspace", label: t("settings.permissionJudge.records.detail.workspace"), value: record.workspaceRoot }] : []),
    ...(record.highRisk ? [{
      key: "highRisk", label: t("settings.permissionJudge.records.detail.highRisk"),
      value: HIGH_RISK_KEYS[record.highRisk] ? t(HIGH_RISK_KEYS[record.highRisk]) : record.highRisk,
    }] : []),
    ...(trace ? [{ key: "model", label: t("settings.permissionJudge.records.detail.model"), value: `${trace.model} · ${trace.policyVersion}` }] : []),
    ...(trace?.logits ? [{
      key: "scores", label: t("settings.permissionJudge.records.detail.scores"),
      value: MARK_LABELS.map((name, index) => `${t(RESULT_KEYS[name])} ${trace.logits![index].toFixed(2)}`).join(" · "),
    }] : []),
    ...(trace?.allowProbability != null ? [{
      key: "probability", label: t("settings.permissionJudge.records.detail.probability"),
      value: `${(trace.allowProbability * 100).toFixed(1)}%`,
    }] : []),
    ...(record.judge ? [{
      key: "judgeTime", label: t("settings.permissionJudge.records.detail.judgeTime"),
      value: formatMessage(t("settings.permissionJudge.ms"), { value: record.judge.elapsedMs.toFixed(1) }),
    }] : []),
    ...(record.judge?.error ? [{ key: "judgeError", label: t("settings.permissionJudge.records.detail.error"), value: record.judge.error, tone: "attention" as const }] : []),
    ...(record.review ? [{
      key: "review", label: t("settings.permissionJudge.records.detail.review"),
      value: record.review.reason ?? record.review.error ?? "—",
    }, {
      key: "reviewTime", label: t("settings.permissionJudge.records.detail.reviewTime"),
      value: formatMessage(t("settings.permissionJudge.ms"), { value: record.review.elapsedMs.toFixed(1) }),
    }] : []),
  ];
  return <PageItem data-record-id={record.id} icon={look.icon} tone={look.tone}
    title={<span className="break-all font-mono text-xs">{callSummary(record)}</span>}
    meta={<>
      {result ? <Badge variant="outline">{t(RESULT_KEYS[result])}</Badge> : null}
      <Badge variant="secondary">{label(t, OUTCOME_KEYS, record.outcome)}</Badge>
      {record.correction ? <Badge data-record-correction={record.correction}>
        {formatMessage(t("settings.permissionJudge.records.marked"), { label: t(RESULT_KEYS[record.correction]) })}
      </Badge> : null}
    </>}
    description={[props.time, record.workspaceRoot].filter(Boolean).join(" · ")}>
    <PageDetails label={t("settings.permissionJudge.records.details")} items={facts}>
      <pre className="mt-2 max-h-48 overflow-auto rounded-md bg-muted p-2 text-xs break-all whitespace-pre-wrap">{record.arguments}</pre>
    </PageDetails>
    {trace ? <div className="mt-2 flex flex-wrap items-center gap-1" data-record-mark="">
      <PageHint className="mr-1">{t("settings.permissionJudge.records.markAs")}</PageHint>
      {MARK_LABELS.map((name) => (
        <FilterChip key={name} selected={record.correction === name} disabled={props.busy}
          onClick={() => props.onMark(record.correction === name ? null : name)}>
          {t(RESULT_KEYS[name])}
        </FilterChip>
      ))}
    </div> : <PageHint className="mt-2">{t("settings.permissionJudge.records.noTrace")}</PageHint>}
  </PageItem>;
}

// ClearDialog confirms removing the whole decision log.
// ClearDialog 确认删除全部判定记录。
function ClearDialog(props: {
  busy: boolean;
  count: number;
  open: boolean;
  t: Translate;
  onClear: () => void;
  onOpenChange: (open: boolean) => void;
}) {
  const { t } = props;
  return <AlertDialog open={props.open} onOpenChange={(open) => { if (!props.busy) props.onOpenChange(open); }}>
    <AlertDialogContent>
      <AlertDialogHeader>
        <AlertDialogTitle>{t("settings.permissionJudge.records.clearTitle")}</AlertDialogTitle>
        <AlertDialogDescription>
          {formatMessage(t("settings.permissionJudge.records.clearDescription"), { count: props.count })}
        </AlertDialogDescription>
      </AlertDialogHeader>
      <AlertDialogFooter>
        <AlertDialogCancel disabled={props.busy}>{t("settings.permissionJudge.records.cancel")}</AlertDialogCancel>
        <AlertDialogAction disabled={props.busy} data-records-clear-confirm=""
          onClick={(event) => { event.preventDefault(); props.onClear(); }}>
          {t("settings.permissionJudge.records.clearConfirm")}
        </AlertDialogAction>
      </AlertDialogFooter>
    </AlertDialogContent>
  </AlertDialog>;
}
