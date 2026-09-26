// Publish prepared directories with a recoverable previous generation.
// 发布已准备目录，并保留可恢复的上一代。
import { lstat, rename, rm } from 'node:fs/promises';
import path from 'node:path';
import { randomUUID } from 'node:crypto';

// directoryExists checks exact directory identity; links and non-directories are rejected.
// directoryExists 检查精确目录身份；拒绝链接与非目录。
async function directoryExists(directory) {
  try {
    // metadata is obtained without following a directory link.
    // metadata 在不跟随目录链接的情况下取得。
    const metadata = await lstat(directory);
    if (!metadata.isDirectory() || metadata.isSymbolicLink()) throw new Error(`Publication path must be a real directory: ${directory}`);
    return true;
  } catch (error) {
    if (error.code === 'ENOENT') return false;
    throw error;
  }
}

// recoverPublication restores an interrupted switch, or validates a completed switch before retiring its backup.
// recoverPublication 恢复被中断的切换，或先验证已完成切换再移除备份。
// destination is the exact publication root; validate must reject an unusable generation; returns no value.
// destination 是精确发布根目录；validate 必须拒绝不可用的一代；无返回值。
export async function recoverPublication(destination, validate) {
  // backup has a deterministic identity so recovery does not guess among candidate directories.
  // backup 具有确定身份，使恢复无需猜测候选目录。
  const backup = `${destination}.previous`;
  if (!await directoryExists(backup)) return;
  if (!await directoryExists(destination)) {
    await rename(backup, destination);
    return;
  }
  try { await validate(destination); }
  catch (error) {
    // rejected preserves the incomplete new generation for diagnosis instead of deleting unknown contents.
    // rejected 保留不完整的新一代以供诊断，不删除未知内容。
    const rejected = `${destination}.rejected-${randomUUID()}`;
    await rename(destination, rejected);
    try { await rename(backup, destination); }
    catch (restoreError) { throw new AggregateError([error, restoreError], `Publication recovery failed; old generation remains at ${backup}`); }
    return;
  }
  await rm(backup, { recursive: true });
}

// publishPreparedDirectory validates stage before moving destination; renameDirectory is an injectable filesystem boundary.
// publishPreparedDirectory 在移动 destination 前验证 stage；renameDirectory 是可注入的文件系统边界。
// Returns after validation and switch; a failed switch restores the old directory or explicitly reports its backup path.
// 校验并切换后返回；切换失败恢复旧目录，或明确报告旧版备份路径。
export async function publishPreparedDirectory(stage, destination, validate, renameDirectory = rename) {
  if (path.resolve(stage) === path.resolve(destination)) throw new Error('Publication stage must differ from destination.');
  if (!await directoryExists(stage)) throw new Error('Publication stage is absent.');
  await recoverPublication(destination, validate);
  await validate(stage);
  // backup is always the exact destination sibling, not a discovered or user-supplied deletion path.
  // backup 始终是目标的精确同级目录，不是搜索或由用户提供的删除路径。
  const backup = `${destination}.previous`;
  // previous determines whether a failed promotion needs rollback.
  // previous 决定提升失败时是否需要回滚。
  const previous = await directoryExists(destination);
  if (previous) await renameDirectory(destination, backup);
  try {
    await renameDirectory(stage, destination);
  } catch (error) {
    if (previous) {
      try { await renameDirectory(backup, destination); }
      catch (restoreError) { throw new AggregateError([error, restoreError], `Publication rollback failed; old generation remains at ${backup}`); }
    }
    throw error;
  }
  // Keep the prior generation until the next validated invocation, including across an abrupt process exit.
  // 保留上一代直到下一次经过验证的调用，包括进程突然退出的情况。
}
