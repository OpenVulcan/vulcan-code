// Protect cache reuse across commits and invalidation across real compiler changes.
// 保护跨提交缓存复用，以及真实编译器变化时的缓存失效。
import assert from 'node:assert/strict';
import test from 'node:test';
import { compilerIdentity } from './ci-rust-cache.mjs';

// version and environment model the selected toolchain and the CI-only profile settings.
// version 与 environment 模拟选中工具链和仅用于 CI 的配置。
const version = 'rustc 1.97.1\ncommit-hash: fixed\nhost: x86_64-pc-windows-msvc\n';
const environment = { CARGO_INCREMENTAL: '0', CARGO_PROFILE_DEV_DEBUG: '0', CARGO_TARGET_DIR: 'target/ci-rust' };

test('source commits and workflow run numbers retain the compiler namespace', () => {
  assert.equal(compilerIdentity(version, { ...environment, GITHUB_SHA: 'first', GITHUB_RUN_ID: '1' }),
    compilerIdentity(version, { ...environment, GITHUB_SHA: 'second', GITHUB_RUN_ID: '2' }));
});

test('environment order and trailing version whitespace do not invalidate reuse', () => {
  assert.equal(compilerIdentity(version, environment),
    compilerIdentity(version.trim(), Object.fromEntries(Object.entries(environment).reverse())));
});

test('the selected compiler, host and compiler flags partition caches', () => {
  assert.notEqual(compilerIdentity(version, environment), compilerIdentity(version.replace('fixed', 'new-compiler'), environment));
  assert.notEqual(compilerIdentity(version, environment), compilerIdentity(version.replace('windows-msvc', 'linux-gnu'), environment));
  for (const [name, value] of Object.entries({ RUSTFLAGS: '-C target-cpu=native', CARGO_PROFILE_DEV_DEBUG: '2', CL: '/MT', CARGO_INCREMENTAL: '1' })) {
    assert.notEqual(compilerIdentity(version, environment), compilerIdentity(version, { ...environment, [name]: value }), `${name} must partition incompatible artifacts`);
  }
});

test('missing compiler identity is rejected rather than sharing an unknown namespace', () => {
  assert.throws(() => compilerIdentity('  ', environment), /identity is empty/);
});
