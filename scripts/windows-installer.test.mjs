// Windows installer script generation on synthetic layouts.
// 基于合成布局验证 Windows 安装程序脚本的生成。
import assert from 'node:assert/strict';
import test from 'node:test';
import { installerAssetName, innoSetupScript } from './windows-installer.mjs';

// windowsLayout is the minimal release layout the installer reads.
// windowsLayout 是安装程序读取的最小正式版布局。
const windowsLayout = { platform: 'win32', architecture: 'x64', desktopExecutable: 'desktop/runtime/VulcanCodeDesktop.exe' };

// options builds one complete generation request around a layout.
// options 围绕一个布局构造完整的生成请求。
function options(layout = windowsLayout, overrides = {}) {
  return {
    root: 'D:\\dist', layout, version: '1.2.3', entries: ['bin', 'runtime-layout.json'],
    outputDir: 'D:\\out', icon: 'D:\\icon.ico', ...overrides,
  };
}

// sectionLines returns the non-empty lines of one INI section.
// sectionLines 返回一个 INI 节中的非空行。
function sectionLines(script, section) {
  const start = script.indexOf(`[${section}]\n`);
  assert.notEqual(start, -1, `missing [${section}]`);
  const body = script.slice(start + section.length + 3);
  const end = body.search(/^\[/m);
  return (end === -1 ? body : body.slice(0, end)).split('\n').filter(Boolean);
}

test('installer name follows the release asset naming for the same version and platform', () => {
  assert.equal(installerAssetName(windowsLayout, '1.2.3'), 'vulcan-code-1.2.3-windows-x64-setup');
});

test('upgrade and uninstall clear exactly the distribution top-level entries', () => {
  const script = innoSetupScript(options());
  const expected = ['Type: filesandordirs; Name: "{app}\\bin"', 'Type: filesandordirs; Name: "{app}\\runtime-layout.json"'];
  assert.deepEqual(sectionLines(script, 'InstallDelete'), expected);
  assert.deepEqual(sectionLines(script, 'UninstallDelete'), expected);
});

test('installation is per user and never requests elevation', () => {
  const setup = sectionLines(innoSetupScript(options()), 'Setup');
  assert.ok(setup.includes('PrivilegesRequired=lowest'));
  assert.ok(setup.includes('DefaultDirName={autopf}\\VulcanCode'));
});

test('only a Windows x64 layout can be packaged', () => {
  assert.throws(() => innoSetupScript(options({ ...windowsLayout, architecture: 'arm64' })));
  assert.throws(() => innoSetupScript(options({ ...windowsLayout, platform: 'linux' })));
});

test('values that would break out of a quoted script value are rejected', () => {
  assert.throws(() => innoSetupScript(options(windowsLayout, { root: 'D:\\dist"\n[Run]' })));
  assert.throws(() => innoSetupScript(options(windowsLayout, { entries: ['bad"name'] })));
});
