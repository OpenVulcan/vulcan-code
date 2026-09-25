import { StartupLogoMark } from "@/components/shell/startup-logo-mark";
import { StartupDownloadProgress } from "@/components/shell/startup-download-progress";
import type { StartupSnapshot } from "@/types";
import { activeStartupStep, startupProgressPercent } from "@/components/shell/startup-progress";
import { resolveAppLocale, type LocaleCode } from "@/i18n-locales";

// BootText stores the minimal boot-screen labels used by the startup gate.
// BootText 保存启动门使用的最小启动屏文案。
interface BootText {
  // Workspace loading title.
  // 工作区加载标题。
  loadingWorkspace: string;
  // Service connection detail.
  // 服务连接详情。
  connectingService: string;
  // Project index loading detail.
  // 项目索引加载详情。
  loadingProjectIndex: string;
  // Startup step title prefix.
  // 启动步骤标题前缀。
  loadingStepPrefix: string;
  // Startup failure prefix.
  // 启动失败前缀。
  failedPrefix: string;
  // Label for optional raw failure details, keeping the recovery action visible.
  // 可选原始错误详情的标签，让恢复操作保持可见。
  errorDetails: string;
  // Managed-runtime download progress label.
  // 受管运行时下载进度标签。
  downloadProgress: string;
  // Real completed step count label, independent from measured percentage.
  // 真实完成步骤计数标签，独立于实测百分比。
  completedSteps: string;
  // Explicit recovery action shown after a startup failure.
  // 启动失败后显示的显式恢复操作。
  reloadApplication: string;
}

// BOOT_TEXT maps startup labels without importing the full application translation catalog.
// BOOT_TEXT 在不导入完整应用翻译表的情况下映射启动文案。
const BOOT_TEXT: Record<LocaleCode, BootText> = {
  "zh-CN": {
    loadingWorkspace: "正在加载 Workspace",
    connectingService: "正在连接 Vulcan Core Service",
    loadingProjectIndex: "正在加载项目与会话索引",
    loadingStepPrefix: "正在加载",
    failedPrefix: "启动失败：",
    errorDetails: "查看详情",
    downloadProgress: "下载进度",
    completedSteps: "已完成 {completed}/{total} 个步骤",
    reloadApplication: "重新加载应用",
  },
  "en-US": {
    loadingWorkspace: "Loading Workspace",
    connectingService: "Connecting to Vulcan Core Service",
    loadingProjectIndex: "Loading project and conversation indexes",
    loadingStepPrefix: "Loading",
    failedPrefix: "Startup failed: ",
    errorDetails: "View details",
    downloadProgress: "Download progress",
    completedSteps: "Completed {completed}/{total} steps",
    reloadApplication: "Reload application",
  },
  "zh-TW": {
    loadingWorkspace: "正在載入 Workspace",
    connectingService: "正在連線 Vulcan Core Service",
    loadingProjectIndex: "正在載入專案與對話索引",
    loadingStepPrefix: "正在載入",
    failedPrefix: "啟動失敗：",
    errorDetails: "檢視詳細資訊",
    downloadProgress: "下載進度",
    completedSteps: "已完成 {completed}/{total} 個步驟",
    reloadApplication: "重新載入應用程式",
  },
  "ja-JP": {
    loadingWorkspace: "Workspace を読み込んでいます",
    connectingService: "Vulcan Core Service に接続しています",
    loadingProjectIndex: "プロジェクトと会話のインデックスを読み込んでいます",
    loadingStepPrefix: "読み込み中",
    failedPrefix: "起動に失敗しました：",
    errorDetails: "詳細を表示",
    downloadProgress: "ダウンロードの進行状況",
    completedSteps: "{completed}/{total} ステップ完了",
    reloadApplication: "アプリを再読み込み",
  },
  "ko-KR": {
    loadingWorkspace: "Workspace를 불러오는 중",
    connectingService: "Vulcan Core Service에 연결하는 중",
    loadingProjectIndex: "프로젝트 및 대화 인덱스를 불러오는 중",
    loadingStepPrefix: "불러오는 중",
    failedPrefix: "시작 실패: ",
    errorDetails: "자세히 보기",
    downloadProgress: "다운로드 진행률",
    completedSteps: "{total}단계 중 {completed}단계 완료",
    reloadApplication: "앱 다시 불러오기",
  },
  "es-ES": {
    loadingWorkspace: "Cargando Workspace",
    connectingService: "Conectando con Vulcan Core Service",
    loadingProjectIndex: "Cargando los índices de proyectos y conversaciones",
    loadingStepPrefix: "Cargando",
    failedPrefix: "Error al iniciar: ",
    errorDetails: "Ver detalles",
    downloadProgress: "Progreso de la descarga",
    completedSteps: "{completed}/{total} pasos completados",
    reloadApplication: "Recargar la aplicación",
  },
  "fr-FR": {
    loadingWorkspace: "Chargement du Workspace",
    connectingService: "Connexion à Vulcan Core Service",
    loadingProjectIndex: "Chargement des index de projets et de conversations",
    loadingStepPrefix: "Chargement",
    failedPrefix: "Échec du démarrage : ",
    errorDetails: "Voir les détails",
    downloadProgress: "Progression du téléchargement",
    completedSteps: "{completed}/{total} étapes terminées",
    reloadApplication: "Recharger l’application",
  },
  "de-DE": {
    loadingWorkspace: "Workspace wird geladen",
    connectingService: "Verbindung mit Vulcan Core Service wird hergestellt",
    loadingProjectIndex: "Projekt- und Unterhaltungsindizes werden geladen",
    loadingStepPrefix: "Wird geladen",
    failedPrefix: "Start fehlgeschlagen: ",
    errorDetails: "Details anzeigen",
    downloadProgress: "Download-Fortschritt",
    completedSteps: "{completed}/{total} Schritte abgeschlossen",
    reloadApplication: "Anwendung neu laden",
  },
  "pt-BR": {
    loadingWorkspace: "Carregando o Workspace",
    connectingService: "Conectando ao Vulcan Core Service",
    loadingProjectIndex: "Carregando os índices de projetos e conversas",
    loadingStepPrefix: "Carregando",
    failedPrefix: "Falha ao iniciar: ",
    errorDetails: "Ver detalhes",
    downloadProgress: "Progresso do download",
    completedSteps: "{completed}/{total} etapas concluídas",
    reloadApplication: "Recarregar o aplicativo",
  },
  "ru-RU": {
    loadingWorkspace: "Загрузка Workspace",
    connectingService: "Подключение к Vulcan Core Service",
    loadingProjectIndex: "Загрузка индексов проектов и бесед",
    loadingStepPrefix: "Загрузка",
    failedPrefix: "Не удалось запустить: ",
    errorDetails: "Подробнее",
    downloadProgress: "Ход загрузки",
    completedSteps: "Выполнено шагов: {completed} из {total}",
    reloadApplication: "Перезагрузить приложение",
  },
};

