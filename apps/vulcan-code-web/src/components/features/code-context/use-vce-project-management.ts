import * as React from "react";
import { useI18n } from "@/i18n";
import type { ServiceRequestOptions } from "@/service/protocol";
import {
  cancelVceOperation,
  executeVceMaintenance,
  listVceFiles,
  listVceIntegrityIssues,
  listVceOperations,
  loadVceBlobDetail,
  loadVceProjectOverview,
  planVceMaintenance,
  retryVceOperation,
} from "@/service/vce-service";
import type {
  VceBlobDetailResponse,
  VceFileSummary,
  VceIntegrityIssueSummary,
  VceMaintenancePlanResponse,
  VceOperationSummary,
  VceProjectOverviewResponse,
} from "@/types";
import {
  emptyVceManagementPage,
  materializeVceManagementPage,
  VCE_MANAGEMENT_PAGE_SIZE,
  type VceManagementPageRequest,
  type VceManagementPageState,
} from "./code-context-pagination";
import type { useVceProjectJob } from "./use-vce-project-job";
import type { VceMaintenanceConfirmation } from "./vce-maintenance-plan-preview";
import {
  VCE_PROJECT_OVERVIEW_READ_DEADLINE_MS,
  VCE_PROJECT_OVERVIEW_READ_TIMEOUT_MESSAGE_KEY,
} from "./vce-observer-timeouts";

// VceProjectJob is the durable project job observer shared by the settings panel.
// VceProjectJob 是设置面板共享的项目持久作业观察器。
export type VceProjectJob = ReturnType<typeof useVceProjectJob>;

// loadAuthoritativeVceOverview reads and validates the current host-owned project snapshot.
// loadAuthoritativeVceOverview 读取并校验当前由宿主持有的项目快照。
//
// Parameters: projectId is the exact mounted project and loader performs the service read.
// 参数：projectId 是精确的已挂载项目，loader 执行服务读取。
// options optionally carries the caller-owned abort signal for this read.
// 参数：options 可选携带调用方持有的本次读取中止信号。
// Returns: the validated current overview for the requested project.
// 返回值：请求项目经过校验的当前概览。
export async function loadAuthoritativeVceOverview(
  projectId: string,
  loader: (
    projectId: string,
    options?: ServiceRequestOptions,
  ) => Promise<VceProjectOverviewResponse> = loadVceProjectOverview,
  options?: ServiceRequestOptions,
): Promise<VceProjectOverviewResponse> {
  const overview = options
    ? await loader(projectId, options)
    : await loader(projectId);
  if (overview.status.projectId !== projectId) {
    throw new Error(
      "VCE overview recovery returned a mismatched project identity.",
    );
  }
  return overview;
}

// VceProjectManagementOptions binds management reads and mutations to one mounted project.
// VceProjectManagementOptions 将管理读取与修改绑定到一个已挂载项目。
export interface VceProjectManagementOptions {
  // Exact project identity selected by the settings dialog.
  // 设置弹窗选择的精确项目身份。
  projectId: string;
  // Current authoritative settings and runtime overview.
  // 当前权威设置与运行时概览。
  overview: VceProjectOverviewResponse | null;
  // Replaces the parent-owned overview after a management mutation.
  // 在管理修改后替换父级持有的概览。
  onOverviewChanged: (overview: VceProjectOverviewResponse) => void;
  // Durable project job that owns the write path while active.
  // 活跃时持有写入路径的项目持久作业。
  indexing: VceProjectJob;
  // Whether the admitted project and service lifetime are still current.
  // 准入项目与服务生命周期是否仍为当前。
  isCurrentAction: () => boolean;
}

