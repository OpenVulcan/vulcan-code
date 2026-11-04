// Windows installer script generation on synthetic layouts.
// 基于合成布局验证 Windows 安装程序脚本的生成。
import assert from 'node:assert/strict';
import test from 'node:test';
import { innoSignToolName, installLayout, installerAssetName, innoSetupScript } from './windows-installer.mjs';

// windowsLayout is the minimal release layout the installer reads.
// windowsLayout 是安装程序读取的最小正式版布局。
const windowsLayout = { platform: 'win32', architecture: 'x64', desktopExecutable: 'desktop/runtime/VulcanCodeDesktop.exe' };

// releaseIdentity is the shape the desktop notification identity resolver returns for the release executable.
// releaseIdentity 是桌面通知身份解析器为正式版可执行文件返回的形状。
const releaseIdentity = { name: 'Product Desktop', appUserModelId: 'example.product.desktop', toastActivatorClsid: '{AB3EF79F-A068-4BA3-9B40-814138499202}' };

// options builds one complete generation request around a layout.
// options 围绕一个布局构造完整的生成请求。
function options(layout = windowsLayout, overrides = {}) {
  return {
    root: 'D:\\dist', layout, version: '1.2.3', entries: ['bin', 'runtime-layout.json'],
    outputDir: 'D:\\out', icon: 'D:\\icon.ico', identity: releaseIdentity, ...overrides,
  };
}

