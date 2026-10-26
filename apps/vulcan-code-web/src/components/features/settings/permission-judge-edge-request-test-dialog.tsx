import * as React from "react";
import { Send } from "lucide-react";
import { PageDetails, PageFacts, PageNotice, PageRow, PageTable, type PageFact, type PageTableRow } from "@/components/page";
import { Badge, type BadgeVariant } from "@/ui-kit/primitives/badge";
import { Button } from "@/ui-kit/primitives/button";
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/ui-kit/primitives/dialog";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/ui-kit/primitives/select";
import { Textarea } from "@/ui-kit/primitives/textarea";
import { formatMessage, type Translate, type TranslationKey } from "@/i18n";
import { errorMessage } from "@/lib/display-format";
import { testPermissionJudgeEdgeRequest } from "@/service/permission-judge-service";
import type {
  JudgeLabel,
  PermissionJudgeEdgeRequestAttempt,
  PermissionJudgeEdgeRequestTest,
  PermissionJudgeEdgeTestInput,
  PermissionJudgeEdgeTestPreset,
} from "@/types";

// CUSTOM is the select value of a typed command; preset values never equal it.
// CUSTOM 是输入命令在下拉框中的取值；示例取值从不等于它。
const CUSTOM = "custom";

// PRESET_KEYS labels each sample command the service defines.
// PRESET_KEYS 为服务定义的每个示例命令提供标签。
const PRESET_KEYS: Record<PermissionJudgeEdgeTestPreset, TranslationKey> = {
  ordinary: "settings.permissionJudge.edge.requestTest.preset.ordinary",
  destructive: "settings.permissionJudge.edge.requestTest.preset.destructive",
  database_reset: "settings.permissionJudge.edge.requestTest.preset.databaseReset",
  oversized: "settings.permissionJudge.edge.requestTest.preset.oversized",
};

// LABEL_KEYS names each judge decision.
// LABEL_KEYS 为每种判定结果命名。
const LABEL_KEYS: Record<JudgeLabel, TranslationKey> = {
  allow: "settings.permissionJudge.records.label.allow",
  review: "settings.permissionJudge.records.label.review",
  block: "settings.permissionJudge.records.label.block",
};

// LABEL_VARIANTS colours each judge decision.
// LABEL_VARIANTS 为每种判定结果着色。
const LABEL_VARIANTS: Record<JudgeLabel, BadgeVariant> = { allow: "success", review: "warning", block: "destructive" };

