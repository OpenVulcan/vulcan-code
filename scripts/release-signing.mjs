// Release signing helpers for disposable CI runners; credential material is never printed or reported.
// 一次性 CI Runner 使用的发行签名辅助工具；凭据内容绝不输出或写入报告。
import { createHash, createHmac } from 'node:crypto';
import { spawnSync } from 'node:child_process';
import { mkdir, mkdtemp, readFile, rm, writeFile } from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

// The repository is resolved from this script so the published key is always the checked-in one.
// 仓库由脚本位置确定，确保发布公钥始终是仓库内的那一份。
const repository = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
// publishedKeyPath is the public key users verify downloads against; the CI secret must match it.
// publishedKeyPath 是用户校验下载物所用的公钥；CI 密钥必须与之匹配。
const publishedKeyPath = path.join(repository, 'RELEASE-SIGNING-KEY.asc');
const base32Alphabet = 'ABCDEFGHIJKLMNOPQRSTUVWXYZ234567';

// SigningError carries a stable code that is safe to print; other failures are reported by type only.
// SigningError 携带可安全输出的稳定错误码；其他失败只报告类型。
class SigningError extends Error {}

// decodeBase32 accepts a raw seed with or without `=` padding, because Certum issues padded seeds.
// decodeBase32 接受带或不带 `=` 填充的原始种子，因为 Certum 下发的种子带填充。
function decodeBase32(seed) {
  const canonical = seed.trim().toUpperCase().replace(/=+$/, '');
  if (!/^[A-Z2-7]{16,128}$/.test(canonical)) throw new SigningError('totp_seed_must_be_raw_base32');
  const bytes = [];
  let buffer = 0;
  let bits = 0;
  for (const character of canonical) {
    buffer = (buffer << 5) | base32Alphabet.indexOf(character);
    bits += 5;
    if (bits >= 8) {
      bits -= 8;
      bytes.push((buffer >> bits) & 0xff);
    }
  }
  return Buffer.from(bytes);
}

// totp returns the SimplySign HMAC-SHA-256 one-time code for a Base32 seed at Unix time `seconds`.
// totp 根据 Base32 种子与 Unix 时间 `seconds` 返回 SimplySign 使用的 HMAC-SHA-256 动态码。
export function totp(seed, seconds, digits = 6) {
  const counter = Buffer.alloc(8);
  counter.writeBigUInt64BE(BigInt(Math.floor(seconds / 30)));
  const digest = createHmac('sha256', decodeBase32(seed)).update(counter).digest();
  const offset = digest[digest.length - 1] & 15;
  const number = digest.readUInt32BE(offset) & 0x7fffffff;
  return String(number % 10 ** digits).padStart(digits, '0');
}

// tamperPe flips one byte inside the first section, which Authenticode hashes, avoiding excluded header fields.
// tamperPe 翻转第一个节内的一个字节；该区域受 Authenticode 哈希覆盖，避开不参与签名的头部字段。
export function tamperPe(content) {
  const bytes = Buffer.from(content);
  if (bytes.length < 64 || bytes.toString('latin1', 0, 2) !== 'MZ') throw new SigningError('invalid_pe_fixture');
  const header = bytes.readUInt32LE(0x3c);
  if (header + 24 > bytes.length || bytes.toString('latin1', header, header + 4) !== 'PE\0\0') {
    throw new SigningError('invalid_pe_signature');
  }
  const section = header + 24 + bytes.readUInt16LE(header + 20);
  if (section + 40 > bytes.length) throw new SigningError('invalid_pe_section');
  const size = bytes.readUInt32LE(section + 16);
  const offset = bytes.readUInt32LE(section + 20);
  if (size === 0 || offset < section + 40 || offset + size > bytes.length) throw new SigningError('invalid_pe_section');
  bytes[offset] ^= 1;
  return bytes;
}

// gpg runs GnuPG against one isolated home and captures every stream, since output can identify keys.
// gpg 在一个隔离目录中运行 GnuPG 并捕获全部输出流，因为输出可能暴露密钥身份。
function gpg(home, args, input) {
  const result = spawnSync('gpg', ['--homedir', home, '--batch', '--no-tty', ...args], { input, timeout: 60_000 });
  if (result.error) throw new SigningError('gpg_unavailable');
  return result;
}

// primaryFingerprints lists primary-key fingerprints of one record type (`pub` or `sec`) in a home.
// primaryFingerprints 列出隔离目录中某一记录类型（`pub` 或 `sec`）的主密钥指纹。
function primaryFingerprints(home, record) {
  const listing = gpg(home, ['--with-colons', '--fixed-list-mode', record === 'sec' ? '--list-secret-keys' : '--list-keys']);
  if (listing.status !== 0) throw new SigningError(`gpg_list_${record}_failed`);
  const fingerprints = [];
  let pending = false;
  for (const line of listing.stdout.toString('utf8').split('\n')) {
    const fields = line.split(':');
    if (fields[0] === record) pending = true;
    else if (pending && fields[0] === 'fpr') {
      fingerprints.push(fields[9]);
      pending = false;
    }
  }
  return fingerprints;
}

