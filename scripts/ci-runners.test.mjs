// Exercise the choice of self-hosted pools on synthetic runner lists.
// 使用合成的运行器列表验证自托管池的选择。
import assert from 'node:assert/strict';
import test from 'node:test';
import { POOLS, poolLabel, selectPools } from './ci-runners.mjs';

// runner describes one registered runner with `status` and the labels `names`.
// runner 描述一个已注册的运行器，状态为 `status`，标签为 `names`。
function runner(status, ...names) {
  return { status, labels: ['self-hosted', 'macOS', ...names].map(name => ({ name })) };
}

test('auto uses a pool only while one of its runners is online', () => {
  const intel = poolLabel('macos-x64');
  assert.equal(selectPools('auto', [runner('online', intel)])['macos-x64'], intel);
  assert.deepEqual(selectPools('auto', [runner('offline', intel)]), {});
  assert.deepEqual(selectPools('auto', [runner('online', 'some-other-label')]), {});
  assert.deepEqual(selectPools('auto', null), {});
});

test('hosted uses no pool and self-hosted uses every pool regardless of runners', () => {
  const online = POOLS.map(pool => runner('online', poolLabel(pool)));
  assert.deepEqual(selectPools('hosted', online), {});
  const forced = selectPools('self-hosted', null);
  assert.deepEqual(POOLS.map(pool => forced[pool]), POOLS.map(poolLabel));
});

test('an alias takes the first of its pools with a runner online', () => {
  const intel = poolLabel('macos-x64');
  const arm = poolLabel('macos-arm64');
  assert.equal(selectPools('auto', [runner('online', intel)]).macos, intel);
  assert.equal(selectPools('auto', [runner('online', intel), runner('online', arm)]).macos, arm);
  assert.equal(selectPools('auto', []).macos, undefined);
  assert.equal(selectPools('self-hosted', [runner('online', intel)]).macos, intel);
  assert.equal(selectPools('self-hosted', null).macos, arm);
});

test('an unknown mode is refused', () => {
  assert.throws(() => selectPools('fastest', []), /unknown runners mode/);
});
