// Share the shipped Cargo package and executable inventory with Unix builds and publication.
// 与 Unix 构建和发布流程共用分发 Cargo 包及可执行文件清单。
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import path from 'node:path';

// shippedRustPackages is also consumed by the PowerShell build and third-party notice inventory.
// shippedRustPackages 同时供 PowerShell 构建和第三方声明清单使用。
export const shippedRustPackages = JSON.parse(readFileSync(new URL('./shipped-rust-packages.json', import.meta.url), 'utf8'));

// shippedBinaryNames returns all required executable names for platform; package/bin aliases come from Cargo declarations.
// shippedBinaryNames 返回 platform 对应的全部必需可执行文件名；包与二进制别名来自 Cargo 声明。
export function shippedBinaryNames(platform = process.platform) {
  return shippedRustPackages.map(item => item.binary + (platform === 'win32' ? '.exe' : ''));
}

// rootWorkspacePackages lists the packages the root Cargo workspace builds; an item with a `workspace`
// lives in its own Cargo workspace (such as the code-mode host) and is built by that workspace's helper.
// rootWorkspacePackages 列出根 Cargo 工作区构建的包；带 `workspace` 的项位于独立 Cargo 工作区
// （如代码模式宿主），由该工作区的辅助脚本构建。
export const rootWorkspacePackages = shippedRustPackages.filter(item => item.workspace === undefined);

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  if (process.argv[2] !== 'cargo-args' || process.argv.length !== 3) {
    throw new Error('Usage: shipped-rust-packages cargo-args');
  }
  process.stdout.write(rootWorkspacePackages.flatMap(item => ['-p', item.package]).join(' '));
}
