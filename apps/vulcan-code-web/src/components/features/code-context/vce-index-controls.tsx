import * as React from "react";
import { ChevronDown, Database, RefreshCw } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { useI18n } from "@/i18n";
import type { VceProjectOverviewResponse, VceProjectSettingsInput } from "@/types";
import type { VceProjectJob, VceProjectManagement } from "./use-vce-project-management";
import { MaintenancePlanPreview } from "./vce-maintenance-plan-preview";

// VceIndexControls renders index creation, synchronization, rebuild recovery and update policy for an enabled project.
// VceIndexControls 为已启用项目渲染索引创建、同步、重建恢复与更新策略。
export function VceIndexControls(props: {
  // Exact project identity used for radio grouping and plan confirmation.
  // 用于单选分组与计划确认的精确项目身份。
  projectId: string;
  // Current authoritative settings and runtime overview.
  // 当前权威设置与运行时概览。
  overview: VceProjectOverviewResponse;
  // Whether an overview read or project mutation is active.
  // 是否正在执行概览读取或项目修改。
  busy: boolean;
  // User-editable settings draft.
  // 用户可编辑的设置草稿。
  draft: VceProjectSettingsInput;
  // Replaces the draft without saving.
  // 替换草稿但不保存。
  setDraft: (draft: VceProjectSettingsInput) => void;
  // Saves a complete draft against its frozen revision.
  // 按冻结修订保存完整草稿。
  saveDraft: (next: VceProjectSettingsInput) => Promise<void>;
  // Durable project job observer.
  // 项目持久作业观察器。
  indexing: VceProjectJob;
  // Whether active work is shown as a neutral status.
  // 活动作业是否以中性状态显示。
  indexingStatusVisible: boolean;
  // Whether an unsuccessful publication must recover through rebuild.
  // 未成功发布是否必须通过重建恢复。
  rebuildRecoveryRequired: boolean;
  // Manual synchronization availability and label.
  // 手动同步的可用性与文案。
  synchronization: { disabled: boolean; label: string };
  // Exact active job label, or null without an acknowledged receipt.
  // 精确活动作业文案；没有已确认回执时为空。
  activeJobStatusLabel: string | null;
  // Admits explicit initialization or synchronization.
  // 准入明确初始化或同步。
  refreshIndex: () => Promise<void>;
  // Shared management state and maintenance actions.
  // 共享管理状态与维护动作。
  management: VceProjectManagement;
}) {
  // t localizes index controls.
  // t 本地化索引控件。
  const { t } = useI18n();
  const { draft, setDraft, saveDraft, indexing, indexingStatusVisible, rebuildRecoveryRequired, synchronization, activeJobStatusLabel, refreshIndex } = props;
  const { managementBusy, maintenancePlan, managementError, loadMaintenancePlan, executeMaintenance } = props.management;
  // status exposes lifecycle and checkpoint state without provider implementation details.
  // status 暴露生命周期与检查点状态且不包含 Provider 实现细节。
  const status = props.overview.status;
  return (
    <section className="vce-index-controls">
      {!indexingStatusVisible && !rebuildRecoveryRequired ? <div className="vce-index-primary-action">
        <div>
          <strong>
            {status.checkpointVersion == null
              ? t("codeContext.area.createProjectIndex")
              : t("codeContext.area.updateProjectIndex")}
          </strong>
          <p className="mt-1 text-sm text-muted-foreground">
            {status.checkpointVersion == null
              ? t("codeContext.area.filesAreScannedAnd")
              : t("codeContext.area.manualSynchronizationBypassesThe")}
          </p>
        </div>
        <Button
          type="button"
          disabled={
            props.busy ||
            synchronization.disabled ||
            (indexingStatusVisible && activeJobStatusLabel === null) ||
            !props.overview.configuration.ready ||
            !props.overview.settings.enabled ||
            rebuildRecoveryRequired
          }
          onClick={() => void refreshIndex()}
        >
          <Database size={15} />
          {status.checkpointVersion == null
            ? t("codeContext.area.createSemanticIndex")
            : synchronization.label}
        </Button>
      </div> : null}
      {indexingStatusVisible && indexing.job?.kind === "synchronize" ? <div className="flex justify-end">
        <Button type="button" size="sm" variant="outline"
          disabled={props.busy || synchronization.disabled || !props.overview.configuration.ready || !props.overview.settings.enabled || rebuildRecoveryRequired}
          onClick={() => void refreshIndex()}>{synchronization.label}</Button>
      </div> : null}
      {rebuildRecoveryRequired && !indexingStatusVisible ? (
        <section className="space-y-3 rounded-lg border border-amber-500/30 bg-amber-500/5 p-3">
          <div className="flex flex-wrap items-start justify-between gap-3">
            <div className="min-w-0">
              <strong>
                {t("codeContext.area.theIndexNeedsTo")}
              </strong>
              <p className="mt-1 text-sm text-muted-foreground">
                {status.checkpointVersion == null
                  ? indexing.active
                    ? t("codeContext.area.thePreviousIndexCreation")
                    : t("codeContext.area.thePreviousIndexCreation2")
                  : t("codeContext.area.theCurrentIndexMust")}
              </p>
            </div>
            <Button
              disabled={
                props.busy ||
                managementBusy ||
                indexing.active ||
                !props.overview.configuration.ready ||
                !props.overview.settings.enabled
              }
              onClick={() => void loadMaintenancePlan("rebuild")}
              type="button"
              variant="outline"
            >
              <RefreshCw size={15} />
              {maintenancePlan?.mode === "rebuild"
                ? t("codeContext.area.regenerateRebuildPlan")
                : t("codeContext.area.generateRebuildPlan")}
            </Button>
          </div>
          {maintenancePlan?.mode === "rebuild" ? (
            <MaintenancePlanPreview
              active={indexing.active}
              busy={managementBusy}
              error={managementError}
              onExecute={(confirmation) =>
                void executeMaintenance(confirmation)
              }
              plan={maintenancePlan}
              projectId={props.projectId}
              settingsRevision={props.overview.settings.revision}
            />
          ) : null}
        </section>
      ) : null}
      <details className="vce-settings-disclosure">
        <summary>
          <RefreshCw size={16} aria-hidden="true" />
          <span><strong>{t("codeContext.area.updatePolicy")}</strong>
            <small>{draft.updateMode === "manual" ? t("codeContext.area.manualUpdatesOnYour") : t("codeContext.area.followProjectFileChanges")}</small>
          </span>
          <ChevronDown size={16} aria-hidden="true" />
        </summary>
        <div className="vce-settings-disclosure-body">
          <fieldset className="space-y-2" disabled={props.busy}>
            <legend className="text-sm font-medium">
              {t("codeContext.area.indexUpdateMode")}
            </legend>
            <div className="flex flex-wrap gap-4 text-sm">
              {(["manual", "automatic"] as const).map((mode) => (
                <label key={mode} className="flex items-center gap-2">
                  <input
                    type="radio"
                    name={`vce-update-mode-${props.projectId}`}
                    checked={draft.updateMode === mode}
                    onChange={() => {
                      const next = { ...draft, updateMode: mode };
                      setDraft(next);
                      void saveDraft(next);
                    }}
                  />
                  {mode === "manual"
                    ? t("codeContext.area.manualUpdates")
                    : t("codeContext.area.automaticUpdates")}
                </label>
              ))}
            </div>
            <p className="text-xs text-muted-foreground">
              {t("codeContext.area.thisProjectSettingIs")}
            </p>
          </fieldset>
          <label className="flex flex-wrap items-center gap-3 text-sm">
            {t("codeContext.area.minimumAutomaticIntervalSeconds")}
            <Input
              className="w-28"
              type="number"
              min={60}
              max={86400}
              disabled={props.busy}
              value={draft.autoUpdateMinIntervalSeconds}
              onChange={(event) =>
                setDraft({
                  ...draft,
                  autoUpdateMinIntervalSeconds: Number(event.target.value),
                })
              }
            />
            <Button
              type="button"
              size="sm"
              variant="outline"
              disabled={
                props.busy ||
                !Number.isInteger(draft.autoUpdateMinIntervalSeconds) ||
                draft.autoUpdateMinIntervalSeconds < 60 ||
                draft.autoUpdateMinIntervalSeconds > 86400
              }
              onClick={() => void saveDraft(draft)}
            >
              {t("codeContext.area.saveInterval")}
            </Button>
          </label>
        </div>
      </details>
    </section>
  );
}
