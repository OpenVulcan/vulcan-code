// Coordinate terminal-only Core startup and the native Linux Debug update cycle.
// 协调纯终端 Core 启动，以及 Linux 原生 Debug 更新流程。
import { spawn, execFile } from 'node:child_process';
import { createHash, randomUUID } from 'node:crypto';
import { createReadStream, constants } from 'node:fs';
import { access, copyFile, mkdir, open, readFile, readlink, realpath, rename, rm, rmdir, stat } from 'node:fs/promises';
import path from 'node:path';
import { createInterface } from 'node:readline';
import { setTimeout as delay } from 'node:timers/promises';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { isDeepStrictEqual, promisify } from 'node:util';
import { validateRuntimeLayoutIdentity, verifyPackagedLayout } from './desktop-runtime.mjs';
import { acquirePublicationGuard } from './publish-distribution.mjs';
import { shippedFileName, shippedRustPackages } from './shipped-rust-packages.mjs';

// One deadline bounds each terminal launch or graceful shutdown operation.
// 每次终端启动或优雅关闭操作均受此唯一截止时长约束。
const operationTimeoutMs = 30_000;
// Poll only local discovery; never guess service ports or print discovery credentials.
// 仅轮询本地发现信息，不猜测服务端口，也不输出发现凭据。
const pollIntervalMs = 100;
// Resolve source commands independently of the terminal's current directory.
// 源码命令解析不依赖终端当前目录。
const repository = fileURLToPath(new URL('..', import.meta.url));
// Capture bounded read-only commands without invoking a shell.
// 不经 shell 捕获有界的只读命令结果。
const execFileAsync = promisify(execFile);
// The service loads this native LanceDB library from its own directory, so both are built and replaced together.
// 服务从自身目录加载此原生 LanceDB 库，因此两者一同构建、一同替换。
const lanceLibrary = shippedRustPackages.find(item => item.package === 'vulcan-lancedb-native');

// Validate a complete distribution and reuse its packaged launch-description and health readers.
// 校验完整发行物，并复用其中已打包的启动描述及健康身份读取器。
// root/layout/profile select one published generation; return its verified terminal launch context.
// root/layout/profile 选择已发布的一代；返回校验后的终端启动上下文。
export async function createCoreContext(root, layout, profile) {
  validateRuntimeLayoutIdentity(layout, profile);
  await verifyPackagedLayout(layout, root);
  // Package verification above establishes containment for every relative manifest entry.
  // 上述发行校验已确认清单中每个相对条目均位于发行目录内。
  const servicePath = await realpath(path.join(root, layout.serviceExecutable));
  // Mutating Core workflows may only select executables physically inside this distribution.
  // 可修改 Core 的流程只能选择物理位置确实属于此发行目录的可执行文件。
  const canonicalRoot = await realpath(root);
  if (!servicePath.startsWith(canonicalRoot + path.sep)) throw new Error('Core executable resolves outside its distribution.');
  // The compiled module owns the actual discovery/health protocol used by the desktop too.
  // 该编译模块同时拥有桌面使用的真实发现及健康校验协议。
  const launcher = await import(pathToFileURL(path.join(root, path.dirname(layout.desktopEntry), 'service-launcher.js')).href);
  // --describe supplies user-data paths; launch scripts never reconstruct them from HOME.
  // --describe 提供用户数据路径；启动脚本绝不从 HOME 重新拼接。
  const description = await launcher.readServiceLaunchDescription(servicePath, []);
  if (description.profile !== profile || !description.modes.includes('--headless')
      || await realpath(description.executablePath) !== servicePath) {
    throw new Error('Core does not match this distribution or support the declared headless launch mode. Rebuild first.');
  }
  return {
    root, layout, servicePath, description, launcher,
    env: { ...process.env, VULCAN_RUNTIME_LAYOUT: path.join(root, 'runtime-layout.json') },
  };
}

