import React from "react";
import { RotateCw, X } from "lucide-react";
import { StartupLogoMark } from "@/components/shell/startup-logo-mark";
import { readElectronDesktopBridge } from "@/host-shell/electron-bridge";
import { StartupProgressPanel, type StartupProgressLabels } from "@/components/shell/startup-progress-panel";
import type { StartupSnapshot } from "@/types";
import { DEFAULT_LOCALE, resolveAppLocale, type LocaleCode } from "@/i18n-locales";
import defaultBootCatalog from "@catalogs/boot/en-US.json";

// BootText stores the minimal boot-screen labels: the shared progress copy plus the boot failure copy.
// BootText 保存最小启动屏文案：共用进度文案加上启动失败文案。
export interface BootText extends StartupProgressLabels {
  // Startup failure prefix.
  // 启动失败前缀。
  failedPrefix: string;
  // Label for optional raw failure details, keeping the recovery action visible.
  // 可选原始错误详情的标签，让恢复操作保持可见。
  errorDetails: string;
  // Explicit recovery action shown after a startup failure.
  // 启动失败后显示的显式恢复操作。
  reloadApplication: string;
  // Accessible label of the desktop startup page's close button.
  // 桌面启动页关闭按钮的无障碍标签。
  closeWindow: string;
}

// DEFAULT_BOOT_TEXT is the English boot catalog; assigning it proves the shared catalog covers BootText.
// DEFAULT_BOOT_TEXT 是英文启动目录；该赋值证明共享目录覆盖 BootText。
const DEFAULT_BOOT_TEXT: BootText = defaultBootCatalog;

// The boot entry loads only the selected locale, with English available synchronously.
// 启动入口只加载选中的语言，英语可同步读取。
const bootCatalogLoaders = new Map(
  Object.entries(import.meta.glob<Partial<BootText>>(["@catalogs/boot/*.json", "!@catalogs/boot/en-US.json"], { import: "default" }))
    .map(([path, load]) => [/([a-z]{2}-[A-Z]{2})\.json$/u.exec(path)?.[1], load] as const),
);

// Cache the localized result separately from its pending import for immediate reuse across startup components.
// 将本地化结果和待完成导入分开缓存，便于启动组件之间立即复用。
const bootCatalogs = new Map<LocaleCode, BootText>([[DEFAULT_LOCALE, DEFAULT_BOOT_TEXT]]);
// Concurrent consumers share one import per locale.
// 并发消费者为每种语言共享同一次导入。
const pendingBootCatalogs = new Map<LocaleCode, Promise<BootText>>();

/** Loads the boot labels for a locale, filling missing keys with the default catalog.
 * 加载指定语言的启动文案，使用默认目录补全缺失键。 */
export function loadBootText(locale: LocaleCode): Promise<BootText> {
  // Resolved text and in-flight imports are both shared by startup surfaces.
  // 已解析的文案与正在执行的导入都由启动界面共享。
  const cached = bootCatalogs.get(locale);
  if (cached) return Promise.resolve(cached);
  const pending = pendingBootCatalogs.get(locale);
  if (pending) return pending;
  const load = bootCatalogLoaders.get(locale);
  if (!load) return Promise.resolve(DEFAULT_BOOT_TEXT);
  // The default copy remains available if an updated installation cannot read a locale asset.
  // 更新后的安装无法读取语言资源时，仍可使用默认文案。
  const request = load().then((catalog) => {
    const text = { ...DEFAULT_BOOT_TEXT, ...catalog };
    bootCatalogs.set(locale, text);
    return text;
  }).catch((error: unknown) => {
    console.error(`Startup language catalog could not be loaded: ${locale}`, error);
    return DEFAULT_BOOT_TEXT;
  }).finally(() => pendingBootCatalogs.delete(locale));
  pendingBootCatalogs.set(locale, request);
  return request;
}

/** Returns the selected boot catalog once loaded, or null while its import is pending.
 * 选定的启动目录加载后返回该目录，导入等待期间返回 null。 */
