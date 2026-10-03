import React from "react";
import { StartupLogoMark } from "@/components/shell/startup-logo-mark";
import { StartupProgressPanel, type StartupProgressLabels } from "@/components/shell/startup-progress-panel";
import type { StartupSnapshot } from "@/types";
import { resolveAppLocale, type LocaleCode } from "@/i18n-locales";

// BootText stores the minimal boot-screen labels: the shared progress copy plus the boot failure copy.
// BootText 保存最小启动屏文案：共用进度文案加上启动失败文案。
interface BootText extends StartupProgressLabels {
  // Startup failure prefix.
  // 启动失败前缀。
  failedPrefix: string;
  // Label for optional raw failure details, keeping the recovery action visible.
  // 可选原始错误详情的标签，让恢复操作保持可见。
  errorDetails: string;
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
    loadingStep: "正在加载 {step}",
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
    loadingStep: "Loading {step}",
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
    loadingStep: "正在載入 {step}",
    failedPrefix: "啟動失敗：",
    errorDetails: "檢視詳細資訊",
    downloadProgress: "下載進度",
    completedSteps: "已完成 {completed}/{total} 個步驟",
    reloadApplication: "重新載入應用程式",
  },
  "zh-HK": {
    loadingWorkspace: "正在載入 Workspace",
    connectingService: "正在連接 Vulcan Core Service",
    loadingProjectIndex: "正在載入項目與對話索引",
    loadingStep: "正在載入 {step}",
    failedPrefix: "啟動失敗：",
    errorDetails: "查看詳細資訊",
    downloadProgress: "下載進度",
    completedSteps: "已完成 {completed}/{total} 個步驟",
    reloadApplication: "重新載入應用程式",
  },
  "ja-JP": {
    loadingWorkspace: "Workspace を読み込んでいます",
    connectingService: "Vulcan Core Service に接続しています",
    loadingProjectIndex: "プロジェクトと会話のインデックスを読み込んでいます",
    loadingStep: "{step} を読み込んでいます",
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
    loadingStep: "{step} 불러오는 중",
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
    loadingStep: "Cargando {step}",
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
    loadingStep: "Chargement de {step}",
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
    loadingStep: "{step} wird geladen",
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
    loadingStep: "Carregando {step}",
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
    loadingStep: "Загрузка: {step}",
    failedPrefix: "Не удалось запустить: ",
    errorDetails: "Подробнее",
    downloadProgress: "Ход загрузки",
    completedSteps: "Выполнено шагов: {completed} из {total}",
    reloadApplication: "Перезагрузить приложение",
  },
};

// STARTUP_SURFACE_FADE_MS is the single duration of the startup surface hand-off fade; the stylesheet reads it
// through --startup-surface-fade.
// STARTUP_SURFACE_FADE_MS 是启动层交接淡出的唯一时长；样式表通过 --startup-surface-fade 读取。
export const STARTUP_SURFACE_FADE_MS = 320;

// BootLoadingScreen renders the persistent startup surface with real startup progress. WebuiApp keeps exactly one
// instance mounted from the first probe until the workbench takes over, so the logo motion plays once per page load;
// on hand-off it fades out over the freshly mounted workbench and then reports that it has left.
// BootLoadingScreen 渲染带真实启动进度的常驻启动层。WebuiApp 从首次探测到工作台接管期间只挂载一个实例，
// 使标志动效每次页面加载只播放一次；交接时它在刚挂载的工作台上方淡出，随后报告已离场。
export function BootLoadingScreen(props: {
  // Startup snapshot received before the workbench bundle is loaded.
  // 工作台包加载前收到的启动快照。
  startup?: StartupSnapshot | null;
  // True once the workbench owns the screen; the surface fades out and stops intercepting input.
  // 工作台接管屏幕后为 true；启动层淡出并停止拦截输入。
  leaving?: boolean;
  // Called after the fade-out so the owner can unmount the surface.
  // 淡出结束后调用，使持有者可以卸载启动层。
  onLeft?: () => void;
}) {
  const { leaving = false, onLeft } = props;
  // text stores localized labels for the boot screen without importing the full translation catalog.
  // text 保存启动屏本地化文案，而不导入完整翻译表。
  const text = BOOT_TEXT[bootLocale(props.startup?.uiLocale)];

  React.useEffect(() => {
    if (!leaving || !onLeft) {
      return;
    }
    const timer = window.setTimeout(onLeft, STARTUP_SURFACE_FADE_MS);
    return () => window.clearTimeout(timer);
  }, [leaving, onLeft]);

  return (
    <main
      className="loading-screen startup-surface"
      data-leaving={leaving ? "" : undefined}
      aria-hidden={leaving ? true : undefined}
      style={{ "--startup-surface-fade": `${STARTUP_SURFACE_FADE_MS}ms` } as React.CSSProperties}
    >
      <StartupProgressPanel startup={props.startup ?? null} labels={text} />
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