// sectionLines returns the non-empty, non-comment lines of one INI section.
// sectionLines 返回一个 INI 节中非空且非注释的行。
function sectionLines(script, section) {
  const start = script.indexOf(`[${section}]\n`);
  assert.notEqual(start, -1, `missing [${section}]`);
  const body = script.slice(start + section.length + 3);
  const end = body.search(/^\[/m);
  return (end === -1 ? body : body.slice(0, end)).split('\n').filter(line => line && !line.startsWith(';'));
}

// parameters parses one `Key: "value"; Flags: a b` line into its parameters.
// parameters 将一行 `Key: "value"; Flags: a b` 解析为参数。
function parameters(line) {
  return Object.fromEntries(line.split(/;\s*/).map(part => {
    const colon = part.indexOf(':');
    return [part.slice(0, colon).trim(), part.slice(colon + 1).trim().replace(/^"(.*)"$/, '$1')];
  }));
}

test('installer name is the one the release manifest accepts for its inno download', () => {
  assert.equal(installerAssetName(windowsLayout, '1.2.3'), 'VulcanCode-1.2.3-windows-x64-setup');
});

test('only a signed build names the sign tool', () => {
  assert.ok(!sectionLines(innoSetupScript(options()), 'Setup').some(line => line.startsWith('SignTool=')));
  assert.ok(sectionLines(innoSetupScript(options(windowsLayout, { signed: true })), 'Setup').includes(`SignTool=${innoSignToolName}`));
});

test('the release tree is installed as its own version directory beside the launchers', () => {
  const files = sectionLines(innoSetupScript(options()), 'Files').map(parameters);
  assert.ok(files.some(file => file.Source === 'D:\\dist\\*' && file.DestDir === '{app}\\versions\\1.2.3'));
  const destinations = files.filter(file => file.DestName).map(file => `${file.DestDir}\\${file.DestName}`);
  assert.deepEqual(destinations.sort(), [`{app}\\${installLayout.cli}\\${installLayout.forwarder}`, `{app}\\${installLayout.launcher}`].sort());
  for (const file of files.filter(file => file.DestName)) assert.ok(file.Source.startsWith('D:\\dist\\bin\\'));
});

test('installing over the single-directory layout removes exactly its top-level release entries', () => {
  const removed = sectionLines(innoSetupScript(options()), 'InstallDelete').map(parameters).map(entry => entry.Name);
  assert.ok(removed.includes('{app}\\bin'));
  assert.ok(removed.includes('{app}\\runtime-layout.json'));
  assert.ok(removed.includes('{app}\\versions\\1.2.3'));
  assert.ok(!removed.includes('{app}') && !removed.includes('{app}\\versions'));
});

test('uninstall removes the launchers, versions, updates and pointer but no user data', () => {
  const removed = sectionLines(innoSetupScript(options()), 'UninstallDelete').map(parameters).map(entry => entry.Name);
  for (const name of [installLayout.versions, installLayout.updates, installLayout.cli, installLayout.current]) {
    assert.ok(removed.includes(`{app}\\${name}`), name);
  }
  assert.ok(removed.every(name => name.startsWith('{app}\\') || name.startsWith('{userprograms}\\')));
});

test('the desktop shortcut and PATH tasks are selected by default', () => {
  const tasks = sectionLines(innoSetupScript(options()), 'Tasks').map(parameters);
  assert.deepEqual(tasks.map(task => task.Name).sort(), ['addtopath', 'desktopicon']);
  assert.ok(tasks.every(task => !(task.Flags ?? '').includes('unchecked')));
  assert.ok(sectionLines(innoSetupScript(options()), 'Setup').includes('ChangesEnvironment=yes'));
});

test('shortcuts start the launcher under the desktop AppUserModelID', () => {
  const icons = sectionLines(innoSetupScript(options()), 'Icons').map(parameters);
  assert.equal(icons.length, 2);
  for (const icon of icons) {
    assert.equal(icon.Filename, `{app}\\${installLayout.launcher}`);
    assert.equal(icon.AppUserModelID, releaseIdentity.appUserModelId);
    assert.equal(icon.AppUserModelToastActivatorCLSID, 'AB3EF79F-A068-4BA3-9B40-814138499202');
  }
});

test('the pointer written after install names the installed version directory', () => {
  const match = /CurrentJson = '(.*)';/.exec(innoSetupScript(options()));
  assert.ok(match);
  assert.deepEqual(JSON.parse(match[1]), { version: '1.2.3', path: 'versions\\1.2.3' });
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

test('a distribution entry that collides with the install layout is rejected', () => {
  for (const entry of ['versions', 'CLI', 'current.json', 'VulcanCode.exe', 'unins000.exe']) {
    assert.throws(() => innoSetupScript(options(windowsLayout, { entries: ['bin', entry] })), entry);
  }
});

test('a debug or missing desktop identity cannot label the shortcuts', () => {
  for (const identity of [undefined, { ...releaseIdentity, appUserModelId: 'example.product.desktop.debug.0123456789ab' },
    { ...releaseIdentity, appUserModelId: null }, { ...releaseIdentity, toastActivatorClsid: 'not-a-clsid' }]) {
    assert.throws(() => innoSetupScript(options(windowsLayout, { identity })));
  }
});

test('values that would break out of a quoted script value are rejected', () => {
  assert.throws(() => innoSetupScript(options(windowsLayout, { root: 'D:\\dist"\n[Run]' })));
  assert.throws(() => innoSetupScript(options(windowsLayout, { entries: ['bad"name'] })));
  assert.throws(() => innoSetupScript(options(windowsLayout, { version: '1.2.3"' })));
});

test('an interactive install offers a launch and a relaunch-requesting silent install starts the app', () => {
  const run = sectionLines(innoSetupScript(options()), 'Run').map(parameters);
  const launcher = `{app}\\${installLayout.launcher}`;
  const offered = run.find(entry => entry.Flags.split(' ').includes('postinstall'));
  assert.equal(offered.Filename, launcher);
  assert.ok(offered.Flags.split(' ').includes('skipifsilent'));
  const relaunch = run.find(entry => entry.Flags.split(' ').includes('skipifnotsilent'));
  assert.equal(relaunch.Filename, launcher);
  assert.equal(relaunch.Check, 'RelaunchRequested');
  assert.match(innoSetupScript(options()), /function RelaunchRequested: Boolean;/);
});