// PermissionJudgeEdgeRequestTestDialog sends real judge requests to one official direct endpoint
// and shows each request's stage times and answer; results stay in the dialog.
// PermissionJudgeEdgeRequestTestDialog 向一个官方直连端点发送真实判定请求，显示每个请求的各阶段
// 耗时与回答；结果只保留在对话框中。
// Parameters: the endpoint to test (null closes the dialog), the close callback and the translator.
// Returns the dialog.
// 参数：要测试的端点（null 表示关闭）、关闭回调与翻译函数。返回该对话框。
export function PermissionJudgeEdgeRequestTestDialog(props: {
  endpointId: string | null;
  onClose: () => void;
  t: Translate;
}) {
  const { t, endpointId } = props;
  const [choice, setChoice] = React.useState<PermissionJudgeEdgeTestPreset | typeof CUSTOM>("ordinary");
  const [command, setCommand] = React.useState("");
  const [busy, setBusy] = React.useState(false);
  const [result, setResult] = React.useState<PermissionJudgeEdgeRequestTest | null>(null);
  const [error, setError] = React.useState<string | null>(null);

  React.useEffect(() => {
    setResult(null);
    setError(null);
  }, [endpointId]);

  const send = async () => {
    if (endpointId === null) return;
    const input: PermissionJudgeEdgeTestInput = choice === CUSTOM
      ? { kind: "custom", command }
      : { kind: "preset", preset: choice };
    setBusy(true);
    setError(null);
    try {
      setResult(await testPermissionJudgeEdgeRequest(endpointId, input));
    } catch (cause) {
      setResult(null);
      setError(errorMessage(cause));
    } finally {
      setBusy(false);
    }
  };

  const ms = (value: number | null) => value === null ? "—"
    : formatMessage(t("settings.permissionJudge.edge.endpoints.ms"), { value: Math.round(value) });
  const outcome = (attempt: PermissionJudgeEdgeRequestAttempt): React.ReactNode => {
    const first = attempt.answer?.results[0];
    const status = attempt.httpStatus === null
      ? t("settings.permissionJudge.edge.requestTest.unreachable")
      : formatMessage(t("settings.permissionJudge.edge.requestTest.httpStatus"), { status: attempt.httpStatus });
    return <span className="vk-form-inline" title={attempt.error ?? undefined}>
      {first ? <Badge variant={LABEL_VARIANTS[first.label]}>{t(LABEL_KEYS[first.label])}</Badge> : null}
      <span>{status}</span>
      {first?.inputTokens != null
        ? <span>{formatMessage(t("settings.permissionJudge.edge.requestTest.tokens"), { tokens: first.inputTokens })}</span>
        : null}
      {first && !first.modelEvaluated
        ? <span>{formatMessage(t("settings.permissionJudge.edge.requestTest.notEvaluated"), { reason: first.reasonCode ?? "—" })}</span>
        : null}
      {attempt.errorCode ? <code>{attempt.errorCode}</code> : null}
      {attempt.tokenRenewed ? <Badge variant="outline">{t("settings.permissionJudge.edge.requestTest.tokenRenewed")}</Badge> : null}
    </span>;
  };
  const rows: PageTableRow[] = (result?.attempts ?? []).map((attempt, index) => ({
    key: attempt.requestId,
    cells: {
      request: index + 1,
      total: ms(attempt.totalMs),
      network: ms(attempt.networkMs),
      endpoint: ms(attempt.endpointMs),
      node: ms(attempt.nodeMs),
      queue: ms(attempt.queueMs),
      compute: ms(attempt.computeMs),
      result: outcome(attempt),
    },
  }));
  const answered = result?.attempts.filter((attempt) => attempt.answer !== null).at(-1) ?? null;
  const answerFacts: PageFact[] = answered?.answer ? [
    { key: "region", label: t("settings.permissionJudge.edge.requestTest.answer.region"), value: answered.answer.region },
    { key: "model", label: t("settings.permissionJudge.edge.requestTest.answer.model"), value: answered.answer.model },
    { key: "policy", label: t("settings.permissionJudge.edge.requestTest.answer.policy"), value: answered.answer.policyVersion },
    ...answered.answer.results.flatMap((entry, index) => entry.logits === null ? [] : [{
      key: `scores-${index}`,
      label: t("settings.permissionJudge.edge.requestTest.answer.scores"),
      value: formatMessage(t("settings.permissionJudge.edge.requestTest.answer.scoresValue"), {
        allow: entry.logits[0].toFixed(3),
        review: entry.logits[1].toFixed(3),
        block: entry.logits[2].toFixed(3),
      }),
    }]),
  ] : [];
  const failures = (result?.attempts ?? []).filter((attempt) => attempt.error !== null);

  return <Dialog open={endpointId !== null} onOpenChange={(open) => { if (!open && !busy) props.onClose(); }}>
    <DialogContent size="xl" layout="form" data-judge-edge-request-test={endpointId ?? ""}>
      <DialogHeader>
        <DialogTitle>{formatMessage(t("settings.permissionJudge.edge.requestTest.title"), { endpoint: endpointId ?? "" })}</DialogTitle>
        <DialogDescription>{t("settings.permissionJudge.edge.requestTest.description")}</DialogDescription>
      </DialogHeader>
      <div className="vk-form-stack">
        <PageRow label={t("settings.permissionJudge.edge.requestTest.input")}>
          <Select disabled={busy} value={choice}
            onValueChange={(value) => setChoice(value as PermissionJudgeEdgeTestPreset | typeof CUSTOM)}>
            <SelectTrigger aria-label={t("settings.permissionJudge.edge.requestTest.input")} className="vk-form-w-lg"
              data-judge-edge-test-input=""><SelectValue /></SelectTrigger>
            <SelectContent>
              {(Object.keys(PRESET_KEYS) as PermissionJudgeEdgeTestPreset[]).map((preset) =>
                <SelectItem key={preset} value={preset}>{t(PRESET_KEYS[preset])}</SelectItem>)}
              <SelectItem value={CUSTOM}>{t("settings.permissionJudge.edge.requestTest.custom")}</SelectItem>
            </SelectContent>
          </Select>
        </PageRow>
        {choice === CUSTOM ? <Textarea aria-label={t("settings.permissionJudge.edge.requestTest.custom")} disabled={busy}
          rows={3} value={command} data-judge-edge-test-command=""
          placeholder={t("settings.permissionJudge.edge.requestTest.customPlaceholder")}
          onChange={(event) => setCommand(event.target.value)} /> : null}
        {error ? <PageNotice tone="error" details={error} detailsLabel={t("settings.errors.showErrorDetails")}>
          {t("settings.permissionJudge.edge.requestTest.failed")}
        </PageNotice> : null}
        {result ? <>
          <p className="vk-form-note" data-judge-edge-test-summary="">
            {result.succeeded > 0
              ? formatMessage(t("settings.permissionJudge.edge.requestTest.summary"), {
                succeeded: result.succeeded,
                total: result.attempts.length,
                min: ms(result.minMs),
                median: ms(result.medianMs),
                max: ms(result.maxMs),
              })
              : formatMessage(t("settings.permissionJudge.edge.requestTest.noneSucceeded"), { total: result.attempts.length })}
          </p>
          <p className="vk-form-note">
            {formatMessage(t("settings.permissionJudge.edge.requestTest.target"), {
              endpoint: result.endpoint,
              site: result.site,
              chars: result.renderedChars,
            })} {t("settings.permissionJudge.edge.requestTest.firstNote")}
          </p>
          <PageTable
            columns={[
              { key: "request", label: t("settings.permissionJudge.edge.requestTest.column.request"), numeric: true },
              { key: "total", label: t("settings.permissionJudge.edge.requestTest.column.total"), numeric: true },
              { key: "network", label: t("settings.permissionJudge.edge.requestTest.column.network"), numeric: true },
              { key: "endpoint", label: t("settings.permissionJudge.edge.requestTest.column.endpoint"), numeric: true },
              { key: "node", label: t("settings.permissionJudge.edge.requestTest.column.node"), numeric: true },
              { key: "queue", label: t("settings.permissionJudge.edge.requestTest.column.queue"), numeric: true },
              { key: "compute", label: t("settings.permissionJudge.edge.requestTest.column.compute"), numeric: true },
              { key: "result", label: t("settings.permissionJudge.edge.requestTest.column.result"), wrap: true },
            ]}
            label={t("settings.permissionJudge.edge.requestTest.column.result")}
            rows={rows}
            minWidth={720}
          />
          {answerFacts.length > 0 ? <PageFacts items={answerFacts} data-judge-edge-test-answer="" /> : null}
          {answered?.answer ? <PageDetails label={t("settings.permissionJudge.edge.requestTest.answer.details")} items={[
            { key: "checkpoint", label: t("settings.permissionJudge.edge.requestTest.answer.checkpoint"), value: <code>{answered.answer.checkpointSha256}</code> },
            { key: "request", label: t("settings.permissionJudge.edge.requestTest.answer.requestId"), value: <code>{answered.requestId}</code> },
          ]} /> : null}
          {failures.length > 0 ? <PageNotice tone="error" data-judge-edge-test-failures=""
            details={failures.map((attempt) => `${attempt.requestId}: ${attempt.error}`).join("\n")}
            detailsLabel={t("settings.errors.showErrorDetails")}>
            {formatMessage(t("settings.permissionJudge.edge.requestTest.failures"), { count: failures.length })}
          </PageNotice> : null}
        </> : null}
      </div>
      <DialogFooter showCloseButton>
        <Button type="button" disabled={busy || (choice === CUSTOM && command.trim() === "")} data-judge-edge-test-send=""
          onClick={() => void send()}>
          <Send size={14} />{t(busy ? "settings.permissionJudge.edge.requestTest.sending" : "settings.permissionJudge.edge.requestTest.send")}
        </Button>
      </DialogFooter>
    </DialogContent>
  </Dialog>;
}
