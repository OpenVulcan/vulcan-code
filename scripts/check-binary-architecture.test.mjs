import assert from 'node:assert/strict';
import { mkdir, mkdtemp, rm, writeFile } from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';
import { machOArchitectures, mismatches } from './check-binary-architecture.mjs';

const X64 = 0x01000007;
const ARM64 = 0x0100000c;

// thin builds a little-endian 64-bit Mach-O header for one CPU type.
// thin 为单一 CPU 类型构造小端 64 位 Mach-O 文件头。
function thin(cpuType) {
  const header = Buffer.alloc(32);
  header.writeUInt32BE(0xcffaedfe, 0);
  header.writeInt32LE(cpuType, 4);
  return header;
}

// universal builds a fat header listing the given CPU types.
// universal 构造列出给定 CPU 类型的通用文件头。
function universal(...cpuTypes) {
  const header = Buffer.alloc(8 + cpuTypes.length * 20);
  header.writeUInt32BE(0xcafebabe, 0);
  header.writeUInt32BE(cpuTypes.length, 4);
  cpuTypes.forEach((cpuType, index) => header.writeInt32BE(cpuType, 8 + index * 20));
  return header;
}

// Headers are classified by their own format; Java class files and plain files are not Mach-O.
// 文件头按自身格式分类；Java class 文件与普通文件不是 Mach-O。
test('Mach-O headers report their architectures', () => {
  assert.deepEqual(machOArchitectures(thin(X64)), [X64]);
  assert.deepEqual(machOArchitectures(universal(X64, ARM64)), [X64, ARM64]);
  const javaClass = Buffer.alloc(8);
  javaClass.writeUInt32BE(0xcafebabe, 0);
  javaClass.writeUInt32BE(0x00000041, 4);
  assert.equal(machOArchitectures(javaClass), null);
  assert.equal(machOArchitectures(Buffer.from('#!/bin/sh\n')), null);
});

// A tree passes only when every Mach-O file outside single-platform npm packages carries the target architecture.
// 只有单平台 npm 包以外的每个 Mach-O 文件都含目标架构时，目录树才通过。
test('a tree reports Mach-O files without the target architecture', async () => {
  const root = await mkdtemp(path.join(os.tmpdir(), 'vulcan-arch-'));
  try {
    await mkdir(path.join(root, 'bin'), { recursive: true });
    await mkdir(path.join(root, 'browser/chrome-mac-arm64'), { recursive: true });
    await mkdir(path.join(root, 'node_modules/@reflink/reflink-darwin-arm64'), { recursive: true });
    await writeFile(path.join(root, 'bin/core'), thin(X64));
    await writeFile(path.join(root, 'bin/both'), universal(ARM64, X64));
    await writeFile(path.join(root, 'bin/script'), '#!/bin/sh\n');
    await writeFile(path.join(root, 'browser/chrome-mac-arm64/chrome'), thin(ARM64));
    await writeFile(path.join(root, 'node_modules/@reflink/reflink-darwin-arm64/reflink.node'), thin(ARM64));
    assert.deepEqual(await mismatches(root, X64), [path.join('browser', 'chrome-mac-arm64', 'chrome')]);
    assert.deepEqual((await mismatches(root, ARM64)).sort(), [path.join('bin', 'core')]);
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});
