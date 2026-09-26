// Prepare a complete distribution before switching the live directory.
// 切换正式目录之前准备完整发行物。
import { cp, mkdir, mkdtemp, readFile, rm, lstat, realpath, writeFile } from 'node:fs/promises';
import { execFileSync } from 'node:child_process';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { verifyPackagedLayout } from './desktop-runtime.mjs';
import { publishPreparedDirectory, recoverPublication } from './directory-publication.mjs';

// acquirePublicationGuard serializes publishers; a dead process can be reclaimed under a separate recovery lock.
// acquirePublicationGuard 将发布串行化；退出进程的所有权可在独立恢复锁下回收。
// guard is the exact owned directory; missing or ambiguous ownership is rejected, never guessed.
// guard 是精确所有权目录；所有权缺失或歧义时拒绝，不猜测。
export async function acquirePublicationGuard(guard) {
  try { await mkdir(guard); }
  catch (error) {
    if (error.code !== 'EEXIST') throw error;
    // recovery serializes stale-owner inspection so two publishers cannot remove each other's lock.
    // recovery 将过期所有者检查串行化，避免两个发布者删除彼此的锁。
    const recovery = path.join(guard, 'recovery');
    await mkdir(recovery);
    try {
      // owner records only process identity and never selects filesystem paths.
      // owner 只记录进程身份，绝不选择文件系统路径。
      const owner = JSON.parse(await readFile(path.join(guard, 'owner.json'), 'utf8'));
      if (!Number.isSafeInteger(owner.pid) || owner.pid <= 0) throw new Error('Invalid publication owner.');
      try {
        process.kill(owner.pid, 0);
        throw new Error(`Distribution publication is already owned by process ${owner.pid}.`);
      } catch (statusError) {
        if (statusError.code !== 'ESRCH') throw statusError;
      }
      await rm(guard, { recursive: true });
    } catch (recoveryError) {
      await rm(recovery, { recursive: true, force: true });
      throw recoveryError;
    }
    // A concurrent fresh publisher may win this mkdir; losing must abort without deleting its ownership.
    // 并发的新发布者可能抢先创建；竞争失败必须终止，不删除其所有权。
    await mkdir(guard);
  }
  await writeFile(path.join(guard, 'owner.json'), JSON.stringify({ pid: process.pid }), { flag: 'wx' });
}

// validateDistribution checks the existing layout contract without starting any service.
// validateDistribution 检查既有布局契约，不启动服务。
// root selects the complete generation to validate; invalid contents throw.
// root 选择待验证的完整一代；内容无效时抛出异常。
async function validateDistribution(root) {
  // layout is written by the existing packager after profile and executable validation.
  // layout 由既有打包器在验证模式和可执行文件后写入。
  const layout = JSON.parse(await readFile(path.join(root, 'runtime-layout.json'), 'utf8'));
  await verifyPackagedLayout(layout, root);
}

// publishDistribution stages synchronized resources and exact compiled binaries, then promotes the verified result.
// publishDistribution 暂存已同步资源和精确编译二进制，再提升验证通过的结果。
// repository, destination and binary paths are explicit; profile selects the existing packaging contract.
// repository、destination 和二进制路径均明确；profile 选择既有打包契约。
export async function publishDistribution(repository, destination, serviceBinary, terminalBinary, profile) {
  if (!['debug', 'release'].includes(profile)) throw new Error('Expected debug or release profile.');
  // targetRoot is the only staging boundary accepted by the desktop packager.
  // targetRoot 是桌面打包器接受的唯一暂存边界。
  const targetRoot = path.join(repository, 'target');
  await mkdir(targetRoot, { recursive: true });
  if ((await lstat(targetRoot)).isSymbolicLink()) throw new Error('Publication staging root must not be a link.');
  // resolvedRoot prevents cleanup from escaping the real repository through a parent link.
  // resolvedRoot 防止清理通过父链接逃逸真实仓库。
  const resolvedRoot = await realpath(targetRoot);
  if (resolvedRoot !== path.join(await realpath(repository), 'target')) throw new Error('Publication staging root escapes repository.');
  // guard refuses concurrent publishers and supports conservative recovery after a confirmed process exit.
  // guard 拒绝并发发布，并在确认进程退出后支持保守恢复。
  const guard = `${destination}.publishing`;
  await acquirePublicationGuard(guard);
  // stage is a unique directory owned exclusively by this publication.
  // stage 是本次发布独占的唯一目录。
  let stage;
  try {
    await recoverPublication(destination, validateDistribution);
    stage = await mkdtemp(path.join(targetRoot, 'distribution-stage-'));
    await cp(destination, stage, { recursive: true, verbatimSymlinks: true });
    await mkdir(path.join(stage, 'bin'), { recursive: true });
    await cp(serviceBinary, path.join(stage, 'bin', path.basename(serviceBinary)));
    await cp(terminalBinary, path.join(stage, 'bin', path.basename(terminalBinary)));
    await rm(path.join(stage, 'bin', 'vulcan-windows-console-helper.exe'), { force: true });
    execFileSync(process.execPath, [path.join(repository, 'scripts/desktop-runtime.mjs'), 'package', profile], {
      env: { ...process.env, VULCAN_DESKTOP_DISTRIBUTION_ROOT: stage }, stdio: 'inherit', windowsHide: true,
    });
    await publishPreparedDirectory(stage, destination, validateDistribution);
    console.log(`Distribution published: ${destination}`);
  } finally {
    // The only recursively cleaned staging path is the unique child created above.
    // 唯一递归清理的暂存路径是上方创建的唯一子目录。
    if (stage && path.dirname(stage) === targetRoot && path.basename(stage).startsWith('distribution-stage-')) await rm(stage, { recursive: true, force: true });
    await rm(guard, { recursive: true });
  }
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  // repository is derived from the script, while destination is resolved by the authoritative packager.
  // repository 由脚本确定，destination 由权威打包器解析。
  const repository = path.resolve(fileURLToPath(new URL('..', import.meta.url)));
  // arguments require both successful Cargo artifacts and the selected profile.
  // arguments 要求提供两个成功生成的 Cargo 制品及所选模式。
  const [profile, serviceBinary, terminalBinary] = process.argv.slice(2);
  if (!serviceBinary || !terminalBinary || process.argv.length !== 5) throw new Error('Usage: publish-distribution <profile> <service binary> <terminal binary>');
  const destination = execFileSync(process.execPath, [path.join(repository, 'scripts/desktop-runtime.mjs'), 'root'], { encoding: 'utf8' }).trim();
  await publishDistribution(repository, destination, path.resolve(serviceBinary), path.resolve(terminalBinary), profile);
}
