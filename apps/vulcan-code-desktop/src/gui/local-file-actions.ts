// Local file actions open conversation file references with the OS: the default program, or the file manager.
// 本地文件操作通过操作系统打开会话中的文件引用：使用默认程序，或在文件管理器中显示。
import { stat } from 'node:fs/promises';
import path from 'node:path';

// Programs, scripts, installers and shortcuts run when the OS opens them, so a mistaken click on an AI-written link
// would execute them. They are only ever shown in their folder. This set is the only definition of that rule.
// 程序、脚本、安装包与快捷方式在操作系统打开时即会运行，误点 AI 写出的链接就会执行它们。此类文件只能在所在文件夹中
// 显示。本集合是该规则的唯一定义。
const RUNNABLE_FILE_EXTENSIONS: ReadonlySet<string> = new Set([
  // Windows programs, installers, shortcuts and system handlers.
  // Windows 程序、安装包、快捷方式与系统处理器。
  '.exe', '.com', '.scr', '.pif', '.cpl', '.msi', '.msp', '.mst', '.msc', '.msix', '.msixbundle', '.appx',
  '.appxbundle', '.appinstaller', '.application', '.appref-ms', '.gadget', '.lnk', '.url', '.website', '.reg',
  '.inf', '.scf', '.sct', '.hta', '.chm', '.settingcontent-ms', '.library-ms', '.diagcab', '.xll',
  // Windows script hosts and shells.
  // Windows 脚本宿主与命令行。
  '.bat', '.cmd', '.ps1', '.psm1', '.psd1', '.ps1xml', '.vb', '.vbs', '.vbe', '.js', '.jse', '.wsf', '.wsh', '.ws',
  '.ahk', '.au3',
  // Interpreted scripts that file associations run directly.
  // 文件关联会直接运行的解释型脚本。
  '.py', '.pyw', '.pyz', '.pl', '.rb', '.php', '.jar', '.sh', '.bash', '.zsh', '.fish', '.csh', '.ksh', '.command',
  '.tool',
  // macOS and Linux applications, installers and launchers.
  // macOS 与 Linux 应用、安装包与启动器。
  '.app', '.pkg', '.mpkg', '.dmg', '.workflow', '.terminal', '.desktop', '.appimage', '.run', '.deb', '.rpm',
  '.snap', '.flatpakref',
]);

// LocalFileInspection describes a file before the workbench offers actions for it.
// LocalFileInspection 描述工作台为文件提供操作之前的检查结果。
export interface LocalFileInspection {
  // Whether a file or folder exists at the path.
  // 路径处是否存在文件或文件夹。
  exists: boolean;
  // Whether the path names a regular file the default program may open; false for runnable files and folders.
  // 路径是否为可交给默认程序打开的普通文件；可运行文件与文件夹为 false。
  opensWithDefaultApp: boolean;
}

// requireAbsolutePath accepts only an absolute local path string and returns it unchanged.
// requireAbsolutePath 仅接受本地绝对路径字符串，并原样返回。
function requireAbsolutePath(value: unknown): string {
  if (typeof value !== 'string' || value.includes('\0') || !path.isAbsolute(value)) {
    throw new TypeError('A local file action needs an absolute file path.');
  }
  return value;
}

// isRunnableFilePath tells whether opening the path would run it. Windows ignores trailing dots and spaces and
// treats a later colon as a data stream, so both are stripped before the extension is read; on other systems an
// executable permission bit marks a runnable file whatever its name.
// isRunnableFilePath 判断打开该路径是否会运行它。Windows 忽略末尾的点与空格，并把后续冒号视为数据流，因此读取扩展名
// 前先去除二者；其他系统上，带执行权限位的文件无论名称如何都视为可运行。
export function isRunnableFilePath(filePath: string, mode: number, platform: NodeJS.Platform = process.platform): boolean {
  const windows = platform === 'win32';
  let name = (windows ? path.win32 : path.posix).basename(filePath);
  if (windows) name = name.replace(/:.*$/, '').replace(/[. ]+$/, '');
  if (RUNNABLE_FILE_EXTENSIONS.has((windows ? path.win32 : path.posix).extname(name).toLowerCase())) return true;
  return !windows && (mode & 0o111) !== 0;
}

// inspectLocalFile reports whether the path exists and whether the default program may open it.
// inspectLocalFile 报告路径是否存在，以及能否交给默认程序打开。
export async function inspectLocalFile(value: unknown): Promise<LocalFileInspection> {
  const filePath = requireAbsolutePath(value);
  const info = await stat(filePath).catch(() => null);
  if (!info) return { exists: false, opensWithDefaultApp: false };
  return { exists: true, opensWithDefaultApp: info.isFile() && !isRunnableFilePath(filePath, info.mode) };
}

// openLocalFileWithDefaultApp opens a regular, non-runnable file with its default program. `openPath` is the OS
// opener; it resolves with an error message, or an empty string on success.
// openLocalFileWithDefaultApp 用默认程序打开普通的非可运行文件。`openPath` 是操作系统打开器，失败时返回错误信息，
// 成功时返回空字符串。
export async function openLocalFileWithDefaultApp(value: unknown, openPath: (filePath: string) => Promise<string>): Promise<void> {
  const filePath = requireAbsolutePath(value);
  const inspection = await inspectLocalFile(filePath);
  if (!inspection.exists) throw new Error('The file no longer exists.');
  if (!inspection.opensWithDefaultApp) {
    throw new Error('Programs, scripts and folders are not opened from conversation links; show the file in its folder instead.');
  }
  const failure = await openPath(filePath);
  if (failure) throw new Error(`The default program could not open the file: ${failure}`);
}

// showLocalFileInFolder selects an existing file or folder in the system file manager. `showItemInFolder` is the
// OS file manager action.
// showLocalFileInFolder 在系统文件管理器中选中已有文件或文件夹。`showItemInFolder` 是操作系统文件管理器操作。
export async function showLocalFileInFolder(value: unknown, showItemInFolder: (filePath: string) => void): Promise<void> {
  const filePath = requireAbsolutePath(value);
  if (!(await inspectLocalFile(filePath)).exists) throw new Error('The file no longer exists.');
  showItemInFolder(filePath);
}
