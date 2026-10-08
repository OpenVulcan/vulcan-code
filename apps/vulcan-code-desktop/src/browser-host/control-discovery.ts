// Control-endpoint discovery file owned by one browser host.
// 单个浏览器宿主拥有的控制端点发现文件。
//
// Verified platform fact (W02): the Electron main process on Windows receives neither the
// spawner's standard-input pipe nor a clean standard-output stream, because the runtime writes
// stray bytes to standard output before application code runs. The host therefore publishes its
// loopback control endpoint through an atomic discovery file inside its own user-data directory,
// which is the same explicit mechanism the runtime uses for its own DevToolsActivePort file.
// 已实测平台事实（W02）：Windows 上 Electron 主进程既收不到启动方的标准输入管道，也拿不到
// 干净的标准输出流，因为运行时会先向标准输出写入杂散字节。宿主因此通过其自有用户数据目录内的
// 原子发现文件发布回环控制端点，这与运行时自身发布 DevToolsActivePort 的显式机制一致。

import { rename, writeFile } from 'node:fs/promises';
import { readFile } from 'node:fs/promises';
import path from 'node:path';

// The discovery file name and schema version are generated from the Core definitions.
// 发现文件名与架构版本由 Core 定义生成。
import {
  BROWSER_HOST_CONTROL_DISCOVERY_FILENAME,
  BROWSER_HOST_CONTROL_DISCOVERY_SCHEMA_VERSION,
} from './protocol-constants.generated.js';

export { BROWSER_HOST_CONTROL_DISCOVERY_FILENAME, BROWSER_HOST_CONTROL_DISCOVERY_SCHEMA_VERSION };

/** One published control endpoint document. */
/** 一份已发布的控制端点文档。 */
export interface BrowserHostControlDiscovery {
  /** Schema version of this document. */
  /** 本文档的架构版本。 */
  schemaVersion: number;
  /** Control protocol version understood by the host. */
  /** 宿主理解的控制协议版本。 */
  protocolVersion: number;
  /** Loopback host of the control listener. */
  /** 控制监听器的回环主机。 */
  controlHost: string;
  /** Ephemeral loopback port of the control listener. */
  /** 控制监听器的临时回环端口。 */
  controlPort: number;
  /** Operating-system process id of the host. */
  /** 宿主的操作系统进程标识。 */
  pid: number;
  /** Session identity assigned by Core. */
  /** 由 Core 分配的会话标识。 */
  sessionId: string;
  /** Host generation assigned by Core. */
  /** 由 Core 分配的宿主代际。 */
  hostGeneration: string;
  /** Runtime version of the host build. */
  /** 宿主构建的运行时版本。 */
  runtimeVersion: string;
  /** UTC start time in Unix milliseconds. */
  /** UTC 启动时间，单位 Unix 毫秒。 */
  startedAt: string;
}

/** Returns the control discovery path inside one user-data directory. */
/** 返回一个用户数据目录内的控制发现路径。 */
export function controlDiscoveryPath(userDataDirectory: string): string {
  return path.join(userDataDirectory, BROWSER_HOST_CONTROL_DISCOVERY_FILENAME);
}

/** Writes the control discovery document atomically so Core never reads a partial file. */
/** 原子写入控制发现文档，使 Core 绝不读取半份文件。 */
export async function writeControlDiscovery(
  userDataDirectory: string,
  document: BrowserHostControlDiscovery,
): Promise<void> {
  const target = controlDiscoveryPath(userDataDirectory);
  // The temporary file shares the target directory so the replacement stays atomic.
  // 临时文件与目标位于同一目录，使替换保持原子性。
  const temporary = `${target}.tmp-${process.pid}`;
  await writeFile(temporary, `${JSON.stringify(document, null, 2)}\n`, 'utf8');
  await rename(temporary, target);
}

/** Reads one control discovery document from a user-data directory. */
/** 从一个用户数据目录读取一份控制发现文档。 */
export async function readControlDiscovery(
  userDataDirectory: string,
): Promise<BrowserHostControlDiscovery | null> {
  try {
    const body = await readFile(controlDiscoveryPath(userDataDirectory), 'utf8');
    const document = JSON.parse(body) as BrowserHostControlDiscovery;
    if (
      document.schemaVersion !== BROWSER_HOST_CONTROL_DISCOVERY_SCHEMA_VERSION ||
      typeof document.controlPort !== 'number' ||
      document.controlPort <= 0
    ) {
      return null;
    }
    return document;
  } catch {
    return null;
  }
}
