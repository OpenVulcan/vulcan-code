// User-facing text for the desktop shell, shown before or outside the WebUI in the WebUI's languages.
// 桌面壳面向用户的文本，在 WebUI 之前或之外以 WebUI 所支持的语言展示。
//
// The language list and matching rules mirror `SUPPORTED_LOCALES` and `matchLocaleTag` in
// `apps/vulcan-code-web/src/i18n-locales.ts`, which own them; a desktop test keeps the lists aligned.
// 语言列表与匹配规则与 `apps/vulcan-code-web/src/i18n-locales.ts` 中持有它们的 `SUPPORTED_LOCALES`、
// `matchLocaleTag` 保持一致；一个桌面测试保证列表一致。

/** One language supported by the desktop shell.
 * 桌面壳支持的一种语言。
 */
export type DesktopLocale =
  | 'en-US' | 'zh-CN' | 'zh-TW' | 'zh-HK' | 'ja-JP' | 'ko-KR' | 'es-ES' | 'fr-FR' | 'de-DE' | 'pt-BR' | 'ru-RU';

/** Every piece of desktop-shell text; `{name}` placeholders are filled by `formatDesktopText`. */
export interface DesktopText {
  // Startup window document title.
  // 启动窗口文档标题。
  startupTitle: string;
  // Accessible label of the startup window close button.
  // 启动窗口关闭按钮的无障碍标签。
  closeWindow: string;
  // Stage text before the first real stage arrives.
  // 首个真实阶段到达前的阶段文本。
  preparing: string;
  // Stage text after a startup failure.
  // 启动失败后的阶段文本。
  startupFailed: string;
  // Launcher stage: probing for a running service.
  // 启动器阶段：探测正在运行的服务。
  checkingInstance: string;
  // Launcher stage: reusing a running service, `{pid}`.
  // 启动器阶段：复用正在运行的服务，`{pid}`。
  reusingInstance: string;
  // Launcher stage: starting the background service.
  // 启动器阶段：启动后台服务。
  startingService: string;
  // Launcher stage: waiting for business readiness.
  // 启动器阶段：等待业务就绪。
  awaitingReady: string;
  // Launcher stage: service ready, `{pid}`.
  // 启动器阶段：服务就绪，`{pid}`。
  serviceReady: string;
  // Launcher failure: the service process could not be spawned, `{error}`.
  // 启动器失败：无法创建服务进程，`{error}`。
  launchFailed: string;
  // Launcher failure: the process exited before readiness, `{code}`.
  // 启动器失败：进程在就绪前退出，`{code}`。
  exitedBeforeReady: string;
  // Launcher failure: the service reported an unusable status, `{status}`.
  // 启动器失败：服务报告了不可用状态，`{status}`。
  serviceUnavailable: string;
  // Launcher failure: no ready discovery document in time, `{seconds}` and `{path}`.
  // 启动器失败：未在时限内发布就绪发现文档，`{seconds}` 与 `{path}`。
  readyTimeout: string;
  // Workbench failure: the WebUI could not be loaded, `{url}` and `{error}`.
  // 工作台失败：无法加载 WebUI，`{url}` 与 `{error}`。
  workbenchLoadFailed: string;
  // Built-in browser close confirmation title.
  // 内置浏览器关闭确认标题。
  closeBrowserTitle: string;
  // Built-in browser close confirmation message, `{count}`.
  // 内置浏览器关闭确认消息，`{count}`。
  closeBrowserMessage: string;
  // Built-in browser close confirmation detail.
  // 内置浏览器关闭确认详情。
  closeBrowserDetail: string;
  // Cancel button.
  // 取消按钮。
  cancel: string;
  // Confirm button closing every page.
  // 关闭全部页面的确认按钮。
  closeAll: string;
}

/** Desktop-shell text for every supported language.
 * 每种受支持语言的桌面壳文本。
 */
