// Application icon of the desktop windows and the macOS Dock.
// 桌面窗口与 macOS 程序坞使用的应用图标。
//
// Windows takes the icon from the branded executable's resources. Linux and macOS run the stock
// Electron binary, so without this the Linux dock shows a generic icon for the window and the macOS
// Dock shows Electron's own icon.
// Windows 从品牌化可执行文件的资源中取得图标。Linux 与 macOS 运行原版 Electron 程序，若不设置，
// Linux 程序坞会为窗口显示通用图标，macOS 程序坞会显示 Electron 自带的图标。

import { app, nativeImage, type NativeImage } from 'electron';
import { existsSync } from 'node:fs';
import path from 'node:path';

// APP_ICON_FILE is the icon `scripts/desktop-runtime.mjs` copies beside `gui/` in the packaged app.
// APP_ICON_FILE 是 `scripts/desktop-runtime.mjs` 复制到打包应用中 `gui/` 旁的图标。
export const APP_ICON_FILE = 'icon.png';

// MACOS_ICON_FILE is the macOS variant copied beside it: the artwork sits on Apple's icon grid (824 of
// 1024 pixels with a transparent margin and the system corner radius), so the Dock and the launcher
// bundle (`apps/vulcan-code-setup`) show it at the size of other applications' icons. The full-bleed
// icon would fill the whole Dock tile while running and be shrunk into the system shape while closed.
// MACOS_ICON_FILE 是复制到其旁的 macOS 版本：图形位于 Apple 图标网格上（1024 像素中占 824，带透明边距与
// 系统圆角），使程序坞与启动器应用包（`apps/vulcan-code-setup`）以与其他应用图标相同的大小显示它。
// 铺满画布的图标在运行时会占满整个程序坞格子，关闭时又会被系统缩进标准形状中。
export const MACOS_ICON_FILE = 'icon-macos.png';

// appIconCandidates lists where the icon may be: beside `gui/` when packaged, then the repository
// source for a development build running from `apps/vulcan-code-desktop/dist/gui`.
// appIconCandidates 列出图标可能的位置：打包时位于 `gui/` 旁，其次是从
// `apps/vulcan-code-desktop/dist/gui` 运行的开发构建所用的仓库源文件。
// Parameters: guiDirectory is the directory of the compiled GUI modules; file is the icon file name.
// 参数：guiDirectory 为已编译 GUI 模块所在目录；file 为图标文件名。
export function appIconCandidates(guiDirectory: string, file: string = APP_ICON_FILE): string[] {
  return [
    path.join(guiDirectory, '..', file),
    path.join(guiDirectory, '..', '..', '..', 'vulcan-code-service', 'icons', file),
  ];
}

// loadedIcons caches the first readable icon per file name; null records that none was found.
// loadedIcons 按文件名缓存第一个可读取的图标；null 表示未找到。
const loadedIcons = new Map<string, NativeImage | null>();

// appIcon returns the icon of the given file name, or undefined when no such file can be read.
// appIcon 返回指定文件名的图标；无法读取该文件时返回 undefined。
export function appIcon(file: string = APP_ICON_FILE): NativeImage | undefined {
  if (!loadedIcons.has(file)) {
    const found = appIconCandidates(import.meta.dirname, file).find((candidate) => existsSync(candidate));
    const image = found ? nativeImage.createFromPath(found) : null;
    loadedIcons.set(file, image && !image.isEmpty() ? image : null);
  }
  return loadedIcons.get(file) ?? undefined;
}

// windowIconOptions returns the BrowserWindow icon option for Linux; other platforms take their
// icon from the executable or the application bundle.
// windowIconOptions 返回 Linux 上 BrowserWindow 的图标选项；其他平台从可执行文件或应用包取得图标。
export function windowIconOptions(): { icon?: NativeImage } {
  if (process.platform !== 'linux') return {};
  const icon = appIcon();
  return icon ? { icon } : {};
}

// applyDockIcon replaces Electron's icon in the macOS Dock with the macOS application icon; it does
// nothing on other platforms. Call it once the app is ready.
// applyDockIcon 将 macOS 程序坞中 Electron 的图标替换为 macOS 应用图标；其他平台不做任何事。应在应用就绪后调用。
export function applyDockIcon(): void {
  if (process.platform !== 'darwin') return;
  const icon = appIcon(MACOS_ICON_FILE);
  if (icon) app.dock?.setIcon(icon);
}
