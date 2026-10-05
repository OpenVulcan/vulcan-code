import * as React from "react";
import { FileCode2, RefreshCw, SearchCheck } from "lucide-react";
import { Button } from "@/components/ui/button";
import { useI18n } from "@/i18n";
import { listVceFiles, listVceIntegrityIssues, listVceOperations } from "@/service/vce-service";
import type { VceProjectOverviewResponse } from "@/types";
import {
  nextVceManagementPageRequest,
  previousVceManagementPageRequest,
  VCE_MANAGEMENT_PAGE_SIZE,
  type VceManagementPageState,
} from "./code-context-pagination";
import type { VceProjectManagement } from "./use-vce-project-management";
import { MaintenancePlanPreview } from "./vce-maintenance-plan-preview";

// VceManagementDetails renders indexed files, Blob details, operation history and integrity maintenance.
// VceManagementDetails 渲染已索引文件、Blob 详情、操作历史与完整性维护。
export function VceManagementDetails(props: {
  // Exact project identity used by paged reads and plan confirmation.
  // 分页读取与计划确认使用的精确项目身份。
  projectId: string;
  // Current authoritative settings and runtime overview.
  // 当前权威设置与运行时概览。
  overview: VceProjectOverviewResponse;
  // Whether a live indexing job still owns the project's write path.
  // 是否仍有活跃索引作业持有项目写入路径。
  indexingActive: boolean;
  // Whether rebuild planning is already offered by the recovery section.
  // 重建计划是否已由恢复分区提供。
  rebuildRecoveryRequired: boolean;
  // Shared management state and actions.
  // 共享管理状态与动作。
  management: VceProjectManagement;
}) {
  // t localizes management lists and actions.
  // t 本地化管理列表与操作。
  const { t } = useI18n();
  const { rebuildRecoveryRequired } = props;
  const {
    managementOverview,
    managementBusy,
    filePage,
    setFilePage,
    operationPage,
    setOperationPage,
    issuePage,
    setIssuePage,
    blob,
    maintenancePlan,
    managementError,
    reloadManagement,
    loadManagementPage,
    inspectBlob,
    loadMaintenancePlan,
    executeMaintenance,
    retryOperation,
    cancelOperation,
  } = props.management;
  return (
    <>
      <section className="project-settings-panel space-y-2">
        <div className="flex items-center justify-between gap-2">
          <strong>{t("codeContext.area.filesAndBlobs")}</strong>
          <Button
            disabled={managementBusy}
            onClick={() => void reloadManagement()}
            size="sm"
            type="button"
            variant="outline"
          >
            <RefreshCw size={14} />
            {t("codeContext.area.refresh")}
          </Button>
        </div>
        {filePage.items.length === 0 ? (
          <p className="project-settings-muted">
            {t("codeContext.area.noIndexedFiles")}
          </p>
        ) : (
          filePage.items.map((file) => (
            <button
              className="flex w-full items-center justify-between gap-3 rounded-md border p-2 text-left text-sm"
              key={`${file.path}:${file.generation}`}
              onClick={() =>
                void inspectBlob(file.committedBlobName ?? file.blobName)
              }
              type="button"
            >
              <span className="min-w-0">
                <FileCode2 className="mr-2 inline" size={14} />
                <span className="break-all">{file.path}</span>
              </span>
              <span className="shrink-0 text-xs text-muted-foreground">
                {file.chunkCount} {t("codeContext.area.chunks2")}
              </span>
            </button>
          ))
        )}
        <ManagementPagination
          busy={managementBusy}
          onNext={() =>
            void loadManagementPage(
              nextVceManagementPageRequest(filePage),
              (cursor, readIdentity) =>
                listVceFiles(
                  props.projectId,
                  readIdentity,
                  cursor,
                  VCE_MANAGEMENT_PAGE_SIZE,
                ),
              setFilePage,
            )
          }
          onPrevious={() =>
            void loadManagementPage(
              previousVceManagementPageRequest(filePage),
              (cursor, readIdentity) =>
                listVceFiles(
                  props.projectId,
                  readIdentity,
                  cursor,
                  VCE_MANAGEMENT_PAGE_SIZE,
                ),
              setFilePage,
            )
          }
          page={filePage}
        />
      </section>

      {blob ? (
        <section className="project-settings-panel space-y-2">
          <strong>
            {t("codeContext.area.blobDetail")}: {blob.path}
          </strong>
          <p className="break-all text-xs text-muted-foreground">
            {blob.blobName}
          </p>
          {blob.chunks.map((chunk) => (
            <pre
              className="max-h-48 overflow-auto rounded-md border bg-muted/20 p-2 text-xs"
              key={chunk.chunkId}
            >{`${chunk.startLine}-${chunk.endLine} · ${chunk.kind}\n${chunk.preview ?? ""}`}</pre>
          ))}
        </section>
      ) : null}

      <section className="project-settings-panel space-y-2">
        <strong>{t("codeContext.area.operationHistory")}</strong>
        {operationPage.items.length === 0 ? (
          <p className="project-settings-muted">
            {t("codeContext.area.noOperationHistory")}
          </p>
        ) : (
          operationPage.items.map((operation) => (
            <div
              className="flex items-center justify-between gap-2 text-sm"
              key={operation.operationId}
            >
              <p>
                {operation.kind} · {operation.phase} ·{" "}
                {operation.completedUnits}/{operation.totalUnits}
                {operation.error ? ` · ${operation.error}` : ""}
              </p>
              {operation.phase === "failed" ||
              operation.phase === "cancelled" ? (
                <Button
                  disabled={managementBusy}
                  onClick={() => void retryOperation(operation.operationId)}
                  size="sm"
                  type="button"
                  variant="outline"
                >
                  {t("codeContext.area.retry")}
                </Button>
              ) : operation.phase !== "completed" ? (
                <Button
                  disabled={managementBusy}
                  onClick={() =>
                    void cancelOperation(operation.operationId)
                  }
                  size="sm"
                  type="button"
                  variant="outline"
                >
                  {t("codeContext.area.cancel")}
                </Button>
              ) : null}
            </div>
          ))
        )}
        <ManagementPagination
          busy={managementBusy}
          onNext={() =>
            void loadManagementPage(
              nextVceManagementPageRequest(operationPage),
              (cursor) =>
                listVceOperations(
                  props.projectId,
                  cursor,
                  VCE_MANAGEMENT_PAGE_SIZE,
                ),
              setOperationPage,
            )
          }
          onPrevious={() =>
            void loadManagementPage(
              previousVceManagementPageRequest(operationPage),
              (cursor) =>
                listVceOperations(
                  props.projectId,
                  cursor,
                  VCE_MANAGEMENT_PAGE_SIZE,
                ),
              setOperationPage,
            )
          }
          page={operationPage}
        />
      </section>

      <section className="project-settings-panel space-y-2">
        <strong>{t("codeContext.area.integrityAndMaintenance")}</strong>
        {issuePage.items.length === 0 ? (
          <p className="project-settings-muted">
            <SearchCheck className="mr-2 inline" size={14} />
            {managementOverview === null
              ? t("codeContext.area.integrityResultsHaveNot")
              : t("codeContext.area.noIntegrityIssuesFound")}
          </p>
        ) : (
          issuePage.items.map((issue) => (
            <div className="text-sm text-destructive" key={issue.issueId}>
              <p>
                {issue.kind}: {issue.message}
              </p>
              {issue.path != null ? (
                <p className="break-all font-mono text-xs">{issue.path}</p>
              ) : null}
            </div>
          ))
        )}
        <div className="flex flex-wrap gap-2">
          <Button
            disabled={
              managementBusy ||
              props.indexingActive ||
              !props.overview.configuration.ready
            }
            onClick={() => void loadMaintenancePlan("repair")}
            size="sm"
            type="button"
            variant="outline"
          >
            {t("codeContext.area.planRepair")}
          </Button>
          <Button
            disabled={
              managementBusy ||
              props.indexingActive ||
              !props.overview.configuration.ready
            }
            onClick={() => void loadMaintenancePlan("rebuild")}
            size="sm"
            type="button"
            variant="outline"
          >
            {t("codeContext.area.planRebuild")}
          </Button>
          <Button
            disabled={
              managementBusy ||
              props.indexingActive ||
              !props.overview.configuration.ready
            }
            onClick={() => void loadMaintenancePlan("cleanup_staging")}
            size="sm"
            type="button"
            variant="outline"
          >
            {t("codeContext.area.planStagingCleanup")}
          </Button>
          {props.overview.configuration.ready ? (
            <Button
              disabled={managementBusy || props.indexingActive}
              onClick={() => void loadMaintenancePlan("compact")}
              size="sm"
              type="button"
              variant="outline"
            >
              {t("codeContext.area.inspectAndCompactPhysical")}
            </Button>
          ) : null}
        </div>
        {maintenancePlan &&
        (maintenancePlan.mode !== "compact" ||
          props.overview.configuration.ready) &&
        (!rebuildRecoveryRequired || maintenancePlan.mode !== "rebuild") ? (
          <MaintenancePlanPreview
            active={props.indexingActive}
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
        <ManagementPagination
          busy={managementBusy}
          onNext={() =>
            void loadManagementPage(
              nextVceManagementPageRequest(issuePage),
              (cursor, readIdentity) =>
                listVceIntegrityIssues(
                  props.projectId,
                  readIdentity,
                  cursor,
                  VCE_MANAGEMENT_PAGE_SIZE,
                ),
              setIssuePage,
            )
          }
          onPrevious={() =>
            void loadManagementPage(
              previousVceManagementPageRequest(issuePage),
              (cursor, readIdentity) =>
                listVceIntegrityIssues(
                  props.projectId,
                  readIdentity,
                  cursor,
                  VCE_MANAGEMENT_PAGE_SIZE,
                ),
              setIssuePage,
            )
          }
          page={issuePage}
        />
      </section>
    </>
  );
}

// ManagementPagination renders cursor-safe previous and next controls for one bounded list.
// ManagementPagination 为一项有界列表渲染游标安全的上一页与下一页控件。
function ManagementPagination<T>(props: {
  // Whether another management request is active.
  // 是否有其他管理请求正在执行。
  busy: boolean;
  // Loads the immediately following opaque page.
  // 加载紧邻的下一不透明分页。
  onNext: () => void;
  // Loads the immediately preceding opaque page.
  // 加载紧邻的上一不透明分页。
  onPrevious: () => void;
  // Current page and cursor history.
  // 当前分页及游标历史。
  page: VceManagementPageState<T>;
}) {
  const { t } = useI18n();
  const pageNumber = props.page.previousCursors.length + 1;
  return (
    <div className="flex items-center justify-end gap-2 text-xs text-muted-foreground">
      <span>
        {t("codeContext.area.page")} {pageNumber}
        {props.page.total === null
          ? ""
          : ` · ${t("codeContext.area.total")} ${props.page.total}`}
      </span>
      <Button
        disabled={props.busy || props.page.previousCursors.length === 0}
        onClick={props.onPrevious}
        size="sm"
        type="button"
        variant="outline"
      >
        {t("codeContext.area.previous")}
      </Button>
      <Button
        disabled={props.busy || props.page.nextCursor === null}
        onClick={props.onNext}
        size="sm"
        type="button"
        variant="outline"
      >
        {t("codeContext.area.next")}
      </Button>
    </div>
  );
}
