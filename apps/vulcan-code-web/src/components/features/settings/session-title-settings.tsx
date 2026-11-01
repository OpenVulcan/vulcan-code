import * as React from "react";
import { toast } from "@/ui-kit/primitives/toast";
import { Combobox } from "@/ui-kit/primitives/combobox";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/ui-kit/primitives/select";
import { Switch } from "@/ui-kit/primitives/switch";
import { SettingsErrorNotice } from "@/components/features/settings/settings-error-notice";
import {
  candidateSupportsDisabledReasoning,
  inferenceRouteKey,
  inferenceRoutePickerOptions,
} from "@/components/features/inference-route-picker";
import type { SettingsActionProps } from "@/components/features/settings/settings-action-types";
import { useI18n, type TranslationKey } from "@/i18n";
import { errorMessage } from "@/lib/display-format";
import { loadPublicAiSettings, savePublicAiInferenceRoute } from "@/service/settings-actions-service";
import type {
  PublicAiInferenceRoute,
  PublicAiInternalInferenceCandidate,
  PublicAiReasoningEffort,
  PublicAiSettingsSnapshot,
} from "@/types";
import "@/ui-kit/styles/features/settings-forms.css";

// The title generator's consumer profile and its single LLM purpose, owned by the service kernel.
// 标题生成器的消费者配置档及其唯一 LLM 用途，由服务内核持有。
const SESSION_TITLE_CONSUMER_PROFILE_ID = "session_title";
const SESSION_TITLE_PURPOSE_ID = "session_title_generation";

// SESSION_MODEL is the picker value that saves no route, so each session titles itself.
// SESSION_MODEL 是不保存路由的选择值，使每个会话用自身模型生成标题。
const SESSION_MODEL = "session-model";

// MODEL_DEFAULT_EFFORT is the select value that saves no explicit reasoning effort.
// MODEL_DEFAULT_EFFORT 是不保存显式思考档位的选择值。
const MODEL_DEFAULT_EFFORT = "model-default";

// Labels of the reasoning efforts a model may declare; unknown values render as themselves.
// 模型可能声明的思考档位标签；未知值按原样显示。
const EFFORT_KEYS: Record<string, TranslationKey> = {
  none: "composer.reasoning.level.none",
  adaptive: "composer.reasoning.level.adaptive",
  minimal: "composer.reasoning.level.minimal",
  low: "composer.reasoning.level.low",
  medium: "composer.reasoning.level.medium",
  high: "composer.reasoning.level.high",
  xhigh: "composer.reasoning.level.xHigh",
  max: "composer.reasoning.level.max",
  ultra: "composer.reasoning.level.ultra",
};

// sessionTitleCandidates keeps the validated internal routes the backend accepts for titles,
// with the models that can turn thinking off first.
// sessionTitleCandidates 保留后端接受用于标题的已验证内部路由，可关闭思考的模型排在前面。
export function sessionTitleCandidates(
  candidates: PublicAiInternalInferenceCandidate[],
): PublicAiInternalInferenceCandidate[] {
  const eligible = candidates.filter((candidate) => (
    candidate.enabled
    && candidate.route.usage_scope === "internal_background"
    && candidate.route.operation === "conversation_respond"
    && candidate.route.binding.kind === "protocol"
  ));
  return [
    ...eligible.filter(candidateSupportsDisabledReasoning),
    ...eligible.filter((candidate) => !candidateSupportsDisabledReasoning(candidate)),
  ];
}

// defaultTitleEffort turns thinking off when the model allows it, otherwise keeps the model default.
// defaultTitleEffort 在模型允许时关闭思考，否则保留模型默认值。
function defaultTitleEffort(candidate: PublicAiInternalInferenceCandidate): string {
  return (candidate.capabilities.reasoning_efforts ?? []).includes("none") ? "none" : MODEL_DEFAULT_EFFORT;
}

