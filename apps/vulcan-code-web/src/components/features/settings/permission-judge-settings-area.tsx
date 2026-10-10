import * as React from "react";
import { ExternalLink, FolderCheck, Gauge, RefreshCw, RotateCcw } from "lucide-react";
import { toast } from "sonner";
import { StatusPill } from "@/components/shared/status";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { Input } from "@/components/ui/input";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { StreamPlaceholder } from "@/components/ui/stream-placeholder";
import { Switch } from "@/components/ui/switch";
import { formatMessage, useI18n, type Translate, type TranslationKey } from "@/i18n";
import { errorMessage, formatBytes } from "@/lib/display-format";
import { PermissionJudgeCloudSection } from "@/components/features/settings/permission-judge-cloud-section";
import { DEVICE_KEYS, PHASE_KEYS, REASON_KEYS, label } from "@/components/features/settings/permission-judge-labels";
import { PermissionJudgeEntryContext } from "@/components/composer/permission-judge-entry-context";
import { PermissionReviewRouteSection } from "@/components/features/settings/permission-review-route-section";
import { SettingsErrorNotice } from "@/components/features/settings/settings-error-notice";
import {
  activatePermissionJudgeModel,
  benchmarkPermissionJudge,
  installPermissionJudgeCuda,
  loadPermissionJudgeSettings,
  loadPermissionJudgeStatus,
  registerPermissionJudgeModel,
  reloadPermissionJudge,
  removePermissionJudgeModel,
  rollbackPermissionJudgeModel,
  savePermissionJudgeSettings,
} from "@/service/permission-judge-service";
import { openExternalUrl } from "@/service/project-actions-service";
import type {
  JudgeBackend,
  JudgeDevice,
  PermissionJudgeBenchmarkResult,
  PermissionJudgeCudaComponent,
  PermissionJudgeSettingsSnapshot,
  PermissionJudgeStatus,
} from "@/types";

// STATUS_POLL_MS is how often the live status refreshes while this page is open.
// STATUS_POLL_MS 是本页打开期间实时状态的刷新间隔。
const STATUS_POLL_MS = 2000;

