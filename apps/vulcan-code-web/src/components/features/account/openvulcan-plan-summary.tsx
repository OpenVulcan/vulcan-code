import * as React from "react";
import { LoaderCircle, RefreshCw } from "lucide-react";
import { Button } from "@/ui-kit/primitives/button";
import { SettingsErrorNotice } from "@/components/features/settings/settings-error-notice";
import { formatMessage, useI18n, type TranslationKey } from "@/i18n";
import { PageEmptyState } from "@/components/page";
import { errorMessage } from "@/lib/display-format";
import { queryPublicAiConnectionUsage } from "@/service/settings-actions-service";
import type { PublicAiAccountUsage } from "@/types/public-ai";
import { formatExpiryDate, formatMicroPoints, formatPoints, pointsToMicro, remainingShare } from "./openvulcan-usage-format";

// TIER_LABELS maps the server's documented membership tiers to catalog keys; unknown tiers show their raw name.
// TIER_LABELS 将服务端约定的会员等级映射为目录键；未知等级显示原始名称。
const TIER_LABELS: Record<string, TranslationKey> = {
  free: "openvulcan.plan.tier.free",
  star: "openvulcan.plan.tier.star",
  plus: "openvulcan.plan.tier.plus",
  pro: "openvulcan.plan.tier.pro",
};

// SOURCE_LABELS maps the server's quota lot sources to catalog keys; unknown sources show their raw name.
// SOURCE_LABELS 将服务端额度批次来源映射为目录键；未知来源显示原始名称。
const SOURCE_LABELS: Record<string, TranslationKey> = {
  free_window: "openvulcan.plan.source.free_window",
  member_window: "openvulcan.plan.source.member_window",
  serial: "openvulcan.plan.source.serial",
  admin: "openvulcan.plan.source.admin",
};

// OpenVulcanPlanSummary reads the account's plan and effective quota lots when it mounts and on refresh.
// OpenVulcanPlanSummary 在挂载与刷新时读取账户套餐及生效额度批次。
// The parent keys this component by connection ID; returns the tier, total remaining points and each lot.
// 父组件按连接 ID 设置本组件 key；返回等级、剩余积分合计与各批次。
export function OpenVulcanPlanSummary(props: {
  // Public AI connection that owns the signed-in OpenVulcan credential.
  // 持有已登录 OpenVulcan 凭据的公共 AI 连接。
  connectionId: string;
}) {
  // t and locale own copy and number formatting; usage, busy and error own the latest observation.
  // t 与 locale 负责文案与数字格式；usage、busy 与 error 持有最近一次观测。
  const { locale, t } = useI18n();
  const [usage, setUsage] = React.useState<PublicAiAccountUsage | null>(null);
  const [busy, setBusy] = React.useState(false);
  const [error, setError] = React.useState<string | null>(null);
  // version invalidates observations on unmount or a newer read.
  // version 在卸载或更新的读取时使观测失效。
  const version = React.useRef(0);
  const read = React.useCallback(async () => {
    const generation = ++version.current;
    setBusy(true);
    setError(null);
    try {
      const next = await queryPublicAiConnectionUsage(props.connectionId);
      if (generation === version.current) setUsage(next);
    } catch (cause) { if (generation === version.current) setError(errorMessage(cause)); }
    finally { if (generation === version.current) setBusy(false); }
  }, [props.connectionId]);
  React.useEffect(() => {
    void read();
    return () => { version.current += 1; };
  }, [read]);
  // remaining sums every lot exactly; lots are independent allowances, never overlapping summaries.
  // remaining 精确累加各批次；批次是相互独立的额度，不是重叠摘要。
  const remaining = React.useMemo(() => usage
    ? usage.quota_windows.reduce((total, lot) => total + (pointsToMicro(lot.remaining) ?? 0n), 0n).toString()
    : null, [usage]);
  const tier = usage?.plan_name;
  return <section className="openvulcan-plan" aria-busy={busy}>
    <header className="openvulcan-plan-header">
      <h3>{t("openvulcan.plan.title")}</h3>
      {tier ? <span className="openvulcan-plan-tier" data-tier={tier}>{TIER_LABELS[tier] ? t(TIER_LABELS[tier]) : tier}</span> : null}
      <Button variant="ghost" size="sm" disabled={busy} data-openvulcan-plan-refresh="" onClick={() => void read()}>
        {busy ? <LoaderCircle aria-hidden="true" className="vk-spin" /> : <RefreshCw aria-hidden="true" />}
        {t("openvulcan.usage.refresh")}
      </Button>
    </header>
    {error ? <SettingsErrorNotice message={t("openvulcan.account.failed")} detail={error} /> : null}
    {remaining !== null ? <p className="openvulcan-plan-balance">
      <span>{t("openvulcan.plan.remaining")}</span>
      <strong>{formatMicroPoints(remaining, locale)}</strong>
    </p> : null}
    {usage && usage.quota_windows.length > 0 ? <ul className="openvulcan-plan-lots">
      {usage.quota_windows.map((lot) => {
        const source = lot.bucket_name ?? lot.display_name;
        const share = remainingShare(lot.remaining, lot.total);
        return <li key={lot.id}>
          <div className="openvulcan-plan-lot-head">
            <span>{SOURCE_LABELS[source] ? t(SOURCE_LABELS[source]) : source}</span>
            <span>
              {lot.remaining && lot.total ? `${formatPoints(lot.remaining, locale)} / ${formatPoints(lot.total, locale)}` : null}
            </span>
          </div>
          {share !== null ? <div className="openvulcan-plan-meter" role="meter" aria-valuemin={0} aria-valuemax={100} aria-valuenow={share}>
            <i style={{ width: `${share}%` }} />
          </div> : null}
          {lot.resets_at ? <small>{formatMessage(t("openvulcan.plan.expires"), { date: formatExpiryDate(lot.resets_at, locale) })}</small> : null}
        </li>;
      })}
    </ul> : usage ? <PageEmptyState title={t("openvulcan.plan.empty")} /> : null}
  </section>;
}
