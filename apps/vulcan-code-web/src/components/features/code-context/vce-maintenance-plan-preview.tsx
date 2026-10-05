import * as React from "react";
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog";
import { Button } from "@/components/ui/button";
import { useI18n } from "@/i18n";
import type { VceMaintenancePlanResponse } from "@/types";

// VceMaintenanceConfirmation freezes the exact plan authority at dialog-open time.
// VceMaintenanceConfirmation 在对话框打开时冻结计划的精确权威。
export interface VceMaintenanceConfirmation {
  // Exact mounted project identity used for admission.
  // 准入使用的精确已挂载项目身份。
  projectId: string;
  // Exact settings revision used for optimistic admission.
  // 乐观准入使用的精确设置修订。
  settingsRevision: number;
  // Exact checkpoint visible when the plan was confirmed.
  // 确认计划时可见的精确检查点。
  checkpointVersion: number | null;
  // Exact maintenance mode returned by the plan.
  // 计划返回的精确维护模式。
  mode: string;
  // Exact action identities returned by the plan.
  // 计划返回的精确动作身份。
  actionIds: string[];
  // Exact model fingerprint frozen by the read-only preview.
  // 只读预览冻结的精确模型指纹。
  embeddingFingerprint: string;
  // Stable replay identity for this one human confirmation.
  // 本次人为确认使用的稳定重放身份。
  idempotencyKey: string;
}

