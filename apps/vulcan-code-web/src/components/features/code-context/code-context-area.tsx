import * as React from "react";
import {
  Database,
  LoaderCircle,
  RefreshCw,
} from "lucide-react";
import { Button } from "@/components/ui/button";
import { StreamPlaceholder } from "@/components/ui/stream-placeholder";
import { Switch } from "@/components/ui/switch";
import { useI18n } from "@/i18n";
import type {
  VceJobSummary,
  VceProjectOverviewResponse,
  VceProjectSettingsInput,
  VceProjectSettingsSummary,
} from "@/types";
import { useVceProjectJob } from "./use-vce-project-job";
import { useVceActionScope } from "./use-vce-action-scope";
import { useVceProjectManagement } from "./use-vce-project-management";
import { VceIndexControls } from "./vce-index-controls";
import { VceJobProgress, vceActiveJobStatusLabel, vceSynchronizationControl } from "./vce-job-progress";
import { MaintenancePlanPreview } from "./vce-maintenance-plan-preview";
import { VceManagementDetails } from "./vce-management-details";
import { VceRetrievalSettings } from "./vce-retrieval-settings";

export { loadAuthoritativeVceOverview } from "./use-vce-project-management";

// CodeContextArea renders project settings and paged VCE management data.
// CodeContextArea 渲染项目设置与分页 VCE 管理数据。
export function CodeContextArea(props: {
  // Exact project identity selected by the settings dialog.
  // 设置弹窗选择的精确项目身份。
  projectId: string;
  // Current authoritative settings and runtime overview.
  // 当前权威设置与运行时概览。
  overview: VceProjectOverviewResponse | null;
  // Whether an overview read or project mutation is active.
  // 是否正在执行概览读取或项目修改。
  busy: boolean;
  // Persists a complete editable settings document.
  // 持久化完整可编辑设置文档。
  onSave: (
    settings: VceProjectSettingsInput,
    expectedRevision: number,
  ) => Promise<VceProjectSettingsSummary>;
  // Shared activity read error; never inferred from empty management results.
  // 共享活动读取错误；绝不从空管理结果推断。
  activityError: string | null;
  // Reload shared lightweight authority without scanning management data; resolves after the read chain.
  // 重载共享轻量权威，不扫描管理数据；读取链完成后返回。
  onReloadActivity: () => Promise<void>;
  // Admit explicit initialization or synchronization and return its durable receipt, not completion.
  // 准入明确初始化或同步，返回其持久回执而非完成状态。
  onRefresh: () => Promise<VceJobSummary>;
  // Replaces the parent-owned overview after a management mutation.
  // 在管理修改后替换父级持有的概览。
  onOverviewChanged: (overview: VceProjectOverviewResponse) => void;
  // Navigate to host-owned global model settings; no arguments or mutation result.
  // 导航到宿主持有的全局模型设置；无参数及修改结果。
  onOpenEngineSettings: () => void;
}) {
  // t selects local bilingual copy without exposing VCE implementation names to users.
  // t 选择本地双语文案，且不向用户暴露 VCE 实现名称。
  const { t } = useI18n();
  // indexing follows this project's independent durable job, not the conversation state.
  // indexing 跟随本项目独立持久作业，而非会话状态。
  const indexing = useVceProjectJob(props.projectId);
  // projectReportsActiveJob is the host lifecycle projection available before the receipt observer finishes rehydrating.
  // projectReportsActiveJob 是作业回执观察器完成恢复前，宿主已经提供的生命周期投影。
  const projectReportsActiveJob =
    props.overview?.status.lifecycle === "indexing";
  // activeJobStatusLabel uses the exact acknowledged operation kind whenever its receipt is available.
  // activeJobStatusLabel 在已取得回执时使用已确认的精确操作类型。
  const activeJobStatusLabel = vceActiveJobStatusLabel(indexing.job, t);
  // Use the acknowledged project job to offer at most one subsequent manual synchronization.
  // 使用已确认项目作业提供至多一次后续手动同步。
  const synchronization = activeJobStatusLabel !== null
    ? vceSynchronizationControl(indexing.job, t)
    : projectReportsActiveJob
    ? {
        disabled: true,
        label: t("codeContext.area.indexJobInProgress"),
      }
    : vceSynchronizationControl(indexing.job, t);
  // replacementInProgressActivityError identifies the expected read race while a durable replacement owns the project.
  // replacementInProgressActivityError 识别持久替换作业持有项目期间预期的读取竞争。
  const replacementInProgressActivityError =
    props.activityError ===
    "VCE explicit replacement is in progress; observe its durable job";
  // indexingStatusVisible turns active work into a neutral localized status instead of an inline error.
  // indexingStatusVisible 将活动作业转换为中性的本地化状态，而不是行内错误。
  const indexingStatusVisible =
    indexing.active ||
    projectReportsActiveJob ||
    replacementInProgressActivityError;
  // draft holds only user-editable fields and never mirrors host-owned revision fields.
  // draft 只保存用户可编辑字段，绝不镜像宿主持有的修订字段。
  const [draft, setDraft] = React.useState<VceProjectSettingsInput | null>(
    () => (props.overview ? editableSettings(props.overview.settings) : null),
  );
  // draftBase freezes the exact persisted revision from which the user's editable draft was created.
  // draftBase 固定用户可编辑草稿创建时的精确持久修订。
  const draftBase = React.useRef<VceProjectSettingsSummary | null>(
    props.overview?.settings ?? null,
  );
  // draftConflict warns about remote changes without discarding unsaved local edits.
  // draftConflict 提示远端变化，不丢弃未保存的本地编辑。
  const [draftConflict, setDraftConflict] = React.useState(false);
  // Actions retain the admitted project and service lifetime, not merely a reusable project ID.
  // 操作保留准入项目和服务生命周期，而不只是可重复使用的项目编号。
  const isCurrentAction = useVceActionScope(props.projectId);
  React.useEffect(() => {
    if (!props.overview) return;
    // settings is projected explicitly so host-owned metadata cannot be resubmitted.
    // settings 被显式投影，因此宿主持有的元数据不会被重新提交。
    const settings = props.overview.settings;
    if (
      draftBase.current !== null &&
      draftBase.current.revision >= settings.revision
    )
      return;
    // A new checkpoint/progress snapshot is not a settings edit. A remote settings edit cannot
    // silently rebase a dirty draft onto a newer revision and overwrite someone else's choices.
    // 新检查点或进度快照不是设置编辑。远端设置编辑不能静默把脏草稿
    // 改基到新修订并覆盖他人的选择。
    if (
      draft !== null &&
      draftBase.current !== null &&
      JSON.stringify(draft) !==
        JSON.stringify(editableSettings(draftBase.current))
    ) {
      setDraftConflict(true);
      return;
    }
    draftBase.current = settings;
    setDraft(editableSettings(settings));
    setDraftConflict(false);
  }, [props.overview, draft]);

  // management owns optional lists, Blob details and maintenance plans behind one request fence.
  // management 在同一请求保护下持有可选列表、Blob 详情与维护计划。
  const management = useVceProjectManagement({
    projectId: props.projectId,
    overview: props.overview,
    onOverviewChanged: props.onOverviewChanged,
    indexing,
    isCurrentAction,
  });
  const {
    managementExpanded,
    setManagementExpanded,
    managementOverview,
    managementBusy,
    maintenancePlan,
    managementError,
    setManagementError,
    loadMaintenancePlan,
    executeMaintenance,
    retryOperation,
  } = management;

  // Save next against its frozen revision; refresh lightweight authority on failure without erasing edits.
  // 按冻结修订保存 next；失败时刷新轻量权威，不擦除编辑。
  // Parameters: next contains the complete typed settings draft for this project revision.
  // 参数：next 包含当前项目修订的完整强类型设置草稿。
  // Returns after acknowledgement or visible error handling; management scans are never required.
  // 确认或可见错误处理后返回；绝不要求管理扫描。
  const saveDraft = async (next: VceProjectSettingsInput) => {
    if (draftBase.current === null) return;
    setManagementError(null);
    try {
      // saved is the acknowledged settings document, not a synthesized local revision.
      // saved 为已确认设置文档，绝非本地合成修订。
      // Preserve raw draft lines while typing; remove blank rules only at the existing save boundary.
      // 输入时保留原始草稿行；仅在既有保存边界移除空规则。
      const saved = await props.onSave(
        {
          ...next,
          extraIgnorePatterns: next.extraIgnorePatterns
            .map((value) => value.trim())
            .filter(Boolean),
        },
        draftBase.current.revision,
      );
      if (isCurrentAction()) {
        draftBase.current = saved;
        setDraft(editableSettings(saved));
        setDraftConflict(false);
      }
    } catch (error) {
      if (isCurrentAction()) {
        setManagementError(
          error instanceof Error ? error.message : String(error),
        );
        // Shared read errors are rendered independently through activityError, never swallowed here.
        // 共享读取错误通过 activityError 独立渲染，绝不在此吞掉。
        await refreshStatus();
      }
    }
  };

  // refreshIndex reports a rejected refresh locally instead of creating an unhandled promise.
  // refreshIndex 在本地报告被拒绝的刷新，而不是产生未处理 Promise。
  const refreshIndex = async () => {
    if (
      !props.overview?.configuration.ready ||
      !props.overview.settings.enabled
    )
      return;
    setManagementError(null);
    try {
      const receipt = await props.onRefresh();
      if (isCurrentAction()) indexing.receive(receipt);
    } catch (error) {
      if (isCurrentAction())
        setManagementError(
          error instanceof Error ? error.message : String(error),
        );
    }
  };

  // Refresh visible metadata only; this icon has no authority to scan or initialize an index.
  // 仅刷新可见元数据；此图标无权扫描或初始化索引。
  // Takes no arguments and replaces only the current project's authoritative overview.
  // 无参数，仅替换当前项目权威概览。
  // Returns after refreshing metadata or retaining an explicit error; failures never require a management scan.
  // 刷新元数据或保留明确错误后返回；失败绝不要求管理扫描。
  const refreshStatus = async () => {
    try {
      await props.onReloadActivity();
    } catch (error) {
      if (isCurrentAction())
        setManagementError(
          error instanceof Error ? error.message : String(error),
        );
    }
  };

  if (!props.overview || !draft) {
    return (
      <div className="project-settings-section">
        {props.activityError && !replacementInProgressActivityError ? (
          <>
            <p role="alert" className="text-sm text-destructive">
              {props.activityError}
            </p>
            <Button type="button" variant="outline" disabled={props.busy} onClick={() => void refreshStatus()}>
              {t("codeContext.area.reloadStatus")}
            </Button>
          </>
        ) : null}
        {managementError || indexing.error ? (
          <p role="alert" className="text-sm text-destructive">{managementError ?? indexing.error}</p>
        ) : null}
        {indexing.job ? (
          <VceJobProgress
            presentation="settings"
            job={indexing.job}
            onCancel={() => void indexing.cancel()}
            cancelPending={indexing.cancelPending}
          />
        ) : indexingStatusVisible ? (
          <p
            className="project-settings-muted"
            role="status"
            aria-live="polite"
          >
            <LoaderCircle className="mr-2 inline animate-spin" size={14} />
            {t("projectSettings.codeContext.generating")}
          </p>
        ) : (
          <StreamPlaceholder className="w-full" label={t("codeContext.area.loadingCodeSemanticIndex")} shape="fields" count={3} />
        )}
      </div>
    );
  }

  // status exposes lifecycle and checkpoint state without provider implementation details.
  // status 暴露生命周期与检查点状态且不包含 Provider 实现细节。
  const status = props.overview.status;
  // rebuildRecoveryRequired keeps an unsuccessful first publication on an explicit recovery path.
  // rebuildRecoveryRequired 将首次发布失败保持在明确的恢复路径上。
  const rebuildRecoveryRequired =
    status.lifecycle === "rebuild_required" ||
    (status.lifecycle === "failed" && status.checkpointVersion == null) ||
    (status.checkpointVersion == null &&
      isUnpublishedVceRecoveryJob(indexing.job));
  // counts supplies aggregate storage statistics when the runtime is active.
  // counts 在运行时活动时提供存储聚合统计。
  const counts =
    managementOverview?.status.projectId === props.projectId &&
    managementOverview.status.checkpointVersion === status.checkpointVersion &&
    managementOverview.settings.revision === props.overview.settings.revision
      ? managementOverview.counts
      : null;
  // lifecycleLabels translates the declared host states without deriving health from counters.
  // lifecycleLabels 翻译宿主已声明状态，不根据计数推断健康程度。
  const lifecycleLabels: Record<VceProjectOverviewResponse["status"]["lifecycle"], string> = {
    uninitialized: t("codeContext.area.notCreated"),
    disabled: t("codeContext.area.disabled"),
    starting: t("codeContext.area.starting"),
    restoring: t("codeContext.area.restoring"),
    indexing: t("codeContext.area.indexing"),
    ready: t("codeContext.area.ready"),
    degraded_optional_ai: t("codeContext.area.limitedFeatures"),
    blocked_configuration: t("codeContext.area.modelsRequired"),
    rebuild_required: t("codeContext.area.rebuildRequired"),
    failed: t("codeContext.area.failed"),
    stopping: t("codeContext.area.stopping"),
  };
  return (
    <div className="project-settings-section vce-project-settings">
      <header className="project-settings-heading project-settings-heading-with-action vce-settings-heading">
        <Database size={18} />
        <div>
          <strong>{t("codeContext.area.codeSemanticIndex")}</strong>
          <span>
            {t("codeContext.area.providesNaturalLanguageCode")}
          </span>
        </div>
        <div className="project-settings-heading-actions">
          <label className="vce-settings-enabled">
            <span>{draft.enabled ? t("codeContext.area.enabled") : t("codeContext.area.disabled")}</span>
            <Switch
              aria-label={t("codeContext.area.enableCodeSemanticIndex")}
              checked={draft.enabled}
              disabled={props.busy}
              onCheckedChange={(enabled) => {
                const next = { ...draft, enabled };
                setDraft(next);
                void saveDraft(next);
              }}
            />
          </label>
          <Button
            aria-label={t("codeContext.area.refreshStatus")}
            disabled={props.busy}
            onClick={() => void refreshStatus()}
            size="icon-sm"
            type="button"
            variant="outline"
          >
            <RefreshCw
              className={props.busy ? "animate-spin" : undefined}
              size={15}
            />
          </Button>
        </div>
      </header>

      {props.activityError && !replacementInProgressActivityError ? (
        <p role="alert" className="text-sm text-destructive">
          {props.activityError}
        </p>
      ) : null}
      {draftConflict ? (
        <div
          role="alert"
          className="rounded-lg border border-amber-500/30 p-3 text-sm"
        >
          <p>
            {t("codeContext.area.projectSettingsChangedElsewhere")}
          </p>
          <Button
            type="button"
            variant="outline"
            size="sm"
            disabled={props.busy}
            onClick={() => {
              draftBase.current = props.overview!.settings;
              setDraft(editableSettings(props.overview!.settings));
              setDraftConflict(false);
            }}
          >
            {t("codeContext.area.discardEditsAndLoad")}
          </Button>
        </div>
      ) : null}

      {!props.overview.configuration.ready ? (
        <div className="rounded-lg border border-amber-500/30 bg-amber-500/5 p-3 text-sm">
          <p>
            {t("codeContext.area.engineModelsAreNot")}
          </p>
          <Button
            type="button"
            variant="link"
            size="sm"
            onClick={props.onOpenEngineSettings}
          >
            {t("codeContext.area.configureEngineModels")}
          </Button>
          {draft.enabled ? (
            <div className="mt-3 space-y-2 border-t pt-3">
              <p className="text-xs text-muted-foreground">
                {t("codeContext.area.physicalCompactionOfAn")}
              </p>
              <Button
                disabled={managementBusy || indexing.active}
                onClick={() => void loadMaintenancePlan("compact")}
                size="sm"
                type="button"
                variant="outline"
              >
                {t("codeContext.area.inspectAndCompactPhysical")}
              </Button>
              {maintenancePlan?.mode === "compact" ? (
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
            </div>
          ) : null}
        </div>
      ) : null}

      {indexing.job ? (
        <VceJobProgress
          presentation="settings"
          job={indexing.job}
          onCancel={() => void indexing.cancel()}
          cancelPending={indexing.cancelPending}
          onRetry={
            indexing.job.kind !== "rebuild" &&
            !rebuildRecoveryRequired &&
            !managementBusy &&
            props.overview?.configuration.ready === true &&
            draft.enabled
              ? () => void retryOperation(indexing.job!.jobId)
              : undefined
          }
        />
      ) : indexingStatusVisible ? <StreamPlaceholder className="vce-job-rehydrating" label={t("codeContext.area.loadingIndexTaskProgress")} count={2} /> : null}
      {indexing.error ? (
        <p role="alert" className="text-sm text-destructive">
          {t("codeContext.area.indexStatusUnavailable")}
          {indexing.error}
        </p>
      ) : null}

      <div className="vce-index-statistics" aria-label={t("codeContext.area.indexOverview")}>
        <div data-lifecycle={status.lifecycle}>
          <strong>{t("codeContext.area.indexStatus")}</strong>
          <p>{indexingStatusVisible ? activeJobStatusLabel ?? t("codeContext.area.indexing") : lifecycleLabels[status.lifecycle]}</p>
        </div>
        <div>
          <strong>{t("codeContext.area.checkpoint")}</strong>
          <p>{status.checkpointVersion ?? "—"}</p>
        </div>
        <div>
          <strong>{t("codeContext.area.files")}</strong>
          <p>{counts?.searchableFiles ?? status.presentFiles}</p>
        </div>
        <div>
          <strong>{t("codeContext.area.chunks")}</strong>
          <p>{counts?.chunks ?? "—"}</p>
        </div>
        <div>
          <strong>{t("codeContext.area.vectors")}</strong>
          <p>{counts?.vectors ?? "—"}</p>
        </div>
        <div>
          <strong>{t("codeContext.area.exactSymbols")}</strong>
          <p>{counts?.symbols ?? "—"}</p>
        </div>
      </div>

      {draft.enabled ? (
        <VceIndexControls
          activeJobStatusLabel={activeJobStatusLabel}
          busy={props.busy}
          draft={draft}
          indexing={indexing}
          indexingStatusVisible={indexingStatusVisible}
          management={management}
          overview={props.overview}
          projectId={props.projectId}
          rebuildRecoveryRequired={rebuildRecoveryRequired}
          refreshIndex={refreshIndex}
          saveDraft={saveDraft}
          setDraft={setDraft}
          synchronization={synchronization}
        />
      ) : null}

      {status.lastError && !indexingStatusVisible && status.lastError !== indexing.job?.error ? (
        <p className="rounded-lg border border-destructive/30 bg-destructive/5 p-3 text-sm text-destructive">
          {status.lastError}
        </p>
      ) : null}

      <VceRetrievalSettings
        busy={props.busy}
        draft={draft}
        overview={props.overview}
        saveDraft={saveDraft}
        setDraft={setDraft}
      />

      {draft.enabled && status.checkpointVersion != null ? (
        <Button
          type="button"
          variant="outline"
          className="vce-index-details-toggle"
          aria-expanded={managementExpanded}
          onClick={() => setManagementExpanded(!managementExpanded)}
        >
          {managementExpanded
            ? t("codeContext.area.hideIndexDetails")
            : t("codeContext.area.viewIndexDetailsAnd")}
        </Button>
      ) : null}
      {managementExpanded &&
      draft.enabled &&
      status.checkpointVersion != null ? (
        <VceManagementDetails
          indexingActive={indexing.active}
          management={management}
          overview={props.overview}
          projectId={props.projectId}
          rebuildRecoveryRequired={rebuildRecoveryRequired}
        />
      ) : null}

      {managementBusy ? (
        <StreamPlaceholder label={t("codeContext.area.loadingManagementData")} shape="rows" count={3} />
      ) : null}
      {managementError ? (
        <p className="rounded-lg border border-destructive/30 bg-destructive/5 p-3 text-sm text-destructive">
          {managementError}
        </p>
      ) : null}
    </div>
  );
}

// isUnpublishedVceRecoveryJob recognizes only terminal initialize/rebuild receipts with no checkpoint.
// isUnpublishedVceRecoveryJob 只识别无检查点的初始化／重建终态回执。
function isUnpublishedVceRecoveryJob(job: VceJobSummary | null): boolean {
  if (job === null || (job.kind !== "initialize" && job.kind !== "rebuild"))
    return false;
  return (
    job.state === "failed" ||
    job.state === "cancelled" ||
    job.state === "interrupted"
  );
}

// editableSettings strips host-owned fields from one settings summary.
// editableSettings 从一项设置摘要中剥离宿主持有字段。
function editableSettings(
  settings: VceProjectOverviewResponse["settings"],
): VceProjectSettingsInput {
  return {
    enabled: settings.enabled,
    updateMode: settings.updateMode,
    autoUpdateMinIntervalSeconds: settings.autoUpdateMinIntervalSeconds,
    maxFileBytes: settings.maxFileBytes,
    extraIgnorePatterns: settings.extraIgnorePatterns,
    allowQueryRewrite: settings.allowQueryRewrite,
    allowRerank: settings.allowRerank,
    embeddingMaxConcurrency: settings.embeddingMaxConcurrency,
    queryMaxConcurrency: settings.queryMaxConcurrency,
    maxResults: settings.maxResults,
    maxContextChars: settings.maxContextChars,
  };
}
