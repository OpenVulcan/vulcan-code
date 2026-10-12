import * as React from "react";
import { ArrowUpRight, CircleUserRound, Globe2, LoaderCircle, RefreshCw, ShieldCheck } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { SettingsErrorNotice } from "@/components/features/settings/settings-error-notice";
import { useI18n } from "@/i18n";
import { OpenVulcanAccountContext } from "./openvulcan-account-context";
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

// OpenVulcanAccountDialog presents one optional regional login or the current globally shared account.
// OpenVulcanAccountDialog 展示可跳过的区域登录，或当前全局共享账户。
// Returns no dialog outside the application account context; credentials never enter this component.
// 应用账户上下文之外不返回弹窗；凭据绝不进入本组件。
export default function OpenVulcanAccountDialog() {
  // account is the shell-owned controller; t resolves the authoritative shared catalog.
  // account 是外壳持有的控制器；t 解析权威共享目录。
  const account = React.useContext(OpenVulcanAccountContext);
  const { t } = useI18n();
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
  return (
    <Dialog open={account.open} onOpenChange={account.setOpen}>
      <DialogContent size="md" scrollMode="body-scroll" className="openvulcan-account-columns">
        <DialogHeader className="min-w-0 pr-6">
          <span className="mb-2 flex size-10 items-center justify-center rounded-xl bg-primary/10 text-primary">
            <ShieldCheck aria-hidden="true" className="size-5" />
          </span>
          <DialogTitle>{t("openvulcan.account.title")}</DialogTitle>
          <DialogDescription className="leading-relaxed">
            {t(account.snapshot?.connectionId ? "openvulcan.account.shared" : "openvulcan.account.description")}
          </DialogDescription>
        </DialogHeader>
        {issue && !account.busy && !account.waiting ? (
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
        {account.snapshot?.connectionId ? (
          <div className="openvulcan-account-columns grid min-w-0 gap-3">
            <div className="flex min-w-0 items-center gap-3 rounded-lg border p-3">
              <CircleUserRound aria-hidden="true" className="size-6 shrink-0 text-muted-foreground" />
              <div className="grid min-w-0 gap-1">
                <strong className="font-medium">{account.snapshot.displayName}</strong>
                <span className="text-xs text-muted-foreground">{t(account.snapshot.region === "china" ? "openvulcan.account.china" : "openvulcan.account.global")}</span>
              </div>
            </div>
            {account.snapshot.connected ? <OpenVulcanUsageHistory key={account.snapshot.connectionId} /> : null}
            <Button variant="outline" disabled={account.busy} onClick={() => void account.logout()}>
              {t("openvulcan.account.logout")}
            </Button>
          </div>
        ) : (
          <div className="openvulcan-account-columns grid min-w-0 gap-3">
            <Button className="h-auto min-w-0 justify-start gap-3 whitespace-normal py-3 text-left" disabled={account.busy || account.waiting} onClick={() => void account.login("china")}>
              <Globe2 aria-hidden="true" />
              <span className="grid min-w-0 flex-1 gap-0.5">
                <span>{t("openvulcan.account.china")}</span>
                <span className="text-xs font-normal">openvulcan.cn</span>
              </span>
              <ArrowUpRight aria-hidden="true" />
            </Button>
            <Button variant="outline" className="h-auto min-w-0 justify-start gap-3 whitespace-normal py-3 text-left" disabled={account.busy || account.waiting} onClick={() => void account.login("global")}>
              <Globe2 aria-hidden="true" />
              <span className="grid min-w-0 flex-1 gap-0.5">
                <span>{t("openvulcan.account.global")}</span>
                <span className="text-xs font-normal text-muted-foreground">openvulcan.com</span>
              </span>
              <ArrowUpRight aria-hidden="true" />
            </Button>
            {account.waiting || account.busy ? (
              <p role="status" className="flex min-w-0 items-start gap-2 rounded-lg bg-muted p-3 text-sm leading-relaxed text-muted-foreground">
                <LoaderCircle aria-hidden="true" className="mt-0.5 size-4 shrink-0 animate-spin" />
                {t(account.waiting && !account.busy ? "openvulcan.account.waiting" : "openvulcan.account.connecting")}
              </p>
            ) : null}
          </div>
        )}
        <DialogFooter>
          <Button variant="ghost" disabled={account.busy} onClick={() => account.setOpen(false)}>
            {t(account.snapshot?.connectionId ? "openvulcan.account.close" : "openvulcan.account.skip")}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

