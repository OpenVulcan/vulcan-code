// Pin Playwright's component registry to the copy shipped inside the host package.
// 将 Playwright 组件注册表固定为宿主包内随附的副本。
import process from 'node:process';

// Playwright reads this once when playwright-core loads, so main.ts imports this module first.
// "0" is Playwright's hermetic mode: components live in playwright-core/.local-browsers, where
// packaging places ffmpeg (video recording) and, on Windows, winldd. Overriding any inherited
// value keeps a user's own Playwright cache from substituting other component revisions.
// Playwright 在加载 playwright-core 时只读取一次该变量，因此 main.ts 最先导入本模块。
// "0" 是 Playwright 的自包含模式：组件位于 playwright-core/.local-browsers，打包时在此放入
// ffmpeg（录屏）以及 Windows 上的 winldd。覆盖继承的取值，避免用户自己的 Playwright 缓存
// 替换为其他组件版本。
process.env.PLAYWRIGHT_BROWSERS_PATH = '0';