// Start at most one detached headless Core, or await an already starting/ready owner.
// 最多启动一个分离的无头 Core，或等待已经启动中、已就绪的持有者。
// context comes from createCoreContext; return the health-verified instance without logging its token.
// context 来自 createCoreContext；返回健康身份已验证的实例，不记录其令牌。
export async function ensureHeadlessCore(context) {
  // An existing instance is trusted only after the packaged reader checks its complete health identity.
  // 既有实例只有经过打包读取器的完整健康身份校验后才可被信任。
  let instance = await context.launcher.readLiveServiceInstance(context.description.discoveryPath);
  if (instance?.status === 'ready') {
    console.log(`Core is ready (pid ${instance.pid}).`);
    return instance;
  }
  if (instance && instance.status !== 'starting') throw new Error(`Core is ${instance.status}; it cannot accept a terminal client.`);
  // These handles belong only to this invocation, never to a reused user process.
  // 这些句柄仅属于本次调用，绝不指向复用的用户进程。
  let child;
  let exited = false;
  let launchError;
  // A shared deadline also bounds waiting for a different already-starting owner.
  // 同一截止时间也约束对另一个启动中持有者的等待。
  const deadline = Date.now() + operationTimeoutMs;
  // The unique diagnostic file belongs to this launch and is not interleaved with the TUI screen.
  // 唯一诊断文件属于本次启动，不与 TUI 屏幕输出交错。
  let diagnosticPath;
  try {
    for (;;) {
      if (instance?.status === 'ready') {
        if (child && instance.pid !== child.pid) throw new Error('A different Core owner became ready during this launch.');
        console.log(`Core is ready (pid ${instance.pid}).`);
        return instance;
      }
      if (instance && instance.status !== 'starting') throw new Error(`Core is ${instance.status}; startup did not complete.`);
      if (!instance && !child) {
        // Diagnostics stay in the repository's generated artifacts, with owner-only permissions.
        // 诊断保存在仓库生成产物中，并限制为所有者可读写。
        const diagnosticDirectory = path.join(repository, 'target', 'core-cli');
        await mkdir(diagnosticDirectory, { recursive: true });
        diagnosticPath = path.join(diagnosticDirectory, `${randomUUID()}.log`);
        // Close the parent descriptor after spawn; the detached child owns its inherited descriptors.
        // spawn 后关闭父描述符；分离子进程拥有已继承的描述符。
        const diagnostic = await open(diagnosticPath, 'wx', 0o600);
        try {
          child = spawn(context.servicePath, ['--headless', '--no-debug-console'], {
            cwd: context.root, env: context.env, detached: true, windowsHide: true,
            stdio: ['ignore', diagnostic.fd, diagnostic.fd],
          });
          child.once('error', error => { launchError = error; });
          child.once('exit', () => { exited = true; });
          child.unref();
        } finally {
          await diagnostic.close();
        }
        console.log(`Starting headless Core. Startup log: ${diagnosticPath}`);
      }
      if (launchError || exited) throw new Error(`Core exited before readiness. Inspect ${diagnosticPath}.`);
      if (Date.now() >= deadline) throw new Error(`Core startup timed out. Inspect ${diagnosticPath ?? context.description.discoveryPath}.`);
      await delay(pollIntervalMs);
      instance = await context.launcher.readLiveServiceInstance(context.description.discoveryPath);
    }
  } catch (error) {
    if (child && !exited && !launchError) {
      child.kill('SIGTERM');
      // Do not claim cleanup or restore a binary while a started child still owns it.
      // 已启动子进程仍持有程序时，不声称清理完成，也不恢复二进制。
      const exitDeadline = Date.now() + operationTimeoutMs;
      while (!exited && Date.now() < exitDeadline) await delay(pollIntervalMs);
      if (!exited) throw new Error(`Core did not stop after a failed launch. Inspect ${diagnosticPath}.`);
    }
    throw error;
  }
}

