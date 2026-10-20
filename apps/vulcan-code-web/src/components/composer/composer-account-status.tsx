import * as React from "react";
import { createPortal } from "react-dom";
import { ProviderIcon } from "@/components/features/settings/provider-icon";
import { StatusPeek } from "@/components/shell/status-peek";
import { useI18n } from "@/i18n";
import { ComposerAccountHostContext } from "./composer-settings-host";
import { ModelAccountUsageCard, ModelAccountUsageSummary, type ModelAccountDetails } from "./model-account-details";

// ComposerAccountStatus puts the active model account and its plan in the status bar. The item reads the
// locally cached usage once on mount; its card shows the whole plan and offers a manual refresh, and the
// card's settings button opens the model services page.
// ComposerAccountStatus 将当前模型账号及其套餐放入状态栏。该项在挂载时读取一次本地缓存的用量；卡片显示完整套餐并提供
// 手动刷新，卡片中的设置按钮打开模型服务页面。
export function ComposerAccountStatus(props: { account: ModelAccountDetails }) {
  const { t } = useI18n();
  const target = React.useContext(ComposerAccountHostContext);
  const { account } = props;
  const selected = account.selectedAccount;
  const load = account.load;
  const connectionId = selected?.connectionId ?? null;
  React.useEffect(() => {
    if (target.host && connectionId) void load();
  }, [connectionId, load, target.host]);
  if (!target.host || !selected) return null;
  return createPortal(
    <StatusPeek
      card={<ModelAccountUsageCard details={account} />}
      label={`${t("status.account.title")} · ${selected.accountName}`}
      onOpen={() => void load()}
      onOpenSettings={target.onOpenSettings}
      optional
      settingsLabel={t("settings.modelProviders")}
      title={t("status.account.title")}
      wide
    >
      <ProviderIcon
        category={account.provider?.category}
        providerId={account.connection?.provider_id ?? selected.providerId}
        serviceEntryId={account.connection?.service_entry_id}
        size={14}
      />
      <span>{selected.accountName}</span>
      {account.connection && account.usage ? (
        <span className="vk-status-plan">
          <ModelAccountUsageSummary
            connection={account.connection}
            credentialProfile={account.credentialProfile}
            usage={account.usage}
          />
        </span>
      ) : null}
    </StatusPeek>,
    target.host,
  );
}
