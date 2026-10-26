import * as React from "react";
import { ArrowUpRight, LoaderCircle, LogOut, RefreshCw, X } from "lucide-react";
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
  { region: "global", domain: "openvulcan.com", label: "openvulcan.account.global" },
  { region: "china", domain: "openvulcan.cn", label: "openvulcan.account.china" },
] as const satisfies ReadonlyArray<{ region: OpenVulcanRegion; domain: string; label: string }>;

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

// OpenVulcanSignIn is the signed-out entry: one card per site that signs in there at once, and, where the
// surface can be dismissed, a third card that continues without signing in. While the browser
// authorization waits, it can be cancelled so a closed authorization page never blocks a new sign-in.
// OpenVulcanSignIn 是未登录入口：每个站点一张卡片，点击即在该站点登录；界面可关闭时再提供第三张“暂不登录”
// 卡片。浏览器授权等待期间可以取消，使已关闭的授权页不会阻塞新的登录。
// Parameters: the account controller, and onSkip for surfaces that can continue without signing in.
// 参数：账户控制器，以及可在不登录时继续的界面所提供的 onSkip。
export function OpenVulcanSignIn(props: { account: OpenVulcanAccountController; onSkip?: () => void }) {
  const { t } = useI18n();
  const { account } = props;
  const pending = account.busy || account.waiting;
  return (
    <div className="openvulcan-account-signin">
      <div className="openvulcan-account-choices" role="group" aria-label={t("openvulcan.account.choose")}>
        {SITES.map((entry) => {
          const active = pending && account.loginRegion === entry.region;
          return (
            <button
              key={entry.region}
              type="button"
              className="openvulcan-account-choice"
              aria-busy={active || undefined}
              data-active={active || undefined}
              disabled={pending}
              data-openvulcan-login={entry.region}
              onClick={() => void account.login(entry.region)}
            >
              <strong>{t(entry.label)}</strong>
              <span>{entry.domain}</span>
              {active ? <LoaderCircle aria-hidden="true" className="vk-spin" /> : <ArrowUpRight aria-hidden="true" />}
            </button>
          );
        })}
        {props.onSkip ? (
          <button type="button" className="openvulcan-account-choice" data-openvulcan-skip="" disabled={account.busy} onClick={props.onSkip}>
            <strong>{t("openvulcan.account.later")}</strong>
            <span>{t("openvulcan.account.laterHint")}</span>
          </button>
        ) : null}
      </div>
      {pending ? (
        <div role="status" className="openvulcan-account-pending">
          <span>{t(account.waiting && !account.busy ? "openvulcan.account.waiting" : "openvulcan.account.connecting")}</span>
          {account.waiting && !account.busy ? (
            <Button variant="outline" size="sm" data-openvulcan-cancel="" onClick={account.cancel}>
              <X aria-hidden="true" />{t("openvulcan.account.cancelLogin")}
            </Button>
          ) : null}
        </div>
      ) : null}
    </div>
  );
}