// signAndVerifyGpg signs `file` with the GPG_PRIVATE_KEY secret, verifies it using only the published key,
// and requires a modified copy to be rejected; the report is written to `evidence` even on failure.
// signAndVerifyGpg 用 GPG_PRIVATE_KEY 签名 `file`，只用发布公钥验签，并要求修改后的副本被拒绝；
// 即使失败也会把报告写入 `evidence`。
export async function signAndVerifyGpg(file, evidence) {
  await mkdir(evidence, { recursive: true });
  const report = { kind: 'gpg', status: 'failed', stage: 'credentials' };
  // The secret leaves the environment before any child process can inherit it.
  // 在任何子进程继承之前，先把密钥移出环境变量。
  let privateKey = process.env.GPG_PRIVATE_KEY ?? '';
  delete process.env.GPG_PRIVATE_KEY;
  const temporary = await mkdtemp(path.join(os.tmpdir(), 'release-signing-gpg-'));
  const signer = path.join(temporary, 'signer');
  const verifier = path.join(temporary, 'verifier');
  try {
    if (!privateKey.trimStart().startsWith('-----BEGIN PGP PRIVATE KEY BLOCK-----')) {
      throw new SigningError('gpg_private_key_unavailable_or_not_armored');
    }
    await mkdir(signer, { mode: 0o700 });
    await mkdir(verifier, { mode: 0o700 });
    report.stage = 'import';
    if (gpg(verifier, ['--import'], await readFile(publishedKeyPath)).status !== 0) throw new SigningError('gpg_import_published_failed');
    const published = primaryFingerprints(verifier, 'pub');
    if (published.length !== 1) throw new SigningError('published_key_must_hold_one_primary_key');
    if (gpg(signer, ['--import'], Buffer.from(privateKey)).status !== 0) throw new SigningError('gpg_import_secret_failed');
    privateKey = '';
    const secret = primaryFingerprints(signer, 'sec');
    if (secret.length !== 1 || secret[0] !== published[0]) throw new SigningError('secret_key_does_not_match_published_key');
    report.primaryFingerprint = published[0];

    report.stage = 'sign';
    const signature = path.join(evidence, `${path.basename(file)}.asc`);
    const signed = gpg(signer, ['--local-user', published[0], '--armor', '--detach-sign', '--output', signature, file]);
    if (signed.status !== 0) throw new SigningError('gpg_sign_failed');

    report.stage = 'verify';
    const verified = gpg(verifier, ['--status-fd', '1', '--verify', signature, file]);
    const valid = verified.stdout.toString('utf8').split('\n').filter(line => line.startsWith('[GNUPG:] VALIDSIG '));
    if (verified.status !== 0 || valid.length !== 1 || valid[0].trim().split(' ').at(-1) !== published[0]) {
      throw new SigningError('gpg_verify_failed');
    }

    report.stage = 'tamper_rejection';
    const content = await readFile(file);
    const tampered = path.join(temporary, 'tampered');
    const modified = Buffer.from(content);
    modified[modified.length - 1] ^= 1;
    await writeFile(tampered, modified);
    const rejected = gpg(verifier, ['--status-fd', '1', '--verify', signature, tampered]);
    if (rejected.status === 0 || !rejected.stdout.toString('utf8').includes('[GNUPG:] BADSIG ')) {
      throw new SigningError('gpg_tampered_file_not_rejected');
    }
    Object.assign(report, { status: 'passed', stage: 'complete', sha256: createHash('sha256').update(content).digest('hex') });
  } catch (error) {
    report.error = error instanceof SigningError ? error.message : error.constructor.name;
    throw error;
  } finally {
    privateKey = '';
    // Agents hold the imported secret; stop them before removing their homes.
    // 代理进程持有已导入的私钥；先停止它们再删除目录。
    for (const home of [signer, verifier]) spawnSync('gpgconf', ['--homedir', home, '--kill', 'all'], { stdio: 'ignore', timeout: 10_000 });
    await rm(temporary, { recursive: true, force: true });
    const json = JSON.stringify(report, null, 2);
    await writeFile(path.join(evidence, 'gpg-result.json'), `${json}\n`);
    console.log(json);
  }
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const [command, ...rest] = process.argv.slice(2);
  try {
    if (command === 'totp' && rest.length === 0) {
      const seed = process.env.CERTUM_TOTP_SECRET ?? '';
      delete process.env.CERTUM_TOTP_SECRET;
      // Starting near the end of a window risks the code expiring before login; wait for the next one.
      // 临近时间窗末尾时登录前验证码可能过期，因此等待下一个时间窗。
      const remaining = 30 - (Math.floor(Date.now() / 1000) % 30);
      if (remaining < 10) await new Promise(resolve => setTimeout(resolve, (remaining + 1) * 1000));
      console.log(totp(seed, Math.floor(Date.now() / 1000)));
    } else if (command === 'gpg' && rest.length === 2) {
      await signAndVerifyGpg(path.resolve(rest[0]), path.resolve(rest[1]));
    } else if (command === 'tamper-pe' && rest.length === 2) {
      await writeFile(rest[1], tamperPe(await readFile(rest[0])));
    } else {
      throw new SigningError('usage_totp_or_gpg_file_evidence_or_tamper_pe_source_destination');
    }
  } catch (error) {
    // Only stable codes are printed: native tool diagnostics can echo credential material.
    // 只输出稳定错误码：原生工具诊断可能回显凭据内容。
    console.error(`release_signing_failed: ${error instanceof SigningError ? error.message : error.constructor.name}`);
    process.exit(1);
  }
}
