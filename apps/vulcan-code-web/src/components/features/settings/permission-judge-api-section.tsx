import * as React from "react";
import { PlugZap } from "lucide-react";
import { toast } from "@/ui-kit/primitives/toast";
import { PageNotice, PageRow, PageSection } from "@/components/page";
import { Button } from "@/ui-kit/primitives/button";
import { Combobox } from "@/ui-kit/primitives/combobox";
import { StreamPlaceholder } from "@/ui-kit/primitives/stream-placeholder";
import { inferenceRouteKey, inferenceRoutePickerOptions } from "@/components/features/inference-route-picker";
import { providerIsCoreAccount } from "@/components/features/settings/model-services/provider-directory";
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
  PublicAiProviderDescriptor,
  PublicAiSettingsSnapshot,
} from "@/types";

// The API judge's consumer profile, owned by the service kernel.
// API 判定的消费者配置档，由服务内核持有。
const PERMISSION_JUDGE_CONSUMER_PROFILE_ID = "permission_judge";

// permissionJudgeApiCandidates keeps the validated internal decision routes of providers the user
// connects themselves, mirroring the backend's save validation: OpenVulcan account routes judge only
// as the cloud judge, never as the API judge.
// permissionJudgeApiCandidates 保留用户自行接入的供应商中已验证的内部判定路由，与后端保存校验一致：
// OpenVulcan 账户的路由只能作为云端判定，不能作为 API 判定。
export function permissionJudgeApiCandidates(
  candidates: PublicAiInternalInferenceCandidate[],
  providers: readonly PublicAiProviderDescriptor[],
): PublicAiInternalInferenceCandidate[] {
  const coreAccounts = new Set(providers.filter(providerIsCoreAccount).map((provider) => provider.id));
  return candidates.filter((candidate) => (
    candidate.enabled
    && !coreAccounts.has(candidate.route.provider_id)
    && candidate.route.usage_scope === "internal_background"
    && candidate.route.operation === "decision_evaluate"
    && candidate.route.binding.kind === "action"
  ));
}

// PermissionJudgeApiSection selects the API decision route, states that calls leave this
// machine, and tests the saved route with one real request.
// PermissionJudgeApiSection 选择 API 判定路由，说明调用会离开本机，并用一次真实请求测试已保存路由。
// Parameters: `onConfiguredChange` receives whether a decision route is saved.
// 参数：`onConfiguredChange` 接收是否已保存判定路由。
export function PermissionJudgeApiSection(props: { onConfiguredChange: (configured: boolean) => void }) {
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
    () => permissionJudgeApiCandidates(snapshot?.internalInferenceCandidates ?? [], snapshot?.providers ?? []),
    [snapshot],
  );
  const options = React.useMemo(() => inferenceRoutePickerOptions(candidates), [candidates]);

  if (!snapshot) {
    return <PageSection title={t("settings.permissionJudge.api.title")}>
      {loadError
        ? <PageNotice tone="error" details={loadError} detailsLabel={t("settings.errors.showErrorDetails")}>
          {t("settings.permissionJudge.api.loadFailed")}
        </PageNotice>
        : <StreamPlaceholder label={t("ui.loading")} shape="rows" count={2} />}
    </PageSection>;
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
      toast.success(t(route ? "settings.permissionJudge.api.saved" : "settings.permissionJudge.api.cleared"));
    } catch (cause) {
      setActionError({ message: t("settings.permissionJudge.api.saveFailed"), detail: errorMessage(cause) });
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
      setTestResult(formatMessage(t("settings.permissionJudge.api.testResult"), {
        model: result.modelId,
        duration: formatMessage(t("settings.permissionJudge.ms"), { value: result.durationMs.toFixed(0) }),
      }));
    } catch (cause) {
      setActionError({ message: t("settings.permissionJudge.api.testFailed"), detail: errorMessage(cause) });
    } finally {
      setBusy(false);
    }
  };

  return <PageSection layout="rows" data-permission-judge-api title={t("settings.permissionJudge.api.title")}
    description={t("settings.permissionJudge.api.description")}
    footer={savedRoute ? <>
      <Button type="button" variant="outline" size="sm" disabled={busy || !selected} onClick={() => void test()}>
        <PlugZap size={14} />{t("settings.permissionJudge.api.test")}
      </Button>
      <Button type="button" variant="ghost" size="sm" disabled={busy}
        onClick={() => void save(null)}>{t("settings.permissionJudge.api.clear")}</Button>
    </> : undefined}>
    <PageNotice tone="warning" role="note">{t("settings.permissionJudge.api.dataNotice")}</PageNotice>
    {savedRoute && !selected
      ? <PageNotice tone="error">{t("settings.permissionJudge.api.unavailable")}</PageNotice>
      : null}
    <PageRow label={t("settings.permissionJudge.api.route")}>
      <Combobox
        ariaLabel={t("settings.permissionJudge.api.route")}
        disabled={busy || candidates.length === 0}
        emptyText={t("settings.permissionJudge.api.noCandidate")}
        onValueChange={(key) => {
          const candidate = candidates.find((item) => inferenceRouteKey(item.route) === key);
          if (candidate) void save(candidate.route);
        }}
        options={options}
        placeholder={t(candidates.length === 0
          ? "settings.permissionJudge.api.noCandidate"
          : "settings.permissionJudge.api.selectRoute")}
        searchPlaceholder={t("settings.permissionJudge.api.search")}
        value={selected ? routeKey : ""}
      />
    </PageRow>
    {testResult ? <PageNotice tone="success" data-api-test-result="">{testResult}</PageNotice> : null}
    {actionError
      ? <PageNotice tone="error" details={actionError.detail} detailsLabel={t("settings.errors.showErrorDetails")}>
        {actionError.message}
      </PageNotice>
      : null}
  </PageSection>;
}
