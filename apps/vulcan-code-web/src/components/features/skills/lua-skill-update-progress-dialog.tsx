import { CheckCircle2, CircleAlert, LoaderCircle } from "lucide-react";
import { PageDetails } from "@/components/page";
import { Button } from "@/ui-kit/primitives/button";
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/ui-kit/primitives/dialog";
import { Progress } from "@/ui-kit/primitives/progress";
import { useI18n, type TranslationKey } from "@/i18n";
import type { LuaSkillsOperationProgressSummary } from "@/types";
import { formatBytes } from "@/lib/display-format";

// LuaSkillUpdateRequest retains the clicked package and RPC result before progress arrives.
// LuaSkillUpdateRequest 在进度到达前保留已点击的技能包与 RPC 结果。
export interface LuaSkillUpdateRequest {
  // skillId is the exact package selected by the update button.
  // skillId 是更新按钮选中的精确技能包标识。
  skillId: string;
  // previousCorrelationId excludes the retained operation that preceded this click.
  // previousCorrelationId 排除本次点击之前保留的操作。
  previousCorrelationId: string | null;
  // outcome stays null until the update RPC settles.
  // outcome 在更新 RPC 结束前保持为 null。
  outcome: "succeeded" | "failed" | null;
  // error retains a rejected RPC's details, including failures before progress starts.
  // error 保留 RPC 拒绝详情，包括进度开始前的失败。
  error: string | null;
}

// UPDATE_PHASE_LABELS translates phases emitted by the pinned LuaSkills 0.5.7 manager.
// UPDATE_PHASE_LABELS 翻译已锁定的 LuaSkills 0.5.7 管理器发出的阶段。
const UPDATE_PHASE_LABELS: Record<string, TranslationKey> = {
  validating_request: "skills.update.phase.validatingRequest",
  resolving_source: "skills.update.phase.resolvingSource",
  downloading_archive: "skills.update.phase.downloadingArchive",
  extracting_archive: "skills.update.phase.extractingArchive",
  validating_skill_manifest: "skills.update.phase.validatingSkillManifest",
  backing_up_current: "skills.update.phase.backingUpCurrent",
  staging_update: "skills.update.phase.stagingUpdate",
  reloading_runtime: "skills.update.phase.reloadingRuntime",
  committing: "skills.update.phase.committing",
  completed: "skills.update.phase.completed",
  failed: "skills.update.phase.failed",
};