// Compile only Core and its LanceDB library, using Cargo's declared artifact stream rather than a guessed target directory.
// 只编译 Core 及其 LanceDB 库，使用 Cargo 声明的制品流，不猜测 target 目录。
// The library builds in its own invocation; shipped-rust-packages.mjs `shippedCargoArgs` explains why.
// 库在独立调用中构建；原因见 shipped-rust-packages.mjs 的 `shippedCargoArgs`。
// root selects the source checkout; return the unique successful service executable and library paths.
// root 选择源码检出；返回唯一成功生成的服务可执行路径与库路径。
export async function buildCore(root) {
  const artifacts = new Set();
  for (const message of await cargoArtifacts(root, 'vulcan-code-service')) {
    if (message.target.name === 'vulcan-code-service' && message.target.kind.includes('bin') && message.executable) artifacts.add(message.executable);
  }
  const libraries = new Set();
  for (const message of await cargoArtifacts(root, lanceLibrary.package)) {
    if (message.target.name !== lanceLibrary.library || !message.target.kind.includes('cdylib')) continue;
    for (const filename of message.filenames) if (path.basename(filename) === shippedFileName(lanceLibrary)) libraries.add(filename);
  }
  if (artifacts.size !== 1 || libraries.size !== 1) {
    throw new Error('Core compilation failed or Cargo did not report exactly one service artifact and one LanceDB library.');
  }
  // realpath also rejects a reported artifact that was never created.
  // realpath 也会拒绝未实际创建的已报告制品。
  return { service: await realpath([...artifacts][0]), library: await realpath([...libraries][0]) };
}

// Build one package and return its compiler-artifact messages once Cargo reports success.
// 构建单个包，在 Cargo 报告成功后返回其 compiler-artifact 消息。
// root selects the source checkout and pkg the Cargo package; failure or malformed output throws.
// root 选择源码检出，pkg 选择 Cargo 包；失败或输出畸形时抛出异常。
async function cargoArtifacts(root, pkg) {
  // Store every reported artifact and the explicit final success record.
  // 保存每个报告的制品，以及显式的最终成功记录。
  const artifacts = [];
  let succeeded = false;
  // Cargo inherits the repository's concurrency limits; no jobs/test settings are overridden.
  // Cargo 继承仓库并发限制，不覆盖 jobs 或测试设置。
  const cargo = spawn('cargo', ['build', '--locked', '-p', pkg, '--message-format=json'], {
    cwd: root, stdio: ['ignore', 'pipe', 'inherit'], windowsHide: true,
  });
  // Observe process failure immediately, including executable-not-found errors.
  // 立即观察进程失败，包括找不到可执行文件的错误。
  const completion = new Promise(resolve => {
    cargo.once('error', error => resolve({ error }));
    cargo.once('exit', (code, signal) => resolve({ code, signal }));
  });
  try {
    for await (const line of createInterface({ input: cargo.stdout })) {
      // Cargo JSON is a declared protocol; malformed output must fail instead of selecting stale files.
      // Cargo JSON 是声明的协议；输出畸形必须失败，不能改选旧文件。
      const message = JSON.parse(line);
      if (message.reason === 'compiler-artifact') artifacts.push(message);
      if (message.reason === 'compiler-message') process.stderr.write(message.message.rendered);
      if (message.reason === 'build-finished') succeeded = message.success === true;
    }
  } catch (error) {
    cargo.kill('SIGTERM');
    await completion;
    throw error;
  }
  // A successful process alone does not prove that Cargo published the requested artifact; the caller checks that.
  // 进程成功本身不能证明 Cargo 已发布请求的制品；由调用方检查。
  const result = await completion;
  if (result.error || result.code !== 0 || !succeeded) throw new Error(`Cargo build of ${pkg} failed.`);
  return artifacts;
}

// Resolve a Linux process executable; only an absent process is treated as already stopped.
// 解析 Linux 进程可执行路径；只有进程不存在时才视为已经停止。
// pid is a validated positive integer; return the kernel-reported executable or null.
// pid 是已校验的正整数；返回内核报告的可执行路径，或 null。
async function linuxExecutable(pid) {
  try { return await readlink(`/proc/${pid}/exe`); }
  catch (error) {
    if (error.code === 'ENOENT' || error.code === 'ESRCH') return null;
    throw error;
  }
}

