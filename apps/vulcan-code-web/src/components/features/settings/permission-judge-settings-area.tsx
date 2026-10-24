import * as React from "react";
import { Activity, Bot, Box, ChevronLeft, History, ChevronRight, Gauge, ListChecks, RefreshCw, RotateCcw } from "lucide-react";
import { toast } from "@/ui-kit/primitives/toast";
import {
  PageBody, PageEmptyState, PageFacts, PageHint, PageItem, PageNotice, PageRow, PageSection, PageToolbar, type PageFact,
} from "@/components/page";
import { StatusPill } from "@/components/shared/status";
import { Badge } from "@/ui-kit/primitives/badge";
import { Button } from "@/ui-kit/primitives/button";
import { Input } from "@/ui-kit/primitives/input";
import { SegmentedControl, SegmentedControlItem } from "@/ui-kit/primitives/choice-controls";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/ui-kit/primitives/select";
import { StreamPlaceholder } from "@/ui-kit/primitives/stream-placeholder";
import { Switch } from "@/ui-kit/primitives/switch";
import { formatMessage, useI18n, type Translate, type TranslationKey } from "@/i18n";
import { errorMessage, formatBytes } from "@/lib/display-format";
import { PermissionJudgeCloudSection } from "@/components/features/settings/permission-judge-cloud-section";
import { PermissionJudgeDownloadsSection } from "@/components/features/settings/permission-judge-downloads-section";
import {
  PermissionJudgeEdgeSection, edgeModelEnabled, type EdgeModelList,
} from "@/components/features/settings/permission-judge-edge-section";
import { OpenVulcanAccountContext } from "@/components/features/account/openvulcan-account-context";
import {
  PermissionJudgeCudaPage, cudaMissingLabels, cudaReadiness,
} from "@/components/features/settings/permission-judge-cuda-page";
import { DEVICE_KEYS, MODE_KEYS, OPTION_HINT_KEYS, OPTION_KEYS, PHASE_KEYS, REASON_KEYS, label } from "@/components/features/settings/permission-judge-labels";
import { PermissionJudgePolicyPage } from "@/components/features/settings/permission-judge-policy-page";
import { PermissionJudgeRecordsPage } from "@/components/features/settings/permission-judge-records-page";
import { PermissionJudgeRegisterForm } from "@/components/features/settings/permission-judge-register-form";
import { PermissionJudgeReviewRulesPage } from "@/components/features/settings/permission-judge-review-rules-page";
import { PermissionJudgeUnattendedPage } from "@/components/features/settings/permission-judge-unattended-page";
import { PermissionJudgeEntryContext } from "@/components/composer/permission-judge-entry-context";
import { PermissionReviewRouteSection } from "@/components/features/settings/permission-review-route-section";
import {
  activatePermissionJudgeModel,
  benchmarkPermissionJudge,
  downloadPermissionJudgeModel,
  installPermissionJudgeCuda,
  loadPermissionJudgeEdgeModels,
  loadPermissionJudgeSettings,
  loadPermissionJudgeStatus,
  registerPermissionJudgeModel,
  reloadPermissionJudge,
  removePermissionJudgeModel,
  rollbackPermissionJudgeModel,
  savePermissionJudgeSettings,
} from "@/service/permission-judge-service";
import type {
  JudgeBackend,
  JudgeDevice,
  PermissionJudgeBenchmarkResult,
  PermissionJudgeModelFormat,
  PermissionJudgeSettingsSnapshot,
  PermissionJudgeStatus,
  ProjectSummary,
} from "@/types";
import "@/ui-kit/styles/features/settings-forms.css"

// Labels of stable codes sent by the service; unknown codes render as themselves.
// 服务发送的稳定代码对应的标签；未知代码按原样显示。
const BACKEND_KEYS: Record<JudgeBackend, TranslationKey> = {
  local: "settings.permissionJudge.backend.local",
  cloud: "settings.permissionJudge.backend.cloud",
  edge: "settings.permissionJudge.backend.edge",
};

// NEEDS_KEYS explain what a backend still lacks before it can be used; the direct backend first
// needs a sign-in, then an official model the server enables.
// NEEDS_KEYS 说明后端在可用之前仍缺少什么；官方直连后端先需要登录，再需要服务端启用的官方模型。
const NEEDS_KEYS: Record<JudgeBackend, TranslationKey> = {
  local: "settings.permissionJudge.needsModel",
  cloud: "settings.permissionJudge.needsCloudRoute",
  edge: "settings.permissionJudge.needsEdgeAccount",
};

// formatMs renders a duration, or a dash when the reading is absent.
// formatMs 渲染时长；读数缺失时显示破折号。
function formatMs(t: Translate, value: number | null | undefined): string {
  return value == null ? "—" : formatMessage(t("settings.permissionJudge.ms"), { value: value.toFixed(1) });
}

// formatOptionalBytes renders a size, or a dash when the reading is absent.
// formatOptionalBytes 渲染大小；读数缺失时显示破折号。
function formatOptionalBytes(value: number | null | undefined): string {
  return value == null ? "—" : formatBytes(value);
}

