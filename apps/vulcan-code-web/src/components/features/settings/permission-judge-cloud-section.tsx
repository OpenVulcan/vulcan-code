import * as React from "react";
import { PlugZap } from "lucide-react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { Combobox } from "@/components/ui/combobox";
import { StreamPlaceholder } from "@/components/ui/stream-placeholder";
import { inferenceRouteKey, inferenceRoutePickerOptions } from "@/components/features/inference-route-picker";
import { SettingsErrorNotice } from "@/components/features/settings/settings-error-notice";
import { formatMessage, useI18n } from "@/i18n";
import { errorMessage } from "@/lib/display-format";
import {
  loadPublicAiSettings,
  savePublicAiInferenceRoute,
  testPublicAiInferenceRoute,
} from "@/service/settings-actions-service";
import type {
  PublicAiInferenceRoute,
  PublicAiInternalInferenceCandidate,
  PublicAiSettingsSnapshot,
} from "@/types";

// The cloud judge's consumer profile, owned by the service kernel.
// 云端判定的消费者配置档，由服务内核持有。
const PERMISSION_JUDGE_CONSUMER_PROFILE_ID = "permission_judge";

// permissionJudgeCloudCandidates keeps the validated internal decision routes, mirroring the
// backend's save validation.
// permissionJudgeCloudCandidates 保留已验证的内部判定路由，与后端保存校验一致。
export function permissionJudgeCloudCandidates(
  candidates: PublicAiInternalInferenceCandidate[],
): PublicAiInternalInferenceCandidate[] {
  return candidates.filter((candidate) => (
    candidate.enabled
    && candidate.route.usage_scope === "internal_background"
    && candidate.route.operation === "decision_evaluate"
    && candidate.route.binding.kind === "action"
  ));
}

// PermissionJudgeCloudSection selects the cloud decision route, states that calls leave this
// machine, and tests the saved route with one real request.
// PermissionJudgeCloudSection 选择云端判定路由，说明调用会离开本机，并用一次真实请求测试已保存路由。
// Parameters: `onConfiguredChange` receives whether a decision route is saved.
// 参数：`onConfiguredChange` 接收是否已保存判定路由。
export function PermissionJudgeCloudSection(props: { onConfiguredChange: (configured: boolean) => void }) {
  const { t } = useI18n();
  const { onConfiguredChange } = props;
  const [snapshot, setSnapshot] = React.useState<PublicAiSettingsSnapshot | null>(null);
  const [loadError, setLoadError] = React.useState("");
  const [actionError, setActionError] = React.useState<{ message: string; detail: string } | null>(null);
  const [busy, setBusy] = React.useState(false);
  const [testResult, setTestResult] = React.useState("");

  React.useEffect(() => {
    let cancelled = false;
    void loadPublicAiSettings().then(
      (next) => { if (!cancelled) setSnapshot(next); },
      (cause: unknown) => { if (!cancelled) setLoadError(errorMessage(cause)); },
    );
    return () => { cancelled = true; };
  }, []);

  const savedRoute = snapshot?.inferenceProfiles[PERMISSION_JUDGE_CONSUMER_PROFILE_ID]?.decision;
  React.useEffect(() => { onConfiguredChange(savedRoute !== undefined); }, [onConfiguredChange, savedRoute]);

  const candidates = React.useMemo(
    () => permissionJudgeCloudCandidates(snapshot?.internalInferenceCandidates ?? []),
    [snapshot],
  );
  const options = React.useMemo(() => inferenceRoutePickerOptions(candidates), [candidates]);

  if (!snapshot) {
    return <section className="settings-preferences-section">
      <h2>{t("settings.permissionJudge.cloud.title")}</h2>
      {loadError
        ? <SettingsErrorNotice detail={loadError} message={t("settings.permissionJudge.cloud.loadFailed")} />
        : <StreamPlaceholder label={t("ui.loading")} shape="rows" count={2} />}
    </section>;
  }

  const routeKey = savedRoute ? inferenceRouteKey(savedRoute) : "";
  const selected = candidates.find((candidate) => inferenceRouteKey(candidate.route) === routeKey);

  // save commits the route through the shared public-AI transaction.
  // save 通过共享公共 AI 事务提交路由。
  const save = async (route: PublicAiInferenceRoute | null) => {
    setBusy(true);
    setActionError(null);
    setTestResult("");
    try {
      setSnapshot(await savePublicAiInferenceRoute({
        consumerProfileId: PERMISSION_JUDGE_CONSUMER_PROFILE_ID,
        operation: "decision_evaluate",
        route,
      }));
      toast.success(t(route ? "settings.permissionJudge.cloud.saved" : "settings.permissionJudge.cloud.cleared"));
    } catch (cause) {
      setActionError({ message: t("settings.permissionJudge.cloud.saveFailed"), detail: errorMessage(cause) });
    } finally {
      setBusy(false);
    }
  };

  // test sends one billed request through the saved route.
  // test 经由已保存路由发送一次计费请求。
  const test = async () => {
    setBusy(true);
    setActionError(null);
    setTestResult("");
    try {
      const result = await testPublicAiInferenceRoute({
        consumerProfileId: PERMISSION_JUDGE_CONSUMER_PROFILE_ID,
        operation: "decision_evaluate",
      });
      setTestResult(formatMessage(t("settings.permissionJudge.cloud.testResult"), {
        model: result.modelId,
        duration: formatMessage(t("settings.permissionJudge.ms"), { value: result.durationMs.toFixed(0) }),
      }));
    } catch (cause) {
      setActionError({ message: t("settings.permissionJudge.cloud.testFailed"), detail: errorMessage(cause) });
    } finally {
      setBusy(false);
    }
  };

  return <section className="settings-preferences-section" data-permission-judge-cloud>
    <h2>{t("settings.permissionJudge.cloud.title")}</h2>
    <p className="settings-muted-line">{t("settings.permissionJudge.cloud.description")}</p>
    <p className="settings-muted-line" role="note">{t("settings.permissionJudge.cloud.dataNotice")}</p>
    {savedRoute && !selected
      ? <p className="settings-muted-line" role="alert">{t("settings.permissionJudge.cloud.unavailable")}</p>
      : null}
    <div className="settings-preference-row">
      <span>{t("settings.permissionJudge.cloud.route")}</span>
      <Combobox
        ariaLabel={t("settings.permissionJudge.cloud.route")}
        disabled={busy || candidates.length === 0}
        emptyText={t("settings.permissionJudge.cloud.noCandidate")}
        onValueChange={(key) => {
          const candidate = candidates.find((item) => inferenceRouteKey(item.route) === key);
          if (candidate) void save(candidate.route);
        }}
        options={options}
        placeholder={t(candidates.length === 0
          ? "settings.permissionJudge.cloud.noCandidate"
          : "settings.permissionJudge.cloud.selectRoute")}
        searchPlaceholder={t("settings.permissionJudge.cloud.search")}
        value={selected ? routeKey : ""}
      />
    </div>
    {savedRoute ? <div className="settings-card-actions">
      <Button type="button" variant="outline" size="sm" disabled={busy || !selected} onClick={() => void test()}>
        <PlugZap size={14} />{t("settings.permissionJudge.cloud.test")}
      </Button>
      <Button type="button" variant="ghost" size="sm" disabled={busy}
        onClick={() => void save(null)}>{t("settings.permissionJudge.cloud.clear")}</Button>
    </div> : null}
    {testResult ? <p className="settings-muted-line" role="status">{testResult}</p> : null}
    {actionError
      ? <SettingsErrorNotice className="settings-save-error" detail={actionError.detail} message={actionError.message} />
      : null}
  </section>;
}
