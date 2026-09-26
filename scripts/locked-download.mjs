// Obtain exact build bytes from a verified cache or a pinned source.
// 从已校验缓存或固定来源取得精确构建字节。
import { createHash, randomUUID } from 'node:crypto';
import { mkdir, readFile, rename, rm, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

// verifyBytes checks bytes against a reviewed SHA-256 and returns the same bytes or throws.
// verifyBytes 按评审后的 SHA-256 检查字节，返回原字节或抛出异常。
export function verifyBytes(bytes, sha256) {
  if (!/^[a-f0-9]{64}$/.test(sha256)) throw new Error('Invalid locked SHA-256.');
  if (createHash('sha256').update(bytes).digest('hex') !== sha256) throw new Error(`Build input checksum mismatch: ${sha256}`);
  return bytes;
}

// obtainLockedInput reads asset {url, sha256} into cache; offline forbids network access and returns verified bytes.
// obtainLockedInput 将资产 {url, sha256} 读入 cache；offline 禁止网络访问并返回已验证字节。
export async function obtainLockedInput(asset, cache, offline = false) {
  if (!/^[a-f0-9]{64}$/.test(asset.sha256)) throw new Error('Invalid locked SHA-256.');
  // file is content-addressed so different tools or revisions cannot overwrite each other.
  // file 按内容寻址，避免不同工具或修订相互覆盖。
  const file = path.join(cache, asset.sha256);
  try {
    return verifyBytes(await readFile(file), asset.sha256);
  } catch (error) {
    if (error.code !== 'ENOENT') throw error;
  }
  if (offline) throw new Error(`Offline build input is absent: ${asset.sha256}`);
  if (!asset.url.startsWith('https://')) throw new Error('Locked build inputs require HTTPS.');
  // response belongs to the exact locked URL; failures never select another version or mirror.
  // response 属于锁定的精确地址；失败绝不选择其他版本或镜像。
  const response = await fetch(asset.url, { signal: AbortSignal.timeout(10 * 60 * 1000) });
  if (!response.ok) throw new Error(`Build input download failed (${response.status}): ${asset.url}`);
  const bytes = verifyBytes(Buffer.from(await response.arrayBuffer()), asset.sha256);
  await mkdir(cache, { recursive: true });
  // temporary owns only this write; readers never observe a partially written cache entry.
  // temporary 仅归属本次写入；读取方不会观察到写入一半的缓存。
  const temporary = `${file}.${randomUUID()}.tmp`;
  try {
    await writeFile(temporary, bytes, { flag: 'wx' });
    await rename(temporary, file);
  } finally {
    await rm(temporary, { force: true });
  }
  return bytes;
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const [url, sha256, cache, destination, mode] = process.argv.slice(2);
  if (!destination || (mode !== undefined && mode !== '--offline')) throw new Error('Usage: locked-download <url> <sha256> <cache> <destination> [--offline]');
  const bytes = await obtainLockedInput({ url, sha256 }, path.resolve(cache), mode === '--offline');
  await writeFile(destination, bytes);
}
