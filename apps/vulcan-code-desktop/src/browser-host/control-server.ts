// Loopback control listener owned by one browser host process.
// 单个浏览器宿主进程拥有的回环控制监听器。
//
// Verified platform fact (W02): the Electron main process on Windows does not receive the
// spawner's standard-input pipe, so a bidirectional stdio control channel is not available.
// The host therefore accepts one authenticated loopback control connection instead; loss of
// that connection is the single authoritative "Core exited" signal and no PID is polled.
// 已实测平台事实（W02）：Windows 上 Electron 主进程收不到启动方的标准输入管道，因此无法使用
// 双向标准流控制通道。宿主改为接受一条经过鉴权的回环控制连接；该连接丢失是唯一权威的
// “Core 已退出”信号，且不轮询任何进程标识。

import net from 'node:net';

import { BROWSER_HOST_CDP_LOOPBACK_HOST } from './cdp-discovery.js';

/** Bounded wait for Core to open the control connection. */
/** 等待 Core 打开控制连接的有限时长。 */
export const BROWSER_HOST_CONTROL_ACCEPT_TIMEOUT_MS = 30_000;

/** Live control listener for one browser host. */
/** 单个浏览器宿主的活动控制监听器。 */
export interface BrowserHostControlListener {
  /** Loopback host the listener is bound to. */
  /** 监听器绑定的回环主机。 */
  host: string;
  /** Bound ephemeral loopback port. */
  /** 已绑定的临时回环端口。 */
  port: number;
  /** Resolves with the single accepted control socket. */
  /** 以唯一被接受的控制套接字完成。 */
  waitForClient(timeoutMs: number): Promise<net.Socket>;
  /** Releases the listener without touching the accepted socket. */
  /** 释放监听器，且不影响已接受的套接字。 */
  close(): void;
}

/** Binds one loopback control listener that accepts exactly one connection. */
/** 绑定一个只接受一条连接的回环控制监听器。 */
export async function listenForCoreControl(): Promise<BrowserHostControlListener> {
  let pending: ((socket: net.Socket) => void) | null = null;
  let accepted = false;
  const server = net.createServer((socket) => {
    if (accepted || !pending) {
      // Only the first connection is the Core control channel; extras are refused outright.
      // 只有第一条连接是 Core 控制通道；额外连接一律拒绝。
      socket.destroy();
      return;
    }
    accepted = true;
    socket.setNoDelay(true);
    const resolve = pending;
    pending = null;
    resolve(socket);
  });
  await new Promise<void>((resolve, reject) => {
    server.once('error', reject);
    server.listen(0, BROWSER_HOST_CDP_LOOPBACK_HOST, () => {
      server.off('error', reject);
      resolve();
    });
  });
  const address = server.address();
  if (!address || typeof address === 'string') {
    server.close();
    throw new Error('Browser host control listener did not report a loopback port.');
  }
  return {
    host: BROWSER_HOST_CDP_LOOPBACK_HOST,
    port: address.port,
    waitForClient(timeoutMs: number): Promise<net.Socket> {
      if (accepted) {
        return Promise.reject(new Error('Browser host control connection was already accepted.'));
      }
      return new Promise<net.Socket>((resolve, reject) => {
        const timer = setTimeout(() => {
          pending = null;
          reject(new Error(`Core did not open the browser host control connection within ${timeoutMs}ms.`));
        }, timeoutMs);
        pending = (socket) => {
          clearTimeout(timer);
          resolve(socket);
        };
      });
    },
    close(): void {
      server.close();
    },
  };
}
