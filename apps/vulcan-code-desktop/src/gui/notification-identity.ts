// Configure the desktop GUI's native notification identity before Electron becomes ready.
// 在 Electron 就绪前配置桌面 GUI 的原生通知身份。
import { app } from 'electron';
import path from 'node:path';
import { resolveGuiNotificationIdentity } from './notification-identity-values.js';

// configured prevents packaged and direct development entries from changing identity twice.
// configured 防止发行入口和直接开发入口重复修改应用身份。
let configured = false;

// canUseNativeNotifications reports whether this host has a program-owned Windows identity.
// canUseNativeNotifications 报告当前宿主是否拥有程序专属的 Windows 身份。
// The return value is false for generic source Electron on Windows and true elsewhere.
// 返回值在 Windows 通用源码 Electron 中为 false，在其他环境中为 true。
export function canUseNativeNotifications(): boolean {
  return process.platform !== 'win32' || path.basename(process.execPath).toLowerCase() !== 'electron.exe';
}

// configureGuiNotificationIdentity applies the process-owned GUI name, AUMID, and activator.
// configureGuiNotificationIdentity 应用进程专属的 GUI 名称、AUMID 和激活器。
// It has no parameters or return value and is idempotent within the process.
// 它没有参数和返回值，并且在进程内保持幂等。
export function configureGuiNotificationIdentity(): void {
  if (configured) return;
  // identity is resolved from the exact executable that will receive native activation.
  // identity 根据将接收原生激活的精确可执行程序解析。
  const identity = resolveGuiNotificationIdentity(process.execPath, process.platform);
  app.setName(identity.name);
  if (identity.appUserModelId !== null && identity.toastActivatorClsid !== null) {
    app.setAppUserModelId(identity.appUserModelId);
    app.setToastActivatorCLSID(identity.toastActivatorClsid);
  }
  configured = true;
}
