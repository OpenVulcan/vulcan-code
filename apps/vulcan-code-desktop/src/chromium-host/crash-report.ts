// Persist fatal host evidence without handling or suppressing the original process failure.
// 持久化宿主致命错误证据，不处理或压制原有进程失败。
import { mkdirSync, writeFileSync } from 'node:fs';
import path from 'node:path';

/** Install a fatal-error monitor for the exact Core-owned control directory and generation. */
/** 为 Core 所属的精确控制目录和代次安装致命错误监视器。 */
/** Returns a disposer for tests; production keeps the monitor until the host exits. */
/** 返回供测试使用的清理函数；生产环境保持监视直至宿主退出。 */
export function monitorHostCrash(controlDirectory: string, sessionId: string, hostGeneration: string): () => void {
  // A monitor observes Node's default fatal path; it does not make an unhealthy host continue running.
  // 监视器观察 Node 默认致命退出路径，不会让已损坏的宿主继续运行。
  const monitor = (error: Error, origin: string): void => {
    try {
      // Redact the exact one-use relay capability path before retaining an upstream stack.
      // 保存上游堆栈前脱敏精确的一次性中继能力路径。
      const stack = (error.stack ?? error.message).replace(/\/devtools\/browser\/[A-Za-z0-9_-]+/g, '/devtools/browser/[redacted]').slice(0, 16_384);
      mkdirSync(controlDirectory, { recursive: true });
      writeFileSync(path.join(controlDirectory, 'browser-host-crash.json'), JSON.stringify({
        schemaVersion: 1, sessionId, hostGeneration, pid: process.pid, origin,
        occurredAt: new Date().toISOString(), stack,
      }, null, 2));
    } catch {
      // A failed diagnostic write must not replace the original exception or prevent the default exit.
      // 诊断写入失败不得覆盖原始异常或阻止默认退出。
    }
  };
  process.on('uncaughtExceptionMonitor', monitor);
  return () => { process.off('uncaughtExceptionMonitor', monitor); };
}