// useVceProjectManagement owns paged management lists, Blob details, maintenance plans and operation actions.
// useVceProjectManagement 持有分页管理列表、Blob 详情、维护计划与操作动作。
// Parameters: props binds the mounted project, its overview and job observer. Returns: management state and actions.
// 参数：props 绑定已挂载项目、其概览与作业观察器。返回值：管理状态与动作。
export function useVceProjectManagement(props: VceProjectManagementOptions) {
  // t localizes management diagnostics.
  // t 本地化管理诊断信息。
  const { t } = useI18n();
  const { indexing, isCurrentAction } = props;
  // managementExpanded is a human request to load optional lists and aggregate counts.
  // managementExpanded 为人为请求加载可选列表和聚合计数。
  const [managementExpanded, setManagementExpanded] = React.useState(false);
  // managementOverview retains statistics only for their exact project/checkpoint/settings identity.
  // managementOverview 仅按其精确项目、检查点及设置身份保留统计。
  const [managementOverview, setManagementOverview] =
    React.useState<VceProjectOverviewResponse | null>(props.overview);
  // managementBusy covers bounded file, operation, and integrity reads.
  // managementBusy 表示有界文件、操作与完整性读取正在进行。
  const [managementBusy, setManagementBusy] = React.useState(false);
  // filePage stores one bounded file page and its opaque navigation history.
  // filePage 保存一页有界文件记录及其不透明导航历史。
  const [filePage, setFilePage] = React.useState<
    VceManagementPageState<VceFileSummary>
  >(emptyVceManagementPage);
  // operationPage stores one bounded durable-operation page.
  // operationPage 保存一页有界持久操作记录。
  const [operationPage, setOperationPage] = React.useState<
    VceManagementPageState<VceOperationSummary>
  >(emptyVceManagementPage);
  // issuePage stores one bounded integrity-difference page.
  // issuePage 保存一页有界完整性差异。
  const [issuePage, setIssuePage] = React.useState<
    VceManagementPageState<VceIntegrityIssueSummary>
  >(emptyVceManagementPage);
  // blob stores the currently inspected exact Blob detail.
  // blob 保存当前检查的精确 Blob 详情。
  const [blob, setBlob] = React.useState<VceBlobDetailResponse | null>(null);
  // maintenancePlan stores the latest read-only maintenance preview.
  // maintenancePlan 保存最近一份只读维护预览。
  const [maintenancePlan, setMaintenancePlan] =
    React.useState<VceMaintenancePlanResponse | null>(null);
  // managementError stores a bounded management-load diagnostic.
  // managementError 保存一条有界管理加载诊断。
  const [managementError, setManagementError] = React.useState<string | null>(
    null,
  );
  // managementRequestVersion invalidates stale asynchronous responses after project or page changes.
  // managementRequestVersion 在项目或分页变化后使陈旧异步响应失效。
  const managementRequestVersion = React.useRef(0);
  // managementOverviewReadAbortController owns the optional management overview transport.
  // managementOverviewReadAbortController 持有可选管理概览读取的传输中止控制器。
  const managementOverviewReadAbortController =
    React.useRef<AbortController | null>(null);
  // managementOverviewReadDeadlineTimer bounds the optional overview request by its generation.
  // managementOverviewReadDeadlineTimer 按请求代次限制可选概览请求的最长等待时间。
  const managementOverviewReadDeadlineTimer = React.useRef<ReturnType<
    typeof setTimeout
  > | null>(null);
  // maintenanceMutationInFlightRef prevents double submission before React can repaint disabled controls.
  // maintenanceMutationInFlightRef 防止 React 重绘禁用控件前重复提交维护操作。
  const maintenanceMutationInFlightRef = React.useRef(false);

  React.useLayoutEffect(() => {
    managementRequestVersion.current += 1;
    maintenanceMutationInFlightRef.current = false;
    setManagementBusy(false);
    setMaintenancePlan(null);
    setManagementError(null);
    return () => {
      managementRequestVersion.current += 1;
      // Retiring this panel aborts only its browser read; it never cancels a host operation.
      // 此面板退役时仅中止浏览器读取，绝不取消宿主作业。
      if (managementOverviewReadDeadlineTimer.current !== null) {
        clearTimeout(managementOverviewReadDeadlineTimer.current);
        managementOverviewReadDeadlineTimer.current = null;
      }
      managementOverviewReadAbortController.current?.abort();
      managementOverviewReadAbortController.current = null;
    };
  }, [isCurrentAction]);

  // reloadManagement binds content lists to one overview identity; operation history remains live.
  // reloadManagement 将内容列表绑定同一概览身份；操作历史保持实时。
  const reloadManagement = React.useCallback(async () => {
    const requestVersion = managementRequestVersion.current + 1;
    managementRequestVersion.current = requestVersion;
    // A new management request retires the previous overview read before starting another one.
    // 新的管理请求会先退役上一次概览读取，再开始新的读取。
    managementOverviewReadAbortController.current?.abort();
    if (managementOverviewReadDeadlineTimer.current !== null) {
      clearTimeout(managementOverviewReadDeadlineTimer.current);
      managementOverviewReadDeadlineTimer.current = null;
    }
    // Retired details and lists cannot remain beside a newly published overview, even on read failure.
    // 即使读取失败，退役详情和列表也不能继续与新发布概览并列显示。
    setManagementOverview(null);
    setMaintenancePlan(null);
    setFilePage(emptyVceManagementPage());
    setOperationPage(emptyVceManagementPage());
    setIssuePage(emptyVceManagementPage());
    setBlob(null);
    if (
      !managementExpanded ||
      !props.overview?.configuration.ready ||
      !props.overview.settings.enabled ||
      props.overview.status.checkpointVersion == null
    ) {
      setFilePage(emptyVceManagementPage());
      setOperationPage(emptyVceManagementPage());
      setIssuePage(emptyVceManagementPage());
      setBlob(null);
      setManagementBusy(false);
      return;
    }
    // controller owns this overview read and its browser abort signal.
    // controller 持有本次概览读取及其浏览器中止信号。
    const controller = new AbortController();
    // timedOut distinguishes a retryable deadline from a silent lifecycle abort.
    // timedOut 区分可重试的期限超时与静默生命周期中止。
    let timedOut = false;
    // deadline bounds this request and is cleared by completion or retirement.
    // deadline 限制本次请求，并在完成或退役时清理。
    const deadline = setTimeout(() => {
      timedOut = true;
      controller.abort();
    }, VCE_PROJECT_OVERVIEW_READ_DEADLINE_MS);
    managementOverviewReadAbortController.current = controller;
    managementOverviewReadDeadlineTimer.current = deadline;
    setManagementBusy(true);
    setManagementError(null);
    try {
      // Pin the host-issued identity before requesting lists; never infer it from a numeric checkpoint.
      // 请求列表之前固定宿主签发身份；绝不从数字检查点推断身份。
      const overview = await loadAuthoritativeVceOverview(
        props.projectId,
        loadVceProjectOverview,
        {
          signal: controller.signal,
        },
      );
      clearTimeout(deadline);
      if (managementOverviewReadDeadlineTimer.current === deadline) {
        managementOverviewReadDeadlineTimer.current = null;
      }
      if (managementOverviewReadAbortController.current === controller) {
        managementOverviewReadAbortController.current = null;
      }
      if (controller.signal.aborted) {
        if (timedOut && managementRequestVersion.current === requestVersion) {
          setManagementError(t(VCE_PROJECT_OVERVIEW_READ_TIMEOUT_MESSAGE_KEY));
        }
        return;
      }
      if (managementRequestVersion.current !== requestVersion) return;
      if (!overview.readIdentity)
        throw new Error(
          t("codeContext.area.noPublishedIndexIs"),
        );
      const [filePage, operationPage, issuePage] = await Promise.all([
        listVceFiles(
          props.projectId,
          overview.readIdentity,
          null,
          VCE_MANAGEMENT_PAGE_SIZE,
        ),
        listVceOperations(props.projectId, null, VCE_MANAGEMENT_PAGE_SIZE),
        listVceIntegrityIssues(
          props.projectId,
          overview.readIdentity,
          null,
          VCE_MANAGEMENT_PAGE_SIZE,
        ),
      ]);
      if (managementRequestVersion.current !== requestVersion) {
        return;
      }
      setManagementOverview(overview);
      const firstPageRequest = { cursor: null, previousCursors: [] };
      setFilePage(materializeVceManagementPage(filePage, firstPageRequest));
      setOperationPage(
        materializeVceManagementPage(operationPage, firstPageRequest),
      );
      setIssuePage(materializeVceManagementPage(issuePage, firstPageRequest));
    } catch (error) {
      if (controller.signal.aborted) {
        if (timedOut && managementRequestVersion.current === requestVersion) {
          setManagementError(t(VCE_PROJECT_OVERVIEW_READ_TIMEOUT_MESSAGE_KEY));
        }
      } else if (managementRequestVersion.current === requestVersion) {
        setManagementError(
          error instanceof Error ? error.message : String(error),
        );
      }
    } finally {
      clearTimeout(deadline);
      if (managementOverviewReadDeadlineTimer.current === deadline) {
        managementOverviewReadDeadlineTimer.current = null;
      }
      if (managementOverviewReadAbortController.current === controller) {
        managementOverviewReadAbortController.current = null;
      }
      if (managementRequestVersion.current === requestVersion) {
        setManagementBusy(false);
      }
    }
  }, [
    isCurrentAction,
    managementExpanded,
    props.overview?.configuration.ready,
    props.overview?.settings.enabled,
    props.overview?.status.checkpointVersion,
    props.overview?.settings.revision,
    props.projectId,
  ]);

  React.useEffect(() => {
    // A replacement can publish a new physical namespace at the same numeric checkpoint.
    // 替换可以在相同数字检查点发布新的物理命名空间。
    // Durable job identity/state changes retire lists and previews; progress-only sequence changes do not scan.
    // 持久作业身份或状态变化使列表与预览退役；仅进度序号变化不触发扫描。
    void reloadManagement();
    return () => {
      managementRequestVersion.current += 1;
    };
  }, [
    reloadManagement,
    props.overview?.status.checkpointVersion,
    indexing.job?.jobId,
    indexing.job?.state,
  ]);

  // loadManagementPage commits only the latest page response for the selected project.
  // loadManagementPage 只提交所选项目的最新分页响应。
  const loadManagementPage = React.useCallback(
    async <T,>(
      request: VceManagementPageRequest | null,
      loader: (
        cursor: string | null,
        readIdentity: string,
      ) => Promise<import("@/types").VcePageResponse<T>>,
      applyPage: (page: VceManagementPageState<T>) => void,
    ) => {
      if (!request) {
        return;
      }
      const requestVersion = managementRequestVersion.current + 1;
      managementRequestVersion.current = requestVersion;
      setManagementBusy(true);
      setManagementError(null);
      try {
        if (!managementOverview?.readIdentity)
          throw new Error(
            t("codeContext.area.refreshIndexDetailsFirst"),
          );
        const response = await loader(
          request.cursor,
          managementOverview.readIdentity,
        );
        if (managementRequestVersion.current === requestVersion) {
          applyPage(materializeVceManagementPage(response, request));
        }
      } catch (error) {
        if (managementRequestVersion.current === requestVersion) {
          setManagementError(
            error instanceof Error ? error.message : String(error),
          );
        }
      } finally {
        if (managementRequestVersion.current === requestVersion) {
          setManagementBusy(false);
        }
      }
    },
    [managementOverview],
  );

  // inspectBlob loads only the selected immutable Blob identity.
  // inspectBlob 只加载所选不可变 Blob 身份。
  const inspectBlob = async (blobName: string | null | undefined) => {
    if (!blobName) {
      return;
    }
    const projectId = props.projectId;
    // Use the same request fence as overview and pagination so delayed details cannot reappear.
    // 与概览及分页共用请求保护，避免迟到详情重新出现。
    const requestVersion = ++managementRequestVersion.current;
    setBlob(null);
    setManagementBusy(true);
    setManagementError(null);
    try {
      if (!managementOverview?.readIdentity)
        throw new Error(
          t("codeContext.area.refreshIndexDetailsFirst"),
        );
      const nextBlob = await loadVceBlobDetail(
        projectId,
        blobName,
        managementOverview.readIdentity,
      );
      if (
        isCurrentAction() &&
        managementRequestVersion.current === requestVersion
      ) {
        setBlob(nextBlob);
      }
    } catch (error) {
      if (
        isCurrentAction() &&
        managementRequestVersion.current === requestVersion
      ) {
        setManagementError(
          error instanceof Error ? error.message : String(error),
        );
      }
    } finally {
      if (
        isCurrentAction() &&
        managementRequestVersion.current === requestVersion
      ) {
        setManagementBusy(false);
      }
    }
  };

  // loadMaintenancePlan obtains a read-only plan and performs no mutation.
  // loadMaintenancePlan 获取只读计划且不执行修改。
  const loadMaintenancePlan = async (
    mode: "repair" | "rebuild" | "cleanup_staging" | "compact",
  ) => {
    const projectId = props.projectId;
    // A live durable job owns the project's write path until it reaches a terminal state.
    // 活跃持久作业在进入终态前持有项目写入路径。
    if (indexing.active) {
      setManagementError(
        t("codeContext.area.theCurrentIndexJob"),
      );
      return;
    }
    // Only rebuild can recover an uninitialized or failed first publication.
    // 只有重建可以恢复尚未初始化或首次发布失败的项目。
    if (
      mode !== "rebuild" &&
      mode !== "compact" &&
      props.overview?.status.checkpointVersion == null
    ) {
      setManagementError(
        t("codeContext.area.thisProjectHasNo"),
      );
      return;
    }
    // A preview is retired by the same publication, collapse and reload boundary as its displayed data.
    // 预览与其显示数据共享发布、折叠及重载退役边界。
    const requestVersion = ++managementRequestVersion.current;
    setMaintenancePlan(null);
    setManagementBusy(true);
    setManagementError(null);
    try {
      const nextPlan = await planVceMaintenance(projectId, mode);
      if (
        isCurrentAction() &&
        managementRequestVersion.current === requestVersion
      ) {
        setMaintenancePlan(nextPlan);
      }
    } catch (error) {
      if (
        isCurrentAction() &&
        managementRequestVersion.current === requestVersion
      ) {
        setManagementError(
          error instanceof Error ? error.message : String(error),
        );
      }
    } finally {
      if (
        isCurrentAction() &&
        managementRequestVersion.current === requestVersion
      ) {
        setManagementBusy(false);
      }
    }
  };

  // executeMaintenance executes only a dialog-frozen current read-only plan.
  // executeMaintenance 仅执行由对话框冻结且仍然有效的当前只读计划。
  const executeMaintenance = async (
    confirmation: VceMaintenanceConfirmation,
  ) => {
    const overview = props.overview;
    if (maintenanceMutationInFlightRef.current) {
      return;
    }
    if (!overview || !maintenancePlan || maintenancePlan.actions.length === 0) {
      setManagementError(
        t("codeContext.area.theCurrentPlanHas"),
      );
      return;
    }
    if (indexing.active) {
      setManagementError(
        t("codeContext.area.theCurrentIndexJob"),
      );
      return;
    }
    const projectId = props.projectId;
    const checkpoint = overview.status.checkpointVersion ?? null;
    const plannedCheckpoint = maintenancePlan.checkpointVersion;
    const actionIds = maintenancePlan.actions.map((action) => action.actionId);
    // A model-unavailable overview intentionally hides query authority and its checkpoint. Compact
    // mode instead retains the exact stored checkpoint from its plan; the host revalidates it at admission.
    // 模型不可用概览会有意隐藏查询权威及其检查点。compact 模式改用计划中的精确已存检查点；宿主在准入时再次校验。
    const storedCompactionAuthority =
      maintenancePlan.mode === "compact" &&
      !overview.configuration.ready &&
      checkpoint === null &&
      plannedCheckpoint !== null;
    // Model replacement hides the predecessor checkpoint in the overview while its rebuild
    // plan retains that exact stored checkpoint; the host validates it at job admission.
    // 模型替换会在概览中隐藏前驱检查点，而重建计划保留精确的已存检查点；宿主在作业准入时校验。
    const storedRebuildAuthority =
      maintenancePlan.mode === "rebuild" &&
      overview.status.lifecycle === "rebuild_required" &&
      checkpoint === null &&
      plannedCheckpoint !== null;
    const planStillCurrent =
      confirmation.projectId === projectId &&
      confirmation.settingsRevision === overview.settings.revision &&
      confirmation.checkpointVersion === plannedCheckpoint &&
      (storedCompactionAuthority || storedRebuildAuthority || plannedCheckpoint === checkpoint) &&
      confirmation.mode === maintenancePlan.mode &&
      confirmation.embeddingFingerprint ===
        maintenancePlan.embeddingFingerprint &&
      confirmation.actionIds.length === actionIds.length &&
      confirmation.actionIds.every(
        (actionId, index) => actionId === actionIds[index],
      );
    if (!planStillCurrent) {
      setMaintenancePlan(null);
      setManagementError(
        t("codeContext.area.theMaintenancePlanIs"),
      );
      return;
    }
    if (plannedCheckpoint === null && maintenancePlan.mode !== "rebuild") {
      setManagementError(
        t("codeContext.area.maintenanceRequiresAPublished"),
      );
      return;
    }
    maintenanceMutationInFlightRef.current = true;
    setManagementBusy(true);
    setManagementError(null);
    try {
      // result distinguishes durable admission from completed cleanup; it is never assumed to be completion.
      // result 区分持久准入与已完成清理；绝不把准入当作完成。
      const result = await executeVceMaintenance(
        projectId,
        maintenancePlan.mode,
        overview.settings.revision,
        plannedCheckpoint,
        actionIds,
        confirmation.idempotencyKey,
        maintenancePlan.embeddingFingerprint,
      );
      if (!isCurrentAction()) {
        return;
      }
      if (result.kind === "job") {
        indexing.receive(result.job);
      } else {
        props.onOverviewChanged(result.overview);
      }
      setMaintenancePlan(null);
      await reloadManagement();
    } catch (error) {
      if (isCurrentAction()) {
        setManagementError(
          error instanceof Error ? error.message : String(error),
        );
      }
    } finally {
      if (isCurrentAction()) {
        maintenanceMutationInFlightRef.current = false;
        setManagementBusy(false);
      }
    }
  };

  // retryOperation reruns one failed or cancelled operation as a guarded recovery refresh.
  // retryOperation 将一项失败或已取消操作重跑为受保护恢复刷新。
  const retryOperation = async (operationId: string) => {
    const overview = props.overview;
    if (!overview || maintenanceMutationInFlightRef.current) {
      return;
    }
    const projectId = props.projectId;
    maintenanceMutationInFlightRef.current = true;
    setManagementBusy(true);
    setManagementError(null);
    try {
      const receipt = await retryVceOperation(
        projectId,
        operationId,
        overview.settings.revision,
        overview.status.checkpointVersion ?? null,
      );
      if (!isCurrentAction()) {
        return;
      }
      indexing.receive(receipt);
    } catch (error) {
      if (isCurrentAction()) {
        setManagementError(
          error instanceof Error ? error.message : String(error),
        );
      }
    } finally {
      if (isCurrentAction()) {
        maintenanceMutationInFlightRef.current = false;
        setManagementBusy(false);
      }
    }
  };

  // cancelOperation requests cancellation only for the exact visible active operation.
  // cancelOperation 仅请求取消精确可见的活动操作。
  const cancelOperation = async (operationId: string) => {
    const overview = props.overview;
    if (!overview || maintenanceMutationInFlightRef.current) {
      return;
    }
    const projectId = props.projectId;
    maintenanceMutationInFlightRef.current = true;
    setManagementBusy(true);
    setManagementError(null);
    try {
      const receipt = await cancelVceOperation(projectId, operationId);
      if (!isCurrentAction()) {
        return;
      }
      indexing.receive(receipt);
    } catch (error) {
      if (isCurrentAction()) {
        setManagementError(
          error instanceof Error ? error.message : String(error),
        );
      }
    } finally {
      if (isCurrentAction()) {
        maintenanceMutationInFlightRef.current = false;
        setManagementBusy(false);
      }
    }
  };

  return {
    managementExpanded,
    setManagementExpanded,
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
    setManagementError,
    reloadManagement,
    loadManagementPage,
    inspectBlob,
    loadMaintenancePlan,
    executeMaintenance,
    retryOperation,
    cancelOperation,
  };
}

// VceProjectManagement is the management state and action surface shared with presentational sections.
// VceProjectManagement 是与展示分区共享的管理状态与动作接口。
export type VceProjectManagement = ReturnType<typeof useVceProjectManagement>;
