import React from "react";
import { CircleAlert, KeyRound } from "lucide-react";
import { Button } from "@/ui-kit/primitives/button";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/ui-kit/primitives/select";
import { useI18n, type Translate, type TranslationKey } from "@/i18n";
import { SecretValueInput } from "@/components/environment/secret-value-input";
import type {
  EnvironmentValueScope,
  EnvironmentValuesSubmitRequest,
  PendingEnvironmentRequestSnapshot,
  PendingEnvironmentVariableRequest,
  PendingQuestionSnapshot,
} from "@/types";
import { AskRequesterLine } from "./tool-approval-bar";
import { useAskAnchorStyle } from "./use-ask-anchor-style";

// EnvironmentRequestStage describes whether the user is entering values or confirming a stop.
// EnvironmentRequestStage 描述用户处于输入值阶段还是停止确认阶段。
type EnvironmentRequestStage = "entering" | "stopConfirm";

// EnvironmentValuesRequestProps carry one value request and the shared Ask callbacks.
// EnvironmentValuesRequestProps 携带一条取值请求及共享的 Ask 回调。
interface EnvironmentValuesRequestProps {
  // Pending request that owns the value request.
  // 拥有该取值请求的待处理请求。
  request: PendingQuestionSnapshot;
  // Child agent label when a subagent raised the request.
  // 子代理发起请求时的子代理标签。
  requester?: string | null;
  // Requested variables.
  // 请求的变量。
  environment: PendingEnvironmentRequestSnapshot;
  // Abandons the request, which cancels the call and interrupts the session.
  // 放弃请求，会取消调用并中断会话。
  onReject: (requestId: string) => Promise<void>;
  // Stores the entered values or declines; values never pass through the question reply.
  // 存储输入的值或拒绝；值从不经过问题回复。
  onSubmit: (request: EnvironmentValuesSubmitRequest) => Promise<void>;
}

// ENVIRONMENT_SCOPE_KEYS maps value scopes to their catalog labels.
// ENVIRONMENT_SCOPE_KEYS 将值作用域映射到目录标签。
const ENVIRONMENT_SCOPE_KEYS: Record<EnvironmentValueScope, TranslationKey> = {
  session: "environment.scope.session",
  project: "environment.scope.project",
  global: "environment.scope.global",
};

// FieldDraft is what the user entered for one requested variable.
// FieldDraft 是用户为一个请求变量输入的内容。
interface FieldDraft {
  value: string;
  scope: EnvironmentValueScope;
}

// initialScope keeps the agent's suggestion unless the session has no project to store into.
// initialScope 保留代理的建议，除非会话没有可存入的项目。
function initialScope(variable: PendingEnvironmentVariableRequest, projectAvailable: boolean): EnvironmentValueScope {
  if (variable.scope === "project" && !projectAvailable) return "session";
  return variable.scope;
}

