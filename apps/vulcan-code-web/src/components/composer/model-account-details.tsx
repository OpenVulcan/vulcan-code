import * as React from "react";
import { RefreshCw } from "lucide-react";
import {
  modelAccountEffectiveProtocolId,
  modelAccountProtocolDisplayName,
  type ModelPickerChoice,
} from "@/components/composer/model-picker-control";
import { ProviderIcon } from "@/components/features/settings/provider-icon";
import { localizedProviderName } from "@/components/features/settings/provider-presentation";
import { usageCapabilitySupportsQuotaWindows } from "@/components/public-ai/credential-usage-capability";
import type { CredentialUsageOverviewProps } from "@/components/public-ai/credential-usage-overview";
import { Button } from "@/ui-kit/primitives/button";
import { StreamPlaceholder } from "@/ui-kit/primitives/stream-placeholder";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/ui-kit/primitives/tooltip";
import { useI18n } from "@/i18n";
import {
  loadPublicAiSettings,
  queryPublicAiConnectionUsage,
} from "@/service/settings-actions-service";
import type {
  PublicAiAccountUsage,
  PublicAiConnection,
  PublicAiCredentialProfile,
  PublicAiProviderDescriptor,
  PublicAiSettingsSnapshot,
} from "@/types";

// loadCredentialUsageOverview fetches the usage card module; account usage only exists after the picker reads its
// snapshot, so the module stays out of the initial bundle and is requested as that read starts.
// loadCredentialUsageOverview 加载用量卡片模块；账号用量只在选择器读取快照后才存在，
// 因此该模块不进入首屏包，并在读取开始时请求。
const loadCredentialUsageOverview = () => import("@/components/public-ai/credential-usage-overview");
const LazyCredentialUsageOverview = React.lazy(() => loadCredentialUsageOverview()
  .then((module) => ({ default: module.CredentialUsageOverview })));
const LazyCredentialUsageSummaryLine = React.lazy(() => loadCredentialUsageOverview()
  .then((module) => ({ default: module.CredentialUsageSummaryLine })));

// ModelAccountUsageOverview renders the full usage card once its module has loaded.
// ModelAccountUsageOverview 在模块加载后渲染完整用量卡片。
export function ModelAccountUsageOverview(props: CredentialUsageOverviewProps) {
  return (
    <React.Suspense fallback={<StreamPlaceholder className="py-1" shape="tiles" count={1} />}>
      <LazyCredentialUsageOverview {...props} />
    </React.Suspense>
  );
}

// ModelAccountDetails is the resolved credential and usage view of the active model account.
// ModelAccountDetails 是当前模型账号解析后的凭据与用量视图。
export interface ModelAccountDetails {
  // Exact account matching the selected connection; undefined requires explicit reselection.
  // 与当前连接精确匹配的账号；undefined 表示需要显式重新选择。
  selectedAccount: ModelPickerChoice | undefined;
  // Authoritative redacted credential metadata for the selected account.
  // 当前账号的权威脱敏凭据元数据。
  connection: PublicAiConnection | undefined;
  // Registered adapter that owns the exact credential.
  // 拥有该精确凭据的已注册适配器。
  provider: PublicAiProviderDescriptor | undefined;
  // Exact acquisition and usage contract selected by the credential.
  // 该凭据选择的精确获取与用量契约。
  credentialProfile: PublicAiCredentialProfile | undefined;
  // Latest cached or explicitly refreshed account observation.
  // 最新缓存或显式刷新的账号观测。
  usage: PublicAiAccountUsage | undefined;
  // Backend-resolved wire protocol of the exact provider/account/model route.
  // 后端为精确供应商/账号/模型路由解析出的 Wire 协议。
  effectiveProtocolId: string | null;
  // Whether the account contract exposes plan quota windows.
  // 账号契约是否公开套餐额度窗口。
  quotaWindowsSupported: boolean;
  // Whether a usage overview has anything to show: quota accounts always, balance accounts once observed.
  // 用量概览是否有内容可显示：额度账号始终显示，余额账号取得观测后显示。
  usageOverviewVisible: boolean;
  // Whether the declared credential contract authorizes an explicit provider usage query.
  // 已声明的凭据契约是否授权显式供应商用量查询。
  usageSupported: boolean;
  // Memory-snapshot read in progress.
  // 正在读取内存快照。
  loading: boolean;
  // Explicit provider usage refresh in progress.
  // 正在执行显式供应商用量刷新。
  refreshing: boolean;
  // Latest read or refresh failure for this exact account.
  // 当前精确账号最近一次读取或刷新失败。
  error: string | null;
  // Reads the latest Core-managed memory snapshot; never contacts a provider.
  // 读取最新的 Core 托管内存快照；从不联系供应商。
  load: () => Promise<void>;
  // Performs the only provider-facing action: an explicit, deduplicated usage refresh.
  // 执行唯一面向供应商的操作：显式且去重的用量刷新。
  refresh: () => Promise<void>;
}

