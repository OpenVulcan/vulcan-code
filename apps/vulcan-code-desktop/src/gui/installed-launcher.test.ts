import assert from 'node:assert/strict';
import test from 'node:test';
import { INSTALL_LAUNCHER_NAME, resolveInstalledLauncher } from './installed-launcher.js';

// installedFiles models one install root with its pointer and launcher.
// installedFiles 模拟一个带有指针文件与启动器的安装根目录。
const installedFiles = new Set([
  'c:\\users\\me\\appdata\\local\\programs\\vulcancode\\current.json',
  `c:\\users\\me\\appdata\\local\\programs\\vulcancode\\${INSTALL_LAUNCHER_NAME.toLowerCase()}`,
]);

// exists matches the case-insensitive Windows file system.
// exists 匹配不区分大小写的 Windows 文件系统。
function exists(file: string): boolean {
  return installedFiles.has(file.toLowerCase());
}

test('a desktop executable inside a version directory resolves to the install launcher', () => {
  const launcher = resolveInstalledLauncher(
    'C:\\Users\\me\\AppData\\Local\\Programs\\VulcanCode\\versions\\0.1.3\\desktop\\runtime\\VulcanCodeDesktop.exe',
    exists,
  );
  assert.equal(launcher?.toLowerCase(), `c:\\users\\me\\appdata\\local\\programs\\vulcancode\\${INSTALL_LAUNCHER_NAME.toLowerCase()}`);
});

test('executables outside an installed layout have no launcher', () => {
  for (const executable of [
    'D:\\openvulcan\\vulcan-code\\output-release\\desktop\\runtime\\VulcanCodeDesktop.exe',
    'D:\\other\\versions\\0.1.3\\desktop\\runtime\\VulcanCodeDesktop.exe',
    'VulcanCodeDesktop.exe',
  ]) {
    assert.equal(resolveInstalledLauncher(executable, exists), null, executable);
  }
});