export function useBootText(locale: LocaleCode): BootText | null {
  // The loaded result carries its locale so a previous selection cannot overwrite a newer one.
  // 加载结果携带语言标识，避免之前的选择覆盖新的选择。
  const [loaded, setLoaded] = React.useState<{ locale: LocaleCode; text: BootText }>();
  React.useEffect(() => {
    if (bootCatalogs.has(locale)) return;
    // An unmounted or superseded consumer no longer receives the result.
    // 已卸载或被新选择取代的消费者不再接收结果。
    let active = true;
    void loadBootText(locale).then((text) => { if (active) setLoaded({ locale, text }); });
    return () => { active = false; };
  }, [locale]);
  return bootCatalogs.get(locale) ?? (loaded?.locale === locale ? loaded.text : null);
}

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
  // Language used while no snapshot carries one, such as the desktop startup page before Core answers.
  // 尚无快照携带语言时使用的语言，例如 Core 应答前的桌面启动页。
  uiLocale?: string | null;
  // Host-reported detail shown while no snapshot exists yet.
  // 尚无快照时显示的宿主上报详情。
  detail?: string | null;
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
  const text = useBootText(bootLocale(props.startup?.uiLocale ?? props.uiLocale)) ?? DEFAULT_BOOT_TEXT;

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
      <StartupProgressPanel startup={props.startup ?? null} labels={text} detail={props.detail} />
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
  // False when the page already draws its own window bar, such as the desktop startup page.
  // 页面已自绘窗口条时为 false，例如桌面启动页。
  windowBar?: boolean;
}) {
  // text stores localized labels for the boot error.
  // text 保存启动错误的本地化文案。
  const text = useBootText(bootLocale(props.uiLocale)) ?? DEFAULT_BOOT_TEXT;
  // desktop is the frameless desktop window that needs drawn close and reload buttons, or null.
  // desktop 是需要自绘关闭与刷新按钮的无边框桌面窗口，否则为 null。
  const desktop = props.windowBar === false ? null : framelessDesktopWindow();
  return (
    <main className="loading-screen error">
      {desktop ? (
        <StartupWindowBar
          closeLabel={text.closeWindow}
          onClose={() => void desktop.closeWindow().catch(() => window.close())}
          onReload={props.onReload}
          reloadLabel={text.reloadApplication}
        />
      ) : null}
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

// NATIVE_WINDOW_BUTTONS is true on macOS, whose frameless desktop window keeps its native buttons.
// NATIVE_WINDOW_BUTTONS 在 macOS 上为 true，其无边框桌面窗口保留原生按钮。
export const NATIVE_WINDOW_BUTTONS = typeof navigator !== "undefined" && /Mac/u.test(navigator.userAgent);

// framelessDesktopWindow returns the desktop bridge when this page fills a desktop window without native buttons.
// framelessDesktopWindow 在本页占满无原生按钮的桌面窗口时返回桌面桥。
function framelessDesktopWindow() {
  return NATIVE_WINDOW_BUTTONS ? null : readElectronDesktopBridge();
}

// StartupWindowBar is the drag strip of a frameless desktop window before the workbench draws its own titlebar: an
// optional reload button and a close button at the right edge.
// StartupWindowBar 是工作台自绘标题栏之前无边框桌面窗口的拖动条：右端可选的刷新按钮与关闭按钮。
export function StartupWindowBar(props: {
  // Accessible label of the close button.
  // 关闭按钮的无障碍标签。
  closeLabel: string;
  // Closes the window.
  // 关闭窗口。
  onClose: () => void;
  // Reloads the page; the reload button is shown only when it is given.
  // 重新加载页面；仅在提供时显示刷新按钮。
  onReload?: () => void;
  // Accessible label of the reload button.
  // 刷新按钮的无障碍标签。
  reloadLabel?: string;
}) {
  return (
    <div className="startup-window-bar">
      {props.onReload ? (
        <button aria-label={props.reloadLabel} className="startup-window-button" onClick={props.onReload} title={props.reloadLabel} type="button">
          <RotateCw />
        </button>
      ) : null}
      <button aria-label={props.closeLabel} className="startup-window-button" data-action="close" onClick={props.onClose} title={props.closeLabel} type="button">
        <X />
      </button>
    </div>
  );
}

// bootLocale resolves the saved or device language without importing the full i18n module.
// bootLocale 在不导入完整 i18n 模块的情况下解析已保存语言或设备语言。
function bootLocale(value: string | null | undefined): LocaleCode {
  return resolveAppLocale(value);
}