// useModelAccountDetails resolves one exact model account and owns its snapshot read and usage refresh.
// useModelAccountDetails 解析一个精确模型账号，并持有其快照读取与用量刷新。
// Parameters: accounts are the exact credential choices of the active model; value is the selected connection id.
// 参数：accounts 为当前模型的精确凭据选项；value 为当前连接标识。
// Returns: the resolved details; responses are scoped to the exact account and component lifetime.
// 返回值：解析后的详情；响应限定到精确账号及组件生命周期。
export function useModelAccountDetails(accounts: ModelPickerChoice[], value: string): ModelAccountDetails {
  const [snapshot, setSnapshot] = React.useState<PublicAiSettingsSnapshot | null>(null);
  const [loading, setLoading] = React.useState(false);
  const [refreshing, setRefreshing] = React.useState(false);
  const [error, setError] = React.useState<string | null>(null);
  // owner scopes read and refresh responses to the exact account and component lifetime.
  // owner 将读取与刷新响应限定到精确账号及组件生命周期。
  const owner = React.useRef({ value, active: true, readVersion: 0, refreshing: false });
  if (owner.current.value !== value) {
    owner.current.active = false;
    owner.current = { value, active: true, readVersion: 0, refreshing: false };
  }
  React.useEffect(() => {
    const current = owner.current;
    current.active = true;
    setLoading(false);
    setRefreshing(false);
    setError(null);
    return () => { current.active = false; };
  }, [value]);

  const selectedAccount = accounts.find((account) => account.connectionId === value);
  const connection = snapshot?.connections.find((candidate) => candidate.id === selectedAccount?.connectionId);
  const effectiveProtocolId = modelAccountEffectiveProtocolId(snapshot, selectedAccount);
  const provider = snapshot?.providers.find((candidate) => candidate.id === connection?.provider_id);
  const entry = provider?.service_entries.find((candidate) => candidate.id === connection?.service_entry_id);
  const credentialProfile = entry?.credential_profiles.find((candidate) => candidate.id === connection?.credential_profile_id);
  const usage = connection ? snapshot?.usageCache[connection.id] : undefined;
  const quotaWindowsSupported = connection ? usageCapabilitySupportsQuotaWindows(connection.usage_capability) : false;
  const usageSupported = Boolean(
    connection
    && credentialProfile
    && credentialProfile.account_usage_query !== "unsupported"
    && (!connection.usage_requires_management_key || connection.has_management_key),
  );

  const load = React.useCallback(async () => {
    const current = owner.current;
    const version = ++current.readVersion;
    void loadCredentialUsageOverview();
    setLoading(true);
    setError(null);
    try {
      const next = await loadPublicAiSettings();
      if (current.active && current.readVersion === version) setSnapshot(next);
    } catch (reason) {
      if (current.active && current.readVersion === version) setError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      if (current.active && current.readVersion === version) setLoading(false);
    }
  }, []);

  const refresh = React.useCallback(async () => {
    const current = owner.current;
    if (!connection || !usageSupported || current.refreshing) {
      return;
    }
    current.refreshing = true;
    setRefreshing(true);
    setError(null);
    try {
      const refreshed = await queryPublicAiConnectionUsage(connection.id);
      if (!current.active) return;
      setSnapshot((existing) => existing ? {
        ...existing,
        usageCache: { ...existing.usageCache, [connection.id]: refreshed },
      } : existing);
    } catch (reason) {
      if (current.active) setError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      current.refreshing = false;
      if (current.active) setRefreshing(false);
    }
  }, [connection, usageSupported]);

  return {
    selectedAccount,
    connection,
    provider,
    credentialProfile,
    usage,
    effectiveProtocolId,
    quotaWindowsSupported,
    usageOverviewVisible: quotaWindowsSupported || Boolean(usage),
    usageSupported,
    loading,
    refreshing,
    error,
    load,
    refresh,
  };
}

