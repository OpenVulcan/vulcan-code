// Runtime version reported by the browser host and the desktop client.
// 浏览器宿主与桌面客户端报告的运行时版本。

import process from 'node:process';

/** Runtime version string shared by the desktop client and the Core browser host. */
/** 桌面客户端与 Core 浏览器宿主共享的运行时版本文本。 */
export const BROWSER_HOST_RUNTIME_VERSION = '0.1.0';

/** Returns the Chromium version of the running Electron runtime when available. */
/** 可用时返回正在运行的 Electron 运行时的 Chromium 版本。 */
export function currentChromiumVersion(): string | null {
  // versions.chrome is defined by the Electron main process at runtime.
  // versions.chrome 由 Electron 主进程在运行时定义。
  const versions = (process as unknown as { versions?: Record<string, string> }).versions;
  return versions?.chrome ?? null;
}

/** Returns the Node.js version of the running Electron runtime when available. */
/** 可用时返回正在运行的 Electron 运行时的 Node.js 版本。 */
export function currentElectronNodeVersion(): string | null {
  const versions = (process as unknown as { versions?: Record<string, string> }).versions;
  return versions?.node ?? null;
}