// Labels of stable codes sent by the service; unknown codes render as themselves.
// 服务发送的稳定代码对应的标签；未知代码按原样显示。
const BACKEND_KEYS: Record<JudgeBackend, TranslationKey> = {
  local: "settings.permissionJudge.backend.local",
  cloud: "settings.permissionJudge.backend.cloud",
};
const INSTALL_PHASE_KEYS: Record<string, TranslationKey> = {
  downloading: "settings.permissionJudge.cuda.phase.downloading",
  verifying: "settings.permissionJudge.cuda.phase.verifying",
  extracting: "settings.permissionJudge.cuda.phase.extracting",
  completed: "settings.permissionJudge.cuda.phase.completed",
};
const SOURCE_KEYS: Record<string, TranslationKey> = {
  managed: "settings.permissionJudge.cuda.source.managed",
  toolkit: "settings.permissionJudge.cuda.source.toolkit",
  search_path: "settings.permissionJudge.cuda.source.searchPath",
};
const LINK_KEYS: Record<string, TranslationKey> = {
  cuda_toolkit: "settings.permissionJudge.cuda.link.toolkit",
  cuda_toolkit_archive: "settings.permissionJudge.cuda.link.toolkitArchive",
  nvidia_driver: "settings.permissionJudge.cuda.link.driver",
  cuda_redistributables: "settings.permissionJudge.cuda.link.redistributables",
  cuda_compatibility: "settings.permissionJudge.cuda.link.compatibility",
  onnxruntime_cuda_requirements: "settings.permissionJudge.cuda.link.onnxruntimeRequirements",
};
const COMPONENT_KEYS: Record<PermissionJudgeCudaComponent, TranslationKey> = {
  onnxruntime: "settings.permissionJudge.cuda.onnxruntime",
  cuda_libraries: "settings.permissionJudge.cuda.libraries",
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

// Draft holds the system-level choices until the user saves them.
// Draft 在用户保存前保存系统级选择。
interface Draft {
  enabled: boolean;
  backend: JudgeBackend;
  device: JudgeDevice | null;
  cudaLibraryDirectory: string;
  llmMayAllowBlocked: boolean;
}

// draftOf reads the saved choices; an unset backend is local and an unset device follows the
// recommendation.
// draftOf 读取已保存的选择；未设置的后端为本机，未设置的设备跟随推荐值。
function draftOf(snapshot: PermissionJudgeSettingsSnapshot): Draft {
  return {
    enabled: snapshot.state === "enabled",
    backend: snapshot.backend ?? "local",
    device: snapshot.device ?? snapshot.recommendedDevice,
    cudaLibraryDirectory: snapshot.cuda.lockedDirectory ?? "",
    llmMayAllowBlocked: snapshot.llmMayAllowBlocked,
  };
}

// Failure is one failed action: a readable outcome plus the raw service error.
// Failure 是一次失败的操作：可读结果加原始服务错误。
interface Failure {
  message: string;
  detail: string;
}

// PermissionJudgeSettingsArea manages the permission judge: switch and backend, then the cloud
// decision route or the local device, models, live status and CUDA runtime, plus speed tests.
// Nothing persists until the user saves or acts.
// PermissionJudgeSettingsArea 管理权限判定：开关与后端，其后是云端判定路由或本机设备、模型、实时状态与
// CUDA 执行库，以及速度测试。用户保存或操作前不持久化任何内容。
// Parameters: none. Returns the settings page.
// 参数：无。返回设置页。
export function PermissionJudgeSettingsArea() {
  const { t } = useI18n();
  // openGuide re-enters the setup guide; absent outside the workbench layout.
  // openGuide 重新进入初始化引导；在工作台布局之外为空。
  const openGuide = React.useContext(PermissionJudgeEntryContext)?.openGuide;
  // snapshot is the last full read; status is refreshed separately by polling.
  // snapshot 是最近一次完整读取；status 由轮询单独刷新。
  const [snapshot, setSnapshot] = React.useState<PermissionJudgeSettingsSnapshot | null>(null);
  const [status, setStatus] = React.useState<PermissionJudgeStatus | null>(null);
  const [draft, setDraft] = React.useState<Draft | null>(null);
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

  // applySnapshot replaces the page state with a fresh service read.
  // applySnapshot 用最新服务读取结果替换页面状态。
  const applySnapshot = React.useCallback((next: PermissionJudgeSettingsSnapshot) => {
    setSnapshot(next);
    setStatus(next.status);
    setDraft(draftOf(next));
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

  // Poll the live status; a finished CUDA installation re-reads detection.
  // 轮询实时状态；CUDA 安装结束后重新读取探测结果。
  const installRunning = status?.cudaInstall?.running ?? false;
  const wasInstalling = React.useRef(false);
  React.useEffect(() => {
    let cancelled = false;
    const timer = window.setInterval(() => {
      void loadPermissionJudgeStatus().then((next) => { if (!cancelled) setStatus(next); }).catch(() => undefined);
    }, STATUS_POLL_MS);
    return () => { cancelled = true; window.clearInterval(timer); };
  }, []);
  React.useEffect(() => {
    if (wasInstalling.current && !installRunning) void reload();
    wasInstalling.current = installRunning;
  }, [installRunning, reload]);

  // run executes one action, keeping the failure visible until the next action.
  // run 执行一个操作，失败信息保持显示直到下一次操作。
  const run = async (name: string, failureKey: TranslationKey, action: () => Promise<void>) => {
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

  if (!snapshot || !draft || !status) {
    return <section className="settings-preferences-page settings-detail-panel">
      {loadError
        ? <SettingsErrorNotice detail={loadError} message={t("settings.permissionJudge.loadFailed")} />
        : <StreamPlaceholder label={t("ui.loading")} shape="rows" count={4} />}
    </section>;
  }

  const saved = draftOf(snapshot);
  const dirty = JSON.stringify(saved) !== JSON.stringify(draft);
  const activeModel = snapshot.models.find((model) => model.id === snapshot.activeModelId) ?? null;
  const local = draft.backend === "local";
  const backendReady = local ? activeModel !== null : cloudConfigured;
  const savedCloud = snapshot.state === "enabled" && snapshot.backend === "cloud";
  const benchmarkReady = savedCloud
    ? !dirty
    : snapshot.backend !== "cloud" && status.worker.phase === "ready";
  const save = () => run("save", "settings.permissionJudge.saveFailed", async () => {
    const directory = draft.cudaLibraryDirectory.trim();
    applySnapshot(await savePermissionJudgeSettings({
      state: draft.enabled ? "enabled" : "off",
      backend: draft.backend,
      device: draft.device,
      cudaLibraryDirectory: directory === "" ? null : directory,
      llmMayAllowBlocked: draft.llmMayAllowBlocked,
    }));
    toast.success(t("settings.permissionJudge.saved"));
  });

  return <section className="settings-preferences-page settings-detail-panel">
    {openGuide ? <div className="settings-card-actions">
      <Button type="button" size="sm" variant="outline" onClick={openGuide}>{t("setupGuide.open")}</Button>
    </div> : null}
    <section className="settings-preferences-section">
      <label className="settings-preference-row">
        <div>
          <h2>{t("settings.permissionJudge.enabled")}</h2>
          <p className="settings-muted-line">{t("settings.permissionJudge.enabledHint")}</p>
        </div>
        <Switch checked={draft.enabled} disabled={busy !== null || (!draft.enabled && !backendReady)}
          onCheckedChange={(enabled) => setDraft({ ...draft, enabled })} />
      </label>
      {!backendReady ? <p className="settings-muted-line">
        {t(local ? "settings.permissionJudge.needsModel" : "settings.permissionJudge.needsCloudRoute")}
      </p> : null}
      <div className="settings-preference-row">
        <div>
          <span>{t("settings.permissionJudge.backend")}</span>
          <p className="settings-muted-line">{t("settings.permissionJudge.backendHint")}</p>
        </div>
        <Select disabled={busy !== null} value={draft.backend}
          onValueChange={(backend) => setDraft({ ...draft, backend: backend as JudgeBackend })}>
          <SelectTrigger aria-label={t("settings.permissionJudge.backend")} className="w-64"><SelectValue /></SelectTrigger>
          <SelectContent>
            {(Object.keys(BACKEND_KEYS) as JudgeBackend[]).map((backend) => (
              <SelectItem key={backend} value={backend}>{t(BACKEND_KEYS[backend])}</SelectItem>
            ))}
          </SelectContent>
        </Select>
      </div>
      {local ? <div className="settings-preference-row">
        <div>
          <span>{t("settings.permissionJudge.device")}</span>
          {snapshot.recommendedDevice ? <p className="settings-muted-line">
            {formatMessage(t("settings.permissionJudge.recommended"), { device: t(DEVICE_KEYS[snapshot.recommendedDevice]) })}
          </p> : null}
        </div>
        <Select disabled={busy !== null} value={draft.device ?? undefined}
          onValueChange={(device) => setDraft({ ...draft, device: device as JudgeDevice })}>
          <SelectTrigger aria-label={t("settings.permissionJudge.device")} className="w-64"><SelectValue /></SelectTrigger>
          <SelectContent>
            {snapshot.devices.map((device) => (
              <SelectItem disabled={device.unavailableReason !== null} key={device.device} value={device.device}>
                {t(DEVICE_KEYS[device.device])}
                {device.unavailableReason ? ` · ${label(t, REASON_KEYS, device.unavailableReason)}` : ""}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
      </div> : null}
      <label className="settings-preference-row">
        <div>
          <span>{t("settings.permissionJudge.llmMayAllowBlocked")}</span>
          <p className="settings-muted-line">{t("settings.permissionJudge.llmMayAllowBlockedHint")}</p>
        </div>
        <Switch checked={draft.llmMayAllowBlocked} disabled={busy !== null}
          onCheckedChange={(llmMayAllowBlocked) => setDraft({ ...draft, llmMayAllowBlocked })} />
      </label>
      <div className="settings-card-actions">
        <Button type="button" disabled={!dirty || busy !== null} onClick={() => void save()}>{t("settings.permissionJudge.save")}</Button>
        {dirty ? <Button type="button" variant="ghost" disabled={busy !== null} onClick={() => setDraft(saved)}>{t("settings.permissionJudge.discard")}</Button> : null}
      </div>
    </section>

    {failure ? <SettingsErrorNotice className="settings-save-error" detail={failure.detail} message={failure.message} /> : null}

    {local
      ? <WorkerStatusSection busy={busy} run={run} snapshot={snapshot} status={status} t={t}
        onReloaded={(next) => setStatus(next)} />
      : <PermissionJudgeCloudSection onConfiguredChange={setCloudConfigured} />}

    <PermissionReviewRouteSection />

    <section className="settings-preferences-section">
      <h2>{t("settings.permissionJudge.benchmark")}</h2>
      <p className="settings-muted-line">
        {t(savedCloud ? "settings.permissionJudge.benchmarkCloudHint" : "settings.permissionJudge.benchmarkHint")}
      </p>
      <div className="settings-card-actions">
        {([1, 10] as const).map((batchSize) => (
          <Button key={batchSize} type="button" variant="outline" size="sm"
            disabled={busy !== null || !benchmarkReady}
            onClick={() => void run(`benchmark-${batchSize}`, "settings.permissionJudge.benchmarkFailed", async () => {
              const result = await benchmarkPermissionJudge(batchSize);
              setBenchmarks((previous) => [result, ...previous].slice(0, 6));
            })}>
            <Gauge size={14} />{t(batchSize === 1 ? "settings.permissionJudge.benchmarkSingle" : "settings.permissionJudge.benchmarkBatch")}
          </Button>
        ))}
      </div>
      {benchmarks.map((result, index) => (
        <p className="settings-muted-line" key={index}>
          {result.device
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
            })}
        </p>
      ))}
    </section>

    {local ? <>
      <ModelsSection applySnapshot={applySnapshot} busy={busy} run={run} snapshot={snapshot} t={t} />

      <CudaSection busy={busy} draft={draft} run={run} setDraft={setDraft} snapshot={snapshot} status={status} t={t}
        onStatus={(next) => setStatus(next)} />
    </> : null}
  </section>;
}

// RunAction executes one named page action and reports its failure.
// RunAction 执行一个具名页面操作并报告其失败。
type RunAction = (name: string, failureKey: TranslationKey, action: () => Promise<void>) => Promise<void>;

// WorkerStatusSection shows the loaded model's phase, memory, GPU use and latency.
// WorkerStatusSection 展示已加载模型的阶段、内存、GPU 占用与延迟。
function WorkerStatusSection(props: {
  busy: string | null;
  run: RunAction;
  snapshot: PermissionJudgeSettingsSnapshot;
  status: PermissionJudgeStatus;
  t: Translate;
  onReloaded: (status: PermissionJudgeStatus) => void;
}) {
  const { t } = props;
  const worker = props.status.worker;
  const model = props.snapshot.models.find((candidate) => candidate.id === worker.modelId);
  const rows: [string, string][] = [
    [t("settings.permissionJudge.status.model"), model?.name ?? worker.modelId ?? "—"],
    [t("settings.permissionJudge.status.device"), worker.device ? t(DEVICE_KEYS[worker.device]) : "—"],
    [t("settings.permissionJudge.status.process"), worker.processId == null ? "—" : String(worker.processId)],
    [t("settings.permissionJudge.status.memory"), formatMessage(t("settings.permissionJudge.status.memoryValue"), {
      workingSet: formatOptionalBytes(worker.workingSetBytes),
      private: formatOptionalBytes(worker.privateBytes),
    })],
    [t("settings.permissionJudge.status.gpuMemory"), formatMessage(t("settings.permissionJudge.status.gpuMemoryValue"), {
      dedicated: formatOptionalBytes(worker.gpuDedicatedBytes),
      shared: formatOptionalBytes(worker.gpuSharedBytes),
    })],
    [t("settings.permissionJudge.status.gpuUtilization"), worker.gpuUtilizationPercent == null ? "—" : `${worker.gpuUtilizationPercent.toFixed(0)}%`],
    [t("settings.permissionJudge.status.startup"), formatMessage(t("settings.permissionJudge.status.startupValue"), {
      verify: formatMs(t, worker.verifyMs),
      load: formatMs(t, worker.loadMs),
      warmup: formatMs(t, worker.warmupMs),
    })],
    [t("settings.permissionJudge.status.latency"), formatMessage(t("settings.permissionJudge.status.latencyValue"), {
      p50: formatMs(t, worker.recentP50Ms),
      p95: formatMs(t, worker.recentP95Ms),
      count: worker.evaluations,
    })],
  ];
  const pill = worker.phase === "ready" ? "ready" : worker.phase === "failed" ? "error" : worker.phase === "warming" ? "warning" : "pending";
  return <section className="settings-preferences-section">
    <div className="settings-preference-row">
      <h2>{t("settings.permissionJudge.status")}</h2>
      <div className="settings-card-actions">
        <StatusPill label={label(t, PHASE_KEYS, worker.phase)} status={pill} />
        {worker.phase === "failed" ? <Button type="button" size="sm" variant="outline" disabled={props.busy !== null}
          onClick={() => void props.run("reload", "settings.permissionJudge.reloadFailed", async () => props.onReloaded(await reloadPermissionJudge()))}>
          <RefreshCw size={14} />{t("settings.permissionJudge.reload")}
        </Button> : null}
      </div>
    </div>
    {worker.phase === "failed" && worker.error ? <SettingsErrorNotice detail={worker.error} message={t("settings.permissionJudge.status.loadFailed")} /> : null}
    {props.snapshot.activeModelIssue ? <SettingsErrorNotice detail={props.snapshot.activeModelIssue.detail}
      message={formatMessage(t("settings.permissionJudge.activeModelIssue"), { code: props.snapshot.activeModelIssue.code })} /> : null}
    <dl className="grid gap-x-6 gap-y-1 text-sm sm:grid-cols-[max-content_1fr]">
      {rows.map(([name, value]) => <React.Fragment key={name}>
        <dt className="text-muted-foreground">{name}</dt>
        <dd className="tabular-nums">{value}</dd>
      </React.Fragment>)}
    </dl>
  </section>;
}

// ModelsSection lists registered models and registers new ones from local folders.
// ModelsSection 列出已登记模型，并从本机文件夹登记新模型。
function ModelsSection(props: {
  applySnapshot: (snapshot: PermissionJudgeSettingsSnapshot) => void;
  busy: string | null;
  run: RunAction;
  snapshot: PermissionJudgeSettingsSnapshot;
  t: Translate;
}) {
  const { t, snapshot } = props;
  const [path, setPath] = React.useState("");
  const [name, setName] = React.useState("");
  const [custom, setCustom] = React.useState(false);
  const packageName = (id: string | null) => snapshot.knownPackages.find((known) => known.id === id)?.displayName ?? id ?? "";
  return <section className="settings-preferences-section">
    <h2>{t("settings.permissionJudge.models")}</h2>
    <p className="settings-muted-line">{t("settings.permissionJudge.modelsHint")}</p>
    {snapshot.models.length === 0 ? <p className="settings-muted-line">{t("settings.permissionJudge.modelsEmpty")}</p> : null}
    {snapshot.models.map((model) => {
      const active = model.id === snapshot.activeModelId;
      return <div className="settings-preference-row" key={model.id}>
        <div className="min-w-0">
          <div className="flex flex-wrap items-center gap-2">
            <span className="font-medium">{model.name}</span>
            {active ? <Badge variant="secondary">{t("settings.permissionJudge.modelActive")}</Badge> : null}
            <Badge variant="outline">{model.knownPackageId
              ? formatMessage(t("settings.permissionJudge.modelOfficial"), { name: packageName(model.knownPackageId) })
              : t("settings.permissionJudge.modelCustom")}</Badge>
          </div>
          <p className="settings-muted-line">{formatMessage(t("settings.permissionJudge.modelMeta"), {
            policy: model.policyVersion, precision: model.precision, size: formatBytes(model.totalBytes),
          })}</p>
          <p className="settings-muted-line break-all">{model.path}</p>
        </div>
        <div className="settings-card-actions">
          {!active ? <Button type="button" size="sm" variant="outline" disabled={props.busy !== null}
            onClick={() => void props.run(`activate-${model.id}`, "settings.permissionJudge.activateFailed", async () => props.applySnapshot(await activatePermissionJudgeModel(model.id)))}>
            {t("settings.permissionJudge.activate")}
          </Button> : null}
          {!active ? <Button type="button" size="sm" variant="ghost" disabled={props.busy !== null}
            onClick={() => void props.run(`remove-${model.id}`, "settings.permissionJudge.removeFailed", async () => props.applySnapshot(await removePermissionJudgeModel(model.id)))}>
            {t("settings.permissionJudge.remove")}
          </Button> : null}
        </div>
      </div>;
    })}
    {snapshot.previousModelId && snapshot.models.some((model) => model.id === snapshot.previousModelId) ? <div className="settings-card-actions">
      <Button type="button" size="sm" variant="outline" disabled={props.busy !== null}
        onClick={() => void props.run("rollback", "settings.permissionJudge.activateFailed", async () => props.applySnapshot(await rollbackPermissionJudgeModel()))}>
        <RotateCcw size={14} />{formatMessage(t("settings.permissionJudge.rollback"), { name: snapshot.models.find((model) => model.id === snapshot.previousModelId)?.name ?? "" })}
      </Button>
    </div> : null}
    <h3>{t("settings.permissionJudge.register")}</h3>
    <div className="grid gap-2">
      <Input aria-label={t("settings.permissionJudge.registerPath")} placeholder={t("settings.permissionJudge.registerPath")}
        value={path} onChange={(event) => setPath(event.target.value)} />
      <Input aria-label={t("settings.permissionJudge.registerName")} placeholder={t("settings.permissionJudge.registerName")}
        value={name} onChange={(event) => setName(event.target.value)} />
      <label className="flex items-center gap-2 text-sm">
        <Checkbox checked={custom} onCheckedChange={(checked) => setCustom(checked === true)} />
        {t("settings.permissionJudge.registerCustom")}
      </label>
      <div className="settings-card-actions">
        <Button type="button" size="sm" disabled={props.busy !== null || path.trim() === ""}
          onClick={() => void props.run("register", "settings.permissionJudge.registerFailed", async () => {
            props.applySnapshot(await registerPermissionJudgeModel(path.trim(), name.trim(), custom));
            setPath("");
            setName("");
            setCustom(false);
          })}>
          {t("settings.permissionJudge.registerSubmit")}
        </Button>
      </div>
    </div>
  </section>;
}

// CudaSection shows the driver, the detected CUDA folders, one-click installs and official links.
// CudaSection 展示驱动、探测到的 CUDA 文件夹、一键安装与官方链接。
function CudaSection(props: {
  busy: string | null;
  draft: Draft;
  run: RunAction;
  setDraft: (draft: Draft) => void;
  snapshot: PermissionJudgeSettingsSnapshot;
  status: PermissionJudgeStatus;
  t: Translate;
  onStatus: (status: PermissionJudgeStatus) => void;
}) {
  const { t, draft } = props;
  const cuda = props.snapshot.cuda;
  const install = props.status.cudaInstall;
  const open = (url: string) => void openExternalUrl(url).catch((cause) => {
    toast.error(t("settings.permissionJudge.cuda.openFailed"), { description: errorMessage(cause) });
  });
  if (!cuda.supported) {
    return <section className="settings-preferences-section">
      <h2>{t("settings.permissionJudge.cuda")}</h2>
      <p className="settings-muted-line">{t("settings.permissionJudge.cuda.unsupported")}</p>
    </section>;
  }
  const driver = cuda.driver
    ? formatMessage(t("settings.permissionJudge.cuda.driverValue"), { version: cuda.driver.version, gpus: cuda.driver.gpus.join(", ") })
    : t("settings.permissionJudge.cuda.driverMissing");
  return <section className="settings-preferences-section">
    <h2>{t("settings.permissionJudge.cuda")}</h2>
    <p className="settings-muted-line">{t("settings.permissionJudge.cudaHint")}</p>
    <dl className="grid gap-x-6 gap-y-1 text-sm sm:grid-cols-[max-content_1fr]">
      <dt className="text-muted-foreground">{t("settings.permissionJudge.cuda.driver")}</dt>
      <dd>
        {driver}
        {cuda.minimumDriverMajor != null && cuda.cudaMajor != null ? <span className="settings-muted-line"> · {formatMessage(t("settings.permissionJudge.cuda.driverMinimum"), {
          major: cuda.minimumDriverMajor, cuda: cuda.cudaMajor,
        })}</span> : null}
      </dd>
      <dt className="text-muted-foreground">{t("settings.permissionJudge.cuda.onnxruntime")}</dt>
      <dd className="break-all">{cuda.onnxruntimeDirectory ?? t("settings.permissionJudge.cuda.notInstalled")}</dd>
      <dt className="text-muted-foreground">{t("settings.permissionJudge.cuda.libraries")}</dt>
      <dd className="break-all">{formatMessage(t("settings.permissionJudge.cuda.required"), { libraries: cuda.requiredLibraries.join(", ") })}</dd>
    </dl>
    {cuda.detectedDirectories.map((detected) => (
      <div className="settings-preference-row" key={detected.directory}>
        <div className="min-w-0">
          <span>{label(t, SOURCE_KEYS, detected.source)}</span>
          <p className="settings-muted-line break-all">{detected.directory}</p>
        </div>
        {draft.cudaLibraryDirectory === detected.directory
          ? <Badge variant="secondary"><FolderCheck size={12} />{t("settings.permissionJudge.cuda.locked")}</Badge>
          : <Button type="button" size="sm" variant="outline" disabled={props.busy !== null}
            onClick={() => props.setDraft({ ...draft, cudaLibraryDirectory: detected.directory })}>
            {t("settings.permissionJudge.cuda.useDirectory")}
          </Button>}
      </div>
    ))}
    <Input aria-label={t("settings.permissionJudge.cuda.directory")} placeholder={t("settings.permissionJudge.cuda.directory")}
      value={draft.cudaLibraryDirectory} onChange={(event) => props.setDraft({ ...draft, cudaLibraryDirectory: event.target.value })} />
    {cuda.lockedDirectory && cuda.lockedDirectoryMissing.length > 0 ? <p className="settings-muted-line">
      {formatMessage(t("settings.permissionJudge.cuda.lockedMissing"), { libraries: cuda.lockedDirectoryMissing.join(", ") })}
    </p> : null}
    <h3>{t("settings.permissionJudge.cuda.install")}</h3>
    {cuda.archives.map((archive) => (
      <div className="settings-preference-row" key={archive.url}>
        <div className="min-w-0">
          <span>{t(COMPONENT_KEYS[archive.component])}</span>
          <p className="settings-muted-line break-all">{archive.name} · {formatBytes(archive.size)}</p>
        </div>
        <Button type="button" size="sm" variant="outline" disabled={props.busy !== null || install?.running === true}
          onClick={() => void props.run(`install-${archive.component}`, "settings.permissionJudge.cuda.installFailed", async () => props.onStatus(await installPermissionJudgeCuda(archive.component)))}>
          {t("settings.permissionJudge.cuda.installAction")}
        </Button>
      </div>
    ))}
    {install ? <p className="settings-muted-line">
      {install.running
        ? formatMessage(t("settings.permissionJudge.cuda.installing"), {
          component: t(COMPONENT_KEYS[install.component]),
          phase: install.phase ? label(t, INSTALL_PHASE_KEYS, install.phase) : "…",
          progress: install.completedBytes != null && install.totalBytes
            ? `${formatBytes(install.completedBytes)} / ${formatBytes(install.totalBytes)}`
            : "",
        })
        : install.error
          ? null
          : formatMessage(t("settings.permissionJudge.cuda.installed"), { component: t(COMPONENT_KEYS[install.component]), directory: install.directory ?? "" })}
    </p> : null}
    {install && !install.running && install.error ? <SettingsErrorNotice detail={install.error} message={t("settings.permissionJudge.cuda.installFailed")} /> : null}
    <h3>{t("settings.permissionJudge.cuda.links")}</h3>
    <div className="settings-card-actions flex-wrap">
      {cuda.links.map((link) => (
        <Button key={link.id} type="button" size="sm" variant="ghost" onClick={() => open(link.url)}>
          <ExternalLink size={14} />{label(t, LINK_KEYS, link.id)}
        </Button>
      ))}
    </div>
  </section>;
}
