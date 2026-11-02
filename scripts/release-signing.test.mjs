// One-time code and Authenticode tamper fixture behavior for release signing.
// 发行签名所用动态码与 Authenticode 篡改样本的行为。
import assert from 'node:assert/strict';
import test from 'node:test';
import { tamperPe, totp } from './release-signing.mjs';

// rfc6238Seed is the RFC 6238 SHA-256 test key, Base32-encoded as Certum delivers seeds.
// rfc6238Seed 是 RFC 6238 的 SHA-256 测试密钥，按 Certum 下发种子的方式做 Base32 编码。
const rfc6238Seed = 'GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQGEZA====';

test('totp matches the RFC 6238 SHA-256 vectors', () => {
  const vectors = [[59, '46119246'], [1111111109, '68084774'], [1111111111, '67062674'],
    [1234567890, '91819424'], [2000000000, '90698825'], [20000000000, '77737706']];
  for (const [seconds, expected] of vectors) assert.equal(totp(rfc6238Seed, seconds, 8), expected);
});

test('totp accepts unpadded and lowercase seeds and yields six digits by default', () => {
  const code = totp(rfc6238Seed, 59);
  assert.match(code, /^\d{6}$/);
  assert.equal(totp(rfc6238Seed.replace(/=+$/, '').toLowerCase(), 59), code);
});

test('totp rejects anything that is not a raw Base32 seed', () => {
  for (const invalid of ['', 'not-a-seed', '0123456789', 'otpauth://totp/test']) {
    assert.throws(() => totp(invalid, 59), { message: 'totp_seed_must_be_raw_base32' });
  }
});

// minimalPe builds the smallest PE image the parser accepts, with its first section after the section table.
// minimalPe 构造一个足以解析的 PE 映像，其第一个节位于节表之后。
function minimalPe() {
  const bytes = Buffer.alloc(0x200);
  bytes.write('MZ', 0, 'latin1');
  bytes.writeUInt32LE(0x40, 0x3c);
  bytes.write('PE\0\0', 0x40, 'latin1');
  bytes.writeUInt16LE(0xf0, 0x40 + 20);
  const section = 0x40 + 24 + 0xf0;
  bytes.writeUInt32LE(0x10, section + 16);
  bytes.writeUInt32LE(0x180, section + 20);
  return bytes;
}

test('tamperPe changes exactly one byte inside the first section', () => {
  const original = minimalPe();
  const tampered = tamperPe(original);
  const changed = [...tampered].flatMap((byte, index) => (byte === original[index] ? [] : [index]));
  assert.deepEqual(changed, [0x180]);
});

test('tamperPe rejects files that are not PE images', () => {
  assert.throws(() => tamperPe(Buffer.alloc(128)), { message: 'invalid_pe_fixture' });
  const noSignature = minimalPe();
  noSignature.write('XX', 0x40, 'latin1');
  assert.throws(() => tamperPe(noSignature), { message: 'invalid_pe_signature' });
  const emptySection = minimalPe();
  emptySection.writeUInt32LE(0, 0x40 + 24 + 0xf0 + 16);
  assert.throws(() => tamperPe(emptySection), { message: 'invalid_pe_section' });
});