// Choices are the system-level judge settings as shown; the backend is the one on screen, which may
// not be saved yet.
// Choices 是界面所示的系统级判定设置；后端为界面上的选择，可能尚未保存。
interface Choices {
  backend: JudgeBackend;
  edgeModelId: string | null;
  device: JudgeDevice | null;
  cudaLibraryDirectory: string | null;
  llmMayAllowBlocked: boolean;
  recordDecisions: boolean;
  approvalTimeoutSeconds: number;
}

// savedChoices reads the saved choices; an unset backend is local, an unset official direct model is
// the first one the server enables and an unset device follows the recommendation.
// savedChoices 读取已保存的选择；未设置的后端为本机，未设置的官方直连模型为服务端启用的第一个，未设置的
// 设备跟随推荐值。
function savedChoices(snapshot: PermissionJudgeSettingsSnapshot, edgeModels: EdgeModelList): Choices {
  return {
    backend: snapshot.backend ?? "local",
    edgeModelId: snapshot.edgeModelId ?? (edgeModels.phase === "ready" ? edgeModels.models[0] ?? null : null),
    device: snapshot.device ?? snapshot.recommendedDevice,
    cudaLibraryDirectory: snapshot.cuda.lockedDirectory,
    llmMayAllowBlocked: snapshot.llmMayAllowBlocked,
    recordDecisions: snapshot.recordDecisions,
    approvalTimeoutSeconds: snapshot.approvalTimeoutSeconds,
  };
}

// View is the page shown inside the settings frame: the overview or one of its sub-pages.
// View 是设置框架内显示的页面：总览或其子页面之一。
type View = "overview" | "models" | "cuda" | "diagnostics" | "records" | "policy" | "unattended" | "reviewRules";

// Failure is one failed action: a readable outcome plus the raw service error.
// Failure 是一次失败的操作：可读结果加原始服务错误。
interface Failure {
  message: string;
  detail: string;
}

// RunAction executes one named page action and reports its failure.
// RunAction 执行一个具名页面操作并报告其失败。
type RunAction = (name: string, failureKey: TranslationKey, action: () => Promise<void>) => Promise<void>;