// EnvironmentValuesRequest renders one request for environment values in the session Ask bar.
// EnvironmentValuesRequest 在会话 Ask 面板中渲染一条环境变量取值请求。
// Every field is masked until its own eye toggle reveals it; Enter never saves.
// 每个输入框在其眼睛开关显示前保持遮蔽；Enter 从不保存。
export function EnvironmentValuesRequest(props: EnvironmentValuesRequestProps) {
  const { t } = useI18n();
  const { environment, request } = props;
  const [stage, setStage] = React.useState<EnvironmentRequestStage>("entering");
  const [drafts, setDrafts] = React.useState<FieldDraft[]>(() =>
    environment.variables.map((variable) => ({ value: "", scope: initialScope(variable, environment.projectAvailable) })),
  );
  const [error, setError] = React.useState<string | null>(null);
  const [busy, setBusy] = React.useState(false);
  // mutationPending blocks repeated events before React commits the disabled state.
  // mutationPending 在 React 提交禁用状态前阻止重复事件。
  const mutationPending = React.useRef(false);
  const mounted = React.useRef(true);
  React.useEffect(() => {
    mounted.current = true;
    return () => { mounted.current = false; };
  }, []);
  const barRef = React.useRef<HTMLElement | null>(null);
  const titleId = React.useId();
  const descriptionId = React.useId();
  const overlayStyle = useAskAnchorStyle(request.id);

  React.useEffect(() => {
    const focusFrame = window.requestAnimationFrame(() => {
      barRef.current?.querySelector<HTMLInputElement>("input")?.focus() ?? barRef.current?.focus();
    });
    return () => window.cancelAnimationFrame(focusFrame);
  }, [request.id]);

  // acknowledge runs one backend call and keeps the panel locked after success.
  // acknowledge 执行一次后端调用，成功后保持面板锁定。
  const acknowledge = React.useCallback(async (action: () => Promise<void>) => {
    if (mutationPending.current) return;
    mutationPending.current = true;
    setBusy(true);
    setError(null);
    try {
      await action();
    } catch (failure) {
      if (!mounted.current) return;
      mutationPending.current = false;
      setBusy(false);
      setError(failure instanceof Error ? failure.message : String(failure));
    }
  }, []);

  const filled = drafts.some((draft) => draft.value.length > 0);
  const save = () => acknowledge(() => props.onSubmit({
    requestId: request.id,
    declined: false,
    values: environment.variables.flatMap((variable, index) => {
      const draft = drafts[index];
      return draft && draft.value.length > 0 ? [{ name: variable.name, value: draft.value, scope: draft.scope }] : [];
    }),
  }));
  const decline = () => acknowledge(() => props.onSubmit({ requestId: request.id, declined: true, values: [] }));
  const stop = () => acknowledge(() => props.onReject(request.id));

  const updateDraft = (index: number, patch: Partial<FieldDraft>) => {
    setDrafts((current) => current.map((draft, position) => position === index ? { ...draft, ...patch } : draft));
  };

  // handleKeyDown maps Esc to the stop confirmation. Keys from an open scope menu are left to the menu: its
  // portal is outside the panel in the DOM, but React still bubbles its events here.
  // handleKeyDown 把 Esc 映射到停止确认。打开的作用域菜单的按键留给菜单处理：其门户在 DOM 中位于面板之外，
  // 但 React 仍会把事件冒泡到这里。
  const handleKeyDown = (event: React.KeyboardEvent<HTMLElement>) => {
    if (mutationPending.current || busy || event.nativeEvent.isComposing || event.defaultPrevented) return;
    const target = event.target as Element;
    if (!event.currentTarget.contains(target) || event.currentTarget.querySelector('[aria-expanded="true"]')) return;
    if (event.key === "Escape") {
      event.preventDefault();
      setStage((current) => current === "entering" ? "stopConfirm" : "entering");
      return;
    }
    if (event.key === "Enter" && stage === "stopConfirm") {
      event.preventDefault();
      void stop();
    }
  };

  return (
    <div className="vk-ask-layer" style={overlayStyle}>
      <section
        aria-describedby={descriptionId}
        aria-labelledby={titleId}
        aria-modal="false"
        className="vk-ask"
        data-environment-request={request.id}
        data-kind="environment"
        onKeyDown={handleKeyDown}
        ref={barRef}
        role="dialog"
        tabIndex={-1}
      >
        <h2 className="vk-sr-only" id={titleId}>{t("ask.environment.title")}</h2>
        <p className="vk-sr-only" id={descriptionId}>{t("ask.environment.intro")}</p>
        <AskRequesterLine requester={props.requester} t={t} />
        <div className="vk-ask-head">
          <KeyRound size={16} />
          <div className="vk-ask-title">{t("ask.environment.title")}</div>
        </div>
        {environment.purpose ? <p className="vk-ask-reason" data-environment-purpose>{environment.purpose}</p> : null}
        <p className="vk-ask-reason">{t("ask.environment.intro")}</p>
        <div className="vk-ask-env-fields">
          {environment.variables.map((variable, index) => (
            <EnvironmentValueField
              disabled={busy || stage === "stopConfirm"}
              draft={drafts[index] ?? { value: "", scope: "session" }}
              key={variable.name}
              onChange={(patch) => updateDraft(index, patch)}
              projectAvailable={environment.projectAvailable}
              t={t}
              variable={variable}
            />
          ))}
        </div>
        {error ? <div role="alert" className="vk-ask-error">{error}</div> : null}
        {stage === "stopConfirm" ? (
          <div className="vk-ask-confirm" data-tone="bad">
            <CircleAlert size={16} />
            <div>
              <strong>{t("ask.confirm.rejectTitle")}</strong>
              <small>{t("ask.approval.stopBody")}</small>
            </div>
          </div>
        ) : null}
        <div className="vk-ask-actions">
          <Button
            data-environment-action={stage === "entering" ? "stop" : "back"}
            disabled={busy}
            onClick={() => {
              if (mutationPending.current) return;
              setStage((current) => current === "entering" ? "stopConfirm" : "entering");
            }}
            size="sm"
            type="button"
            variant="ghost"
          >
            <span>{stage === "entering" ? t("ask.approval.stop") : t("ask.approval.back")}</span>
            <kbd className="vk-kbd">ESC</kbd>
          </Button>
          {stage === "entering" ? (
            <>
              <Button data-environment-action="decline" disabled={busy} onClick={() => void decline()} size="sm" type="button" variant="outline">
                {t("ask.environment.decline")}
              </Button>
              <Button data-environment-action="save" disabled={busy || !filled} onClick={() => void save()} size="sm" type="button">
                {busy ? t("ask.action.processing") : t("ask.environment.save")}
              </Button>
            </>
          ) : (
            <Button data-environment-action="confirm-stop" disabled={busy} onClick={() => void stop()} size="sm" type="button" variant="destructive">
              {busy ? t("ask.action.processing") : t("ask.action.confirmReject")}
            </Button>
          )}
        </div>
      </section>
    </div>
  );
}

