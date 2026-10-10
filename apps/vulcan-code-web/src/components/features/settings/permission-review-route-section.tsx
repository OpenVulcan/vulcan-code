import * as React from "react";
import { toast } from "sonner";
import { PageNotice, PageRow, PageSection } from "@/components/page";
import { Button } from "@/components/ui/button";
import { Combobox } from "@/components/ui/combobox";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { StreamPlaceholder } from "@/components/ui/stream-placeholder";
import { inferenceRouteKey, inferenceRoutePickerOptions } from "@/components/features/inference-route-picker";
import { useI18n, type TranslationKey } from "@/i18n";
import { errorMessage } from "@/lib/display-format";
import { loadPublicAiSettings, savePublicAiInferenceRoute } from "@/service/settings-actions-service";
import type {
  PublicAiInferenceRoute,
  PublicAiInternalInferenceCandidate,
  PublicAiReasoningEffort,
  PublicAiSettingsSnapshot,
} from "@/types";

// The reviewer's consumer profile and its single LLM purpose, owned by the service kernel.
// 复核器的消费者配置档及其唯一 LLM 用途，由服务内核持有。
const PERMISSION_REVIEW_CONSUMER_PROFILE_ID = "permission_review";
const PERMISSION_REVIEW_PURPOSE_ID = "tool_call_review";

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

// permissionReviewCandidates keeps the validated internal routes that can answer with JSON,
// mirroring the backend's save validation.
// permissionReviewCandidates 保留能以 JSON 作答的已验证内部路由，与后端保存校验一致。
export function permissionReviewCandidates(
  candidates: PublicAiInternalInferenceCandidate[],
): PublicAiInternalInferenceCandidate[] {
  return candidates.filter((candidate) => (
    candidate.enabled
    && candidate.route.usage_scope === "internal_background"
    && candidate.route.operation === "conversation_respond"
    && candidate.route.binding.kind === "protocol"
    && (candidate.capabilities.structured_output === "json_schema"
      || candidate.capabilities.structured_output === "json_object")
  ));
}

// PermissionReviewRouteSection selects the reviewing LLM route and its reasoning effort.
// PermissionReviewRouteSection 选择复核 LLM 路由及其思考档位。
export function PermissionReviewRouteSection() {
  const { t } = useI18n();
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
    () => permissionReviewCandidates(snapshot?.internalInferenceCandidates ?? []),
    [snapshot],
  );
  const options = React.useMemo(() => inferenceRoutePickerOptions(candidates), [candidates]);

  if (!snapshot) {
    return <PageSection title={t("settings.permissionJudge.review.title")}>
      {loadError
        ? <PageNotice tone="error" details={loadError} detailsLabel={t("settings.errors.showErrorDetails")}>
          {t("settings.permissionJudge.review.loadFailed")}
        </PageNotice>
        : <StreamPlaceholder label={t("ui.loading")} shape="rows" count={2} />}
    </PageSection>;
  }

  const profile = snapshot.inferenceProfiles[PERMISSION_REVIEW_CONSUMER_PROFILE_ID];
  const savedRoute = profile?.llm_routes?.[PERMISSION_REVIEW_PURPOSE_ID];
  const savedEffort = profile?.llm_reasoning_efforts?.[PERMISSION_REVIEW_PURPOSE_ID];
  const routeKey = savedRoute ? inferenceRouteKey(savedRoute) : "";
  const selected = candidates.find((candidate) => inferenceRouteKey(candidate.route) === routeKey);
  const efforts = selected?.capabilities.reasoning_efforts ?? [];

  // save commits the route and effort through the shared public-AI transaction.
  // save 通过共享公共 AI 事务提交路由与思考档位。
  const save = async (route: PublicAiInferenceRoute | null, effort: string) => {
    setBusy(true);
    setSaveError("");
    try {
      setSnapshot(await savePublicAiInferenceRoute({
        consumerProfileId: PERMISSION_REVIEW_CONSUMER_PROFILE_ID,
        purposeId: PERMISSION_REVIEW_PURPOSE_ID,
        operation: "conversation_respond",
        route,
        reasoningEffort: route && effort !== MODEL_DEFAULT_EFFORT
          ? effort as PublicAiReasoningEffort : undefined,
      }));
      toast.success(t(route ? "settings.permissionJudge.review.saved" : "settings.permissionJudge.review.cleared"));
    } catch (cause) {
      setSaveError(errorMessage(cause));
    } finally {
      setBusy(false);
    }
  };

  // selectRoute keeps the current effort only when the new model declares it.
  // selectRoute 仅在新模型声明当前档位时保留它。
  const selectRoute = (key: string) => {
    const candidate = candidates.find((item) => inferenceRouteKey(item.route) === key);
    if (!candidate) return;
    const keep = savedEffort && (candidate.capabilities.reasoning_efforts ?? []).includes(savedEffort);
    void save(candidate.route, keep ? savedEffort : MODEL_DEFAULT_EFFORT);
  };

  return <PageSection layout="rows" data-permission-review title={t("settings.permissionJudge.review.title")}
    description={t("settings.permissionJudge.review.description")}
    footer={savedRoute ? <Button type="button" size="sm" variant="outline" disabled={busy}
      onClick={() => void save(null, MODEL_DEFAULT_EFFORT)}>{t("settings.permissionJudge.review.clear")}</Button> : undefined}>
    {savedRoute && !selected
      ? <PageNotice tone="error">{t("settings.permissionJudge.review.unavailable")}</PageNotice>
      : null}
    <PageRow label={t("settings.permissionJudge.review.route")}>
      <Combobox
        ariaLabel={t("settings.permissionJudge.review.route")}
        disabled={busy || candidates.length === 0}
        emptyText={t("settings.permissionJudge.review.noCandidate")}
        onValueChange={selectRoute}
        options={options}
        placeholder={t(candidates.length === 0
          ? "settings.permissionJudge.review.noCandidate"
          : "settings.permissionJudge.review.selectRoute")}
        searchPlaceholder={t("settings.permissionJudge.review.search")}
        value={selected ? routeKey : ""}
      />
    </PageRow>
    {selected ? <PageRow label={t("settings.permissionJudge.review.reasoning")}>
      <Select disabled={busy} value={savedEffort ?? MODEL_DEFAULT_EFFORT}
        onValueChange={(effort) => void save(selected.route, effort)}>
        <SelectTrigger aria-label={t("settings.permissionJudge.review.reasoning")} className="w-56"><SelectValue /></SelectTrigger>
        <SelectContent>
          <SelectItem value={MODEL_DEFAULT_EFFORT}>{t("settings.permissionJudge.review.reasoningDefault")}</SelectItem>
          {efforts.map((effort) => (
            <SelectItem key={effort} value={effort}>{EFFORT_KEYS[effort] ? t(EFFORT_KEYS[effort]) : effort}</SelectItem>
          ))}
        </SelectContent>
      </Select>
    </PageRow> : null}
    {saveError
      ? <PageNotice tone="error" details={saveError} detailsLabel={t("settings.errors.showErrorDetails")}>
        {t("settings.permissionJudge.review.saveFailed")}
      </PageNotice>
      : null}
  </PageSection>;
}
