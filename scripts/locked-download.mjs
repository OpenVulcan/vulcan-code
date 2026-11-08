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
// network error, 429 or a 5xx).
// RETRY_DELAYS_MS 是同一锁定地址在暂时性失败（网络错误、429 或 5xx）后每次重试前的等待。
export const RETRY_DELAYS_MS = [5_000, 20_000, 60_000, 120_000, 240_000];

// fetchLocked requests `url` with `headers`, repeating it after each delay while the failure is transient; any other
// status, or the last transient one, is returned or thrown to the caller.
// fetchLocked 以 `headers` 请求 `url`，失败为暂时性时按每个等待重试；其他状态或最后一次暂时性失败交给调用方。
async function fetchLocked(url, retryDelaysMs, headers = {}) {
  for (let attempt = 0; ; attempt += 1) {
    let response;
    try {
      response = await fetch(url, { headers, signal: AbortSignal.timeout(10 * 60 * 1000) });
    } catch (error) {
      if (attempt >= retryDelaysMs.length) throw error;
    }
    if (response && (response.ok || (response.status !== 429 && response.status < 500) || attempt >= retryDelaysMs.length)) return response;
    console.warn(`Build input request failed (${response ? response.status : 'network error'}), retrying: ${url}`);
    await new Promise(resolve => setTimeout(resolve, retryDelaysMs[attempt]));
  }
}

// GITHUB_RELEASE_DOWNLOAD matches a github.com release download URL: owner, repository, tag and asset name.
// GITHUB_RELEASE_DOWNLOAD 匹配 github.com 发布下载地址：所有者、仓库、标签与资产名。
const GITHUB_RELEASE_DOWNLOAD = /^https:\/\/github\.com\/([^/]+)\/([^/]+)\/releases\/download\/([^/]+)\/([^/]+)$/;

// githubReleaseDownloadParts splits a github.com release download URL, or returns null for any other URL.
// githubReleaseDownloadParts 拆分 github.com 发布下载地址；其他地址返回 null。
export function githubReleaseDownloadParts(url) {
  const match = GITHUB_RELEASE_DOWNLOAD.exec(url);
  return match && { owner: match[1], repo: match[2], tag: match[3], name: decodeURIComponent(match[4]) };
}

// githubApiHeaders authenticates API requests when a token is present; shared CI addresses exhaust the anonymous limit.
// githubApiHeaders 在存在令牌时为 API 请求附加认证；共享 CI 地址会耗尽匿名配额。
function githubApiHeaders(accept) {
  const token = process.env.GITHUB_TOKEN;
  return token ? { accept, authorization: `Bearer ${token}` } : { accept };
}

// releaseLookups shares one API release lookup per tag within this process.
// releaseLookups 在本进程内为每个标签共享一次 API 发布查询。
const releaseLookups = new Map();

// fetchDownload requests `url` like fetchLocked, but takes a github.com release download from the same asset's API
// endpoint: github.com/<repo>/releases/download/ can answer 504 from every edge for hours while the API works. The
// bytes are the same asset, so locked checksums still apply.
// fetchDownload 与 fetchLocked 一样请求 `url`，但 github.com 发布下载改从同一资产的 API 端点获取：
// github.com/<repo>/releases/download/ 可能在所有边缘节点上持续数小时返回 504，而 API 正常。字节属于同一资产，
// 锁定的摘要照常适用。
export async function fetchDownload(url, retryDelaysMs = RETRY_DELAYS_MS) {
  const parts = githubReleaseDownloadParts(url);
  if (!parts) return fetchLocked(url, retryDelaysMs);
  const releaseUrl = `https://api.github.com/repos/${parts.owner}/${parts.repo}/releases/tags/${parts.tag}`;
  if (!releaseLookups.has(releaseUrl)) {
    releaseLookups.set(releaseUrl, (async () => {
      const response = await fetchLocked(releaseUrl, retryDelaysMs, githubApiHeaders('application/vnd.github+json'));
      if (!response.ok) throw new Error(`GitHub release lookup failed (${response.status}): ${releaseUrl}`);
      return response.json();
    })());
  }
  const release = await releaseLookups.get(releaseUrl).catch(error => {
    releaseLookups.delete(releaseUrl);
    throw error;
  });
  const asset = release.assets?.find(item => item.name === parts.name);
  if (!asset) throw new Error(`GitHub release ${parts.owner}/${parts.repo}@${parts.tag} has no asset ${parts.name}`);
  return fetchLocked(asset.url, retryDelaysMs, githubApiHeaders('application/octet-stream'));
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
  // response belongs to the exact locked asset; failures repeat its request and never select another version or mirror.
  // response 属于锁定的精确资产；失败只重试其请求，绝不选择其他版本或镜像。
  const response = await fetchDownload(asset.url, retryDelaysMs);
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
