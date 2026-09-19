// Local debugging-endpoint discovery for one browser host.
// 单个浏览器宿主的本地调试端点发现。
//
// The host enables remote debugging on an ephemeral loopback port and reports the endpoint
// through the runtime-written DevToolsActivePort file inside its own user-data directory.
// No code scans unrelated machine ports.
// 宿主在临时回环端口启用远程调试，并通过其自有用户数据目录中由运行时写出的
// DevToolsActivePort 文件报告端点。任何代码都不扫描机器上的无关端口。

import { readFile } from 'node:fs/promises';
import path from 'node:path';

/** File name written by the Chromium runtime once the debugging listener is bound. */
/** 调试监听器绑定后由 Chromium 运行时写出的文件名。 */
export const DEVTOOLS_ACTIVE_PORT_FILENAME = 'DevToolsActivePort';

/** Default loopback host used by the local debugging listener. */
/** 本地调试监听器使用的默认回环主机。 */
export const BROWSER_HOST_CDP_LOOPBACK_HOST = '127.0.0.1';

/** One resolved local debugging endpoint. */
/** 一个已解析的本地调试端点。 */
export interface ResolvedCdpEndpoint {
  /** Loopback host of the debugging listener. */
  /** 调试监听器的回环主机。 */
  host: string;
  /** Bound ephemeral port of the debugging listener. */
  /** 调试监听器已绑定的临时端口。 */
  port: number;
  /** Browser-level WebSocket path reported by the runtime. */
  /** 运行时报告的浏览器级 WebSocket 路径。 */
  browserWebSocketPath: string;
}

/** Parses one DevToolsActivePort document into a resolved endpoint. */
/** 将一份 DevToolsActivePort 文档解析为已解析端点。 */
export function parseDevToolsActivePort(
  document: string,
  host: string,
): ResolvedCdpEndpoint | null {
  // lines keeps the port line and the browser WebSocket path line in runtime order.
  // lines 按运行时顺序保留端口行与浏览器 WebSocket 路径行。
  const lines = document
    .split('\n')
    .map((line) => line.trim())
    .filter((line) => line.length > 0);
  if (lines.length < 2) {
    return null;
  }
  const portText = lines[0]!;
  const browserWebSocketPath = lines[1]!;
  if (!/^[0-9]+$/.test(portText)) {
    return null;
  }
  const port = Number.parseInt(portText, 10);
  if (!Number.isInteger(port) || port <= 0 || port > 65535) {
    return null;
  }
  if (!browserWebSocketPath.startsWith('/')) {
    return null;
  }
  return { host, port, browserWebSocketPath };
}

/** Reads the runtime debugging endpoint, retrying until the bounded deadline expires. */
/** 读取运行时调试端点，在有限期限内重试直至成功。 */
export async function waitForCdpEndpoint(
  userDataDirectory: string,
  timeoutMs: number,
  host: string = BROWSER_HOST_CDP_LOOPBACK_HOST,
): Promise<ResolvedCdpEndpoint> {
  const activePortPath = path.join(userDataDirectory, DEVTOOLS_ACTIVE_PORT_FILENAME);
  const deadline = Date.now() + timeoutMs;
  let lastError: unknown = null;
  for (;;) {
    try {
      const document = await readFile(activePortPath, 'utf8');
      const endpoint = parseDevToolsActivePort(document, host);
      if (endpoint) {
        return endpoint;
      }
      lastError = new Error(
        `${DEVTOOLS_ACTIVE_PORT_FILENAME} did not contain a usable port and browser path.`,
      );
    } catch (error) {
      lastError = error;
    }
    if (Date.now() >= deadline) {
      throw new Error(
        `Local debugging endpoint was not published within ${timeoutMs}ms: ${String(lastError)}`,
      );
    }
    await new Promise((resolve) => setTimeout(resolve, 50));
  }
}
