import React from "react";
import { CircleAlert, ShieldAlert } from "lucide-react";
import { Button } from "@/components/ui/button";
import { formatMessage, useI18n, type Translate, type TranslationKey } from "@/i18n";
import { cn } from "@/lib/utils";
import type { PendingApprovalSnapshot, PendingQuestionSnapshot, PermissionModeKey } from "@/types";
import { useAskAnchorStyle } from "./use-ask-anchor-style";

// ToolApprovalStage describes whether the user is deciding or confirming a stop.
// ToolApprovalStage 描述用户处于决定阶段还是停止确认阶段。
type ToolApprovalStage = "deciding" | "stopConfirm";

// ToolApprovalRequestProps carries one approval request and the shared Ask acknowledgement callbacks.
// ToolApprovalRequestProps 携带一条审批请求及共享的 Ask 确认回调。
interface ToolApprovalRequestProps {
  // Pending request that owns the approval.
  // 拥有该审批的待处理请求。
  request: PendingQuestionSnapshot;
  // Permission facts of the gated tool call.
  // 受管控工具调用的权限事实。
  approval: PendingApprovalSnapshot;
  // Abandons the request, which cancels the call and interrupts the session.
  // 放弃请求，会取消调用并中断会话。
  onReject: (requestId: string) => Promise<void>;
  // Replies with one canonical approval label.
  // 以一个规范审批标签回复。
  onSubmit: (requestId: string, answers: string[][]) => Promise<void>;
}

// PERMISSION_MODE_KEYS maps canonical permission modes to their catalog labels.
// PERMISSION_MODE_KEYS 将规范权限模式映射到目录标签。
const PERMISSION_MODE_KEYS: Record<PermissionModeKey, TranslationKey> = {
  read_only: "permissionMode.readOnly",
  workspace_write: "permissionMode.workspaceWrite",
  auto_approve: "permissionMode.autoApprove",
  full_access: "permissionMode.fullAccess",
};

// HIGH_RISK_KEYS maps backend high-risk category keys to their catalog labels.
// HIGH_RISK_KEYS 将后端高风险类别键映射到目录标签。
export const HIGH_RISK_KEYS: Record<string, TranslationKey> = {
  recursive_delete: "ask.approval.risk.recursiveDelete",
  destructive_git: "ask.approval.risk.destructiveGit",
  disk_format: "ask.approval.risk.diskFormat",
  system_configuration: "ask.approval.risk.systemConfiguration",
  opaque_script: "ask.approval.risk.opaqueScript",
  database_drop: "ask.approval.risk.databaseDrop",
  permission_change: "ask.approval.risk.permissionChange",
  publish: "ask.approval.risk.publish",
  infrastructure_destroy: "ask.approval.risk.infrastructureDestroy",
};

// JUDGE_LABEL_KEYS maps permission judge labels to their catalog labels.
// JUDGE_LABEL_KEYS 将权限判定标签映射到目录标签。
const JUDGE_LABEL_KEYS: Record<string, TranslationKey> = {
  allow: "ask.approval.judge.allow",
  review: "ask.approval.judge.review",
  block: "ask.approval.judge.block",
};

// REVIEW_VERDICT_KEYS maps permission reviewer verdicts to their catalog labels.
// REVIEW_VERDICT_KEYS 将权限复核决定映射到目录标签。
const REVIEW_VERDICT_KEYS: Record<string, TranslationKey> = {
  allow: "ask.approval.review.allow",
  ask_user: "ask.approval.review.askUser",
  deny: "ask.approval.review.deny",
};