export const DESKTOP_TEXT: Readonly<Record<DesktopLocale, DesktopText>> = {
  'en-US': {
    startupTitle: 'Vulcan Code is starting',
    closeWindow: 'Close window',
    preparing: 'Preparing…',
    startupFailed: 'Startup failed',
    checkingInstance: 'Checking for a running service…',
    reusingInstance: 'Connected to the running service (process {pid}).',
    startingService: 'Starting the background service…',
    awaitingReady: 'Waiting for the service to become ready…',
    serviceReady: 'The service is ready (process {pid}).',
    launchFailed: 'Unable to start the background service: {error}',
    exitedBeforeReady: 'The service process exited before becoming ready (exit code {code}).',
    serviceUnavailable: 'The background service is not available yet. Current status: {status}.',
    readyTimeout: 'The service did not publish its ready discovery document within {seconds} seconds: {path}',
    workbenchLoadFailed: 'Unable to load the workbench {url}: {error}',
    closeBrowserTitle: 'Close built-in browser',
    closeBrowserMessage: '{count} pages are open. Close all of them?',
    closeBrowserDetail: 'This releases the resources used by these pages and then closes the browser tab.',
    cancel: 'Cancel',
    closeAll: 'Close all',
  },
  'zh-CN': {
    startupTitle: 'Vulcan Code 启动中',
    closeWindow: '关闭窗口',
    preparing: '正在准备…',
    startupFailed: '启动失败',
    checkingInstance: '正在检查已在运行的服务…',
    reusingInstance: '已连接到运行中的服务（进程 {pid}）。',
    startingService: '正在启动后台服务…',
    awaitingReady: '正在等待服务完成业务就绪…',
    serviceReady: '服务已就绪（进程 {pid}）。',
    launchFailed: '无法启动后台服务：{error}',
    exitedBeforeReady: '服务进程在就绪前退出，退出码 {code}。',
    serviceUnavailable: '后台服务尚不可用，当前状态：{status}。',
    readyTimeout: '服务在 {seconds} 秒内未发布就绪发现文档：{path}',
    workbenchLoadFailed: '无法加载工作台 {url}：{error}',
    closeBrowserTitle: '关闭内置浏览器',
    closeBrowserMessage: '当前有 {count} 个正在访问的页面，是否全部关闭？',
    closeBrowserDetail: '全部关闭将释放这些网页占用的资源，然后关闭内置浏览器页签。',
    cancel: '取消',
    closeAll: '全部关闭',
  },
  'zh-TW': {
    startupTitle: 'Vulcan Code 啟動中',
    closeWindow: '關閉視窗',
    preparing: '正在準備…',
    startupFailed: '啟動失敗',
    checkingInstance: '正在檢查已在執行的服務…',
    reusingInstance: '已連線到執行中的服務（處理程序 {pid}）。',
    startingService: '正在啟動背景服務…',
    awaitingReady: '正在等待服務完成就緒…',
    serviceReady: '服務已就緒（處理程序 {pid}）。',
    launchFailed: '無法啟動背景服務：{error}',
    exitedBeforeReady: '服務處理程序在就緒前結束，結束代碼 {code}。',
    serviceUnavailable: '背景服務尚無法使用，目前狀態：{status}。',
    readyTimeout: '服務未在 {seconds} 秒內發布就緒探索文件：{path}',
    workbenchLoadFailed: '無法載入工作台 {url}：{error}',
    closeBrowserTitle: '關閉內建瀏覽器',
    closeBrowserMessage: '目前有 {count} 個開啟中的頁面，要全部關閉嗎？',
    closeBrowserDetail: '全部關閉會釋放這些網頁佔用的資源，然後關閉內建瀏覽器分頁。',
    cancel: '取消',
    closeAll: '全部關閉',
  },
  // Hong Kong Traditional Chinese uses local desktop terminology and retains the English placeholders.
  // 香港繁体中文采用当地桌面术语，并保留英文占位符。
  'zh-HK': {
    startupTitle: 'Vulcan Code 啟動中',
    closeWindow: '關閉窗口',
    preparing: '正在準備…',
    startupFailed: '啟動失敗',
    checkingInstance: '正在檢查已在運行的服務…',
    reusingInstance: '已連接至運行中的服務（程序 {pid}）。',
    startingService: '正在啟動背景服務…',
    awaitingReady: '正在等待服務就緒…',
    serviceReady: '服務已就緒（程序 {pid}）。',
    launchFailed: '無法啟動背景服務：{error}',
    exitedBeforeReady: '服務程序在就緒前退出，退出碼 {code}。',
    serviceUnavailable: '背景服務尚未可用，目前狀態：{status}。',
    readyTimeout: '服務未在 {seconds} 秒內發佈就緒探索文檔：{path}',
    workbenchLoadFailed: '無法載入工作台 {url}：{error}',
    closeBrowserTitle: '關閉內置瀏覽器',
    closeBrowserMessage: '目前有 {count} 個已開啟的頁面，是否全部關閉？',
    closeBrowserDetail: '全部關閉會釋放這些網頁佔用的資源，然後關閉內置瀏覽器標籤頁。',
    cancel: '取消',
    closeAll: '全部關閉',
  },
  'ja-JP': {
    startupTitle: 'Vulcan Code を起動しています',
    closeWindow: 'ウィンドウを閉じる',
    preparing: '準備しています…',
    startupFailed: '起動に失敗しました',
    checkingInstance: '実行中のサービスを確認しています…',
    reusingInstance: '実行中のサービスに接続しました（プロセス {pid}）。',
    startingService: 'バックグラウンドサービスを起動しています…',
    awaitingReady: 'サービスの準備完了を待っています…',
    serviceReady: 'サービスの準備ができました（プロセス {pid}）。',
    launchFailed: 'バックグラウンドサービスを起動できません：{error}',
    exitedBeforeReady: 'サービスプロセスが準備完了前に終了しました（終了コード {code}）。',
    serviceUnavailable: 'バックグラウンドサービスはまだ利用できません。現在の状態：{status}。',
    readyTimeout: 'サービスが {seconds} 秒以内に準備完了の検出ドキュメントを公開しませんでした：{path}',
    workbenchLoadFailed: 'ワークベンチ {url} を読み込めません：{error}',
    closeBrowserTitle: '内蔵ブラウザーを閉じる',
    closeBrowserMessage: '{count} 個のページが開いています。すべて閉じますか？',
    closeBrowserDetail: 'これらのページが使用しているリソースを解放してから、ブラウザータブを閉じます。',
    cancel: 'キャンセル',
    closeAll: 'すべて閉じる',
  },
  'ko-KR': {
    startupTitle: 'Vulcan Code 시작 중',
    closeWindow: '창 닫기',
    preparing: '준비 중…',
    startupFailed: '시작 실패',
    checkingInstance: '실행 중인 서비스를 확인하는 중…',
    reusingInstance: '실행 중인 서비스에 연결했습니다(프로세스 {pid}).',
    startingService: '백그라운드 서비스를 시작하는 중…',
    awaitingReady: '서비스가 준비될 때까지 기다리는 중…',
    serviceReady: '서비스가 준비되었습니다(프로세스 {pid}).',
    launchFailed: '백그라운드 서비스를 시작할 수 없습니다: {error}',
    exitedBeforeReady: '서비스 프로세스가 준비되기 전에 종료되었습니다(종료 코드 {code}).',
    serviceUnavailable: '백그라운드 서비스를 아직 사용할 수 없습니다. 현재 상태: {status}.',
    readyTimeout: '서비스가 {seconds}초 안에 준비 완료 검색 문서를 게시하지 않았습니다: {path}',
    workbenchLoadFailed: '워크벤치 {url}을(를) 불러올 수 없습니다: {error}',
    closeBrowserTitle: '내장 브라우저 닫기',
    closeBrowserMessage: '{count}개의 페이지가 열려 있습니다. 모두 닫을까요?',
    closeBrowserDetail: '이 페이지들이 사용하는 리소스를 해제한 다음 브라우저 탭을 닫습니다.',
    cancel: '취소',
    closeAll: '모두 닫기',
  },
  'es-ES': {
    startupTitle: 'Iniciando Vulcan Code',
    closeWindow: 'Cerrar ventana',
    preparing: 'Preparando…',
    startupFailed: 'Error al iniciar',
    checkingInstance: 'Buscando un servicio en ejecución…',
    reusingInstance: 'Conectado al servicio en ejecución (proceso {pid}).',
    startingService: 'Iniciando el servicio en segundo plano…',
    awaitingReady: 'Esperando a que el servicio esté listo…',
    serviceReady: 'El servicio está listo (proceso {pid}).',
    launchFailed: 'No se puede iniciar el servicio en segundo plano: {error}',
    exitedBeforeReady: 'El proceso del servicio terminó antes de estar listo (código de salida {code}).',
    serviceUnavailable: 'El servicio en segundo plano aún no está disponible. Estado actual: {status}.',
    readyTimeout: 'El servicio no publicó su documento de descubrimiento en {seconds} segundos: {path}',
    workbenchLoadFailed: 'No se puede cargar el área de trabajo {url}: {error}',
    closeBrowserTitle: 'Cerrar el navegador integrado',
    closeBrowserMessage: 'Hay {count} páginas abiertas. ¿Quieres cerrarlas todas?',
    closeBrowserDetail: 'Se liberarán los recursos que usan estas páginas y después se cerrará la pestaña del navegador.',
    cancel: 'Cancelar',
    closeAll: 'Cerrar todas',
  },
  'fr-FR': {
    startupTitle: 'Démarrage de Vulcan Code',
    closeWindow: 'Fermer la fenêtre',
    preparing: 'Préparation…',
    startupFailed: 'Échec du démarrage',
    checkingInstance: 'Recherche d’un service en cours d’exécution…',
    reusingInstance: 'Connecté au service en cours d’exécution (processus {pid}).',
    startingService: 'Démarrage du service en arrière-plan…',
    awaitingReady: 'En attente que le service soit prêt…',
    serviceReady: 'Le service est prêt (processus {pid}).',
    launchFailed: 'Impossible de démarrer le service en arrière-plan : {error}',
    exitedBeforeReady: 'Le processus du service s’est arrêté avant d’être prêt (code de sortie {code}).',
    serviceUnavailable: 'Le service en arrière-plan n’est pas encore disponible. État actuel : {status}.',
    readyTimeout: 'Le service n’a pas publié son document de découverte en {seconds} secondes : {path}',
    workbenchLoadFailed: 'Impossible de charger l’espace de travail {url} : {error}',
    closeBrowserTitle: 'Fermer le navigateur intégré',
    closeBrowserMessage: '{count} pages sont ouvertes. Les fermer toutes ?',
    closeBrowserDetail: 'Les ressources utilisées par ces pages seront libérées, puis l’onglet du navigateur sera fermé.',
    cancel: 'Annuler',
    closeAll: 'Tout fermer',
  },
  'de-DE': {
    startupTitle: 'Vulcan Code wird gestartet',
    closeWindow: 'Fenster schließen',
    preparing: 'Wird vorbereitet…',
    startupFailed: 'Start fehlgeschlagen',
    checkingInstance: 'Suche nach einem laufenden Dienst…',
    reusingInstance: 'Mit dem laufenden Dienst verbunden (Prozess {pid}).',
    startingService: 'Hintergrunddienst wird gestartet…',
    awaitingReady: 'Warten, bis der Dienst bereit ist…',
    serviceReady: 'Der Dienst ist bereit (Prozess {pid}).',
    launchFailed: 'Der Hintergrunddienst kann nicht gestartet werden: {error}',
    exitedBeforeReady: 'Der Dienstprozess wurde beendet, bevor er bereit war (Exitcode {code}).',
    serviceUnavailable: 'Der Hintergrunddienst ist noch nicht verfügbar. Aktueller Status: {status}.',
    readyTimeout: 'Der Dienst hat sein Discovery-Dokument nicht innerhalb von {seconds} Sekunden veröffentlicht: {path}',
    workbenchLoadFailed: 'Die Arbeitsumgebung {url} kann nicht geladen werden: {error}',
    closeBrowserTitle: 'Integrierten Browser schließen',
    closeBrowserMessage: '{count} Seiten sind geöffnet. Alle schließen?',
    closeBrowserDetail: 'Dadurch werden die von diesen Seiten belegten Ressourcen freigegeben und anschließend der Browser-Tab geschlossen.',
    cancel: 'Abbrechen',
    closeAll: 'Alle schließen',
  },
  'pt-BR': {
    startupTitle: 'Iniciando o Vulcan Code',
    closeWindow: 'Fechar janela',
    preparing: 'Preparando…',
    startupFailed: 'Falha ao iniciar',
    checkingInstance: 'Procurando um serviço em execução…',
    reusingInstance: 'Conectado ao serviço em execução (processo {pid}).',
    startingService: 'Iniciando o serviço em segundo plano…',
    awaitingReady: 'Aguardando o serviço ficar pronto…',
    serviceReady: 'O serviço está pronto (processo {pid}).',
    launchFailed: 'Não foi possível iniciar o serviço em segundo plano: {error}',
    exitedBeforeReady: 'O processo do serviço foi encerrado antes de ficar pronto (código de saída {code}).',
    serviceUnavailable: 'O serviço em segundo plano ainda não está disponível. Status atual: {status}.',
    readyTimeout: 'O serviço não publicou o documento de descoberta em {seconds} segundos: {path}',
    workbenchLoadFailed: 'Não foi possível carregar o ambiente de trabalho {url}: {error}',
    closeBrowserTitle: 'Fechar o navegador integrado',
    closeBrowserMessage: 'Há {count} páginas abertas. Fechar todas?',
    closeBrowserDetail: 'Isso libera os recursos usados por essas páginas e depois fecha a aba do navegador.',
    cancel: 'Cancelar',
    closeAll: 'Fechar todas',
  },
  'ru-RU': {
    startupTitle: 'Запуск Vulcan Code',
    closeWindow: 'Закрыть окно',
    preparing: 'Подготовка…',
    startupFailed: 'Не удалось запустить',
    checkingInstance: 'Поиск запущенной службы…',
    reusingInstance: 'Подключено к запущенной службе (процесс {pid}).',
    startingService: 'Запуск фоновой службы…',
    awaitingReady: 'Ожидание готовности службы…',
    serviceReady: 'Служба готова (процесс {pid}).',
    launchFailed: 'Не удалось запустить фоновую службу: {error}',
    exitedBeforeReady: 'Процесс службы завершился до готовности (код выхода {code}).',
    serviceUnavailable: 'Фоновая служба пока недоступна. Текущее состояние: {status}.',
    readyTimeout: 'Служба не опубликовала документ обнаружения за {seconds} с: {path}',
    workbenchLoadFailed: 'Не удалось загрузить рабочую среду {url}: {error}',
    closeBrowserTitle: 'Закрыть встроенный браузер',
    closeBrowserMessage: 'Открыто страниц: {count}. Закрыть все?',
    closeBrowserDetail: 'Ресурсы, занятые этими страницами, будут освобождены, после чего вкладка браузера закроется.',
    cancel: 'Отмена',
    closeAll: 'Закрыть все',
  },
};

