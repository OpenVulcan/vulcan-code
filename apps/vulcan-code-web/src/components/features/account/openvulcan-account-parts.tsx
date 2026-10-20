import * as React from "react";
import { ArrowUpRight, LoaderCircle, LogOut, RefreshCw } from "lucide-react";
import { Button } from "@/ui-kit/primitives/button";
import { SettingsErrorNotice } from "@/components/features/settings/settings-error-notice";
import { useI18n } from "@/i18n";
import type { OpenVulcanAccountSnapshot, OpenVulcanRegion } from "@/service/openvulcan-account-service";
import { AccountAvatar } from "./account-avatar";
import type { OpenVulcanAccountController } from "./openvulcan-account-context";

// Shared pieces of the quick account dialog and the account settings page; both read one controller.
// 快捷账户弹窗与账户设置页共用的部件；两者读取同一个控制器。

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
export const SITES = [
  { region: "china", domain: "openvulcan.cn", label: "openvulcan.account.china" },
  { region: "global", domain: "openvulcan.com", label: "openvulcan.account.global" },
] as const satisfies ReadonlyArray<{ region: OpenVulcanRegion; domain: string; label: string }>;

// defaultSite suggests the deployment matching the interface language; the user can switch before signing in.
// defaultSite 按界面语言建议部署；用户可在登录前切换。
function defaultSite(locale: string): OpenVulcanRegion {
  return locale.toLowerCase().startsWith("zh") ? "china" : "global";
}

// signedInAccount returns the snapshot only when an account is stored, whether or not it is still connected.
// signedInAccount 仅在已保存账户时返回快照，无论其是否仍处于连接状态。
export function signedInAccount(account: OpenVulcanAccountController): OpenVulcanAccountSnapshot | null {
  return account.snapshot?.connectionId ? account.snapshot : null;
}

// OpenVulcanAccountIssue shows the current failure with its recovery action, or nothing while an action runs.
// OpenVulcanAccountIssue 显示当前失败及其恢复操作；操作进行中时不显示。
export function OpenVulcanAccountIssue(props: { account: OpenVulcanAccountController }) {
  const { t } = useI18n();
  const { account } = props;
  // issue prefers the current action; host renewal diagnostics explicitly distinguish an unusable credential.
  // issue 优先展示当前操作；宿主续期诊断明确区分不可用凭据。
  const issue = account.error ?? (account.snapshot?.message ? {
    stage: account.snapshot.connected ? "load" as const : "reauthenticate" as const,
    detail: account.snapshot.message,
    region: null,
  } : null);
  if (!issue || account.busy || account.waiting) return null;
  // retryRegion is present only when a fresh login is valid for the failed operation.
  // retryRegion 仅在失败操作允许发起新登录时存在。
  const retryRegion = (issue.stage === "start" || issue.stage === "authorize" || issue.stage === "connect")
    && !account.snapshot?.connectionId ? issue.region : null;
  return (
    <SettingsErrorNotice
      key={issue.stage + issue.detail}
      message={<span className="vk-field-content">
        <strong>{t(failureCopy[issue.stage][0])}</strong>
        <span className="vk-field-hint">{t(failureCopy[issue.stage][1])}</span>
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
  );
}

// OpenVulcanIdentity is the signed-in account row: initial, name, connection state, site and sign-out.
// OpenVulcanIdentity 是已登录账户行：首字符、名称、连接状态、站点与退出登录。
export function OpenVulcanIdentity(props: {
  account: OpenVulcanAccountController;
  signedIn: OpenVulcanAccountSnapshot;
  // actions are extra buttons placed before sign-out.
  // actions 是放在退出登录之前的额外按钮。
  actions?: React.ReactNode;
}) {
  const { t } = useI18n();
  const { account, signedIn } = props;
  const site = SITES.find((entry) => entry.region === signedIn.region);
  return (
    <section className="openvulcan-account-profile">
      <AccountAvatar name={signedIn.displayName} className="openvulcan-account-avatar" />
      <div className="openvulcan-account-identity">
        <strong>{signedIn.displayName}</strong>
        <span>
          <i data-state={signedIn.connected ? "connected" : "expired"} />
          {t(signedIn.connected ? "openvulcan.account.connected" : "openvulcan.account.expired")}
          {site ? <> · {site.domain}</> : null}
        </span>
      </div>
      {props.actions}
      <Button variant="ghost" size="sm" disabled={account.busy} data-openvulcan-logout="" onClick={() => void account.logout()}>
        <LogOut aria-hidden="true" />{t("openvulcan.account.logout")}
      </Button>
    </section>
  );
}

// OpenVulcanSignIn is the signed-out entry: one primary sign-in action with the site choice as a quiet line.
// OpenVulcanSignIn 是未登录入口：一个主登录操作，站点选择作为低调的次要行。
export function OpenVulcanSignIn(props: { account: OpenVulcanAccountController }) {
  const { locale, t } = useI18n();
  const { account } = props;
  // site is the deployment the next sign-in uses; a failed login keeps its own region for retry.
  // site 是下一次登录使用的部署；失败的登录保留自身区域以便重试。
  const [site, setSite] = React.useState<OpenVulcanRegion>(() => account.error?.region ?? defaultSite(locale));
  const pending = account.busy || account.waiting;
  return (
    <div className="openvulcan-account-signin">
      <Button className="openvulcan-account-login" disabled={pending} data-openvulcan-login="" onClick={() => void account.login(site)}>
        {pending ? <LoaderCircle aria-hidden="true" className="vk-spin" /> : null}
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
  );
}