// ToolApprovalRequest renders one tool-call approval in the session Ask bar.
// ToolApprovalRequest 在会话 Ask 面板中渲染一条工具调用审批。
// Enter never approves; approving or denying needs an explicit click, and Esc opens the stop confirmation.
// Enter 从不批准；批准或拒绝需要显式点击，Esc 打开停止确认。
export function ToolApprovalRequest(props: ToolApprovalRequestProps) {
  // t resolves stable WebUI copy keys for the active language.
  // t 根据当前语言解析稳定的 WebUI 文案键。
  const { t } = useI18n();
  const { approval, request } = props;
  // stage stores whether the user is deciding or confirming a stop.
  // stage 保存用户处于决定阶段还是停止确认阶段。
  const [stage, setStage] = React.useState<ToolApprovalStage>("deciding");
  // error stores the latest failed acknowledgement message.
  // error 保存最近一次确认失败的消息。
  const [error, setError] = React.useState<string | null>(null);
  // busy disables actions while the backend call is pending.
  // busy 在后端调用未完成时禁用操作。
  const [busy, setBusy] = React.useState(false);
  // mutationPending blocks repeated events before React commits the disabled state.
  // mutationPending 在 React 提交禁用状态前阻止重复事件。
  const mutationPending = React.useRef(false);
  // mounted prevents a retired request from publishing late errors into its replacement.
  // mounted 阻止已离开的请求把迟到错误发布到替代请求中。
  const mounted = React.useRef(true);
  React.useEffect(() => {
    mounted.current = true;
    return () => { mounted.current = false; };
  }, []);
  // barRef owns keyboard focus so Esc reaches the stop confirmation.
  // barRef 承载键盘焦点，使 Esc 能到达停止确认。
  const barRef = React.useRef<HTMLElement | null>(null);
  const titleId = React.useId();
  const descriptionId = React.useId();
  const overlayStyle = useAskAnchorStyle(request.id);

  React.useEffect(() => {
    const focusFrame = window.requestAnimationFrame(() => barRef.current?.focus());
    return () => window.cancelAnimationFrame(focusFrame);
  }, [request.id]);

  // acknowledge runs one backend acknowledgement and keeps the bar locked after success.
  // acknowledge 执行一次后端确认，成功后保持面板锁定。
  const acknowledge = React.useCallback(async (action: () => Promise<void>) => {
    if (mutationPending.current) {
      return;
    }
    mutationPending.current = true;
    setBusy(true);
    setError(null);
    try {
      await action();
      // Successful acknowledgements stay locked until the authoritative request leaves the surface.
      // 成功确认后保持锁定，直到权威请求从面板移除。
    } catch (failure) {
      if (!mounted.current) return;
      mutationPending.current = false;
      setBusy(false);
      setError(failure instanceof Error ? failure.message : String(failure));
    }
  }, []);

  const reply = (label: string) => acknowledge(() => props.onSubmit(request.id, [[label]]));
  const stop = () => acknowledge(() => props.onReject(request.id));

  // handleKeyDown maps Esc and Enter to the stop confirmation only.
  // handleKeyDown 只把 Esc 与 Enter 映射到停止确认。
  const handleKeyDown = (event: React.KeyboardEvent<HTMLElement>) => {
    if (mutationPending.current || busy || event.nativeEvent.isComposing) {
      return;
    }
    if (event.key === "Escape") {
      event.preventDefault();
      setStage((current) => current === "deciding" ? "stopConfirm" : "deciding");
      return;
    }
    if (event.key === "Enter" && stage === "stopConfirm") {
      event.preventDefault();
      void stop();
    }
  };

  // riskKey is the catalog label of the matched high-risk category, when known.
  // riskKey 是命中高风险类别的目录标签（已知时）。
  const riskKey = approval.highRisk ? HIGH_RISK_KEYS[approval.highRisk] : undefined;
  const riskLabel = approval.highRisk ? (riskKey ? t(riskKey) : approval.highRisk) : null;
  const modeKey = PERMISSION_MODE_KEYS[approval.permissionMode];

  return (
    <div className="pointer-events-none fixed bottom-4 z-50 flex justify-center px-4" style={overlayStyle}>
      <section
        aria-describedby={descriptionId}
        aria-labelledby={titleId}
        aria-modal="false"
        className="pointer-events-auto max-h-[min(420px,calc(100vh-2rem))] w-full max-w-[760px] overflow-hidden rounded-2xl border border-border/80 bg-popover p-0 text-[13px] text-popover-foreground shadow-2xl outline-none"
        data-approval-request={request.id}
        onKeyDown={handleKeyDown}
        ref={barRef}
        role="dialog"
        tabIndex={-1}
      >
        <div className="flex max-h-[min(420px,calc(100vh-2rem))] min-h-0 flex-col gap-2.5 p-3.5">
          <h2 className="sr-only" id={titleId}>{t("ask.approval.title")}</h2>
          <p className="sr-only" id={descriptionId}>{t("ask.approval.description")}</p>
          <div className="flex items-start gap-2">
            <ShieldAlert className={cn("mt-0.5 size-4 shrink-0", riskLabel ? "text-destructive" : "text-muted-foreground")} />
            <div className="min-w-0 flex-1 text-sm leading-5 font-semibold">
              {formatMessage(t(toolApprovalHeadlineKey(approval)), { tool: approval.toolName })}
            </div>
          </div>
          <ToolApprovalSubjectBody approval={approval} />
          <div className="flex flex-wrap items-center gap-1.5 text-[11px] leading-4">
            <span className="rounded-md bg-muted px-1.5 py-0.5 text-muted-foreground" data-permission-mode={approval.permissionMode}>
              {formatMessage(t("ask.approval.mode"), { mode: modeKey ? t(modeKey) : approval.permissionMode })}
            </span>
            {riskLabel ? (
              <span className="rounded-md bg-destructive/10 px-1.5 py-0.5 text-destructive" data-high-risk={approval.highRisk ?? undefined}>
                {formatMessage(t("ask.approval.highRisk"), { category: riskLabel })}
              </span>
            ) : null}
            {approval.judge ? <ToolApprovalJudgeChip judge={approval.judge} t={t} /> : null}
            {approval.review ? <ToolApprovalReviewChip review={approval.review} t={t} /> : null}
          </div>
          {approval.review?.kind === "reviewed" && approval.review.reason ? (
            <p className="text-xs leading-5 text-muted-foreground" data-review-reason>{approval.review.reason}</p>
          ) : null}
          {error ? (
            <div role="alert" className="max-h-24 overflow-y-auto break-words rounded-lg border border-destructive/25 bg-destructive/10 px-2.5 py-1.5 text-xs leading-5 text-destructive">
              {error}
            </div>
          ) : null}
          {stage === "stopConfirm" ? <ToolApprovalStopConfirm t={t} /> : null}
          <div className="flex items-center justify-end gap-1.5">
            <Button
              className="h-8 gap-1.5 px-2 text-xs text-muted-foreground"
              data-approval-action={stage === "deciding" ? "stop" : "back"}
              disabled={busy}
              onClick={() => {
                if (mutationPending.current) return;
                setStage((current) => current === "deciding" ? "stopConfirm" : "deciding");
              }}
              size="sm"
              type="button"
              variant="ghost"
            >
              <span>{stage === "deciding" ? t("ask.approval.stop") : t("ask.approval.back")}</span>
              <span className="rounded bg-muted px-1.5 py-0.5 text-[11px] text-muted-foreground">ESC</span>
            </Button>
            {stage === "deciding" ? (
              <>
                <Button
                  className="h-8 rounded-xl px-3 text-xs"
                  data-approval-action="deny"
                  disabled={busy}
                  onClick={() => void reply(approval.denyLabel)}
                  size="sm"
                  type="button"
                  variant="outline"
                >
                  {t("ask.approval.deny")}
                </Button>
                <Button
                  className="h-8 rounded-xl px-3 text-xs"
                  data-approval-action="allow"
                  disabled={busy}
                  onClick={() => void reply(approval.allowLabel)}
                  size="sm"
                  type="button"
                >
                  {busy ? t("ask.action.processing") : t("ask.approval.allow")}
                </Button>
              </>
            ) : (
              <Button
                className="h-8 rounded-xl px-3 text-xs"
                data-approval-action="confirm-stop"
                disabled={busy}
                onClick={() => void stop()}
                size="sm"
                type="button"
                variant="destructive"
              >
                {busy ? t("ask.action.processing") : t("ask.action.confirmReject")}
              </Button>
            )}
          </div>
        </div>
      </section>
    </div>
  );
}

