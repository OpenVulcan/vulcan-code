// Configure the desktop GUI's stable native notification identity before Electron becomes ready.
// 在 Electron 就绪前配置桌面 GUI 稳定的原生通知身份。
import { app } from 'electron';
import path from 'node:path';

// The GUI identity is distinct from the service tray's existing application identity.
// GUI 身份与服务托盘已有的应用身份分离。
export const WINDOWS_DESKTOP_APP_ID = 'ai.vulcan.code.desktop';
// This fixed GUID lets Windows route a toast activation to the same GUI after a restart.
// 此固定 GUID 使 Windows 在重启后仍能将通知激活送回同一 GUI。
export const WINDOWS_TOAST_ACTIVATOR_CLSID = '{AB3EF79F-A068-4BA3-9B40-814138499202}';
// Development uses a separate identity so opening its GUI cannot redirect release toasts.
// 开发版使用独立身份，避免打开开发 GUI 后把正式版通知激活重定向到开发程序。
export const WINDOWS_DEBUG_DESKTOP_APP_ID = 'ai.vulcan.code.desktop.debug';
// This GUID is stable across development restarts but distinct from the release activator.
// 此 GUID 在开发版重启间保持稳定，同时区别于正式版激活器。
export const WINDOWS_DEBUG_TOAST_ACTIVATOR_CLSID = '{3A5A6297-FA5F-4789-83BC-1520F0D103A7}';

// configured prevents the packaged entry and the direct development entry from changing identity twice.
// configured 防止发行入口与直接开发入口重复更改应用身份。
let configured = false;

// Source Electron shares the generic Electron.lnk name with other development launches.
// 源码版 Electron 与其他开发启动共用 Electron.lnk 名称。
// Electron 44 adopts that shortcut's existing CLSID when the notification presenter starts,
// even when it differs from the CLSID explicitly configured for this process. A branded
// debug or release executable has its own product-named shortcut and may use native toasts.
// Electron 44 启动通知呈现器时会采用该快捷方式已有的 CLSID，即使它不同于本进程显式配置的值；
// 品牌化调试或正式版可执行文件拥有独立产品名快捷方式，可以使用原生通知。
export function canUseNativeNotifications(): boolean {
  return process.platform !== 'win32' || path.basename(process.execPath).toLowerCase() !== 'electron.exe';
}

// configureGuiNotificationIdentity sets the GUI name, AUMID and toast activator before native registration.
// configureGuiNotificationIdentity 在原生注册前设置 GUI 名称、应用 ID 与通知激活器。
// It has no parameters or return value and does not register a shortcut until a notification is used.
// 它没有参数和返回值，且不会在实际使用通知前注册快捷方式。
export function configureGuiNotificationIdentity(): void {
  if (configured) return;
  // The packaged Windows executable name is fixed by the validated runtime layout. All other
  // Windows executables, including source Electron and the debug package, use a separate identity.
  // Windows 正式版可执行文件名由运行时布局固定；源码 Electron 和调试包等其他程序使用独立身份。
  const release = process.platform !== 'win32'
    || path.basename(process.execPath).toLowerCase() === 'vulcancodedesktop.exe';
  app.setName(release ? 'Vulcan Code Desktop' : 'Vulcan Code Desktop Debug');
  if (process.platform === 'win32') {
    app.setAppUserModelId(release ? WINDOWS_DESKTOP_APP_ID : WINDOWS_DEBUG_DESKTOP_APP_ID);
    app.setToastActivatorCLSID(release
      ? WINDOWS_TOAST_ACTIVATOR_CLSID : WINDOWS_DEBUG_TOAST_ACTIVATOR_CLSID);
  }
  configured = true;
}
