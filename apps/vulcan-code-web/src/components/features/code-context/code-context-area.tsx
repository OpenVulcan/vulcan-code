import * as React from "react";
import {
  ChevronDown,
  Database,
  FileCode2,
  LoaderCircle,
  RefreshCw,
  Save,
  SearchCheck,
  SlidersHorizontal,
} from "lucide-react";
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
import { StreamPlaceholder } from "@/components/ui/stream-placeholder";
import { Input } from "@/components/ui/input";
import { Switch } from "@/components/ui/switch";
import { Textarea } from "@/components/ui/textarea";
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
  VceJobSummary,
  VceMaintenancePlanResponse,
  VceOperationSummary,
  VceProjectOverviewResponse,
  VceProjectSettingsInput,
  VceProjectSettingsSummary,
} from "@/types";
import {
  emptyVceManagementPage,
  materializeVceManagementPage,
  nextVceManagementPageRequest,
  previousVceManagementPageRequest,
  VCE_MANAGEMENT_PAGE_SIZE,
  type VceManagementPageRequest,
  type VceManagementPageState,
} from "./code-context-pagination";
import { useVceProjectJob } from "./use-vce-project-job";
import { useVceActionScope } from "./use-vce-action-scope";
import { VceJobProgress, vceActiveJobStatusLabel, vceSynchronizationControl } from "./vce-job-progress";
import {
  VCE_PROJECT_OVERVIEW_READ_DEADLINE_MS,
  VCE_PROJECT_OVERVIEW_READ_TIMEOUT_MESSAGE_KEY,
} from "./vce-observer-timeouts";

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

// VceMaintenanceConfirmation freezes the exact plan authority at dialog-open time.
// VceMaintenanceConfirmation 在对话框打开时冻结计划的精确权威。
interface VceMaintenanceConfirmation {
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
  // Actions retain the admitted project and service lifetime, not merely a reusable project ID.
  // 操作保留准入项目和服务生命周期，而不只是可重复使用的项目编号。
  const isCurrentAction = useVceActionScope(props.projectId);
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
      ) : null}

      {status.lastError && !indexingStatusVisible && status.lastError !== indexing.job?.error ? (
        <p className="rounded-lg border border-destructive/30 bg-destructive/5 p-3 text-sm text-destructive">
          {status.lastError}
        </p>
      ) : null}

      <details className="vce-settings-disclosure vce-advanced-settings">
        <summary>
          <SlidersHorizontal size={16} aria-hidden="true" />
          <span><strong>{t("codeContext.area.retrievalAndFileFilters")}</strong>
            <small>{t("codeContext.area.ignoreRulesResultLimits")}</small>
          </span>
          <ChevronDown size={16} aria-hidden="true" />
        </summary>
        <div className="vce-settings-disclosure-body">
          <details className="rounded-lg border p-3 text-sm">
            <summary className="cursor-pointer font-medium">
              {t("codeContext.area.defaultFilterRules")} ({props.overview.settings.defaultIgnorePatterns.length})
            </summary>
            <p className="mt-2 text-xs text-muted-foreground">
              {t("codeContext.area.theseRulesAlwaysApply")}
            </p>
            <pre className="mt-2 max-h-48 overflow-auto whitespace-pre-wrap rounded bg-muted p-2 text-xs">
              {props.overview.settings.defaultIgnorePatterns.join("\n")}
            </pre>
          </details>
          <label className="grid gap-1 text-sm">
            {t("codeContext.area.customIgnoreRulesOne")}
            <Textarea
              disabled={props.busy}
              onChange={(event) =>
                setDraft({
                  ...draft,
                  extraIgnorePatterns: event.target.value.split(/\r?\n/),
                })
              }
              value={draft.extraIgnorePatterns.join("\n")}
            />
          </label>
          <div className="grid grid-cols-2 gap-3">
            <NumericSetting
              disabled={props.busy}
              label={t("codeContext.area.maximumFileBytes")}
              value={draft.maxFileBytes}
              onChange={(value) => setDraft({ ...draft, maxFileBytes: value })}
            />
            <NumericSetting
              disabled={props.busy}
              label={t("codeContext.area.maximumResults")}
              value={draft.maxResults}
              onChange={(value) => setDraft({ ...draft, maxResults: value })}
            />
            <NumericSetting
              disabled={props.busy}
              label={t("codeContext.area.contextCharacterBudget")}
              value={draft.maxContextChars}
              onChange={(value) => setDraft({ ...draft, maxContextChars: value })}
            />
            <NumericSetting
              disabled={props.busy}
              label={t("codeContext.area.embeddingConcurrency")}
              value={draft.embeddingMaxConcurrency}
              onChange={(value) =>
                setDraft({ ...draft, embeddingMaxConcurrency: value })
              }
            />
            <NumericSetting
              disabled={props.busy}
              label={t("codeContext.area.queryConcurrency")}
              value={draft.queryMaxConcurrency}
              onChange={(value) =>
                setDraft({ ...draft, queryMaxConcurrency: value })
              }
            />
          </div>
          <ToggleSetting
            checked={draft.allowQueryRewrite}
            disabled={props.busy}
            label={t("codeContext.area.allowLlmQueryRewriting")}
            onChange={(value) => setDraft({ ...draft, allowQueryRewrite: value })}
          />
          <ToggleSetting
            checked={draft.allowRerank}
            disabled={props.busy}
            label={t("codeContext.area.allowCandidateReranking")}
            onChange={(value) => setDraft({ ...draft, allowRerank: value })}
          />
          <Button
            disabled={props.busy}
            onClick={() => void saveDraft(draft)}
            type="button"
          >
            <Save size={15} /> {t("codeContext.area.saveSettings")}
          </Button>
        </div>
      </details>

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
                  indexing.active ||
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
                  indexing.active ||
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
                  indexing.active ||
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
                  disabled={managementBusy || indexing.active}
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

