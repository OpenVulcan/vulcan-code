// Explicit installer for Core's pinned Chromium, isolated from generated desktop distributions.
// Core 固定版本 Chromium 的显式安装器，与生成的桌面发行目录隔离。
import { spawn } from 'node:child_process';
import { randomUUID } from 'node:crypto';
import { createRequire } from 'node:module';
import { access, mkdir, open, readFile, rename, rm, writeFile } from 'node:fs/promises';
import path from 'node:path';

// install receives one absolute Core-owned runtime root; publishes only after a real launch succeeds.
// install 接收一个 Core 持有的绝对运行目录；仅在实际启动验证成功后发布。
// Returns after writing the authoritative manifest; failures leave the previous installation selected.
// 写入权威清单后返回；失败时保留之前选中的安装。
async function install(): Promise<void> {
  // runtimeRoot is supplied by Core, never by an untrusted webpage.
  // runtimeRoot 由 Core 提供，绝不由不受信任的网页提供。
  const runtimeRoot = process.argv[2];
  if (!runtimeRoot || !path.isAbsolute(runtimeRoot)) throw new Error('An absolute Core browser runtime root is required.');
  await mkdir(runtimeRoot, { recursive: true });
  // lockPath serializes installers across Core restarts as well as concurrent clients.
  // lockPath 在 Core 重启及并发客户端之间串行化安装器。
  const lockPath = path.join(runtimeRoot, 'install.lock');
  try {
    // existing identifies a still-running installer; dead processes leave a recoverable lock.
    // existing 标识仍在运行的安装器；已退出进程留下的锁可恢复。
    const existing = Number(await readFile(lockPath, 'utf8'));
    if (!Number.isSafeInteger(existing) || existing <= 0) throw new Error('Browser installation lock is invalid.');
    try {
      process.kill(existing, 0);
      throw new Error('A browser installation is already running. Wait and refresh its status.');
    } catch (error) {
      if ((error as NodeJS.ErrnoException).code !== 'ESRCH') throw error;
      await rm(lockPath);
    }
  } catch (error) {
    if ((error as NodeJS.ErrnoException).code !== 'ENOENT') throw error;
  }
  // lock is acquired exclusively before creating any download directory.
  // lock 在创建下载目录之前以独占方式取得。
  const lock = await open(lockPath, 'wx');
  await lock.writeFile(String(process.pid));
  await lock.close();
  // stage is an immutable generation: repair never overwrites files used by a live browser.
  // stage 是不可变代次；修复绝不覆盖活动浏览器正在使用的文件。
  const stage = path.join(runtimeRoot, 'installs', randomUUID());
  // published prevents cleanup from removing the newly selected runtime.
  // published 防止清理时移除新选中的运行时。
  let published = false;
  try {
    await mkdir(stage, { recursive: true });
    // require resolves the exact package shipped beside this installer.
    // require 解析与此安装器一起分发的精确包。
    const require = createRequire(import.meta.url);
    const packagePath = require.resolve('playwright-core/package.json');
    const packageVersion: string = JSON.parse(await readFile(packagePath, 'utf8')).version;
    process.env.PLAYWRIGHT_BROWSERS_PATH = stage;
    console.log('Downloading the Chromium version pinned by the Core browser host.');
    await new Promise<void>((resolve, reject) => {
      // child uses the official pinned CLI without shell interpolation or a visible Windows console.
      // child 使用官方固定版本 CLI，不拼接 shell，也不显示 Windows 控制台。
      const child = spawn(process.execPath, [path.join(path.dirname(packagePath), 'cli.js'), 'install', 'chromium', '--no-shell', '--no-progress'], {
        env: { ...process.env, ELECTRON_RUN_AS_NODE: '1' }, stdio: 'inherit', windowsHide: true, timeout: 15 * 60 * 1000,
      });
      child.once('error', reject);
      child.once('exit', (code) => code === 0 ? resolve() : reject(new Error(`Chromium download failed (exit ${code}). Check the installation log and network connection, then retry.`)));
    });
    // chromium is imported only after its private browser directory is fixed.
    // chromium 仅在其私有浏览器目录固定后导入。
    const { chromium } = await import('playwright-core');
    const executable = chromium.executablePath();
    await access(executable);
    console.log('Verifying Chromium startup.');
    const browser = await chromium.launch({ executablePath: executable, headless: true, timeout: 30_000 });
    await browser.close();
    // temporary supports atomic replacement of the selected manifest after full verification.
    // temporary 用于在完整验证后原子替换选中清单。
    const temporary = path.join(runtimeRoot, `current-${randomUUID()}.json`);
    await writeFile(temporary, JSON.stringify({ schemaVersion: 1, playwrightVersion: packageVersion,
      chromiumExecutable: path.relative(runtimeRoot, executable) }, null, 2));
    await rename(temporary, path.join(runtimeRoot, 'current.json'));
    published = true;
    console.log('Core browser installation verified and activated.');
  } finally {
    // stage was generated under the explicit runtime root and is the only recursively removed path.
    // stage 由显式运行根生成，是唯一递归删除的路径。
    if (!published) await rm(stage, { recursive: true, force: true });
    await rm(lockPath, { force: true });
  }
}

void install().catch((error: unknown) => { console.error(error); process.exitCode = 1; });
