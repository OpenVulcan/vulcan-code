// Exercise reuse fingerprints and the prebuilt inventory on synthetic trees.
// 使用合成目录验证复用指纹与预构建清单。
import assert from 'node:assert/strict';
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';
import { canonical, fingerprint, lockClosure, nodePlatformOf, parseDepFile, rustUnits, Tree } from './release-reuse.mjs';
import { prebuiltArtifacts, prebuiltKinds } from './shipped-rust-packages.mjs';

// METADATA is a cargo metadata resolve: `app` uses a path library, a registry crate and a build tool,
// and only tests use `harness`.
// METADATA 是一份 cargo metadata 解析结果：`app` 使用一个路径库、一个注册表 crate 与一个构建工具，只有测试使用 `harness`。
function metadata(root) {
  const member = (name, deps) => ({ id: `path+file:///${name}#0.1.0`, name, version: '0.1.0', source: null, manifest_path: path.join(root, name, 'Cargo.toml'), deps });
  const crate = name => ({ id: `registry+https://github.com/rust-lang/crates.io-index#${name}@1.0.0`, name, version: '1.0.0', source: 'registry+https://github.com/rust-lang/crates.io-index' });
  const kinds = kind => [{ kind, target: null }];
  const app = member('app');
  const library = member('library');
  const [serde, cc, harness] = [crate('serde'), crate('cc'), crate('harness')];
  return {
    packages: [app, library, serde, cc, harness],
    workspace_members: [app.id, library.id],
    resolve: {
      nodes: [
        { id: app.id, deps: [
          { pkg: library.id, dep_kinds: kinds(null) },
          { pkg: cc.id, dep_kinds: kinds('build') },
          { pkg: harness.id, dep_kinds: kinds('dev') },
        ] },
        { id: library.id, deps: [{ pkg: serde.id, dep_kinds: kinds(null) }] },
        { id: serde.id, deps: [] }, { id: cc.id, deps: [] }, { id: harness.id, deps: [] },
      ],
    },
  };
}

// fixture writes a small repository and returns it with a fingerprint function over its current state.
// fixture 写出一个小型仓库，并返回它以及基于其当前状态的指纹函数。
function fixture(t) {
  const root = mkdtempSync(path.join(os.tmpdir(), 'release-reuse-'));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  const files = {
    'Cargo.toml': '[workspace]', 'app/Cargo.toml': '[package]', 'library/Cargo.toml': '[package]',
    'app/src/main.rs': 'fn main() {}', 'library/src/lib.rs': '', 'library/catalogs/en.json': '{}',
  };
  for (const [file, content] of Object.entries(files)) {
    mkdirSync(path.dirname(path.join(root, file)), { recursive: true });
    writeFileSync(path.join(root, file), content);
  }
  const unit = { name: 'rust-binary', packages: ['app'], cargo: ['-p', 'app'] };
  const inputs = ['app/src/main.rs', 'library/catalogs', 'library/src/lib.rs'];
  const current = (tracked = Object.keys(files), env = {}) => fingerprint(unit, 'linux-x64', inputs,
    { rustc: 'rustc 1.0.0\nhost: x86_64-unknown-linux-gnu\n', metadata: metadata(root), env, tree: new Tree(root, tracked) });
  return { root, files, current };
}

test('a dependency file yields the repository inputs, without generated or outside files', () => {
  const root = path.resolve(os.tmpdir(), 'checkout');
  const at = (...parts) => path.join(root, ...parts).replace(/ /g, '\\ ');
  const prefix = process.platform === 'win32' ? '\\\\?\\' : '';
  const text = `${at('target', 'release', 'app')}: ${at('src', 'main.rs')} ${prefix}${at('assets', 'two words.txt')} `
    + `${at('target', 'release', 'build', 'out.rs')} ${path.join(path.dirname(root), 'registry', 'lib.rs')} ${at('src', 'main.rs')}\n\n`
    + `${at('src', 'main.rs')}:\n`;
  assert.deepEqual(parseDepFile(text, root), ['assets/two words.txt', 'src/main.rs']);
  assert.throws(() => parseDepFile('not a dependency file', root));
});