// PermissionJudgeSettingsArea manages the permission judge that auto-approve mode uses. The overview
// reads top-down: choose where calls are judged and set that judge up, then the LLM review and
// tools. Detailed work opens as sub-pages: model management, the CUDA environment and diagnostics.
// Every choice saves at once, except a backend that is not set up yet: it waits until it is ready,
// so the judge only becomes configured through an explicit, working choice.
// PermissionJudgeSettingsArea 管理自动审批模式所用的权限判定。总览自上而下：选择在哪里判定并完成该判定器
// 的设置，然后是 LLM 复核与工具。细节操作以子页面打开：模型管理、CUDA 环境与诊断。每个选择立即保存，只有
// 尚未设置完成的后端会等到就绪后才保存，因此判定器只会经由明确且可用的选择成为已配置。
// Parameters: none. Returns the settings page.
// 参数：无。返回设置页。
export function PermissionJudgeSettingsArea(props: { projects?: readonly ProjectSummary[] }) {
  const { locale, t } = useI18n();
  // openGuide re-enters the setup guide; absent outside the workbench layout.
  // openGuide 重新进入初始化引导；在工作台布局之外为空。
  const openGuide = React.useContext(PermissionJudgeEntryContext)?.openGuide;
  // signedIn tells whether the OpenVulcan account the official direct judge uses is signed in.
  // signedIn 表示官方直连判定所用的 OpenVulcan 账户是否已登录。
  const signedIn = React.useContext(OpenVulcanAccountContext)?.snapshot?.connected === true;
  // snapshot is the last full read; status is refreshed separately by polling.
  // snapshot 是最近一次完整读取；status 由轮询单独刷新。
  const [snapshot, setSnapshot] = React.useState<PermissionJudgeSettingsSnapshot | null>(null);
  const [status, setStatus] = React.useState<PermissionJudgeStatus | null>(null);
  // held keeps the backend choice that cannot be saved yet because that backend is not set up.
  // held 保存因后端尚未设置完成而暂时无法保存的后端选择。
  const [held, setHeld] = React.useState<Partial<Choices>>({});
  const [view, setView] = React.useState<View>("overview");
  // busy names the running action so only one action runs at a time.
  // busy 记录正在执行的操作，保证同一时间只执行一个操作。
  const [busy, setBusy] = React.useState<string | null>(null);
  const [failure, setFailure] = React.useState<Failure | null>(null);
  const [loadError, setLoadError] = React.useState<string | null>(null);
  // benchmarks keeps this page's speed test results, newest first; they are never saved.
  // benchmarks 保存本页速度测试结果（最新在前），不会持久化。
  const [benchmarks, setBenchmarks] = React.useState<PermissionJudgeBenchmarkResult[]>([]);
  // cloudConfigured tells whether a cloud decision route is saved, as reported by its section.
  // cloudConfigured 表示是否已保存云端判定路由，由其区块报告。
  const [cloudConfigured, setCloudConfigured] = React.useState(false);
  // topRef brings a newly opened page or a new failure into view.
  // topRef 将新打开的页面或新失败滚动到可见处。
  const topRef = React.useRef<HTMLDivElement>(null);
  React.useEffect(() => {
    topRef.current?.scrollIntoView?.({ block: "nearest" });
  }, [view, failure]);

  // applySnapshot replaces the page state with a fresh service read.
  // applySnapshot 用最新服务读取结果替换页面状态。
  const applySnapshot = React.useCallback((next: PermissionJudgeSettingsSnapshot) => {
    setSnapshot(next);
    setStatus(next.status);
  }, []);

  const reload = React.useCallback(async () => {
    try {
      applySnapshot(await loadPermissionJudgeSettings());
      setLoadError(null);
    } catch (cause) {
      setLoadError(errorMessage(cause));
    }
  }, [applySnapshot]);

  React.useEffect(() => { void reload(); }, [reload]);

  // edgeModels is the official direct model list, read while the direct backend is on screen and
  // the account is signed in; edgeRead drops answers of superseded reads.
  // edgeModels 是官方直连模型清单，在界面显示官方直连后端且账户已登录时读取；edgeRead 丢弃已被取代的读取结果。
  const [edgeModels, setEdgeModels] = React.useState<EdgeModelList>({ phase: "signedOut" });
  const edgeRead = React.useRef(0);
  const edgeShown = (held.backend ?? snapshot?.backend ?? "local") === "edge";
  const loadEdgeModels = React.useCallback(async () => {
    const read = ++edgeRead.current;
    setEdgeModels({ phase: "loading" });
    try {
      const list = await loadPermissionJudgeEdgeModels();
      if (read === edgeRead.current) setEdgeModels({ phase: "ready", models: list.models.map((model) => model.modelId) });
    } catch (cause) {
      if (read === edgeRead.current) setEdgeModels({ phase: "failed", error: errorMessage(cause) });
    }
  }, []);
  React.useEffect(() => {
    if (signedIn && edgeShown) {
      void loadEdgeModels();
    } else {
      edgeRead.current += 1;
      setEdgeModels({ phase: "signedOut" });
    }
  }, [signedIn, edgeShown, loadEdgeModels]);

  // Poll the live status at the interval it publishes; a finished CUDA installation re-reads detection.
  // 按实时状态发布的间隔轮询；CUDA 安装结束后重新读取探测结果。
  const installRunning = status?.cudaInstall?.running ?? false;
  const pollIntervalMs = status?.pollIntervalMs;
  const wasInstalling = React.useRef(false);
  React.useEffect(() => {
    if (pollIntervalMs === undefined) return;
    let cancelled = false;
    const timer = window.setInterval(() => {
      void loadPermissionJudgeStatus().then((next) => { if (!cancelled) setStatus(next); }).catch(() => undefined);
    }, pollIntervalMs);
    return () => { cancelled = true; window.clearInterval(timer); };
  }, [pollIntervalMs]);
  React.useEffect(() => {
    if (wasInstalling.current && !installRunning) void reload();
    wasInstalling.current = installRunning;
  }, [installRunning, reload]);
  // A finished model download re-reads the registry it added to.
  // 模型下载结束后重新读取其加入的登记表。
  const downloadRunning = status?.modelDownload?.running ?? false;
  const wasDownloading = React.useRef(false);
  React.useEffect(() => {
    if (wasDownloading.current && !downloadRunning) void reload();
    wasDownloading.current = downloadRunning;
  }, [downloadRunning, reload]);

  // run executes one action, keeping the failure visible until the next action.
  // run 执行一个操作，失败信息保持显示直到下一次操作。
  const run: RunAction = async (name, failureKey, action) => {
    if (busy) return;
    setBusy(name);
    setFailure(null);
    try {
      await action();
    } catch (cause) {
      setFailure({ message: t(failureKey), detail: errorMessage(cause) });
    } finally {
      setBusy(null);
    }
  };

  const failureNotice = failure ? <PageNotice tone="error" details={failure.detail} detailsLabel={t("settings.errors.showErrorDetails")}>
    {failure.message}
  </PageNotice> : null;

  if (!snapshot || !status) {
    return <PageBody>
      {loadError
        ? <PageNotice tone="error" details={loadError} detailsLabel={t("settings.errors.showErrorDetails")}
          actions={<Button type="button" size="sm" variant="outline" onClick={() => void reload()}>
            <RefreshCw size={14} />{t("settings.permissionJudge.reload")}
          </Button>}>
          {t("settings.permissionJudge.loadFailed")}
        </PageNotice>
        : <StreamPlaceholder label={t("ui.loading")} shape="rows" count={4} />}
    </PageBody>;
  }

  const saved = savedChoices(snapshot, edgeModels);
  const choices: Choices = { ...saved, ...held };
  const activeModel = snapshot.models.find((model) => model.id === snapshot.activeModelId) ?? null;
  const local = choices.backend === "local";
  // The direct backend is ready with a signed-in account and a model the server enables; the saved
  // direct model also stays ready, since the service keeps an unchanged choice.
  // 官方直连后端在账户已登录且模型由服务端启用时就绪；已保存的直连模型同样保持就绪，因为服务会保留未变的选择。
  const ready = (backend: JudgeBackend, edgeModelId = choices.edgeModelId) => backend === "local"
    ? activeModel !== null
    : backend === "cloud" ? cloudConfigured : signedIn && (edgeModelEnabled(edgeModels, edgeModelId)
      || (snapshot.backend === "edge" && edgeModelId !== null && edgeModelId === snapshot.edgeModelId));
  const configured = snapshot.backend !== null;

  // persist saves `change` at once. A backend is saved only when it is chosen explicitly (`change`
  // names it, or `commit` confirms the held one) and is set up; otherwise it stays held and the saved
  // backend (possibly none) is kept, so other choices never configure the judge as a side effect.
  // persist 立即保存 `change`。后端只有在被明确选择（`change` 指定了后端，或 `commit` 确认暂存的后端）
  // 且已设置完成时才会保存；否则保持暂存并沿用已保存的后端（可能为无），其他选择不会顺带配置判定器。
  const persist = (change: Partial<Choices>, commit = false) => {
    const next = { ...choices, ...change };
    const chosen = commit || change.backend !== undefined;
    const backend = chosen && ready(next.backend, next.edgeModelId) ? next.backend : snapshot.backend;
    // The official direct model is saved with its backend; until then it stays held like the backend.
    // 官方直连模型与其后端一同保存；在此之前与后端一样保持暂存。
    const edgeModelId = backend === "edge" ? next.edgeModelId : snapshot.edgeModelId;
    const remaining: Partial<Choices> = {
      ...(backend === next.backend ? {} : { backend: next.backend }),
      ...(edgeModelId === next.edgeModelId ? {} : { edgeModelId: next.edgeModelId }),
    };
    const request = { ...next, backend, edgeModelId };
    if (JSON.stringify(request) === JSON.stringify({ ...saved, backend: snapshot.backend, edgeModelId: snapshot.edgeModelId })) {
      setHeld(remaining);
      return;
    }
    void run("save", "settings.permissionJudge.saveFailed", async () => {
      applySnapshot(await savePermissionJudgeSettings(request));
      setHeld(remaining);
      toast.success(t("settings.permissionJudge.saved"));
    });
  };

  const back = () => setView("overview");
  const subPage = (title: string, content: React.ReactNode) => <PageBody data-permission-judge-page="" data-judge-view={view}>
    <div ref={topRef}>
      <PageToolbar>
        <Button type="button" size="sm" variant="ghost" data-judge-back="" onClick={back}>
          <ChevronLeft size={14} />{t("settings.permissionJudge")}
        </Button>
        <span aria-hidden="true" className="vk-faint">/</span>
        <h2 className="vk-form-title">{title}</h2>
      </PageToolbar>
    </div>
    {failureNotice}
    {content}
  </PageBody>;

  if (view === "models") {
    return subPage(t("settings.permissionJudge.models"),
      <ModelsPage applySnapshot={applySnapshot} busy={busy} run={run} snapshot={snapshot} status={status} t={t}
        onDownload={(packageId, format) => void run(`download-${packageId}-${format}`, "settings.permissionJudge.downloads.failed",
          async () => setStatus(await downloadPermissionJudgeModel(packageId, format)))} />);
  }
  if (view === "cuda") {
    return subPage(t("settings.permissionJudge.cuda"),
      <PermissionJudgeCudaPage busy={busy} cuda={snapshot.cuda} install={status.cudaInstall}
        selectedDirectory={choices.cudaLibraryDirectory} t={t}
        onInstall={(component) => void run(`install-${component}`, "settings.permissionJudge.cuda.installFailed",
          async () => setStatus(await installPermissionJudgeCuda(component)))}
        onSelectDirectory={(cudaLibraryDirectory) => persist({ cudaLibraryDirectory })} />);
  }
  if (view === "policy") {
    return subPage(t("settings.permissionJudge.policy.title"), <PermissionJudgePolicyPage projects={props.projects ?? []} />);
  }
  if (view === "reviewRules") {
    return subPage(t("settings.permissionJudge.reviewRules.title"), <PermissionJudgeReviewRulesPage projects={props.projects ?? []} />);
  }
  if (view === "unattended") {
    return subPage(t("settings.permissionJudge.unattended.title"), <PermissionJudgeUnattendedPage projects={props.projects ?? []} />);
  }
  if (view === "records") {
    return subPage(t("settings.permissionJudge.records.title"), <>
      <PageSection layout="rows" data-records-switch="" title={t("settings.permissionJudge.records.recording")}>
        <PageRow label={t("settings.permissionJudge.records.enabled")} description={t("settings.permissionJudge.records.enabledHint")}>
          <Switch aria-label={t("settings.permissionJudge.records.enabled")} checked={choices.recordDecisions}
            disabled={busy !== null} onCheckedChange={(recordDecisions) => persist({ recordDecisions })} />
        </PageRow>
      </PageSection>
      <PermissionJudgeRecordsPage />
    </>);
  }
  if (view === "diagnostics") {
    // A remote judge has no local worker; its speed test times real requests.
    // 远端判定没有本机 worker；其速度测试为真实请求计时。
    const savedRemote = snapshot.backend === "cloud" || snapshot.backend === "edge";
    const benchmarkReady = savedRemote || status.worker.phase === "ready";
    return subPage(t("settings.permissionJudge.diagnostics"), <>
      {savedRemote ? null : <WorkerStatusSection snapshot={snapshot} status={status} t={t} />}
      <PageSection layout="rows" title={t("settings.permissionJudge.benchmark")}
        description={t(snapshot.backend === "cloud" ? "settings.permissionJudge.benchmarkCloudHint"
          : snapshot.backend === "edge" ? "settings.permissionJudge.benchmarkEdgeHint" : "settings.permissionJudge.benchmarkHint")}
        actions={([1, 10] as const).map((batchSize) => (
          <Button key={batchSize} type="button" variant="outline" size="sm"
            disabled={busy !== null || !benchmarkReady}
            onClick={() => void run(`benchmark-${batchSize}`, "settings.permissionJudge.benchmarkFailed", async () => {
              const result = await benchmarkPermissionJudge(batchSize);
              setBenchmarks((previous) => [result, ...previous].slice(0, 6));
            })}>
            <Gauge size={14} />{t(batchSize === 1 ? "settings.permissionJudge.benchmarkSingle" : "settings.permissionJudge.benchmarkBatch")}
          </Button>
        ))}>
        {benchmarks.map((result, index) => (
          <PageItem key={index} icon={<Gauge />} title={result.device
            ? formatMessage(t("settings.permissionJudge.benchmarkResult"), {
              batch: result.batchSize,
              device: t(DEVICE_KEYS[result.device]),
              p50: formatMs(t, result.p50Ms),
              p95: formatMs(t, result.p95Ms),
              perItem: formatMs(t, result.perItemMs),
              rate: result.itemsPerSecond.toFixed(1),
            })
            : formatMessage(t("settings.permissionJudge.benchmarkCloudResult"), {
              batch: result.batchSize,
              model: result.modelId,
              total: formatMs(t, result.p50Ms),
              perItem: formatMs(t, result.perItemMs),
              rate: result.itemsPerSecond.toFixed(1),
            })} />
        ))}
      </PageSection>
    </>);
  }

  // The toolbar summarizes what is saved, not held choices.
  // 工具栏概括已保存的内容，而非暂存的选择。
  const stateLabel = snapshot.backend !== null
    ? `${t("settings.permissionJudge.state.configured")} · ${t(BACKEND_KEYS[snapshot.backend])}`
    : t("settings.permissionJudge.state.unset");
  const stateTone = configured ? "ready" : "warning";
  // pending is a backend on screen that is not the saved one: the first setup, or a switch.
  // pending 表示界面上的后端不是已保存的后端：首次设置或切换。
  const pending = choices.backend !== snapshot.backend;
  const pendingBackend = t(BACKEND_KEYS[choices.backend]);
  const pendingText = configured
    ? formatMessage(t("settings.permissionJudge.switchBackendPending"), { backend: pendingBackend })
    : ready(choices.backend)
      ? formatMessage(t("settings.permissionJudge.readyToUse"), { backend: pendingBackend })
      : t(choices.backend === "edge" && signedIn ? "settings.permissionJudge.needsEdgeModel" : NEEDS_KEYS[choices.backend]);
  const worker = status.worker;
  const phaseTone = worker.phase === "ready" ? "ready" : worker.phase === "failed" ? "error" : worker.phase === "warming" ? "warning" : "pending";
  const cudaMissing = snapshot.cuda.supported ? cudaMissingLabels(t, cudaReadiness(snapshot.cuda)) : [];
  const open = (next: View, text = t("settings.permissionJudge.open")) => <Button type="button" size="sm" variant="outline" data-judge-open={next} onClick={() => setView(next)}>
    {text}<ChevronRight size={14} />
  </Button>;

  return <PageBody data-permission-judge-page="" data-judge-view="overview">
    <div ref={topRef}>
      <PageToolbar actions={openGuide
        ? <Button type="button" size="sm" variant="outline" onClick={openGuide}>{t("setupGuide.open")}</Button>
        : null}>
        <StatusPill label={stateLabel} status={stateTone} />
      </PageToolbar>
    </div>
    {failureNotice}

    <PageSection layout="rows" title={t("settings.permissionJudge")} description={t("settings.permissionJudge.description")}>
      <PageRow label={t("settings.permissionJudge.backend")} description={t("settings.permissionJudge.backendHint")}>
        <SegmentedControl aria-label={t("settings.permissionJudge.backend")} disabled={busy !== null} value={choices.backend}
          onValueChange={(backend) => persist({ backend: backend as JudgeBackend })}>
          {(Object.keys(BACKEND_KEYS) as JudgeBackend[]).map((backend) => (
            <SegmentedControlItem key={backend} value={backend}>{t(BACKEND_KEYS[backend])}</SegmentedControlItem>
          ))}
        </SegmentedControl>
      </PageRow>
      {pending ? <PageNotice tone="info" data-judge-switching={configured ? "switch" : "setup"}
        actions={<Button type="button" size="sm" disabled={busy !== null || !ready(choices.backend)} onClick={() => persist({}, true)}>
          {formatMessage(t(configured ? "settings.permissionJudge.switchBackend" : "settings.permissionJudge.useBackend"),
            { backend: pendingBackend })}
        </Button>}>
        {pendingText}
      </PageNotice> : null}
    </PageSection>

    {local ? <PageSection layout="rows" data-judge-local="" title={t("settings.permissionJudge.backend.local")}>
      <PageRow label={t("settings.permissionJudge.activeModel")} description={activeModel
        ? `${activeModel.name} · ${formatMessage(t("settings.permissionJudge.modelMeta"), {
          policy: activeModel.policyVersion, precision: activeModel.precision, size: formatBytes(activeModel.totalBytes),
        })}`
        : t("settings.permissionJudge.noActiveModel")}>
        <Button type="button" size="sm" variant={activeModel ? "outline" : "default"} data-judge-open="models" onClick={() => setView("models")}>
          {t(activeModel ? "settings.permissionJudge.manageModels" : "settings.permissionJudge.addModel")}<ChevronRight size={14} />
        </Button>
      </PageRow>
      <PageRow label={t("settings.permissionJudge.device")} description={snapshot.recommendedDevice
        ? formatMessage(t("settings.permissionJudge.recommended"), { device: t(DEVICE_KEYS[snapshot.recommendedDevice]) })
        : undefined}>
        <Select disabled={busy !== null} value={choices.device ?? undefined}
          onValueChange={(device) => persist({ device: device as JudgeDevice })}>
          <SelectTrigger aria-label={t("settings.permissionJudge.device")} className="vk-form-w-lg"><SelectValue /></SelectTrigger>
          <SelectContent>
            {snapshot.devices.map((device) => (
              <SelectItem disabled={device.unavailableReason !== null} key={device.device} value={device.device}>
                {t(DEVICE_KEYS[device.device])}
                {device.unavailableReason ? ` · ${label(t, REASON_KEYS, device.unavailableReason)}` : ""}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
      </PageRow>
      {snapshot.cuda.supported ? <PageRow label={t("settings.permissionJudge.cuda")} description={cudaMissing.length === 0
        ? t("settings.permissionJudge.cuda.summaryReady")
        : formatMessage(t("settings.permissionJudge.cuda.summaryMissing"), { items: new Intl.ListFormat(locale, { type: "conjunction" }).format(cudaMissing) })}>
        <div className="vk-form-inline">
          <StatusPill status={cudaMissing.length === 0 ? "ready" : "warning"}
            label={t(cudaMissing.length === 0 ? "settings.permissionJudge.cuda.ready" : "settings.permissionJudge.cuda.missing")} />
          <Button type="button" size="sm" variant="outline" data-judge-open="cuda" onClick={() => setView("cuda")}>
            {t("settings.permissionJudge.cuda.manage")}<ChevronRight size={14} />
          </Button>
        </div>
      </PageRow> : null}
      {activeModel ? <PageRow label={t("settings.permissionJudge.runState")} description={worker.phase === "ready"
        ? formatMessage(t("settings.permissionJudge.status.latencyValue"), {
          p50: formatMs(t, worker.recentP50Ms), p95: formatMs(t, worker.recentP95Ms), count: worker.evaluations,
        })
        : undefined}>
        <div className="vk-form-inline">
          <StatusPill label={label(t, PHASE_KEYS, worker.phase)} status={phaseTone} />
          {worker.phase === "failed" ? <Button type="button" size="sm" variant="outline" disabled={busy !== null}
            onClick={() => void run("reload", "settings.permissionJudge.reloadFailed", async () => setStatus(await reloadPermissionJudge()))}>
            <RefreshCw size={14} />{t("settings.permissionJudge.reload")}
          </Button> : null}
        </div>
      </PageRow> : null}
      {worker.phase === "failed" && worker.error ? <PageNotice tone="error" details={worker.error} detailsLabel={t("settings.errors.showErrorDetails")}>
        {t("settings.permissionJudge.status.loadFailed")}
      </PageNotice> : null}
      {snapshot.activeModelIssue ? <PageNotice tone="error" details={snapshot.activeModelIssue.detail} detailsLabel={t("settings.errors.showErrorDetails")}>
        {formatMessage(t("settings.permissionJudge.activeModelIssue"), { code: snapshot.activeModelIssue.code })}
      </PageNotice> : null}
    </PageSection> : choices.backend === "cloud"
      ? <PermissionJudgeCloudSection onConfiguredChange={setCloudConfigured} />
      : <PermissionJudgeEdgeSection edgeModels={edgeModels} edgeModelId={choices.edgeModelId}
        disabled={busy !== null} t={t} onEdgeModelChange={(edgeModelId) => persist({ edgeModelId })}
        onRetry={() => void loadEdgeModels()} />}

    {configured && snapshot.policyOptions.length > 0 ? <PageSection layout="rows" data-judge-policy-summary=""
      title={t("settings.permissionJudge.policy.title")} description={t("settings.permissionJudge.policy.description")}
      actions={open("policy", t("settings.permissionJudge.policy.manage"))}>
      {snapshot.policyOptions.map((option) => (
        <PageRow key={option.key} label={label(t, OPTION_KEYS, option.key)}
          description={Object.hasOwn(OPTION_HINT_KEYS, option.key) ? t(OPTION_HINT_KEYS[option.key]) : undefined}>
          <span className="vk-form-text">
            {formatMessage(t("settings.permissionJudge.policy.default"), { mode: label(t, MODE_KEYS, option.defaultMode) })}
          </span>
        </PageRow>
      ))}
    </PageSection> : null}

    <PermissionReviewRouteSection>
      <PageRow label={t("settings.permissionJudge.llmMayAllowBlocked")} description={t("settings.permissionJudge.llmMayAllowBlockedHint")}>
        <Switch aria-label={t("settings.permissionJudge.llmMayAllowBlocked")} checked={choices.llmMayAllowBlocked}
          disabled={busy !== null} onCheckedChange={(llmMayAllowBlocked) => persist({ llmMayAllowBlocked })} />
      </PageRow>
      <PageItem icon={<ListChecks />} title={t("settings.permissionJudge.reviewRules.title")}
        description={t("settings.permissionJudge.reviewRules.entryHint")} actions={open("reviewRules")} />
    </PermissionReviewRouteSection>

    <PageSection layout="rows" data-judge-approval="" title={t("settings.permissionJudge.approval.title")}
      description={t("settings.permissionJudge.approval.description")}>
      <PageRow label={t("settings.permissionJudge.approval.timeout")} description={t("settings.permissionJudge.approval.timeoutHint")}>
        <ApprovalTimeoutInput value={choices.approvalTimeoutSeconds} max={snapshot.maxApprovalTimeoutSeconds}
          disabled={busy !== null} label={t("settings.permissionJudge.approval.timeout")}
          onCommit={(approvalTimeoutSeconds) => persist({ approvalTimeoutSeconds })} />
      </PageRow>
    </PageSection>

    <PageSection layout="rows" data-judge-tools="" title={t("settings.permissionJudge.tools")}>
      <PageItem icon={<Bot />} title={t("settings.permissionJudge.unattended.title")}
        description={t("settings.permissionJudge.unattended.entryHint")} actions={open("unattended")} />
      <PageItem icon={<Activity />} title={t("settings.permissionJudge.diagnostics")}
        description={t("settings.permissionJudge.diagnosticsHint")} actions={open("diagnostics")} />
      <PageItem icon={<History />} title={t("settings.permissionJudge.records.title")}
        meta={<Badge variant="outline" data-records-state={choices.recordDecisions ? "on" : "off"}>
          {t(choices.recordDecisions ? "settings.permissionJudge.records.on" : "settings.permissionJudge.records.off")}
        </Badge>}
        description={t("settings.permissionJudge.records.entryHint")} actions={open("records")} />
    </PageSection>
  </PageBody>;
}

// ApprovalTimeoutInput edits the approval wait in seconds and saves it when the field loses focus or
// Enter is pressed; text outside 0..max returns to the saved value.
// ApprovalTimeoutInput 以秒为单位编辑审批等待时长，在失去焦点或按下回车时保存；超出 0..max 的输入恢复为
// 已保存值。
function ApprovalTimeoutInput(props: {
  value: number;
  max: number;
  disabled: boolean;
  label: string;
  onCommit: (seconds: number) => void;
}) {
  const [draft, setDraft] = React.useState(String(props.value));
  React.useEffect(() => setDraft(String(props.value)), [props.value]);
  const commit = () => {
    const seconds = Number(draft);
    if (draft.trim() === "" || !Number.isInteger(seconds) || seconds < 0 || seconds > props.max) {
      setDraft(String(props.value));
      return;
    }
    if (seconds !== props.value) props.onCommit(seconds);
  };
  return <Input type="number" inputMode="numeric" min={0} max={props.max} step={1} className="vk-form-w-xs"
    aria-label={props.label} data-approval-timeout="" disabled={props.disabled} value={draft}
    onChange={(event) => setDraft(event.currentTarget.value)} onBlur={commit}
    onKeyDown={(event) => { if (event.key === "Enter") event.currentTarget.blur(); }} />;
}

// WorkerStatusSection shows the loaded model's phase, memory, GPU use and latency.
// WorkerStatusSection 展示已加载模型的阶段、内存、GPU 占用与延迟。
function WorkerStatusSection(props: {
  snapshot: PermissionJudgeSettingsSnapshot;
  status: PermissionJudgeStatus;
  t: Translate;
}) {
  const { t } = props;
  const worker = props.status.worker;
  const model = props.snapshot.models.find((candidate) => candidate.id === worker.modelId);
  const facts: PageFact[] = [
    { key: "model", label: t("settings.permissionJudge.status.model"), value: model?.name ?? worker.modelId ?? "—" },
    { key: "device", label: t("settings.permissionJudge.status.device"), value: worker.device ? t(DEVICE_KEYS[worker.device]) : "—" },
    { key: "process", label: t("settings.permissionJudge.status.process"), value: worker.processId == null ? "—" : String(worker.processId) },
    { key: "memory", label: t("settings.permissionJudge.status.memory"), value: formatMessage(t("settings.permissionJudge.status.memoryValue"), {
      workingSet: formatOptionalBytes(worker.workingSetBytes),
      private: formatOptionalBytes(worker.privateBytes),
    }) },
    { key: "gpuMemory", label: t("settings.permissionJudge.status.gpuMemory"), value: formatMessage(t("settings.permissionJudge.status.gpuMemoryValue"), {
      dedicated: formatOptionalBytes(worker.gpuDedicatedBytes),
      shared: formatOptionalBytes(worker.gpuSharedBytes),
    }) },
    { key: "gpuUtilization", label: t("settings.permissionJudge.status.gpuUtilization"), value: worker.gpuUtilizationPercent == null ? "—" : `${worker.gpuUtilizationPercent.toFixed(0)}%` },
    { key: "startup", label: t("settings.permissionJudge.status.startup"), value: formatMessage(t("settings.permissionJudge.status.startupValue"), {
      verify: formatMs(t, worker.verifyMs),
      load: formatMs(t, worker.loadMs),
      warmup: formatMs(t, worker.warmupMs),
    }) },
    { key: "latency", label: t("settings.permissionJudge.status.latency"), value: formatMessage(t("settings.permissionJudge.status.latencyValue"), {
      p50: formatMs(t, worker.recentP50Ms),
      p95: formatMs(t, worker.recentP95Ms),
      count: worker.evaluations,
    }) },
  ];
  const pill = worker.phase === "ready" ? "ready" : worker.phase === "failed" ? "error" : worker.phase === "warming" ? "warning" : "pending";
  return <PageSection title={t("settings.permissionJudge.status")}
    status={<StatusPill label={label(t, PHASE_KEYS, worker.phase)} status={pill} />}>
    <PageFacts items={facts} layout="grid" />
  </PageSection>;
}

// ModelsPage lists registered models with their switch, rollback and removal, then adds new ones
// from local folders.
// ModelsPage 列出已登记模型及其切换、回退与移除操作，然后从本机文件夹添加新模型。
function ModelsPage(props: {
  applySnapshot: (snapshot: PermissionJudgeSettingsSnapshot) => void;
  busy: string | null;
  run: RunAction;
  snapshot: PermissionJudgeSettingsSnapshot;
  status: PermissionJudgeStatus;
  t: Translate;
  onDownload: (packageId: string, format: PermissionJudgeModelFormat) => void;
}) {
  const { t, snapshot } = props;
  const packageName = (id: string | null) => snapshot.knownPackages.find((known) => known.id === id)?.displayName ?? id ?? "";
  const previous = snapshot.previousModelId
    ? snapshot.models.find((model) => model.id === snapshot.previousModelId) ?? null
    : null;
  return <>
    <PageSection layout="rows" title={t("settings.permissionJudge.models")} description={t("settings.permissionJudge.modelsHint")}
      actions={previous ? <Button type="button" size="sm" variant="outline" disabled={props.busy !== null}
        onClick={() => void props.run("rollback", "settings.permissionJudge.activateFailed", async () => props.applySnapshot(await rollbackPermissionJudgeModel()))}>
        <RotateCcw size={14} />{formatMessage(t("settings.permissionJudge.rollback"), { name: previous.name })}
      </Button> : undefined}>
      {snapshot.models.length === 0
        ? <PageEmptyState icon={<Box />} title={t("settings.permissionJudge.modelsEmpty")} />
        : snapshot.models.map((model) => {
          const active = model.id === snapshot.activeModelId;
          return <PageItem key={model.id} icon={<Box />} tone={active ? "success" : "neutral"} title={model.name}
            meta={<>
              {active ? <Badge variant="secondary">{t("settings.permissionJudge.modelActive")}</Badge> : null}
              <Badge variant="outline">{model.knownPackageId
                ? formatMessage(t("settings.permissionJudge.modelOfficial"), { name: packageName(model.knownPackageId) })
                : t("settings.permissionJudge.modelCustom")}</Badge>
            </>}
            description={formatMessage(t("settings.permissionJudge.modelMeta"), {
              policy: model.policyVersion, precision: model.precision, size: formatBytes(model.totalBytes),
            })}
            actions={active ? undefined : <>
              <Button type="button" size="sm" variant="outline" disabled={props.busy !== null}
                onClick={() => void props.run(`activate-${model.id}`, "settings.permissionJudge.activateFailed", async () => props.applySnapshot(await activatePermissionJudgeModel(model.id)))}>
                {t("settings.permissionJudge.activate")}
              </Button>
              <Button type="button" size="sm" variant="ghost" disabled={props.busy !== null}
                onClick={() => void props.run(`remove-${model.id}`, "settings.permissionJudge.removeFailed", async () => props.applySnapshot(await removePermissionJudgeModel(model.id)))}>
                {t("settings.permissionJudge.remove")}
              </Button>
            </>}>
            <PageHint className="vk-form-break">{model.path}</PageHint>
          </PageItem>;
        })}
    </PageSection>
    <PermissionJudgeDownloadsSection busy={props.busy} download={props.status.modelDownload}
      knownPackages={snapshot.knownPackages} t={t} onDownload={props.onDownload} />
    <PageSection title={t("settings.permissionJudge.register")} description={t("settings.permissionJudge.registerHint")}>
      <PermissionJudgeRegisterForm busy={props.busy} knownPackages={snapshot.knownPackages}
        onRegister={(request, reset) => props.run("register", "settings.permissionJudge.registerFailed", async () => {
          props.applySnapshot(await registerPermissionJudgeModel(request.path, request.name, request.custom));
          reset();
        })} />
    </PageSection>
  </>;
}
