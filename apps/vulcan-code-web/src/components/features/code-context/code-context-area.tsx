import * as React from "react";
import { Database, FileCode2, LoaderCircle, RefreshCw, Save, SearchCheck } from "lucide-react";
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
import { VceJobProgress, vceSynchronizationControl } from "./vce-job-progress";
import {
  VCE_PROJECT_OVERVIEW_READ_DEADLINE_MS,
  VCE_PROJECT_OVERVIEW_READ_TIMEOUT_MESSAGE,
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
  loader: (projectId: string, options?: ServiceRequestOptions) => Promise<VceProjectOverviewResponse> = loadVceProjectOverview,
  options?: ServiceRequestOptions,
): Promise<VceProjectOverviewResponse> {
  const overview = options ? await loader(projectId, options) : await loader(projectId);
  if (overview.status.projectId !== projectId) {
    throw new Error("VCE overview recovery returned a mismatched project identity.");
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
  onSave: (settings: VceProjectSettingsInput, expectedRevision: number) => Promise<VceProjectSettingsSummary>;
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
  // locale selects local bilingual copy without exposing VCE implementation names to users.
  // locale 选择本地双语文案，且不向用户暴露 VCE 实现名称。
  const { locale, t } = useI18n();
  // indexing follows this project's independent durable job, not the conversation state.
  // indexing 跟随本项目独立持久作业，而非会话状态。
  const indexing = useVceProjectJob(props.projectId);
  // projectReportsActiveJob is the host lifecycle projection available before the receipt observer finishes rehydrating.
  // projectReportsActiveJob 是作业回执观察器完成恢复前，宿主已经提供的生命周期投影。
  const projectReportsActiveJob = props.overview?.status.lifecycle === "indexing";
  // Use the acknowledged project job to offer at most one subsequent manual synchronization.
  // 使用已确认项目作业提供至多一次后续手动同步。
  const synchronization = projectReportsActiveJob
    ? { disabled: true, label: locale === "zh-CN" ? "索引任务进行中" : "Index job in progress" }
    : vceSynchronizationControl(indexing.job, locale === "zh-CN");
  // zh returns Chinese copy for the active Chinese locale and English otherwise.
  // zh 为当前中文语言返回中文文案，否则返回英文文案。
  const copy = React.useCallback(
    (zh: string, en: string) => (locale === "zh-CN" ? zh : en),
    [locale],
  );
  // replacementInProgressActivityError identifies the expected read race while a durable replacement owns the project.
  // replacementInProgressActivityError 识别持久替换作业持有项目期间预期的读取竞争。
  const replacementInProgressActivityError = props.activityError === "VCE explicit replacement is in progress; observe its durable job";
  // indexingStatusVisible turns active work into a neutral localized status instead of an inline error.
  // indexingStatusVisible 将活动作业转换为中性的本地化状态，而不是行内错误。
  const indexingStatusVisible = indexing.active || projectReportsActiveJob || replacementInProgressActivityError;
  // draft holds only user-editable fields and never mirrors host-owned revision fields.
  // draft 只保存用户可编辑字段，绝不镜像宿主持有的修订字段。
  const [draft, setDraft] = React.useState<VceProjectSettingsInput | null>(() => (
    props.overview ? editableSettings(props.overview.settings) : null
  ));
  // draftBase freezes the exact persisted revision from which the user's editable draft was created.
  // draftBase 固定用户可编辑草稿创建时的精确持久修订。
  const draftBase = React.useRef<VceProjectSettingsSummary | null>(props.overview?.settings ?? null);
  // draftConflict warns about remote changes without discarding unsaved local edits.
  // draftConflict 提示远端变化，不丢弃未保存的本地编辑。
  const [draftConflict, setDraftConflict] = React.useState(false);
  // managementExpanded is a human request to load optional lists and aggregate counts.
  // managementExpanded 为人为请求加载可选列表和聚合计数。
  const [managementExpanded, setManagementExpanded] = React.useState(false);
  // managementOverview retains statistics only for their exact project/checkpoint/settings identity.
  // managementOverview 仅按其精确项目、检查点及设置身份保留统计。
  const [managementOverview, setManagementOverview] = React.useState<VceProjectOverviewResponse | null>(props.overview);
  // managementBusy covers bounded file, operation, and integrity reads.
  // managementBusy 表示有界文件、操作与完整性读取正在进行。
  const [managementBusy, setManagementBusy] = React.useState(false);
  // filePage stores one bounded file page and its opaque navigation history.
  // filePage 保存一页有界文件记录及其不透明导航历史。
  const [filePage, setFilePage] = React.useState<VceManagementPageState<VceFileSummary>>(
    emptyVceManagementPage,
  );
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
  const [maintenancePlan, setMaintenancePlan] = React.useState<VceMaintenancePlanResponse | null>(null);
  // managementError stores a bounded management-load diagnostic.
  // managementError 保存一条有界管理加载诊断。
  const [managementError, setManagementError] = React.useState<string | null>(null);
  // managementRequestVersion invalidates stale asynchronous responses after project or page changes.
  // managementRequestVersion 在项目或分页变化后使陈旧异步响应失效。
  const managementRequestVersion = React.useRef(0);
  // managementOverviewReadAbortController owns the optional management overview transport.
  // managementOverviewReadAbortController 持有可选管理概览读取的传输中止控制器。
  const managementOverviewReadAbortController = React.useRef<AbortController | null>(null);
  // managementOverviewReadDeadlineTimer bounds the optional overview request by its generation.
  // managementOverviewReadDeadlineTimer 按请求代次限制可选概览请求的最长等待时间。
  const managementOverviewReadDeadlineTimer = React.useRef<ReturnType<typeof setTimeout> | null>(null);
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
    if (draftBase.current !== null && draftBase.current.revision >= settings.revision) return;
    // A new checkpoint/progress snapshot is not a settings edit. A remote settings edit cannot
    // silently rebase a dirty draft onto a newer revision and overwrite someone else's choices.
    // 新检查点或进度快照不是设置编辑。远端设置编辑不能静默把脏草稿
    // 改基到新修订并覆盖他人的选择。
    if (draft !== null && draftBase.current !== null
      && JSON.stringify(draft) !== JSON.stringify(editableSettings(draftBase.current))) {
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
    if (!managementExpanded || !props.overview?.configuration.ready || !props.overview.settings.enabled || props.overview.status.checkpointVersion == null) {
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
      const overview = await loadAuthoritativeVceOverview(props.projectId, loadVceProjectOverview, {
        signal: controller.signal,
      });
      clearTimeout(deadline);
      if (managementOverviewReadDeadlineTimer.current === deadline) {
        managementOverviewReadDeadlineTimer.current = null;
      }
      if (managementOverviewReadAbortController.current === controller) {
        managementOverviewReadAbortController.current = null;
      }
      if (controller.signal.aborted) {
        if (timedOut && managementRequestVersion.current === requestVersion) {
          setManagementError(VCE_PROJECT_OVERVIEW_READ_TIMEOUT_MESSAGE);
        }
        return;
      }
      if (managementRequestVersion.current !== requestVersion) return;
      if (!overview.readIdentity) throw new Error(copy("索引尚无可读取的已发布版本。", "No published index is available for management reads."));
      const [filePage, operationPage, issuePage] = await Promise.all([
        listVceFiles(props.projectId, overview.readIdentity, null, VCE_MANAGEMENT_PAGE_SIZE),
        listVceOperations(props.projectId, null, VCE_MANAGEMENT_PAGE_SIZE),
        listVceIntegrityIssues(props.projectId, overview.readIdentity, null, VCE_MANAGEMENT_PAGE_SIZE),
      ]);
      if (managementRequestVersion.current !== requestVersion) {
        return;
      }
      setManagementOverview(overview);
      const firstPageRequest = { cursor: null, previousCursors: [] };
      setFilePage(materializeVceManagementPage(filePage, firstPageRequest));
      setOperationPage(materializeVceManagementPage(operationPage, firstPageRequest));
      setIssuePage(materializeVceManagementPage(issuePage, firstPageRequest));
    } catch (error) {
      if (controller.signal.aborted) {
        if (timedOut && managementRequestVersion.current === requestVersion) {
          setManagementError(VCE_PROJECT_OVERVIEW_READ_TIMEOUT_MESSAGE);
        }
      } else if (managementRequestVersion.current === requestVersion) {
        setManagementError(error instanceof Error ? error.message : String(error));
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
  }, [copy, isCurrentAction, managementExpanded, props.overview?.configuration.ready, props.overview?.settings.enabled, props.overview?.status.checkpointVersion, props.overview?.settings.revision, props.projectId]);

  React.useEffect(() => {
    // A replacement can publish a new physical namespace at the same numeric checkpoint.
    // 替换可以在相同数字检查点发布新的物理命名空间。
    // Durable job identity/state changes retire lists and previews; progress-only sequence changes do not scan.
    // 持久作业身份或状态变化使列表与预览退役；仅进度序号变化不触发扫描。
    void reloadManagement();
    return () => {
      managementRequestVersion.current += 1;
    };
  }, [reloadManagement, props.overview?.status.checkpointVersion, indexing.job?.jobId, indexing.job?.state]);

  // loadManagementPage commits only the latest page response for the selected project.
  // loadManagementPage 只提交所选项目的最新分页响应。
  const loadManagementPage = React.useCallback(async <T,>(
    request: VceManagementPageRequest | null,
    loader: (cursor: string | null, readIdentity: string) => Promise<import("@/types").VcePageResponse<T>>,
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
      if (!managementOverview?.readIdentity) throw new Error(copy("请先刷新索引详情。", "Refresh index details first."));
      const response = await loader(request.cursor, managementOverview.readIdentity);
      if (managementRequestVersion.current === requestVersion) {
        applyPage(materializeVceManagementPage(response, request));
      }
    } catch (error) {
      if (managementRequestVersion.current === requestVersion) {
        setManagementError(error instanceof Error ? error.message : String(error));
      }
    } finally {
      if (managementRequestVersion.current === requestVersion) {
        setManagementBusy(false);
      }
    }
  }, [copy, managementOverview]);

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
      if (!managementOverview?.readIdentity) throw new Error(copy("请先刷新索引详情。", "Refresh index details first."));
      const nextBlob = await loadVceBlobDetail(projectId, blobName, managementOverview.readIdentity);
      if (isCurrentAction() && managementRequestVersion.current === requestVersion) {
        setBlob(nextBlob);
      }
    } catch (error) {
      if (isCurrentAction() && managementRequestVersion.current === requestVersion) {
        setManagementError(error instanceof Error ? error.message : String(error));
      }
    } finally {
      if (isCurrentAction() && managementRequestVersion.current === requestVersion) {
        setManagementBusy(false);
      }
    }
  };

  // loadMaintenancePlan obtains a read-only plan and performs no mutation.
  // loadMaintenancePlan 获取只读计划且不执行修改。
  const loadMaintenancePlan = async (mode: "repair" | "rebuild" | "cleanup_staging") => {
    const projectId = props.projectId;
    // A live durable job owns the project's write path until it reaches a terminal state.
    // 活跃持久作业在进入终态前持有项目写入路径。
    if (indexing.active) {
      setManagementError(copy("当前索引任务仍在运行，请先取消或等待它结束。", "The current index job is still active. Cancel it or wait for it to finish first."));
      return;
    }
    // Only rebuild can recover an uninitialized or failed first publication.
    // 只有重建可以恢复尚未初始化或首次发布失败的项目。
    if (mode !== "rebuild" && props.overview?.status.checkpointVersion == null) {
      setManagementError(copy("该项目尚无已发布检查点，只能生成重建计划。", "This project has no published checkpoint; only a rebuild plan is available."));
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
      if (isCurrentAction() && managementRequestVersion.current === requestVersion) {
        setMaintenancePlan(nextPlan);
      }
    } catch (error) {
      if (isCurrentAction() && managementRequestVersion.current === requestVersion) {
        setManagementError(error instanceof Error ? error.message : String(error));
      }
    } finally {
      if (isCurrentAction() && managementRequestVersion.current === requestVersion) {
        setManagementBusy(false);
      }
    }
  };

  // Save next against its frozen revision; refresh lightweight authority on failure without erasing edits.
  // 按冻结修订保存 next；失败时刷新轻量权威，不擦除编辑。
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
      const saved = await props.onSave({
        ...next,
        extraIgnorePatterns: next.extraIgnorePatterns.map((value) => value.trim()).filter(Boolean),
      }, draftBase.current.revision);
      if (isCurrentAction()) {
        draftBase.current = saved;
        setDraft(editableSettings(saved));
        setDraftConflict(false);
      }
    } catch (error) {
      if (isCurrentAction()) {
        setManagementError(error instanceof Error ? error.message : String(error));
        // Shared read errors are rendered independently through activityError, never swallowed here.
        // 共享读取错误通过 activityError 独立渲染，绝不在此吞掉。
        await props.onReloadActivity();
      }
    }
  };

  // refreshIndex reports a rejected refresh locally instead of creating an unhandled promise.
  // refreshIndex 在本地报告被拒绝的刷新，而不是产生未处理 Promise。
  const refreshIndex = async () => {
    if (!props.overview?.configuration.ready || !props.overview.settings.enabled) return;
    setManagementError(null);
    try {
      const receipt = await props.onRefresh();
      if (isCurrentAction()) indexing.receive(receipt);
    } catch (error) {
      if (isCurrentAction()) setManagementError(error instanceof Error ? error.message : String(error));
    }
  };

  // Refresh visible metadata only; this icon has no authority to scan or initialize an index.
  // 仅刷新可见元数据；此图标无权扫描或初始化索引。
  // Takes no arguments and replaces only the current project's authoritative overview.
  // 无参数，仅替换当前项目权威概览。
  const refreshStatus = async () => {
    try {
      await props.onReloadActivity();
    } catch (error) {
      if (isCurrentAction()) setManagementError(error instanceof Error ? error.message : String(error));
    }
  };

  // executeMaintenance executes only a dialog-frozen current read-only plan.
  // executeMaintenance 仅执行由对话框冻结且仍然有效的当前只读计划。
  const executeMaintenance = async (confirmation: VceMaintenanceConfirmation) => {
    const overview = props.overview;
    if (maintenanceMutationInFlightRef.current) {
      return;
    }
    if (!overview || !maintenancePlan || maintenancePlan.actions.length === 0) {
      setManagementError(copy("当前计划没有可执行操作。", "The current plan has no executable actions."));
      return;
    }
    if (indexing.active) {
      setManagementError(copy("当前索引任务仍在运行，请先取消或等待它结束。", "The current index job is still active. Cancel it or wait for it to finish first."));
      return;
    }
    const projectId = props.projectId;
    const checkpoint = overview.status.checkpointVersion ?? null;
    const actionIds = maintenancePlan.actions.map((action) => action.actionId);
    const planStillCurrent = confirmation.projectId === projectId
      && confirmation.settingsRevision === overview.settings.revision
      && confirmation.checkpointVersion === checkpoint
      && confirmation.mode === maintenancePlan.mode
      && confirmation.embeddingFingerprint === maintenancePlan.embeddingFingerprint
      && confirmation.actionIds.length === actionIds.length
      && confirmation.actionIds.every((actionId, index) => actionId === actionIds[index]);
    if (!planStillCurrent) {
      setMaintenancePlan(null);
      setManagementError(copy("维护计划已失效，请重新生成计划后再确认。", "The maintenance plan is stale. Generate a new plan before confirming it."));
      return;
    }
    if (checkpoint === null && maintenancePlan.mode !== "rebuild") {
      setManagementError(copy("维护操作要求一个已发布检查点。", "Maintenance requires a published checkpoint."));
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
        checkpoint,
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
        setManagementError(error instanceof Error ? error.message : String(error));
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
        setManagementError(error instanceof Error ? error.message : String(error));
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
      const receipt = await cancelVceOperation(
        projectId,
        operationId,
      );
      if (!isCurrentAction()) {
        return;
      }
      indexing.receive(receipt);
    } catch (error) {
      if (isCurrentAction()) {
        setManagementError(error instanceof Error ? error.message : String(error));
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
        {props.activityError && !replacementInProgressActivityError ? <>
          <p role="alert" className="text-sm text-destructive">{props.activityError}</p>
          <Button type="button" onClick={() => void props.onReloadActivity()}>{copy("重新读取状态", "Reload status")}</Button>
        </> : null}
        {indexing.job ? <VceJobProgress
          chinese={locale === "zh-CN"}
          job={indexing.job}
          onCancel={() => void indexing.cancel()}
          cancelPending={indexing.cancelPending}
        /> : indexingStatusVisible ? <p className="project-settings-muted" role="status" aria-live="polite">
          <LoaderCircle className="mr-2 inline animate-spin" size={14} />{t("projectSettings.codeContext.generating")}
        </p> : <>
          <LoaderCircle className="animate-spin" size={18} />
          <p className="project-settings-muted">{copy("正在加载代码语义索引…", "Loading code semantic index…")}</p>
        </>}
      </div>
    );
  }

  // status exposes lifecycle and checkpoint state without provider implementation details.
  // status 暴露生命周期与检查点状态且不包含 Provider 实现细节。
  const status = props.overview.status;
  // rebuildRecoveryRequired keeps an unsuccessful first publication on an explicit recovery path.
  // rebuildRecoveryRequired 将首次发布失败保持在明确的恢复路径上。
  const rebuildRecoveryRequired = status.lifecycle === "rebuild_required"
    || (status.lifecycle === "failed" && status.checkpointVersion == null)
    || (status.checkpointVersion == null && isUnpublishedVceRecoveryJob(indexing.job));
  // counts supplies aggregate storage statistics when the runtime is active.
  // counts 在运行时活动时提供存储聚合统计。
  const counts = managementOverview?.status.projectId === props.projectId
    && managementOverview.status.checkpointVersion === status.checkpointVersion
    && managementOverview.settings.revision === props.overview.settings.revision
    ? managementOverview.counts : null;
  return (
    <div className="project-settings-section">
      <header className="project-settings-heading project-settings-heading-with-action">
        <Database size={18} />
        <div>
          <strong>{copy("代码语义索引", "Code semantic index")}</strong>
          <span>{copy("为 AI 提供当前项目的自然语言代码检索。", "Provides natural-language code retrieval for AI in this project.")}</span>
        </div>
        <div className="project-settings-heading-actions">
          {indexingStatusVisible ? <div className="project-settings-heading-status" role="status" aria-live="polite">
            <LoaderCircle className="animate-spin" size={14} />{t("projectSettings.codeContext.generating")}
          </div> : null}
          <Button
            aria-label={copy("刷新状态", "Refresh status")}
            disabled={props.busy || !draft.enabled}
            onClick={() => void refreshStatus()}
            size="icon-sm"
            type="button"
            variant="outline"
          >
            <RefreshCw className={props.busy ? "animate-spin" : undefined} size={15} />
          </Button>
        </div>
      </header>

      {props.activityError && !replacementInProgressActivityError ? <p role="alert" className="text-sm text-destructive">{props.activityError}</p> : null}
      {draftConflict ? <div role="alert" className="rounded-lg border border-amber-500/30 p-3 text-sm">
        <p>{copy("项目设置已在其他位置更新。您的未保存修改已保留；请重新载入设置后再保存，避免覆盖新设置。", "Project settings changed elsewhere. Unsaved edits are preserved; reload before saving to avoid overwriting newer settings.")}</p>
        <Button type="button" variant="outline" size="sm" disabled={props.busy} onClick={() => {
          draftBase.current = props.overview!.settings;
          setDraft(editableSettings(props.overview!.settings));
          setDraftConflict(false);
        }}>{copy("放弃修改并载入当前设置", "Discard edits and load current settings")}</Button>
      </div> : null}

      {!props.overview.configuration.ready ? <div className="rounded-lg border border-amber-500/30 bg-amber-500/5 p-3 text-sm">
        <p>{copy("引擎模型尚未就绪。您可以保存项目设置，但创建、更新索引和 AI 检索保持关闭。", "Engine models are not ready. Project settings can be saved, but indexing and AI retrieval remain disabled.")}</p>
        <Button type="button" variant="link" size="sm" onClick={props.onOpenEngineSettings}>{copy("配置语义引擎模型", "Configure engine models")}</Button>
      </div> : null}

      <div className="project-settings-toggle-row">
        <Switch
          aria-label={copy("启用代码语义索引", "Enable code semantic index")}
          checked={draft.enabled}
          disabled={props.busy}
          onCheckedChange={(enabled) => {
            const next = { ...draft, enabled };
            setDraft(next);
            void saveDraft(next);
          }}
        />
        <span>
          <strong>{copy("启用代码语义索引", "Enable code semantic index")}</strong>
          <small>{copy("开启只授权此项目使用语义引擎；索引需要单独创建，默认手动更新。", "Enabling only authorizes this project to use the engine; create its index separately. Updates are manual by default.")}</small>
        </span>
      </div>

      {draft.enabled ? (
        <section className="space-y-3 rounded-lg border p-4">
          <div className="flex flex-wrap items-center justify-between gap-3">
            <div>
              <strong>{status.checkpointVersion == null ? copy("创建项目索引", "Create project index") : copy("更新项目索引", "Update project index")}</strong>
              <p className="mt-1 text-sm text-muted-foreground">{status.checkpointVersion == null ? copy("只有点击创建，才会扫描此项目并调用向量模型。", "Files are scanned and embedded only after you choose to create this project's index.") : copy("手动同步立即受理，不受自动更新间隔限制。", "Manual synchronization bypasses the automatic update interval.")}</p>
            </div>
            <Button type="button" disabled={props.busy || synchronization.disabled || indexingStatusVisible || !props.overview.configuration.ready || !props.overview.settings.enabled || rebuildRecoveryRequired} onClick={() => void refreshIndex()}>
              <Database size={15} />{status.checkpointVersion == null ? copy("创建语义索引", "Create semantic index") : synchronization.label}
            </Button>
          </div>
          {rebuildRecoveryRequired ? (
            <section className="space-y-3 rounded-lg border border-amber-500/30 bg-amber-500/5 p-3">
              <div className="flex flex-wrap items-start justify-between gap-3">
                <div className="min-w-0">
                  <strong>{copy("索引需要重建", "The index needs to be rebuilt")}</strong>
                  <p className="mt-1 text-sm text-muted-foreground">{status.checkpointVersion == null
                    ? indexing.active
                      ? copy("上次索引创建没有发布可用结果。请先取消遗留任务，再生成并确认一份重建计划。", "The previous index creation did not publish a usable result. Cancel the stale job, then generate and confirm a rebuild plan.")
                      : copy("上次索引创建没有发布可用结果。现在可以生成并确认重建计划，使用当前模型设置重新创建索引。", "The previous index creation did not publish a usable result. You can now generate and confirm a rebuild plan to recreate the index with the current model settings.")
                    : copy("当前索引需要按新的模型契约重建。请生成并确认一份重建计划；当前运行中的任务必须先取消或等待结束。", "The current index must be rebuilt for the new model contract. Generate and confirm a rebuild plan; an active job must be cancelled or allowed to finish first.")}</p>
                </div>
                <Button
                  disabled={props.busy || managementBusy || indexing.active || !props.overview.configuration.ready || !props.overview.settings.enabled}
                  onClick={() => void loadMaintenancePlan("rebuild")}
                  type="button"
                  variant="outline"
                >
                  <RefreshCw size={15} />{maintenancePlan?.mode === "rebuild" ? copy("重新生成重建计划", "Regenerate rebuild plan") : copy("生成重建计划", "Generate rebuild plan")}
                </Button>
              </div>
              {maintenancePlan?.mode === "rebuild" ? (
                <MaintenancePlanPreview
                  active={indexing.active}
                  busy={managementBusy}
                  copy={copy}
                  checkpointVersion={status.checkpointVersion ?? null}
                  error={managementError}
                  onExecute={(confirmation) => void executeMaintenance(confirmation)}
                  plan={maintenancePlan}
                  projectId={props.projectId}
                  settingsRevision={props.overview.settings.revision}
                />
              ) : null}
            </section>
          ) : null}
          {indexing.job ? <VceJobProgress job={indexing.job} chinese={locale === "zh-CN"} onCancel={() => void indexing.cancel()} cancelPending={indexing.cancelPending}
            onRetry={indexing.job.kind !== "rebuild" && !rebuildRecoveryRequired && !managementBusy && props.overview?.configuration.ready === true && draft.enabled
              ? () => void retryOperation(indexing.job!.jobId) : undefined} /> : null}
          {indexing.error ? <p role="alert" className="text-sm text-destructive">{copy("索引状态暂不可用：", "Index status unavailable: ")}{indexing.error}</p> : null}
          <fieldset className="space-y-2" disabled={props.busy}>
            <legend className="text-sm font-medium">{copy("索引更新模式", "Index update mode")}</legend>
            <div className="flex flex-wrap gap-4 text-sm">
              {(["manual", "automatic"] as const).map((mode) => <label key={mode} className="flex items-center gap-2"><input type="radio" name={`vce-update-mode-${props.projectId}`} checked={draft.updateMode === mode} onChange={() => { const next = { ...draft, updateMode: mode }; setDraft(next); void saveDraft(next); }} />{mode === "manual" ? copy("手动更新", "Manual updates") : copy("自动更新", "Automatic updates")}</label>)}
            </div>
            <p className="text-xs text-muted-foreground">{copy("此设置属于项目，所有会话共享。自动更新仅在首次初始化成功后跟随文件变化。", "This project setting is shared across conversations. Automatic file-change updates start only after initialization succeeds.")}</p>
          </fieldset>
          <label className="flex items-center gap-3 text-sm">
            {copy("自动更新最低间隔（秒）", "Minimum automatic interval (seconds)")}
            <Input className="w-28" type="number" min={60} max={86400} disabled={props.busy} value={draft.autoUpdateMinIntervalSeconds} onChange={(event) => setDraft({ ...draft, autoUpdateMinIntervalSeconds: Number(event.target.value) })} />
            <Button type="button" size="sm" variant="outline" disabled={props.busy || !Number.isInteger(draft.autoUpdateMinIntervalSeconds) || draft.autoUpdateMinIntervalSeconds < 60 || draft.autoUpdateMinIntervalSeconds > 86400} onClick={() => void saveDraft(draft)}>{copy("保存间隔", "Save interval")}</Button>
          </label>
        </section>
      ) : null}

      <div className="project-settings-grid">
        <div className="project-settings-panel"><strong>{copy("生命周期", "Lifecycle")}</strong><p>{status.lifecycle}</p></div>
        <div className="project-settings-panel"><strong>{copy("检查点", "Checkpoint")}</strong><p>{status.checkpointVersion ?? "—"}</p></div>
        <div className="project-settings-panel"><strong>{copy("文件", "Files")}</strong><p>{counts?.searchableFiles ?? status.presentFiles}</p></div>
        <div className="project-settings-panel"><strong>{copy("代码块", "Chunks")}</strong><p>{counts?.chunks ?? "—"}</p></div>
        <div className="project-settings-panel"><strong>{copy("向量", "Vectors")}</strong><p>{counts?.vectors ?? "—"}</p></div>
        <div className="project-settings-panel"><strong>{copy("精确符号", "Exact symbols")}</strong><p>{counts?.symbols ?? "—"}</p></div>
      </div>

      {status.lastError && !indexingStatusVisible ? <p className="rounded-lg border border-destructive/30 bg-destructive/5 p-3 text-sm text-destructive">{status.lastError}</p> : null}

      <section className="project-settings-panel space-y-3">
        <strong>{copy("高级设置", "Advanced settings")}</strong>
        <label className="grid gap-1 text-sm">
          {copy("附加忽略规则（每行一条）", "Additional ignore rules (one per line)")}
          <Textarea
            disabled={props.busy}
            onChange={(event) => setDraft({ ...draft, extraIgnorePatterns: event.target.value.split(/\r?\n/) })}
            value={draft.extraIgnorePatterns.join("\n")}
          />
        </label>
        <div className="grid grid-cols-2 gap-3">
          <NumericSetting disabled={props.busy} label={copy("最大文件字节数", "Maximum file bytes")} value={draft.maxFileBytes} onChange={(value) => setDraft({ ...draft, maxFileBytes: value })} />
          <NumericSetting disabled={props.busy} label={copy("最大结果数", "Maximum results")} value={draft.maxResults} onChange={(value) => setDraft({ ...draft, maxResults: value })} />
          <NumericSetting disabled={props.busy} label={copy("上下文字符预算", "Context character budget")} value={draft.maxContextChars} onChange={(value) => setDraft({ ...draft, maxContextChars: value })} />
          <NumericSetting disabled={props.busy} label={copy("向量并发", "Embedding concurrency")} value={draft.embeddingMaxConcurrency} onChange={(value) => setDraft({ ...draft, embeddingMaxConcurrency: value })} />
          <NumericSetting disabled={props.busy} label={copy("查询并发", "Query concurrency")} value={draft.queryMaxConcurrency} onChange={(value) => setDraft({ ...draft, queryMaxConcurrency: value })} />
        </div>
        <ToggleSetting checked={draft.allowQueryRewrite} disabled={props.busy} label={copy("允许 LLM 查询改写", "Allow LLM query rewriting")} onChange={(value) => setDraft({ ...draft, allowQueryRewrite: value })} />
        <ToggleSetting checked={draft.allowRerank} disabled={props.busy} label={copy("允许候选重排", "Allow candidate reranking")} onChange={(value) => setDraft({ ...draft, allowRerank: value })} />
        <Button disabled={props.busy} onClick={() => void saveDraft(draft)} type="button">
          <Save size={15} /> {copy("保存设置", "Save settings")}
        </Button>
      </section>

      {props.overview.configuration.ready && draft.enabled && status.checkpointVersion != null ? <Button
        type="button" variant="outline" aria-expanded={managementExpanded} onClick={() => setManagementExpanded(!managementExpanded)}>
        {managementExpanded ? copy("收起索引详情", "Hide index details") : copy("查看索引详情与统计", "View index details and statistics")}
      </Button> : null}
      {managementExpanded && props.overview.configuration.ready && draft.enabled && status.checkpointVersion != null ? (
        <>
          <section className="project-settings-panel space-y-2">
            <div className="flex items-center justify-between gap-2">
              <strong>{copy("文件与 Blob", "Files and Blobs")}</strong>
              <Button disabled={managementBusy} onClick={() => void reloadManagement()} size="sm" type="button" variant="outline"><RefreshCw size={14} />{copy("刷新", "Refresh")}</Button>
            </div>
            {filePage.items.length === 0 ? <p className="project-settings-muted">{copy("暂无已索引文件。", "No indexed files.")}</p> : filePage.items.map((file) => (
              <button className="flex w-full items-center justify-between gap-3 rounded-md border p-2 text-left text-sm" key={`${file.path}:${file.generation}`} onClick={() => void inspectBlob(file.committedBlobName ?? file.blobName)} type="button">
                <span className="min-w-0"><FileCode2 className="mr-2 inline" size={14} /><span className="break-all">{file.path}</span></span>
                <span className="shrink-0 text-xs text-muted-foreground">{file.chunkCount} {copy("块", "chunks")}</span>
              </button>
            ))}
            <ManagementPagination
              busy={managementBusy}
              copy={copy}
              onNext={() => void loadManagementPage(
                nextVceManagementPageRequest(filePage),
                (cursor, readIdentity) => listVceFiles(props.projectId, readIdentity, cursor, VCE_MANAGEMENT_PAGE_SIZE),
                setFilePage,
              )}
              onPrevious={() => void loadManagementPage(
                previousVceManagementPageRequest(filePage),
                (cursor, readIdentity) => listVceFiles(props.projectId, readIdentity, cursor, VCE_MANAGEMENT_PAGE_SIZE),
                setFilePage,
              )}
              page={filePage}
            />
          </section>

          {blob ? (
            <section className="project-settings-panel space-y-2">
              <strong>{copy("Blob 详情", "Blob detail")}: {blob.path}</strong>
              <p className="break-all text-xs text-muted-foreground">{blob.blobName}</p>
              {blob.chunks.map((chunk) => (
                <pre className="max-h-48 overflow-auto rounded-md border bg-muted/20 p-2 text-xs" key={chunk.chunkId}>{`${chunk.startLine}-${chunk.endLine} · ${chunk.kind}\n${chunk.preview ?? ""}`}</pre>
              ))}
            </section>
          ) : null}

          <section className="project-settings-panel space-y-2">
            <strong>{copy("作业历史", "Operation history")}</strong>
            {operationPage.items.length === 0 ? <p className="project-settings-muted">{copy("暂无作业记录。", "No operation history.")}</p> : operationPage.items.map((operation) => (
              <div className="flex items-center justify-between gap-2 text-sm" key={operation.operationId}>
                <p>{operation.kind} · {operation.phase} · {operation.completedUnits}/{operation.totalUnits}{operation.error ? ` · ${operation.error}` : ""}</p>
                {operation.phase === "failed" || operation.phase === "cancelled" ? (
                  <Button disabled={managementBusy} onClick={() => void retryOperation(operation.operationId)} size="sm" type="button" variant="outline">{copy("重试", "Retry")}</Button>
                ) : operation.phase !== "completed" ? (
                  <Button disabled={managementBusy} onClick={() => void cancelOperation(operation.operationId)} size="sm" type="button" variant="outline">{copy("取消", "Cancel")}</Button>
                ) : null}
              </div>
            ))}
            <ManagementPagination
              busy={managementBusy}
              copy={copy}
              onNext={() => void loadManagementPage(
                nextVceManagementPageRequest(operationPage),
                (cursor) => listVceOperations(props.projectId, cursor, VCE_MANAGEMENT_PAGE_SIZE),
                setOperationPage,
              )}
              onPrevious={() => void loadManagementPage(
                previousVceManagementPageRequest(operationPage),
                (cursor) => listVceOperations(props.projectId, cursor, VCE_MANAGEMENT_PAGE_SIZE),
                setOperationPage,
              )}
              page={operationPage}
            />
          </section>

          <section className="project-settings-panel space-y-2">
            <strong>{copy("完整性与维护", "Integrity and maintenance")}</strong>
            {issuePage.items.length === 0 ? <p className="project-settings-muted"><SearchCheck className="mr-2 inline" size={14} />{copy("未发现完整性问题。", "No integrity issues found.")}</p> : issuePage.items.map((issue) => <p className="text-sm text-destructive" key={issue.issueId}>{issue.kind}: {issue.message}</p>)}
            <div className="flex flex-wrap gap-2">
              <Button disabled={managementBusy || indexing.active} onClick={() => void loadMaintenancePlan("repair")} size="sm" type="button" variant="outline">{copy("生成修复计划", "Plan repair")}</Button>
              <Button disabled={managementBusy || indexing.active} onClick={() => void loadMaintenancePlan("rebuild")} size="sm" type="button" variant="outline">{copy("生成重建计划", "Plan rebuild")}</Button>
              <Button disabled={managementBusy || indexing.active} onClick={() => void loadMaintenancePlan("cleanup_staging")} size="sm" type="button" variant="outline">{copy("生成暂存清理计划", "Plan staging cleanup")}</Button>
            </div>
            {maintenancePlan && (!rebuildRecoveryRequired || maintenancePlan.mode !== "rebuild") ? (
              <MaintenancePlanPreview
                active={indexing.active}
                busy={managementBusy}
                copy={copy}
                checkpointVersion={status.checkpointVersion ?? null}
                error={managementError}
                onExecute={(confirmation) => void executeMaintenance(confirmation)}
                plan={maintenancePlan}
                projectId={props.projectId}
                settingsRevision={props.overview.settings.revision}
              />
            ) : null}
            <ManagementPagination
              busy={managementBusy}
              copy={copy}
              onNext={() => void loadManagementPage(
                nextVceManagementPageRequest(issuePage),
                (cursor, readIdentity) => listVceIntegrityIssues(props.projectId, readIdentity, cursor, VCE_MANAGEMENT_PAGE_SIZE),
                setIssuePage,
              )}
              onPrevious={() => void loadManagementPage(
                previousVceManagementPageRequest(issuePage),
                (cursor, readIdentity) => listVceIntegrityIssues(props.projectId, readIdentity, cursor, VCE_MANAGEMENT_PAGE_SIZE),
                setIssuePage,
              )}
              page={issuePage}
            />
          </section>
        </>
      ) : null}

      {managementBusy ? <p className="project-settings-muted"><LoaderCircle className="mr-2 inline animate-spin" size={14} />{copy("正在加载管理数据…", "Loading management data…")}</p> : null}
      {managementError ? <p className="rounded-lg border border-destructive/30 bg-destructive/5 p-3 text-sm text-destructive">{managementError}</p> : null}
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
  // Exact checkpoint visible when this plan was presented.
  // 此计划展示时可见的精确检查点。
  checkpointVersion: number | null;
  // Current maintenance error remains visible inside the open confirmation dialog.
  // 当前维护错误保留在打开的确认弹窗内可见。
  error: string | null;
  // Localized copy selector owned by the settings surface.
  // 设置界面持有的本地化文案选择器。
  copy: (zh: string, en: string) => string;
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
  // An uninitialized rebuild scans the project even when no existing Blob is affected.
  // 未初始化重建即使没有现有 Blob 受影响，也会扫描整个项目。
  const unpublishedRebuild = props.checkpointVersion === null && props.plan.mode === "rebuild";
  // confirmation retains the exact plan authority while the modal is open.
  // confirmation 在弹窗打开期间保留计划的精确权威。
  const [confirmation, setConfirmation] = React.useState<VceMaintenanceConfirmation | null>(null);
  const actionIds = React.useMemo(() => props.plan.actions.map((action) => action.actionId), [props.plan.actions]);
  const confirmationIsCurrent = confirmation !== null
    && confirmation.projectId === props.projectId
    && confirmation.settingsRevision === props.settingsRevision
    && confirmation.checkpointVersion === props.checkpointVersion
    && confirmation.mode === props.plan.mode
    && confirmation.embeddingFingerprint === props.plan.embeddingFingerprint
    && confirmation.actionIds.length === actionIds.length
    && confirmation.actionIds.every((actionId, index) => actionId === actionIds[index]);

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
      checkpointVersion: props.checkpointVersion,
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
    if (confirmation === null || !confirmationIsCurrent || props.busy || props.active) return;
    props.onExecute(confirmation);
  };
  // scopeLabel describes the real work for both existing-index and first-build recovery plans.
  // scopeLabel 为已有索引及首次恢复计划描述真实工作范围。
  const scopeLabel = unpublishedRebuild
    ? props.copy("重新扫描当前项目并创建索引（当前没有已发布的索引）", "Rescan the current project and create an index (no published index exists)")
    : `${props.plan.affectedBlobCount} ${props.copy("个 Blob", "Blobs")}`;
  return (
    <div className="space-y-2 rounded-md border bg-background/60 p-3">
      <p className="text-sm">{props.copy("计划动作", "Planned actions")}: {props.plan.actions.length} · {props.copy("范围", "Scope")}: {scopeLabel}</p>
      {props.plan.actions.map((action) => <p className="text-sm" key={action.actionId}>{action.description}</p>)}
      <Button disabled={props.busy || props.active || props.plan.actions.length === 0} onClick={openConfirmation} size="sm" type="button">{props.copy("确认并执行", "Confirm and execute")}</Button>
      <AlertDialog open={confirmation !== null && confirmationIsCurrent} onOpenChange={(open) => { if (!open && !props.busy) setConfirmation(null); }}>
        <AlertDialogContent className="max-h-[85vh] overflow-y-auto" size="sm">
          <AlertDialogHeader>
            <AlertDialogTitle>{unpublishedRebuild ? props.copy("确认重新创建语义索引", "Confirm semantic index creation") : props.copy("确认执行索引维护", "Confirm index maintenance")}</AlertDialogTitle>
            <AlertDialogDescription>{unpublishedRebuild
              ? props.copy("本次操作会重新扫描当前项目并创建索引。当前没有已发布的索引，计划将按生成计划时的模型配置执行。", "This will rescan the current project and create an index. No published index exists; the plan will use the model configuration from plan generation time.")
              : props.copy("请核对计划动作与影响范围。计划将按生成计划时的模型配置执行。", "Review the planned actions and scope. The plan will use the model configuration from plan generation time.")}</AlertDialogDescription>
          </AlertDialogHeader>
          <dl className="grid gap-3 rounded-md border p-3 text-sm sm:grid-cols-2">
            <div><dt className="text-muted-foreground">{props.copy("计划动作", "Planned actions")}</dt><dd>{props.plan.actions.length}</dd></div>
            <div><dt className="text-muted-foreground">{props.copy("目标向量维度", "Target vector dimension")}</dt><dd>{props.plan.embeddingDimension}</dd></div>
            <div className="sm:col-span-2"><dt className="text-muted-foreground">{props.copy("影响范围", "Scope")}</dt><dd>{scopeLabel}</dd></div>
          </dl>
          <div className="space-y-1">
            {props.plan.actions.map((action) => <p className="text-sm" key={action.actionId}>{action.description}</p>)}
          </div>
          {props.error ? <p className="break-words text-sm text-destructive" role="alert">{props.error}</p> : null}
          <AlertDialogFooter>
            <AlertDialogCancel disabled={props.busy}>{props.copy("取消", "Cancel")}</AlertDialogCancel>
            <AlertDialogAction disabled={props.busy || props.active || !confirmationIsCurrent} onClick={submitConfirmation}>{props.copy("确认并执行", "Confirm and execute")}</AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </div>
  );
}

