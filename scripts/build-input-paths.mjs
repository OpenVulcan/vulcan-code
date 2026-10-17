// Keep downloaded build inputs outside Cargo artifacts and the published distribution.
// 将下载的构建输入保存在 Cargo 产物和发布目录之外。
import path from 'node:path';
import { fileURLToPath } from 'node:url';

// Resolve persistent input directories for repositoryRoot; return paths without creating or deleting files.
// 解析 repositoryRoot 的持久输入目录；返回路径，不创建或删除文件。
export function buildInputPaths(repositoryRoot) {
  // thirdParty is the checkout-owned storage boundary that cargo clean never owns.
  // thirdParty 是检出目录持有的存储边界，不属于 cargo clean 的管理范围。
  const thirdParty = path.resolve(repositoryRoot, 'third_party');
  return Object.freeze({
    lockedDownloads: path.join(thirdParty, 'build-input-cache'),
    browsers: path.join(thirdParty, 'browser-runtime'),
    electronDownloads: path.join(thirdParty, 'electron-runtime-cache'),
    managedNode: path.join(thirdParty, 'managed-node'),
  });
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  // The shared CLI supplies the same Electron cache to both shell and PowerShell installers.
  // 共享命令行入口为 Shell 与 PowerShell 安装器提供相同的 Electron 缓存。
  const repository = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
  if (process.argv.length !== 3 || process.argv[2] !== 'electron') throw new Error('Usage: node scripts/build-input-paths.mjs electron');
  console.log(buildInputPaths(repository).electronDownloads);
}
