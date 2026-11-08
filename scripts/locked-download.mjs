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

// RETRY_DELAYS_MS are the waits before each repeated request of the same locked URL after a transient failure (a
// network error, 429 or a 5xx): GitHub release downloads answer 504 in bursts that can last several minutes.
// RETRY_DELAYS_MS 是同一锁定地址在暂时性失败（网络错误、429 或 5xx）后每次重试前的等待；GitHub 发布下载会成串地
// 返回 504，可持续数分钟。
export const RETRY_DELAYS_MS = [5_000, 20_000, 60_000, 120_000, 240_000];

// fetchLocked requests `url`, repeating it after each delay while the failure is transient; any other status, or the
// last transient one, is returned or thrown to the caller.
// fetchLocked 请求 `url`，失败为暂时性时按每个等待重试；其他状态或最后一次暂时性失败交给调用方。
async function fetchLocked(url, retryDelaysMs) {
  for (let attempt = 0; ; attempt += 1) {
    let response;
    try {
      response = await fetch(url, { signal: AbortSignal.timeout(10 * 60 * 1000) });
    } catch (error) {
      if (attempt >= retryDelaysMs.length) throw error;
    }
    if (response && (response.ok || (response.status !== 429 && response.status < 500) || attempt >= retryDelaysMs.length)) return response;
    console.warn(`Build input request failed (${response ? response.status : 'network error'}), retrying: ${url}`);
    await new Promise(resolve => setTimeout(resolve, retryDelaysMs[attempt]));
  }
}

// obtainLockedInput reads asset {url, sha256} into cache; offline forbids network access and returns verified bytes.
// obtainLockedInput 将资产 {url, sha256} 读入 cache；offline 禁止网络访问并返回已验证字节。
export async function obtainLockedInput(asset, cache, offline = false, retryDelaysMs = RETRY_DELAYS_MS) {
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
  // response belongs to the exact locked URL; failures repeat that URL and never select another version or mirror.
  // response 属于锁定的精确地址；失败只重试该地址，绝不选择其他版本或镜像。
  const response = await fetchLocked(asset.url, retryDelaysMs);
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