/** Default language when no device language is supported. */
export const DEFAULT_DESKTOP_LOCALE: DesktopLocale = 'en-US';

/** Regions whose Chinese defaults to Traditional script when no script subtag is given. */
const TRADITIONAL_CHINESE_REGIONS: ReadonlySet<string> = new Set(['tw', 'hk', 'mo']);

/** Returns the exact supported locale for a wire value such as `zh-TW`, or `null`. */
export function normalizeDesktopLocale(value: string | null | undefined): DesktopLocale | null {
  return value && Object.hasOwn(DESKTOP_TEXT, value) ? value as DesktopLocale : null;
}

/** Maps one device language tag by explicit Chinese script and region, or by a supported language.
 * 根据中文的显式书写系统与地区，或其他受支持语言，映射设备语言标签。
 * @param tag BCP 47 device language tag; underscores are accepted as subtag separators.
 * @param tag BCP 47 设备语言标签；允许下划线作为子标签分隔符。
 * @returns The closest supported desktop locale, or `null` for an unsupported language.
 * @returns 最接近的受支持桌面语言；不受支持的语言返回 `null`。
 */
export function matchDesktopLocaleTag(tag: string): DesktopLocale | null {
  const subtags = tag.trim().replace(/_/g, '-').toLowerCase().split('-').filter(Boolean);
  const language = subtags[0];
  if (!language) return null;
  if (language === 'zh') {
    // Hong Kong selects its own Traditional catalog; other Traditional regions retain Taiwan's catalog.
    // 香港选择专属繁体目录；其他繁体地区继续使用台湾目录。
    const traditionalLocale = subtags.includes('hk') ? 'zh-HK' : 'zh-TW';
    if (subtags.includes('hant')) return traditionalLocale;
    if (subtags.includes('hans')) return 'zh-CN';
    return subtags.some((subtag) => TRADITIONAL_CHINESE_REGIONS.has(subtag)) ? traditionalLocale : 'zh-CN';
  }
  const locales = Object.keys(DESKTOP_TEXT) as DesktopLocale[];
  return locales.find((locale) => locale.toLowerCase() === subtags.join('-'))
    ?? locales.find((locale) => locale.slice(0, locale.indexOf('-')).toLowerCase() === language)
    ?? null;
}

/** Picks the first supported device language, falling back to English. */
export function detectDesktopLocale(preferredTags: readonly string[]): DesktopLocale {
  for (const tag of preferredTags) {
    const match = matchDesktopLocaleTag(tag);
    if (match) return match;
  }
  return DEFAULT_DESKTOP_LOCALE;
}

/** Fills every `{name}` placeholder; placeholders without a value stay visible. */
export function formatDesktopText(template: string, values: Readonly<Record<string, string | number>>): string {
  return template.replace(/\{(\w+)\}/g, (placeholder, name: string) => Object.hasOwn(values, name) ? String(values[name]) : placeholder);
}