// ModelAccountUsageCard renders the selected account's identity, protocol and usage with an explicit refresh.
// ModelAccountUsageCard 渲染当前账号的身份、协议与用量，并提供显式刷新。
// Parameters: details is the resolved account view and must carry a selected account.
// 参数：details 为解析后的账号视图，且必须带有当前账号。
// Returns: the usage card body; it never queries a provider on its own.
// 返回值：用量卡片主体；不会自行查询供应商。
export function ModelAccountUsageCard(props: { details: ModelAccountDetails }) {
  const { t } = useI18n();
  const { details } = props;
  const account = details.selectedAccount;
  if (!account) {
    return null;
  }
  return (
    <div className="model-account-card">
      <div className="model-account-card-header">
        <ProviderIcon
          category={details.provider?.category}
          className="size-8 shrink-0"
          providerId={details.connection?.provider_id ?? account.providerId}
          serviceEntryId={details.connection?.service_entry_id}
        />
        <div className="model-account-card-title">
          <strong>{account.accountName}</strong>
          <small>
            {details.provider
              ? localizedProviderName(details.provider.id, details.provider.display_name, t)
              : account.providerName}
          </small>
        </div>
        {details.usageSupported ? (
          <Tooltip>
            <TooltipTrigger asChild>
              <Button
                aria-label={t("composer.accountCard.refresh")}
                disabled={details.refreshing || details.loading}
                onClick={() => void details.refresh()}
                size="icon-sm"
                type="button"
                variant="ghost"
              >
                <RefreshCw className={details.refreshing ? "size-4 animate-spin" : "size-4"} />
              </Button>
            </TooltipTrigger>
            <TooltipContent side="top">{t("composer.accountCard.refresh")}</TooltipContent>
          </Tooltip>
        ) : null}
      </div>
      <div className="model-account-card-fact">
        <span>{t("composer.accountCard.protocolType")}</span>
        <strong>{details.effectiveProtocolId ? modelAccountProtocolDisplayName(details.effectiveProtocolId) : "—"}</strong>
      </div>
      {details.connection && details.usageOverviewVisible ? (
        <ModelAccountUsageOverview
          connection={details.connection}
          credentialProfile={details.credentialProfile}
          showQuotaSection={details.quotaWindowsSupported}
          usage={details.usage}
        />
      ) : !details.connection && details.loading ? (
        <StreamPlaceholder className="py-1" label={t("composer.accountCard.loading")} shape="tiles" count={2} />
      ) : !details.connection ? (
        <p className="model-account-card-empty">{t("composer.accountCard.metadataUnavailable")}</p>
      ) : null}
      {details.error ? <p className="model-account-card-error" role="status">{details.error}</p> : null}
    </div>
  );
}

// ModelAccountUsageSummary condenses plan and balance into one line for the account row.
// ModelAccountUsageSummary 将套餐与余额压缩为账号行中的一行文字。
// Parameters: connection, credentialProfile and usage are the selected account's facts.
// 参数：connection、credentialProfile 与 usage 为当前账号的事实。
// Returns: "plan · balance" using the same localized facts as the full usage card; empty until its module loads.
// 返回值：使用与完整用量卡片相同的本地化事实，格式为“套餐 · 余额”；模块加载前为空。
export function ModelAccountUsageSummary(props: {
  connection: PublicAiConnection;
  credentialProfile?: PublicAiCredentialProfile;
  usage?: PublicAiAccountUsage;
}) {
  return (
    <React.Suspense fallback={null}>
      <LazyCredentialUsageSummaryLine {...props} />
    </React.Suspense>
  );
}