// BootLoadingScreen renders the fast startup gate with real startup progress.
// BootLoadingScreen 渲染带真实启动进度的快速启动门。
export function BootLoadingScreen(props: {
  // Startup snapshot received before the workbench bundle is loaded.
  // 工作台包加载前收到的启动快照。
  startup?: StartupSnapshot | null;
}) {
  // locale selects the minimal boot language before full i18n is available.
  // locale 在完整 i18n 可用前选择最小启动语言。
  const locale = bootLocale(props.startup?.uiLocale);
  // text stores localized labels for the boot screen.
  // text 保存启动屏的本地化文案。
  const text = BOOT_TEXT[locale];
  // activeStep selects the current startup row for the progress headline.
  // activeStep 选择当前启动步骤作为进度标题。
  const activeStep = props.startup ? activeStartupStep(props.startup.steps) : null;
  // percent converts startup step states into a bounded visual progress value.
  // percent 将启动步骤状态转换为有界视觉进度值。
  const percent = startupProgressPercent(props.startup ?? null);
  // message shows the current startup phase without loading the full workbench.
  // message 展示当前启动阶段，而不加载完整工作台。
  const message = activeStep?.message ?? (props.startup ? text.loadingProjectIndex : text.connectingService);
  // heading shows either the active startup step or the workspace loading title.
  // heading 展示当前启动步骤或工作区加载标题。
  const heading = activeStep ? `${text.loadingStepPrefix} ${activeStep.label}` : text.loadingWorkspace;
  return (
    <main className="loading-screen">
      <section className="startup-progress-panel" aria-live="polite" aria-busy={!props.startup?.ready && activeStep?.status !== "error"}>
        <StartupLogoMark />
        <div className="startup-progress-content">
          <div className="conversation-loading-heading startup-progress-heading">
            <strong>{heading}</strong>
            {percent !== null ? <span>{Math.round(percent)}%</span> : null}
          </div>
          <div className="conversation-loading-bar startup-progress-bar" aria-label={activeStep?.label ?? text.loadingWorkspace} role="progressbar" aria-valuemin={0} aria-valuemax={100} aria-valuenow={percent !== null ? Math.round(percent) : undefined}>
            <div className={`conversation-loading-bar-value${percent === null ? " startup-download-progress-indeterminate" : ""}`} style={percent !== null ? { width: `${percent}%` } : undefined} />
          </div>
          {props.startup && props.startup.steps.length > 0 ? <small>{text.completedSteps
            .replace("{completed}", String(props.startup.steps.filter((step) => step.status === "ready" || step.status === "warning").length))
            .replace("{total}", String(props.startup.steps.length))}</small> : null}
          <p className="startup-progress-message" title={message} role={activeStep?.status === "error" ? "alert" : undefined}>{message}</p>
          <StartupDownloadProgress step={activeStep} label={text.downloadProgress} />
        </div>
      </section>
    </main>
  );
}

// BootErrorScreen renders a boot failure before the full workbench bundle is loaded.
// BootErrorScreen 在完整工作台包加载前渲染启动失败状态。
export function BootErrorScreen(props: {
  // Error message rendered after startup fails.
  // 启动失败后展示的错误消息。
  error: string;
  // Service-provided UI locale.
  // 服务提供的界面语言。
  uiLocale?: string | null;
  // onReload explicitly restarts the application only after the user chooses recovery.
  // onReload 仅在用户选择恢复后显式重新加载应用。
  onReload?: () => void;
}) {
  // text stores localized labels for the boot error.
  // text 保存启动错误的本地化文案。
  const text = BOOT_TEXT[bootLocale(props.uiLocale)];
  return (
    <main className="loading-screen error">
      <section className="startup-error-panel" aria-live="assertive">
        <StartupLogoMark />
        <div className="startup-error-content">
          <strong>{text.failedPrefix}</strong>
          <details className="startup-error-details">
            <summary>{text.errorDetails}</summary>
            <p className="startup-error-message">{props.error}</p>
          </details>
          {props.onReload ? <button className="startup-reload-button" type="button" onClick={props.onReload}>
            {text.reloadApplication}
          </button> : null}
        </div>
      </section>
    </main>
  );
}

// bootLocale resolves the saved or device language without importing the full i18n module.
// bootLocale 在不导入完整 i18n 模块的情况下解析已保存语言或设备语言。
function bootLocale(value: string | null | undefined): LocaleCode {
  return resolveAppLocale(value);
}
