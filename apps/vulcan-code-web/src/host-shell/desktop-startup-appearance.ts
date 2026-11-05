import { readElectronStartupBridge, type DesktopStartupAppearance } from "@/host-shell/electron-bridge";
import { APPEARANCE_THEMES, getAppearance } from "@/ui-kit/appearance";

// The desktop startup page runs from local files before Core answers, so it cannot read the WebUI's stored language or
// appearance. The workbench reports them to the desktop shell, which remembers them for the next startup page.
// 桌面启动页在 Core 应答前从本地文件运行，读不到 WebUI 已保存的语言与外观。工作台把它们上报给桌面壳，由桌面壳
// 为下一次启动页记住。

// REPORT_DELAY_MS lets a burst of root attribute changes settle into one report.
// REPORT_DELAY_MS 让一串根节点属性变化合并为一次上报。
const REPORT_DELAY_MS = 400;

// REPORTED_ROOT_ATTRIBUTES are the root attributes whose change can alter the reported appearance.
// REPORTED_ROOT_ATTRIBUTES 是其变化会改变上报外观的根节点属性。
const REPORTED_ROOT_ATTRIBUTES = ["lang", "data-vk-theme", "data-vk-scheme", "data-vk-text", "data-vk-motion"];

// currentStartupAppearance reads the language and appearance the document shows now.
// currentStartupAppearance 读取文档当前展示的语言与外观。
function currentStartupAppearance(): DesktopStartupAppearance {
  const root = document.documentElement;
  const appearance = getAppearance();
  // A skin is not part of the startup page, so it is reported as the light or dark theme it renders.
  // 皮肤不属于启动页，因此按其呈现的明暗主题上报。
  const rendered = root.dataset.vkTheme;
  const theme = rendered === undefined || (APPEARANCE_THEMES as readonly string[]).includes(rendered)
    ? appearance.theme
    : root.dataset.vkScheme === "dark" ? "dark" : "light";
  return {
    locale: root.lang,
    theme,
    text: appearance.text,
    motion: appearance.motion,
    ground: getComputedStyle(root).getPropertyValue("--vk-ground").trim(),
  };
}

// startDesktopStartupAppearanceReport reports the appearance now and after every change; it does nothing outside the
// desktop shell. Returns the function that stops reporting.
// startDesktopStartupAppearanceReport 立即并在每次变化后上报外观；在桌面壳之外不做任何事。返回停止上报的函数。
export function startDesktopStartupAppearanceReport(): () => void {
  const bridge = readElectronStartupBridge();
  if (!bridge) return () => {};
  let reported = "";
  let timer: number | undefined;
  const report = () => {
    timer = undefined;
    const appearance = currentStartupAppearance();
    const serialized = JSON.stringify(appearance);
    if (serialized === reported) return;
    reported = serialized;
    void bridge.reportAppearance(appearance).catch((error: unknown) => {
      console.error("The desktop startup appearance could not be saved.", error);
    });
  };
  const schedule = () => {
    if (timer !== undefined) window.clearTimeout(timer);
    timer = window.setTimeout(report, REPORT_DELAY_MS);
  };
  const observer = new MutationObserver(schedule);
  observer.observe(document.documentElement, { attributes: true, attributeFilter: REPORTED_ROOT_ATTRIBUTES });
  schedule();
  return () => {
    observer.disconnect();
    if (timer !== undefined) window.clearTimeout(timer);
  };
}