// MaintenancePlanPreview renders one exact read-only plan and its explicit confirmation action.
// MaintenancePlanPreview 渲染一份精确的只读计划及其明确确认操作。
function MaintenancePlanPreview(props: {
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

// NumericSetting renders one explicit integer input used by a VCE budget.
// NumericSetting 渲染一个用于 VCE 预算的明确整数输入。
function NumericSetting(props: {
  // Whether the setting must reject edits during an authoritative host mutation.
  // 设置在权威宿主修改期间是否必须拒绝编辑。
  disabled: boolean;
  // Human-facing setting label.
  // 面向用户的设置标签。
  label: string;
  // Current integer value.
  // 当前整数值。
  value: number;
  // Receives the exact parsed integer.
  // 接收精确解析的整数。
  onChange: (value: number) => void;
}) {
  return (
    <label className="grid gap-1 text-sm">
      {props.label}
      <Input
        disabled={props.disabled}
        min={1}
        onChange={(event) =>
          props.onChange(Number.parseInt(event.target.value, 10) || 0)
        }
        type="number"
        value={props.value}
      />
    </label>
  );
}

// ToggleSetting renders one labeled Boolean VCE setting.
// ToggleSetting 渲染一个带标签的布尔 VCE 设置。
function ToggleSetting(props: {
  // Current Boolean setting value.
  // 当前布尔设置值。
  checked: boolean;
  // Whether the toggle must reject edits during an authoritative host mutation.
  // 开关在权威宿主修改期间是否必须拒绝编辑。
  disabled: boolean;
  // Human-facing setting label.
  // 面向用户的设置标签。
  label: string;
  // Receives the next explicit Boolean value.
  // 接收下一项明确布尔值。
  onChange: (value: boolean) => void;
}) {
  return (
    <label className="flex items-center gap-2 text-sm">
      <Switch
        checked={props.checked}
        disabled={props.disabled}
        onCheckedChange={props.onChange}
      />
      <span>{props.label}</span>
    </label>
  );
}
