import assert from 'node:assert/strict';
import test from 'node:test';
import { awaitUpdateRestart, isUpdateRestartReason, type UpdateRestartTiming } from './update-restart.js';

// fakeClock advances only when the code under test sleeps.
// fakeClock 只在被测代码休眠时前进。
function fakeClock(): UpdateRestartTiming {
  let now = 0;
  return {
    exitWaitMs: 1_000,
    setupWaitMs: 10_000,
    pollMs: 100,
    now: () => now,
    sleep: async (ms) => {
      now += ms;
    },
  };
}

// sequence answers each read with the next pid, repeating the last one.
// sequence 依次以下一个 pid 应答每次读取，之后重复最后一个。
function sequence(pids: Array<number | null>): { read: () => Promise<number | null>; reads: () => number } {
  let reads = 0;
  return {
    read: async () => pids[Math.min(reads++, pids.length - 1)] ?? null,
    reads: () => reads,
  };
}

test('only the reasons the service announces are accepted', () => {
  for (const reason of ['update', 'installer', 'setup']) assert.equal(isUpdateRestartReason(reason), true);
  for (const reason of ['restart', '', null, 3]) assert.equal(isUpdateRestartReason(reason), false);
});

test('an update relaunches once the announcing Core exited', async () => {
  const live = sequence([7, 7, null]);
  assert.equal(await awaitUpdateRestart('update', 7, live.read, fakeClock()), true);
  assert.equal(live.reads(), 3);
});

test('an installer only quits', async () => {
  assert.equal(await awaitUpdateRestart('installer', 7, sequence([null]).read, fakeClock()), false);
});

test('a setup restart waits for the next Core before relaunching', async () => {
  // The old Core exits, nothing runs while the setup program works, then a new Core answers.
  // 旧 Core 退出，安装器工作期间没有 Core 运行，随后新的 Core 应答。
  const live = sequence([7, null, null, null, 9]);
  const clock = fakeClock();
  assert.equal(await awaitUpdateRestart('setup', 7, live.read, clock), true);
  assert.equal(live.reads(), 5);
  assert.ok(clock.now() < clock.setupWaitMs);
});

test('a setup restart relaunches anyway once its wait ran out', async () => {
  const clock = fakeClock();
  assert.equal(await awaitUpdateRestart('setup', 7, sequence([null]).read, clock), true);
  assert.ok(clock.now() >= clock.setupWaitMs);
});
