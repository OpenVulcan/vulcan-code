import React from "react";
import { CircleAlert, ShieldAlert } from "lucide-react";
import { Button } from "@/ui-kit/primitives/button";
import { formatMessage, useI18n, type Translate, type TranslationKey } from "@/i18n";
import type {
  PendingApprovalSnapshot,
  PendingApprovalSubject,
  PendingQuestionSnapshot,
  PermissionModeKey,
} from "@/types";
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
  // Child agent label when the request comes from a subagent of the visible conversation.
  // 请求来自当前可见会话的子代理时的子代理标签。
  requester?: string | null;
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

// CONFIG_KIND_KEYS maps configured object keys to their catalog labels.
// CONFIG_KIND_KEYS 将被配置对象键映射到目录标签。
const CONFIG_KIND_KEYS: Record<string, TranslationKey> = {
  mcp_server: "ask.approval.config.kind.mcpServer",
  provider_connection: "ask.approval.config.kind.providerConnection",
  custom_provider: "ask.approval.config.kind.customProvider",
};

// CONFIG_ACTION_KEYS maps configuration action keys to their catalog labels.
// CONFIG_ACTION_KEYS 将配置动作键映射到目录标签。
const CONFIG_ACTION_KEYS: Record<string, TranslationKey> = {
  create: "ask.approval.config.action.create",
  update: "ask.approval.config.action.update",
  remove: "ask.approval.config.action.remove",
  enable: "ask.approval.config.action.enable",
  disable: "ask.approval.config.action.disable",
  set_default: "ask.approval.config.action.setDefault",
};

// CONFIG_FIELD_KEYS maps configuration field keys to their catalog labels.
// CONFIG_FIELD_KEYS 将配置字段键映射到目录标签。
const CONFIG_FIELD_KEYS: Record<string, TranslationKey> = {
  name: "ask.approval.config.field.name",
  enabled: "ask.approval.config.field.enabled",
  transport: "ask.approval.config.field.transport",
  command: "ask.approval.config.field.command",
  args: "ask.approval.config.field.args",
  cwd: "ask.approval.config.field.cwd",
  env_passthrough: "ask.approval.config.field.envPassthrough",
  url: "ask.approval.config.field.url",
  headers_from_env: "ask.approval.config.field.headersFromEnv",
  provider: "ask.approval.config.field.provider",
  service_entry: "ask.approval.config.field.serviceEntry",
  credential_profile: "ask.approval.config.field.credentialProfile",
  api_key_env: "ask.approval.config.field.apiKeyEnv",
  base_url: "ask.approval.config.field.baseUrl",
  protocols: "ask.approval.config.field.protocols",
  authentication: "ask.approval.config.field.authentication",
  default_connection: "ask.approval.config.field.defaultConnection",
};

// CONFIG_NOTICE_KEYS maps configuration consequence keys to their catalog sentences.
// CONFIG_NOTICE_KEYS 将配置后果键映射到目录语句。
const CONFIG_NOTICE_KEYS: Record<string, TranslationKey> = {
  activates_for_all_sessions: "ask.approval.config.notice.activates",
  deactivates_for_all_sessions: "ask.approval.config.notice.deactivates",
  removes_for_all_sessions: "ask.approval.config.notice.removes",
  drops_secret_fields: "ask.approval.config.notice.dropsSecrets",
  copies_api_key: "ask.approval.config.notice.copiesApiKey",
  validates_with_provider: "ask.approval.config.notice.validates",
};

