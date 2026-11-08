// Install the desktop app's Electron binary for the architecture of this Node process. The archive is first put into
// the @electron/get cache through fetchDownload, so a github.com release download comes from the GitHub API asset
// endpoint and transient failures repeat; Electron's own install.js then finds it there, verifies it against the
// package checksums and extracts it.
// 为本 Node 进程的架构安装桌面应用的 Electron 二进制。归档先经 fetchDownload 放入 @electron/get 缓存，因此
// github.com 发布下载取自 GitHub API 资产端点并在暂时性失败时重试；随后 Electron 自带的 install.js 从缓存取得归档，
// 按包内摘要校验并解压。
//
//   node scripts/install-electron.mjs
import { spawnSync } from 'node:child_process';
import { createWriteStream, readFileSync, realpathSync } from 'node:fs';
import { createRequire } from 'node:module';
import path from 'node:path';
import { Readable } from 'node:stream';
import { pipeline } from 'node:stream/promises';
import { fileURLToPath } from 'node:url';
import { fetchDownload } from './locked-download.mjs';

// electronRoot is the electron package installed for the desktop app, resolved through pnpm's link so its own
// dependencies (@electron/get) resolve beside it.
// electronRoot 是为桌面应用安装的 electron 包，经 pnpm 链接解析为真实路径，使其依赖（@electron/get）在其旁边解析。
const electronRoot = realpathSync(fileURLToPath(new URL('../apps/vulcan-code-desktop/node_modules/electron/', import.meta.url)));
const require = createRequire(path.join(electronRoot, 'package.json'));
const { version } = require('./package.json');

// The architecture is stated explicitly: under Rosetta, install.js would otherwise pick arm64 for an x64 build.
// 架构显式指定：否则在 Rosetta 下 install.js 会为 x64 构建选择 arm64。
const environment = { ...process.env, ELECTRON_INSTALL_ARCH: process.arch, ELECTRON_INSTALL_PLATFORM: process.platform };

// installedVersion is the version install.js last extracted, if any; a matching one needs no archive.
// installedVersion 是 install.js 上次解压的版本（如有）；版本一致时无需归档。
let installedVersion = '';
try {
  installedVersion = readFileSync(path.join(electronRoot, 'dist', 'version'), 'utf8').trim().replace(/^v/, '');
} catch (error) {
  if (error.code !== 'ENOENT') throw error;
}
if (installedVersion !== version) {
  const { downloadArtifact } = require('@electron/get');
  // The same details as install.js, so the cached archive is the one it looks up.
  // 与 install.js 相同的参数，使缓存的归档正是其查找的那一个。
  await downloadArtifact({
    version,
    artifactName: 'electron',
    cacheRoot: process.env.electron_config_cache,
    checksums: require('./checksums.json'),
    platform: process.platform,
    arch: process.arch,
    downloader: {
      async download(url, targetFilePath) {
        const response = await fetchDownload(url);
        if (!response.ok) throw new Error(`Electron download failed (${response.status}): ${url}`);
        await pipeline(Readable.fromWeb(response.body), createWriteStream(targetFilePath));
      },
    },
  });
}
const result = spawnSync(process.execPath, [path.join(electronRoot, 'install.js')], { stdio: 'inherit', env: environment });
process.exit(result.status ?? 1);