test('canonical JSON ignores key order', () => {
  assert.equal(canonical({ b: [1, { d: 2, c: 3 }], a: 'x' }), canonical({ a: 'x', b: [1, { c: 3, d: 2 }] }));
  assert.notEqual(canonical({ a: [1, 2] }), canonical({ a: [2, 1] }));
});

test('the lock closure follows normal and build dependencies and hashes path manifests', () => {
  const root = path.resolve(os.tmpdir(), 'checkout');
  const closure = lockClosure(metadata(root), ['app'], file => path.basename(path.dirname(file)));
  assert.equal(closure.length, 4);
  assert.ok(closure.some(entry => entry.endsWith('serde@1.0.0')), 'a transitive registry crate');
  assert.ok(closure.some(entry => entry.endsWith('cc@1.0.0')), 'a build dependency');
  assert.ok(!closure.some(entry => entry.includes('harness')), 'never a dev dependency');
  assert.ok(closure.some(entry => entry.startsWith('library@0.1.0 ')), 'a path package by its manifest hash');
  assert.throws(() => lockClosure(metadata(root), ['missing'], () => ''));
});

test('the fingerprint changes with an input, a listing or the environment, and only then', t => {
  const { root, files, current } = fixture(t);
  const before = current();
  assert.equal(current(), before, 'stable for an unchanged tree');
  assert.equal(current(Object.keys(files).reverse()), before, 'independent of listing order');

  writeFileSync(path.join(root, 'library', 'catalogs', 'en.json'), '{"a":1}');
  const edited = current();
  assert.notEqual(edited, before, 'a file inside an input directory');

  writeFileSync(path.join(root, 'app', 'src', 'stray.rs'), '');
  assert.equal(current(), edited, 'an untracked file is not listed');
  assert.notEqual(current([...Object.keys(files), 'app/src/stray.rs']), edited, 'a tracked file next to an input');

  assert.notEqual(current(Object.keys(files), { RUSTFLAGS: '-Copt-level=1' }), edited, 'the compiler flags');
  writeFileSync(path.join(root, 'library', 'Cargo.toml'), '[package]\nedition = "2024"');
  assert.notEqual(current(), edited, 'a path dependency manifest');
});

test('units follow the shipped inventory of each platform', () => {
  assert.equal(nodePlatformOf('macos-x64'), 'darwin');
  assert.throws(() => nodePlatformOf('plan9-x64'));
  const windows = rustUnits('windows-x64');
  assert.deepEqual(windows.map(unit => unit.name), ['rust-binary', 'rust-library', 'rust-isolated']);
  assert.ok(windows.every(unit => unit.files.length === unit.depFiles.length && unit.depFiles.every(file => file.endsWith('.d'))));
  const linux = rustUnits('linux-arm64');
  assert.deepEqual(linux.map(unit => unit.name), ['rust-binary', 'rust-library']);
  const library = linux.find(unit => unit.name === 'rust-library');
  assert.ok(library.depFiles.every(file => file.startsWith('lib') && !file.includes('.so')));
});

test('prebuilt kinds are validated and describe their restored files', t => {
  assert.deepEqual(prebuiltKinds(''), []);
  assert.deepEqual(prebuiltKinds('binary  library'), ['binary', 'library']);
  assert.throws(() => prebuiltKinds('binary webui'));
  const directory = mkdtempSync(path.join(os.tmpdir(), 'prebuilt-'));
  t.after(() => rmSync(directory, { recursive: true, force: true }));
  assert.throws(() => prebuiltArtifacts(['library'], directory, 'linux'), 'a restored file is missing');
  writeFileSync(path.join(directory, 'libvulcan_lancedb_native.so'), '');
  const [artifact, ...rest] = prebuiltArtifacts(['library'], directory, 'linux');
  assert.equal(rest.length, 0);
  assert.deepEqual(artifact.target.kind, ['cdylib']);
  assert.equal(artifact.filenames[0], path.join(directory, 'libvulcan_lancedb_native.so'));
});
