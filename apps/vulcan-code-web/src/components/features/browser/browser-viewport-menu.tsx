import * as React from "react";
import { Maximize, MonitorSmartphone, SlidersHorizontal } from "lucide-react";
import { BROWSER_HOST_MAX_VIEWPORT_DIMENSION } from "@/generated/protocol-limits";
import { formatMessage, useI18n } from "@/i18n";
import { Button } from "@/ui-kit/primitives/button";
import {
  Dialog, DialogClose, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle,
} from "@/ui-kit/primitives/dialog";
import {
  DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuLabel, DropdownMenuRadioGroup,
  DropdownMenuRadioItem, DropdownMenuSeparator, DropdownMenuShortcut, DropdownMenuTrigger,
} from "@/ui-kit/primitives/dropdown-menu";

// BrowserViewportSize gives the Chromium content viewport in CSS pixels.
// BrowserViewportSize 表示以 CSS 像素计量的 Chromium 内容视口。
type BrowserViewportSize = { width: number; height: number };

// Desktop presets cover common workbench and display sizes.
// 桌面预设覆盖常见的工作台和显示器尺寸。
const DESKTOP_SIZES: readonly BrowserViewportSize[] = [
  { width: 1280, height: 800 }, { width: 1366, height: 768 },
  { width: 1440, height: 900 }, { width: 1920, height: 1080 }, { width: 2560, height: 1440 },
];

// Chrome DevTools device profiles supply the portrait CSS dimensions. Derive landscape by swapping axes.
// 竖屏 CSS 尺寸取自 Chrome DevTools 的设备预设，横屏通过交换宽高派生。
// https://github.com/ChromeDevTools/devtools-frontend/blob/main/front_end/models/emulation/EmulatedDevices.ts
const PORTRAIT_SIZES = [
  { device: "Samsung Galaxy S8+", width: 360, height: 740 },
  { device: "iPhone SE", width: 375, height: 667 },
  { device: "iPhone 12 Pro", width: 390, height: 844 },
  { device: "iPhone 14 Pro", width: 393, height: 852 },
  { device: "Google Pixel 7", width: 412, height: 915 },
  { device: "iPhone 14 Pro Max", width: 430, height: 932 },
  { device: "iPad Mini", width: 768, height: 1024 },
] as const;

// sizeLabel formats one viewport for display and exact selection matching.
// sizeLabel 格式化一个视口，供显示和精确匹配选中项。
function sizeLabel({ width, height }: BrowserViewportSize): string {
  return `${width}×${height}`;
}

// The parent supplies authoritative size and one-shot operations for the current browser.
// 父组件提供当前浏览器的权威尺寸及一次性操作。
interface BrowserViewportMenuProps {
  size: BrowserViewportSize | null;
  disabled: boolean;
  readPanelSize: () => BrowserViewportSize | null;
  resize: (size: BrowserViewportSize) => void;
}