// toolApprovalHeadlineKey picks the catalog headline for the approval subject.
// toolApprovalHeadlineKey 为审批对象选择目录标题。
function toolApprovalHeadlineKey(approval: PendingApprovalSnapshot): TranslationKey {
  switch (approval.subject.kind) {
    case "command":
      return approval.subject.text.trim() ? "ask.approval.command" : "ask.approval.shell";
    case "file_write":
      return "ask.approval.fileWrite";
    case "opaque":
      return "ask.approval.opaque";
  }
}

// ToolApprovalSubjectBody shows the exact command or paths the call would affect.
// ToolApprovalSubjectBody 展示调用会影响的确切命令或路径。
function ToolApprovalSubjectBody(props: {
  // Approval whose subject is rendered.
  // 要渲染其对象的审批。
  approval: PendingApprovalSnapshot;
}) {
  const subject = props.approval.subject;
  const lines = subject.kind === "command"
    ? (subject.text.trim() ? [subject.text] : [])
    : subject.kind === "file_write"
      ? subject.paths
      : [];
  if (lines.length === 0) {
    return null;
  }
  return (
    <pre className="max-h-40 min-h-0 overflow-auto whitespace-pre-wrap break-all rounded-lg bg-muted/60 px-2.5 py-1.5 font-mono text-xs leading-5">
      {lines.join("\n")}
    </pre>
  );
}

