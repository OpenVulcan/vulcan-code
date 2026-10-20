import * as React from "react";
import { requestComposerCommand } from "@/components/composer/composer-external-commands";
import type { ElectronLocalBrowserBridge, ElectronLocalBrowserElementPick } from "@/host-shell/electron-bridge";
import { formatMessage, type Translate } from "@/i18n";

// elementPickComposerText describes a picked element for the draft: page, selector, element and its box.
// elementPickComposerText 为草稿描述选中的元素：页面、选择器、元素及其盒子位置。
export function elementPickComposerText(pick: ElectronLocalBrowserElementPick, t: Translate): string {
  return formatMessage(t("browser.elementPick.composerText"), {
    url: pick.url,
    selector: pick.selector,
    tag: pick.tag,
    text: pick.text ? ` "${pick.text}"` : "",
    viewportWidth: Math.round(pick.viewport.width),
    viewportHeight: Math.round(pick.viewport.height),
    x: Math.round(pick.rect.x),
    y: Math.round(pick.rect.y),
    width: Math.round(pick.rect.width),
    height: Math.round(pick.rect.height),
  });
}

// elementPickScreenshotFile wraps the annotated page screenshot as a PNG file the composer can stage.
// elementPickScreenshotFile 将带标注的页面截图包装为输入区可暂存的 PNG 文件。
export function elementPickScreenshotFile(pick: ElectronLocalBrowserElementPick, now: Date): File {
  const stamp = now.toISOString().replace(/[-:]/g, "").replace(/\.\d+Z$/, "Z");
  return new File([new Uint8Array(pick.screenshot)], `browser-element-${stamp}.png`, { type: "image/png" });
}

// BrowserElementPickState is whether a pick is running and the action that starts or cancels it.
// BrowserElementPickState 表示是否正在选择，以及开始或取消选择的操作。
export interface BrowserElementPickState {
  // available is false when the native bridge cannot pick elements.
  // available 在原生桥无法选择元素时为 false。
  available: boolean;
  picking: boolean;
  toggle: () => void;
}

// useBrowserElementPick lets the user point at an element of the active page and hands it, with an annotated
// screenshot, to the composer; choosing the action again while picking cancels it.
// useBrowserElementPick 让用户指向当前页面的一个元素，并将其连同带标注的截图交给输入区；选择进行中再次触发该操作会取消选择。
export function useBrowserElementPick(
  bridge: ElectronLocalBrowserBridge | null,
  activeTabId: string | null,
  t: Translate,
  onError: (message: string) => void,
): BrowserElementPickState {
  const [picking, setPicking] = React.useState(false);
  const pick = bridge?.pickElement;
  const cancel = bridge?.cancelElementPick;
  // pickingTabId is the page a running pick belongs to, so leaving that page can end it.
  // pickingTabId 是进行中的选择所属的页面，离开该页面时可以结束选择。
  const pickingTabId = React.useRef<string | null>(null);
  React.useEffect(() => () => {
    // A pick left running on a hidden page would keep swallowing that page's clicks.
    // 留在隐藏页面上的选择会持续吞掉该页面的点击。
    if (pickingTabId.current) void cancel?.(pickingTabId.current).catch(() => undefined);
  }, [activeTabId, cancel]);
  const toggle = React.useCallback(() => {
    if (!pick || !activeTabId) return;
    if (picking) {
      void cancel?.(activeTabId).catch((reason: unknown) => onError(String(reason)));
      return;
    }
    setPicking(true);
    pickingTabId.current = activeTabId;
    void pick(activeTabId)
      .then((picked) => {
        if (!picked) return;
        requestComposerCommand({
          kind: "insert",
          text: elementPickComposerText(picked, t),
          files: [elementPickScreenshotFile(picked, new Date())],
        });
      })
      .catch((reason: unknown) => onError(String(reason)))
      .finally(() => {
        pickingTabId.current = null;
        setPicking(false);
      });
  }, [activeTabId, cancel, onError, pick, picking, t]);
  return { available: Boolean(pick && cancel), picking, toggle };
}