// Open a single toolbar menu for presets and fitting, with a dialog for explicit custom dimensions.
// 工具栏通过同一菜单提供预设和适配操作，并通过对话框录入自定义尺寸。
export function BrowserViewportMenu({ size, disabled, readPanelSize, resize }: BrowserViewportMenuProps) {
  const { t } = useI18n();
  // The dialog holds a draft independent of remote viewport changes.
  // 对话框保存独立于远端视口变化的输入草稿。
  const [customOpen, setCustomOpen] = React.useState(false);
  // The displayed size is sampled on open; the action samples again before applying.
  // 面板尺寸在打开菜单时采样用于展示，应用前再次采样。
  const [panelSize, setPanelSize] = React.useState<BrowserViewportSize | null>(null);
  const trigger = React.useRef<HTMLButtonElement>(null);
  const openingCustom = React.useRef(false);
  const currentSize = size ? sizeLabel(size) : undefined;

  // Read the validated number inputs and dispatch exactly one explicit resize.
  // 读取已校验的数字输入，并派发恰好一次显式调整。
  function submitCustom(event: React.FormEvent<HTMLFormElement>): void {
    event.preventDefault();
    const form = event.currentTarget;
    if (disabled || !form.reportValidity()) return;
    const dimensions = new FormData(form);
    setCustomOpen(false);
    resize({ width: Number(dimensions.get("width")), height: Number(dimensions.get("height")) });
  }

  // Apply the panel dimensions at the instant the user selects this command.
  // 应用用户选择此命令当下的面板尺寸。
  function fitCurrentPanel(): void {
    const measured = readPanelSize();
    if (measured) resize(measured);
  }

  return (
    <>
      <DropdownMenu onOpenChange={open => { if (open) setPanelSize(readPanelSize()); }}>
        <DropdownMenuTrigger asChild>
          <Button ref={trigger} type="button" size="icon-sm" variant="ghost"
            aria-label={t("browser.resizeViewport")} title={t("browser.resizeViewport")}>
            <MonitorSmartphone size={14} />
          </Button>
        </DropdownMenuTrigger>
        <DropdownMenuContent align="end" onCloseAutoFocus={event => {
          if (openingCustom.current) event.preventDefault();
          openingCustom.current = false;
        }}>
          {currentSize && <DropdownMenuLabel>{t("browser.pageSize")} {currentSize}</DropdownMenuLabel>}
          <DropdownMenuItem disabled={disabled || !panelSize} onSelect={fitCurrentPanel}>
            <Maximize />{t("browser.fitCurrentViewport")}
            {panelSize && <DropdownMenuShortcut>{sizeLabel(panelSize)}</DropdownMenuShortcut>}
          </DropdownMenuItem>
          <DropdownMenuItem disabled={disabled} onSelect={() => { openingCustom.current = true; setCustomOpen(true); }}>
            <SlidersHorizontal />{t("browser.customViewport")}
          </DropdownMenuItem>
          <DropdownMenuSeparator />
          <DropdownMenuRadioGroup value={currentSize} aria-label={t("browser.viewportDesktop")}>
            <DropdownMenuLabel>{t("browser.viewportDesktop")}</DropdownMenuLabel>
            {DESKTOP_SIZES.map(preset => (
              <DropdownMenuRadioItem key={sizeLabel(preset)} value={sizeLabel(preset)} disabled={disabled} onSelect={() => resize(preset)}>
                {sizeLabel(preset)}
              </DropdownMenuRadioItem>
            ))}
          </DropdownMenuRadioGroup>
          {(["browser.viewportPortrait", "browser.viewportLandscape"] as const).map(orientation => (
            <React.Fragment key={orientation}>
              <DropdownMenuSeparator />
              <DropdownMenuRadioGroup value={currentSize} aria-label={t(orientation)}>
                <DropdownMenuLabel>{t(orientation)}</DropdownMenuLabel>
                {PORTRAIT_SIZES.map(({ device, width, height }) => {
                  // Rotate the same device size without defining a second preset.
                  // 旋转同一设备尺寸，避免另行定义一套预设值。
                  const preset = orientation === "browser.viewportPortrait" ? { width, height } : { width: height, height: width };
                  return (
                    <DropdownMenuRadioItem key={device} value={sizeLabel(preset)} disabled={disabled} onSelect={() => resize(preset)}>
                      {device}<DropdownMenuShortcut>{sizeLabel(preset)}</DropdownMenuShortcut>
                    </DropdownMenuRadioItem>
                  );
                })}
              </DropdownMenuRadioGroup>
            </React.Fragment>
          ))}
        </DropdownMenuContent>
      </DropdownMenu>
      <Dialog open={customOpen} onOpenChange={setCustomOpen}>
        <DialogContent onCloseAutoFocus={event => { event.preventDefault(); trigger.current?.focus(); }}>
          <DialogHeader>
            <DialogTitle>{t("browser.customViewport")}</DialogTitle>
            <DialogDescription>{formatMessage(t("browser.viewportRange"), { max: BROWSER_HOST_MAX_VIEWPORT_DIMENSION })}</DialogDescription>
          </DialogHeader>
          <form onSubmit={submitCustom}>
            <div className="vk-browser-custom-size">
              <label>
                {t("browser.viewportWidth")}
                <input className="vk-browser-address" name="width" type="number" inputMode="numeric"
                  required min={1} max={BROWSER_HOST_MAX_VIEWPORT_DIMENSION} step={1} defaultValue={size?.width} />
              </label>
              <label>
                {t("browser.viewportHeight")}
                <input className="vk-browser-address" name="height" type="number" inputMode="numeric"
                  required min={1} max={BROWSER_HOST_MAX_VIEWPORT_DIMENSION} step={1} defaultValue={size?.height} />
              </label>
            </div>
            <DialogFooter>
              <DialogClose asChild><Button type="button" variant="outline">{t("browser.cancel")}</Button></DialogClose>
              <Button type="submit" disabled={disabled}>{t("browser.applyViewport")}</Button>
            </DialogFooter>
          </form>
        </DialogContent>
      </Dialog>
    </>
  );
}
