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

// resolveInstalledLauncher returns the launcher of the install whose versions directory holds executablePath.
// resolveInstalledLauncher 返回其 versions 目录包含 executablePath 的安装的启动器。
// executablePath is the physical desktop executable; exists reports whether a file exists; the return value is
// null outside an installed layout, for example in the repository output trees.
// executablePath 是桌面可执行程序的物理路径；exists 报告文件是否存在；不在安装布局中时（例如仓库输出目录）返回 null。
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