// EnvironmentValueField is one requested variable: its facts, a masked value and where to store it.
// EnvironmentValueField 是一个请求变量：其事实、遮蔽的值以及存储位置。
function EnvironmentValueField(props: {
  variable: PendingEnvironmentVariableRequest;
  draft: FieldDraft;
  projectAvailable: boolean;
  disabled: boolean;
  onChange: (patch: Partial<FieldDraft>) => void;
  t: Translate;
}) {
  const { variable, draft, t } = props;
  const inputId = React.useId();
  return (
    <div className="vk-ask-env-field" data-environment-variable={variable.name}>
      <div className="vk-ask-label-row">
        <label className="vk-ask-env-name" htmlFor={inputId}>{variable.name}</label>
        {variable.secret ? <span className="vk-badge">{t("environment.kind.secret")}</span> : null}
        {variable.definedBySystem ? (
          <span className="vk-badge" data-tone="warn" data-defined-by-system>{t("environment.definedBySystem")}</span>
        ) : null}
      </div>
      {variable.description ? <p className="vk-ask-reason">{variable.description}</p> : null}
      {variable.definedBySystem ? <p className="vk-ask-reason">{t("environment.definedBySystemNote")}</p> : null}
      <div className="vk-ask-env-controls">
        <SecretValueInput
          disabled={props.disabled}
          id={inputId}
          onValueChange={(value) => props.onChange({ value })}
          placeholder={t("environment.value.placeholder")}
          value={draft.value}
        />
        <Select
          disabled={props.disabled}
          onValueChange={(scope) => props.onChange({ scope: scope as EnvironmentValueScope })}
          value={draft.scope}
        >
          <SelectTrigger aria-label={t("environment.scope.label")} size="sm"><SelectValue /></SelectTrigger>
          <SelectContent>
            {(Object.keys(ENVIRONMENT_SCOPE_KEYS) as EnvironmentValueScope[]).map((scope) => (
              <SelectItem disabled={scope === "project" && !props.projectAvailable} key={scope} value={scope}>
                {t(ENVIRONMENT_SCOPE_KEYS[scope])}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
      </div>
    </div>
  );
}
