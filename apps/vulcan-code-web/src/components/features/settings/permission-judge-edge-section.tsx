import * as React from "react";
import { LogIn } from "lucide-react";
import { PageNotice, PageRow, PageSection } from "@/components/page";
import { StatusPill } from "@/components/shared/status";
import { Button } from "@/ui-kit/primitives/button";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/ui-kit/primitives/select";
import { OpenVulcanAccountContext } from "@/components/features/account/openvulcan-account-context";
import { formatMessage, type Translate } from "@/i18n";
import type { PermissionJudgeKnownPackage } from "@/types";

// edgePackages lists the official packages that have a direct route, in display order.
// edgePackages 按显示顺序列出具有直连路由的官方模型包。
export function edgePackages(knownPackages: readonly PermissionJudgeKnownPackage[]): PermissionJudgeKnownPackage[] {
  return knownPackages.filter((known) => known.edgeModelId !== null);
}

// PermissionJudgeEdgeSection sets up the official direct judge: the OpenVulcan account it signs in
// with and the official model it calls. Calls leave this machine, which the notice states first.
// PermissionJudgeEdgeSection 设置官方直连判定：用于登录的 OpenVulcan 账户及其调用的官方模型。调用会
// 离开本机，提示先说明这一点。
// Parameters: the official packages, the chosen edge model, whether controls are locked and the
// change callback. Returns the section.
// 参数：官方模型包、所选边缘模型、控件是否锁定与变更回调。返回该区块。
export function PermissionJudgeEdgeSection(props: {
  knownPackages: readonly PermissionJudgeKnownPackage[];
  edgeModelId: string | null;
  disabled: boolean;
  t: Translate;
  onEdgeModelChange: (edgeModelId: string) => void;
}) {
  const { t } = props;
  // account is the shared OpenVulcan account; absent outside the workbench shell.
  // account 是共享的 OpenVulcan 账户；工作台外壳之外为空。
  const account = React.useContext(OpenVulcanAccountContext);
  const signedIn = account?.snapshot?.connected === true;
  const packages = edgePackages(props.knownPackages);
  return <PageSection layout="rows" data-judge-edge="" title={t("settings.permissionJudge.backend.edge")}
    description={t("settings.permissionJudge.edge.description")}>
    <PageNotice tone="info">{t("settings.permissionJudge.edge.dataNotice")}</PageNotice>
    <PageRow label={t("settings.permissionJudge.edge.account")} description={signedIn
      ? formatMessage(t("settings.permissionJudge.edge.signedIn"), { name: account?.snapshot?.displayName ?? "" })
      : t("settings.permissionJudge.edge.signedOut")}>
      {signedIn
        ? <StatusPill status="ready" label={t("settings.permissionJudge.edge.connected")} />
        : <Button type="button" size="sm" disabled={props.disabled || !account} data-judge-edge-sign-in=""
          onClick={() => account?.setOpen(true)}>
          <LogIn size={14} />{t("settings.permissionJudge.edge.signIn")}
        </Button>}
    </PageRow>
    <PageRow label={t("settings.permissionJudge.edge.model")}
      description={packages.length === 0 ? t("settings.permissionJudge.edge.noModel") : undefined}>
      {packages.length > 0 ? <Select disabled={props.disabled} value={props.edgeModelId ?? undefined}
        onValueChange={props.onEdgeModelChange}>
        <SelectTrigger aria-label={t("settings.permissionJudge.edge.model")} className="vk-form-w-lg"><SelectValue /></SelectTrigger>
        <SelectContent>
          {packages.map((known) => (
            <SelectItem key={known.id} value={known.edgeModelId!}>{known.displayName}</SelectItem>
          ))}
        </SelectContent>
      </Select> : null}
    </PageRow>
  </PageSection>;
}
