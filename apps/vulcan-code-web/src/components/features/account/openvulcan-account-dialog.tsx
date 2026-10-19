import * as React from "react";
import { ArrowUpRight, LoaderCircle, LogOut, RefreshCw } from "lucide-react";
import { Button } from "@/ui-kit/primitives/button";
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/ui-kit/primitives/dialog";
import { SettingsErrorNotice } from "@/components/features/settings/settings-error-notice";
import { VulcanSymbol } from "@/components/shell/vulcan-symbol";
import { useI18n } from "@/i18n";
import type { OpenVulcanRegion } from "@/service/openvulcan-account-service";
import { AccountAvatar } from "./account-avatar";
import { OpenVulcanAccountContext } from "./openvulcan-account-context";
import { OpenVulcanPlanSummary } from "./openvulcan-plan-summary";
import { OpenVulcanUsageHistory } from "./openvulcan-usage-history";
import "./openvulcan-account.css";

// failureCopy maps known operation stages to guidance; diagnostic text never determines behavior.
// failureCopy 将已知操作阶段映射为说明；诊断文本绝不决定行为。
const failureCopy = {
  start: ["openvulcan.account.error.start.title", "openvulcan.account.error.start.help"],
  authorize: ["openvulcan.account.error.authorize.title", "openvulcan.account.error.authorize.help"],
  connect: ["openvulcan.account.error.connect.title", "openvulcan.account.error.connect.help"],
  load: ["openvulcan.account.error.load.title", "openvulcan.account.error.load.help"],
  logout: ["openvulcan.account.error.logout.title", "openvulcan.account.error.logout.help"],
  reauthenticate: ["openvulcan.account.error.reauthenticate.title", "openvulcan.account.error.reauthenticate.help"],
} as const;

// SITES lists the two independent deployments with their public domains, in display order.
// SITES 按显示顺序列出两个独立部署及其公开域名。
const SITES = [
  { region: "china", domain: "openvulcan.cn", label: "openvulcan.account.china" },
  { region: "global", domain: "openvulcan.com", label: "openvulcan.account.global" },
] as const satisfies ReadonlyArray<{ region: OpenVulcanRegion; domain: string; label: string }>;

// defaultSite suggests the deployment matching the interface language; the user can switch before signing in.
// defaultSite 按界面语言建议部署；用户可在登录前切换。
function defaultSite(locale: string): OpenVulcanRegion {
  return locale.toLowerCase().startsWith("zh") ? "china" : "global";
}