// Gracefully stop only the discovery PID whose kernel executable is this exact distribution Core.
// 仅优雅停止内核可执行路径确属当前发行物 Core 的发现 PID。
// context supplies the authoritative paths; foreign or unverifiable owners prevent replacement.
// context 提供权威路径；外部或无法验证的持有者必须阻止替换。
export async function stopLinuxCore(context) {
  // A stale document may outlive its owner, but malformed ownership must never authorize a signal.
  // 过期文档可能比持有者存在更久，但畸形归属绝不能授权发送信号。
  let document;
  try { document = JSON.parse(await readFile(context.description.discoveryPath, 'utf8')); }
  catch (error) { if (error.code === 'ENOENT') return; throw new Error('Cannot verify the existing Core discovery document.'); }
  if (!Number.isSafeInteger(document.pid) || document.pid < 1) throw new Error('Invalid Core owner PID.');
  // The process path is read from procfs, never accepted from the discovery document itself.
  // 进程路径从 procfs 读取，绝不接受发现文档自行声明的路径。
  const executable = await linuxExecutable(document.pid);
  if (executable === null) return;
  if (executable !== context.servicePath) throw new Error(`Another executable owns this Core data root (pid ${document.pid}); it was not stopped.`);
  if ((await stat(`/proc/${document.pid}`)).uid !== process.getuid()) throw new Error('Core belongs to a different Linux user; it was not stopped.');
  process.kill(document.pid, 'SIGTERM');
  // Keep the old artifact untouched until the selected owner releases it.
  // 在选中持有者释放程序之前，保持旧制品不变。
  const deadline = Date.now() + operationTimeoutMs;
  while (await linuxExecutable(document.pid) === executable) {
    if (Date.now() >= deadline) throw new Error('Core did not finish graceful shutdown; its executable was not replaced.');
    await delay(pollIntervalMs);
  }
}

// Hash a large artifact as a stream without holding the executable in memory.
// 流式计算大制品摘要，避免将可执行文件整体加载到内存。
// filename is an exact owned artifact path; return its SHA-256 digest.
// filename 是精确的自有制品路径；返回其 SHA-256 摘要。
async function artifactDigest(filename) {
  // The stream owns only the selected file and closes automatically after consumption.
  // 流仅拥有选定文件，消费结束后自动关闭。
  const hash = createHash('sha256');
  for await (const chunk of createReadStream(filename)) hash.update(chunk);
  return hash.digest('hex');
}

