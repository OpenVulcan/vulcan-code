// Derive side-effect-free desktop notification identities from executable ownership.
// 根据可执行程序所有权派生无副作用的桌面通知身份。
import { createHash } from 'node:crypto';
import path from 'node:path';

// WINDOWS_DESKTOP_APP_ID is the stable installed release identity.
// WINDOWS_DESKTOP_APP_ID 是已安装正式版的稳定身份。
export const WINDOWS_DESKTOP_APP_ID = 'ai.vulcan.code.desktop';

// WINDOWS_TOAST_ACTIVATOR_CLSID routes installed release activations after a restart.
// WINDOWS_TOAST_ACTIVATOR_CLSID 在正式版重启后路由通知激活。
export const WINDOWS_TOAST_ACTIVATOR_CLSID = '{AB3EF79F-A068-4BA3-9B40-814138499202}';

// WINDOWS_RELEASE_EXECUTABLE_NAME is the validated installed GUI executable name.
// WINDOWS_RELEASE_EXECUTABLE_NAME 是经过验证的已安装 GUI 可执行文件名。
const WINDOWS_RELEASE_EXECUTABLE_NAME = 'vulcancodedesktop.exe';

// DEBUG_IDENTITY_HASH_LENGTH keeps the debug shortcut name readable while remaining path-specific.
// DEBUG_IDENTITY_HASH_LENGTH 在保持调试快捷方式名称可读的同时确保其与路径绑定。
const DEBUG_IDENTITY_HASH_LENGTH = 12;

// GuiNotificationIdentity contains the exact native identity configured for one executable.
// GuiNotificationIdentity 包含为单个可执行程序配置的精确原生身份。
export interface GuiNotificationIdentity {
  // name is the Electron application and Windows shortcut name.
  // name 是 Electron 应用名称和 Windows 快捷方式名称。
  name: string;
  // appUserModelId is the Windows application user model identifier.
  // appUserModelId 是 Windows 应用用户模型标识符。
  appUserModelId: string | null;
  // toastActivatorClsid is the Windows COM toast activation identifier.
  // toastActivatorClsid 是 Windows COM 通知激活标识符。
  toastActivatorClsid: string | null;
}

// normalizeExecutablePath produces one case-insensitive Windows identity input.
// normalizeExecutablePath 生成不区分大小写的 Windows 身份输入。
// executablePath is the physical process path; the return value is stable across path casing.
// executablePath 是进程物理路径；返回值在路径大小写变化时保持稳定。
function normalizeExecutablePath(executablePath: string): string {
  // withoutVerbatimPrefix removes the Windows API-only prefix before ordinary path normalization.
  // withoutVerbatimPrefix 在常规路径规范化前移除仅供 Windows API 使用的前缀。
  const withoutVerbatimPrefix = executablePath.startsWith('\\\\?\\')
    ? executablePath.slice(4)
    : executablePath;
  return path.win32.normalize(path.win32.resolve(withoutVerbatimPrefix)).toLowerCase();
}

// createDebugIdentityHash derives deterministic identity material from the executable path.
// createDebugIdentityHash 从可执行程序路径派生确定性的身份材料。
// executablePath is normalized as a Windows path; the return value is a SHA-256 hex digest.
// executablePath 按 Windows 路径规范化；返回值是 SHA-256 十六进制摘要。
function createDebugIdentityHash(executablePath: string): string {
  return createHash('sha256').update(normalizeExecutablePath(executablePath), 'utf8').digest('hex');
}

// createGuidFromHash formats the first 128 hash bits as an RFC 4122 version-5-style GUID.
// createGuidFromHash 将摘要前 128 位格式化为 RFC 4122 第 5 版样式的 GUID。
// hash is a lowercase SHA-256 hex digest; the return value includes Windows GUID braces.
// hash 是小写 SHA-256 十六进制摘要；返回值包含 Windows GUID 花括号。
function createGuidFromHash(hash: string): string {
  // variantNibble applies the RFC 4122 variant bits while retaining two hash bits.
  // variantNibble 应用 RFC 4122 变体位，同时保留两位摘要信息。
  const variantNibble = ((Number.parseInt(hash.slice(16, 17), 16) & 0x3) | 0x8).toString(16);
  return (`{${hash.slice(0, 8)}-${hash.slice(8, 12)}-5${hash.slice(13, 16)}-`
    + `${variantNibble}${hash.slice(17, 20)}-${hash.slice(20, 32)}}`).toUpperCase();
}

// resolveGuiNotificationIdentity returns the native identity owned by an exact executable.
// resolveGuiNotificationIdentity 返回由精确可执行程序拥有的原生身份。
// executablePath and platform describe the current process; the return value is side-effect free.
// executablePath 与 platform 描述当前进程；返回值不产生副作用。
export function resolveGuiNotificationIdentity(
  executablePath: string,
  platform: NodeJS.Platform,
): GuiNotificationIdentity {
  if (platform !== 'win32') {
    return {
      name: 'Vulcan Code Desktop',
      appUserModelId: null,
      toastActivatorClsid: null,
    };
  }
  if (path.win32.basename(executablePath).toLowerCase() === WINDOWS_RELEASE_EXECUTABLE_NAME) {
    return {
      name: 'Vulcan Code Desktop',
      appUserModelId: WINDOWS_DESKTOP_APP_ID,
      toastActivatorClsid: WINDOWS_TOAST_ACTIVATOR_CLSID,
    };
  }
  // identityHash separates debug binaries located in output, target, and temporary probes.
  // identityHash 区分位于 output、target 和临时探针目录中的调试二进制程序。
  const identityHash = createDebugIdentityHash(executablePath);
  // identitySuffix is shared by the shortcut name and AUMID for diagnostic correlation.
  // identitySuffix 由快捷方式名称和 AUMID 共用，便于诊断关联。
  const identitySuffix = identityHash.slice(0, DEBUG_IDENTITY_HASH_LENGTH);
  return {
    name: `Vulcan Code Desktop Debug ${identitySuffix}`,
    appUserModelId: `ai.vulcan.code.desktop.debug.${identitySuffix}`,
    toastActivatorClsid: createGuidFromHash(identityHash),
  };
}