// CONFIG_VALUE_KEYS maps boolean configuration values to their catalog labels.
// CONFIG_VALUE_KEYS 将布尔配置值映射到目录标签。
const CONFIG_VALUE_KEYS: Record<string, TranslationKey> = {
  true: "ask.approval.config.value.on",
  false: "ask.approval.config.value.off",
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

// AskRequesterLine names the child agent that raised the request; the conversation's own requests show nothing.
// AskRequesterLine 标明发起请求的子代理；会话自身的请求不显示。
export function AskRequesterLine(props: { requester?: string | null; t: (key: TranslationKey) => string }) {
  if (!props.requester) return null;
  return <p className="vk-ask-requester">{formatMessage(props.t("ask.requester.subagent"), { agent: props.requester })}</p>;
}

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
    <div className="vk-ask-layer" style={overlayStyle}>
      <section
        aria-describedby={descriptionId}
        aria-labelledby={titleId}
        aria-modal="false"
        className="vk-ask"
        data-approval-request={request.id}
        data-kind="approval"
        data-risk={riskLabel ? "true" : undefined}
        onKeyDown={handleKeyDown}
        ref={barRef}
        role="dialog"
        tabIndex={-1}
      >
        <h2 className="vk-sr-only" id={titleId}>{t("ask.approval.title")}</h2>
        <p className="vk-sr-only" id={descriptionId}>{t("ask.approval.description")}</p>
        <AskRequesterLine requester={props.requester} t={t} />
        <div className="vk-ask-head">
          <ShieldAlert size={16} />
          <div className="vk-ask-title">
            {formatMessage(t(toolApprovalHeadlineKey(approval)), { tool: approval.toolName })}
          </div>
        </div>
        <ToolApprovalSubjectBody approval={approval} />
        <div className="vk-ask-chips">
          <span className="vk-badge" data-permission-mode={approval.permissionMode}>
            {formatMessage(t("ask.approval.mode"), { mode: modeKey ? t(modeKey) : approval.permissionMode })}
          </span>
          {riskLabel ? (
            <span className="vk-badge" data-high-risk={approval.highRisk ?? undefined} data-tone="bad">
              {formatMessage(t("ask.approval.highRisk"), { category: riskLabel })}
            </span>
          ) : null}
          {approval.judge ? <ToolApprovalJudgeChip judge={approval.judge} t={t} /> : null}
          {approval.review ? <ToolApprovalReviewChip review={approval.review} t={t} /> : null}
          {approval.deadlineUnixMs != null ? <ToolApprovalCountdown deadlineUnixMs={approval.deadlineUnixMs} t={t} /> : null}
        </div>
        {approval.review?.kind === "reviewed" && approval.review.reason ? (
          <p className="vk-ask-reason" data-review-reason>{approval.review.reason}</p>
        ) : null}
        {error ? (
          <div role="alert" className="vk-ask-error">
            {error}
          </div>
        ) : null}
        {stage === "stopConfirm" ? <ToolApprovalStopConfirm t={t} /> : null}
        <div className="vk-ask-actions">
          <Button
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
            <kbd className="vk-kbd">ESC</kbd>
          </Button>
          {stage === "deciding" ? (
            <>
              <Button
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
    case "configuration":
      return "ask.approval.config";
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
  if (subject.kind === "configuration") {
    return <ConfigurationChangeBody subject={subject} />;
  }
  const lines = subject.kind === "command"
    ? (subject.text.trim() ? [subject.text] : [])
    : subject.kind === "file_write"
      ? subject.paths
      : [];
  if (lines.length === 0) {
    return null;
  }
  return (
    <pre className="vk-ask-subject">
      {lines.join("\n")}
    </pre>
  );
}

// ConfigurationChangeBody lists what a configuration change sets, field by field, and its consequences.
// ConfigurationChangeBody 逐字段列出配置变更设置的内容及其后果。
function ConfigurationChangeBody(props: {
  // Configuration subject to render.
  // 要渲染的配置对象。
  subject: Extract<PendingApprovalSubject, { kind: "configuration" }>;
}) {
  const { t } = useI18n();
  const { subject } = props;
  const label = (keys: Record<string, TranslationKey>, key: string) => keys[key] ? t(keys[key]) : key;
  const value = (raw: string | null) => raw == null
    ? t("ask.approval.config.unset")
    : CONFIG_VALUE_KEYS[raw] ? t(CONFIG_VALUE_KEYS[raw]) : raw;
  const creating = subject.changes.every((change) => change.before == null);
  return (
    <div className="vk-ask-config" data-config-action={subject.action}>
      <div className="vk-ask-config-target">
        {formatMessage(t("ask.approval.config.summary"), {
          action: label(CONFIG_ACTION_KEYS, subject.action),
          kind: label(CONFIG_KIND_KEYS, subject.targetKind),
          target: subject.target,
        })}
      </div>
      {subject.changes.length > 0 ? (
        <dl className="vk-ask-config-fields">
          {subject.changes.map((change) => (
            <React.Fragment key={change.field}>
              <dt>{label(CONFIG_FIELD_KEYS, change.field)}</dt>
              <dd data-config-field={change.field}>
                {creating ? null : <><del>{value(change.before)}</del>{" → "}</>}
                <span>{value(change.after)}</span>
              </dd>
            </React.Fragment>
          ))}
        </dl>
      ) : null}
      {subject.notices.map((notice) => (
        <p className="vk-ask-reason" data-config-notice={notice} key={notice}>
          {label(CONFIG_NOTICE_KEYS, notice)}
        </p>
      ))}
    </div>
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
      <span className="vk-badge" data-judge="unavailable" title={judge.reason}>
        {t("ask.approval.judge.unavailable")}
      </span>
    );
  }
  const key = Object.hasOwn(JUDGE_LABEL_KEYS, judge.label) ? JUDGE_LABEL_KEYS[judge.label] : undefined;
  return (
    <span
      className="vk-badge"
      data-judge={judge.label}
      data-tone={judge.label === "block" ? "bad" : undefined}
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
      <span className="vk-badge" data-review="unavailable" title={review.reason}>
        {t("ask.approval.review.unavailable")}
      </span>
    );
  }
  const key = Object.hasOwn(REVIEW_VERDICT_KEYS, review.verdict) ? REVIEW_VERDICT_KEYS[review.verdict] : undefined;
  return (
    <span className="vk-badge" data-review={review.verdict}>
      {key ? t(key) : review.verdict}
    </span>
  );
}