// SessionTitleSettingsSection switches generated session titles and picks their model and thinking.
// SessionTitleSettingsSection 开关生成的会话标题，并选择其模型与思考级别。
// Parameters: `actions` saves the switch; `enabled` is the saved switch value.
// 参数：`actions` 保存开关；`enabled` 是已保存的开关值。
export function SessionTitleSettingsSection(props: { actions: SettingsActionProps; enabled: boolean }) {
  const { t } = useI18n();
  const [pendingEnabled, setPendingEnabled] = React.useState<boolean | null>(null);
  const [switchError, setSwitchError] = React.useState("");
  const [snapshot, setSnapshot] = React.useState<PublicAiSettingsSnapshot | null>(null);
  const [loadError, setLoadError] = React.useState("");
  const [saveError, setSaveError] = React.useState("");
  const [busy, setBusy] = React.useState(false);

  React.useEffect(() => {
    let cancelled = false;
    void loadPublicAiSettings().then(
      (next) => { if (!cancelled) setSnapshot(next); },
      (cause: unknown) => { if (!cancelled) setLoadError(errorMessage(cause)); },
    );
    return () => { cancelled = true; };
  }, []);

  const candidates = React.useMemo(
    () => sessionTitleCandidates(snapshot?.internalInferenceCandidates ?? []),
    [snapshot],
  );
  const options = React.useMemo(() => {
    const disabledReasoning = new Set(candidates.filter(candidateSupportsDisabledReasoning)
      .map((candidate) => inferenceRouteKey(candidate.route)));
    return [
      { value: SESSION_MODEL, label: t("settings.sessionTitle.sessionModel"), keywords: [] },
      ...inferenceRoutePickerOptions(candidates).map((option) => disabledReasoning.has(option.value)
        ? { ...option, description: `${option.description} · ${t("settings.sessionTitle.canTurnOffThinking")}` }
        : option),
    ];
  }, [candidates, t]);

  // saveEnabled commits the switch and keeps the shown value until the request settles.
  // saveEnabled 提交开关，并在请求结束前保持显示值。
  const saveEnabled = async (enabled: boolean) => {
    if (pendingEnabled !== null) return;
    setPendingEnabled(enabled);
    setSwitchError("");
    try {
      await props.actions.onSaveSessionTitleGeneration(enabled);
    } catch (cause) {
      setSwitchError(errorMessage(cause));
    } finally {
      setPendingEnabled(null);
    }
  };

  const enabled = pendingEnabled ?? props.enabled;
  const profile = snapshot?.inferenceProfiles[SESSION_TITLE_CONSUMER_PROFILE_ID];
  const savedRoute = profile?.llm_routes?.[SESSION_TITLE_PURPOSE_ID];
  const savedEffort = profile?.llm_reasoning_efforts?.[SESSION_TITLE_PURPOSE_ID];
  const routeKey = savedRoute ? inferenceRouteKey(savedRoute) : "";
  const selected = candidates.find((candidate) => inferenceRouteKey(candidate.route) === routeKey);
  const efforts = selected?.capabilities.reasoning_efforts ?? [];

  // saveRoute commits the route and effort through the shared public-AI transaction.
  // saveRoute 通过共享公共 AI 事务提交路由与思考档位。
  const saveRoute = async (route: PublicAiInferenceRoute | null, effort: string) => {
    setBusy(true);
    setSaveError("");
    try {
      setSnapshot(await savePublicAiInferenceRoute({
        consumerProfileId: SESSION_TITLE_CONSUMER_PROFILE_ID,
        purposeId: SESSION_TITLE_PURPOSE_ID,
        operation: "conversation_respond",
        route,
        reasoningEffort: route && effort !== MODEL_DEFAULT_EFFORT
          ? effort as PublicAiReasoningEffort : undefined,
      }));
      toast.success(t("settings.sessionTitle.saved"));
    } catch (cause) {
      setSaveError(errorMessage(cause));
    } finally {
      setBusy(false);
    }
  };

  // selectRoute starts a newly picked model with thinking off when it allows that.
  // selectRoute 让新选的模型在允许时以关闭思考开始。
  const selectRoute = (key: string) => {
    if (key === SESSION_MODEL) {
      if (savedRoute) void saveRoute(null, MODEL_DEFAULT_EFFORT);
      return;
    }
    const candidate = candidates.find((item) => inferenceRouteKey(item.route) === key);
    if (!candidate || key === routeKey) return;
    void saveRoute(candidate.route, defaultTitleEffort(candidate));
  };

  return <>
    <section className="vk-form-pref" data-session-title-settings>
      <div className="vk-form-pref-text">
        <strong>{t("settings.sessionTitle.title")}</strong>
        <p className="vk-form-note">{t("settings.sessionTitle.hint")}</p>
      </div>
      <Switch
        aria-label={t("settings.sessionTitle.title")}
        checked={enabled}
        disabled={pendingEnabled !== null}
        onCheckedChange={(next) => { void saveEnabled(next); }}
      />
      {pendingEnabled !== null ? <p role="status" className="vk-form-note">{t("settings.sessionTitle.saving")}</p> : null}
      {switchError ? <SettingsErrorNotice detail={switchError} message={t("settings.sessionTitle.switchFailed")} /> : null}
    </section>
    {enabled ? <section className="vk-form-pref">
      <div className="vk-form-pref-text">
        <strong>{t("settings.sessionTitle.route")}</strong>
        <p className="vk-form-note">{t("settings.sessionTitle.recommendation")}</p>
        {!selected ? <p className="vk-form-note">{t("settings.sessionTitle.sessionModelHint")}</p> : null}
      </div>
      <Combobox
        ariaLabel={t("settings.sessionTitle.route")}
        disabled={busy || !snapshot}
        emptyText={t("settings.sessionTitle.noCandidate")}
        onValueChange={selectRoute}
        options={options}
        placeholder={t("settings.sessionTitle.sessionModel")}
        searchPlaceholder={t("settings.sessionTitle.search")}
        value={selected ? routeKey : savedRoute ? "" : SESSION_MODEL}
      />
      {savedRoute && snapshot && !selected
        ? <p role="alert" className="vk-field-error">{t("settings.sessionTitle.unavailable")}</p>
        : null}
      {loadError ? <SettingsErrorNotice detail={loadError} message={t("settings.sessionTitle.loadFailed")} /> : null}
      {saveError ? <SettingsErrorNotice detail={saveError} message={t("settings.sessionTitle.saveFailed")} /> : null}
    </section> : null}
    {enabled && selected ? <section className="vk-form-pref">
      <div className="vk-form-pref-text">
        <strong>{t("settings.sessionTitle.reasoning")}</strong>
      </div>
      <Select disabled={busy} value={savedEffort ?? MODEL_DEFAULT_EFFORT}
        onValueChange={(effort) => void saveRoute(selected.route, effort)}>
        <SelectTrigger aria-label={t("settings.sessionTitle.reasoning")} className="vk-form-w-md"><SelectValue /></SelectTrigger>
        <SelectContent>
          <SelectItem value={MODEL_DEFAULT_EFFORT}>{t("settings.sessionTitle.reasoningDefault")}</SelectItem>
          {efforts.map((effort) => (
            <SelectItem key={effort} value={effort}>{EFFORT_KEYS[effort] ? t(EFFORT_KEYS[effort]) : effort}</SelectItem>
          ))}
        </SelectContent>
      </Select>
    </section> : null}
  </>;
}
