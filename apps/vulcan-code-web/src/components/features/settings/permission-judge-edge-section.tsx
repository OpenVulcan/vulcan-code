import * as React from "react";
import { LogIn, RefreshCw } from "lucide-react";
import { PageNotice, PageRow, PageSection } from "@/components/page";
import { StatusPill } from "@/components/shared/status";
import { Button } from "@/ui-kit/primitives/button";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/ui-kit/primitives/select";
import { OpenVulcanAccountContext } from "@/components/features/account/openvulcan-account-context";
import { formatMessage, type Translate } from "@/i18n";

// EdgeModelList is the cloud model list as last read from the service: not read while
// signed out, loading, the enabled model ids sorted by id, or the read failure.
// EdgeModelList 是最近一次从服务读取的云端模型清单：未登录时不读取、加载中、按 id 排序的已启用模型
// id，或读取失败。
export type EdgeModelList =
  | { phase: "signedOut" }
  | { phase: "loading" }
  | { phase: "ready"; models: string[] }
  | { phase: "failed"; error: string };

// edgeModelEnabled tells whether the server currently enables `modelId`.
// edgeModelEnabled 判断服务端当前是否启用 `modelId`。
export function edgeModelEnabled(list: EdgeModelList, modelId: string | null): boolean {
  return modelId !== null && list.phase === "ready" && list.models.includes(modelId);
}

// PermissionJudgeEdgeSection sets up the cloud judge: the OpenVulcan account it signs in
// with and the official model it calls, chosen among the models the server currently enables. A
// chosen model the server no longer enables stays visible and is marked unavailable. Calls leave
// this machine, which the notice states first.
// PermissionJudgeEdgeSection 设置云端判定：用于登录的 OpenVulcan 账户及其调用的官方模型，从服务端
// 当前启用的模型中选择。服务端已不再启用的所选模型保持可见并标为不可用。调用会离开本机，提示先说明这一点。
// Parameters: the model list, the chosen edge model, whether controls are locked, the change
// callback and the list retry. Returns the section.
// 参数：模型清单、所选边缘模型、控件是否锁定、变更回调与清单重试。返回该区块。
export function PermissionJudgeEdgeSection(props: {
  edgeModels: EdgeModelList;
  edgeModelId: string | null;
  disabled: boolean;
  t: Translate;
  onEdgeModelChange: (edgeModelId: string) => void;
  onRetry: () => void;
}) {
  const { t, edgeModels, edgeModelId } = props;
  // account is the shared OpenVulcan account; absent outside the workbench shell.
  // account 是共享的 OpenVulcan 账户；工作台外壳之外为空。
  const account = React.useContext(OpenVulcanAccountContext);
  const signedIn = account?.snapshot?.connected === true;
  const enabled = edgeModels.phase === "ready" ? edgeModels.models : [];
  // unavailable is the chosen model when the server no longer enables it.
  // unavailable 是服务端已不再启用的所选模型。
  const unavailable = edgeModels.phase === "ready" && edgeModelId !== null && !enabled.includes(edgeModelId)
    ? edgeModelId
    : null;
  const modelHint = edgeModels.phase === "signedOut" ? t("settings.permissionJudge.edge.modelsSignIn")
    : edgeModels.phase === "loading" ? t("settings.permissionJudge.edge.modelsLoading")
      : edgeModels.phase === "ready" && enabled.length === 0 ? t("settings.permissionJudge.edge.noModel") : undefined;
  return <PageSection layout="rows" data-judge-edge="" title={t("settings.permissionJudge.backend.cloud")}
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
    <PageRow label={t("settings.permissionJudge.edge.model")} description={modelHint}
      data-judge-edge-models={edgeModels.phase}>
      {edgeModels.phase === "failed"
        ? <Button type="button" size="sm" variant="outline" disabled={props.disabled} onClick={props.onRetry}>
          <RefreshCw size={14} />{t("settings.permissionJudge.reload")}
        </Button>
        : enabled.length > 0 || unavailable !== null
          ? <Select disabled={props.disabled} value={edgeModelId ?? undefined} onValueChange={props.onEdgeModelChange}>
            <SelectTrigger aria-label={t("settings.permissionJudge.edge.model")} className="vk-form-w-lg"><SelectValue /></SelectTrigger>
            <SelectContent>
              {enabled.map((modelId) => <SelectItem key={modelId} value={modelId}>{modelId}</SelectItem>)}
              {unavailable !== null
                ? <SelectItem key={unavailable} value={unavailable} disabled data-judge-edge-unavailable="">
                  {formatMessage(t("settings.permissionJudge.edge.modelUnavailable"), { model: unavailable })}
                </SelectItem>
                : null}
            </SelectContent>
          </Select>
          : null}
    </PageRow>
    {edgeModels.phase === "failed"
      ? <PageNotice tone="error" details={edgeModels.error} detailsLabel={t("settings.errors.showErrorDetails")}>
        {t("settings.permissionJudge.edge.modelsFailed")}
      </PageNotice>
      : null}
    {unavailable !== null
      ? <PageNotice tone="warning" data-judge-edge-saved-unavailable="">
        {formatMessage(t("settings.permissionJudge.edge.savedUnavailable"), { model: unavailable })}
      </PageNotice>
      : null}
  </PageSection>;
}
