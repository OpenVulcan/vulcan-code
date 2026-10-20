import * as React from "react";
import { ArrowRight } from "lucide-react";
import { Button } from "@/ui-kit/primitives/button";
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/ui-kit/primitives/dialog";
import { VulcanSymbol } from "@/components/shell/vulcan-symbol";
import { useI18n } from "@/i18n";
import { OpenVulcanAccountContext } from "./openvulcan-account-context";
import { OpenVulcanAccountIssue, OpenVulcanIdentity, OpenVulcanSignIn, signedInAccount } from "./openvulcan-account-parts";
import { OpenVulcanPlanSummary } from "./openvulcan-plan-summary";
import { OpenVulcanUsageStatsPanel } from "./openvulcan-usage-stats-panel";
import "./openvulcan-account.css";

// OpenVulcanAccountDialog is the quick account view: one optional sign-in, or the current globally shared
// account with its plan and usage statistics. Hourly call records live on the account settings page.
// OpenVulcanAccountDialog 是快捷账户视图：可跳过的登录，或当前全局共享账户及其套餐与用量统计；
// 小时调用明细位于账户设置页。
// Returns no dialog outside the application account context; credentials never enter this component.
// 应用账户上下文之外不返回弹窗；凭据绝不进入本组件。
export default function OpenVulcanAccountDialog() {
  // account is the shell-owned controller; t resolves the authoritative shared catalog.
  // account 是外壳持有的控制器；t 解析权威共享目录。
  const account = React.useContext(OpenVulcanAccountContext);
  const { t } = useI18n();
  if (!account) return null;
  const signedIn = signedInAccount(account);
  return (
    <Dialog open={account.open} onOpenChange={account.setOpen}>
      <DialogContent size={signedIn ? "lg" : "sm"} scrollMode="body-scroll" className="openvulcan-account-dialog">
        <DialogHeader className="openvulcan-account-header">
          <VulcanSymbol className="openvulcan-account-mark" />
          <DialogTitle>{t(signedIn ? "openvulcan.account.title" : "openvulcan.account.login")}</DialogTitle>
          <DialogDescription>{t(signedIn ? "openvulcan.account.shared" : "openvulcan.account.description")}</DialogDescription>
        </DialogHeader>
        <OpenVulcanAccountIssue account={account} />
        {signedIn ? (
          <div className="openvulcan-account-body">
            <OpenVulcanIdentity account={account} signedIn={signedIn} />
            {signedIn.connected && signedIn.connectionId ? <>
              <OpenVulcanPlanSummary key={`plan:${signedIn.connectionId}`} connectionId={signedIn.connectionId} />
              <OpenVulcanUsageStatsPanel key={signedIn.connectionId} />
            </> : null}
          </div>
        ) : (
          <div className="openvulcan-account-body">
            <OpenVulcanSignIn account={account} />
          </div>
        )}
        <DialogFooter>
          {signedIn && account.openDetails ? (
            <Button variant="outline" disabled={account.busy} data-openvulcan-details="" onClick={account.openDetails}>
              {t("openvulcan.account.details")}<ArrowRight aria-hidden="true" />
            </Button>
          ) : null}
          <Button variant="ghost" disabled={account.busy} onClick={() => account.setOpen(false)}>
            {t(signedIn ? "openvulcan.account.close" : "openvulcan.account.skip")}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