// MaintenancePlanPreview renders one exact read-only plan and its explicit confirmation action.
// MaintenancePlanPreview 渲染一份精确的只读计划及其明确确认操作。
export function MaintenancePlanPreview(props: {
  // Whether a live indexing job still owns the project's write path.
  // 是否仍有活跃索引作业持有项目写入路径。
  active: boolean;
  // Whether the parent is waiting for a plan or execution response.
  // 父级是否正在等待计划或执行回执。
  busy: boolean;
  // Current maintenance error remains visible inside the open confirmation dialog.
  // 当前维护错误保留在打开的确认弹窗内可见。
  error: string | null;
  // Executes only the currently rendered plan after human confirmation.
  // 仅在人为确认后执行当前渲染的计划。
  onExecute: (confirmation: VceMaintenanceConfirmation) => void;
  // Read-only plan returned by the host for the exact current project state.
  // 宿主为当前项目精确状态返回的只读计划。
  plan: VceMaintenancePlanResponse;
  // Exact mounted project identity used to freeze the confirmation.
  // 用于冻结确认的精确已挂载项目身份。
  projectId: string;
  // Exact settings revision used to freeze the confirmation.
  // 用于冻结确认的精确设置修订。
  settingsRevision: number;
}) {
  const { t } = useI18n();
  // An uninitialized rebuild scans the project even when no existing Blob is affected.
  // 未初始化重建即使没有现有 Blob 受影响，也会扫描整个项目。
  const unpublishedRebuild =
    props.plan.checkpointVersion === null && props.plan.mode === "rebuild";
  // physicalCompaction selects model-independent table copy and physical statistics.
  // physicalCompaction 选择与模型无关的表级文案及物理统计。
  const physicalCompaction = props.plan.mode === "compact";
  // confirmation retains the exact plan authority while the modal is open.
  // confirmation 在弹窗打开期间保留计划的精确权威。
  const [confirmation, setConfirmation] =
    React.useState<VceMaintenanceConfirmation | null>(null);
  const actionIds = React.useMemo(
    () => props.plan.actions.map((action) => action.actionId),
    [props.plan.actions],
  );
  const confirmationIsCurrent =
    confirmation !== null &&
    confirmation.projectId === props.projectId &&
    confirmation.settingsRevision === props.settingsRevision &&
    confirmation.checkpointVersion === props.plan.checkpointVersion &&
    confirmation.mode === props.plan.mode &&
    confirmation.embeddingFingerprint === props.plan.embeddingFingerprint &&
    confirmation.actionIds.length === actionIds.length &&
    confirmation.actionIds.every(
      (actionId, index) => actionId === actionIds[index],
    );

  React.useEffect(() => {
    // A changed plan or authority closes the old confirmation and requires a fresh preview.
    // 计划或权威变化时关闭旧确认，并要求重新预览。
    if (confirmation !== null && !confirmationIsCurrent) {
      setConfirmation(null);
    }
  }, [confirmation, confirmationIsCurrent]);

  // openConfirmation freezes all admission fields and creates one replay identity.
  // openConfirmation 冻结全部准入字段并创建一个重放身份。
  const openConfirmation = () => {
    if (props.busy || props.active || actionIds.length === 0) return;
    setConfirmation({
      projectId: props.projectId,
      settingsRevision: props.settingsRevision,
      checkpointVersion: props.plan.checkpointVersion,
      mode: props.plan.mode,
      actionIds: [...actionIds],
      embeddingFingerprint: props.plan.embeddingFingerprint,
      idempotencyKey: crypto.randomUUID(),
    });
  };
  // submitConfirmation prevents a stale or busy dialog from dispatching a command.
  // submitConfirmation 防止过期或忙碌弹窗派发命令。
  const submitConfirmation = (event: React.MouseEvent<HTMLButtonElement>) => {
    event.preventDefault();
    if (
      confirmation === null ||
      !confirmationIsCurrent ||
      props.busy ||
      props.active
    )
      return;
    props.onExecute(confirmation);
  };
  // scopeLabel describes the real work for both existing-index and first-build recovery plans.
  // scopeLabel 为已有索引及首次恢复计划描述真实工作范围。
  const scopeLabel = physicalCompaction
    ? `${props.plan.physicalTables.filter((table) => table.compactionRequired).length} / ${props.plan.physicalTables.length} ${t("codeContext.area.tablesRequireCompaction")}`
    : unpublishedRebuild
      ? t("codeContext.area.rescanTheCurrentProject")
      : `${props.plan.affectedBlobCount} ${t("codeContext.area.blobs")}`;
  return (
    <div className="space-y-2 rounded-md border bg-background/60 p-3">
      <p className="text-sm">
        {t("codeContext.area.plannedActions")}: {props.plan.actions.length}{" "}
        · {t("codeContext.area.scope")}: {scopeLabel}
      </p>
      {physicalCompaction
        ? props.plan.physicalTables.map((table) => (
            <div
              className="grid gap-1 rounded-md border p-2 text-xs sm:grid-cols-3"
              key={table.role}
            >
              <strong>
                {table.role === "content"
                  ? t("codeContext.area.contentVectorTable")
                  : t("codeContext.area.pathVectorTable")}
              </strong>
              <span>
                {t("codeContext.area.rows")}: {table.rowCount.toLocaleString()}{" "}
                · {t("codeContext.area.fragments")}:{" "}
                {table.fragmentCount.toLocaleString()} ·{" "}
                {t("codeContext.area.smallFragments")}:{" "}
                {table.smallFragmentCount.toLocaleString()}
              </span>
              <span>
                {table.compactionRequired
                  ? t("codeContext.area.compactionRecommended")
                  : t("codeContext.area.noCompactionNeeded")}{" "}
                · {t("codeContext.area.version")} {table.version}
              </span>
            </div>
          ))
        : props.plan.actions.map((action) => (
            <p className="text-sm" key={action.actionId}>
              {action.description}
            </p>
          ))}
      <Button
        disabled={props.busy || props.active || props.plan.actions.length === 0}
        onClick={openConfirmation}
        size="sm"
        type="button"
      >
        {physicalCompaction
          ? t("codeContext.area.confirmAndCompact")
          : t("codeContext.area.confirmAndExecute")}
      </Button>
      <AlertDialog
        open={confirmation !== null && confirmationIsCurrent}
        onOpenChange={(open) => {
          if (!open && !props.busy) setConfirmation(null);
        }}
      >
        <AlertDialogContent className="max-h-[85vh] overflow-y-auto" size="sm">
          <AlertDialogHeader>
            <AlertDialogTitle>
              {physicalCompaction
                ? t("codeContext.area.confirmPhysicalIndexCompaction")
                : unpublishedRebuild
                  ? t("codeContext.area.confirmSemanticIndexCreation")
                  : t("codeContext.area.confirmIndexMaintenance")}
            </AlertDialogTitle>
            <AlertDialogDescription>
              {physicalCompaction
                ? t("codeContext.area.thisOnlyCompactsPhysical")
                : unpublishedRebuild
                  ? t("codeContext.area.thisWillRescanThe")
                  : t("codeContext.area.reviewThePlannedActions")}
            </AlertDialogDescription>
          </AlertDialogHeader>
          <dl className="grid gap-3 rounded-md border p-3 text-sm sm:grid-cols-2">
            <div>
              <dt className="text-muted-foreground">
                {t("codeContext.area.plannedActions")}
              </dt>
              <dd>{props.plan.actions.length}</dd>
            </div>
            <div>
              <dt className="text-muted-foreground">
                {physicalCompaction
                  ? t("codeContext.area.existingVectorDimension")
                  : t("codeContext.area.targetVectorDimension")}
              </dt>
              <dd>{props.plan.embeddingDimension}</dd>
            </div>
            <div className="sm:col-span-2">
              <dt className="text-muted-foreground">
                {t("codeContext.area.scope2")}
              </dt>
              <dd>{scopeLabel}</dd>
            </div>
          </dl>
          <div className="space-y-1">
            {physicalCompaction
              ? props.plan.actions.map((action) => (
                  <p className="text-sm" key={action.actionId}>
                    {action.physicalTableRole === "content"
                      ? t("codeContext.area.compactTheContentVector")
                      : t("codeContext.area.compactThePathVector")}
                  </p>
                ))
              : props.plan.actions.map((action) => (
                  <p className="text-sm" key={action.actionId}>
                    {action.description}
                  </p>
                ))}
          </div>
          {props.error ? (
            <p className="break-words text-sm text-destructive" role="alert">
              {props.error}
            </p>
          ) : null}
          <AlertDialogFooter>
            <AlertDialogCancel disabled={props.busy}>
              {t("codeContext.area.cancel")}
            </AlertDialogCancel>
            <AlertDialogAction
              disabled={props.busy || props.active || !confirmationIsCurrent}
              onClick={submitConfirmation}
            >
              {physicalCompaction
                ? t("codeContext.area.confirmAndCompact")
                : t("codeContext.area.confirmAndExecute")}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </div>
  );
}