// ToolApprovalJudgeChip shows the permission judge finding; an unavailable judge shows its reason on hover.
// ToolApprovalJudgeChip 展示权限判定结论；判定不可用时悬停显示原因。
function ToolApprovalJudgeChip(props: {
  // Finding to show.
  // 要展示的结论。
  judge: NonNullable<PendingApprovalSnapshot["judge"]>;
  // Translates stable UI copy keys.
  // 翻译稳定 UI 文案键。
  t: Translate;
}) {
  const { judge, t } = props;
  if (judge.kind === "unavailable") {
    return (
      <span className="rounded-md bg-muted px-1.5 py-0.5 text-muted-foreground" data-judge="unavailable" title={judge.reason}>
        {t("ask.approval.judge.unavailable")}
      </span>
    );
  }
  const key = Object.hasOwn(JUDGE_LABEL_KEYS, judge.label) ? JUDGE_LABEL_KEYS[judge.label] : undefined;
  return (
    <span
      className={cn("rounded-md px-1.5 py-0.5", judge.label === "block" ? "bg-destructive/10 text-destructive" : "bg-muted text-muted-foreground")}
      data-judge={judge.label}
    >
      {key ? t(key) : judge.label}
    </span>
  );
}

// ToolApprovalReviewChip shows the permission reviewer verdict; an unavailable reviewer shows its reason on hover.
// ToolApprovalReviewChip 展示权限复核决定；复核不可用时悬停显示原因。
function ToolApprovalReviewChip(props: {
  // Finding to show.
  // 要展示的结论。
  review: NonNullable<PendingApprovalSnapshot["review"]>;
  // Translates stable UI copy keys.
  // 翻译稳定 UI 文案键。
  t: Translate;
}) {
  const { review, t } = props;
  if (review.kind === "unavailable") {
    return (
      <span className="rounded-md bg-muted px-1.5 py-0.5 text-muted-foreground" data-review="unavailable" title={review.reason}>
        {t("ask.approval.review.unavailable")}
      </span>
    );
  }
  const key = Object.hasOwn(REVIEW_VERDICT_KEYS, review.verdict) ? REVIEW_VERDICT_KEYS[review.verdict] : undefined;
  return (
    <span className="rounded-md bg-muted px-1.5 py-0.5 text-muted-foreground" data-review={review.verdict}>
      {key ? t(key) : review.verdict}
    </span>
  );
}

// ToolApprovalStopConfirm explains what stopping does before it happens.
// ToolApprovalStopConfirm 在停止前说明停止的后果。
function ToolApprovalStopConfirm(props: {
  // Translates stable UI copy keys.
  // 翻译稳定 UI 文案键。
  t: Translate;
}) {
  return (
    <div className="flex items-center gap-2.5 rounded-xl border border-border/70 bg-muted/45 px-2.5 py-1.5">
      <CircleAlert className="size-4 shrink-0 text-destructive" />
      <div className="min-w-0">
        <div className="text-[13px] leading-5 font-medium">{props.t("ask.confirm.rejectTitle")}</div>
        <div className="text-[11px] leading-4 text-muted-foreground">{props.t("ask.approval.stopBody")}</div>
      </div>
    </div>
  );
}
