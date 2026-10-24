import * as React from "react";
import { RefreshCw } from "lucide-react";
import { ProviderModelCatalog } from "@/components/features/settings/provider-model-settings";
import { providerIsCoreAccount } from "@/components/features/settings/model-services/provider-directory";
import { useSnapshotMutations } from "@/components/features/settings/model-services/use-snapshot-mutations";
import { SettingsErrorNotice } from "@/components/features/settings/settings-error-notice";
import { PageEmptyState, PageTabPanel, PageTabs } from "@/components/page";
import { VulcanSymbol } from "@/components/shell/vulcan-symbol";
import { Button } from "@/ui-kit/primitives/button";
import { StreamPlaceholder } from "@/ui-kit/primitives/stream-placeholder";
import { useI18n } from "@/i18n";
import { errorMessage } from "@/lib/display-format";
import { loadPublicAiSettings } from "@/service/settings-actions-service";
import type { PublicAiSettingsSnapshot } from "@/types";
import { OpenVulcanAccountContext } from "./openvulcan-account-context";
import { OpenVulcanAccountIssue, OpenVulcanIdentity, OpenVulcanSignIn, signedInAccount } from "./openvulcan-account-parts";
import { OpenVulcanJudgeModels } from "./openvulcan-judge-models";
import { OpenVulcanPlanSummary } from "./openvulcan-plan-summary";
import { OpenVulcanUsageHistory } from "./openvulcan-usage-history";
import { OpenVulcanUsageStatsPanel } from "./openvulcan-usage-stats-panel";
import "./openvulcan-account.css";

// AccountPageTab is one section of the signed-in account page.
// AccountPageTab 是已登录账户页的一个分区。
type AccountPageTab = "overview" | "models" | "calls";

// OpenVulcanAccountArea is the OpenVulcan account settings page. One installation holds at most one account,
// so the page leads with that account (or its sign-in) instead of a provider list, then offers the overview
// of plan and usage statistics, the OpenVulcan model switches and the hourly call details.
// OpenVulcanAccountArea 是 OpenVulcan 账户设置页。一个安装最多只有一个账户，因此页面以该账户（或其登录入口）
// 开头而不是供应商列表，随后提供套餐与用量统计概览、OpenVulcan 模型开关以及按小时的调用详情。
export function OpenVulcanAccountArea() {
  const { t } = useI18n();
  const account = React.useContext(OpenVulcanAccountContext);
  const [tab, setTab] = React.useState<AccountPageTab>("overview");
  if (!account) return <PageEmptyState title={t("openvulcan.account.failed")} />;
  const signedIn = signedInAccount(account);
  const ready = signedIn?.connected && signedIn.connectionId ? signedIn.connectionId : null;
  return (
    <div className="openvulcan-page">
      <OpenVulcanAccountIssue account={account} />
      {signedIn ? (
        <OpenVulcanIdentity
          account={account}
          signedIn={signedIn}
          actions={<Button variant="ghost" size="sm" disabled={account.busy} onClick={() => void account.refresh()}>
            <RefreshCw aria-hidden="true" />{t("openvulcan.account.refresh")}
          </Button>}
        />
      ) : (
        <section className="openvulcan-page-signin">
          <VulcanSymbol className="openvulcan-account-mark" />
          <p>{t("openvulcan.page.signinHint")}</p>
          <OpenVulcanSignIn account={account} />
        </section>
      )}
      <p className="openvulcan-page-note">{t("openvulcan.account.single")}</p>
      {ready ? <>
        <PageTabs
          ariaLabel={t("settings.openvulcanAccount")}
          onValueChange={setTab}
          options={[
            { value: "overview", label: t("openvulcan.page.overview") },
            { value: "models", label: t("openvulcan.page.models") },
            { value: "calls", label: t("openvulcan.page.calls") },
          ]}
          value={tab}
        />
        <PageTabPanel active={tab === "overview"}>
          <div className="openvulcan-page-overview">
            <OpenVulcanPlanSummary key={`plan:${ready}`} connectionId={ready} />
            <OpenVulcanUsageStatsPanel key={`stats:${ready}`} />
          </div>
        </PageTabPanel>
        <PageTabPanel active={tab === "models"}>
          <div className="openvulcan-page-models-stack">
            <OpenVulcanJudgeModels key={`judge:${ready}`} />
            <OpenVulcanModels key={`models:${ready}`} />
          </div>
        </PageTabPanel>
        <PageTabPanel active={tab === "calls"}>
          <OpenVulcanUsageHistory key={`calls:${ready}`} />
        </PageTabPanel>
      </> : null}
    </div>
  );
}

// OpenVulcanModels lists the core account's models with the same switches as the model services page.
// It owns its own settings snapshot and serialized write lane.
// OpenVulcanModels 以与模型服务页相同的开关列出核心账户的模型；自身持有设置快照与串行写入通道。
function OpenVulcanModels() {
  const { t } = useI18n();
  const [snapshot, setSnapshot] = React.useState<PublicAiSettingsSnapshot | null>(null);
  const [loadError, setLoadError] = React.useState<string | null>(null);
  const mutations = useSnapshotMutations(setSnapshot);
  const reload = React.useCallback(async () => {
    setLoadError(null);
    try { setSnapshot(await loadPublicAiSettings()); }
    catch (cause) { setLoadError(errorMessage(cause)); }
  }, []);
  React.useEffect(() => { void reload(); }, [reload]);
  const providerIds = React.useMemo(
    () => snapshot?.providers.filter(providerIsCoreAccount).map((provider) => provider.id) ?? [],
    [snapshot],
  );
  return (
    <section className="openvulcan-page-models">
      {mutations.error ? (
        <SettingsErrorNotice
          action={<Button onClick={mutations.clearError} size="sm" type="button" variant="ghost">{t("settings.modelServices.dismiss")}</Button>}
          detail={mutations.error}
          message={t("settings.modelServices.saveFailed")}
        />
      ) : null}
      {loadError ? (
        <SettingsErrorNotice
          action={<Button onClick={() => void reload()} size="sm" type="button" variant="outline">{t("settings.modelExperience.retryLoad")}</Button>}
          detail={loadError}
          message={t("settings.modelServices.loadFailed")}
          onServiceReconnected={() => void reload()}
        />
      ) : null}
      {!snapshot ? (
        loadError ? null : <StreamPlaceholder decorative shape="rows" count={6} />
      ) : providerIds.length === 0 ? (
        <PageEmptyState title={t("openvulcan.models.unavailable")} />
      ) : (
        <ProviderModelCatalog
          accountScope={null}
          mutations={mutations}
          onSnapshot={setSnapshot}
          providerIds={providerIds}
          showProvider={false}
          snapshot={snapshot}
        />
      )}
    </section>
  );
}