// LuaSkillUpdateProgressDialog renders measured update progress and retains the final result.
// LuaSkillUpdateProgressDialog 渲染实测更新进度并保留最终结果。
// Parameters: props supplies visibility, the current service operation, the optional local request, and dismissal.
// 参数：props 提供可见状态、当前服务操作、可选本地请求以及关闭回调。
// Returns: an accessible modal; unknown phase progress stays indeterminate until the service reports a value.
// 返回值：无障碍弹窗；阶段进度未知时保持不确定状态，直到服务报告数值。
export function LuaSkillUpdateProgressDialog(props: {
  // open controls the modal without discarding the service's retained progress.
  // open 控制弹窗显示，不丢弃服务端保留的进度。
  open: boolean;
  // progress contains only the update operation following the local request's baseline.
  // progress 只包含本地请求基线之后的更新操作。
  progress: LuaSkillsOperationProgressSummary | null;
  // request exists when this page initiated the update, including its pre-event state.
  // request 在本页发起更新时存在，包含事件到达前的状态。
  request: LuaSkillUpdateRequest | null;
  // onClose dismisses the settled operation without modifying the service record.
  // onClose 关闭已结束操作的弹窗，不修改服务记录。
  onClose: () => void;
}) {
  // t selects the same language as the surrounding skills page.
  // t 选择与外围技能页面一致的语言。
  const { t } = useI18n();
  // running keeps the dialog active until both the service operation and local RPC settle.
  // running 在服务操作与本地 RPC 都结束前保持弹窗运行状态。
  const running = props.progress?.inFlight === true || (props.request !== null && props.request.outcome === null);
  // failed includes RPC failures that happen before a service progress record is created.
  // failed 包含服务进度记录创建前发生的 RPC 失败。
  const failed = !running && (props.progress?.outcome === "failed" || props.request?.outcome === "failed");
  // succeeded requires an explicit terminal result, never a download percentage of 100.
  // succeeded 必须有明确终态结果，不能用下载百分比达到 100 判断。
  const succeeded = !running && !failed && (props.progress?.outcome === "succeeded" || props.request?.outcome === "succeeded");
  // percent uses the measured phase value; only confirmed completion implies overall 100 percent.
  // percent 使用实测阶段数值；只有确认完成后才表示整体达到百分之百。
  const percent = succeeded ? 100 : props.progress?.percent ?? null;
  // phase is optional until the upstream manager emits its first event.
  // phase 在上游管理器发出首个事件前可以不存在。
  const phase = props.progress?.latestPhase;
  // phaseLabel localizes known phases and preserves any newly declared machine-readable phase.
  // phaseLabel 本地化已知阶段，并保留新声明的机器可读阶段。
  const phaseLabel = phase == null
    ? t("skills.update.preparingUpdate")
    : Object.hasOwn(UPDATE_PHASE_LABELS, phase)
      ? t(UPDATE_PHASE_LABELS[phase])
      : phase;
  // title describes the operation outcome independently of the current download phase.
  // title 独立于当前下载阶段描述操作结果。
  const title = running
    ? t("skills.update.skillPackageUpdateIn")
    : failed
      ? t("skills.update.skillPackageUpdateFailed")
      : succeeded
        ? t("skills.update.skillPackageUpdateCompleted")
        : t("skills.update.skillPackageUpdateStatus");
  // skillId uses the clicked package before the optional upstream identity becomes available.
  // skillId 在可选上游标识可用前使用已点击的技能包。
  const skillId = props.request !== null ? props.request.skillId : props.progress?.skillId;

  return (
    <Dialog open={props.open} onOpenChange={(open) => { if (!open && !running) props.onClose(); }}>
      <DialogContent size="sm" scrollMode="body-scroll" showCloseButton={!running}
        onEscapeKeyDown={(event) => { if (running) event.preventDefault(); }}
        onInteractOutside={(event) => { if (running) event.preventDefault(); }}>
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            {running ? <LoaderCircle size={18} className="motion-safe:animate-spin" aria-hidden="true" />
              : failed ? <CircleAlert size={18} className="text-destructive" aria-hidden="true" />
                : succeeded ? <CheckCircle2 size={18} className="text-primary" aria-hidden="true" /> : null}
            {title}
          </DialogTitle>
          <DialogDescription>{skillId ?? t("skills.update.luaskillsPackageUpdate")}</DialogDescription>
        </DialogHeader>
        <div className="space-y-3" role={failed ? "alert" : "status"}>
          <div className="flex items-center justify-between gap-4">
            <span className="text-muted-foreground">{succeeded ? t("skills.update.updateCompleted")
              : failed ? t("skills.update.updateIncomplete") : phaseLabel}</span>
            <span className="shrink-0 font-medium tabular-nums">{percent === null
              ? (running ? t("skills.update.waitingForProgress") : "—") : `${percent}%`}</span>
          </div>
          {percent === null ? (
            <div role="progressbar" aria-label={phaseLabel} className="h-2 overflow-hidden rounded-full bg-muted">
              {running ? <div className="h-full w-1/3 rounded-full bg-primary/60 motion-safe:animate-pulse" /> : null}
            </div>
          ) : <Progress value={percent} aria-label={succeeded ? title : phaseLabel} className="h-2" />}
          {!succeeded && props.progress?.bytesDone != null ? <p className="text-xs text-muted-foreground tabular-nums">
            {t("skills.update.downloaded")}{formatBytes(props.progress.bytesDone)}
            {props.progress.bytesTotal != null ? ` / ${formatBytes(props.progress.bytesTotal)}` : ""}
          </p> : null}
          {props.progress?.message ? <p className="text-sm text-muted-foreground">{props.progress.message}</p> : null}
          {props.progress?.overflowed ? <p className="text-xs text-muted-foreground">{t("skills.update.somePhaseRecordsWere")}</p> : null}
          {props.request?.error ? <PageDetails label={t("skills.update.errorDetails")}><pre>{props.request.error}</pre></PageDetails> : null}
          {running ? <p className="text-xs text-muted-foreground">{t("skills.update.otherSkillOperationsWait")}</p> : null}
        </div>
        <DialogFooter>
          <Button type="button" variant="outline" disabled={running} onClick={props.onClose}>{running
            ? t("skills.update.updating") : t("skills.update.close")}</Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
