// How the desktop client follows a Core restart for an update (docs/contracts/app-updates.md).
// 桌面客户端如何跟随为更新而进行的 Core 重启（docs/contracts/app-updates.md）。

// UpdateRestartReason mirrors the service's `ServiceRestartReason`.
// UpdateRestartReason 镜像服务端的 `ServiceRestartReason`。
export type UpdateRestartReason = 'update' | 'installer' | 'setup';

// isUpdateRestartReason accepts only the reasons the service announces.
// isUpdateRestartReason 只接受服务端会宣布的原因。
export function isUpdateRestartReason(value: unknown): value is UpdateRestartReason {
  return value === 'update' || value === 'installer' || value === 'setup';
}

// UpdateRestartTiming bounds the waits and supplies the clock, so tests drive them.
// UpdateRestartTiming 限定各等待时长并提供时钟，使测试可以驱动它们。
export interface UpdateRestartTiming {
  // exitWaitMs bounds the wait for the announcing Core to exit (the service's `OWNER_EXIT_WAIT`).
  // exitWaitMs 限定等待宣布重启的 Core 退出的时长（服务端 `OWNER_EXIT_WAIT`）。
  exitWaitMs: number;
  // setupWaitMs bounds the wait for the setup program and the service started after it.
  // setupWaitMs 限定等待安装器及其之后启动的服务的时长。
  setupWaitMs: number;
  pollMs: number;
  now: () => number;
  sleep: (ms: number) => Promise<void>;
}

// awaitUpdateRestart waits until the client may end, and returns whether it relaunches from the
// stable entry. Every reason waits for the announcing Core to exit; the setup program then still
// switches versions, so a `setup` restart also waits for the next Core before relaunching, and an
// installer starts the app itself, so the client only quits.
// awaitUpdateRestart 等待到客户端可以结束为止，并返回是否从稳定入口重新启动。每种原因都等待宣布重启的
// Core 退出；之后安装器仍在切换版本，因此 `setup` 重启还要等待下一个 Core 再重新启动；安装包会自行启动
// 应用，因此客户端只退出。
// Parameters: announcing is the pid of the Core that announced the restart; readLivePid returns the
// pid of the live Core, or null.
// 参数：announcing 为宣布重启的 Core 的 pid；readLivePid 返回活动 Core 的 pid，或 null。
export async function awaitUpdateRestart(
  reason: UpdateRestartReason,
  announcing: number | null,
  readLivePid: () => Promise<number | null>,
  timing: UpdateRestartTiming,
): Promise<boolean> {
  const exitDeadline = timing.now() + timing.exitWaitMs;
  while (announcing !== null && timing.now() < exitDeadline) {
    const live = await readLivePid();
    if (live === null || live !== announcing) break;
    await timing.sleep(timing.pollMs);
  }
  if (reason === 'setup') {
    const deadline = timing.now() + timing.setupWaitMs;
    while (timing.now() < deadline) {
      const live = await readLivePid();
      if (live !== null && live !== announcing) break;
      await timing.sleep(timing.pollMs);
    }
  }
  return reason !== 'installer';
}
