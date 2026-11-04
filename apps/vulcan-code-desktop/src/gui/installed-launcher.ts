// Locate the stable launcher of a multi-version Windows install without side effects.
// 无副作用地定位多版本 Windows 安装中的稳定启动器。
import path from 'node:path';

// The install layout is written by scripts/windows-installer.mjs (`installLayout`) and read by the
// launcher in apps/vulcan-code-launcher:
//   <root>\VulcanCode.exe, <root>\current.json, <root>\versions\<version>\desktop\runtime\VulcanCodeDesktop.exe
// 安装布局由 scripts/windows-installer.mjs（`installLayout`）写入，并由 apps/vulcan-code-launcher 中的启动器读取。

// INSTALL_LAUNCHER_NAME is the launcher every shortcut and taskbar pin starts.
// INSTALL_LAUNCHER_NAME 是所有快捷方式与任务栏固定项启动的启动器。
export const INSTALL_LAUNCHER_NAME = 'VulcanCode.exe';

// INSTALL_SHORTCUT_NAME is the display name of the installer's shortcuts, reused for taskbar pins.
// INSTALL_SHORTCUT_NAME 是安装程序快捷方式的显示名称，任务栏固定项沿用该名称。
export const INSTALL_SHORTCUT_NAME = 'Vulcan Code';

// VERSIONS_DIRECTORY holds one release tree per installed version below the install root.
// VERSIONS_DIRECTORY 位于安装根目录下，每个已安装版本各占一个发行树。
const VERSIONS_DIRECTORY = 'versions';

// CURRENT_FILE marks a directory as an install root.
// CURRENT_FILE 标记一个目录为安装根目录。
const CURRENT_FILE = 'current.json';

// CURRENT_LINK is the link to the current version on platforms other than Windows (vulcan-install-core `CURRENT_LINK`).
// CURRENT_LINK 是 Windows 以外平台上指向当前版本的链接（vulcan-install-core `CURRENT_LINK`）。
const CURRENT_LINK = 'current';

// resolveInstalledLauncher returns the launcher of the install whose versions directory holds executablePath.
// resolveInstalledLauncher 返回其 versions 目录包含 executablePath 的安装的启动器。
// executablePath is the physical desktop executable; exists reports whether a file exists; the return value is
// null outside an installed layout, for example in the repository output trees.
// executablePath 是桌面可执行程序的物理路径；exists 报告文件是否存在；不在安装布局中时（例如仓库输出目录）返回 null。
// resolveStableDesktopEntry returns the path that starts the current installed version of this
// client: the launcher on Windows, `<root>/current/<same relative path>` elsewhere; null outside an
// installed layout.
// resolveStableDesktopEntry 返回启动本客户端当前已安装版本的路径：Windows 上为启动器，其他平台为
// `<root>/current/<相同相对路径>`；在已安装布局之外为 null。
// Parameters: executablePath is the running executable, platform its operating system, exists a file check.
// 参数：executablePath 为正在运行的可执行文件，platform 为其操作系统，exists 为文件检查。
export function resolveStableDesktopEntry(
  executablePath: string,
  platform: NodeJS.Platform,
  exists: (file: string) => boolean,
): string | null {
  if (platform === 'win32') return resolveInstalledLauncher(executablePath, exists);
  let directory = path.posix.dirname(executablePath);
  for (;;) {
    const parent = path.posix.dirname(directory);
    if (parent === directory) return null;
    if (path.posix.basename(parent) === VERSIONS_DIRECTORY) {
      const root = path.posix.dirname(parent);
      const current = path.posix.join(root, CURRENT_LINK);
      if (exists(current)) return path.posix.join(current, path.posix.relative(directory, executablePath));
    }
    directory = parent;
  }
}

export function resolveInstalledLauncher(
  executablePath: string,
  exists: (file: string) => boolean,
): string | null {
  let directory = path.win32.dirname(executablePath);
  for (;;) {
    const parent = path.win32.dirname(directory);
    if (parent === directory) return null;
    if (path.win32.basename(parent).toLowerCase() === VERSIONS_DIRECTORY) {
      const root = path.win32.dirname(parent);
      const launcher = path.win32.join(root, INSTALL_LAUNCHER_NAME);
      if (exists(path.win32.join(root, CURRENT_FILE)) && exists(launcher)) return launcher;
    }
    directory = parent;
  }
}