// Rebuild Linux Debug Core, verify its launch contract, and replace it with an owned rollback copy.
// 重建 Linux Debug Core，校验启动契约，再使用自有回退副本替换。
// root selects the source checkout; success leaves clients/resources unchanged and the new Core ready.
// root 选择源码检出；成功后客户端与资源保持不变，新 Core 已就绪。
export async function restartLinuxCore(root) {
  if (process.platform !== 'linux') throw new Error('Native Core restart currently supports Linux; Windows uses make.ps1 core-restart.');
  // The existing resolver remains the authority for root selection and layout identity.
  // 既有解析器继续作为根目录选择及布局身份的权威。
  const resolver = path.join(root, 'scripts', 'desktop-runtime.mjs');
  // Core restart only ever targets the Debug distribution, so the release root is never named here.
  // Core 重启只针对 Debug 发行目录，因此这里从不涉及正式版根目录。
  const selectedRoot = (await execFileAsync(process.execPath, [resolver, 'root', 'debug'])).stdout.trim();
  // Share the publisher's guard so concurrent full builds and Core restarts cannot replace each other's files.
  // 共用发布器的锁，避免并发完整构建与 Core 重启互相替换文件。
  const guard = `${selectedRoot}.publishing`;
  await acquirePublicationGuard(guard);
  try {
    const layout = JSON.parse((await execFileAsync(process.execPath, [resolver, 'layout', 'debug'])).stdout);
    const context = await createCoreContext(selectedRoot, layout, 'debug');
    const built = await buildCore(root);
    const candidate = await context.launcher.readServiceLaunchDescription(built.service, []);
    // Executable location changes during publication; every other described launch field must stay equal.
    // 发布时可执行位置会变化；其余已描述启动字段必须保持一致。
    const { executablePath: oldExecutable, ...oldContract } = context.description;
    const { executablePath: newExecutable, ...newContract } = candidate;
    if (!isDeepStrictEqual(oldContract, newContract)) throw new Error('The Core launch contract changed; use a full make build instead.');
    await access(oldExecutable);
    await access(newExecutable);
    // Unique same-directory files allow an atomic Unix rename and retain the old files until readiness.
    // 同目录唯一文件支持 Unix 原子改名，并将旧文件保留到就绪确认之后。
    // A distribution published before the library split has no library yet; restoring it then removes the new one.
    // 库拆分之前发布的发行物尚无该库；此时恢复即删除新库。
    const attempt = randomUUID();
    const replacements = [
      { built: built.service, output: context.servicePath },
      { built: built.library, output: path.join(path.dirname(context.servicePath), shippedFileName(lanceLibrary)) },
    ].map(item => ({ ...item, staged: `${item.output}.${attempt}.next`, backup: `${item.output}.${attempt}.previous`, existed: false }));
    try {
      for (const item of replacements) {
        await copyFile(item.built, item.staged, constants.COPYFILE_EXCL);
        if (await artifactDigest(item.built) !== await artifactDigest(item.staged)) throw new Error(`The staged ${path.basename(item.output)} differs from the Cargo artifact.`);
        try {
          await copyFile(item.output, item.backup, constants.COPYFILE_EXCL);
          item.existed = true;
        } catch (error) { if (error.code !== 'ENOENT' || item.output === context.servicePath) throw error; }
      }
      await stopLinuxCore(context);
      for (const item of replacements) await rename(item.staged, item.output);
      try {
        await ensureHeadlessCore(context);
      } catch (error) {
        // Restore only after the failed owner has stopped; do not overwrite a still-running replacement.
        // 仅在失败持有者停止后恢复；不覆盖仍在运行的替换程序。
        await stopLinuxCore(context);
        for (const item of replacements) {
          if (item.existed) await rename(item.backup, item.output);
          else await rm(item.output);
        }
        await ensureHeadlessCore(context);
        throw new Error('New Core failed; the previous Core was restored and restarted.', { cause: error });
      }
      for (const item of replacements) if (item.existed) await rm(item.backup);
      console.log('Debug Core restarted. Electron, Vite, TUI, and synchronized resources were not rebuilt.');
    } catch (error) {
      // If any later stage fails, the uniquely named backups are preserved for explicit recovery.
      // 后续任一阶段失败时，保留唯一命名备份供显式恢复。
      throw new Error(`${error.message} Recovery copies, when created: ${replacements.map(item => item.backup).join(', ')}`, { cause: error });
    } finally {
      for (const item of replacements) await rm(item.staged, { force: true });
    }
  } finally {
    // Release only the two exact entries owned by this invocation, without recursive cleanup.
    // 仅释放本次调用拥有的两个精确条目，不递归清理。
    await rm(path.join(guard, 'owner.json'));
    await rmdir(guard);
  }
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  // Command parsing is explicit; unsupported variants cannot fall through to another implementation.
  // 命令解析显式执行；不支持的变体不能转入其他实现。
  const [command, ...extra] = process.argv.slice(2);
  try {
    if (extra.length || !['build', 'restart'].includes(command)) throw new Error('Usage: node scripts/core-cli.mjs build|restart');
    if (command === 'build') {
      const built = await buildCore(repository);
      console.log(`Debug Core compiled: ${built.service} (LanceDB library: ${built.library})`);
    }
    else await restartLinuxCore(repository);
  } catch (error) {
    console.error(error.message);
    process.exitCode = 1;
  }
}