// isUnpublishedVceRecoveryJob recognizes only terminal initialize/rebuild receipts with no checkpoint.
// isUnpublishedVceRecoveryJob 只识别无检查点的初始化／重建终态回执。
function isUnpublishedVceRecoveryJob(job: VceJobSummary | null): boolean {
  if (job === null || (job.kind !== "initialize" && job.kind !== "rebuild")) return false;
  return job.state === "failed" || job.state === "cancelled" || job.state === "interrupted";
}

// ManagementPagination renders cursor-safe previous and next controls for one bounded list.
// ManagementPagination 为一项有界列表渲染游标安全的上一页与下一页控件。
function ManagementPagination<T>(props: {
  // Whether another management request is active.
  // 是否有其他管理请求正在执行。
  busy: boolean;
  // Localized copy selector owned by the parent surface.
  // 父界面持有的本地化文案选择器。
  copy: (zh: string, en: string) => string;
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
  const pageNumber = props.page.previousCursors.length + 1;
  return (
    <div className="flex items-center justify-end gap-2 text-xs text-muted-foreground">
      <span>
        {props.copy("第", "Page")} {pageNumber}
        {props.page.total === null ? "" : ` · ${props.copy("共", "Total")} ${props.page.total}`}
      </span>
      <Button
        disabled={props.busy || props.page.previousCursors.length === 0}
        onClick={props.onPrevious}
        size="sm"
        type="button"
        variant="outline"
      >
        {props.copy("上一页", "Previous")}
      </Button>
      <Button
        disabled={props.busy || props.page.nextCursor === null}
        onClick={props.onNext}
        size="sm"
        type="button"
        variant="outline"
      >
        {props.copy("下一页", "Next")}
      </Button>
    </div>
  );
}

// editableSettings strips host-owned fields from one settings summary.
// editableSettings 从一项设置摘要中剥离宿主持有字段。
function editableSettings(settings: VceProjectOverviewResponse["settings"]): VceProjectSettingsInput {
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
  return <label className="grid gap-1 text-sm">{props.label}<Input disabled={props.disabled} min={1} onChange={(event) => props.onChange(Number.parseInt(event.target.value, 10) || 0)} type="number" value={props.value} /></label>;
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
  return <label className="flex items-center gap-2 text-sm"><Switch checked={props.checked} disabled={props.disabled} onCheckedChange={props.onChange} /><span>{props.label}</span></label>;
}