// secondsUntil returns the whole seconds left before a Unix-millisecond deadline, never below zero.
// secondsUntil 返回距 Unix 毫秒截止时间剩余的整秒数，最小为零。
function secondsUntil(deadlineUnixMs: number): number {
  return Math.max(0, Math.ceil((deadlineUnixMs - Date.now()) / 1000));
}

// ToolApprovalCountdown shows how long the approval still waits before the service denies it; the
// service owns the deadline, so reaching zero only waits for the request to leave.
// ToolApprovalCountdown 展示审批在服务拒绝前还会等待多久；截止时间由服务掌握，因此归零后只等待请求离开。
function ToolApprovalCountdown(props: {
  // Unix milliseconds when the service denies the unanswered approval.
  // 服务拒绝未回答审批的 Unix 毫秒时间。
  deadlineUnixMs: number;
  // Translates stable UI copy keys.
  // 翻译稳定 UI 文案键。
  t: Translate;
}) {
  const { deadlineUnixMs } = props;
  const [seconds, setSeconds] = React.useState(() => secondsUntil(deadlineUnixMs));
  React.useEffect(() => {
    setSeconds(secondsUntil(deadlineUnixMs));
    const timer = window.setInterval(() => setSeconds(secondsUntil(deadlineUnixMs)), 1000);
    return () => window.clearInterval(timer);
  }, [deadlineUnixMs]);
  return (
    <span
      aria-live="off"
      className="vk-badge"
      data-approval-countdown={seconds}
      data-tone={seconds <= 10 ? "bad" : undefined}
    >
      {formatMessage(props.t("ask.approval.countdown"), { seconds })}
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
    <div className="vk-ask-confirm" data-tone="bad">
      <CircleAlert size={16} />
      <div>
        <strong>{props.t("ask.confirm.rejectTitle")}</strong>
        <small>{props.t("ask.approval.stopBody")}</small>
      </div>
    </div>
  );
}
