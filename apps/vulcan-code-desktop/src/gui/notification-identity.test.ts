import assert from 'node:assert/strict';
import test from 'node:test';
import {
  WINDOWS_DESKTOP_APP_ID,
  WINDOWS_TOAST_ACTIVATOR_CLSID,
  resolveGuiNotificationIdentity,
} from './notification-identity-values.js';

test('installed release keeps the stable product notification identity', () => {
  // identity is the stable release contract independent of path casing.
  // identity 是不受路径大小写影响的稳定正式版契约。
  const identity = resolveGuiNotificationIdentity(
    'C:\\Program Files\\Vulcan Code\\VulcanCodeDesktop.EXE',
    'win32',
  );
  assert.deepEqual(identity, {
    name: 'Vulcan Code Desktop',
    appUserModelId: WINDOWS_DESKTOP_APP_ID,
    toastActivatorClsid: WINDOWS_TOAST_ACTIVATOR_CLSID,
  });
});

test('debug identity is deterministic and Windows-path case insensitive', () => {
  // first and second represent the same executable through equivalent Windows paths.
  // first 与 second 通过等价的 Windows 路径表示同一可执行程序。
  const first = resolveGuiNotificationIdentity(
    'D:\\openvulcan\\vulcan-code\\output\\desktop\\runtime\\VulcanCodeDesktopDebug.exe',
    'win32',
  );
  const second = resolveGuiNotificationIdentity(
    '\\\\?\\d:\\OPENVULCAN\\vulcan-code\\output\\desktop\\runtime\\VulcanCodeDesktopDebug.exe',
    'win32',
  );
  assert.deepEqual(first, second);
  assert.match(first.name, /^Vulcan Code Desktop Debug [0-9a-f]{12}$/);
  assert.match(first.appUserModelId ?? '', /^ai\.vulcan\.code\.desktop\.debug\.[0-9a-f]{12}$/);
  assert.match(
    first.toastActivatorClsid ?? '',
    /^\{[0-9A-F]{8}-[0-9A-F]{4}-5[0-9A-F]{3}-[89AB][0-9A-F]{3}-[0-9A-F]{12}\}$/,
  );
});

test('different debug executable paths cannot claim the same Windows notification identity', () => {
  // outputIdentity is owned by the executable launched from the runtime output tree.
  // outputIdentity 归运行时输出目录启动的可执行程序所有。
  const outputIdentity = resolveGuiNotificationIdentity(
    'D:\\openvulcan\\vulcan-code\\output\\desktop\\runtime\\VulcanCodeDesktopDebug.exe',
    'win32',
  );
  // targetIdentity is owned by a physically separate executable in the target tree.
  // targetIdentity 归 target 目录中的另一个物理可执行程序所有。
  const targetIdentity = resolveGuiNotificationIdentity(
    'D:\\openvulcan\\vulcan-code\\target\\w11-debug-distribution-current\\desktop\\runtime\\VulcanCodeDesktopDebug.exe',
    'win32',
  );
  assert.notEqual(outputIdentity.name, targetIdentity.name);
  assert.notEqual(outputIdentity.appUserModelId, targetIdentity.appUserModelId);
  assert.notEqual(outputIdentity.toastActivatorClsid, targetIdentity.toastActivatorClsid);
});

test('non-Windows hosts do not receive Windows notification registration values', () => {
  // identity preserves the cross-platform product name without Windows-only registration.
  // identity 保留跨平台产品名，并且不携带 Windows 专属注册值。
  const identity = resolveGuiNotificationIdentity('/opt/vulcan/VulcanCodeDesktop', 'linux');
  assert.deepEqual(identity, {
    name: 'Vulcan Code Desktop',
    appUserModelId: null,
    toastActivatorClsid: null,
  });
});
