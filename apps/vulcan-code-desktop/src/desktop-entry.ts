// Route one packaged Electron runtime to either the GUI or the Core browser host.
// 将同一打包 Electron 运行时路由到 GUI 或 Core 浏览器宿主。

// A packaged Electron executable loads its app main regardless of a script path argument.
// 打包 Electron 可执行文件会加载应用主入口，而不会因为脚本路径参数切换入口。
import { configureGuiNotificationIdentity } from './gui/notification-identity.js';

if (process.argv.includes('--vulcan-browser-host')) {
  await import('./browser-host/main.js');
} else {
  // Configure before the dynamic GUI import yields and Electron initializes notifications.
  // 在动态导入 GUI 让出执行权、Electron 初始化通知之前配置身份。
  configureGuiNotificationIdentity();
  await import('./gui/main.js');
}