// OpenVulcanAccountDialog presents one optional sign-in, or the current globally shared account and its usage.
// OpenVulcanAccountDialog 展示可跳过的登录，或当前全局共享账户及其用量。
// Returns no dialog outside the application account context; credentials never enter this component.
// 应用账户上下文之外不返回弹窗；凭据绝不进入本组件。
export default function OpenVulcanAccountDialog() {
  // account is the shell-owned controller; t resolves the authoritative shared catalog.
  // account 是外壳持有的控制器；t 解析权威共享目录。
  const account = React.useContext(OpenVulcanAccountContext);
  const { locale, t } = useI18n();
  // site is the deployment the next sign-in uses; a failed login keeps its own region for retry.
  // site 是下一次登录使用的部署；失败的登录保留自身区域以便重试。
  const [site, setSite] = React.useState<OpenVulcanRegion>(() => account?.error?.region ?? defaultSite(locale));
  if (!account) return null;
  // issue prefers the current action; host renewal diagnostics explicitly distinguish an unusable credential.
  // issue 优先展示当前操作；宿主续期诊断明确区分不可用凭据。
  const issue = account.error ?? (account.snapshot?.message ? {
    stage: account.snapshot.connected ? "load" as const : "reauthenticate" as const,
    detail: account.snapshot.message,
    region: null,
  } : null);
  // retryRegion is present only when a fresh login is valid for the failed operation.
  // retryRegion 仅在失败操作允许发起新登录时存在。
  const retryRegion = issue && (issue.stage === "start" || issue.stage === "authorize" || issue.stage === "connect")
    && !account.snapshot?.connectionId ? issue.region : null;
  const signedIn = account.snapshot?.connectionId ? account.snapshot : null;
  const pending = account.busy || account.waiting;
  const signedInSite = SITES.find((entry) => entry.region === signedIn?.region);
  return (
    <Dialog open={account.open} onOpenChange={account.setOpen}>
      <DialogContent size={signedIn ? "lg" : "sm"} scrollMode="body-scroll" className="openvulcan-account-dialog">
        <DialogHeader className="openvulcan-account-header">
          <VulcanSymbol className="openvulcan-account-mark" />
          <DialogTitle>{t(signedIn ? "openvulcan.account.title" : "openvulcan.account.login")}</DialogTitle>
          <DialogDescription>{t(signedIn ? "openvulcan.account.shared" : "openvulcan.account.description")}</DialogDescription>
        </DialogHeader>
        {issue && !pending ? (
          <SettingsErrorNotice
            key={issue.stage + issue.detail}
            message={<span className="grid gap-1">
              <strong className="font-medium">{t(failureCopy[issue.stage][0])}</strong>
              <span className="text-xs leading-relaxed text-muted-foreground">{t(failureCopy[issue.stage][1])}</span>
            </span>}
            detail={issue.detail}
            action={retryRegion ? (
              <Button variant="outline" size="sm" onClick={() => void account.login(retryRegion)}>
                <RefreshCw aria-hidden="true" />{t("openvulcan.account.retryLogin")}
              </Button>
            ) : issue.stage === "load" || issue.stage === "logout" ? (
              <Button variant="outline" size="sm" onClick={() => void account.refresh()}>
                <RefreshCw aria-hidden="true" />{t("openvulcan.account.refresh")}
              </Button>
            ) : undefined}
          />
        ) : null}
        {signedIn ? (
          <div className="openvulcan-account-body">
            <section className="openvulcan-account-profile">
              <AccountAvatar name={signedIn.displayName} className="openvulcan-account-avatar" />
              <div className="openvulcan-account-identity">
                <strong>{signedIn.displayName}</strong>
                <span>
                  <i data-state={signedIn.connected ? "connected" : "expired"} />
                  {t(signedIn.connected ? "openvulcan.account.connected" : "openvulcan.account.expired")}
                  {signedInSite ? <> · {signedInSite.domain}</> : null}
                </span>
              </div>
              <Button variant="ghost" size="sm" disabled={account.busy} data-openvulcan-logout="" onClick={() => void account.logout()}>
                <LogOut aria-hidden="true" />{t("openvulcan.account.logout")}
              </Button>
            </section>
            {signedIn.connected && signedIn.connectionId ? <>
              <OpenVulcanPlanSummary key={`plan:${signedIn.connectionId}`} connectionId={signedIn.connectionId} />
              <OpenVulcanUsageHistory key={signedIn.connectionId} />
            </> : null}
          </div>
        ) : (
          <div className="openvulcan-account-body">
            <Button className="openvulcan-account-login" disabled={pending} data-openvulcan-login="" onClick={() => void account.login(site)}>
              {pending ? <LoaderCircle aria-hidden="true" className="animate-spin" /> : null}
              {t("openvulcan.account.login")}
              {pending ? null : <ArrowUpRight aria-hidden="true" />}
            </Button>
            <div className="openvulcan-account-site" role="radiogroup" aria-label={t("openvulcan.account.site")}>
              <span>{t("openvulcan.account.site")}</span>
              {SITES.map((entry) => (
                <button
                  key={entry.region}
                  type="button"
                  role="radio"
                  aria-checked={site === entry.region}
                  aria-label={t(entry.label)}
                  title={t(entry.label)}
                  disabled={pending}
                  data-openvulcan-site={entry.region}
                  onClick={() => setSite(entry.region)}
                >
                  {entry.domain}
                </button>
              ))}
            </div>
            {pending ? (
              <p role="status" className="openvulcan-account-pending">
                {t(account.waiting && !account.busy ? "openvulcan.account.waiting" : "openvulcan.account.connecting")}
              </p>
            ) : null}
          </div>
        )}
        <DialogFooter>
          <Button variant="ghost" disabled={account.busy} onClick={() => account.setOpen(false)}>
            {t(signedIn ? "openvulcan.account.close" : "openvulcan.account.skip")}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
